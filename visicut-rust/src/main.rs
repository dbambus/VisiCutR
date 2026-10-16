mod compose_ui;
mod devices_ui;
mod form;
mod jobs_ui;
mod preview_ui;
mod settings;
mod vectorize_ui;
mod workspace;

use eframe::egui::{
    self, Color32, Key, KeyboardShortcut, Modifiers, Pos2, Rect, Sense, Stroke, Vec2,
};
use std::path::{Path, PathBuf};
use visicut_core::{
    i18n::{self, tr},
    tr,
};
use visicut_core::{
    import,
    ltt::{self, OutputJob, PreparedJob},
    project::{CutOrder, Operation, Project},
    session, svg,
};

const WARNING: Color32 = Color32::from_rgb(180, 75, 40);

/// Befehle aus Menü und Tastatur. `Modifiers::COMMAND` ist Strg unter
/// Windows und Linux (⌘ unter macOS).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Command {
    New,
    Open,
    /// Grafiken als neue Teile hinzufügen.
    AddPart,
    Demo,
    Vectorize,
    Save,
    SaveAs,
    Export,
    /// Projekt für Java-VisiCut als PLF schreiben.
    ExportPlf,
    Quit,
    Center,
    Devices,
    ToggleGrid,
    ToggleCamera,
    RefreshCamera,
    ZoomIn,
    ZoomOut,
    ZoomFit,
    Materials,
    Preview,
    Send,
    Text,
    Shape,
    Sign,
    Array,
    Nest,
    Undo,
    Delete,
    Duplicate,
    /// Das gewählte Teil als neues Teil kopieren.
    DuplicatePart,
    RotateLeft,
    RotateRight,
    RotateBy,
    FlipHorizontal,
    FlipVertical,
    /// Eintrag aus „Zuletzt geöffnet“ (Index in der Liste).
    OpenRecent(usize),
    ClearRecent,
    /// Alle ungesicherten Zwischenstände löschen (`session`).
    ClearCache,
    /// Sprache der Oberfläche; `None` folgt dem System.
    Language(Option<i18n::Language>),
    /// Reihenfolge der Schnitt- und Markierpfade (Menü Job).
    CutOrder(CutOrder),
    /// Aktiver Lasercutter (Menü Datei → Lasercutter), Index in der Geräteliste.
    Device(usize),
}

const COMMAND_SHIFT: Modifiers = Modifiers::COMMAND.plus(Modifiers::SHIFT);

impl Command {
    /// Reihenfolge der Tastaturprüfung: Kürzel mit Umschalt zuerst, weil egui
    /// zusätzliche Umschalttasten beim Vergleich ignoriert.
    const KEYBOARD: [Command; 16] = [
        Self::AddPart,
        Self::SaveAs,
        Self::Export,
        Self::Materials,
        Self::RefreshCamera,
        Self::Preview,
        Self::New,
        Self::Open,
        Self::Save,
        Self::Quit,
        Self::Devices,
        Self::ToggleCamera,
        Self::ZoomFit,
        Self::ZoomIn,
        Self::ZoomOut,
        Self::ToggleGrid,
    ];

    fn title(self) -> &'static str {
        match self {
            Self::New => tr("Neues Projekt"),
            Self::Open => tr("Öffnen …"),
            Self::AddPart => tr("Hinzufügen …"),
            Self::Demo => tr("Beispiel öffnen"),
            Self::Vectorize => tr("Bitmap vektorisieren …"),
            Self::Save => tr("Sichern"),
            Self::SaveAs => tr("Sichern unter …"),
            Self::Export => tr("LTT-Job exportieren …"),
            Self::ExportPlf => tr("Als VisiCut-Projekt (PLF) exportieren …"),
            Self::Quit => tr("Beenden"),
            Self::Center => tr("Motiv auf dem Arbeitsbett zentrieren"),
            Self::Devices => tr("Einstellungen …"),
            Self::Device(_) => tr("Lasercutter"),
            Self::ToggleGrid => tr("Raster anzeigen"),
            Self::ToggleCamera => tr("Kamerabild anzeigen"),
            Self::RefreshCamera => tr("Kamerabild aktualisieren"),
            Self::ZoomIn => tr("Vergrößern"),
            Self::ZoomOut => tr("Verkleinern"),
            Self::ZoomFit => tr("Zoom zurücksetzen"),
            Self::Materials => tr("Materialbibliothek …"),
            Self::Preview => tr("Vorschau & Zeit …"),
            Self::Send => tr("An Lasercutter senden …"),
            Self::Text => tr("Text …"),
            Self::Shape => tr("Form …"),
            Self::Sign => tr("Schild …"),
            Self::Array => tr("Kopien im Raster …"),
            Self::Nest => tr("Teile verschachteln (Nesting) …"),
            Self::Undo => tr("Widerrufen"),
            Self::Delete => tr("Löschen"),
            Self::Duplicate => tr("Duplizieren"),
            Self::DuplicatePart => tr("Teil duplizieren"),
            Self::RotateLeft => tr("90° nach links drehen"),
            Self::RotateRight => tr("90° nach rechts drehen"),
            Self::RotateBy => tr("Um den Winkel drehen"),
            Self::FlipHorizontal => tr("Horizontal spiegeln"),
            Self::FlipVertical => tr("Vertikal spiegeln"),
            Self::OpenRecent(_) => tr("Zuletzt geöffnet"),
            Self::ClearRecent => tr("Liste leeren"),
            Self::ClearCache => tr("Zwischenspeicher leeren …"),
            Self::Language(_) => tr("Sprache"),
            Self::CutOrder(order) => jobs_ui::cut_order_title(order),
        }
    }

    fn shortcut(self) -> Option<KeyboardShortcut> {
        let (modifiers, key) = match self {
            Self::New => (Modifiers::COMMAND, Key::N),
            Self::Open => (Modifiers::COMMAND, Key::O),
            Self::AddPart => (COMMAND_SHIFT, Key::O),
            Self::Save => (Modifiers::COMMAND, Key::S),
            Self::SaveAs => (COMMAND_SHIFT, Key::S),
            Self::Export => (COMMAND_SHIFT, Key::E),
            Self::Quit => (Modifiers::COMMAND, Key::Q),
            Self::Devices => (Modifiers::COMMAND, Key::Comma),
            Self::ToggleCamera => (Modifiers::COMMAND, Key::K),
            Self::RefreshCamera => (COMMAND_SHIFT, Key::K),
            Self::ZoomIn => (Modifiers::COMMAND, Key::Plus),
            Self::ZoomOut => (Modifiers::COMMAND, Key::Minus),
            Self::ZoomFit => (Modifiers::COMMAND, Key::Num0),
            Self::Materials => (COMMAND_SHIFT, Key::M),
            Self::Preview => (COMMAND_SHIFT, Key::P),
            Self::ToggleGrid => (Modifiers::COMMAND, Key::G),
            // Nur ohne aktives Eingabefeld (siehe `keyboard`).
            Self::Undo => (Modifiers::COMMAND, Key::Z),
            Self::Delete => (Modifiers::NONE, Key::Delete),
            Self::Duplicate => (Modifiers::COMMAND, Key::D),
            Self::Demo
            | Self::Vectorize
            | Self::Center
            | Self::Send
            | Self::Text
            | Self::Shape
            | Self::Sign
            | Self::Array
            | Self::Nest
            | Self::DuplicatePart
            | Self::RotateLeft
            | Self::RotateRight
            | Self::RotateBy
            | Self::FlipHorizontal
            | Self::FlipVertical
            | Self::OpenRecent(_)
            | Self::ClearRecent
            | Self::ClearCache
            | Self::Language(_)
            | Self::CutOrder(_)
            | Self::Device(_)
            | Self::ExportPlf => return None,
        };
        Some(KeyboardShortcut::new(modifiers, key))
    }
}

/// Aktionen, die ungesicherte Änderungen verwerfen würden.
enum Replace {
    New,
    Open(PathBuf),
    Demo,
    Vectorized { svg: String, name: String },
    Calibration(Vec<[f64; 2]>),
    ShiftPattern(f32),
    Quit,
}

/// Dateien, deren Zeichnung über die Zeichenfläche hinausreicht; die Wahl
/// gilt für alle gemeinsam.
struct PendingImport {
    /// Alle abgelegten Dateien, auch solche ohne Überstand.
    paths: Vec<PathBuf>,
    /// Die erste Grafik ersetzt das Projekt (Öffnen) statt Teil zu werden.
    replace: bool,
    /// Dateinamen mit Überstand.
    names: Vec<String>,
}

/// Mehrere LTT-Aufträge warten auf die Bestätigung zum Ersetzen.
struct PendingExport {
    targets: Vec<PathBuf>,
    existing: Vec<String>,
    jobs: Vec<OutputJob>,
}

/// Stand vor einer Werkzeug- oder Löschaktion: Projekt mit allen Teilen und
/// deren Texturen.
struct Snapshot {
    project: Project,
    texture: Option<egui::TextureHandle>,
    part_textures: Vec<Option<egui::TextureHandle>>,
}

/// Name eines Teils für Listen; ohne Namen „Teil 2“ (Position in der Liste).
fn part_title(name: &str, index: usize) -> String {
    if name.trim().is_empty() {
        tr!("Teil {number}", number = index + 1)
    } else {
        name.to_owned()
    }
}

/// Name und Größe des Teils an `index` (Listenreihenfolge) ohne Kopie der
/// Grafik.
fn part_summary(project: &Project, index: usize) -> (&str, f32, f32) {
    if index == project.active_part {
        return (&project.part_name, project.width_mm, project.height_mm);
    }
    let part = &project.parts[index - usize::from(index > project.active_part)];
    (&part.name, part.width_mm, part.height_mm)
}

/// Reiter der Hauptleiste wie in Bambu Studio.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Prepare,
    Preview,
    Settings,
}

/// Bereich im Reiter „Einstellungen“.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsPage {
    Devices,
    Materials,
    General,
}

struct VisiCutRust {
    project: Project,
    page: Page,
    settings_page: SettingsPage,
    /// Textur des gewählten Teils.
    texture: Option<egui::TextureHandle>,
    /// Texturen der übrigen Teile, parallel zu `project.parts`.
    part_textures: Vec<Option<egui::TextureHandle>>,
    /// Objekte in den Farben ihrer Bearbeitung (`ltt::processing_preview`)
    /// und der Stand, für den sie gilt.
    live: Option<(u64, Option<egui::TextureHandle>)>,
    /// Umriss des Objekts unter der Maus in der Objektliste (mm relativ zur
    /// Motivecke) mit dem Stand und Objekt, für das er gilt.
    hover_outline: Option<(u64, usize, Vec<visicut_core::geometry::Contour>)>,
    /// Umrisse aller Objekte für den Treffertest auf der Arbeitsfläche.
    all_outlines: Option<(u64, Vec<Vec<visicut_core::geometry::Contour>>)>,
    /// Nullpunkt des Geräts beim letzten Bild (unten links?); ein Wechsel
    /// setzt den Standard-Bezugspunkt.
    origin_bottom_left: Option<bool>,
    status: String,
    /// Deutlich sichtbarer Hinweis (Fehler, Importwarnungen) in einem Dialog.
    alert: Option<String>,
    /// Zeichnung ragt über die Zeichenfläche: Auswahl steht aus.
    pending_import: Option<PendingImport>,
    /// Gewählte Behandlung beim laufenden Import: `Some(true)` ganze Zeichnung.
    fit_policy: Option<bool>,
    project_path: Option<PathBuf>,
    /// Projektname während des Umbenennens im Kopf der Arbeitsfläche.
    name_draft: Option<String>,
    /// Zwischenstand, aus dem das Projekt geöffnet wurde (`session`);
    /// Sichern löscht ihn.
    cached: Option<PathBuf>,
    dirty: bool,
    proportional: bool,
    allow_close: bool,
    zoom: f32,
    /// Bildpunkte je mm auf dem Bildschirm, zuletzt in `bed` berechnet.
    view_scale: f32,
    show_grid: bool,
    /// Projekt ist vollständig und passt aufs Bett (pro Bild neu bestimmt).
    sendable: bool,
    /// Wartet auf „Sichern / Nicht sichern / Abbrechen“.
    replace: Option<Replace>,
    overwrite: Option<PendingExport>,
    /// Wartet auf die Bestätigung „Zwischenspeicher leeren“ (Anzahl Stände).
    confirm_clear_cache: Option<usize>,
    confirm_send: Option<PreparedJob>,
    preview: preview_ui::PreviewUi,
    /// „Vorschau & Zeit“ (Menü, Reiterleiste) setzt dieses Feld; `update`
    /// berechnet die Vorschau und wechselt in den Reiter „Vorschau“.
    show_preview: bool,
    send_result: Option<std::sync::mpsc::Receiver<Result<Vec<String>, String>>>,
    devices: devices_ui::DeviceUi,
    jobs: jobs_ui::JobUi,
    vectorize: vectorize_ui::VectorizeUi,
    compose: compose_ui::ComposeUi,
    /// Stand vor der letzten Werkzeug- oder Löschaktion (ein Schritt).
    undo: Option<Snapshot>,
    /// Bezugspunkt für X/Y und Größenänderungen (0, ½ oder 1 je Achse).
    reference: [f32; 2],
    rotate_deg: f32,
    /// Ziehen an einem Anfasser des gewählten Teils.
    handle_drag: Option<HandleDrag>,
    /// Sprache und zuletzt geöffnete Dateien.
    settings: settings::AppSettings,
    /// Laufende Update-Prüfung (`true`: von Hand gestartet) und gefundenes Release.
    update_check: Option<(bool, UpdateCheck)>,
    update_found: Option<visicut_core::update::Release>,
    #[cfg(feature = "screenshot")]
    capture: Option<(PathBuf, u32)>,
}

