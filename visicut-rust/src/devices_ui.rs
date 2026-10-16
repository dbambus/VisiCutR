//! Lasercutter management, rotary axis and camera for the egui interface.
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use visicut_core::{
    camera,
    device::{self, CameraCalibration, DeviceStore, LaserDevice},
    project::{
        FIRMWARE_CURVE_ACCELERATION, LIBLASERCUT_CURVE_ACCELERATION, MachineSettings, Project,
    },
    shift_calibration,
};
use visicut_core::{i18n::tr, tr};

/// Tooltip der Kamera-Anmeldung.
fn camera_login_help() -> &'static str {
    tr(
        "Optionale HTTP-Anmeldung für die Kamera-URL; wird nur mit Benutzer und Passwort gesendet. Das Passwort steht in der Geräteliste und wird beim Exportieren weggelassen.",
    )
}
/// Tooltip der Kamera-Aktualisierung.
fn camera_refresh_help() -> &'static str {
    tr(
        "Pause zwischen zwei Kamerabildern, solange das Kamerabild eingeblendet ist (VisiCut „Zeiten (ms) Kamera“), mindestens 50 ms. 0: nur beim Einblenden und mit „Kamerabild aktualisieren“.",
    )
}

type Pending = Receiver<Result<image::RgbaImage, String>>;
type Download = Receiver<Result<Vec<LaserDevice>, String>>;

/// Arbeitsbett des LTT iLaser 4000 in mm. Die Geräteimporte lehnen andere
/// Maße ab, deshalb legt das Gerät die Bettgröße fest (wie macOS).
pub const BED_MM: [f32; 2] = [1000.0, 600.0];

fn spawn(work: impl FnOnce() -> Result<image::RgbaImage, String> + Send + 'static) -> Pending {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(work());
    });
    rx
}

/// Polls background work; `None` while it is still running.
fn poll(
    pending: &mut Option<Pending>,
    ctx: &egui::Context,
) -> Option<Result<image::RgbaImage, String>> {
    let result = match pending.as_ref()?.try_recv() {
        Ok(result) => result,
        Err(TryRecvError::Empty) => {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            return None;
        }
        Err(TryRecvError::Disconnected) => Err(tr("Kamera unerwartet abgebrochen").into()),
    };
    *pending = None;
    Some(result)
}

fn texture(ctx: &egui::Context, name: &str, image: &image::RgbaImage) -> egui::TextureHandle {
    let pixels = egui::ColorImage::from_rgba_unmultiplied(
        [image.width() as usize, image.height() as usize],
        image.as_raw(),
    );
    ctx.load_texture(name, pixels, egui::TextureOptions::LINEAR)
}

struct Calibration {
    device: LaserDevice,
    points: CameraCalibration,
    picture: Option<(egui::TextureHandle, Vec2)>,
    loading: Option<Pending>,
    selected: usize,
    problem: Option<String>,
}

pub enum Action {
    /// Open VisiCut's calibration marks for these bed points as a project.
    CalibrationPage(Vec<[f64; 2]>),
    /// Open the engrave shift test pattern with this power as a project.
    ShiftPattern(f32),
}

/// Zeilenversatz-Kalibrierung eines Geräts: Testmuster und Ergebnis.
struct ShiftTest {
    device: String,
    power: f32,
    /// Bester Block je Geschwindigkeit (1 … 5, von links).
    best: [usize; 10],
}

pub struct DeviceUi {
    pub store: DeviceStore,
    draft: Option<(DeviceStore, usize)>,
    show_camera: bool,
    camera: Option<egui::TextureHandle>,
    camera_loading: Option<Pending>,
    /// Laufende Kamera-Aktualisierung: nächstes Bild und Fehler in Folge.
    camera_next: Option<std::time::Instant>,
    camera_errors: u32,
    /// „Suchen“: laufende VisiCam-Suche und gefundene URLs zur Auswahl.
    camera_search: Option<Receiver<Vec<String>>>,
    camera_found: Vec<String>,
    calibration: Option<Calibration>,
    shift_test: Option<ShiftTest>,
    /// Laufender Download von Labor-Einstellungen (Laborname, Ergebnis).
    download: Option<(String, Download)>,
}

impl DeviceUi {
    pub fn load() -> (Self, Option<String>) {
        let (store, error) = match DeviceStore::load(&device::config_dir()) {
            Ok(store) => (store, None),
            Err(e) => (
                DeviceStore::default(),
                Some(tr!("Geräteliste nicht lesbar: {error}", error = e)),
            ),
        };
        let ui = Self {
            store,
            draft: None,
            show_camera: false,
            camera: None,
            camera_loading: None,
            camera_next: None,
            camera_errors: 0,
            camera_search: None,
            camera_found: Vec::new(),
            calibration: None,
            shift_test: None,
            download: None,
        };
        (ui, error)
    }

