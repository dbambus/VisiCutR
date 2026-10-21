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
    /// Passmarken im Kalibrierfoto (Pixel); beim Übernehmen in mm gesichert.
    markers: Vec<[f64; 2]>,
    loading: Option<Pending>,
    selected: usize,
    problem: Option<String>,
    /// Wo die gespeicherte Kalibrierung die Bettpunkte im Foto erwartet.
    expected: Option<camera::Mapping>,
    /// Objektiv, mit dem die Kalibrierung die Verzeichnung bestimmt.
    lens: LensChoice,
    /// Zuletzt gewählte Anordnung; „Zurücksetzen“ stellt sie wieder her.
    grid: Grid,
    /// Restfehler und Linse der aktuellen Punkte (Text für den Dialog).
    quality: Option<String>,
    /// Grauwerte des Fotos für die Kreuzerkennung.
    gray: Option<camera::Gray>,
    /// Ergebnis der letzten Kreuzerkennung: erkannt, gesucht.
    crosses: Option<(usize, usize)>,
    /// Ausgeblendet, während die Kalibrierseite gelasert wird.
    parked: bool,
    /// Ansicht: Vergrößerung (1 = ganzes Foto) und Bildpunkt in der Mitte.
    zoom: f32,
    center: Option<Vec2>,
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
    /// Das zuletzt geladene entzerrte Kamerabild (Reststück-Erkennung).
    camera_image: Option<std::sync::Arc<image::RgbaImage>>,
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
            camera_image: None,
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

    /// Zuletzt geladenes Kamerabild, auf das Bett entzerrt.
    pub fn camera_image(&self) -> Option<std::sync::Arc<image::RgbaImage>> {
        self.camera_image.clone()
    }

    /// Blendet das Kamerabild ein (lädt es, falls nötig).
    pub fn show_camera(&mut self, project: &Project) {
        if !self.show_camera {
            self.toggle_camera(project);
        }
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
            self.camera_image = None;
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
                self.camera_image = Some(std::sync::Arc::new(image));
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
        if let Some(calibration) = self.calibration.as_mut().filter(|c| !c.parked) {
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
        // Die Kalibrierseite wird erst gelasert: der Dialog wartet mit seinen
        // Punkten, bis „Kalibrieren …“ ihn wieder öffnet.
        match &action {
            Some(Action::CalibrationPage(_)) => {
                if let Some(c) = &mut self.calibration {
                    c.parked = true;
                }
            }
            Some(_) => self.calibration = None,
            None => {}
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
    /// Gerät rechts; Änderungen werden gesichert, sobald kein Feld mehr
    /// bearbeitet wird (Eingabe beendet, kein Ziehen).
    pub fn settings_page(&mut self, ui: &mut egui::Ui, project: &mut Project, status: &mut String) {
        self.open_manager();
        let Some((mut draft, mut editing)) = self.draft.take() else {
            return;
        };
        let ctx = ui.ctx().clone();
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
        // Die Geräteliste füllt die Seite, ihre Symbolleiste sitzt unten wie
        // bei den Materialien (Platz für die Knopfzeile bleibt frei).
        let spacing = ui.spacing().item_spacing.y;
        let row = ui.spacing().interact_size.y;
        let columns = (ui.available_height() - row - 6.0 * spacing - 8.0).max(240.0);
        egui::ScrollArea::vertical()
            .id_salt("device_manager")
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(170.0);
                        egui::ScrollArea::vertical()
                            .id_salt("device_list")
                            .auto_shrink([false, false])
                            .max_height(columns - 28.0 - spacing)
                            .show(ui, |ui| {
                                for (i, device) in draft.devices.iter().enumerate() {
                                    let label = if i == draft.selected {
                                        tr!("{name} (aktiv)", name = device.name)
                                    } else {
                                        device.name.clone()
                                    };
                                    ui.selectable_value(&mut editing, i, label);
                                }
                            });
                        use crate::object_bar::{Icon, list_button, list_footer};
                        let removable = draft.devices.len() > 1;
                        list_footer(ui, 170.0, |ui| {
                            if list_button(ui, Icon::Duplicate, true, tr("Kopie des Lasercutters anlegen")) {
                                editing = draft.duplicate(editing);
                            }
                            if list_button(ui, Icon::Minus, removable, tr("Lasercutter entfernen"))
                                && let Some(next) = draft.remove(editing)
                            {
                                editing = next;
                            }
                        });
                    });
                    // Trennstrich so hoch wie die Liste; `separator()` nähme die ganze
                    // Höhe und schöbe die Knopfzeile aus dem Bild.
                    let (line, _) =
                        ui.allocate_exact_size(egui::vec2(12.0, columns), Sense::hover());
                    ui.painter().vline(
                        line.center().x,
                        line.y_range(),
                        ui.visuals().widgets.noninteractive.bg_stroke,
                    );
                    ui.vertical(|ui| {
                        let device = &mut draft.devices[editing];
                        // Ein Raster für alle Abschnitte: die Felder fluchten in
                        // einer Spalte, auch bei aufgeklappter „Maschine“.
                        egui::Grid::new("device")
                            .num_columns(2)
                            .min_col_width(machine_label_width(ui))
                            .show(ui, |ui| {
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
                            top_label(ui, tr("Hinweis nach dem Senden"));
                            ui.vertical(|ui| {
                                let width = ui.spacing().text_edit_width;
                                ui.set_max_width(width);
                                ui.add(
                                    egui::TextEdit::multiline(&mut device.job_sent_text)
                                        .desired_width(width)
                                        .desired_rows(4),
                                );
                                ui.small(tr(
                                    "Autofokus, Druckluft und Absaugung stellt der LTT-Treiber \
                                     nicht ein; dieser Text erinnert nach dem Senden 10 s lang daran. \
                                     $jobname wird durch die Auftragsnamen ersetzt.",
                                ));
                            });
                            ui.end_row();
                            let mut machine = egui::collapsing_header::CollapsingState::load_with_default_open(
                                ui.ctx(),
                                egui::Id::new("device_machine"),
                                false,
                            );
                            let mut toggle = false;
                            section_title(ui, |ui| {
                                let size = egui::Vec2::splat(ui.spacing().icon_width);
                                let (_, icon) = ui.allocate_exact_size(size, Sense::click());
                                // Kleines Dreieck wie im CollapsingHeader.
                                let (small, _) = ui.spacing().icon_rectangles(icon.rect);
                                egui::collapsing_header::paint_default_icon(
                                    ui,
                                    machine.openness(ui.ctx()),
                                    &icon.clone().with_new_rect(small),
                                );
                                let title = egui::Label::new(
                                    egui::RichText::new(tr("Maschine")).strong(),
                                )
                                .sense(Sense::click());
                                toggle = icon.clicked() | ui.add(title).clicked();
                            });
                            if toggle {
                                machine.toggle(ui);
                            }
                            machine.store(ui.ctx());
                            if machine.is_open() {
                                shift_test |= machine_rows(ui, &mut device.machine);
                            }
                        });
                        // Kamera als eigener Bereich, abgesetzt von den Lasereinstellungen.
                        ui.add_space(14.0);
                        ui.separator();
                        ui.add_space(4.0);
                        ui.heading(format!("📷 {}", tr("Kamera")));
                        ui.add_space(4.0);
                        // Wie im VisiCut-Dialog: URL mit „Suchen“, Anmeldung, Zeiten.
                        egui::Grid::new("device_camera")
                            .num_columns(2)
                            .min_col_width(machine_label_width(ui))
                            .show(ui, |ui| {
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
                });
            });
        let parked = self
            .calibration
            .as_mut()
            .filter(|c| c.parked && c.device.name == draft.devices[editing].name);
        if calibrate && let Some(c) = parked {
            // Zurück nach dem Lasern: gleich ein neues Foto mit den Kreuzen.
            let shot = c.device.clone();
            c.parked = false;
            c.problem = None;
            c.loading = Some(spawn(move || camera::capture(&shot)));
        } else if calibrate {
            let device = draft.devices[editing].clone();
            let points = device
                .camera_calibration
                .clone()
                .unwrap_or(CameraCalibration {
                    reference_points: calibration_grid(),
                    ..Default::default()
                });
            let shot = device.clone();
            // Ohne gespeicherte Linse das Profil der FAU-Kamera vorschlagen.
            let lens = match points.lens.as_ref().map(|l| l.profile.as_str()) {
                Some("generic") => LensChoice::Generic,
                _ => LensChoice::RaspberryPiV2,
            };
            let grid = Grid::of(&points.reference_points);
            self.calibration = Some(Calibration {
                lens,
                grid,
                quality: None,
                expected: None,
                device,
                points,
                picture: None,
                markers: Vec::new(),
                loading: Some(spawn(move || camera::capture(&shot))),
                selected: 0,
                problem: None,
                gray: None,
                crosses: None,
                zoom: 1.0,
                center: None,
                parked: false,
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
        // Sofort sichern statt Entwurf: sonst ging z. B. eine neue Kamera-URL
        // beim Verlassen der Seite verloren und „Kamera“ lud weiter die alte.
        let editing_field = ctx.memory(|m| m.focused().is_some()) || ctx.dragged_id().is_some();
        if draft != self.store && !editing_field {
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
                    .add_filter(tr("VisiCutR-Lasercutter"), &["vcrdevices"])
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

/// Beschriftungen der Zeilen unter „Maschine“.
fn machine_labels() -> [&'static str; 3] {
    [
        tr("Zeilenversatz"),
        tr("Kurvenbeschleunigung"),
        tr("Verweilzeit am Konturstart"),
    ]
}

/// Mindestbreite der Beschriftungsspalte im Geräteraster: so breit wie die
/// Beschriftungen unter „Maschine“, damit die Felder beim Aufklappen nicht
/// nach rechts springen.
fn machine_label_width(ui: &egui::Ui) -> f32 {
    let font = egui::TextStyle::Body.resolve(ui.style());
    machine_labels()
        .into_iter()
        .map(|label| {
            ui.painter()
                .layout_no_wrap(label.to_owned(), font.clone(), Color32::PLACEHOLDER)
                .size()
                .x
        })
        .fold(0.0, f32::max)
}

/// Beschriftung oben in ihrer Rasterzelle, auf Höhe der ersten Zeile eines
/// mehrzeiligen Felds (sonst stünde sie in der Mitte).
fn top_label(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.vertical(|ui| ui.horizontal(|ui| ui.label(text)).inner)
        .inner
}

/// Abschnittstitel in der Beschriftungsspalte mit etwas Abstand nach oben.
fn section_title(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    ui.vertical(|ui| {
        ui.add_space(8.0);
        ui.horizontal(add);
    });
    ui.end_row();
}

/// Zeilen „Maschine“ im Geräteraster: Zeilenversatz, Kurvenbeschleunigung
/// und Verweilzeit; `true`, wenn die Kalibrierung geöffnet werden soll.
fn machine_rows(ui: &mut egui::Ui, machine: &mut MachineSettings) -> bool {
    let [shift, acceleration, pierce_delay] = machine_labels();
    top_label(ui, shift);
    let calibrate = ui
        .vertical(|ui| {
            egui::Grid::new("engrave_shift").show(ui, |ui| {
                for (row, values) in machine.engrave_shift.chunks_mut(5).enumerate() {
                    for (i, value) in values.iter_mut().enumerate() {
                        ui.label(format!("{} %", (row * 5 + i + 1) * 10));
                        ui.add(egui::DragValue::new(value).range(-400.0..=400.0).speed(1.0));
                    }
                    ui.end_row();
                }
            });
            ui.small(tr("Beim Gravieren, in 1/4000 Zoll (8 = ein Pixel)"));
            ui.button(tr("Zeilenversatz kalibrieren …")).clicked()
        })
        .inner;
    ui.end_row();
    top_label(ui, acceleration);
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
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
    });
    ui.end_row();
    top_label(ui, pierce_delay);
    ui.vertical(|ui| {
        ui.add(
            egui::DragValue::new(&mut machine.pierce_delay_ms)
                .range(0.0..=1000.0)
                .suffix(" ms"),
        );
        ui.small(tr(
            "pddelay der Gerätekonfiguration; nur für die Zeitschätzung.",
        ));
    });
    ui.end_row();
    ui.label("");
    if ui.button(tr("Standardwerte")).clicked() {
        *machine = MachineSettings::default();
    }
    ui.end_row();
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

#[derive(Clone, Copy, PartialEq)]
enum LensChoice {
    RaspberryPiV2,
    Generic,
    Off,
}

impl LensChoice {
    fn label(self) -> &'static str {
        match self {
            Self::RaspberryPiV2 => tr("Raspberry Pi Camera v2"),
            Self::Generic => tr("Andere Kamera (aus den Kreuzen)"),
            Self::Off => tr("Keine Linsenkorrektur"),
        }
    }

    fn start(self, size: Vec2) -> Option<camera::Lens> {
        let size = [size.x as f64, size.y as f64];
        match self {
            Self::RaspberryPiV2 => Some(camera::Lens::raspberry_pi_v2(size)),
            Self::Generic => Some(camera::Lens::generic(size)),
            Self::Off => None,
        }
    }
}

/// Anordnung der Kalibrierkreuze.
#[derive(Clone, Copy, PartialEq)]
enum Grid {
    /// 5 × 3 Kreuze bis 50 mm an den Rand des Betts (auch für die Linse).
    Fine,
    /// 3 × 2 Kreuze wie bisher (200/500/800 × 120/480 mm).
    Coarse,
}

impl Grid {
    /// Anordnung zu gespeicherten Punkten: die passende, sonst nach Anzahl.
    fn of(points: &[[f64; 2]]) -> Self {
        [Self::Fine, Self::Coarse]
            .into_iter()
            .find(|g| g.points() == points)
            .unwrap_or(if points.len() <= 6 {
                Self::Coarse
            } else {
                Self::Fine
            })
    }

    fn label(self) -> &'static str {
        match self {
            Self::Fine => tr("5 × 3 Kreuze"),
            Self::Coarse => tr("3 × 2 Kreuze"),
        }
    }

    fn points(self) -> Vec<[f64; 2]> {
        let (xs, ys): (&[f64], &[f64]) = match self {
            Self::Fine => (&[50.0, 275.0, 500.0, 725.0, 950.0], &[50.0, 300.0, 550.0]),
            Self::Coarse => (&[200.0, 500.0, 800.0], &[120.0, 480.0]),
        };
        ys.iter()
            .flat_map(|y| xs.iter().map(move |x| [*x, *y]))
            .collect()
    }
}

fn calibration_grid() -> Vec<[f64; 2]> {
    Grid::Fine.points()
}

/// Kalibrierung aus den aktuellen Punkten mit angepasster Linse.
fn fitted(c: &Calibration) -> Option<CameraCalibration> {
    let size = c.picture.as_ref()?.1;
    let mut points = c.points.clone();
    points.marker_points.clear();
    points.lens = None;
    camera::homography(&points).ok()?;
    points.lens = c
        .lens
        .start(size)
        .map(|start| camera::fit_lens(&points, start));
    Some(points)
}

/// Ersetzt die Punkte durch das 5 × 3-Raster; mit Foto dort, wo die bisherigen
/// Punkte die Kreuze erwarten lassen.
fn set_grid(c: &mut Calibration, grid: Grid) {
    let grid = grid.points();
    let size = c.picture.as_ref().map(|p| p.1);
    // Vorhersage aus der gespeicherten Kalibrierung (an den Passmarken
    // nachgeführt), sonst aus den aktuellen Punkten.
    let expected = c
        .expected
        .clone()
        .or_else(|| fitted(c).and_then(|points| camera::mapping(&points).ok()))
        .map(|mapping| {
            grid.iter()
                .map(|r| mapping.to_pixel(*r))
                .collect::<Vec<_>>()
        });
    c.points.view_points = match (expected, size) {
        (Some(expected), _) => expected,
        (None, Some(size)) => grid
            .iter()
            .map(|r| [r[0] / 1000.0 * size.x as f64, r[1] / 600.0 * size.y as f64])
            .collect(),
        (None, None) => Vec::new(),
    };
    c.points.reference_points = grid;
    c.selected = 0;
    c.crosses = None;
    c.quality = None;
    if c.expected.is_some() {
        detect_crosses(c);
    }
}

/// Restfehler der Punkte als Text, in Pixeln und ungefähr in mm.
fn update_quality(c: &mut Calibration) {
    c.quality = fitted(c).and_then(|points| {
        let residuals = camera::residuals(&points).ok()?;
        let mapping = camera::mapping(&points).ok()?;
        let (a, b) = (
            mapping.to_pixel([0.0, 0.0]),
            mapping.to_pixel([1000.0, 600.0]),
        );
        let per_mm = ((b[0] - a[0]).abs() / 1000.0).max((b[1] - a[1]).abs() / 600.0);
        let worst = residuals.iter().cloned().fold(0.0, f64::max);
        let rms = (residuals.iter().map(|r| r * r).sum::<f64>() / residuals.len() as f64).sqrt();
        let mut text = tr!(
            "Restfehler: im Mittel {rms} px, höchstens {max} px (≈ {mm} mm)",
            rms = format!("{rms:.1}"),
            max = format!("{worst:.1}"),
            mm = format!("{:.1}", worst / per_mm)
        );
        if let Some(lens) = &points.lens {
            text += &format!(
                "\n{} k1 = {:.3}, k2 = {:.3}",
                tr("Linse:"),
                lens.k1,
                lens.k2
            );
        }
        Some(text)
    });
}

/// Setzt jeden Punkt auf das gelaserte Kreuz in seiner Nähe (Suchradius 40 mm).
fn detect_crosses(c: &mut Calibration) {
    let Some(gray) = &c.gray else {
        return;
    };
    let n = c
        .points
        .reference_points
        .len()
        .min(c.points.view_points.len());
    // Pixel je mm an jedem Punkt aus den aktuellen Punkten, sonst geschätzt.
    let h = camera::homography(&CameraCalibration {
        reference_points: c.points.reference_points[..n].to_vec(),
        view_points: c.points.view_points[..n].to_vec(),
        marker_points: Vec::new(),
        lens: None,
    })
    .ok();
    let mut found = 0;
    for i in 0..n {
        let (r, v) = (c.points.reference_points[i], c.points.view_points[i]);
        let per_mm = h
            .map(|h| {
                let (a, b, d) = (
                    camera::transform(&h, r),
                    camera::transform(&h, [r[0] + 10.0, r[1]]),
                    camera::transform(&h, [r[0], r[1] + 10.0]),
                );
                ((b[0] - a[0]).hypot(b[1] - a[1]) + (d[0] - a[0]).hypot(d[1] - a[1])) / 20.0
            })
            .filter(|s| (0.3..30.0).contains(s))
            .unwrap_or(gray.width() as f64 / 1300.0);
        if let Some(p) = camera::find_cross(gray, v, 40.0 * per_mm, 5.0 * per_mm) {
            c.points.view_points[i] = p;
            found += 1;
        }
    }
    c.crosses = Some((found, n));
    update_quality(c);
}

/// Breite der Spalte rechts neben dem Foto im Kalibrierdialog.
const PANEL_WIDTH: f32 = 290.0;

/// Spalte rechts im Kalibrierdialog: Objektiv, Kalibrierpunkte, Status und
/// Bedienhinweise, gestaltet wie die Einstellungsseiten.
fn calibration_panel(ui: &mut egui::Ui, c: &mut Calibration, height: f32) {
    use crate::object_bar::{Icon, list_button, list_footer};
    egui::ScrollArea::vertical()
        .id_salt("calibration_panel")
        .auto_shrink([false, false])
        .max_height(height)
        .show(ui, |ui| {
            crate::form::section(ui, tr("Objektiv"), |ui| {
                let before = c.lens;
                crate::form::Dropdown::from_id_salt("calibration_lens")
                    .width(ui.available_width())
                    .selected_text(c.lens.label())
                    .show_ui(ui, |ui| {
                        for choice in [LensChoice::RaspberryPiV2, LensChoice::Generic, LensChoice::Off] {
                            ui.selectable_value(&mut c.lens, choice, choice.label());
                        }
                    })
                    .response
                    .on_hover_text(tr("Die Verzeichnung des Objektivs wird ab 6 Kreuzen aus den Kreuzen bestimmt (ab 10 genauer); das Profil liefert Bildmitte, Brennweite und Startwert."));
                if c.lens != before {
                    update_quality(c);
                }
            });
            crate::form::section(ui, tr("Kalibrierpunkte"), |ui| {
                let current = [Grid::Fine, Grid::Coarse]
                    .into_iter()
                    .find(|g| g.points() == c.points.reference_points);
                let mut chosen = current;
                crate::form::row(ui, tr("Anordnung"), |ui| {
                    crate::form::Dropdown::from_id_salt("calibration_grid")
                        .width(130.0)
                        .selected_text(current.map_or(tr("Eigene Punkte"), Grid::label))
                        .show_ui(ui, |ui| {
                            for grid in [Grid::Fine, Grid::Coarse] {
                                ui.selectable_value(&mut chosen, Some(grid), grid.label());
                            }
                        })
                        .response
                        .on_hover_text(tr("5 × 3: 15 Kreuze gleichmäßig über das Bett, genauer und für die Linsenkorrektur. 3 × 2: 6 Kreuze wie bisher. Nach dem Wechsel die Kalibrierseite neu öffnen und markieren."));
                });
                if let Some(grid) = chosen.filter(|g| Some(*g) != current) {
                    c.grid = grid;
                    set_grid(c, grid);
                }
                ui.add_space(4.0);
                let height = ui.spacing().interact_size.y;
                egui::Grid::new("calibration_points")
                    .num_columns(3)
                    .spacing([8.0, 4.0])
                    .show(ui, |ui| {
                        for title in ["", "X (mm)", "Y (mm)"] {
                            ui.label(egui::RichText::new(title).color(Color32::from_gray(110)));
                        }
                        ui.end_row();
                        for (i, r) in c.points.reference_points.iter_mut().enumerate() {
                            let number = egui::RichText::new((i + 1).to_string());
                            if ui
                                .add_sized([30.0, height], egui::Button::selectable(c.selected == i, number))
                                .clicked()
                            {
                                c.selected = i;
                                if let Some(p) = c.points.view_points.get(i) {
                                    c.center = Some(Vec2::new(p[0] as f32, p[1] as f32));
                                }
                            }
                            ui.add_sized([72.0, height], egui::DragValue::new(&mut r[0]).range(0.0..=1000.0));
                            ui.add_sized([72.0, height], egui::DragValue::new(&mut r[1]).range(0.0..=600.0));
                            ui.end_row();
                        }
                    });
                ui.add_space(4.0);
                let has_picture = c.picture.is_some();
                let removable = c.points.reference_points.len() > 2;
                let moved = c.points.reference_points != c.grid.points();
                list_footer(ui, ui.available_width(), |ui| {
                    if list_button(ui, Icon::Plus, has_picture, tr("Punkt hinzufügen"))
                        && let Some((_, size)) = &c.picture
                    {
                        c.points.reference_points.push([500.0, 300.0]);
                        c.points.view_points.push([size.x as f64 / 2.0, size.y as f64 / 2.0]);
                        c.selected = c.points.reference_points.len() - 1;
                    }
                    if list_button(ui, Icon::Minus, removable, tr("Gewählten Punkt entfernen")) {
                        c.points.reference_points.remove(c.selected);
                        if c.selected < c.points.view_points.len() {
                            c.points.view_points.remove(c.selected);
                        }
                        c.selected = c.selected.saturating_sub(1);
                    }
                    ui.add_space(6.0);
                    let grid = c.grid;
                    if ui
                        .add_enabled(moved, egui::Button::new(tr("Zurücksetzen")))
                        .on_hover_text(tr!(
                            "Punkte wieder auf die Anordnung „{grid}“ setzen",
                            grid = grid.label()
                        ))
                        .clicked()
                    {
                        set_grid(c, grid);
                    }
                });
            });
            if c.picture.is_some() || c.problem.is_some() {
                crate::form::section(ui, tr("Status"), |ui| {
                    if c.picture.is_some() {
                        crate::form::caption(ui, match c.markers.len() {
                            0 => tr("Keine Passmarken erkannt: das Kamerabild wird nur über die Kalibrierung ausgerichtet.").to_owned(),
                            1 => tr("1 Passmarke erkannt (grün); für die Nachführung sind mindestens 2 nötig.").to_owned(),
                            n => tr!("{count} Passmarken erkannt (grün): spätere Bilder werden daran nachgeführt.", count = n),
                        });
                    }
                    if let Some((found, total)) = c.crosses {
                        crate::form::caption(ui, tr!("Kreuze erkannt: {found} von {total}", found = found, total = total));
                    }
                    if let Some(quality) = &c.quality {
                        crate::form::caption(ui, quality.clone());
                    }
                    if let Some(problem) = &c.problem {
                        ui.colored_label(Color32::from_rgb(180, 75, 40), problem);
                    }
                });
            }
            crate::form::section(ui, tr("Bedienung"), |ui| {
                crate::form::caption(ui, tr("1. Kalibrierseite öffnen und markieren.\n2. Foto aufnehmen.\n3. Marker auf die Kreuze ziehen (Nummer = Anzahl Striche).\nMindestens 4 Punkte."));
            });
        });
}

/// Knopfzeile unter dem Foto: Foto und Kreuze links, Ansicht in der Mitte,
/// Kalibrierseite und „Übernehmen“ rechts.
fn calibration_toolbar(
    ui: &mut egui::Ui,
    c: &mut Calibration,
    close: &mut bool,
    apply: &mut Option<CameraCalibration>,
    action: &mut Option<Action>,
) {
    use crate::object_bar::{Icon, list_button};
    ui.horizontal(|ui| {
        if ui
            .add_enabled(c.loading.is_none(), egui::Button::new(tr("Foto aufnehmen")))
            .clicked()
        {
            let device = c.device.clone();
            c.problem = None;
            c.loading = Some(spawn(move || camera::capture(&device)));
        }
        if ui
            .add_enabled(c.gray.is_some(), egui::Button::new(tr("Kreuze erkennen")))
            .on_hover_text(tr("Setzt jeden Punkt genau auf die Mitte des gelaserten Kreuzes in seiner Nähe. Punkte vorher grob in die Nähe ziehen."))
            .clicked()
        {
            detect_crosses(c);
        }
        ui.separator();
        if list_button(ui, Icon::Minus, c.zoom > 1.0, tr("Verkleinern")) {
            c.zoom = (c.zoom / 1.5).max(1.0);
        }
        ui.add_sized([52.0, ui.spacing().interact_size.y], egui::Label::new(format!("{:.0} %", c.zoom * 100.0)));
        if list_button(ui, Icon::Plus, c.zoom < 20.0, tr("Vergrößern")) {
            c.zoom = (c.zoom * 1.5).min(20.0);
        }
        if ui
            .add_enabled(c.picture.is_some(), egui::Button::new(tr("Auf Punkt")))
            .on_hover_text(tr("Gewählten Punkt vergrößert in die Mitte holen"))
            .clicked()
            && let Some(p) = c.points.view_points.get(c.selected)
        {
            c.zoom = c.zoom.max(6.0);
            c.center = Some(Vec2::new(p[0] as f32, p[1] as f32));
        }
        if ui
            .add_enabled(c.zoom > 1.0, egui::Button::new(tr("Ganzes Foto")))
            .clicked()
        {
            c.zoom = 1.0;
            c.center = None;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let ready = c.points.view_points.len() == c.points.reference_points.len();
            if ui
                .add_enabled(ready, crate::form::primary_button(tr("Übernehmen")))
                .clicked()
            {
                let result = fitted(c)
                    .ok_or_else(|| tr("Kalibrierpunkte liegen alle auf einer Linie").to_owned())
                    .and_then(|mut points| {
                        points.marker_points = camera::markers_on_bed(&c.markers, &points)?;
                        Ok(points)
                    });
                match result {
                    Ok(points) => {
                        *apply = Some(points);
                        *close = true;
                    }
                    Err(e) => c.problem = Some(e),
                }
            }
            if ui.button(tr("Kalibrierseite als Projekt öffnen")).clicked() {
                *action = Some(Action::CalibrationPage(c.points.reference_points.clone()));
            }
        });
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
            c.markers = camera::find_markers(&image);
            // Mit vorhandener Kalibrierung: Punkte dorthin, wo die Kreuze jetzt
            // zu erwarten sind (an den Passmarken nachgeführt), und gleich suchen.
            c.expected = c
                .device
                .camera_calibration
                .as_ref()
                .and_then(|previous| camera::tracked_mapping(previous, &image).ok())
                .map(|(mapping, _)| mapping);
            c.gray = Some(camera::Gray::new(&image));
            if let Some(mapping) = &c.expected {
                c.points.view_points = c
                    .points
                    .reference_points
                    .iter()
                    .map(|r| mapping.to_pixel(*r))
                    .collect();
                detect_crosses(c);
            }
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
        .default_size([980.0, 600.0])
        .collapsible(false)
        .show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                let space = Vec2::new(
                    (ui.available_width() - PANEL_WIDTH - 12.0).max(200.0),
                    // Platz für die Knopfzeile darunter.
                    (ui.available_height() - ui.spacing().interact_size.y - 18.0).max(420.0),
                );
                let (rect, response) = ui.allocate_exact_size(space, Sense::click_and_drag());
                let painter = ui.painter().with_clip_rect(rect);
                painter.rect_filled(rect, 0.0, Color32::from_gray(235));
                if let Some((texture, size)) = &c.picture {
                    let size = *size;
                    let fit = (rect.width() / size.x).min(rect.height() / size.y);
                    // Zoomen: Zwei-Finger-Zoom oder ⌘/Strg + Mausrad um den Mauszeiger,
                    // Verschieben: Scrollen oder mit rechter/mittlerer Maustaste ziehen.
                    if response.hovered() {
                        let (factor, scroll) =
                            ui.input(|i| (i.zoom_delta(), i.smooth_scroll_delta));
                        let pointer = response.hover_pos().unwrap_or(rect.center());
                        let before = c.center.unwrap_or(size / 2.0);
                        let anchor = before + (pointer - rect.center()) / (fit * c.zoom);
                        c.zoom = (c.zoom * factor).clamp(1.0, 20.0);
                        c.center = Some(
                            anchor
                                - (pointer - rect.center()) / (fit * c.zoom)
                                - scroll / (fit * c.zoom),
                        );
                    }
                    if response.dragged_by(egui::PointerButton::Secondary)
                        || response.dragged_by(egui::PointerButton::Middle)
                    {
                        c.center = Some(
                            c.center.unwrap_or(size / 2.0) - response.drag_delta() / (fit * c.zoom),
                        );
                    }
                    let scale = fit * c.zoom;
                    // Nicht über den Bildrand hinaus verschieben.
                    let half = rect.size() / (2.0 * scale);
                    let center = c.center.unwrap_or(size / 2.0);
                    let center = Vec2::new(
                        if half.x * 2.0 >= size.x {
                            size.x / 2.0
                        } else {
                            center.x.clamp(half.x, size.x - half.x)
                        },
                        if half.y * 2.0 >= size.y {
                            size.y / 2.0
                        } else {
                            center.y.clamp(half.y, size.y - half.y)
                        },
                    );
                    c.center = Some(center);
                    let image = Rect::from_min_size(rect.center() - center * scale, size * scale);
                    painter.image(
                        texture.id(),
                        image,
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        Color32::WHITE,
                    );
                    let to_screen =
                        |p: [f64; 2]| image.min + Vec2::new(p[0] as f32, p[1] as f32) * scale;
                    let primary =
                        response.clicked() || response.dragged_by(egui::PointerButton::Primary);
                    if let (Some(pointer), true) = (response.interact_pointer_pos(), primary) {
                        if response.drag_started_by(egui::PointerButton::Primary)
                            || response.clicked()
                        {
                            // Grab the nearest marker, otherwise move the selected one.
                            if let Some((i, _)) = c
                                .points
                                .view_points
                                .iter()
                                .enumerate()
                                .map(|(i, p)| (i, to_screen(*p).distance(pointer)))
                                .filter(|(_, d)| *d < 16.0)
                                .min_by(|a, b| a.1.total_cmp(&b.1))
                            {
                                c.selected = i;
                            }
                        }
                        if let Some(p) = c.points.view_points.get_mut(c.selected) {
                            let local = (pointer - image.min) / scale;
                            *p = [
                                local.x.clamp(0.0, size.x) as f64,
                                local.y.clamp(0.0, size.y) as f64,
                            ];
                        }
                    }
                    if response.drag_stopped() || response.clicked() {
                        c.quality = None;
                    }
                    // Pfeiltasten: gewählten Marker um einen Bildschirmpunkt verschieben (⇧: zehn).
                    if response.hovered() && !ctx.wants_keyboard_input() {
                        let step = ui.input(|i| {
                            let n = if i.modifiers.shift { 10.0 } else { 1.0 };
                            let mut d = Vec2::ZERO;
                            for (key, dir) in [
                                (egui::Key::ArrowLeft, -Vec2::X),
                                (egui::Key::ArrowRight, Vec2::X),
                                (egui::Key::ArrowUp, -Vec2::Y),
                                (egui::Key::ArrowDown, Vec2::Y),
                            ] {
                                if i.key_pressed(key) {
                                    d += dir * n;
                                }
                            }
                            d / scale
                        });
                        if step != Vec2::ZERO
                            && let Some(p) = c.points.view_points.get_mut(c.selected)
                        {
                            *p = [
                                (p[0] + step.x as f64).clamp(0.0, size.x as f64),
                                (p[1] + step.y as f64).clamp(0.0, size.y as f64),
                            ];
                        }
                    }
                    for p in &c.markers {
                        painter.circle_stroke(
                            to_screen(*p),
                            18.0,
                            Stroke::new(2.5, Color32::from_rgb(40, 180, 90)),
                        );
                    }
                    for (i, p) in c.points.view_points.iter().enumerate() {
                        let center = to_screen(*p);
                        let color = if i == c.selected {
                            Color32::from_rgb(40, 120, 230)
                        } else {
                            Color32::YELLOW
                        };
                        painter.circle_stroke(center, 12.0, Stroke::new(2.0, color));
                        painter.line_segment(
                            [center - Vec2::X * 12.0, center + Vec2::X * 12.0],
                            Stroke::new(1.0, color),
                        );
                        painter.line_segment(
                            [center - Vec2::Y * 12.0, center + Vec2::Y * 12.0],
                            Stroke::new(1.0, color),
                        );
                        painter.text(
                            center + Vec2::new(16.0, -16.0),
                            egui::Align2::LEFT_BOTTOM,
                            (i + 1).to_string(),
                            egui::FontId::proportional(14.0),
                            color,
                        );
                    }
                } else {
                    painter.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        if c.loading.is_some() {
                            tr("Foto wird aufgenommen …")
                        } else {
                            tr("Kein Kamerabild")
                        },
                        egui::FontId::proportional(16.0),
                        Color32::from_gray(120),
                    );
                }
                ui.vertical(|ui| {
                    ui.set_width(PANEL_WIDTH);
                    calibration_panel(ui, c, space.y);
                });
            });
            ui.add_space(6.0);
            calibration_toolbar(ui, c, close, apply, action);
        });
    *close |= !open;
}