impl VisiCutRust {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let settings = settings::AppSettings::load();
        i18n::set_language(settings.language());
        // Die Oberfläche ist hell: auch die Titelleiste hell, nicht schwarz auf
        // dunklem Hintergrund.
        cc.egui_ctx.set_theme(egui::ThemePreference::Light);
        // Die Titelleiste richtet winit nach dem Windows-Theme; egui gibt die
        // Vorgabe nicht von selbst weiter.
        cc.egui_ctx
            .send_viewport_cmd(egui::ViewportCommand::SetTheme(egui::SystemTheme::Light));
        cc.egui_ctx.set_visuals(egui::Visuals::light());
        form::apply_style(&cc.egui_ctx);
        // Strg+Plus/Minus/0 zoomen die Arbeitsfläche, nicht die Oberfläche.
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        let (devices, device_error) = devices_ui::DeviceUi::load();
        let (jobs, material_error) = jobs_ui::JobUi::load();
        let mut app = Self {
            project: Project::default(),
            texture: None,
            live: None,
            hover_outline: None,
            all_outlines: None,
            part_textures: Vec::new(),
            origin_bottom_left: None,
            status: tr("Datei öffnen, hierher ziehen oder das Beispiel öffnen").into(),
            alert: None,
            pending_import: None,
            fit_policy: None,
            project_path: None,
            name_draft: None,
            cached: None,
            dirty: false,
            proportional: true,
            allow_close: false,
            zoom: 1.0,
            view_scale: 1.0,
            show_grid: true,
            sendable: false,
            replace: None,
            overwrite: None,
            confirm_clear_cache: None,
            confirm_send: None,
            show_preview: false,
            page: Page::Prepare,
            settings_page: SettingsPage::Devices,
            preview: preview_ui::PreviewUi::default(),
            send_result: None,
            devices,
            jobs,
            vectorize: vectorize_ui::VectorizeUi::default(),
            compose: compose_ui::ComposeUi::default(),
            undo: None,
            reference: [0.0, 0.0],
            rotate_deg: 0.0,
            handle_drag: None,
            settings,
            update_check: None,
            update_found: None,
            #[cfg(feature = "screenshot")]
            capture: None,
        };
        app.reset_project();
        app.preview.single_job = app.settings.single_job;
        // Alle 14 Tage im Hintergrund nach einem neuen Release fragen.
        if !app.settings.update_check_disabled
            && visicut_core::update::is_due(
                app.settings.last_update_check,
                visicut_core::update::now_secs(),
            )
        {
            app.start_update_check(false);
        }
        if let Some(error) = device_error {
            app.report(tr!(
                "Geräteliste nicht lesbar, FAU-Standard wird verwendet: {error}",
                error = error
            ));
        } else if let Some(error) = material_error {
            app.report(error);
        }
        let arguments: Vec<String> = std::env::args().skip(1).collect();
        #[cfg(feature = "screenshot")]
        if let Some(index) = arguments
            .iter()
            .position(|argument| argument == "--capture")
        {
            app.capture = arguments
                .get(index + 1)
                .map(|path| (PathBuf::from(path), 0));
        }
        // Reiter für die Aufnahme: `--page preview|materials|general|devices`.
        #[cfg(feature = "screenshot")]
        if let Some(index) = arguments.iter().position(|argument| argument == "--page") {
            match arguments.get(index + 1).map(String::as_str) {
                Some("preview") => app.show_preview = true,
                Some("devices") => app.show_settings(SettingsPage::Devices),
                Some("materials") => app.show_settings(SettingsPage::Materials),
                Some("general") => app.show_settings(SettingsPage::General),
                _ => {}
            }
        }
        if arguments
            .first()
            .is_some_and(|argument| argument == "--demo")
        {
            app.perform(&cc.egui_ctx, Replace::Demo);
        } else if let Some(path) = arguments.first().filter(|a| !a.starts_with('-')) {
            app.perform(&cc.egui_ctx, Replace::Open(PathBuf::from(path)));
        } else if let Some(path) = session::take_resume(&session::dir()) {
            // Beim letzten Beenden ungesicherter Stand.
            app.perform(&cc.egui_ctx, Replace::Open(path));
        }
        cc.egui_ctx.request_repaint();
        app
    }

    /// Sichert Sprache und Verlauf; ein Fehler steht nur in der Statuszeile.
    /// Fragt im Hintergrund bei GitHub nach einem neuen Release.
    fn start_update_check(&mut self, manual: bool) {
        if self.update_check.is_some() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(visicut_core::update::check());
        });
        self.update_check = Some((manual, rx));
        if manual {
            self.status = tr("Suche nach Updates …").into();
        }
    }

    /// Ergebnis der Update-Prüfung; Fehler der automatischen Prüfung stehen
    /// nur in der Statuszeile.
    fn poll_update(&mut self, ctx: &egui::Context) {
        let Some((manual, receiver)) = &self.update_check else {
            return;
        };
        let manual = *manual;
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
                return;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Err(tr("Update-Prüfung abgebrochen").into())
            }
        };
        self.update_check = None;
        match result {
            Ok(found) => {
                self.settings.last_update_check = visicut_core::update::now_secs();
                self.save_settings();
                match found {
                    Some(release) => self.update_found = Some(release),
                    None if manual => {
                        self.status = tr!(
                            "VisiCutRust {version} ist aktuell",
                            version = visicut_core::update::current_version()
                        )
                    }
                    None => {}
                }
            }
            Err(e) if manual => self.report(e),
            Err(e) => self.status = e,
        }
    }

    /// Hinweis auf ein neues Release mit Link zur Download-Seite.
    fn update_window(&mut self, ctx: &egui::Context) {
        let Some(release) = &self.update_found else {
            return;
        };
        let mut close = false;
        egui::Window::new(tr("Update verfügbar"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(tr!(
                    "VisiCutRust {version} ist verfügbar (installiert: {current}).",
                    version = release.version,
                    current = visicut_core::update::current_version()
                ));
                if !release.notes.is_empty() {
                    egui::ScrollArea::vertical()
                        .max_height(200.0)
                        .show(ui, |ui| {
                            ui.small(&release.notes);
                        });
                }
                ui.horizontal(|ui| {
                    if ui.button(tr("Download-Seite öffnen")).clicked() {
                        ctx.open_url(egui::OpenUrl::new_tab(&release.url));
                        close = true;
                    }
                    close |= ui.button(tr("Später")).clicked();
                });
            });
        if close {
            self.update_found = None;
        }
    }

    fn save_settings(&mut self) {
        if let Err(e) = self.settings.save() {
            self.status = tr!("Einstellungen nicht gesichert: {error}", error = e);
        }
    }

    /// Merkt sich eine geöffnete oder gesicherte Datei für „Zuletzt geöffnet“.
    fn remember(&mut self, path: &Path) {
        self.settings.remember(path);
        self.save_settings();
    }

    /// Fehler und Warnungen: Statuszeile und ein Hinweisdialog.
    fn report(&mut self, message: String) {
        self.status = message.lines().next().unwrap_or_default().to_owned();
        self.alert = Some(match self.alert.take() {
            Some(previous) if previous != message => format!("{previous}\n\n{message}"),
            _ => message,
        });
    }

    fn busy(&self) -> bool {
        self.send_result.is_some()
    }

    fn modal_open(&self) -> bool {
        self.alert.is_some()
            || self.replace.is_some()
            || self.overwrite.is_some()
            || self.confirm_clear_cache.is_some()
            || self.confirm_send.is_some()
    }

    fn reset_project(&mut self) {
        self.project = Project::default();
        self.project.name = tr("Neues Projekt").into();
        self.project.material.clear();
        self.devices.apply(&mut self.project);
        self.texture = None;
        self.part_textures.clear();
        self.project_path = None;
        self.cached = None;
        self.dirty = false;
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            project: self.project.clone(),
            texture: self.texture.clone(),
            part_textures: self.part_textures.clone(),
        }
    }

    /// Texturen aller Teile in Listenreihenfolge.
    fn all_textures(&self) -> Vec<Option<egui::TextureHandle>> {
        let mut all = self.part_textures.clone();
        if self.project.has_artwork() {
            all.insert(
                self.project.active_part.min(all.len()),
                self.texture.clone(),
            );
        }
        all
    }

    /// Verteilt Texturen in Listenreihenfolge nach einer Umordnung der Teile
    /// (Wählen, Hinzufügen, Entfernen) auf das gewählte und die übrigen Teile.
    fn arrange_textures(&mut self, mut all: Vec<Option<egui::TextureHandle>>) {
        if !self.project.has_artwork() || all.is_empty() {
            self.texture = None;
            self.part_textures.clear();
            return;
        }
        let active = self.project.active_part.min(all.len() - 1);
        self.texture = all.remove(active);
        self.part_textures = all;
    }

    /// Texturen für alle Teile von `project`; Grafiken, die schon angezeigt
    /// werden, behalten ihre Textur, die übrigen werden gezeichnet.
    #[allow(clippy::type_complexity)]
    fn textures_for(
        &self,
        ctx: &egui::Context,
        project: &Project,
    ) -> Result<
        (
            Option<egui::TextureHandle>,
            Vec<Option<egui::TextureHandle>>,
        ),
        String,
    > {
        let existing: Vec<(&str, &egui::TextureHandle)> = std::iter::once(&self.texture)
            .chain(&self.part_textures)
            .zip(
                std::iter::once(self.project.svg.as_str())
                    .chain(self.project.parts.iter().map(|p| p.svg.as_str())),
            )
            .filter_map(|(texture, svg)| texture.as_ref().map(|t| (svg, t)))
            .collect();
        let texture = |svg: &str| -> Result<Option<egui::TextureHandle>, String> {
            if svg.is_empty() {
                return Ok(None);
            }
            if let Some((_, texture)) = existing.iter().find(|(s, _)| *s == svg) {
                return Ok(Some((*texture).clone()));
            }
            let preview = svg::render(svg)?;
            Ok(Some(ctx.load_texture(
                "SVG",
                preview.image,
                egui::TextureOptions::LINEAR,
            )))
        };
        let active = texture(&project.svg)?;
        let others = project
            .parts
            .iter()
            .map(|part| texture(&part.svg).map_err(|e| format!("{}: {e}", part.name)))
            .collect::<Result<_, _>>()?;
        Ok((active, others))
    }

    /// Wählt das Teil an `index` (Listenreihenfolge) zum Bearbeiten.
    fn select_part(&mut self, index: usize) {
        if index == self.project.active_part || index >= self.project.part_count() {
            return;
        }
        let all = self.all_textures();
        self.project.select_part(index);
        self.arrange_textures(all);
        self.status = tr!(
            "Teil „{name}“ gewählt",
            name = part_title(&self.project.part_name, index)
        );
    }

    /// Kopiert das gewählte Teil um 5 mm versetzt als neues Teil.
    fn duplicate_part(&mut self) {
        if self.texture.is_none() {
            return;
        }
        self.undo = Some(self.snapshot());
        let mut all = self.all_textures();
        all.push(self.texture.clone());
        let mut part = self.project.active();
        part.x_mm += 5.0;
        part.y_mm += 5.0;
        self.project.add_part(part);
        self.arrange_textures(all);
        self.dirty = true;
        self.status = tr("Teil dupliziert · Bearbeiten › Widerrufen macht es rückgängig").into();
    }

    /// Grafiken über den Dateidialog als neue Teile hinzufügen.
    fn choose_parts(&mut self, ctx: &egui::Context) {
        if self.busy() {
            return;
        }
        if let Some(paths) = rfd::FileDialog::new()
            .set_title(tr("Grafiken als Teile hinzufügen"))
            .add_filter(tr("Grafik"), import::EXTENSIONS)
            .pick_files()
        {
            self.drop_files(ctx, paths);
        }
    }

    /// Übernimmt eine Grafik. Wie bei jedem Import gelten alte Schritte und
    /// Regeln nicht mehr; PLF-Dateien bringen ihre eigenen Schritte mit.
    fn import(
        &mut self,
        ctx: &egui::Context,
        source: String,
        name: String,
        steps: Vec<visicut_core::project::JobStep>,
    ) -> Result<(), String> {
        let preview = svg::render(&source)?;
        self.project.svg = source;
        self.project.part_name = name.clone();
        self.project.name = name;
        self.project.width_mm = preview.width_mm;
        self.project.height_mm = preview.height_mm;
        self.project.steps = steps;
        self.project.ignore_filters.clear();
        // Die neue Grafik ersetzt alle Teile.
        self.project.parts.clear();
        self.project.active_part = 0;
        self.part_textures.clear();
        self.texture = Some(ctx.load_texture("SVG", preview.image, egui::TextureOptions::LINEAR));
        self.project_path = None;
        self.cached = None;
        self.dirty = true;
        self.status = tr("Datei importiert · Maße aus der Datei übernommen").into();
        Ok(())
    }

    fn finish_import(
        &mut self,
        ctx: &egui::Context,
        path: &Path,
        imported: visicut_core::svg_import::Imported,
    ) -> Result<(), String> {
        let name = path.file_stem().unwrap_or_default().to_string_lossy();
        self.import(ctx, imported.svg, name.into(), imported.steps)?;
        self.status = tr!(
            "{file} importiert · Originalmaße übernommen",
            file = path.file_name().unwrap_or_default().to_string_lossy()
        );
        self.remember(path);
        if !imported.warnings.is_empty() {
            self.alert = Some(imported.warnings.join("\n"));
        }
        Ok(())
    }

    fn open_path(&mut self, ctx: &egui::Context, path: &Path) -> Result<(), String> {
        let meta = std::fs::metadata(path).map_err(|e| {
            tr!(
                "{path} ist nicht lesbar: {error}",
                path = path.display(),
                error = e
            )
        })?;
        if meta.len() > import::MAX_FILE_BYTES {
            return Err(tr("Datei ist größer als 25 MB").into());
        }
        if import::is_importable(path) {
            let mut imported = import::read_file(path)?;
            if let Some(whole) = visicut_core::svg_import::fit_to_content(&imported.svg) {
                match self.fit_policy {
                    Some(true) => imported.svg = whole,
                    Some(false) => {}
                    None => {
                        self.pending_import = Some(PendingImport {
                            paths: vec![path.to_owned()],
                            replace: true,
                            names: vec![file_name(path)],
                        });
                        return Ok(());
                    }
                }
            }
            self.finish_import(ctx, path, imported)
        } else {
            let source = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
            let project: Project = serde_json::from_str(&source)
                .map_err(|e| tr!("Ungültiges Projekt: {error}", error = e))?;
            // Entwürfe ohne Motiv oder außerhalb des Betts lassen sich öffnen.
            project.validate_document()?;
            (self.texture, self.part_textures) = self.textures_for(ctx, &project)?;
            self.project = project;
            if visicut_core::i18n::is_default_text("Material wählen", &self.project.material) {
                self.project.material.clear();
            }
            self.devices.apply(&mut self.project);
            // Ein Zwischenstand hat noch keinen eigenen Speicherort.
            if session::is_cached(&session::dir(), path) {
                self.project_path = None;
                self.cached = Some(path.to_owned());
                self.dirty = true;
                self.status = tr("Ungesicherter Stand geöffnet. Zum Behalten sichern.").into();
            } else {
                self.project_path = Some(path.to_owned());
                self.cached = None;
                self.dirty = false;
                self.status = tr("Projekt geöffnet").into();
            }
            self.remember(path);
            Ok(())
        }
    }

    /// Abgelegte Dateien: Grafiken kommen als neue Teile hinzu (ohne Motiv
    /// öffnet die erste), ein einzelnes Projekt wird geöffnet.
    fn drop_files(&mut self, ctx: &egui::Context, mut paths: Vec<PathBuf>) {
        if self.busy() {
            self.status = tr("Bitte die laufende Übertragung abwarten").into();
            return;
        }
        let (graphics, projects): (Vec<PathBuf>, Vec<PathBuf>) =
            paths.drain(..).partition(|p| import::is_importable(p));
        if graphics.is_empty() {
            if let Some(path) = projects.into_iter().next() {
                self.request(ctx, Replace::Open(path));
            }
            return;
        }
        if self.fit_policy.is_none() {
            let overflowing: Vec<String> = graphics
                .iter()
                .filter(|p| {
                    import::read_file(p)
                        .is_ok_and(|i| visicut_core::svg_import::fit_to_content(&i.svg).is_some())
                })
                .map(|p| file_name(p))
                .collect();
            if !overflowing.is_empty() {
                // Eine Frage für alle Dateien; danach läuft der Import weiter.
                self.pending_import = Some(PendingImport {
                    replace: self.project.svg.is_empty(),
                    paths: graphics.iter().chain(&projects).cloned().collect(),
                    names: overflowing,
                });
                return;
            }
        }
        let mut notes: Vec<String> = projects
            .iter()
            .map(|p| {
                tr!(
                    "{file}: Projekte lassen sich nicht hinzufügen, nur öffnen",
                    file = p.file_name().unwrap_or_default().to_string_lossy()
                )
            })
            .collect();
        let mut rest = graphics.as_slice();
        if self.project.svg.is_empty() {
            if let Err(e) = self.open_path(ctx, &rest[0]) {
                notes.insert(0, e);
                return self.report(notes.join("\n"));
            }
            rest = &rest[1..];
            if let Some(warnings) = self.alert.take() {
                notes.insert(0, warnings);
            }
        }
        if !rest.is_empty() {
            // Jede Datei wird ein eigenes Teil mit eigener Lage und Zuordnung.
            let before = self.project.part_count();
            match visicut_core::compose::add_files_as_parts(
                &self.project,
                rest,
                self.fit_policy == Some(true),
            ) {
                Ok((project, more)) => {
                    notes.extend(more);
                    self.apply_composed(ctx, project, Vec::new());
                    let added = self.project.part_count().saturating_sub(before);
                    if added > 0 {
                        self.status = tr!(
                            "{count} Datei(en) als Teile hinzugefügt · \
                             Bearbeiten › Widerrufen macht es rückgängig",
                            count = added
                        );
                    }
                }
                Err(e) => notes.push(e),
            }
        }
        if !notes.is_empty() {
            self.alert = Some(notes.join("\n"));
        }
    }

    /// Sichert das Projekt; `false`, wenn der Dialog abgebrochen wurde.
    fn save(&mut self, as_copy: bool) -> Result<bool, String> {
        self.project.validate_document()?;
        let path = self.project_path.clone().filter(|_| !as_copy).or_else(|| {
            rfd::FileDialog::new()
                .set_title(tr("VisiCutRust-Projekt sichern"))
                .add_filter(tr("VisiCutRust Projekt"), &["vcr"])
                .set_file_name(format!("{}.vcr", self.project.name))
                .save_file()
        });
        let Some(path) = path else {
            return Ok(false);
        };
        let bytes = serde_json::to_vec_pretty(&self.project).map_err(|e| e.to_string())?;
        workspace::write_atomic(&path, &bytes)?;
        if let Some(cache) = self.cached.take() {
            session::discard(&session::dir(), &cache);
            let cache = std::fs::canonicalize(&cache).unwrap_or(cache);
            self.settings.recent.retain(|p| p != &cache && p.exists());
        }
        self.remember(&path);
        self.project_path = Some(path);
        self.dirty = false;
        self.status = tr("Projekt mit eingebetteter SVG gesichert").into();
        Ok(true)
    }

    /// Führt eine Aktion aus. Ungesicherte Änderungen kommen vorher ohne
    /// Rückfrage in den Zwischenspeicher; nur wenn das scheitert, wird gefragt.
    fn request(&mut self, ctx: &egui::Context, action: Replace) {
        if self.busy() && !matches!(action, Replace::Quit) {
            self.status = tr("Bitte die laufende Übertragung abwarten").into();
        } else if self.dirty && !self.stash(matches!(action, Replace::Quit)) {
            self.replace = Some(action);
        } else {
            self.perform(ctx, action);
        }
    }

    /// Löscht alle Zwischenstände und ihre Einträge unter „Zuletzt geöffnet“.
    /// Das geöffnete Projekt bleibt im Speicher, verliert aber die Bindung.
    fn clear_cache(&mut self) {
        match session::clear(&session::dir()) {
            Ok(removed) => {
                let removed: Vec<PathBuf> = removed
                    .into_iter()
                    .map(|p| std::fs::canonicalize(&p).unwrap_or(p))
                    .collect();
                self.settings
                    .recent
                    .retain(|p| !removed.contains(p) && p.exists());
                self.save_settings();
                self.cached = None;
                self.status = tr!(
                    "Zwischenspeicher geleert ({count} Stände)",
                    count = removed.len()
                );
            }
            Err(e) => self.report(tr!("Zwischenspeicher nicht geleert: {error}", error = e)),
        }
    }

    /// Legt den ungesicherten Stand ab und führt ihn unter „Zuletzt
    /// geöffnet“; mit `resume` öffnet er beim nächsten Start wieder.
    fn stash(&mut self, resume: bool) -> bool {
        let dir = session::dir();
        let stashed =
            session::stash(&dir, &self.project, self.cached.as_deref()).and_then(|path| {
                if resume {
                    session::set_resume(&dir, Some(&path))?;
                }
                Ok(path)
            });
        match stashed {
            Ok(path) => {
                self.remember(&path);
                true
            }
            Err(e) => {
                self.report(tr!("Zwischenspeichern fehlgeschlagen: {error}", error = e));
                false
            }
        }
    }

    fn perform(&mut self, ctx: &egui::Context, action: Replace) {
        let result = match action {
            Replace::New => {
                self.reset_project();
                self.status = tr("Neues Projekt. Datei öffnen, um zu beginnen.").into();
                Ok(())
            }
            Replace::Open(path) => self.open_path(ctx, &path),
            Replace::Demo => self
                .import(
                    ctx,
                    include_str!("../examples/demo.svg").into(),
                    tr("Beispiel").into(),
                    Vec::new(),
                )
                .map(|()| {
                    self.status =
                        tr("Beispiel geöffnet. Material und Bearbeitung auswählen.").into();
                }),
            Replace::Vectorized { svg, name } => self.import(ctx, svg, name, Vec::new()),
            Replace::Calibration(points) => self.calibration_page(ctx, &points),
            Replace::ShiftPattern(power) => self.shift_pattern(ctx, power),
            Replace::Quit => {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                Ok(())
            }
        };
        if let Err(e) = result {
            self.report(e);
        }
    }

    /// VisiCuts Kalibriermarken als Markierauftrag auf dem ganzen Bett.
    fn calibration_page(&mut self, ctx: &egui::Context, points: &[[f64; 2]]) -> Result<(), String> {
        let [w, h] = devices_ui::BED_MM.map(f64::from);
        if points
            .iter()
            .any(|p| !(5.0..=w - 5.0).contains(&p[0]) || !(5.0..=h - 15.0).contains(&p[1]))
        {
            return Err(tr(
                "Kalibrierpunkte müssen mindestens 5 mm (unten 15 mm) vom Bettrand entfernt sein",
            )
            .into());
        }
        let svg = visicut_core::camera::calibration_svg(points, w, h);
        self.import(ctx, svg, tr("Kalibrierung").into(), Vec::new())?;
        self.project.rotary_axis = false;
        (self.project.x_mm, self.project.y_mm) = (0.0, 0.0);
        self.project.operation = Operation::Mark;
        self.status = tr(
            "Kalibrierseite geöffnet: Material und Markier-Parameter prüfen, senden, danach Foto aufnehmen.",
        )
        .into();
        Ok(())
    }

    /// Übernimmt das Ergebnis eines Objekt-Werkzeugs; neue Schritte erhalten
    /// die Werte der Materialbibliothek.
    /// Andere Teile bleiben erhalten; neue Teile (Hinzufügen) bringen ihre
    /// eigenen Schritte mit.
    fn apply_composed(&mut self, ctx: &egui::Context, project: Project, notes: Vec<String>) {
        let textures = match self.textures_for(ctx, &project) {
            Ok(textures) => textures,
            Err(e) => return self.report(e),
        };
        self.undo = Some(self.snapshot());
        let same_part = project.active_part == self.project.active_part
            && project.parts.len() == self.project.parts.len();
        let old_steps = if same_part {
            self.project.steps.len()
        } else {
            project.steps.len()
        };
        let old_objects = visicut_core::selection::objects(&self.project.svg)
            .map(|o| o.len())
            .unwrap_or(0);
        self.project = project;
        (self.texture, self.part_textures) = textures;
        for index in old_steps..self.project.steps.len() {
            // Steps of the existing motif keep its parameters.
            if self.project.steps[index]
                .objects
                .iter()
                .all(|o| *o >= old_objects)
            {
                self.jobs.apply_preset_to(&mut self.project, index);
            }
        }
        self.dirty = true;
        self.status = tr!(
            "Motiv aktualisiert · {width} × {height} mm",
            width = format!("{:.0}", self.project.width_mm),
            height = format!("{:.0}", self.project.height_mm)
        );
        if !notes.is_empty() {
            self.alert = Some(notes.join("\n"));
        }
    }

    fn composed(&mut self, ctx: &egui::Context, result: Result<Project, String>) {
        match result {
            Ok(project) => self.apply_composed(ctx, project, Vec::new()),
            Err(e) => self.report(e),
        }
    }

    /// Drehen und Spiegeln um die Mitte des Motivs (ohne Verzerren).
    fn transform(
        &mut self,
        ctx: &egui::Context,
        rotate_deg: f64,
        horizontal: bool,
        vertical: bool,
    ) {
        let result =
            visicut_core::compose::transform(&self.project, rotate_deg, horizontal, vertical);
        self.composed(ctx, result);
    }

    /// Entfernt das gewählte Teil; das folgende wird gewählt. Ohne weitere
    /// Teile bleibt ein leeres Projekt, Material, Gerät und Einstellungen
    /// bleiben.
    fn delete_part(&mut self) {
        if self.texture.is_none() {
            return;
        }
        self.undo = Some(self.snapshot());
        let several = self.project.part_count() > 1;
        let mut all = self.all_textures();
        all.remove(self.project.active_part.min(all.len() - 1));
        self.project.remove_active_part();
        self.arrange_textures(all);
        self.dirty = true;
        self.status = if several {
            tr("Teil entfernt · Bearbeiten › Widerrufen stellt es wieder her")
        } else {
            tr("Motiv entfernt · Bearbeiten › Widerrufen stellt es wieder her")
        }
        .into();
    }

    /// Löscht einzelne Objekte aus dem Motiv (Objektliste).
    fn delete_objects(&mut self, ctx: &egui::Context, objects: &[usize]) {
        let result = visicut_core::compose::remove_objects(&self.project, objects);
        match result {
            Ok(project) if project.svg.is_empty() => self.delete_part(),
            other => self.composed(ctx, other),
        }
    }

    fn undo_last(&mut self) {
        if let Some(snapshot) = self.undo.take() {
            self.project = snapshot.project;
            self.texture = snapshot.texture;
            self.part_textures = snapshot.part_textures;
            self.dirty = true;
            self.status = tr("Letzte Änderung widerrufen").into();
        }
    }

    /// Testmuster für den Zeilenversatz mit den Werten des aktiven Geräts.
    fn shift_pattern(&mut self, ctx: &egui::Context, power: f32) -> Result<(), String> {
        let pattern = visicut_core::shift_calibration::project(&self.project, power);
        self.import(
            ctx,
            pattern.svg.clone(),
            pattern.name.clone(),
            pattern.steps.clone(),
        )?;
        self.project = Project {
            width_mm: self.project.width_mm,
            height_mm: self.project.height_mm,
            // Das Muster ersetzt alle Teile (`pattern` übernimmt sie sonst
            // aus dem bisherigen Projekt).
            part_name: self.project.part_name.clone(),
            active_part: 0,
            parts: Vec::new(),
            ..pattern
        };
        self.status = tr(
            "Zeilenversatz-Testmuster geöffnet: Material prüfen, senden, danach je Zeile den besten Block eintragen.",
        )
        .into();
        Ok(())
    }

    fn choose_file(&mut self, ctx: &egui::Context) {
        if self.busy() {
            return;
        }
        if let Some(path) = rfd::FileDialog::new()
            .set_title(tr("Grafik oder VisiCutRust-Projekt öffnen"))
            .add_filter(
                tr("Grafik oder VisiCutRust-Projekt"),
                &[import::EXTENSIONS, &["vcr", "json"]].concat(),
            )
            .add_filter(tr("Grafik"), import::EXTENSIONS)
            .add_filter(tr("VisiCutRust-Projekt"), &["vcr", "json"])
            .pick_file()
        {
            self.request(ctx, Replace::Open(path));
        }
    }

    /// Schreibt das Projekt als PLF für Java-VisiCut; die Hinweise nennen,
    /// was das Format nicht enthält.
    fn export_plf(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title(tr("Als VisiCut-Projekt (PLF) exportieren"))
            .add_filter(tr("VisiCut-Projekt"), &["plf"])
            .set_file_name(format!("{}.plf", self.project.name))
            .save_file()
        else {
            return;
        };
        match visicut_core::import::write_plf(&self.project, &path) {
            Ok(notes) => {
                self.status = tr("Als VisiCut-Projekt exportiert").into();
                self.alert = Some(notes.join("\n"));
            }
            Err(e) => self.report(e),
        }
    }

    fn export(&mut self) {
        if !self.sendable {
            return;
        }
        let single_job = self.preview.single_job;
        let prepared = match ltt::require_material(&self.project)
            .and_then(|()| ltt::prepare_with(&self.project, single_job))
        {
            Ok(prepared) => prepared,
            Err(e) => return self.report(e),
        };
        let mut jobs = prepared.jobs;
        if jobs.len() == 1 {
            let job = jobs.remove(0);
            let Some(path) = rfd::FileDialog::new()
                .set_title(tr("LTT-Auftrag exportieren"))
                .add_filter(tr("LTT-Auftrag"), &["ltt"])
                .set_file_name(format!("{}.ltt", job.name))
                .save_file()
            else {
                return;
            };
            match workspace::write_atomic(&path, &job.bytes) {
                Ok(()) => self.status = tr!("Exportiert: {files}", files = path.display()),
                Err(e) => self.report(e),
            }
            return;
        }
        let Some(folder) = rfd::FileDialog::new()
            .set_title(tr!(
                "Ordner für {count} getrennte LTT-Aufträge wählen",
                count = jobs.len()
            ))
            .pick_folder()
        else {
            return;
        };
        let plan = workspace::export_plan(&folder, jobs.iter().map(|job| job.name.as_str()));
        let pending = PendingExport {
            targets: plan.targets,
            existing: plan.existing,
            jobs,
        };
        if pending.existing.is_empty() {
            self.write_export(pending);
        } else {
            self.overwrite = Some(pending);
        }
    }

    fn write_export(&mut self, pending: PendingExport) {
        for (path, job) in pending.targets.iter().zip(&pending.jobs) {
            if let Err(e) = workspace::write_atomic(path, &job.bytes) {
                return self.report(e);
            }
        }
        self.status = tr!(
            "Exportiert: {files}",
            files = pending
                .jobs
                .iter()
                .map(|job| job.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    fn send(&mut self) {
        if !self.sendable {
            return;
        }
        let single_job = self.preview.single_job;
        match ltt::require_material(&self.project)
            .and_then(|()| ltt::prepare_with(&self.project, single_job))
        {
            Ok(prepared) => self.confirm_send = Some(prepared),
            Err(e) => self.report(e),
        }
    }

    fn transmit(&mut self, prepared: PreparedJob) {
        let host = self.project.hostname.clone();
        let port = self.project.port;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(ltt::transmit_jobs(&host, port, &prepared.jobs));
        });
        self.send_result = Some(rx);
        self.status = tr("Job wird an den Lasercutter übertragen …").into();
    }

    fn show_settings(&mut self, page: SettingsPage) {
        self.page = Page::Settings;
        self.settings_page = page;
    }

    /// Reiterwechsel; „Vorschau“ berechnet einen veralteten Auftrag neu.
    fn switch_page(&mut self, ctx: &egui::Context, page: Page) {
        self.page = page;
        if page == Page::Preview && self.preview.outdated() && !self.project.svg.is_empty() {
            self.preview.request(ctx, &self.project, &mut self.status);
        }
    }

    /// Dunkle Leiste wie in Bambu Studio: Öffnen und Sichern, die Reiter
    /// „Vorbereiten · Vorschau · Einstellungen“, rechts Berechnen und Senden.
    fn top_bar(&mut self, ui: &mut egui::Ui) -> Option<Command> {
        let mut clicked = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for (command, icon) in [(Command::Open, "📂"), (Command::Save, "💾")] {
                let hover = command.title().trim_end_matches(" …").to_owned();
                if form::top_button(ui, self.enabled(command), icon)
                    .on_hover_text(hover)
                    .clicked()
                {
                    clicked = Some(command);
                }
            }
            ui.add_space(12.0);
            for (page, title) in [
                (Page::Prepare, format!("✂  {}", tr("Vorbereiten"))),
                (Page::Preview, format!("👁  {}", tr("Vorschau"))),
                (Page::Settings, format!("⚙  {}", tr("Einstellungen"))),
            ] {
                if form::top_tab(ui, self.page == page, &title).clicked() {
                    let ctx = ui.ctx().clone();
                    self.switch_page(&ctx, page);
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.add_space(4.0);
                if ui
                    .add_enabled(
                        self.enabled(Command::Send),
                        form::primary_button(&format!("✈ {}", tr("Senden"))),
                    )
                    .on_hover_text(Command::Send.title().trim_end_matches(" …"))
                    .clicked()
                {
                    clicked = Some(Command::Send);
                }
                if ui
                    .add_enabled(
                        !self.project.svg.is_empty(),
                        egui::Button::new(tr("Vorschau & Zeit berechnen"))
                            .min_size(Vec2::new(0.0, 28.0)),
                    )
                    .on_hover_text(tr(
                        "Auftragsvorschau, Zeitschätzung und Simulation (Strg/⌘+Umschalt+P)",
                    ))
                    .clicked()
                {
                    clicked = Some(Command::Preview);
                }
            });
        });
        clicked
    }

    /// Reiter „Einstellungen“: Bereiche links, Inhalt rechts.
    fn settings_tab(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("settings_nav")
            .resizable(false)
            .exact_width(200.0)
            .frame(
                egui::Frame::side_top_panel(&ctx.style())
                    .fill(form::SIDEBAR)
                    .inner_margin(egui::Margin::symmetric(8, 10)),
            )
            .show(ctx, |ui| {
                for (page, title) in [
                    (SettingsPage::Devices, tr("Lasercutter")),
                    (SettingsPage::Materials, tr("Materialien")),
                    (SettingsPage::General, tr("Allgemein")),
                ] {
                    let text = egui::RichText::new(title).size(14.0);
                    if ui
                        .add_sized(
                            [ui.available_width(), 30.0],
                            egui::Button::selectable(self.settings_page == page, text),
                        )
                        .clicked()
                    {
                        self.settings_page = page;
                    }
                }
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(&ctx.style()).inner_margin(16.0))
            .show(ctx, |ui| match self.settings_page {
                SettingsPage::Devices => {
                    ui.heading(tr("Lasercutter"));
                    ui.add_space(6.0);
                    self.devices
                        .settings_page(ui, &mut self.project, &mut self.status);
                }
                SettingsPage::Materials => {
                    ui.heading(tr("Materialbibliothek"));
                    ui.add_space(6.0);
                    self.jobs.library_page(ui, &mut self.status);
                }
                SettingsPage::General => self.general_page(ui),
            });
    }

    /// Einstellungen → Allgemein: Update-Prüfung und Geräteliste übertragen.
    fn general_page(&mut self, ui: &mut egui::Ui) {
        ui.heading(tr("Allgemein"));
        ui.add_space(6.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.set_max_width(620.0);
            form::section(ui, tr("Updates"), |ui| {
                let mut auto = !self.settings.update_check_disabled;
                if ui
                    .checkbox(&mut auto, tr("Alle 14 Tage nach Updates suchen"))
                    .on_hover_text(tr("Fragt bei GitHub nach einem neuen Release von VisiCutRust; es wird nichts automatisch installiert."))
                    .changed()
                {
                    self.settings.update_check_disabled = !auto;
                    self.save_settings();
                }
                form::row(ui, tr("Installiert"), |ui| {
                    ui.label(visicut_core::update::current_version())
                });
                let last = self.settings.last_update_check;
                let days = visicut_core::update::now_secs().saturating_sub(last) / 86_400;
                form::row(ui, tr("Zuletzt geprüft"), |ui| {
                    ui.label(match (last, days) {
                        (0, _) => tr("noch nie").to_owned(),
                        (_, 0) => tr("heute").to_owned(),
                        (_, days) => tr!("vor {days} Tagen", days = days),
                    })
                });
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            self.update_check.is_none(),
                            egui::Button::new(tr("Jetzt nach Updates suchen")),
                        )
                        .clicked()
                    {
                        self.start_update_check(true);
                    }
                    if self.update_check.is_some() {
                        ui.spinner();
                    }
                });
            });
            form::section(ui, tr("Lasercutter"), |ui| {
                self.devices
                    .transfer(ui, &mut self.project, &mut self.status);
                form::caption(ui, tr("Importieren liest VisiCut-Gerätedateien, .vcsettings-Archive und eigene Exporte; Herunterladen holt die Einstellungen eines Labors. Neue Geräte werden sofort gesichert."));
            });
        });
    }

    /// Kopf der Seitenleiste „Vorbereiten“: aktiver Lasercutter wie die
    /// Druckerkarte in Bambu Studio.
    fn device_card(&mut self, ui: &mut egui::Ui) -> Option<Command> {
        let mut clicked = None;
        form::panel_header(ui, tr("Lasercutter"), |ui| {
            if ui
                .small_button("⚙")
                .on_hover_text(tr("Lasercutter einrichten"))
                .clicked()
            {
                clicked = Some(Command::Devices);
            }
        });
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(10, 4))
            .show(ui, |ui| {
                form::card(ui, |ui| {
                    let current = self.devices.store.selected;
                    let names: Vec<String> = self
                        .devices
                        .store
                        .devices
                        .iter()
                        .map(|d| d.name.clone())
                        .collect();
                    form::Dropdown::from_id_salt("active_device")
                        .width(ui.available_width() - 8.0)
                        .selected_text(egui::RichText::new(&names[current]).strong())
                        .show_ui(ui, |ui| {
                            for (index, name) in names.iter().enumerate() {
                                if ui.selectable_label(index == current, name).clicked() {
                                    clicked = Some(Command::Device(index));
                                }
                            }
                        });
                    let device = self.devices.device();
                    form::caption(
                        ui,
                        format!(
                            "{}:{} · {} × {} mm",
                            device.hostname,
                            device.port,
                            self.project.bed_width_mm,
                            self.project.bed_height_mm
                        ),
                    );
                });
            });
        clicked
    }

    fn enabled(&self, command: Command) -> bool {
        match command {
            Command::New
            | Command::Open
            | Command::AddPart
            | Command::Demo
            | Command::Vectorize
            | Command::Devices => !self.busy(),
            Command::Export | Command::Send | Command::Preview => self.sendable,
            Command::Text | Command::Shape | Command::Sign => !self.busy(),
            Command::Array
            | Command::Nest
            | Command::Delete
            | Command::Duplicate
            | Command::DuplicatePart
            | Command::RotateLeft
            | Command::RotateRight
            | Command::RotateBy
            | Command::FlipHorizontal
            | Command::FlipVertical => self.texture.is_some() && !self.busy(),
            Command::OpenRecent(_) => !self.busy(),
            Command::ClearRecent => !self.settings.recent.is_empty(),
            Command::ClearCache => !session::states(&session::dir()).is_empty(),
            Command::Language(_) => true,
            Command::CutOrder(_) | Command::Device(_) => !self.busy(),
            Command::Undo => self.undo.is_some() && !self.busy(),
            Command::Center | Command::ExportPlf => self.texture.is_some() && !self.busy(),
            Command::ToggleCamera => self.devices.has_camera(),
            Command::RefreshCamera => self.devices.camera_shown() && !self.devices.camera_loading(),
            Command::ZoomIn => self.zoom < workspace::ZOOM_MAX,
            Command::ZoomOut => self.zoom > workspace::ZOOM_MIN,
            Command::Save
            | Command::SaveAs
            | Command::Quit
            | Command::ToggleGrid
            | Command::ZoomFit
            | Command::Materials => true,
        }
    }

    fn run(&mut self, ctx: &egui::Context, command: Command) {
        if !self.enabled(command) {
            return;
        }
        match command {
            Command::New => self.request(ctx, Replace::New),
            Command::Open => self.choose_file(ctx),
            Command::AddPart => self.choose_parts(ctx),
            Command::Demo => self.request(ctx, Replace::Demo),
            Command::Vectorize => self.vectorize.pick(&mut self.status),
            Command::Save | Command::SaveAs => {
                if let Err(e) = self.save(command == Command::SaveAs) {
                    self.report(e);
                }
            }
            Command::Export => self.export(),
            Command::ExportPlf => self.export_plf(),
            Command::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Command::Center => {
                workspace::center(&mut self.project);
                self.dirty = true;
            }
            Command::Devices => self.show_settings(SettingsPage::Devices),
            Command::ToggleGrid => self.show_grid = !self.show_grid,
            Command::ToggleCamera => self.devices.toggle_camera(&self.project),
            Command::RefreshCamera => self.devices.refresh_camera(&self.project),
            Command::ZoomIn => self.zoom = workspace::zoom_in(self.zoom),
            Command::ZoomOut => self.zoom = workspace::zoom_out(self.zoom),
            Command::ZoomFit => self.zoom = 1.0,
            Command::Materials => self.show_settings(SettingsPage::Materials),
            Command::Preview => self.show_preview = true,
            Command::Send => self.send(),
            Command::Text => self.compose.open(compose_ui::Tool::Text, &self.project),
            Command::Shape => self.compose.open(compose_ui::Tool::Shape, &self.project),
            Command::Sign => self.compose.open(compose_ui::Tool::Sign, &self.project),
            Command::Array => self.compose.open(compose_ui::Tool::Array, &self.project),
            Command::Nest => self.compose.open(compose_ui::Tool::Nest, &self.project),
            Command::Undo => self.undo_last(),
            Command::Delete => self.delete_part(),
            Command::DuplicatePart => self.duplicate_part(),
            Command::Duplicate => {
                let result = visicut_core::compose::duplicate(&self.project, 5.0);
                self.composed(ctx, result);
            }
            Command::RotateLeft => self.transform(ctx, -90.0, false, false),
            Command::RotateRight => self.transform(ctx, 90.0, false, false),
            Command::RotateBy => {
                let angle = std::mem::take(&mut self.rotate_deg) as f64;
                self.transform(ctx, angle, false, false);
            }
            Command::FlipHorizontal => self.transform(ctx, 0.0, true, false),
            Command::FlipVertical => self.transform(ctx, 0.0, false, true),
            Command::OpenRecent(index) => {
                if let Some(path) = self.settings.recent.get(index).cloned() {
                    self.request(ctx, Replace::Open(path));
                }
            }
            Command::ClearRecent => {
                self.settings.recent.clear();
                self.save_settings();
            }
            Command::ClearCache => {
                self.confirm_clear_cache = Some(session::states(&session::dir()).len());
            }
            Command::Device(index) => {
                if self
                    .devices
                    .select(index, &mut self.project, &mut self.status)
                {
                    self.dirty = true;
                }
            }
            Command::CutOrder(order) => {
                if self.project.cut_order != order {
                    self.project.cut_order = order;
                    self.dirty = true;
                }
            }
            Command::Language(language) => {
                self.settings.language = language.map(|l| l.code().to_owned());
                i18n::set_language(self.settings.language());
                self.save_settings();
            }
        }
    }

    fn keyboard(&mut self, ctx: &egui::Context) {
        if self.modal_open() {
            return;
        }
        let mut commands = Vec::new();
        ctx.input_mut(|i| {
            for command in Command::KEYBOARD {
                if command
                    .shortcut()
                    .is_some_and(|shortcut| i.consume_shortcut(&shortcut))
                {
                    commands.push(command);
                }
            }
            // „+“ liegt auf vielen Tastaturen auf der Taste mit „=“.
            if i.consume_key(Modifiers::COMMAND, Key::Equals) {
                commands.push(Command::ZoomIn);
            }
        });
        for command in commands {
            self.run(ctx, command);
        }
        if self.busy() || ctx.memory(|m| m.focused().is_some()) {
            return;
        }
        // Widerrufen, Löschen und Duplizieren nur ohne aktives Eingabefeld,
        // damit diese Tasten dort weiter Text bearbeiten.
        let editing = ctx.input_mut(|i| {
            let mut commands = Vec::new();
            if i.consume_key(Modifiers::COMMAND, Key::Z) {
                commands.push(Command::Undo);
            }
            if i.consume_key(Modifiers::COMMAND, Key::D) {
                commands.push(Command::Duplicate);
            }
            for modifiers in [Modifiers::NONE, Modifiers::COMMAND] {
                if i.consume_key(modifiers, Key::Delete) || i.consume_key(modifiers, Key::Backspace)
                {
                    commands.push(Command::Delete);
                }
            }
            commands
        });
        for command in editing {
            self.run(ctx, command);
        }
        // Pfeiltasten verschieben das Motiv, solange kein Eingabefeld aktiv ist.
        if self.texture.is_none() {
            return;
        }
        let (dx, dy) = ctx.input_mut(|i| {
            let step = if i.modifiers.shift { 10.0 } else { 1.0 };
            let mut delta = (0.0, 0.0);
            for (key, dx, dy) in [
                (Key::ArrowLeft, -step, 0.0),
                (Key::ArrowRight, step, 0.0),
                (Key::ArrowUp, 0.0, -step),
                (Key::ArrowDown, 0.0, step),
            ] {
                let presses = i.count_and_consume_key(Modifiers::SHIFT, key)
                    + i.count_and_consume_key(Modifiers::NONE, key);
                delta.0 += dx * presses as f32;
                delta.1 += dy * presses as f32;
            }
            delta
        });
        if (dx, dy) != (0.0, 0.0) && workspace::nudge(&mut self.project, dx, dy) {
            self.dirty = true;
        }
    }

    fn menu_item(&self, ui: &mut egui::Ui, command: Command, clicked: &mut Option<Command>) {
        let mut button = egui::Button::new(command.title());
        if let Some(shortcut) = command.shortcut() {
            button = button.shortcut_text(ui.ctx().format_shortcut(&shortcut));
        }
        let checked = match command {
            Command::ToggleGrid => Some(self.show_grid),
            Command::ToggleCamera => Some(self.devices.camera_shown()),
            _ => None,
        };
        if let Some(checked) = checked {
            button = button.selected(checked);
        }
        if ui.add_enabled(self.enabled(command), button).clicked() {
            *clicked = Some(command);
        }
    }

    fn menu_bar(&self, ui: &mut egui::Ui) -> Option<Command> {
        let mut clicked = None;
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button(tr("Datei"), |ui| {
                for command in [
                    Command::New,
                    Command::Open,
                    Command::AddPart,
                    Command::Demo,
                    Command::Vectorize,
                ] {
                    self.menu_item(ui, command, &mut clicked);
                }
                ui.menu_button(tr("Zuletzt geöffnet"), |ui| {
                    if self.settings.recent.is_empty() {
                        ui.add_enabled(false, egui::Button::new(tr("Keine Dateien")));
                    }
                    for (index, path) in self.settings.recent.iter().enumerate() {
                        let command = Command::OpenRecent(index);
                        let name = path.file_name().map_or_else(
                            || path.display().to_string(),
                            |n| n.to_string_lossy().into_owned(),
                        );
                        if ui
                            .add_enabled(self.enabled(command), egui::Button::new(name))
                            .on_hover_text(path.display().to_string())
                            .clicked()
                        {
                            clicked = Some(command);
                        }
                    }
                    ui.separator();
                    self.menu_item(ui, Command::ClearRecent, &mut clicked);
                });
                self.menu_item(ui, Command::ClearCache, &mut clicked);
                ui.separator();
                // Aktiver Lasercutter und Einstellungen statt einer Auswahl in
                // der Seitenleiste.
                ui.menu_button(tr("Lasercutter"), |ui| {
                    for (index, device) in self.devices.store.devices.iter().enumerate() {
                        let command = Command::Device(index);
                        if ui
                            .add_enabled(
                                self.enabled(command),
                                egui::Button::selectable(
                                    index == self.devices.store.selected,
                                    &device.name,
                                ),
                            )
                            .on_hover_text(format!("{}:{}", device.hostname, device.port))
                            .clicked()
                        {
                            clicked = Some(command);
                        }
                    }
                });
                self.menu_item(ui, Command::Devices, &mut clicked);
                ui.separator();
                for command in [
                    Command::Save,
                    Command::SaveAs,
                    Command::Export,
                    Command::ExportPlf,
                ] {
                    self.menu_item(ui, command, &mut clicked);
                }
                ui.separator();
                self.menu_item(ui, Command::Quit, &mut clicked);
            });
            ui.menu_button(tr("Bearbeiten"), |ui| {
                self.menu_item(ui, Command::Undo, &mut clicked);
                ui.separator();
                self.menu_item(ui, Command::Duplicate, &mut clicked);
                self.menu_item(ui, Command::DuplicatePart, &mut clicked);
                self.menu_item(ui, Command::Delete, &mut clicked);
                ui.separator();
                for command in [
                    Command::RotateLeft,
                    Command::RotateRight,
                    Command::FlipHorizontal,
                    Command::FlipVertical,
                ] {
                    self.menu_item(ui, command, &mut clicked);
                }
                ui.separator();
                self.menu_item(ui, Command::Center, &mut clicked);
                ui.add_enabled(
                    false,
                    egui::Button::new(tr("Motiv verschieben")).shortcut_text(tr("Pfeiltasten")),
                )
                .on_disabled_hover_text(tr("1 mm je Tastendruck, mit Umschalt 10 mm"));
            });
            ui.menu_button(tr("Objekt"), |ui| {
                for command in [Command::Text, Command::Shape, Command::Sign] {
                    self.menu_item(ui, command, &mut clicked);
                }
                ui.separator();
                for command in [Command::Array, Command::Nest] {
                    self.menu_item(ui, command, &mut clicked);
                }
            });
            ui.menu_button(tr("Darstellung"), |ui| {
                for command in [Command::ZoomIn, Command::ZoomOut, Command::ZoomFit] {
                    self.menu_item(ui, command, &mut clicked);
                }
                self.menu_item(ui, Command::ToggleGrid, &mut clicked);
                ui.separator();
                self.menu_item(ui, Command::ToggleCamera, &mut clicked);
                self.menu_item(ui, Command::RefreshCamera, &mut clicked);
                ui.separator();
                ui.menu_button(tr("Sprache"), |ui| {
                    let chosen = self
                        .settings
                        .language
                        .as_deref()
                        .and_then(i18n::Language::from_code);
                    if ui
                        .add(egui::Button::new(tr("Systemsprache")).selected(chosen.is_none()))
                        .clicked()
                    {
                        clicked = Some(Command::Language(None));
                    }
                    ui.separator();
                    for language in i18n::Language::ALL {
                        let button = egui::Button::new(language.native_name())
                            .selected(chosen == Some(language));
                        if ui.add(button).clicked() {
                            clicked = Some(Command::Language(Some(language)));
                        }
                    }
                });
            });
            ui.menu_button(tr("Job"), |ui| {
                for command in [Command::Materials, Command::Preview] {
                    self.menu_item(ui, command, &mut clicked);
                }
                ui.separator();
                // Reihenfolge der Schnitt- und Markierpfade.
                ui.menu_button(tr("Schnittreihenfolge"), |ui| {
                    for order in CutOrder::ALL {
                        // Darunter VisiCuts weitere Reihenfolgen.
                        if order == CutOrder::File {
                            ui.separator();
                        }
                        let command = Command::CutOrder(order);
                        if ui
                            .add_enabled(
                                self.enabled(command),
                                egui::Button::selectable(
                                    self.project.cut_order == order,
                                    command.title(),
                                ),
                            )
                            .on_hover_text(jobs_ui::cut_order_help(order))
                            .clicked()
                        {
                            clicked = Some(command);
                        }
                    }
                });
                ui.separator();
                self.menu_item(ui, Command::Send, &mut clicked);
            });
        });
        clicked
    }

    /// Seitenleiste in der Reihenfolge der macOS-Oberfläche: Projekt,
    /// Rotationseinheit, Material, Zuordnung und Bearbeitung, Position. Die
    /// Auftragsbefehle stehen in der Werkzeugleiste.
    fn sidebar(&mut self, ui: &mut egui::Ui) -> Option<Command> {
        let mut clicked = None;
        if self.project.has_artwork() {
            form::section(ui, tr("Teile"), |ui| self.parts_panel(ui, &mut clicked));
        }
        if self.devices.device().rotary_axis {
            form::section(ui, tr("Rotationseinheit"), |ui| {
                self.devices.rotary(ui, &mut self.project, &mut self.dirty);
            });
        }
        form::section(ui, tr("Material"), |ui| {
            self.jobs.material(ui, &mut self.project, &mut self.dirty);
        });
        self.jobs.processing(ui, &mut self.project, &mut self.dirty);
        form::section(ui, tr("Position und Größe"), |ui| {
            self.transform_panel(ui, &mut clicked);
        });
        ui.add_space(8.0);
        clicked
    }

    /// Liste der Teile wie VisiCuts Objektliste: Klick wählt das Teil, dessen
    /// Lage und Zuordnung die Abschnitte darunter bearbeiten.
    fn parts_panel(&mut self, ui: &mut egui::Ui, clicked: &mut Option<Command>) {
        let mut select = None;
        for index in 0..self.project.part_count() {
            let (name, width, height) = part_summary(&self.project, index);
            let active = index == self.project.active_part;
            let title = egui::RichText::new(part_title(name, index));
            ui.horizontal(|ui| {
                let title = if active { title.strong() } else { title };
                if ui.selectable_label(active, title).clicked() {
                    select = Some(index);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.weak(format!("{width:.0} × {height:.0} mm"));
                });
            });
        }
        if let Some(index) = select {
            self.select_part(index);
        }
        ui.add_space(2.0);
        // Umbrechen statt die Seitenleiste zu verbreitern (längere Übersetzungen).
        ui.horizontal_wrapped(|ui| {
            for (command, text, hover) in [
                (
                    Command::AddPart,
                    tr("Datei hinzufügen …"),
                    Command::AddPart.title(),
                ),
                (
                    Command::DuplicatePart,
                    tr("Duplizieren"),
                    Command::DuplicatePart.title(),
                ),
                (Command::Delete, tr("Entfernen"), tr("Teil entfernen")),
            ] {
                let mut hover = hover.trim_end_matches(" …").to_owned();
                if let Some(shortcut) = command.shortcut() {
                    hover += &format!(" ({})", ui.ctx().format_shortcut(&shortcut));
                }
                if ui
                    .add_enabled(self.enabled(command), egui::Button::new(text))
                    .on_hover_text(hover)
                    .clicked()
                {
                    *clicked = Some(command);
                }
            }
        });
    }

    /// Position und Größe wie in Grafikprogrammen: Bezugspunkt, X/Y des
    /// Bezugspunkts, Breite/Höhe um ihn herum, Drehen und Spiegeln.
    fn transform_panel(&mut self, ui: &mut egui::Ui, clicked: &mut Option<Command>) {
        let mut reference = self.reference;
        let mut dirty = false;
        let mut proportional = self.proportional;
        // Nullpunkt unten links: Y zählt wie am Gerät von der unteren
        // Bettkante nach oben (nur Anzeige, der Auftrag bleibt gleich).
        let bottom = self.devices.origin_bottom_left();
        let p = &mut self.project;
        // Wie das Transformieren-Feld in Grafikprogrammen: Bezugspunkt, X/Y,
        // B/H und rechts die Kette, die das Seitenverhältnis hält.
        let (mut new_x, mut new_y, mut new_w, mut new_h) = (None, None, None, None);
        // Feste Breite wie die Karten darüber: Felder teilen den Rest unter sich auf.
        let total = ui.available_width();
        ui.horizontal(|ui| {
            ui.set_max_width(total);
            reference_picker(ui, &mut reference);
            let [fx, fy] = reference;
            let mut x = p.x_mm + p.width_mm * fx;
            let top = p.y_mm + p.height_mm * fy;
            let mut y = if bottom { p.bed_height_mm - top } else { top };
            let (mut w, mut h) = (p.width_mm, p.height_mm);
            fn field(value: &mut f32, min: f32) -> egui::DragValue<'_> {
                egui::DragValue::new(value)
                    .speed(0.1)
                    .range(min..=10000.0)
                    .max_decimals(2)
                    .suffix(" mm")
            }
            let spacing = ui.spacing().item_spacing.x;
            let lock = 32.0;
            let labels = 14.0;
            let field_w =
                ((total - 40.0 - lock - 2.0 * labels - 7.0 * spacing) / 2.0).clamp(50.0, 120.0);
            egui::Grid::new("position_size")
                .num_columns(4)
                // Sonst sind die Beschriftungsspalten 40 px breit.
                .min_col_width(0.0)
                .spacing([4.0, 4.0])
                .show(ui, |ui| {
                    ui.label("X");
                    if ui
                        .add_sized([field_w, 20.0], field(&mut x, -10000.0))
                        .changed()
                    {
                        new_x = Some(x);
                    }
                    ui.label("B").on_hover_text(tr("Breite"));
                    if ui.add_sized([field_w, 20.0], field(&mut w, 0.1)).changed() {
                        new_w = Some(w);
                    }
                    ui.end_row();
                    ui.label("Y");
                    if ui
                        .add_sized([field_w, 20.0], field(&mut y, -10000.0))
                        .changed()
                    {
                        new_y = Some(y);
                    }
                    ui.label("H").on_hover_text(tr("Höhe"));
                    if ui.add_sized([field_w, 20.0], field(&mut h, 0.1)).changed() {
                        new_h = Some(h);
                    }
                    ui.end_row();
                });
            let tip = if proportional {
                tr("Seitenverhältnis beibehalten")
            } else {
                tr("Seitenverhältnis frei")
            };
            if ui
                .add_sized([lock, 20.0], egui::Button::selectable(proportional, "🔗"))
                .on_hover_text(tip)
                .clicked()
            {
                proportional = !proportional;
            }
        });
        let [fx, fy] = reference;
        if let Some(x) = new_x {
            p.x_mm = x - p.width_mm * fx;
            dirty = true;
        }
        if let Some(y) = new_y {
            let top = if bottom { p.bed_height_mm - y } else { y };
            p.y_mm = top - p.height_mm * fy;
            dirty = true;
        }
        let anchor = [p.x_mm + p.width_mm * fx, p.y_mm + p.height_mm * fy];
        let ratio = p.height_mm / p.width_mm;
        let mut resized = false;
        if let Some(w) = new_w {
            p.width_mm = w;
            if proportional {
                p.height_mm = w * ratio;
            }
            resized = true;
        }
        if let Some(h) = new_h {
            p.height_mm = h;
            if proportional {
                p.width_mm = h / ratio;
            }
            resized = true;
        }
        if resized {
            // Der Bezugspunkt bleibt stehen.
            p.x_mm = anchor[0] - p.width_mm * fx;
            p.y_mm = anchor[1] - p.height_mm * fy;
            dirty = true;
        }
        self.reference = reference;
        self.proportional = proportional;
        self.dirty |= dirty;
        let editable = self.texture.is_some() && !self.busy();
        // Drehen und Spiegeln nur als Symbole; die Tooltips nennen den Befehl.
        ui.horizontal(|ui| {
            ui.label("⟳").on_hover_text(tr("Drehen"));
            ui.add(
                egui::DragValue::new(&mut self.rotate_deg)
                    .range(-360.0..=360.0)
                    .speed(1.0)
                    .suffix("°"),
            )
            .on_hover_text(tr("Positive Winkel drehen im Uhrzeigersinn"));
            if ui
                .add_enabled(editable && self.rotate_deg != 0.0, egui::Button::new("✔"))
                .on_hover_text(tr("Anwenden"))
                .clicked()
            {
                *clicked = Some(Command::RotateBy);
            }
            ui.separator();
            for (command, text) in [
                (Command::RotateLeft, "⟲ 90°"),
                (Command::RotateRight, "⟳ 90°"),
                (Command::FlipHorizontal, "↔"),
                (Command::FlipVertical, "↕"),
            ] {
                if ui
                    .add_enabled(editable, egui::Button::new(text))
                    .on_hover_text(command.title())
                    .clicked()
                {
                    *clicked = Some(command);
                }
            }
        });
    }

    /// Rechnet die farbige Bearbeitung neu, wenn sich Motiv, Teile oder
    /// Zuordnung geändert haben; reines Verschieben verschiebt das Bild.
    fn refresh_live(&mut self, ctx: &egui::Context) {
        // Während des Ziehens an einem Anfasser wird das Bild nur gestreckt.
        if self.handle_drag.is_some() {
            return;
        }
        use std::hash::{Hash, Hasher};
        let p = &self.project;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        p.svg.hash(&mut hasher);
        let position = (!p.parts.is_empty()).then_some((p.x_mm, p.y_mm));
        // Texturgröße folgt der Anzeige (in groben Stufen, damit nicht jeder
        // Mausradschritt neu rechnet).
        let [_, _, bw, bh] = p.bounds();
        let shown = (bw.max(bh) * self.view_scale * ctx.pixels_per_point()).max(1.0);
        let shown = 2f32.powf((shown.log2() * 2.0).ceil() / 2.0);
        let max_px = (shown * 1.5).clamp(800.0, 4096.0);
        // Linien mindestens 1,5 Bildschirmpunkte breit, sonst verschwinden sie
        // beim Verkleinern der Textur.
        let min_line_px = 1.5 * max_px / shown;
        max_px.to_bits().hash(&mut hasher);
        serde_json::to_string(&(
            &p.steps,
            p.operation,
            &p.ignore_filters,
            &p.parts,
            p.active_part,
            p.width_mm,
            p.height_mm,
            position,
        ))
        .unwrap_or_default()
        .hash(&mut hasher);
        let key = hasher.finish();
        if self.live.as_ref().is_some_and(|(k, _)| *k == key) {
            return;
        }
        let texture = (self.texture.is_some())
            .then(|| ltt::processing_preview_sized(&self.project, max_px, min_line_px).ok())
            .flatten()
            .and_then(|png| resvg::tiny_skia::Pixmap::decode_png(&png).ok())
            .map(|pixmap| {
                let image = egui::ColorImage::from_rgba_premultiplied(
                    [pixmap.width() as usize, pixmap.height() as usize],
                    pixmap.data(),
                );
                ctx.load_texture("processing", image, egui::TextureOptions::LINEAR)
            });
        self.live = Some((key, texture));
    }

    fn canvas(&mut self, ui: &mut egui::Ui) -> Option<Command> {
        let mut clicked = None;
        self.refresh_live(ui.ctx());
        let panel = ui.max_rect();
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(20, 14))
            .show(ui, |ui| self.canvas_header(ui, &mut clicked));
        // Probleme mit der Lage des Motivs stehen rot über dem Arbeitsbett.
        if !self.project.svg.is_empty()
            && let Err(e) = self.project.validate()
        {
            egui::Frame::new()
                .fill(Color32::from_rgb(200, 40, 40))
                .inner_margin(egui::Margin::symmetric(20, 8))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.colored_label(Color32::WHITE, format!("⚠ {e}"));
                });
        }
        form::divider(ui);
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(20, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| self.preview.toolbar(ui, &self.project, &mut self.status));
            });
        form::divider(ui);
        ui.painter()
            .rect_filled(ui.available_rect_before_wrap(), 0.0, form::CANVAS);
        if self.texture.is_none() && !self.devices.camera_shown() {
            self.empty_state(ui, &mut clicked);
        } else {
            self.bed(ui, &mut clicked);
        }
        if ui.ctx().input(|i| !i.raw.hovered_files.is_empty()) {
            let painter = ui.ctx().layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("drop"),
            ));
            painter.rect_filled(
                panel,
                6.0,
                Color32::from_rgba_unmultiplied(52, 116, 170, 60),
            );
            painter.text(
                panel.center(),
                egui::Align2::CENTER_CENTER,
                if self.project.svg.is_empty() {
                    tr("Loslassen zum Öffnen")
                } else {
                    tr("Loslassen zum Hinzufügen")
                },
                egui::FontId::proportional(26.0),
                Color32::from_rgb(30, 70, 110),
            );
        }
        clicked
    }

    /// Titel, Gerät und Ansichtsschalter über dem Arbeitsbett.
    fn canvas_header(&mut self, ui: &mut egui::Ui, clicked: &mut Option<Command>) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                // Projektname: Doppelklick zum Umbenennen, ⏎ oder Klick
                // daneben übernimmt, Esc bricht ab.
                if let Some(draft) = &mut self.name_draft {
                    let field = ui.add(
                        egui::TextEdit::singleline(draft)
                            .font(egui::FontId::proportional(20.0))
                            .desired_width(320.0),
                    );
                    field.request_focus();
                    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        self.name_draft = None;
                    } else if field.lost_focus() {
                        let name = draft.trim().to_owned();
                        if !name.is_empty() && name != self.project.name {
                            self.project.name = name;
                            self.dirty = true;
                        }
                        self.name_draft = None;
                    }
                } else if self.texture.is_some() {
                    let title = ui
                        .add(
                            egui::Label::new(
                                egui::RichText::new(self.project.name.as_str())
                                    .size(20.0)
                                    .strong(),
                            )
                            .sense(egui::Sense::click()),
                        )
                        .on_hover_text(tr("Doppelklick zum Umbenennen"));
                    if title.double_clicked() {
                        self.name_draft = Some(self.project.name.clone());
                    }
                } else {
                    ui.label(
                        egui::RichText::new(tr("Arbeitsbereich"))
                            .size(20.0)
                            .strong(),
                    );
                }
                ui.weak(format!(
                    "{} · {} × {} mm{}",
                    self.devices.device().name,
                    self.project.bed_width_mm,
                    self.project.bed_height_mm,
                    if self.project.rotary_axis {
                        format!(" · {}", tr("Rotationseinheit"))
                    } else {
                        String::new()
                    }
                ));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let tool = |ui: &mut egui::Ui, command: Command, text: &str| {
                    let mut hover = command.title().to_owned();
                    if let Some(shortcut) = command.shortcut() {
                        hover += &format!(" ({})", ui.ctx().format_shortcut(&shortcut));
                    }
                    ui.add_enabled(self.enabled(command), egui::Button::new(text))
                        .on_hover_text(hover)
                        .clicked()
                        .then_some(command)
                };
                let mut chosen = Vec::new();
                chosen.extend(tool(ui, Command::ZoomFit, tr("Zurücksetzen")));
                chosen.extend(tool(ui, Command::ZoomIn, "+"));
                ui.label(format!("{:.0} %", self.zoom * 100.0));
                chosen.extend(tool(ui, Command::ZoomOut, "−"));
                ui.add_space(6.0);
                // Umschalter wie die Symbolschalter unter macOS.
                let toggle = |ui: &mut egui::Ui, command: Command, text: &str, on: bool| {
                    ui.add_enabled(self.enabled(command), egui::Button::new(text).selected(on))
                        .on_hover_text(command.title())
                        .clicked()
                        .then_some(command)
                };
                chosen.extend(toggle(
                    ui,
                    Command::ToggleGrid,
                    tr("Raster"),
                    self.show_grid,
                ));
                if self.devices.has_camera() {
                    if self.devices.camera_loading() {
                        ui.spinner();
                    } else if self.devices.camera_shown() {
                        chosen.extend(tool(ui, Command::RefreshCamera, "⟳"));
                    }
                    let shown = self.devices.camera_shown();
                    let camera = format!("📷 {}", tr("Kamera"));
                    chosen.extend(toggle(ui, Command::ToggleCamera, &camera, shown));
                }
                if let Some(command) = chosen.pop() {
                    *clicked = Some(command);
                }
            });
        });
    }

    /// Wie unter macOS: ohne Motiv ein Einstieg statt des leeren Betts.
    fn empty_state(&mut self, ui: &mut egui::Ui, clicked: &mut Option<Command>) {
        let open = ui
            .ctx()
            .format_shortcut(&Command::Open.shortcut().expect("Öffnen hat ein Kürzel"));
        ui.vertical_centered(|ui| {
            ui.add_space(((ui.available_height() - 250.0) / 2.0).max(24.0));
            ui.label(
                egui::RichText::new("📥")
                    .size(44.0)
                    .color(Color32::from_gray(140)),
            );
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(tr("Mit einer SVG beginnen"))
                    .size(20.0)
                    .strong(),
            );
            ui.add_space(4.0);
            ui.weak(tr(
                "Importiere dein Motiv, wähle ein Material\nund bereite den Job für den Lasercutter vor.",
            ));
            ui.add_space(12.0);
            if form::prominent(ui, self.enabled(Command::Open), tr("SVG importieren …")) {
                *clicked = Some(Command::Open);
            }
            ui.add_space(4.0);
            for command in [Command::Demo, Command::Vectorize] {
                if ui
                    .add_enabled(self.enabled(command), egui::Link::new(command.title()))
                    .clicked()
                {
                    *clicked = Some(command);
                }
            }
            ui.add_space(8.0);
            ui.small(tr!(
                "Auch PDF, DXF, EPS, Bilder und G-Code · {shortcut} · Dateien hierher ziehen",
                shortcut = open
            ))
            .on_hover_text(tr(
                "Mehrere Dateien auf einmal ablegen fügt sie nebeneinander ein",
            ));
        });
    }

    /// Arbeitsbett mit Linealen, Raster, Kamerabild und Motiv.
    fn bed(&mut self, ui: &mut egui::Ui, clicked: &mut Option<Command>) {
        // Probleme mit der Lage des Motivs stehen rot über dem Arbeitsbett.
        const RULER: f32 = 30.0;
        const MARGIN: f32 = 14.0;
        let bed_mm = Vec2::new(self.project.bed_width_mm, self.project.bed_height_mm);
        let available = ui.available_size() - Vec2::splat(RULER + MARGIN);
        // Mausrad zoomt die Arbeitsfläche (nicht scrollen), solange der Zeiger darüber steht.
        if ui.rect_contains_pointer(ui.max_rect()) {
            let wheel = ui.input(|i| i.raw_scroll_delta.y);
            if wheel != 0.0 {
                self.zoom = (self.zoom * (wheel * 0.002).exp())
                    .clamp(workspace::ZOOM_MIN, workspace::ZOOM_MAX);
                ui.input_mut(|i| {
                    i.raw_scroll_delta = Vec2::ZERO;
                    i.smooth_scroll_delta = Vec2::ZERO;
                });
            }
        }
        let scale =
            workspace::fit_scale([available.x, available.y], [bed_mm.x, bed_mm.y]) * self.zoom;
        self.view_scale = scale;
        egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            let size = bed_mm * scale + Vec2::splat(RULER + MARGIN);
            let (rect, response) = ui.allocate_exact_size(size, Sense::click());
            if response.clicked()
                && let Some(id) = ui.memory(|m| m.focused())
            {
                ui.memory_mut(|m| m.surrender_focus(id));
            }
            let bed = Rect::from_min_size(rect.min + Vec2::splat(RULER), bed_mm * scale);
            self.rulers(ui.painter(), bed, scale);
            let painter = ui.painter().with_clip_rect(bed.intersect(ui.clip_rect()));
            painter.rect_filled(bed, 0.0, Color32::WHITE);
            let camera = self.devices.paint_camera(&painter, bed);
            if self.show_grid && !camera {
                let step = workspace::grid_step_mm(scale);
                // Linien an den Linealzahlen kräftiger.
                let labels = workspace::ruler_step_mm(scale);
                let line = |mm: f32| {
                    let major = ((mm / labels).round() * labels - mm).abs() < step * 0.01;
                    Stroke::new(0.5, Color32::from_gray(if major { 200 } else { 228 }))
                };
                for x in workspace::ruler_marks(bed_mm.x, step) {
                    let px = bed.left() + x * scale;
                    painter.line_segment(
                        [Pos2::new(px, bed.top()), Pos2::new(px, bed.bottom())],
                        line(x),
                    );
                }
                for y in workspace::ruler_marks(bed_mm.y, step) {
                    let py = self.ruler_y(bed, scale, y);
                    painter.line_segment(
                        [Pos2::new(bed.left(), py), Pos2::new(bed.right(), py)],
                        line(y),
                    );
                }
            }
            painter.rect_stroke(
                bed,
                0.0,
                Stroke::new(1.0, Color32::from_gray(145)),
                egui::StrokeKind::Inside,
            );
            if let Some(texture) = &self.texture {
                let to_screen = |[x, y, w, h]: [f32; 4]| {
                    Rect::from_min_size(bed.min + Vec2::new(x, y) * scale, Vec2::new(w, h) * scale)
                };
                let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
                // Bearbeitung in Farbe statt der Originalbilder, schon vor der
                // Auftragsvorschau.
                let live = self.live.as_ref().and_then(|(_, t)| t.as_ref());
                // Die übrigen Teile unter dem gewählten, mit dünnem grauem Rahmen.
                let others: Vec<(usize, Rect)> = self
                    .project
                    .parts
                    .iter()
                    .enumerate()
                    .map(|(k, part)| {
                        let index = k + usize::from(k >= self.project.active_part);
                        let rect = to_screen([part.x_mm, part.y_mm, part.width_mm, part.height_mm]);
                        match self.part_textures.get(k).and_then(Option::as_ref) {
                            _ if live.is_some() => {}
                            Some(texture) => {
                                painter.image(texture.id(), rect, uv, Color32::WHITE);
                            }
                            None => {
                                painter.rect_filled(rect, 0.0, Color32::from_gray(235));
                            }
                        }
                        (index, rect)
                    })
                    .collect();
                let motif = to_screen([
                    self.project.x_mm,
                    self.project.y_mm,
                    self.project.width_mm,
                    self.project.height_mm,
                ]);
                match live {
                    Some(live) => painter.image(
                        live.id(),
                        to_screen(self.project.bounds()),
                        uv,
                        Color32::WHITE,
                    ),
                    None => painter.image(texture.id(), motif, uv, Color32::WHITE),
                };
                // Die Auftragsvorschau umfasst alle Teile.
                self.preview
                    .overlay(&painter, to_screen(self.project.bounds()));
                let mut select = None;
                for (index, rect) in others {
                    painter.rect_stroke(
                        rect,
                        0.0,
                        Stroke::new(1.0, Color32::from_gray(150)),
                        egui::StrokeKind::Inside,
                    );
                    let (name, _, _) = part_summary(&self.project, index);
                    let response = ui
                        .interact(
                            rect.intersect(bed),
                            ui.id().with(("part", index)),
                            Sense::click(),
                        )
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text(part_title(name, index));
                    if response.clicked() {
                        select = Some(index);
                    }
                }
                if let Some(index) = select {
                    self.select_part(index);
                    return;
                }
                // Auswahl schwarz gestrichelt: Blau, Rot, Orange und Violett
                // stehen für Bearbeitungen und dürfen nicht verwechselt werden.
                let frame = motif.shrink(0.5);
                painter.extend(egui::Shape::dashed_line(
                    &[
                        frame.left_top(),
                        frame.right_top(),
                        frame.right_bottom(),
                        frame.left_bottom(),
                        frame.left_top(),
                    ],
                    Stroke::new(1.0, Color32::from_black_alpha(190)),
                    5.0,
                    3.0,
                ));
                use std::hash::{Hash, Hasher};
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                self.project.svg.hash(&mut hasher);
                self.project.width_mm.to_bits().hash(&mut hasher);
                self.project.height_mm.to_bits().hash(&mut hasher);
                let key = hasher.finish();
                // In „Einzeln“: Objekt unter der Maus auf der Arbeitsfläche;
                // seine Zeile in der Liste gilt als ausgewählt.
                let objects_mode =
                    jobs_ui::assignment_mode(&self.project) == jobs_ui::AssignmentMode::Objects;
                let pointer = ui
                    .input(|i| i.pointer.hover_pos())
                    .filter(|p| bed.contains(*p) && ui.rect_contains_pointer(bed));
                self.jobs.canvas_hover = match pointer {
                    Some(pos) if objects_mode && self.handle_drag.is_none() => {
                        if self.all_outlines.as_ref().is_none_or(|(k, _)| *k != key) {
                            let outlines = visicut_core::geometry::object_outlines(&self.project)
                                .unwrap_or_default();
                            self.all_outlines = Some((key, outlines));
                        }
                        let mm = [
                            f64::from((pos.x - bed.min.x) / scale - self.project.x_mm),
                            f64::from((pos.y - bed.min.y) / scale - self.project.y_mm),
                        ];
                        self.all_outlines.as_ref().and_then(|(_, outlines)| {
                            visicut_core::geometry::object_at(outlines, mm, f64::from(4.0 / scale))
                        })
                    }
                    _ => None,
                };
                // Objekt unter der Maus (Liste oder Arbeitsfläche): gelb
                // (keine Bearbeitungsfarbe).
                if let Some(index) = self.jobs.hovered_object.take().or(self.jobs.canvas_hover) {
                    if !self
                        .hover_outline
                        .as_ref()
                        .is_some_and(|(k, i, _)| *k == key && *i == index)
                    {
                        let outline = visicut_core::geometry::object_outline(&self.project, index)
                            .unwrap_or_default();
                        self.hover_outline = Some((key, index, outline));
                    }
                    if let Some((_, _, outline)) = &self.hover_outline {
                        let origin = [self.project.x_mm as f64, self.project.y_mm as f64];
                        for contour in outline {
                            let points: Vec<Pos2> = contour
                                .iter()
                                .map(|p| {
                                    bed.min
                                        + Vec2::new(
                                            (origin[0] + p[0]) as f32,
                                            (origin[1] + p[1]) as f32,
                                        ) * scale
                                })
                                .collect();
                            painter.add(egui::Shape::line(
                                points.clone(),
                                Stroke::new(4.0, Color32::from_rgb(255, 204, 0)),
                            ));
                            painter.add(egui::Shape::line(
                                points,
                                Stroke::new(1.0, Color32::from_black_alpha(180)),
                            ));
                        }
                    }
                }
                let response = ui
                    .interact(
                        motif.intersect(bed),
                        ui.id().with("motif"),
                        Sense::click_and_drag(),
                    )
                    .on_hover_cursor(egui::CursorIcon::Grab);
                let editable = !self.busy();
                response.context_menu(|ui| {
                    for group in [
                        &[
                            Command::Duplicate,
                            Command::DuplicatePart,
                            Command::Array,
                            Command::Nest,
                        ][..],
                        &[
                            Command::RotateLeft,
                            Command::RotateRight,
                            Command::FlipHorizontal,
                            Command::FlipVertical,
                        ],
                        &[Command::Delete],
                    ] {
                        for command in group {
                            if ui
                                .add_enabled(editable, egui::Button::new(command.title()))
                                .clicked()
                            {
                                *clicked = Some(*command);
                            }
                        }
                        ui.separator();
                    }
                });
                if response.dragged() && !self.busy() {
                    let delta = response.drag_delta() / scale;
                    let (x, y) = (self.project.x_mm + delta.x, self.project.y_mm + delta.y);
                    if workspace::place(&mut self.project, x, y) {
                        self.dirty = true;
                    }
                }
                if !self.busy() {
                    self.handles(ui, motif, bed, scale, texture.id(), clicked);
                }
            } else {
                let open = ui
                    .ctx()
                    .format_shortcut(&Command::Open.shortcut().expect("Öffnen hat ein Kürzel"));
                painter.text(
                    bed.center(),
                    egui::Align2::CENTER_CENTER,
                    tr!(
                        "Datei öffnen ({shortcut}) oder hierher ziehen",
                        shortcut = open
                    ),
                    egui::FontId::proportional(22.0),
                    Color32::from_gray(145),
                );
            }
        });
    }

    /// Anfasser wie in VisiCut: Ecken und Kantenmitten skalieren (Kette im
    /// Feld B/H hält das Seitenverhältnis, Umschalt kehrt das um), der Kreis
    /// über der Mitte dreht (Umschalt rastet in 15°-Schritten ein).
    fn handles(
        &mut self,
        ui: &mut egui::Ui,
        motif: Rect,
        bed: Rect,
        scale: f32,
        texture: egui::TextureId,
        clicked: &mut Option<Command>,
    ) {
        use egui::CursorIcon;
        let shift = ui.input(|i| i.modifiers.shift);
        let painter = ui.painter();
        let to_mm = |p: Pos2| {
            let mm = (p - bed.min) / scale;
            [mm.x, mm.y]
        };
        let p = &self.project;
        let rect_mm = [p.x_mm, p.y_mm, p.width_mm, p.height_mm];
        let handle_stroke = Stroke::new(1.0, Color32::from_black_alpha(200));
        for (i, handle) in workspace::HANDLES.into_iter().enumerate() {
            let pos = motif.min + Vec2::new(handle[0], handle[1]) * motif.size();
            let cursor = match handle {
                [0.5, _] => CursorIcon::ResizeVertical,
                [_, 0.5] => CursorIcon::ResizeHorizontal,
                [x, y] if x == y => CursorIcon::ResizeNwSe,
                _ => CursorIcon::ResizeNeSw,
            };
            let response = ui
                .interact(
                    Rect::from_center_size(pos, Vec2::splat(14.0)),
                    ui.id().with(("handle", i)),
                    Sense::drag(),
                )
                .on_hover_cursor(cursor);
            if response.drag_started() {
                self.handle_drag = Some(HandleDrag::Resize(rect_mm));
            }
            if response.dragged()
                && let (Some(HandleDrag::Resize(start)), Some(pointer)) =
                    (self.handle_drag, response.interact_pointer_pos())
            {
                let proportional = self.proportional != shift;
                let [x, y, w, h] =
                    workspace::drag_handle(start, handle, to_mm(pointer), proportional);
                let p = &mut self.project;
                (p.x_mm, p.y_mm, p.width_mm, p.height_mm) = (x, y, w, h);
                self.dirty = true;
            }
            if response.drag_stopped() {
                self.handle_drag = None;
            }
            painter.rect(
                Rect::from_center_size(pos, Vec2::splat(7.0)),
                1.0,
                Color32::WHITE,
                handle_stroke,
                egui::StrokeKind::Middle,
            );
        }
        // Drehanfasser über der Mitte, bei wenig Platz darunter.
        let above = motif.top() - 22.0 >= bed.top();
        let (edge, knob) = if above {
            (
                motif.center_top(),
                motif.center_top() - Vec2::new(0.0, 22.0),
            )
        } else {
            (
                motif.center_bottom(),
                motif.center_bottom() + Vec2::new(0.0, 22.0),
            )
        };
        let response = ui
            .interact(
                Rect::from_center_size(knob, Vec2::splat(16.0)),
                ui.id().with("rotate"),
                Sense::drag(),
            )
            .on_hover_cursor(CursorIcon::Alias)
            .on_hover_text(tr("Ziehen dreht das Motiv; mit Umschalt in 15°-Schritten"));
        if response.drag_started()
            && let Some(start) = response.interact_pointer_pos()
        {
            self.handle_drag = Some(HandleDrag::Rotate {
                start,
                degrees: 0.0,
            });
        }
        if response.dragged()
            && let (Some(HandleDrag::Rotate { start, .. }), Some(pointer)) =
                (self.handle_drag, response.interact_pointer_pos())
        {
            let center = motif.center();
            let degrees = workspace::drag_rotation(
                [center.x, center.y],
                [start.x, start.y],
                [pointer.x, pointer.y],
                shift,
            );
            self.handle_drag = Some(HandleDrag::Rotate { start, degrees });
        }
        if response.drag_stopped() {
            if let Some(HandleDrag::Rotate { degrees, .. }) = self.handle_drag
                && degrees.abs() >= 0.1
            {
                // Dreht wie „Drehen“ im Inspektor um die Mitte.
                self.rotate_deg = degrees;
                *clicked = Some(Command::RotateBy);
            }
            self.handle_drag = None;
        }
        let painter = ui.painter();
        painter.line_segment([edge, knob], handle_stroke);
        painter.circle(knob, 4.5, Color32::WHITE, handle_stroke);
        // Vorschau der Drehung: Motiv und Rahmen gedreht um die Mitte.
        if let Some(HandleDrag::Rotate { degrees, .. }) = self.handle_drag {
            let angle = degrees.to_radians();
            egui::Image::new((texture, motif.size()))
                .rotate(angle, Vec2::splat(0.5))
                .tint(Color32::from_white_alpha(160))
                .paint_at(ui, motif);
            let rotation = egui::emath::Rot2::from_angle(angle);
            let center = motif.center();
            let corners: Vec<Pos2> = [
                motif.left_top(),
                motif.right_top(),
                motif.right_bottom(),
                motif.left_bottom(),
                motif.left_top(),
            ]
            .into_iter()
            .map(|c| center + rotation * (c - center))
            .collect();
            ui.painter().extend(egui::Shape::dashed_line(
                &corners,
                Stroke::new(1.0, Color32::from_black_alpha(190)),
                5.0,
                3.0,
            ));
            ui.painter().text(
                knob + Vec2::new(12.0, 0.0),
                egui::Align2::LEFT_CENTER,
                format!("{degrees:.0}°"),
                egui::FontId::proportional(12.0),
                Color32::from_gray(40),
            );
        }
    }

    /// Senkrechte Bildschirmposition einer Lineal- oder Rastermarke `y` mm;
    /// mit Nullpunkt unten links zählt sie von der unteren Bettkante.
    fn ruler_y(&self, bed: Rect, scale: f32, y: f32) -> f32 {
        if self.devices.origin_bottom_left() {
            bed.bottom() - y * scale
        } else {
            bed.top() + y * scale
        }
    }

    /// Lineale mit mm-Beschriftung oben und links vom Bett.
    fn rulers(&self, painter: &egui::Painter, bed: Rect, scale: f32) {
        let step = workspace::ruler_step_mm(scale);
        let color = Color32::from_gray(110);
        let font = egui::FontId::monospace(10.0);
        let tick = Stroke::new(1.0, Color32::from_gray(160));
        for x in workspace::ruler_marks(self.project.bed_width_mm, step) {
            let px = bed.left() + x * scale;
            painter.line_segment(
                [Pos2::new(px, bed.top() - 6.0), Pos2::new(px, bed.top())],
                tick,
            );
            painter.text(
                Pos2::new(px, bed.top() - 7.0),
                egui::Align2::CENTER_BOTTOM,
                workspace::ruler_label(x, step),
                font.clone(),
                color,
            );
        }
        for y in workspace::ruler_marks(self.project.bed_height_mm, step) {
            let py = self.ruler_y(bed, scale, y);
            painter.line_segment(
                [Pos2::new(bed.left() - 6.0, py), Pos2::new(bed.left(), py)],
                tick,
            );
            painter.text(
                Pos2::new(bed.left() - 8.0, py),
                egui::Align2::RIGHT_CENTER,
                workspace::ruler_label(y, step),
                font.clone(),
                color,
            );
        }
        painter.text(
            bed.min - Vec2::splat(8.0),
            egui::Align2::RIGHT_BOTTOM,
            "mm",
            font,
            color,
        );
    }

    /// Dialoge: Hinweis, ungesicherte Änderungen, Ersetzen, Senden.
    fn dialogs(&mut self, ctx: &egui::Context) {
        if let Some(action) = self.replace.take() {
            let mut choice = None;
            let modal = egui::Modal::new(egui::Id::new("replace")).show(ctx, |ui| {
                ui.set_max_width(380.0);
                ui.heading(tr!(
                    "Änderungen an „{name}“ sichern?",
                    name = self.project.name
                ));
                ui.label(tr("Nicht gesicherte Änderungen gehen verloren."));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(tr("Sichern …")).clicked() {
                        choice = Some(true);
                    }
                    if ui.button(tr("Nicht sichern")).clicked() {
                        choice = Some(false);
                    }
                    if ui.button(tr("Abbrechen")).clicked() {
                        ui.close();
                    }
                });
            });
            match choice {
                Some(true) => match self.save(false) {
                    Ok(true) => self.perform(ctx, action),
                    Ok(false) => {}
                    Err(e) => self.report(tr!("Sichern nicht möglich: {error}", error = e)),
                },
                Some(false) => self.perform(ctx, action),
                None if modal.should_close() => {}
                None => self.replace = Some(action),
            }
        }
        if let Some(pending) = self.pending_import.take() {
            // Some(true): ganze Zeichnung, Some(false): nur die Fläche.
            let mut choice = None;
            let modal = egui::Modal::new(egui::Id::new("canvas_overflow")).show(ctx, |ui| {
                ui.set_max_width(400.0);
                ui.heading(tr("Zeichnung größer als die Zeichenfläche"));
                ui.label(tr(
                    "Ein Teil der Zeichnung liegt außerhalb der Zeichenfläche der Datei. Was soll importiert werden?",
                ));
                if pending.paths.len() > 1 {
                    ui.add_space(4.0);
                    for name in &pending.names {
                        ui.monospace(name);
                    }
                    ui.label(tr("Die Wahl gilt für alle genannten Dateien."));
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(tr("Alles importieren")).clicked() {
                        choice = Some(true);
                    }
                    if ui.button(tr("Nur Zeichenfläche")).clicked() {
                        choice = Some(false);
                    }
                    if ui.button(tr("Abbrechen")).clicked() {
                        ui.close();
                    }
                });
            });
            match choice {
                Some(whole) => {
                    self.fit_policy = Some(whole);
                    if pending.replace {
                        if let Err(e) = self.open_path(ctx, &pending.paths[0]) {
                            self.report(e);
                        }
                    } else {
                        self.drop_files(ctx, pending.paths);
                    }
                    self.fit_policy = None;
                }
                None if modal.should_close() => {}
                None => self.pending_import = Some(pending),
            }
        }
        if let Some(pending) = self.overwrite.take() {
            let mut replace = false;
            let modal = egui::Modal::new(egui::Id::new("overwrite")).show(ctx, |ui| {
                ui.heading(tr("Vorhandene LTT-Dateien ersetzen?"));
                for name in &pending.existing {
                    ui.monospace(name);
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    replace = ui.button(tr("Ersetzen")).clicked();
                    if ui.button(tr("Abbrechen")).clicked() {
                        ui.close();
                    }
                });
            });
            if replace {
                self.write_export(pending);
            } else if !modal.should_close() {
                self.overwrite = Some(pending);
            }
        }
        if let Some(count) = self.confirm_clear_cache.take() {
            let mut clear = false;
            let modal = egui::Modal::new(egui::Id::new("clear_cache")).show(ctx, |ui| {
                ui.set_max_width(380.0);
                ui.heading(tr!("{count} ungesicherte Stände löschen?", count = count));
                ui.label(tr(
                    "Die abgelegten Projekte im Zwischenspeicher werden endgültig gelöscht. Gesicherte Projekte und das geöffnete Projekt bleiben erhalten.",
                ));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    clear = ui.button(tr("Löschen")).clicked();
                    if ui.button(tr("Abbrechen")).clicked() {
                        ui.close();
                    }
                });
            });
            if clear {
                self.clear_cache();
            } else if !modal.should_close() {
                self.confirm_clear_cache = Some(count);
            }
        }
        if let Some(prepared) = self.confirm_send.take() {
            let mut send = false;
            let modal = egui::Modal::new(egui::Id::new("send")).show(ctx, |ui| {
                self.send_summary(ui, &prepared);
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    send = ui.button(tr("Senden")).clicked();
                    if ui.button(tr("Abbrechen")).clicked() {
                        ui.close();
                    }
                });
            });
            if send {
                self.transmit(prepared);
            } else if !modal.should_close() {
                self.confirm_send = Some(prepared);
            }
        }
        // Hinweise zuletzt, damit sie über anderen Dialogen liegen.
        if let Some(text) = &self.alert {
            let mut close = false;
            let modal = egui::Modal::new(egui::Id::new("alert")).show(ctx, |ui| {
                ui.set_max_width(460.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("⚠").size(22.0).color(WARNING));
                    ui.heading(tr("Hinweis"));
                });
                egui::ScrollArea::vertical()
                    .max_height(320.0)
                    .show(ui, |ui| ui.label(text));
                ui.add_space(8.0);
                close = ui.button(tr("OK")).clicked();
            });
            if close || modal.should_close() {
                self.alert = None;
            }
        }
    }

    fn send_summary(&self, ui: &mut egui::Ui, prepared: &PreparedJob) {
        ui.set_max_width(560.0);
        let count = prepared.jobs.len();
        ui.heading(if count == 1 {
            tr!("{count} Auftrag an den Lasercutter senden?", count = count)
        } else {
            tr!("{count} Aufträge an den Lasercutter senden?", count = count)
        });
        ui.label(format!(
            "{} · {}:{}",
            self.devices.device().name,
            self.project.hostname,
            self.project.port
        ));
        ui.label(tr!(
            "Aufträge: {jobs} · ca. {time}",
            jobs = prepared
                .jobs
                .iter()
                .map(|job| job.name.as_str())
                .collect::<Vec<_>>()
                .join(" › "),
            time = workspace::duration(prepared.estimated_seconds)
        ));
        ui.add_space(6.0);
        egui::ScrollArea::vertical()
            .max_height(260.0)
            .show(ui, |ui| {
                egui::Grid::new("send-steps")
                    .num_columns(3)
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong(tr("Schritt"));
                        ui.strong(tr("Parameter"));
                        ui.strong(tr("Zeit"));
                        ui.end_row();
                        for (index, step) in prepared.steps.iter().enumerate() {
                            ui.label(format!(
                                "{}. {}",
                                index + 1,
                                jobs_ui::operation_title(step.operation)
                            ))
                            .on_hover_text(&step.description);
                            ui.label(workspace::step_parameters(step));
                            ui.label(workspace::duration(step.estimated_seconds));
                            ui.end_row();
                        }
                    });
            });
        for warning in &prepared.warnings {
            ui.colored_label(WARNING, format!("⚠ {warning}"));
        }
    }

    fn poll_send(&mut self, ctx: &egui::Context) {
        let Some(receiver) = &self.send_result else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Err(tr("Übertragung unerwartet abgebrochen; Gerätestatus prüfen").into())
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
                return;
            }
        };
        self.send_result = None;
        match result {
            Ok(names) => {
                self.status = tr!(
                    "Übertragen: {jobs}. Aufträge am Gerät prüfen und einzeln starten.",
                    jobs = names.join(", ")
                );
                let text = self
                    .devices
                    .device()
                    .job_sent_text
                    .replace("$jobname", &names.join(", "));
                if !text.trim().is_empty() {
                    self.alert = Some(text);
                }
            }
            Err(e) => self.report(tr!("Übertragung fehlgeschlagen: {error}", error = e)),
        }
    }

    #[cfg(feature = "screenshot")]
    fn capture(&mut self, ctx: &egui::Context) {
        let computing = self.preview.computing();
        let Some((path, frames)) = &mut self.capture else {
            return;
        };
        if let Some(image) = ctx.input(|i| {
            i.events.iter().find_map(|event| {
                if let egui::Event::Screenshot { image, .. } = event {
                    Some(image.clone())
                } else {
                    None
                }
            })
        }) {
            let pixels = image
                .pixels
                .iter()
                .flat_map(|pixel| pixel.to_array())
                .collect();
            let size =
                resvg::tiny_skia::IntSize::from_wh(image.size[0] as u32, image.size[1] as u32)
                    .unwrap();
            let pixmap = resvg::tiny_skia::Pixmap::from_vec(pixels, size).unwrap();
            if let Err(error) = pixmap.save_png(path) {
                eprintln!("{error}");
            }
            self.allow_close = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            // Eine laufende Vorschauberechnung abwarten.
            if !computing {
                *frames += 1;
            }
            if *frames == 10 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
    }
}