    pub fn device(&self) -> &LaserDevice {
        &self.store.devices[self.store.selected]
    }

    /// The selected device defines the job target, the bed and rotary availability.
    pub fn apply(&self, project: &mut Project) -> bool {
        let device = self.device();
        let changed = project.hostname != device.hostname
            || project.port != device.port
            || [project.bed_width_mm, project.bed_height_mm] != BED_MM
            || (project.rotary_axis && !device.rotary_axis)
            || project.machine != device.machine;
        project.hostname = device.hostname.clone();
        project.port = device.port;
        project.machine = device.machine.clone();
        [project.bed_width_mm, project.bed_height_mm] = BED_MM;
        project.rotary_axis &= device.rotary_axis;
        changed
    }

    /// Öffnet die Lasercutter-Verwaltung (oder lässt sie offen).
    pub fn open_manager(&mut self) {
        if self.draft.is_none() {
            self.draft = Some((self.store.clone(), self.store.selected));
        }
    }

    /// Rulers and Y count from the bottom left corner of the bed (display
    /// only, VisiCut `originBottomLeft`).
    pub fn origin_bottom_left(&self) -> bool {
        self.device().origin_bottom_left
    }

    pub fn has_camera(&self) -> bool {
        !self.device().camera_url.is_empty()
    }

    pub fn camera_shown(&self) -> bool {
        self.show_camera
    }

    pub fn camera_loading(&self) -> bool {
        self.camera_loading.is_some()
    }

    /// Kamerabild ein- oder ausblenden; lädt es beim ersten Einblenden.
    pub fn toggle_camera(&mut self, project: &Project) {
        if !self.has_camera() {
            return;
        }
        self.show_camera = !self.show_camera;
        self.camera_errors = 0;
        if self.show_camera && (self.camera.is_none() || self.device().camera_refresh_ms > 0) {
            self.refresh_camera(project);
        }
    }

    fn save(&mut self, store: DeviceStore, project: &mut Project, status: &mut String) -> bool {
        if let Err(e) = store.save(&device::config_dir()) {
            *status = e;
            return false;
        }
        let camera_changed = {
            let (old, new) = (self.device(), &store.devices[store.selected]);
            old.camera_url != new.camera_url
                || old.camera_calibration != new.camera_calibration
                || old.camera_user != new.camera_user
                || old.camera_password != new.camera_password
        };
        self.store = store;
        if camera_changed {
            self.camera = None;
            self.show_camera = false;
        }
        self.apply(project);
        *status = tr!(
            "Lasercutter: {name} ({host}:{port})",
            name = self.device().name,
            host = self.device().hostname,
            port = self.device().port
        );
        true
    }

    pub fn refresh_camera(&mut self, project: &Project) {
        if self.camera_loading.is_some() || !self.has_camera() {
            return;
        }
        let device = self.device().clone();
        let (w, h) = (project.bed_width_mm as f64, project.bed_height_mm as f64);
        self.camera_loading = Some(spawn(move || camera::background_image(&device, w, h)));
    }

    /// Macht einen eingerichteten Lasercutter aktiv (Menü Datei → Lasercutter).
    pub fn select(&mut self, index: usize, project: &mut Project, status: &mut String) -> bool {
        if index == self.store.selected || index >= self.store.devices.len() {
            return false;
        }
        let mut store = self.store.clone();
        store.selected = index;
        self.save(store, project, status)
    }

    /// Rotationseinheit (nur bei Geräten, die eine haben). Das Kamerabild
    /// schaltet die Leiste über dem Arbeitsbett.
    pub fn rotary(&self, ui: &mut egui::Ui, project: &mut Project, dirty: &mut bool) {
        *dirty |= ui
            .checkbox(&mut project.rotary_axis, tr("Rotationseinheit verwenden"))
            .changed();
        if project.rotary_axis {
            *dirty |= crate::number(
                ui,
                tr("Durchmesser"),
                &mut project.rotary_diameter_mm,
                5.0..=1000.0,
                " mm",
            );
            crate::form::caption(
                ui,
                tr!(
                    "Y entspricht dem Umfang ({circumference} mm). Gravur zentriert am Gerät \
                     ausrichten. Schritte je Umdrehung aus LibLaserCut, am Gerät nicht validiert.",
                    circumference =
                        format!("{:.1}", std::f32::consts::PI * project.rotary_diameter_mm)
                ),
            );
        }
    }

    /// Paints the camera picture on the bed; returns whether it covers it.
    pub fn paint_camera(&self, painter: &egui::Painter, bed: Rect) -> bool {
        match (&self.camera, self.show_camera) {
            (Some(texture), true) => {
                painter.image(
                    texture.id(),
                    bed,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
                true
            }
            _ => false,
        }
    }

    pub fn windows(
        &mut self,
        ctx: &egui::Context,
        project: &mut Project,
        status: &mut String,
    ) -> Option<Action> {
        // Laufende Aktualisierung wie VisiCuts RefreshCameraThread: nach jedem
        // Bild die eingestellte Pause, nach Fehlern länger (ab 10 s).
        let refresh = self.device().camera_refresh_ms;
        let pause = std::time::Duration::from_millis(u64::from(refresh));
        match poll(&mut self.camera_loading, ctx) {
            Some(Ok(image)) => {
                self.camera = Some(texture(ctx, "Kamera", &image));
                if refresh == 0 || self.camera_errors > 0 {
                    *status = tr("Kamerabild aktualisiert").into();
                }
                self.camera_errors = 0;
                self.camera_next = Some(std::time::Instant::now() + pause);
            }
            Some(Err(e)) if refresh > 0 && self.show_camera => {
                self.camera_errors += 1;
                let wait = std::time::Duration::from_secs(5 * u64::from(self.camera_errors.max(2)));
                self.camera_next = Some(std::time::Instant::now() + wait + pause);
                *status = tr!("Kamerabild: {error}", error = e);
            }
            Some(Err(e)) => {
                self.show_camera = false;
                *status = tr!("Kamerabild: {error}", error = e);
            }
            None => {}
        }
        if refresh > 0 && self.show_camera && self.camera_loading.is_none() {
            match self.camera_next {
                Some(next) if next > std::time::Instant::now() => {
                    ctx.request_repaint_after(next - std::time::Instant::now());
                }
                _ => self.refresh_camera(project),
            }
        }
        self.poll_download(ctx, project, status);
        let mut action = None;
        if let Some(calibration) = &mut self.calibration {
            let mut close = false;
            let mut apply = None;
            calibration_window(ctx, calibration, &mut close, &mut apply, &mut action);
            if let Some(points) = apply
                && let Some((draft, _)) = &mut self.draft
                && let Some(device) = draft
                    .devices
                    .iter_mut()
                    .find(|d| d.name == calibration.device.name)
            {
                device.camera_calibration = Some(points);
            }
            if close {
                self.calibration = None;
            }
        }
        if action.is_some() {
            self.calibration = None;
        }
        if let Some(test) = &mut self.shift_test {
            let active = self
                .draft
                .as_ref()
                .is_none_or(|(draft, _)| *draft == self.store)
                && self.store.devices[self.store.selected].name == test.device;
            let mut open = true;
            let mut apply = false;
            shift_test_window(ctx, test, active, &mut open, &mut apply, &mut action);
            if apply && self.draft.is_none() {
                self.draft = Some((self.store.clone(), self.store.selected));
            }
            if apply
                && let Some((draft, _)) = &mut self.draft
                && let Some(device) = draft.devices.iter_mut().find(|d| d.name == test.device)
            {
                match shift_calibration::apply(&device.machine, &test.best) {
                    Ok(machine) => {
                        device.machine = machine;
                        test.best = [shift_calibration::CENTER_BLOCK; 10];
                        *status = tr("Zeilenversatz übernommen · zum Übernehmen sichern").into();
                    }
                    Err(e) => *status = e,
                }
            }
            if !open {
                self.shift_test = None;
            }
        }
        action
    }

    fn start_download(&mut self, name: &str, url: &'static str, status: &mut String) {
        if self.download.is_some() {
            return;
        }
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(device::download(url));
        });
        self.download = Some((name.to_owned(), rx));
        *status = tr!(
            "Einstellungen von „{name}“ werden heruntergeladen …",
            name = name
        );
    }

    /// Sichert fertige Downloads und übernimmt sie in den Entwurf der Verwaltung.
    fn poll_download(&mut self, ctx: &egui::Context, project: &mut Project, status: &mut String) {
        let Some((name, receiver)) = &self.download else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
                return;
            }
            Err(TryRecvError::Disconnected) => Err(tr("Download unerwartet abgebrochen").into()),
        };
        let name = name.clone();
        self.download = None;
        match result {
            Ok(devices) => {
                if let Some(added) = self.adopt(devices, project, status) {
                    *status = tr!(
                        "{added} LTT iLaser 4000 von „{name}“ importiert",
                        added = added,
                        name = name
                    );
                }
            }
            Err(e) => {
                *status = tr!(
                    "Download von „{name}“ fehlgeschlagen: {error}",
                    name = name,
                    error = e
                )
            }
        }
    }

    /// Seite „Einstellungen → Lasercutter“: Geräteliste links, das gewählte
    /// Gerät rechts; Änderungen bleiben ein Entwurf bis „Sichern“.
    pub fn settings_page(&mut self, ui: &mut egui::Ui, project: &mut Project, status: &mut String) {
        self.open_manager();
        let Some((mut draft, mut editing)) = self.draft.take() else {
            return;
        };
        let ctx = ui.ctx().clone();
        let mut save = false;
        let mut calibrate = false;
        let mut shift_test = false;
        // Ergebnis von „Suchen“: eine Kamera wird übernommen, mehrere zur Wahl.
        if let Some(receiver) = &self.camera_search {
            match receiver.try_recv() {
                Ok(urls) => {
                    self.camera_search = None;
                    match urls.as_slice() {
                        [] => *status = tr("Keine VisiCam im Netz gefunden").into(),
                        [url] => {
                            draft.devices[editing].camera_url = url.clone();
                            *status = tr!("Kamera gefunden: {url}", url = url);
                        }
                        _ => self.camera_found = urls,
                    }
                }
                Err(TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(100))
                }
                Err(TryRecvError::Disconnected) => self.camera_search = None,
            }
        }
        let searching = self.camera_search.is_some();
        let mut search = false;
        let found = std::mem::take(&mut self.camera_found);
        egui::ScrollArea::vertical()
            .id_salt("device_manager")
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(170.0);
                        for (i, device) in draft.devices.iter().enumerate() {
                            let label = if i == draft.selected {
                                tr!("{name} (aktiv)", name = device.name)
                            } else {
                                device.name.clone()
                            };
                            ui.selectable_value(&mut editing, i, label);
                        }
                        ui.horizontal(|ui| {
                            if ui.button(tr("Kopie")).clicked() {
                                let mut copy = draft.devices[editing].clone();
                                let base = copy.name.clone();
                                let mut n = 2;
                                while draft.devices.iter().any(|d| d.name == copy.name) {
                                    copy.name = format!("{base} ({n})");
                                    n += 1;
                                }
                                draft.devices.push(copy);
                                editing = draft.devices.len() - 1;
                            }
                            if ui
                                .add_enabled(
                                    draft.devices.len() > 1,
                                    egui::Button::new(tr("Entfernen")),
                                )
                                .clicked()
                            {
                                draft.devices.remove(editing);
                                if draft.selected >= editing && draft.selected > 0 {
                                    draft.selected -= 1;
                                }
                                editing = editing.min(draft.devices.len() - 1);
                            }
                        });
                    });
                    ui.separator();
                    ui.vertical(|ui| {
                        let device = &mut draft.devices[editing];
                        egui::Grid::new("device").num_columns(2).show(ui, |ui| {
                            for (label, value) in [
                                (tr("Name"), &mut device.name),
                                (tr("Beschreibung"), &mut device.description),
                                (tr("Hostname / IP"), &mut device.hostname),
                            ] {
                                ui.label(label);
                                ui.text_edit_singleline(value);
                                ui.end_row();
                            }
                            ui.label(tr("Port"));
                            ui.add(egui::DragValue::new(&mut device.port).range(1..=65535));
                            ui.end_row();
                            ui.label(tr("Rotationseinheit"));
                            ui.checkbox(&mut device.rotary_axis, tr("vorhanden"));
                            ui.end_row();
                            ui.label(tr("Koordinaten"));
                            ui.checkbox(
                                &mut device.origin_bottom_left,
                                tr("Nullpunkt unten links"),
                            )
                            .on_hover_text(tr(
                                "Lineale und Y-Koordinate zählen von unten links wie am Gerät; \
                                 ändert den Auftrag nicht",
                            ));
                            ui.end_row();
                        });
                        // Kamera wie im VisiCut-Dialog: URL mit „Suchen“, Anmeldung, Zeiten.
                        ui.add_space(6.0);
                        ui.strong(tr("Kamera"));
                        egui::Grid::new("camera").num_columns(2).show(ui, |ui| {
                            ui.label(tr("Kamera-URL"));
                            ui.horizontal(|ui| {
                                ui.text_edit_singleline(&mut device.camera_url);
                                if searching {
                                    ui.spinner();
                                } else if ui
                                    .button(tr("Suchen"))
                                    .on_hover_text(tr(
                                        "Sucht VisiCam-Kameras im lokalen Netz (wie VisiCut)",
                                    ))
                                    .clicked()
                                {
                                    search = true;
                                }
                            });
                            ui.end_row();
                            if !found.is_empty() {
                                ui.label(tr("Gefunden"));
                                ui.vertical(|ui| {
                                    for url in &found {
                                        if ui.button(url).clicked() {
                                            device.camera_url = url.clone();
                                        }
                                    }
                                });
                                ui.end_row();
                            }
                            ui.label(tr("Kamera-Benutzer"));
                            ui.text_edit_singleline(&mut device.camera_user)
                                .on_hover_text(camera_login_help());
                            ui.end_row();
                            ui.label(tr("Kamera-Passwort"));
                            ui.add(
                                egui::TextEdit::singleline(&mut device.camera_password)
                                    .password(true),
                            )
                            .on_hover_text(camera_login_help());
                            ui.end_row();
                            ui.label(tr("Aktualisierung (ms)"));
                            let response = ui
                                .add(
                                    egui::DragValue::new(&mut device.camera_refresh_ms)
                                        .range(0..=3_600_000),
                                )
                                .on_hover_text(camera_refresh_help());
                            // 1 … 49 ms gibt es nicht; wie in VisiCut gelten mindestens 50 ms.
                            if (response.lost_focus() || response.drag_stopped())
                                && (1..device::MIN_CAMERA_REFRESH_MS)
                                    .contains(&device.camera_refresh_ms)
                            {
                                device.camera_refresh_ms = device::MIN_CAMERA_REFRESH_MS;
                            }
                            ui.end_row();
                            ui.label(tr("Kalibrierung"));
                            ui.horizontal(|ui| {
                                ui.label(
                                    device
                                        .camera_calibration
                                        .as_ref()
                                        .map_or(tr("keine").into(), |c| {
                                            tr!("{count} Punkte", count = c.reference_points.len())
                                        }),
                                );
                                calibrate = ui
                                    .add_enabled(
                                        !device.camera_url.is_empty(),
                                        egui::Button::new(tr("Kalibrieren …")),
                                    )
                                    .clicked();
                            });
                            ui.end_row();
                        });
                        ui.label(tr("Hinweis nach dem Senden ($jobname = Auftragsnamen):"));
                        ui.add(
                            egui::TextEdit::multiline(&mut device.job_sent_text).desired_rows(4),
                        );
                        ui.small(tr(
                            "Autofokus, Druckluft und Absaugung stellt der LTT-Treiber nicht ein.",
                        ));
                        egui::CollapsingHeader::new(tr("Maschine"))
                            .id_salt("machine")
                            .show(ui, |ui| {
                                shift_test |= machine_settings(ui, &mut device.machine);
                            });
                    });
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            editing != draft.selected,
                            egui::Button::new(tr("Aktiv verwenden")),
                        )
                        .clicked()
                    {
                        draft.selected = editing;
                    }
                    let changed = draft != self.store;
                    if ui
                        .add_enabled(changed, egui::Button::new(tr("Verwerfen")))
                        .on_hover_text(tr("Ungesicherte Änderungen an der Geräteliste verwerfen"))
                        .clicked()
                    {
                        draft = self.store.clone();
                        editing = editing.min(draft.devices.len() - 1);
                    }
                    save = ui
                        .add_enabled(changed, egui::Button::new(tr("Sichern")))
                        .clicked();
                });
            });
        if calibrate {
            let device = draft.devices[editing].clone();
            let points = device
                .camera_calibration
                .clone()
                .unwrap_or(CameraCalibration {
                    reference_points: vec![
                        [200.0, 120.0],
                        [800.0, 120.0],
                        [800.0, 480.0],
                        [200.0, 480.0],
                    ],
                    view_points: Vec::new(),
                });
            let shot = device.clone();
            self.calibration = Some(Calibration {
                device,
                points,
                picture: None,
                loading: Some(spawn(move || camera::capture(&shot))),
                selected: 0,
                problem: None,
            });
        }
        if shift_test {
            self.shift_test = Some(ShiftTest {
                device: draft.devices[editing].name.clone(),
                power: 30.0,
                best: [shift_calibration::CENTER_BLOCK; 10],
            });
        }
        // Die Auswahl bleibt, bis eine neue Suche startet oder das Fenster schließt.
        if search {
            let (tx, rx) = channel();
            std::thread::spawn(move || {
                let _ = tx.send(camera::discover());
            });
            self.camera_search = Some(rx);
            *status = tr("Suche VisiCam-Kameras im lokalen Netz …").into();
        } else {
            self.camera_found = found;
        }
        if save {
            self.save(draft.clone(), project, status);
        }
        self.draft = Some((draft, editing));
    }

    /// Verwirft den Entwurf der Verwaltung, wenn er nichts Ungesichertes
    /// enthält; so zeigt die Seite beim nächsten Öffnen die aktuelle Liste.
    pub fn close_manager_if_clean(&mut self) {
        if self
            .draft
            .as_ref()
            .is_some_and(|(draft, _)| *draft == self.store)
        {
            self.draft = None;
        }
    }

    /// Sichert hinzugekommene Geräte sofort und hängt sie auch an einen
    /// offenen Entwurf an; `None`, wenn das Sichern fehlschlug.
    fn adopt(
        &mut self,
        devices: Vec<LaserDevice>,
        project: &mut Project,
        status: &mut String,
    ) -> Option<usize> {
        let mut store = self.store.clone();
        let added = store.merge(devices.clone());
        if !self.save(store, project, status) {
            return None;
        }
        if let Some((draft, editing)) = &mut self.draft {
            draft.merge(devices);
            *editing = draft.devices.len() - 1;
        }
        Some(added)
    }

    /// Importieren, Herunterladen und Exportieren der Geräteliste
    /// (Einstellungen → Allgemein). Importe werden sofort gesichert.
    pub fn transfer(&mut self, ui: &mut egui::Ui, project: &mut Project, status: &mut String) {
        let downloading = self.download.as_ref().map(|(name, _)| name.clone());
        let mut download = None;
        ui.horizontal(|ui| {
            if ui.button(tr("Importieren …")).clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter(
                        tr("VisiCut-Einstellungen"),
                        &["xml", "vcsettings", "zip", "vcrdevices", "json"],
                    )
                    .pick_file()
            {
                match std::fs::read(&path)
                    .map_err(|e| e.to_string())
                    .and_then(|b| device::import_bytes(&b))
                {
                    Ok(devices) => {
                        if let Some(count) = self.adopt(devices, project, status) {
                            *status = tr!("{count} LTT iLaser 4000 importiert", count = count);
                        }
                    }
                    Err(e) => *status = e,
                }
            }
            ui.add_enabled_ui(downloading.is_none(), |ui| {
                crate::form::Dropdown::from_id_salt("labs")
                    .selected_text(tr("Herunterladen"))
                    .show_ui(ui, |ui| {
                        for (name, url) in device::LAB_SETTINGS {
                            if ui.selectable_label(false, *name).clicked() {
                                download = Some((*name, *url));
                            }
                        }
                    });
            });
            if let Some(name) = &downloading {
                ui.spinner()
                    .on_hover_text(tr!("Lädt „{name}“ …", name = name));
            }
            // Exportiert die gesicherte Liste, nicht den Entwurf.
            if ui.button(tr("Exportieren …")).clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter(tr("VisiCutRust-Lasercutter"), &["vcrdevices"])
                    .set_file_name("Lasercutter.vcrdevices")
                    .save_file()
            {
                let devices = &self.store.devices;
                *status = match device::export(&path, devices) {
                    Ok(omitted) => {
                        let mut text = tr!("{count} Lasercutter exportiert", count = devices.len());
                        if omitted {
                            text += " · ";
                            text += tr("Kamera-Passwörter wurden nicht exportiert");
                        }
                        text
                    }
                    Err(e) => e,
                };
            }
        });
        if let Some((name, url)) = download {
            self.start_download(name, url, status);
        }
    }
}