/// Bezugspunkt wie in Grafikprogrammen: acht Quadrate an Ecken und
/// Seitenmitten, über den Rahmen verbunden, und ein freies in der Mitte.
fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

fn reference_picker(ui: &mut egui::Ui, reference: &mut [f32; 2]) {
    const SIZE: f32 = 40.0;
    const BOX: f32 = 8.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(SIZE), Sense::click());
    let response = response.on_hover_text(tr("Bezugspunkt für X/Y und Größenänderungen"));
    let painter = ui.painter_at(rect.expand(1.0));
    let line = ui.visuals().widgets.inactive.fg_stroke.color;
    let outline = Rect::from_min_size(rect.min + Vec2::splat(BOX / 2.0), Vec2::splat(SIZE - BOX));
    painter.rect_stroke(
        outline,
        0.0,
        Stroke::new(1.0, line),
        egui::StrokeKind::Middle,
    );
    for row in 0..3 {
        for column in 0..3 {
            let point = [column as f32 / 2.0, row as f32 / 2.0];
            let min = rect.min + Vec2::new(point[0], point[1]) * (SIZE - BOX);
            let square = Rect::from_min_size(min, Vec2::splat(BOX));
            let selected = *reference == point;
            let fill = if selected {
                line
            } else {
                ui.visuals().extreme_bg_color
            };
            painter.rect_filled(square, 0.0, fill);
            painter.rect_stroke(
                square,
                0.0,
                Stroke::new(1.0, line),
                egui::StrokeKind::Inside,
            );
            if response.clicked()
                && response
                    .interact_pointer_pos()
                    .is_some_and(|p| square.expand(3.0).contains(p))
            {
                *reference = point;
            }
        }
    }
}