/// Zeilenversatz, Kurvenbeschleunigung und Verweilzeit eines Geräts;
/// `true`, wenn die Kalibrierung geöffnet werden soll.
fn machine_settings(ui: &mut egui::Ui, machine: &mut MachineSettings) -> bool {
    ui.label(tr(
        "Zeilenversatz beim Gravieren (1/4000 Zoll, 8 = ein Pixel)",
    ));
    egui::Grid::new("engrave_shift").show(ui, |ui| {
        for (row, values) in machine.engrave_shift.chunks_mut(5).enumerate() {
            for (i, value) in values.iter_mut().enumerate() {
                ui.label(format!("{} %", (row * 5 + i + 1) * 10));
                ui.add(egui::DragValue::new(value).range(-400.0..=400.0).speed(1.0));
            }
            ui.end_row();
        }
    });
    let calibrate = ui.button(tr("Zeilenversatz kalibrieren …")).clicked();
    ui.horizontal(|ui| {
        ui.label(tr("Kurvenbeschleunigung"));
        ui.add(
            egui::DragValue::new(&mut machine.curve_acceleration_mm_s2)
                .range(500.0..=5000.0)
                .suffix(" mm/s²"),
        );
        if ui.button("LibLaserCut").clicked() {
            machine.curve_acceleration_mm_s2 = LIBLASERCUT_CURVE_ACCELERATION;
        }
        if ui
            .button(tr("Firmware 0,1 g"))
            .on_hover_text(tr(
                "Rampe des Controllers bei verbundenen Kurven (experimentell)",
            ))
            .clicked()
        {
            machine.curve_acceleration_mm_s2 = FIRMWARE_CURVE_ACCELERATION;
        }
    });
    if machine.experimental_curves() {
        ui.small(tr(
            "Experimentell: Kurven werden anders geplant als in VisiCut.",
        ));
    }
    ui.horizontal(|ui| {
        ui.label(tr("Verweilzeit am Konturstart"));
        ui.add(
            egui::DragValue::new(&mut machine.pierce_delay_ms)
                .range(0.0..=1000.0)
                .suffix(" ms"),
        );
    });
    ui.small(tr(
        "pddelay der Gerätekonfiguration; nur für die Zeitschätzung.",
    ));
    if ui.button(tr("Standardwerte")).clicked() {
        *machine = MachineSettings::default();
    }
    calibrate
}

fn shift_test_window(
    ctx: &egui::Context,
    test: &mut ShiftTest,
    active: bool,
    open: &mut bool,
    apply: &mut bool,
    action: &mut Option<Action>,
) {
    egui::Window::new(tr!(
        "Zeilenversatz kalibrieren · {device}",
        device = test.device
    ))
    .id(egui::Id::new(("shift_test", &test.device)))
    .open(open)
    .default_width(420.0)
    .show(ctx, |ui| {
        ui.label(tr(
            "1. Testmuster öffnen, senden und gravieren. Je Geschwindigkeit (10 % oben bis \
                 100 % unten, Balken links = Tempo) entstehen fünf Blöcke mit −2 … +2 Pixeln \
                 Versatz; Block 3 unter dem Dreieck nutzt die aktuelle Tabelle.",
        ));
        ui.horizontal(|ui| {
            ui.label(tr("Leistung"));
            ui.add(
                egui::DragValue::new(&mut test.power)
                    .range(1.0..=100.0)
                    .suffix(" %"),
            );
            if ui
                .add_enabled(active, egui::Button::new(tr("Testmuster öffnen")))
                .on_disabled_hover_text(tr(
                    "Nur für den aktiven Lasercutter ohne ungesicherte Änderungen",
                ))
                .clicked()
            {
                *action = Some(Action::ShiftPattern(test.power));
            }
        });
        ui.separator();
        ui.label(tr(
            "2. Je Zeile den Block mit den geradesten Balkenkanten wählen. Liegt er am \
                 Rand, übernehmen und das Testmuster erneut gravieren.",
        ));
        egui::Grid::new("shift_best").show(ui, |ui| {
            for (speed, best) in shift_calibration::SPEEDS.iter().zip(&mut test.best) {
                ui.label(format!("{speed} %"));
                for block in 1..=shift_calibration::OFFSETS.len() {
                    ui.radio_value(best, block, block.to_string());
                }
                ui.end_row();
            }
        });
        *apply = ui.button(tr("In die Tabelle übernehmen")).clicked();
    });
}