fn number(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    suffix: &str,
) -> bool {
    form::row(ui, label, |ui| {
        ui.add(
            egui::DragValue::new(value)
                .speed(0.1)
                .range(range)
                .suffix(suffix),
        )
        .changed()
    })
}

/// Ziehen an einem Anfasser: Lage und Größe (mm) beim Start bzw. Startpunkt
/// und aktueller Winkel des Drehanfassers.
#[derive(Clone, Copy)]
enum HandleDrag {
    Resize([f32; 4]),
    Rotate { start: Pos2, degrees: f32 },
}

/// Ergebnis einer Update-Prüfung im Hintergrund.
type UpdateCheck = std::sync::mpsc::Receiver<Result<Option<visicut_core::update::Release>, String>>;

impl eframe::App for VisiCutRust {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        #[cfg(feature = "screenshot")]
        self.capture(ctx);
        self.poll_send(ctx);
        self.sendable = !self.busy() && self.project.validate().is_ok();
        // Nullpunkt unten links: Bezugspunkt standardmäßig unten links, wie
        // am Gerät; eine eigene Wahl bleibt erhalten.
        let bottom = self.devices.origin_bottom_left();
        if self.origin_bottom_left != Some(bottom) {
            let (from, to) = if bottom {
                ([0.0, 0.0], [0.0, 1.0])
            } else {
                ([0.0, 1.0], [0.0, 0.0])
            };
            if self.origin_bottom_left.is_none() || self.reference == from {
                self.reference = to;
            }
            self.origin_bottom_left = Some(bottom);
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            if self.busy() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.status = tr("Bitte die laufende Übertragung vor dem Beenden abwarten").into();
            } else if self.dirty && !self.stash(true) {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.replace = Some(Replace::Quit);
            }
        }
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if !dropped.is_empty() && !self.modal_open() {
            self.drop_files(ctx, dropped);
        }
        self.keyboard(ctx);
        if let Some(object) = self.jobs.take_deleted_object() {
            self.delete_objects(ctx, &[object]);
        }
        match self.compose.windows(ctx, &self.project) {
            Some(Ok((project, notes))) => self.apply_composed(ctx, project, notes),
            Some(Err(e)) => self.report(e),
            None => {}
        }
        // Die Rückfrage bei ungesicherten Änderungen stellt `request`.
        if let Some((svg, name)) = self.vectorize.windows(ctx, false, &mut self.status) {
            self.request(ctx, Replace::Vectorized { svg, name });
        }
        self.poll_update(ctx);
        self.update_window(ctx);
        let device_action = self
            .devices
            .windows(ctx, &mut self.project, &mut self.status);
        match device_action {
            Some(devices_ui::Action::CalibrationPage(points)) => {
                self.request(ctx, Replace::Calibration(points));
            }
            Some(devices_ui::Action::ShiftPattern(power)) => {
                self.request(ctx, Replace::ShiftPattern(power));
            }
            None => {}
        }
        if self.show_preview {
            self.show_preview = false;
            self.preview.request(ctx, &self.project, &mut self.status);
        }
        let mut commands = Vec::new();
        let send_enabled = self.sendable && !self.busy();
        self.preview.update(
            ctx,
            &self.project,
            &mut self.status,
            self.page == Page::Preview,
        );
        // Eine angeforderte Vorschau (Menü, Kürzel, Knopf) zeigt ihren Reiter.
        if self.preview.open {
            self.preview.open = false;
            self.page = Page::Preview;
        }
        if self.page != Page::Settings {
            self.devices.close_manager_if_clean();
            self.jobs.close_library_if_clean();
        }
        if self.preview.single_job != self.settings.single_job {
            self.settings.single_job = self.preview.single_job;
            self.save_settings();
        }
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            commands.extend(self.menu_bar(ui));
        });
        egui::TopBottomPanel::top("tabs")
            .frame(
                egui::Frame::new()
                    .fill(form::TOPBAR)
                    .inner_margin(egui::Margin {
                        left: 6,
                        right: 10,
                        top: 0,
                        bottom: 0,
                    }),
            )
            .show(ctx, |ui| commands.extend(self.top_bar(ui)));
        egui::TopBottomPanel::bottom("status")
            .frame(
                egui::Frame::side_top_panel(&ctx.style())
                    .inner_margin(egui::Margin::symmetric(16, 6)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if self.busy() {
                        ui.spinner();
                    } else {
                        ui.weak("ℹ");
                    }
                    ui.weak(&self.status);
                    if self.dirty {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.weak(tr("geändert"));
                        });
                    }
                });
            });
        match self.page {
            Page::Prepare => {
                egui::SidePanel::left("settings")
                    .resizable(false)
                    .exact_width(340.0)
                    .frame(
                        egui::Frame::side_top_panel(&ctx.style())
                            .fill(form::SIDEBAR)
                            .inner_margin(0.0),
                    )
                    .show(ctx, |ui| {
                        commands.extend(self.device_card(ui));
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            egui::Frame::new()
                                .inner_margin(egui::Margin::symmetric(10, 0))
                                .show(ui, |ui| commands.extend(self.sidebar(ui)));
                        });
                    });
                egui::CentralPanel::default()
                    .frame(egui::Frame::central_panel(&ctx.style()).inner_margin(0.0))
                    .show(ctx, |ui| commands.extend(self.canvas(ui)));
            }
            Page::Preview => {
                egui::CentralPanel::default()
                    .frame(egui::Frame::central_panel(&ctx.style()).inner_margin(0.0))
                    .show(ctx, |ui| {
                        match self
                            .preview
                            .page(ui, &self.project, &mut self.status, send_enabled)
                        {
                            Some(preview_ui::Action::Send) => commands.push(Command::Send),
                            Some(preview_ui::Action::Export) => commands.push(Command::Export),
                            None => {}
                        }
                    });
            }
            Page::Settings => self.settings_tab(ctx),
        }
        for command in commands {
            self.run(ctx, command);
        }
        self.dialogs(ctx);
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
            "{}{} — VisiCutRust",
            self.project.name,
            if self.dirty { " *" } else { "" }
        )));
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1240.0, 820.0])
            .with_min_inner_size([900.0, 620.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "VisiCutRust",
        options,
        Box::new(|cc| Ok(Box::new(VisiCutRust::new(cc)))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use visicut_core::project::Part;

    #[test]
    fn part_summary_follows_list_order() {
        let mut project = Project {
            svg: "<svg/>".into(),
            part_name: "A".into(),
            width_mm: 10.0,
            ..Project::default()
        };
        project.add_part(Part {
            name: String::new(),
            width_mm: 20.0,
            ..project.active()
        });
        project.select_part(0);
        assert_eq!(part_summary(&project, 0).0, "A");
        assert_eq!(part_summary(&project, 1).1, 20.0);
        project.select_part(1);
        assert_eq!(part_summary(&project, 0).0, "A");
        assert_eq!(part_summary(&project, 1).1, 20.0);
        assert!(part_title(part_summary(&project, 1).0, 1).contains('2'));
        assert_eq!(part_title("A", 0), "A");
    }
}