fn calibration_window(
    ctx: &egui::Context,
    c: &mut Calibration,
    close: &mut bool,
    apply: &mut Option<CameraCalibration>,
    action: &mut Option<Action>,
) {
    match poll(&mut c.loading, ctx) {
        Some(Ok(image)) => {
            let size = Vec2::new(image.width() as f32, image.height() as f32);
            c.picture = Some((texture(ctx, "Kalibrierfoto", &image), size));
            // Place missing markers where an uncalibrated camera would see them.
            while c.points.view_points.len() < c.points.reference_points.len() {
                let r = c.points.reference_points[c.points.view_points.len()];
                c.points
                    .view_points
                    .push([r[0] / 1000.0 * size.x as f64, r[1] / 600.0 * size.y as f64]);
            }
        }
        Some(Err(e)) => c.problem = Some(e),
        None => {}
    }
    let mut open = true;
    egui::Window::new(tr!("Kamera kalibrieren · {device}", device = c.device.name))
        .id(egui::Id::new(("camera_calibration", &c.device.name)))
        .open(&mut open)
        .default_size([900.0, 560.0])
        .show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                let space = Vec2::new((ui.available_width() - 220.0).max(200.0), 460.0);
                let (rect, response) = ui.allocate_exact_size(space, Sense::click_and_drag());
                let painter = ui.painter().with_clip_rect(rect);
                painter.rect_filled(rect, 0.0, Color32::from_gray(235));
                if let Some((texture, size)) = &c.picture {
                    let scale = (rect.width() / size.x).min(rect.height() / size.y);
                    let image = Rect::from_center_size(rect.center(), *size * scale);
                    painter.image(texture.id(), image, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                    let to_screen = |p: [f64; 2]| image.min + Vec2::new(p[0] as f32, p[1] as f32) * scale;
                    if let Some(pointer) = response.interact_pointer_pos() {
                        if response.drag_started() || response.clicked() {
                            // Grab the nearest marker, otherwise move the selected one.
                            if let Some((i, _)) = c.points.view_points.iter().enumerate()
                                .map(|(i, p)| (i, to_screen(*p).distance(pointer)))
                                .filter(|(_, d)| *d < 16.0)
                                .min_by(|a, b| a.1.total_cmp(&b.1))
                            {
                                c.selected = i;
                            }
                        }
                        if let Some(p) = c.points.view_points.get_mut(c.selected) {
                            let local = (pointer - image.min) / scale;
                            *p = [local.x.clamp(0.0, size.x) as f64, local.y.clamp(0.0, size.y) as f64];
                        }
                    }
                    for (i, p) in c.points.view_points.iter().enumerate() {
                        let center = to_screen(*p);
                        let color = if i == c.selected { Color32::from_rgb(40, 120, 230) } else { Color32::YELLOW };
                        painter.circle_stroke(center, 12.0, Stroke::new(2.0, color));
                        painter.line_segment([center - Vec2::X * 12.0, center + Vec2::X * 12.0], Stroke::new(1.0, color));
                        painter.line_segment([center - Vec2::Y * 12.0, center + Vec2::Y * 12.0], Stroke::new(1.0, color));
                        painter.text(center + Vec2::new(16.0, -16.0), egui::Align2::LEFT_BOTTOM, (i + 1).to_string(),
                            egui::FontId::proportional(14.0), color);
                    }
                } else {
                    painter.text(rect.center(), egui::Align2::CENTER_CENTER,
                        if c.loading.is_some() { tr("Foto wird aufgenommen …") } else { tr("Kein Kamerabild") },
                        egui::FontId::proportional(16.0), Color32::from_gray(120));
                }
                ui.vertical(|ui| {
                    ui.label(tr("Marker auf dem Bett (mm)"));
                    for (i, r) in c.points.reference_points.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut c.selected, i, (i + 1).to_string());
                            ui.add(egui::DragValue::new(&mut r[0]).range(0.0..=1000.0));
                            ui.add(egui::DragValue::new(&mut r[1]).range(0.0..=600.0));
                        });
                    }
                    ui.horizontal(|ui| {
                        if ui.add_enabled(c.picture.is_some(), egui::Button::new(tr("Punkt +"))).clicked() {
                            let size = c.picture.as_ref().unwrap().1;
                            c.points.reference_points.push([500.0, 300.0]);
                            c.points.view_points.push([size.x as f64 / 2.0, size.y as f64 / 2.0]);
                            c.selected = c.points.reference_points.len() - 1;
                        }
                        if ui.add_enabled(c.points.reference_points.len() > 2, egui::Button::new(tr("Punkt −"))).clicked() {
                            c.points.reference_points.remove(c.selected);
                            if c.selected < c.points.view_points.len() {
                                c.points.view_points.remove(c.selected);
                            }
                            c.selected = c.selected.saturating_sub(1);
                        }
                    });
                    ui.small(tr("1. Kalibrierseite öffnen und markieren.\n2. Foto aufnehmen.\n3. Marker auf die Kreuze ziehen (Nummer = Anzahl Striche).\nMindestens 4 Punkte."));
                    if let Some(problem) = &c.problem {
                        ui.colored_label(Color32::from_rgb(180, 75, 40), problem);
                    }
                });
            });
            ui.horizontal(|ui| {
                if ui.add_enabled(c.loading.is_none(), egui::Button::new(tr("Foto aufnehmen"))).clicked() {
                    let device = c.device.clone();
                    c.problem = None;
                    c.loading = Some(spawn(move || camera::capture(&device)));
                }
                if ui.button(tr("Kalibrierseite als Projekt öffnen")).clicked() {
                    *action = Some(Action::CalibrationPage(c.points.reference_points.clone()));
                }
                if ui.add_enabled(c.points.view_points.len() == c.points.reference_points.len(), egui::Button::new(tr("Übernehmen"))).clicked() {
                    match camera::homography(&c.points) {
                        Ok(_) => {
                            *apply = Some(c.points.clone());
                            *close = true;
                        }
                        Err(e) => c.problem = Some(e),
                    }
                }
            });
        });
    *close |= !open;
}
