mod compose_ui;
mod devices_ui;
mod file_types;
mod font_preview;
mod form;
mod jobs_ui;
mod object_bar;
mod open_files;
mod parameters_ui;
mod preview_ui;
mod remnant_ui;
mod settings;
mod vectorize_ui;
mod workspace;

use eframe::egui::{
    self, Color32, Key, KeyboardShortcut, Modifiers, Pos2, Rect, Sense, Stroke, Vec2,
};
use std::path::{Path, PathBuf};
use visicut_core::{
    addins, import, inkscape, instance,
    ltt::{self, OutputJob, PreparedJob},
    project::{CutOrder, Operation, Project},
    session, svg,
};
use visicut_core::{
    i18n::{self, tr},
    tr,
};

const WARNING: Color32 = Color32::from_rgb(180, 75, 40);
/// Rahmen und Symbol von Hinweisen.
const ALERT_RED: Color32 = Color32::from_rgb(205, 40, 40);
/// So lange bleibt der Hinweis nach dem Senden stehen.
const SENT_NOTICE_DURATION: std::time::Duration = std::time::Duration::from_secs(10);

/// Befehle aus Menü und Tastatur. `Modifiers::COMMAND` ist Strg unter
/// Windows und Linux (⌘ unter macOS).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Command {
    New,
    Open,
    /// Grafiken als neue Teile hinzufügen.
    AddPart,
    /// Geänderte Quelldateien der Teile neu einlesen.
    Reload,
    Demo,
    Vectorize,
    Save,
    SaveAs,
    Export,
    /// Projekt für Java-VisiCut als PLF schreiben.
    ExportPlf,
    Quit,
    Devices,
    ToggleGrid,
    ToggleCamera,
    RefreshCamera,
    /// Reststücke im Kamerabild erkennen.
    Remnants,
    ZoomIn,
    ZoomOut,
    ZoomFit,
    /// Originalgröße: ein Millimeter etwa ein Millimeter auf dem Bildschirm.
    ZoomActual,
    /// Hilfe-Menü.
    Manual,
    Shortcuts,
    ReportBug,
    About,
    Materials,
    Preview,
    Send,
    Text,
    Shape,
    Sign,
    Array,
    Nest,
    Undo,
    /// Den zuletzt widerrufenen Schritt wiederherstellen.
    Redo,
    Delete,
    Duplicate,
    /// Das gewählte Teil als neues Teil kopieren.
    DuplicatePart,
    /// Das gewählte Teil in die Zwischenablage kopieren.
    Copy,
    /// Das kopierte Teil im sichtbaren Ausschnitt einfügen.
    Paste,
    /// Das kopierte Teil an seiner ursprünglichen Stelle einfügen.
    PasteInPlace,
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
    /// Aktiver Lasercutter (Menü Datei → Lasercutter), Index in der Geräteliste.
    Device(usize),
}

const COMMAND_SHIFT: Modifiers = Modifiers::COMMAND.plus(Modifiers::SHIFT);

impl Command {
    /// Reihenfolge der Tastaturprüfung: Kürzel mit Umschalt zuerst, weil egui
    /// zusätzliche Umschalttasten beim Vergleich ignoriert.
    const KEYBOARD: [Command; 19] = [
        Self::AddPart,
        Self::SaveAs,
        Self::Export,
        Self::Materials,
        Self::RefreshCamera,
        Self::Preview,
        Self::New,
        Self::Open,
        Self::Reload,
        Self::Save,
        Self::Quit,
        Self::Devices,
        Self::ToggleCamera,
        Self::ZoomFit,
        Self::ZoomActual,
        Self::ZoomIn,
        Self::ZoomOut,
        Self::ToggleGrid,
        Self::Manual,
    ];

    fn title(self) -> &'static str {
        match self {
            Self::New => tr("Neues Projekt"),
            Self::Open => tr("Öffnen …"),
            Self::AddPart => tr("Hinzufügen …"),
            Self::Reload => tr("Neu laden"),
            Self::Demo => tr("Beispiel öffnen"),
            Self::Vectorize => tr("Bitmap vektorisieren …"),
            Self::Save => tr("Sichern"),
            Self::SaveAs => tr("Sichern unter …"),
            Self::Export => tr("LTT-Job exportieren …"),
            Self::ExportPlf => tr("Als VisiCut-Projekt (PLF) exportieren …"),
            Self::Quit => tr("Beenden"),
            Self::Devices => tr("Einstellungen …"),
            Self::Device(_) => tr("Lasercutter"),
            Self::ToggleGrid => tr("Raster anzeigen"),
            Self::ToggleCamera => tr("Kamerabild anzeigen"),
            Self::RefreshCamera => tr("Kamerabild aktualisieren"),
            Self::Remnants => tr("Reststücke erkennen …"),
            Self::ZoomIn => tr("Vergrößern"),
            Self::ZoomOut => tr("Verkleinern"),
            Self::ZoomFit => tr("Zoom zurücksetzen"),
            Self::ZoomActual => tr("Originalgröße (1:1)"),
            Self::Manual => tr("Anleitung"),
            Self::Shortcuts => tr("Tastenkürzel"),
            Self::ReportBug => tr("Fehler melden …"),
            Self::About => tr("Über VisiCutR"),
            Self::Materials => tr("Materialbibliothek …"),
            Self::Preview => tr("Vorschau & Zeit …"),
            Self::Send => tr("An Lasercutter senden …"),
            Self::Text => tr("Text …"),
            Self::Shape => tr("Form …"),
            Self::Sign => tr("Schild …"),
            Self::Array => tr("Kopien im Raster …"),
            Self::Nest => tr("Teile verschachteln (Nesting) …"),
            Self::Undo => tr("Widerrufen"),
            Self::Redo => tr("Wiederholen"),
            Self::Delete => tr("Löschen"),
            Self::Duplicate => tr("Duplizieren"),
            Self::DuplicatePart => tr("Teil duplizieren"),
            Self::Copy => tr("Kopieren"),
            Self::Paste => tr("Einfügen"),
            Self::PasteInPlace => tr("An Originalposition einfügen"),
            Self::RotateLeft => tr("90° nach links drehen"),
            Self::RotateRight => tr("90° nach rechts drehen"),
            Self::RotateBy => tr("Um den Winkel drehen"),
            Self::FlipHorizontal => tr("Horizontal spiegeln"),
            Self::FlipVertical => tr("Vertikal spiegeln"),
            Self::OpenRecent(_) => tr("Zuletzt geöffnet"),
            Self::ClearRecent => tr("Liste leeren"),
            Self::ClearCache => tr("Zwischenspeicher leeren …"),
        }
    }

    fn shortcut(self) -> Option<KeyboardShortcut> {
        let (modifiers, key) = match self {
            Self::New => (Modifiers::COMMAND, Key::N),
            Self::Open => (Modifiers::COMMAND, Key::O),
            Self::AddPart => (COMMAND_SHIFT, Key::O),
            Self::Reload => (Modifiers::COMMAND, Key::R),
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
            Self::ZoomActual => (Modifiers::COMMAND, Key::Num1),
            Self::Manual => (Modifiers::NONE, Key::F1),
            Self::Materials => (COMMAND_SHIFT, Key::M),
            Self::Preview => (COMMAND_SHIFT, Key::P),
            Self::ToggleGrid => (Modifiers::COMMAND, Key::G),
            // Nur ohne aktives Eingabefeld (siehe `keyboard`).
            Self::Undo => (Modifiers::COMMAND, Key::Z),
            Self::Redo => (COMMAND_SHIFT, Key::Z),
            Self::Delete => (Modifiers::NONE, Key::Delete),
            Self::Duplicate => (Modifiers::COMMAND, Key::D),
            // ⌘C und ⌘V meldet egui als Kopieren/Einfügen (siehe `keyboard`).
            Self::Copy => (Modifiers::COMMAND, Key::C),
            Self::Paste => (Modifiers::COMMAND, Key::V),
            Self::PasteInPlace => (Modifiers::COMMAND, Key::F),
            Self::Demo
            | Self::Vectorize
            | Self::Remnants
            | Self::Shortcuts
            | Self::ReportBug
            | Self::About
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
            | Self::Device(_)
            | Self::ExportPlf => return None,
        };
        Some(KeyboardShortcut::new(modifiers, key))
    }
}

/// Erläuterung unter einer Checkbox, bündig mit deren Beschriftung.
fn indented_caption(ui: &mut egui::Ui, text: &str) {
    egui::Frame::new()
        .inner_margin(egui::Margin {
            left: 18,
            bottom: 4,
            ..Default::default()
        })
        .show(ui, |ui| form::caption(ui, text));
}

/// Geladene FabLab-Einstellungen und was davon übernommen wird.
struct LabOffer {
    name: String,
    url: String,
    settings: visicut_core::lab::LabSettings,
    /// Neue Fassung eines abonnierten FabLabs (Prüfung beim Start).
    update: bool,
    devices: bool,
    materials: bool,
    templates: bool,
    watch: bool,
}

impl LabOffer {
    fn new(
        name: String,
        url: String,
        settings: visicut_core::lab::LabSettings,
        update: bool,
    ) -> Self {
        Self {
            name,
            url,
            settings,
            update,
            devices: true,
            materials: true,
            templates: true,
            watch: true,
        }
    }
}

/// Antwort im Dialog der FabLab-Einstellungen.
#[derive(Clone, Copy, PartialEq)]
enum LabAnswer {
    /// Noch keine; der Dialog bleibt offen, solange er nicht geschlossen wird.
    Open,
    Apply,
    Cancel,
}

/// Überschrift des Dialogs mit dem Namen des FabLabs.
fn lab_offer_title(ui: &mut egui::Ui, offer: &LabOffer) {
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new(if offer.update {
                tr("Neue FabLab-Einstellungen")
            } else {
                tr("FabLab-Einstellungen übernehmen")
            })
            .font(form::bold(20.0)),
        );
        ui.add_space(4.0);
        ui.weak(&offer.name);
    });
}

/// Was übernommen wird: Lasercutter, Materialien, Vorlagen, Suche beim Start.
fn lab_offer_choices(ui: &mut egui::Ui, offer: &mut LabOffer) {
    let settings = &offer.settings;
    ui.checkbox(
        &mut offer.devices,
        tr!("{count} Lasercutter", count = settings.devices.len()),
    );
    indented_caption(ui, tr("Ersetzt Lasercutter gleichen Namens."));
    if let Some(library) = &settings.library {
        ui.checkbox(
            &mut offer.materials,
            tr!(
                "{materials} Materialien mit {profiles} Laserwerten",
                materials = library.materials.len(),
                profiles = settings.profile_count()
            ),
        );
        indented_caption(
            ui,
            tr("Ersetzt die Materialbibliothek samt eigener Änderungen."),
        );
    }
    if !settings.templates.is_empty() {
        ui.checkbox(
            &mut offer.templates,
            tr!(
                "{count} Zuordnungen als Vorlagen",
                count = settings.templates.len()
            ),
        );
    }
    ui.add_space(6.0);
    ui.checkbox(
        &mut offer.watch,
        tr("Beim Start nach neuen Fassungen suchen"),
    );
    if !settings.warnings.is_empty() {
        ui.add_space(6.0);
        egui::CollapsingHeader::new(tr!("{count} Hinweise", count = settings.warnings.len())).show(
            ui,
            |ui| {
                for warning in &settings.warnings {
                    form::caption(ui, warning.clone());
                }
            },
        );
    }
}

/// „Übernehmen“ und „Abbrechen“ bzw. „Später“.
fn lab_offer_buttons(ui: &mut egui::Ui, offer: &LabOffer) -> LabAnswer {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let apply = ui
            .add_enabled(
                offer.devices || offer.materials || offer.templates || offer.watch,
                form::primary_button(tr("Übernehmen")),
            )
            .clicked();
        let cancel = ui
            .add(
                egui::Button::new(if offer.update {
                    tr("Später")
                } else {
                    tr("Abbrechen")
                })
                .min_size(egui::vec2(0.0, 28.0)),
            )
            .clicked();
        if apply {
            LabAnswer::Apply
        } else if cancel {
            LabAnswer::Cancel
        } else {
            LabAnswer::Open
        }
    })
    .inner
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
/// Dateien, die ein Hintergrund-Thread liest, und was danach mit ihnen
/// geschieht: Große oder aufwendige Dateien halten so das Fenster nicht an.
struct Reading {
    result: std::sync::mpsc::Receiver<import::Prefetched>,
    /// Ab einer kurzen Wartezeit zeigt ein Dialog das Lesen mit „Abbrechen“.
    started: std::time::Instant,
    then: Deferred,
    /// Wahl aus der Rückfrage „Zeichnung größer als die Zeichenfläche“.
    fit_policy: Option<bool>,
}

/// Ein Import, der nach dem Lesen der Dateien weiterläuft.
enum Deferred {
    /// Öffnen nach dem Lesen; `dirty`: Das Projekt war beim Aufschieben
    /// schon ungesichert, die Rückfrage dazu ist beantwortet.
    Open {
        path: PathBuf,
        dirty: bool,
    },
    Drop(Vec<PathBuf>),
    Request(instance::Request),
    Reload,
}

/// Projekt aus dem Verlauf. Der Verlauf kennt den Gerätewechsel nicht:
/// Adresse und Maschinenwerte gehören immer zum gewählten Lasercutter.
fn restored(devices: &devices_ui::DeviceUi, mut project: Project) -> Project {
    devices.apply(&mut project);
    project
}

/// Zeichnung (Adresse, Länge), Zahl der Teile und aktives Teil: ändert sich
/// beim Öffnen, Widerrufen und bei Werkzeugen, nicht beim Verschieben.
type ProjectIdentity = (usize, usize, usize, usize);
/// Ergebnis eines Werkzeugs im Hintergrund: Projekt und Hinweise.
type ComposedResult = Result<(Project, Vec<String>), String>;

/// Eine Geste (Ziehen, Tippen in X/Y/B/H) bekommt einen Widerrufen-Schritt.
#[derive(Default)]
struct Gesture {
    /// Projekt und Lage zu Beginn des Bilds.
    start: Option<(ProjectIdentity, [f32; 4])>,
    /// Für die laufende Geste ist schon ein Schritt gemerkt.
    open: bool,
    /// In diesem Bild hat eine Aktion selbst einen Schritt gemerkt.
    explicit: bool,
}

struct Snapshot {
    project: Project,
    texture: Option<egui::TextureHandle>,
    part_textures: Vec<Option<egui::TextureHandle>>,
}

/// Knopfgruppen rechts über der Arbeitsfläche, von links nach rechts.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ViewGroup {
    Remnants,
    Camera,
    Grid,
    Zoom,
    Actual,
    Fit,
}

impl ViewGroup {
    /// Einträge im Menü „…“, wenn die Gruppe nicht mehr in die Leiste passt.
    fn commands(self) -> &'static [Command] {
        match self {
            Self::Remnants => &[Command::Remnants],
            Self::Camera => &[Command::ToggleCamera, Command::RefreshCamera],
            Self::Grid => &[Command::ToggleGrid],
            Self::Zoom => &[Command::ZoomIn, Command::ZoomOut],
            Self::Actual => &[Command::ZoomActual],
            Self::Fit => &[Command::ZoomFit],
        }
    }
}

/// Wie viele Ansichtsgruppen von links neben den Titel passen; reicht der
/// Platz nicht für alle, bleibt Raum für das Menü „…“.
fn fitting_groups(ui: &egui::Ui, groups: &[(ViewGroup, Vec<&str>)]) -> usize {
    let spacing = ui.spacing().item_spacing.x;
    let padding = ui.spacing().button_padding.x * 2.0;
    let font = egui::TextStyle::Button.resolve(ui.style());
    let text_width = |text: &str| {
        ui.fonts_mut(|f| {
            f.layout_no_wrap(text.to_owned(), font.clone(), Color32::BLACK)
                .size()
                .x
        })
    };
    let widths: Vec<f32> = groups
        .iter()
        .map(|(group, texts)| {
            let extra = if *group == ViewGroup::Zoom { 6.0 } else { 0.0 };
            texts
                .iter()
                .map(|t| text_width(t) + padding + spacing)
                .sum::<f32>()
                + extra
        })
        .collect();
    let available = ui.available_width() - 24.0;
    let overflow = text_width("…") + padding + spacing;
    let mut shown = groups.len();
    while shown > 0 {
        let needed: f32 =
            widths[..shown].iter().sum::<f32>() + if shown < groups.len() { overflow } else { 0.0 };
        if needed <= available {
            break;
        }
        shown -= 1;
    }
    shown
}

/// Reihenfolge im Fenster „Tastenkürzel“.
const SHORTCUT_LIST: [Command; 26] = [
    Command::New,
    Command::Open,
    Command::AddPart,
    Command::Reload,
    Command::Save,
    Command::SaveAs,
    Command::Export,
    Command::Undo,
    Command::Redo,
    Command::Duplicate,
    Command::Copy,
    Command::Paste,
    Command::PasteInPlace,
    Command::Delete,
    Command::ZoomIn,
    Command::ZoomOut,
    Command::ZoomFit,
    Command::ZoomActual,
    Command::ToggleGrid,
    Command::ToggleCamera,
    Command::RefreshCamera,
    Command::Materials,
    Command::Preview,
    Command::Devices,
    Command::Manual,
    Command::Quit,
];

/// Widerrufbare Schritte; ältere fallen weg.
const UNDO_STEPS: usize = 50;

/// Legt `snapshot` auf den Stapel und entfernt den ältesten Stand, wenn er
/// mehr als `UNDO_STEPS` hält.
fn push_limited<T>(stack: &mut Vec<T>, snapshot: T) {
    stack.push(snapshot);
    if stack.len() > UNDO_STEPS {
        stack.remove(0);
    }
}

/// Widerrufen und Wiederholen.
struct History<T> {
    /// Stände vor den letzten Werkzeug- oder Löschaktionen, der jüngste
    /// zuletzt (höchstens `UNDO_STEPS`).
    undo: Vec<T>,
    /// Stände vor dem Widerrufen; verfallen mit der nächsten Aktion.
    redo: Vec<T>,
}

impl<T> Default for History<T> {
    fn default() -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }
}

impl<T> History<T> {
    fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    /// Merkt `before` für Widerrufen; widerrufene Schritte lassen sich danach
    /// nicht mehr wiederholen.
    fn record(&mut self, before: T) {
        push_limited(&mut self.undo, before);
        self.redo.clear();
    }

    /// Der Stand vor der letzten Aktion; `current` kommt auf den Wiederholen-Stapel.
    fn undo(&mut self, current: T) -> Option<T> {
        let snapshot = self.undo.pop()?;
        push_limited(&mut self.redo, current);
        Some(snapshot)
    }

    /// Der zuletzt widerrufene Stand; `current` kommt auf den Widerrufen-Stapel.
    fn redo(&mut self, current: T) -> Option<T> {
        let snapshot = self.redo.pop()?;
        push_limited(&mut self.undo, current);
        Some(snapshot)
    }

    fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
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
    Job,
    General,
    /// Erweiterungen für Zeichenprogramme (Inkscape, später weitere).
    AddIns,
}

impl SettingsPage {
    const ALL: [Self; 5] = [
        Self::General,
        Self::Devices,
        Self::Materials,
        Self::Job,
        Self::AddIns,
    ];

    /// Name in der linken Leiste und zugleich Überschrift der Seite.
    fn title(self) -> &'static str {
        match self {
            Self::Devices => tr("Lasercutter"),
            Self::Materials => tr("Materialien"),
            Self::Job => tr("Job"),
            Self::General => tr("Allgemein"),
            Self::AddIns => tr("Software-Add-ins"),
        }
    }
}

struct VisiCutR {
    project: Project,
    navigation: Navigation,
    textures: Textures,
    canvas: CanvasView,
    /// Nullpunkt des Geräts beim letzten Bild (unten links?); ein Wechsel
    /// setzt den Standard-Bezugspunkt.
    origin_bottom_left: Option<bool>,
    messages: Messages,
    pending: Pending,
    /// Gewählte Behandlung beim laufenden Import: `Some(true)` ganze Zeichnung.
    fit_policy: Option<bool>,
    file: FileState,
    inspector: InspectorInputs,
    allow_close: bool,
    /// Zuletzt kopiertes Teil (für Einfügen aus dem Menü und ⌘F).
    copied: Option<visicut_core::project::Part>,
    /// Projekt ist vollständig und passt aufs Bett (pro Bild neu bestimmt).
    sendable: bool,
    preview: preview_ui::PreviewUi,
    background: Background,
    devices: devices_ui::DeviceUi,
    jobs: jobs_ui::JobUi,
    vectorize: vectorize_ui::VectorizeUi,
    remnants: remnant_ui::RemnantUi,
    compose: compose_ui::ComposeUi,
    history: History<Snapshot>,
    /// Laufende Änderung von Lage oder Größe (Ziehen, Felder).
    gesture: Gesture,
    /// Sprache und zuletzt geöffnete Dateien.
    settings: settings::AppSettings,
    /// Felder für die Parameter eines parametrischen Teils.
    parameters: parameters_ui::ParametersUi,
    /// Stand der Inkscape-Erweiterung (Einstellungen), erst bei Bedarf geprüft.
    inkscape: Option<inkscape::Status>,
    #[cfg(feature = "screenshot")]
    capture: Option<(PathBuf, u32)>,
}

/// Rückfragen, die als Dialog auf eine Antwort warten.
#[derive(Default)]
struct Pending {
    /// Zeichnung ragt über die Zeichenfläche: Auswahl steht aus.
    import: Option<PendingImport>,
    /// Wartet auf „Sichern / Nicht sichern / Abbrechen“.
    replace: Option<Replace>,
    overwrite: Option<PendingExport>,
    /// Wartet auf die Bestätigung „Zwischenspeicher leeren“ (Anzahl Stände).
    clear_cache: Option<usize>,
    send: Option<PreparedJob>,
    /// Rückfrage beim Beenden mit ungesicherter Materialbibliothek.
    library_quit: bool,
}

impl Pending {
    /// Eine Rückfrage (außer der Importwahl) ist offen.
    fn asking(&self) -> bool {
        self.replace.is_some()
            || self.overwrite.is_some()
            || self.clear_cache.is_some()
            || self.send.is_some()
            || self.library_quit
    }
}

/// Ansicht der Arbeitsfläche: Zoom, sichtbarer Ausschnitt, Hervorhebung und Anfasser.
struct CanvasView {
    /// Umriss des Objekts unter der Maus in der Objektliste (mm relativ zur
    /// Motivecke) mit dem Stand und Objekt, für das er gilt.
    hover_outline: Option<(u64, usize, Vec<visicut_core::geometry::Contour>)>,
    /// Umrisse aller Objekte für den Treffertest auf der Arbeitsfläche.
    all_outlines: Option<(u64, Vec<Vec<visicut_core::geometry::Contour>>)>,
    /// Umrisse aller Objekte, die im Hintergrund berechnet werden (bei
    /// tausend Pfaden dauert das; das Fenster soll dabei nicht stehen).
    outlines_pending: Option<(
        u64,
        std::sync::mpsc::Receiver<Vec<Vec<visicut_core::geometry::Contour>>>,
    )>,
    zoom: f32,
    /// Bildpunkte je mm auf dem Bildschirm, zuletzt in `bed` berechnet.
    view_scale: f32,
    /// Sichtbarer Teil des Betts (x, y, Breite, Höhe in mm), zuletzt in `bed`
    /// bestimmt; leer, solange das Bett nicht gezeigt wurde.
    visible_mm: [f32; 4],
    show_grid: bool,
    /// Ziehen an einem Anfasser des gewählten Teils.
    handle_drag: Option<HandleDrag>,
}

/// Arbeiten in anderen Threads und Prozessen und ihre noch offenen Ergebnisse.
struct Background {
    send_result: Option<std::sync::mpsc::Receiver<Result<Vec<String>, String>>>,
    reading: Option<Reading>,
    /// Laufende Update-Prüfung (`true`: von Hand gestartet) und gefundenes Release.
    update_check: Option<(bool, UpdateCheck)>,
    update_found: Option<visicut_core::update::Release>,
    /// Geladene FabLab-Einstellungen, die der Dialog zum Übernehmen anbietet.
    lab_offer: Option<LabOffer>,
    /// Laufende Prüfung der abonnierten FabLab-Einstellungen beim Start.
    lab_check: Option<std::sync::mpsc::Receiver<Result<visicut_core::lab::LabSettings, String>>>,
    /// Nimmt Dateien späterer Starts und der Inkscape-Erweiterung entgegen.
    instance: Option<instance::Listener>,
    /// Übergebene Dateien, die auf das Ende eines Dialogs warten.
    handed_over: Option<instance::Request>,
    /// Export oder Senden wartet auf den im Hintergrund vorbereiteten Auftrag.
    after_preview: Option<Command>,
    /// Neu erzeugte Grafik eines parametrischen Teils (Projektstand, Ergebnis).
    parameters: Option<(ProjectIdentity, std::sync::mpsc::Receiver<ComposedResult>)>,
    /// Lasercutter, Material und Senden aus der Befehlszeile; warten, bis
    /// die Dateien geöffnet sind.
    launch_options: Option<instance::Options>,
}

/// Statuszeile und Hinweisdialog.
struct Messages {
    status: String,
    /// Deutlich sichtbarer Hinweis (Fehler, Importwarnungen) in einem Dialog.
    alert: Option<String>,
    /// Hinweis nach dem Senden: schließt sich zu diesem Zeitpunkt von selbst,
    /// solange der Dialog noch genau diesen Text zeigt.
    alert_until: Option<(std::time::Instant, String)>,
}

/// Eingaben im Inspektor, die über ein Bild hinaus gelten.
struct InspectorInputs {
    proportional: bool,
    /// Bezugspunkt für X/Y und Größenänderungen (0, ½ oder 1 je Achse).
    reference: [f32; 2],
    rotate_deg: f32,
}

/// Datei des Projekts, Zwischenstand und ob es seit dem Sichern geändert wurde.
struct FileState {
    /// Gesicherte Projektdatei; `None` für ein neues Projekt.
    path: Option<PathBuf>,
    /// Projektname während des Umbenennens im Kopf der Arbeitsfläche.
    name_draft: Option<String>,
    /// Zwischenstand, aus dem das Projekt geöffnet wurde (`session`);
    /// Sichern löscht ihn.
    cached: Option<PathBuf>,
    dirty: bool,
}

/// Gezeichnete Grafiken der Teile für die Arbeitsfläche.
struct Textures {
    /// Textur des gewählten Teils.
    active: Option<egui::TextureHandle>,
    /// Texturen der übrigen Teile, parallel zu `project.parts`.
    parts: Vec<Option<egui::TextureHandle>>,
    /// Objekte in den Farben ihrer Bearbeitung (`ltt::processing_preview`)
    /// und der Stand, für den sie gilt.
    live: Option<(u64, Option<egui::TextureHandle>)>,
}

/// Gezeigte Seite und offene Fenster.
struct Navigation {
    page: Page,
    settings_page: SettingsPage,
    /// „Vorschau & Zeit“ (Menü, Reiterleiste) setzt dieses Feld; `update`
    /// berechnet die Vorschau und wechselt in den Reiter „Vorschau“.
    show_preview: bool,
    /// Fenster mit den Tastenkürzeln.
    show_shortcuts: bool,
}

/// Lädt die Einstellungen und stellt ihre Sprache ein.
fn load_settings() -> settings::AppSettings {
    // Erster Start nach der Umbenennung: Einstellungen von VisiCutRust übernehmen.
    let moved = visicut_core::device::migrate_legacy_config();
    let mut settings = settings::AppSettings::load();
    if let Some((old, new)) = &moved {
        settings.moved_config(old, new);
        let _ = settings.save();
    }
    i18n::set_language(settings.language());
    settings
}

/// Helles Erscheinungsbild, Schriften und Tastaturzoom der Oberfläche.
fn apply_appearance(ctx: &egui::Context) {
    // Die Oberfläche ist hell: auch die Titelleiste hell, nicht schwarz auf
    // dunklem Hintergrund.
    ctx.set_theme(egui::ThemePreference::Light);
    // Die Titelleiste richtet winit nach dem Windows-Theme; egui gibt die
    // Vorgabe nicht von selbst weiter.
    ctx.send_viewport_cmd(egui::ViewportCommand::SetTheme(egui::SystemTheme::Light));
    ctx.set_visuals(egui::Visuals::light());
    form::apply_style(ctx);
    form::apply_fonts(ctx);
    // Strg+Plus/Minus/0 zoomen die Arbeitsfläche, nicht die Oberfläche.
    ctx.options_mut(|o| o.zoom_with_keyboard = false);
}

impl VisiCutR {
    fn new(cc: &eframe::CreationContext<'_>, launch: instance::Launch) -> Self {
        let settings = load_settings();
        apply_appearance(&cc.egui_ctx);
        let (devices, device_error) = devices_ui::DeviceUi::load();
        let (jobs, material_error) = jobs_ui::JobUi::load();
        let mut app = Self::initial(settings, devices, jobs);
        app.reset_project();
        app.preview.single_job = app.settings.single_job;
        app.start_due_checks();
        if let Some(error) = device_error {
            app.report(tr!(
                "Geräteliste nicht lesbar, FAU-Standard wird verwendet: {error}",
                error = error
            ));
        } else if let Some(error) = material_error {
            app.report(error);
        }
        #[cfg(feature = "screenshot")]
        app.apply_screenshot_arguments();
        if !launch.standalone {
            app.listen_as_instance(&cc.egui_ctx);
        }
        app.open_at_launch(&cc.egui_ctx, launch);
        cc.egui_ctx.request_repaint();
        app
    }

    /// Zustand vor dem ersten Bild: leeres Projekt, nichts offen.
    fn initial(
        settings: settings::AppSettings,
        devices: devices_ui::DeviceUi,
        jobs: jobs_ui::JobUi,
    ) -> Self {
        Self {
            project: Project::default(),
            textures: Textures {
                active: None,
                parts: Vec::new(),
                live: None,
            },
            origin_bottom_left: None,
            messages: Messages {
                status: tr("Datei öffnen, hierher ziehen oder das Beispiel öffnen").into(),
                alert: None,
                alert_until: None,
            },
            fit_policy: None,
            file: FileState {
                path: None,
                name_draft: None,
                cached: None,
                dirty: false,
            },
            inspector: InspectorInputs {
                proportional: true,
                reference: [0.0, 0.0],
                rotate_deg: 0.0,
            },
            allow_close: false,
            canvas: CanvasView {
                zoom: 1.0,
                view_scale: 1.0,
                visible_mm: [0.0; 4],
                show_grid: true,
                hover_outline: None,
                all_outlines: None,
                outlines_pending: None,
                handle_drag: None,
            },
            copied: None,
            sendable: false,
            pending: Pending::default(),
            navigation: Navigation {
                page: Page::Prepare,
                settings_page: SettingsPage::Devices,
                show_preview: false,
                show_shortcuts: false,
            },
            preview: preview_ui::PreviewUi::default(),
            background: Background {
                send_result: None,
                reading: None,
                update_check: None,
                update_found: None,
                lab_offer: None,
                lab_check: None,
                instance: None,
                handed_over: None,
                after_preview: None,
                parameters: None,
                launch_options: None,
            },
            devices,
            jobs,
            vectorize: vectorize_ui::VectorizeUi::default(),
            remnants: remnant_ui::RemnantUi::default(),
            compose: compose_ui::ComposeUi::default(),
            history: History::default(),
            gesture: Gesture::default(),
            settings,
            parameters: Default::default(),
            inkscape: None,
            #[cfg(feature = "screenshot")]
            capture: None,
        }
    }

    /// Startet die fälligen Prüfungen auf ein neues Release und neue
    /// FabLab-Einstellungen.
    fn start_due_checks(&mut self) {
        import::allow_ghostscript_for_postscript(self.settings.ghostscript_for_postscript);
        // Alle 14 Tage im Hintergrund nach einem neuen Release fragen.
        if !self.settings.update_check_disabled
            && visicut_core::update::is_due(
                self.settings.last_update_check,
                visicut_core::update::now_secs(),
            )
        {
            self.start_update_check(false);
        }
        // Einmal am Tag nach einer neuen Fassung der FabLab-Einstellungen fragen.
        if let Some(lab) = self
            .settings
            .lab
            .clone()
            .filter(|lab| lab.is_due(visicut_core::update::now_secs()))
        {
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let _ =
                    tx.send(visicut_core::lab::download(&lab.name, &lab.url).map_err(String::from));
            });
            self.background.lab_check = Some(rx);
        }
    }

    /// `--capture BILD.png` und `--page …` für Bildschirmaufnahmen.
    #[cfg(feature = "screenshot")]
    fn apply_screenshot_arguments(&mut self) {
        let arguments: Vec<String> = std::env::args().skip(1).collect();
        if let Some(index) = arguments
            .iter()
            .position(|argument| argument == "--capture")
        {
            self.capture = arguments
                .get(index + 1)
                .map(|path| (PathBuf::from(path), 0));
        }
        // Reiter für die Aufnahme: `--page preview|materials|job|general|devices`.
        if let Some(index) = arguments.iter().position(|argument| argument == "--page") {
            match arguments.get(index + 1).map(String::as_str) {
                Some("preview") => self.navigation.show_preview = true,
                Some("devices") => self.show_settings(SettingsPage::Devices),
                Some("materials") => self.show_settings(SettingsPage::Materials),
                Some("job") => self.show_settings(SettingsPage::Job),
                Some("general") => self.show_settings(SettingsPage::General),
                Some("addins") => self.show_settings(SettingsPage::AddIns),
                _ => {}
            }
        }
    }

    /// Nimmt Dateien späterer Starts entgegen und merkt sich den
    /// Programmpfad für die Erweiterungen.
    fn listen_as_instance(&mut self, ctx: &egui::Context) {
        // Für das Illustrator-Skript und das CorelDRAW-Makro.
        if let Ok(program) = std::env::current_exe()
            && let Err(e) = addins::remember_program(&visicut_core::device::config_dir(), &program)
        {
            eprintln!("Programmpfad nicht gespeichert: {e}");
        }
        let ctx = ctx.clone();
        match instance::Listener::start(&visicut_core::device::config_dir(), move || {
            ctx.request_repaint()
        }) {
            Ok(listener) => self.background.instance = Some(listener),
            Err(e) => eprintln!("Einzelinstanz nicht verfügbar: {e}"),
        }
    }

    /// Öffnet, was beim Start verlangt wurde: Beispiel oder übergebene
    /// Dateien; sonst bleibt der Auftrag leer.
    fn open_at_launch(&mut self, ctx: &egui::Context, launch: instance::Launch) {
        // Unter macOS kommen per Doppelklick geöffnete Dateien als Ereignis.
        let opened = open_files::take();
        open_files::watch(ctx);
        if launch.demo {
            self.perform(ctx, Replace::Demo);
        } else if !launch.request.files.is_empty() {
            self.open_request(ctx, launch.request);
        } else if !opened.is_empty() {
            self.open_files(ctx, opened);
        } else {
            // Sonst beginnt die App mit einem leeren Auftrag; der beim letzten
            // Beenden ungesicherte Stand steht unter „Zuletzt geöffnet“.
            // Ein Merker älterer Fassungen wird vergessen.
            let _ = session::take_resume(&session::dir());
        }
    }

    /// Sichert Sprache und Verlauf; ein Fehler steht nur in der Statuszeile.
    /// Fragt im Hintergrund bei GitHub nach einem neuen Release.
    fn start_update_check(&mut self, manual: bool) {
        if self.background.update_check.is_some() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(visicut_core::update::check().map_err(String::from));
        });
        self.background.update_check = Some((manual, rx));
        if manual {
            self.messages.status = tr("Suche nach Updates …").into();
        }
    }

    /// Ergebnis der Update-Prüfung; Fehler der automatischen Prüfung stehen
    /// nur in der Statuszeile.
    fn poll_update(&mut self, ctx: &egui::Context) {
        let Some((manual, receiver)) = &self.background.update_check else {
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
        self.background.update_check = None;
        match result {
            Ok(found) => {
                self.settings.last_update_check = visicut_core::update::now_secs();
                self.save_settings();
                match found {
                    Some(release) => self.background.update_found = Some(release),
                    None if manual => {
                        self.messages.status = tr!(
                            "VisiCutR {version} ist aktuell",
                            version = visicut_core::update::current_version()
                        )
                    }
                    None => {}
                }
            }
            Err(e) if manual => self.report(e),
            Err(e) => self.messages.status = e,
        }
    }

    /// Heruntergeladene FabLab-Einstellungen und das Ergebnis der Prüfung beim
    /// Start: Ist etwas neu, fragt ein Dialog, was übernommen wird.
    fn poll_lab(&mut self, ctx: &egui::Context) {
        if let Some((name, url, settings)) = self.devices.downloaded.take() {
            self.background.lab_offer = Some(LabOffer::new(name, url, settings, false));
        }
        let Some(receiver) = &self.background.lab_check else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(500));
                return;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                return self.background.lab_check = None;
            }
        };
        self.background.lab_check = None;
        let Some(lab) = &mut self.settings.lab else {
            return;
        };
        match result {
            Ok(settings) => {
                lab.last_check = visicut_core::update::now_secs();
                if settings.fingerprint != lab.fingerprint && self.background.lab_offer.is_none() {
                    self.background.lab_offer = Some(LabOffer::new(
                        lab.name.clone(),
                        lab.url.clone(),
                        settings,
                        true,
                    ));
                }
                self.save_settings();
            }
            // Ohne Netz nur ein Hinweis in der Statuszeile; morgen wieder.
            Err(e) => {
                self.messages.status = tr!(
                    "FabLab-Einstellungen von „{name}“ nicht abrufbar: {error}",
                    name = lab.name,
                    error = e
                )
            }
        }
    }

    /// Fragt, welche Teile der FabLab-Einstellungen übernommen werden.
    fn lab_window(&mut self, ctx: &egui::Context) {
        let Some(mut offer) = self.background.lab_offer.take() else {
            return;
        };
        let mut answer = LabAnswer::Open;
        let outer = egui::Frame::popup(&ctx.style()).inner_margin(5.0);
        let modal = egui::Modal::new(egui::Id::new("lab"))
            .frame(outer)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .stroke(egui::Stroke::new(2.0, form::ACCENT))
                    .corner_radius(7.0)
                    .inner_margin(egui::Margin::symmetric(18, 14))
                    .show(ui, |ui| {
                        ui.set_width(460.0);
                        lab_offer_title(ui, &offer);
                        ui.add_space(10.0);
                        lab_offer_choices(ui, &mut offer);
                        ui.add_space(12.0);
                        answer = lab_offer_buttons(ui, &offer);
                    });
            });
        match answer {
            LabAnswer::Apply => self.adopt_lab(offer),
            LabAnswer::Open if !modal.should_close() => self.background.lab_offer = Some(offer),
            _ => {}
        }
    }

    fn adopt_lab(&mut self, offer: LabOffer) {
        let LabOffer {
            name,
            url,
            settings,
            devices,
            materials,
            templates,
            watch,
            ..
        } = offer;
        let mut done = Vec::new();
        let mut problems = Vec::new();
        if devices
            && self
                .devices
                .adopt_lab(
                    settings.devices,
                    &mut self.project,
                    &mut self.messages.status,
                )
                .is_some()
        {
            done.push(tr("Lasercutter"));
        }
        if materials && let Some(library) = settings.library {
            match self.jobs.adopt_library(library) {
                Ok(()) => done.push(tr("Materialien")),
                Err(e) => problems.push(e),
            }
        }
        if templates && !settings.templates.is_empty() {
            match self.jobs.adopt_templates(settings.templates) {
                Ok(_) => done.push(tr("Zuordnungen")),
                Err(e) => problems.push(e),
            }
        }
        self.settings.lab = watch.then(|| settings::LabSubscription {
            name: name.clone(),
            url,
            fingerprint: settings.fingerprint,
            last_check: visicut_core::update::now_secs(),
        });
        self.save_settings();
        if !problems.is_empty() {
            self.report(problems.join("\n"));
        } else if !done.is_empty() {
            self.messages.status = tr!(
                "Von „{name}“ übernommen: {parts}",
                name = name,
                parts = done.join(", ")
            );
        }
    }

    /// Hinweis auf ein neues Release mit Link zur Download-Seite.
    fn update_window(&mut self, ctx: &egui::Context) {
        let Some(release) = &self.background.update_found else {
            return;
        };
        let mut close = false;
        egui::Window::new(tr("Update verfügbar"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(tr!(
                    "VisiCutR {version} ist verfügbar (installiert: {current}).",
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
            self.background.update_found = None;
        }
    }

    fn save_settings(&mut self) {
        if let Err(e) = self.settings.save() {
            self.messages.status = tr!("Einstellungen nicht gesichert: {error}", error = e);
        }
    }

    /// Merkt sich eine geöffnete oder gesicherte Datei für „Zuletzt geöffnet“.
    fn remember(&mut self, path: &Path) {
        self.settings.remember(path);
        self.save_settings();
    }

    /// Fehler und Warnungen: Statuszeile und ein Hinweisdialog.
    fn report(&mut self, message: String) {
        // Nach einem Fehler (etwa beim Öffnen) nicht das falsche Projekt senden.
        if let Some(options) = &mut self.background.launch_options {
            options.execute = false;
        }
        self.messages.status = message.lines().next().unwrap_or_default().to_owned();
        self.messages.alert = Some(match self.messages.alert.take() {
            Some(previous) if previous != message => format!("{previous}\n\n{message}"),
            _ => message,
        });
    }

    fn busy(&self) -> bool {
        self.background.send_result.is_some()
    }

    fn modal_open(&self) -> bool {
        self.background.reading.is_some()
            || self.messages.alert.is_some()
            || self.pending.asking()
            || self.pending.import.is_some()
            || self.background.lab_offer.is_some()
    }

    /// Liest die Grafiken unter `paths` im Hintergrund, falls das noch
    /// nicht geschehen ist; danach läuft `then` mit den gelesenen Dateien.
    /// `true`, wenn der Import damit aufgeschoben ist.
    fn read_first(&mut self, ctx: &egui::Context, paths: &[PathBuf], then: Deferred) -> bool {
        if import::is_prefetched(paths.iter().map(PathBuf::as_path)) {
            return false;
        }
        if self.background.reading.is_some() {
            self.messages.status = tr("Bitte warten, bis die Dateien gelesen sind").into();
            return true;
        }
        let (sender, result) = std::sync::mpsc::channel();
        let paths = paths.to_vec();
        let repaint = ctx.clone();
        let started = std::thread::Builder::new()
            .name("visicutr-import".into())
            .stack_size(16 * 1024 * 1024)
            .spawn(move || {
                let _ = sender.send(import::prefetch(&paths));
                repaint.request_repaint();
            });
        if let Err(e) = started {
            self.report(e.to_string());
            return true;
        }
        self.background.reading = Some(Reading {
            result,
            started: std::time::Instant::now(),
            then,
            fit_policy: self.fit_policy,
        });
        self.messages.status = tr("Dateien werden gelesen …").into();
        true
    }

    /// Setzt einen aufgeschobenen Import fort, sobald seine Dateien gelesen sind.
    fn poll_reading(&mut self, ctx: &egui::Context) {
        let Some(reading) = &self.background.reading else {
            return;
        };
        // Erst nach offenen Dialogen: Eine Sende-Bestätigung gilt für den
        // Stand, den sie zeigt; das Ergebnis wartet so lange.
        if self.pending.asking()
            || self.messages.alert.is_some()
            || self.background.lab_offer.is_some()
        {
            return;
        }
        let prefetched = match reading.result.try_recv() {
            Ok(prefetched) => prefetched,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.background.reading = None;
                return self.report(tr("Import ist unerwartet abgebrochen").into());
            }
        };
        let Some(Reading {
            then, fit_policy, ..
        }) = self.background.reading.take()
        else {
            return;
        };
        import::install(prefetched);
        let fit_policy = std::mem::replace(&mut self.fit_policy, fit_policy);
        match then {
            // Mit Rückfrage, falls inzwischen ungesicherte Änderungen entstanden.
            // Neue Rückfrage nur, wenn inzwischen ungesicherte Änderungen
            // entstanden; sonst führte „Nicht sichern“ bei fehlgeschlagenem
            // Zwischenspeichern wieder in denselben Dialog.
            Deferred::Open { path, dirty } if self.file.dirty && !dirty => {
                self.request(ctx, Replace::Open(path))
            }
            Deferred::Open { path, .. } => self.perform(ctx, Replace::Open(path)),
            Deferred::Drop(paths) => self.drop_files(ctx, paths),
            Deferred::Request(request) => self.open_request(ctx, request),
            Deferred::Reload => self.reload(ctx),
        }
        self.fit_policy = fit_policy;
        import::uninstall();
    }

    fn reset_project(&mut self) {
        self.project = Project::default();
        self.project.name = tr("Neues Projekt").into();
        self.project.material.clear();
        self.devices.apply(&mut self.project);
        self.textures.active = None;
        self.textures.parts.clear();
        self.file.path = None;
        self.file.cached = None;
        self.file.dirty = false;
        self.forget_history();
    }

    /// Ein anderes Projekt ersetzt das bisherige: dessen Stände dürfen nicht
    /// mehr zurückkommen, sonst sicherte ⌘S sie unter dem neuen Dateinamen.
    fn forget_history(&mut self) {
        self.history.clear();
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            project: self.project.clone(),
            texture: self.textures.active.clone(),
            part_textures: self.textures.parts.clone(),
        }
    }

    /// Texturen aller Teile in Listenreihenfolge.
    fn all_textures(&self) -> Vec<Option<egui::TextureHandle>> {
        let mut all = self.textures.parts.clone();
        if self.project.has_artwork() {
            all.insert(
                self.project.active_part.min(all.len()),
                self.textures.active.clone(),
            );
        }
        all
    }

    /// Verteilt Texturen in Listenreihenfolge nach einer Umordnung der Teile
    /// (Wählen, Hinzufügen, Entfernen) auf das gewählte und die übrigen Teile.
    fn arrange_textures(&mut self, mut all: Vec<Option<egui::TextureHandle>>) {
        if !self.project.has_artwork() || all.is_empty() {
            self.textures.active = None;
            self.textures.parts.clear();
            return;
        }
        let active = self.project.active_part.min(all.len() - 1);
        self.textures.active = all.remove(active);
        self.textures.parts = all;
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
        let existing: Vec<(&str, &egui::TextureHandle)> = std::iter::once(&self.textures.active)
            .chain(&self.textures.parts)
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
                color_image(preview.pixmap),
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
        self.messages.status = tr!(
            "Teil „{name}“ gewählt",
            name = part_title(&self.project.part_name, index)
        );
    }

    /// Kopiert das gewählte Teil um 5 mm versetzt als neues Teil.
    fn duplicate_part(&mut self) {
        if self.textures.active.is_none() {
            return;
        }
        self.checkpoint();
        let mut all = self.all_textures();
        all.push(self.textures.active.clone());
        let mut part = self.project.active();
        part.x_mm += 5.0;
        part.y_mm += 5.0;
        self.project.add_part(part);
        self.arrange_textures(all);
        self.file.dirty = true;
        self.messages.status = tr("Teil dupliziert · Widerrufen macht es rückgängig").into();
    }

    /// Kopiert das gewählte Teil, auch als Text in die Zwischenablage des
    /// Systems: so kommt es mit ⌘V auch in ein anderes VisiCutR-Fenster.
    fn copy_part(&mut self, ctx: &egui::Context) {
        if self.textures.active.is_none() {
            return;
        }
        let part = self.project.active();
        ctx.copy_text(visicut_core::compose::part_to_clipboard(&part));
        self.messages.status = tr!(
            "Teil „{name}“ kopiert",
            name = part_title(&part.name, self.project.active_part)
        );
        self.copied = Some(part);
    }

    /// Fügt `part` (ohne: das zuletzt kopierte Teil) als neues Teil ein, an
    /// seiner Stelle (`in_place`) oder im sichtbaren Ausschnitt.
    fn paste_part(
        &mut self,
        ctx: &egui::Context,
        part: Option<visicut_core::project::Part>,
        in_place: bool,
    ) {
        let Some(part) = part.or_else(|| self.copied.clone()) else {
            return;
        };
        if self.busy() {
            return;
        }
        let view = (!in_place).then_some(self.canvas.visible_mm);
        let pasted = visicut_core::compose::paste_part(&self.project, part, view)
            .map_err(String::from)
            .and_then(|project| Ok((self.textures_for(ctx, &project)?, project)));
        match pasted {
            Ok(((texture, part_textures), project)) => {
                self.checkpoint();
                self.project = project;
                (self.textures.active, self.textures.parts) = (texture, part_textures);
                self.file.dirty = true;
                self.messages.status = tr("Teil eingefügt · Widerrufen macht es rückgängig").into();
            }
            Err(e) => self.report(e),
        }
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
        self.project.part_source_file = None;
        self.project.part_parametric = None;
        self.project.part_text = None;
        self.project.name = name;
        self.project.width_mm = preview.width_mm;
        self.project.height_mm = preview.height_mm;
        self.project.steps = steps;
        self.project.ignore_filters.clear();
        // Die neue Grafik ersetzt alle Teile.
        self.project.parts.clear();
        self.project.active_part = 0;
        self.textures.parts.clear();
        self.textures.active = Some(ctx.load_texture(
            "SVG",
            color_image(preview.pixmap),
            egui::TextureOptions::LINEAR,
        ));
        self.file.path = None;
        self.file.cached = None;
        self.file.dirty = true;
        self.forget_history();
        self.messages.status = tr("Datei importiert · Maße aus der Datei übernommen").into();
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
        self.project.part_source_file = Some(path.display().to_string());
        self.project.part_parametric = imported.parametric;
        // Ein VisiCut-Projekt (PLF) ist auf den Bettursprung bezogen, nicht
        // auf die Lage des vorigen Motivs.
        if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("plf"))
        {
            (self.project.x_mm, self.project.y_mm) = (0.0, 0.0);
        }
        // Ein zugeschnittenes Projektteil liegt dort, wo es im Projekt lag.
        self.project.x_mm += imported.offset_mm[0] as f32;
        self.project.y_mm += imported.offset_mm[1] as f32;
        if !imported.extra_parts.is_empty() {
            visicut_core::compose::add_extra_parts(&mut self.project, imported.extra_parts)?;
            (self.textures.active, self.textures.parts) = self.textures_for(ctx, &self.project)?;
        }
        self.messages.status = tr!(
            "{file} importiert · Originalmaße übernommen",
            file = path.file_name().unwrap_or_default().to_string_lossy()
        );
        self.remember(path);
        if !imported.warnings.is_empty() {
            self.messages.alert = Some(imported.warnings.join("\n"));
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
            let then = Deferred::Open {
                path: path.to_owned(),
                dirty: self.file.dirty,
            };
            if self.read_first(ctx, &[path.to_owned()], then) {
                return Ok(());
            }
            let mut imported = import::read_file(path)?;
            if let Some(whole) = visicut_core::svg_import::fit_to_content(&imported.svg) {
                match self.fit_policy {
                    Some(true) => imported.svg = whole,
                    Some(false) => {}
                    None => {
                        self.pending.import = Some(PendingImport {
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
            let mut project: Project = serde_json::from_str(&source)
                .map_err(|e| tr!("Ungültiges Projekt: {error}", error = e))?;
            // Bildordner aus der Datei: keine Netzwerkfreigaben, kein Laufwerk.
            project.replace_unsafe_image_folders(path.parent().unwrap_or(Path::new(".")));
            // Entwürfe ohne Motiv oder außerhalb des Betts lassen sich öffnen.
            project.validate_document()?;
            (self.textures.active, self.textures.parts) = self.textures_for(ctx, &project)?;
            self.project = project;
            if visicut_core::i18n::is_default_text("Material wählen", &self.project.material) {
                self.project.material.clear();
            }
            self.devices.apply(&mut self.project);
            self.forget_history();
            // Ein Zwischenstand hat noch keinen eigenen Speicherort.
            if session::is_cached(&session::dir(), path) {
                self.file.path = None;
                self.file.cached = Some(path.to_owned());
                self.file.dirty = true;
                self.messages.status =
                    tr("Ungesicherter Stand geöffnet. Zum Behalten sichern.").into();
            } else {
                self.file.path = Some(path.to_owned());
                self.file.cached = None;
                self.file.dirty = false;
                self.messages.status = tr("Projekt geöffnet").into();
            }
            self.remember(path);
            Ok(())
        }
    }

    /// Dateien aus der Befehlszeile oder von einem späteren Start: mit
    /// `--add` oder mehreren Dateien wie abgelegt, sonst ersetzt die Datei das
    /// Projekt.
    fn open_request(&mut self, ctx: &egui::Context, mut request: instance::Request) {
        if self.read_first(ctx, &request.files, Deferred::Request(request.clone())) {
            return;
        }
        let options = std::mem::take(&mut request.options);
        if !options.is_empty() {
            self.background.launch_options = Some(options);
        }
        if request.update && !self.busy() {
            request.files = self.update_parts(ctx, request.files);
            if request.files.is_empty() {
                return;
            }
        }
        match <[PathBuf; 1]>::try_from(request.files) {
            Ok([path]) if !request.add => self.request(ctx, Replace::Open(path)),
            Ok(files) => self.drop_files(ctx, files.into()),
            Err(files) if !files.is_empty() => self.drop_files(ctx, files),
            Err(_) => {}
        }
    }

    /// Erneut aus Inkscape gesendete Zeichnungen: Gibt es ein Teil gleichen
    /// Namens, wird nur dessen Grafik ersetzt (Lage, Maßstab und Zuordnung
    /// bleiben). Liefert die übrigen Dateien.
    fn update_parts(&mut self, ctx: &egui::Context, files: Vec<PathBuf>) -> Vec<PathBuf> {
        let mut rest = Vec::new();
        let mut project = self.project.clone();
        let mut updated = Vec::new();
        let mut notes = Vec::new();
        for path in files {
            match visicut_core::compose::update_part(&project, &path, self.fit_policy == Some(true))
            {
                Some(Ok((next, more))) => {
                    project = next;
                    notes.extend(more);
                    updated.push(file_name(&path));
                }
                Some(Err(e)) => notes.push(e.to_string()),
                None => rest.push(path),
            }
        }
        if !updated.is_empty() {
            self.apply_composed(ctx, project, notes);
            self.messages.status = tr!("Aktualisiert: {files}", files = updated.join(", "));
        } else if !notes.is_empty() {
            self.report(notes.join("\n"));
        }
        rest
    }

    /// Wendet Lasercutter, Material und Dicke aus der Befehlszeile an; mit
    /// `--execute` folgt die Sende-Bestätigung.
    fn apply_launch_options(&mut self, ctx: &egui::Context, options: instance::Options) {
        let mut problems = Vec::new();
        if let Some(name) = &options.device
            && let Err(e) =
                self.devices
                    .select_named(name, &mut self.project, &mut self.messages.status)
        {
            problems.push(e);
        }
        if options.material.is_some() || options.thickness_mm.is_some() {
            match jobs_ui::choose_material(
                self.jobs.active(),
                &mut self.project,
                options.material.as_deref(),
                options.thickness_mm,
            ) {
                Ok(()) => self.file.dirty = true,
                Err(e) => problems.push(e),
            }
        }
        if !problems.is_empty() {
            self.report(problems.join("\n"));
            return;
        }
        if options.execute {
            match self.project.validate() {
                Ok(()) => {
                    self.sendable = !self.busy();
                    self.send(ctx);
                }
                Err(e) => self.report(tr!("Senden nicht möglich: {error}", error = e)),
            }
        }
    }

    /// Vom Betriebssystem übergebene Dateien: eine wird geöffnet (mit
    /// Rückfrage bei ungesicherten Änderungen), mehrere wie abgelegt.
    fn open_files(&mut self, ctx: &egui::Context, files: Vec<PathBuf>) {
        self.open_request(
            ctx,
            instance::Request {
                files,
                ..Default::default()
            },
        );
    }

    /// Abgelegte Dateien: Grafiken kommen als neue Teile hinzu (ohne Motiv
    /// öffnet die erste), ein einzelnes Projekt wird geöffnet.
    fn drop_files(&mut self, ctx: &egui::Context, mut paths: Vec<PathBuf>) {
        if self.busy() {
            self.messages.status = tr("Bitte die laufende Übertragung abwarten").into();
            return;
        }
        if self.read_first(ctx, &paths, Deferred::Drop(paths.clone())) {
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
                self.pending.import = Some(PendingImport {
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
            if let Some(warnings) = self.messages.alert.take() {
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
                        self.messages.status = tr!(
                            "{count} Datei(en) als Teile hinzugefügt · Widerrufen macht es rückgängig",
                            count = added
                        );
                    }
                }
                Err(e) => notes.push(e.to_string()),
            }
        }
        if !notes.is_empty() {
            self.messages.alert = Some(notes.join("\n"));
        }
    }

    /// Sichert das Projekt; `false`, wenn der Dialog abgebrochen wurde.
    fn save(&mut self, as_copy: bool) -> Result<bool, String> {
        self.project.validate_document()?;
        let path = self.file.path.clone().filter(|_| !as_copy).or_else(|| {
            rfd::FileDialog::new()
                .set_title(tr("VisiCutR-Projekt sichern"))
                .add_filter(tr("VisiCutR Projekt"), &["vcr"])
                .set_file_name(format!("{}.vcr", self.project.name))
                .save_file()
        });
        let Some(path) = path else {
            return Ok(false);
        };
        let bytes = serde_json::to_vec_pretty(&self.project).map_err(|e| e.to_string())?;
        workspace::write_atomic(&path, &bytes)?;
        if let Some(cache) = self.file.cached.take() {
            session::discard(&session::dir(), &cache);
            let cache = std::fs::canonicalize(&cache).unwrap_or(cache);
            self.settings.recent.retain(|p| p != &cache && p.exists());
        }
        self.remember(&path);
        self.file.path = Some(path);
        self.file.dirty = false;
        self.messages.status = tr("Projekt mit eingebetteter SVG gesichert").into();
        Ok(true)
    }

    /// Führt eine Aktion aus. Ungesicherte Änderungen kommen vorher ohne
    /// Rückfrage in den Zwischenspeicher; nur wenn das scheitert, wird gefragt.
    /// Vor dem Beenden: Gerätewerte sichern, die noch auf das Verlassen
    /// eines Felds warten, und bei ungesicherter Materialbibliothek
    /// nachfragen. `false`, wenn das Beenden warten muss.
    fn settings_saved_before_quit(&mut self) -> bool {
        if !self
            .devices
            .save_draft(&mut self.project, &mut self.messages.status)
        {
            self.report(tr!(
                "Geräteliste nicht gesichert: {error}",
                error = self.messages.status
            ));
            return false;
        }
        if self.jobs.library_unsaved() {
            self.pending.library_quit = true;
            return false;
        }
        true
    }

    fn request(&mut self, ctx: &egui::Context, action: Replace) {
        if matches!(action, Replace::Quit) && !self.settings_saved_before_quit() {
            return;
        }
        if self.busy() && !matches!(action, Replace::Quit) {
            self.messages.status = tr("Bitte die laufende Übertragung abwarten").into();
        } else if self.file.dirty && !self.stash() {
            self.pending.replace = Some(action);
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
                self.file.cached = None;
                self.messages.status = tr!(
                    "Zwischenspeicher geleert ({count} Stände)",
                    count = removed.len()
                );
            }
            Err(e) => self.report(tr!("Zwischenspeicher nicht geleert: {error}", error = e)),
        }
    }

    /// Legt den ungesicherten Stand ab und führt ihn unter „Zuletzt
    /// geöffnet“.
    fn stash(&mut self) -> bool {
        match session::stash(&session::dir(), &self.project, self.file.cached.as_deref()) {
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
                self.messages.status = tr("Neues Projekt. Datei öffnen, um zu beginnen.").into();
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
                    self.messages.status =
                        tr("Beispiel geöffnet. Material und Bearbeitung auswählen.").into();
                }),
            Replace::Vectorized { svg, name } => self.import(ctx, svg, name, Vec::new()),
            Replace::Calibration(points) => self.calibration_page(ctx, &points),
            Replace::ShiftPattern(power) => self.shift_pattern(ctx, power),
            // Auch nach den Rückfragen nicht mitten in einer Übertragung.
            Replace::Quit if self.busy() => {
                self.messages.status =
                    tr("Bitte die laufende Übertragung vor dem Beenden abwarten").into();
                Ok(())
            }
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
        self.navigation.page = Page::Prepare;
        self.messages.status = tr(
            "Kalibrierseite geöffnet: Material und Markier-Parameter prüfen, senden, danach Foto aufnehmen.",
        )
        .into();
        Ok(())
    }

    /// Übernimmt das Ergebnis eines Objekt-Werkzeugs; neue Schritte erhalten
    /// die Werte der Materialbibliothek.
    /// Andere Teile bleiben erhalten; neue Teile (Hinzufügen) bringen ihre
    /// eigenen Schritte mit.
    /// Liest die Dateien aller Teile neu ein (nach Änderungen etwa in
    /// Inkscape); Lage, Maßstab und Zuordnung bleiben.
    fn reload(&mut self, ctx: &egui::Context) {
        let sources: Vec<PathBuf> = self
            .project
            .all_parts()
            .into_iter()
            .filter_map(|part| part.source_file.map(PathBuf::from))
            // `reload_parts` reports these; they are not read.
            .filter(|path| !visicut_core::svg_import::is_network_path(path))
            .collect();
        if self.read_first(ctx, &sources, Deferred::Reload) {
            return;
        }
        let (project, changed, notes) =
            visicut_core::compose::reload_parts(&self.project, self.fit_policy == Some(true));
        if changed == 0 {
            self.messages.status = tr("Neu laden: Dateien unverändert").into();
            if !notes.is_empty() {
                self.report(notes.join("\n"));
            }
            return;
        }
        self.apply_composed(ctx, project, notes);
        self.messages.status = if changed == 1 {
            tr("Ein Teil neu geladen").into()
        } else {
            tr!("{count} Teile neu geladen", count = changed)
        };
    }

    /// Neue Werte für das parametrische Teil: Grafik neu erzeugen, Lage,
    /// Maßstab und Zuordnung bleiben.
    fn set_parameters(
        &mut self,
        ctx: &egui::Context,
        values: Vec<(String, visicut_core::import::ParameterValue)>,
    ) {
        // Im Hintergrund (Skripte laufen bis zu 5 s); das Ergebnis gilt nur
        // für den Stand, aus dem es berechnet wurde.
        let project = self.project.clone();
        let identity = self.project_identity();
        let (sender, receiver) = std::sync::mpsc::channel();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let _ = sender.send(
                visicut_core::compose::set_parameters(&project, values).map_err(|e| e.to_string()),
            );
            repaint.request_repaint();
        });
        self.background.parameters = Some((identity, receiver));
        self.messages.status = tr("Grafik wird neu erzeugt …").into();
    }

    fn poll_parameters(&mut self, ctx: &egui::Context) {
        let Some((identity, receiver)) = &self.background.parameters else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Err(tr("Grafik konnte nicht erzeugt werden").into())
            }
        };
        let current = *identity == self.project_identity();
        self.background.parameters = None;
        match result {
            Ok((project, notes)) if current => self.apply_composed(ctx, project, notes),
            // Das Projekt hat sich inzwischen geändert: Ergebnis verwerfen.
            Ok(_) => self.parameters.reset(),
            Err(e) => {
                self.parameters.reset();
                self.report(e);
            }
        }
    }

    fn apply_composed(&mut self, ctx: &egui::Context, project: Project, notes: Vec<String>) {
        let textures = match self.textures_for(ctx, &project) {
            Ok(textures) => textures,
            Err(e) => return self.report(e),
        };
        self.checkpoint();
        let same_part = project.active_part == self.project.active_part
            && project.parts.len() == self.project.parts.len();
        // Ein neues Teil (etwa ein Schild) bekommt für alle Schritte das Profil.
        let new_part = project.parts.len() > self.project.parts.len();
        let old_steps = if same_part {
            self.project.steps.len()
        } else if new_part {
            0
        } else {
            project.steps.len()
        };
        let old_objects = if new_part {
            0
        } else {
            visicut_core::selection::objects(&self.project.svg)
                .map(|o| o.len())
                .unwrap_or(0)
        };
        let added = project
            .part_count()
            .saturating_sub(self.project.part_count());
        self.project = project;
        (self.textures.active, self.textures.parts) = textures;
        // Mehrere neue Teile (etwa mehrere abgelegte Dateien): jedes bekommt
        // die Bibliothekswerte, nicht nur das zuletzt hinzugefügte.
        if new_part && added > 1 {
            let (active, total) = (self.project.active_part, self.project.part_count());
            for part in total - added..total {
                if part == active {
                    continue;
                }
                self.project.select_part(part);
                for index in 0..self.project.steps.len() {
                    self.jobs.apply_preset_to(&mut self.project, index);
                }
            }
            self.project.select_part(active);
        }
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
        self.file.dirty = true;
        self.messages.status = tr!(
            "Motiv aktualisiert · {width} × {height} mm",
            width = format!("{:.0}", self.project.width_mm),
            height = format!("{:.0}", self.project.height_mm)
        );
        if !notes.is_empty() {
            self.messages.alert = Some(notes.join("\n"));
        }
    }

    /// Umrisse der erkannten Reststücke als Schneid-Teil an ihrer Stelle auf dem Bett.
    fn insert_remnants(
        &mut self,
        ctx: &egui::Context,
        remnants: &[visicut_core::remnant::Remnant],
    ) {
        match visicut_core::remnant::add_part(&self.project, remnants) {
            Ok(mut project) => {
                // Schneid-Parameter aus dem Materialprofil statt der Werte des
                // bisherigen Teils (z. B. einer Gravur).
                let preset = jobs_ui::apply_presets(self.jobs.active(), &mut project) > 0;
                self.apply_composed(ctx, project, Vec::new());
                self.messages.status = if preset {
                    tr("Reststücke als Teil eingefügt · Bearbeiten › Widerrufen macht es rückgängig")
                        .into()
                } else {
                    tr("Reststücke als Teil eingefügt; kein Schneid-Profil für dieses Material: Leistung und Geschwindigkeit prüfen")
                        .into()
                };
            }
            Err(e) => self.report(e.to_string()),
        }
    }

    fn composed(
        &mut self,
        ctx: &egui::Context,
        result: Result<Project, visicut_core::compose::ComposeError>,
    ) {
        match result {
            Ok(project) => self.apply_composed(ctx, project, Vec::new()),
            Err(e) => self.report(e.to_string()),
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
        if self.textures.active.is_none() {
            return;
        }
        self.checkpoint();
        let several = self.project.part_count() > 1;
        let mut all = self.all_textures();
        all.remove(self.project.active_part.min(all.len() - 1));
        self.project.remove_active_part();
        self.arrange_textures(all);
        self.file.dirty = true;
        self.messages.status = if several {
            tr("Teil entfernt · Widerrufen stellt es wieder her")
        } else {
            tr("Motiv entfernt · Widerrufen stellt es wieder her")
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

    /// Merkt den Stand vor einer Werkzeug- oder Löschaktion für Widerrufen;
    /// widerrufene Schritte lassen sich danach nicht mehr wiederholen.
    fn checkpoint(&mut self) {
        let snapshot = self.snapshot();
        self.history.record(snapshot);
        self.gesture.explicit = true;
    }

    /// Lage und Größe des aktiven Teils zu Beginn des Bilds, für Widerrufen
    /// von Ziehen, Anfassern und den Feldern X/Y/B/H (siehe
    /// [`Self::end_frame_geometry`]).
    fn begin_frame_geometry(&mut self, ctx: &egui::Context) {
        // Eine Geste endet, wenn keine Taste gedrückt ist und kein Feld den
        // Fokus hat; die nächste Änderung bekommt dann einen eigenen Schritt.
        let idle = !ctx.input(|i| i.pointer.any_down()) && ctx.memory(|m| m.focused().is_none());
        if idle {
            self.gesture.open = false;
        }
        self.gesture.start = Some((self.project_identity(), self.geometry()));
        self.gesture.explicit = false;
    }

    /// Hat sich in diesem Bild nur die Lage des aktiven Teils geändert und
    /// hat keine Aktion selbst einen Schritt gemerkt, wird der Stand vor der
    /// Geste gemerkt: einmal je Geste, nicht je Bild.
    fn end_frame_geometry(&mut self) {
        let Some((identity, before)) = self.gesture.start.take() else {
            return;
        };
        if self.gesture.explicit
            || self.gesture.open
            || identity != self.project_identity()
            || before == self.geometry()
        {
            return;
        }
        let mut snapshot = self.snapshot();
        let p = &mut snapshot.project;
        [p.x_mm, p.y_mm, p.width_mm, p.height_mm] = before;
        self.history.record(snapshot);
        self.gesture.open = true;
    }

    /// Dasselbe Projekt wie zu Beginn des Bilds (kein Öffnen, Widerrufen
    /// oder Werkzeug, das die Zeichnung ersetzt).
    fn project_identity(&self) -> ProjectIdentity {
        (
            self.project.svg.as_ptr() as usize,
            self.project.svg.len(),
            self.project.part_count(),
            self.project.active_part,
        )
    }

    fn geometry(&self) -> [f32; 4] {
        let p = &self.project;
        [p.x_mm, p.y_mm, p.width_mm, p.height_mm]
    }

    fn restore(&mut self, snapshot: Snapshot) {
        self.project = restored(&self.devices, snapshot.project);
        self.textures.active = snapshot.texture;
        self.textures.parts = snapshot.part_textures;
        self.file.dirty = true;
    }

    fn undo_last(&mut self) {
        if let Some(snapshot) = self.history.undo(self.snapshot()) {
            self.restore(snapshot);
            self.messages.status = tr("Letzte Änderung widerrufen").into();
        }
    }

    fn redo_last(&mut self) {
        if let Some(snapshot) = self.history.redo(self.snapshot()) {
            self.restore(snapshot);
            self.messages.status = tr("Änderung wiederhergestellt").into();
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
        self.messages.status = tr(
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
            .set_title(tr("Grafik oder VisiCutR-Projekt öffnen"))
            .add_filter(
                tr("Grafik oder VisiCutR-Projekt"),
                &[import::EXTENSIONS, &["vcr", "json"]].concat(),
            )
            .add_filter(tr("Grafik"), import::EXTENSIONS)
            .add_filter(tr("VisiCutR-Projekt"), &["vcr", "json"])
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
                self.messages.status = tr("Als VisiCut-Projekt exportiert").into();
                self.messages.alert = Some(notes.join("\n"));
            }
            Err(e) => self.report(e.to_string()),
        }
    }

    /// Der fertige Auftrag aus der Vorschau. Fehlt er, wird er im
    /// Hintergrund vorbereitet und `then` läuft danach erneut; die Oberfläche
    /// bleibt dabei bedienbar.
    fn prepared_or_start(&mut self, ctx: &egui::Context, then: Command) -> Option<PreparedJob> {
        match self.preview.prepared(&self.project) {
            Some(Ok(prepared)) => Some(prepared),
            Some(Err(e)) => {
                self.report(e);
                None
            }
            None => {
                self.preview
                    .start(ctx, &self.project, &mut self.messages.status);
                self.background.after_preview = Some(then);
                None
            }
        }
    }

    /// Export oder Senden, die auf die Vorbereitung im Hintergrund warten.
    fn poll_after_preview(&mut self, ctx: &egui::Context) {
        let Some(command) = self.background.after_preview else {
            return;
        };
        if self.preview.prepared(&self.project).is_some() {
            self.background.after_preview = None;
            self.run(ctx, command);
        } else if self.preview.outdated() {
            // Das Projekt hat sich geändert: neu vorbereiten.
            self.preview
                .start(ctx, &self.project, &mut self.messages.status);
        }
    }

    fn export(&mut self, ctx: &egui::Context) {
        if !self.sendable {
            return;
        }
        let Some(prepared) = self.prepared_or_start(ctx, Command::Export) else {
            return;
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
                Ok(()) => self.messages.status = tr!("Exportiert: {files}", files = path.display()),
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
            self.pending.overwrite = Some(pending);
        }
    }

    fn write_export(&mut self, pending: PendingExport) {
        for (path, job) in pending.targets.iter().zip(&pending.jobs) {
            if let Err(e) = workspace::write_atomic(path, &job.bytes) {
                return self.report(e);
            }
        }
        self.messages.status = tr!(
            "Exportiert: {files}",
            files = pending
                .jobs
                .iter()
                .map(|job| job.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    fn send(&mut self, ctx: &egui::Context) {
        if !self.sendable {
            return;
        }
        // Eine eben getippte Adresse gilt (siehe `commit_draft`).
        self.devices
            .commit_draft(&mut self.project, &mut self.messages.status);
        if self.project.validate().is_err() {
            return;
        }
        if let Some(prepared) = self.prepared_or_start(ctx, Command::Send) {
            self.pending.send = Some(prepared);
        }
    }

    fn transmit(&mut self, prepared: PreparedJob) {
        let host = self.project.hostname.clone();
        let port = self.project.port;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(ltt::transmit_jobs(&host, port, &prepared.jobs).map_err(String::from));
        });
        self.background.send_result = Some(rx);
        self.messages.status = tr("Job wird an den Lasercutter übertragen …").into();
    }

    fn show_settings(&mut self, page: SettingsPage) {
        self.navigation.page = Page::Settings;
        self.navigation.settings_page = page;
    }

    /// Reiterwechsel; „Vorschau“ berechnet einen veralteten Auftrag neu.
    fn switch_page(&mut self, ctx: &egui::Context, page: Page) {
        self.navigation.page = page;
        if page == Page::Preview && self.preview.outdated() && !self.project.svg.is_empty() {
            self.devices
                .commit_draft(&mut self.project, &mut self.messages.status);
            self.preview
                .request(ctx, &self.project, &mut self.messages.status);
        }
    }

    /// Dunkle Leiste wie in Bambu Studio: Öffnen und Sichern, die Reiter
    /// „Vorbereiten · Vorschau · Einstellungen“, rechts Berechnen und Senden.
    fn top_bar(&mut self, ui: &mut egui::Ui) -> Option<Command> {
        let mut clicked = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            // Datei-Menü hinter dem Haus wie in Bambu Studio, abgesetzt durch
            // einen Strich.
            let home = form::top_icon(ui, object_bar::Icon::Home).on_hover_text(tr("Datei"));
            egui::Popup::menu(&home).show(|ui| self.file_menu(ui, &mut clicked));
            let (rect, _) = ui.allocate_exact_size(Vec2::new(17.0, 40.0), Sense::hover());
            ui.painter().vline(
                rect.center().x,
                rect.y_range().shrink(10.0),
                Stroke::new(1.0, Color32::from_gray(90)),
            );
            for (page, title) in [
                (Page::Prepare, format!("✂  {}", tr("Vorbereiten"))),
                (Page::Preview, format!("👁  {}", tr("Vorschau"))),
                (Page::Settings, format!("⚙  {}", tr("Einstellungen"))),
            ] {
                if form::top_tab(ui, self.navigation.page == page, &title).clicked() {
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
                for page in SettingsPage::ALL {
                    let text = egui::RichText::new(page.title()).size(14.0);
                    if ui
                        .add_sized(
                            [ui.available_width(), 30.0],
                            egui::Button::selectable(self.navigation.settings_page == page, text),
                        )
                        .clicked()
                    {
                        self.navigation.settings_page = page;
                    }
                }
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(&ctx.style()).inner_margin(16.0))
            .show(ctx, |ui| {
                form::page_heading(ui, self.navigation.settings_page.title());
                match self.navigation.settings_page {
                    SettingsPage::Devices => {
                        self.devices
                            .settings_page(ui, &mut self.project, &mut self.messages.status)
                    }
                    SettingsPage::Materials => {
                        self.jobs.library_page(ui, &mut self.messages.status)
                    }
                    SettingsPage::Job => self.job_page(ui),
                    SettingsPage::General => self.general_page(ui),
                    SettingsPage::AddIns => self.add_ins_page(ui),
                }
            });
    }

    /// Einstellungen → Job: Schnittreihenfolge des Projekts.
    fn job_page(&mut self, ui: &mut egui::Ui) {
        // Volle Fensterbreite wie die übrigen Einstellungsseiten.
        egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
            ui.set_width(ui.available_width());
            form::section(ui, tr("Schnittreihenfolge"), |ui| {
                form::caption(
                    ui,
                    tr("Reihenfolge der Schnitt- und Markierpfade; wird mit dem Projekt gesichert."),
                );
                ui.add_space(4.0);
                let enabled = !self.busy();
                for order in CutOrder::ALL {
                    let selected = self.project.cut_order == order;
                    if ui
                        .add_enabled(
                            enabled,
                            egui::RadioButton::new(selected, jobs_ui::cut_order_title(order)),
                        )
                        .clicked()
                        && !selected
                    {
                        self.project.cut_order = order;
                        self.file.dirty = true;
                    }
                    // Erklärung unter dem Text des Knopfs eingerückt.
                    egui::Frame::new()
                        .inner_margin(egui::Margin {
                            left: 24,
                            ..Default::default()
                        })
                        .show(ui, |ui| form::caption(ui, jobs_ui::cut_order_help(order)));
                    ui.add_space(2.0);
                }
            });
            // Regeln und Ignorieren gehören wie die Reihenfolge zum Projekt; die
            // Schritte selbst stehen unter „Vorbereiten“.
            form::section(ui, tr("Regeln und Vorlagen"), |ui| {
                ui.add_enabled_ui(!self.busy(), |ui| {
                    self.jobs.rules_settings(ui, &mut self.project, &mut self.file.dirty);
                });
            });
            form::section(ui, tr("Ignorieren"), |ui| {
                ui.add_enabled_ui(!self.busy(), |ui| {
                    self.jobs.ignore_rules(ui, &mut self.project, &mut self.file.dirty);
                });
            });
            // Früher in der Objektzuordnung; gehört wie die Reihenfolge zum Projekt.
            form::section(ui, tr("Doppelte Linien"), |ui| {
                if ui
                    .add_enabled(
                        !self.busy(),
                        egui::Checkbox::new(
                            &mut self.project.remove_duplicate_lines,
                            tr("Doppelte Linien entfernen"),
                        ),
                    )
                    .changed()
                {
                    self.file.dirty = true;
                }
                egui::Frame::new()
                    .inner_margin(egui::Margin {
                        left: 24,
                        ..Default::default()
                    })
                    .show(ui, |ui| {
                        form::caption(
                            ui,
                            tr("Linien, die genau auf einer bereits geschnittenen Linie desselben \
                                Schritts liegen (z. B. gemeinsame Kanten), nur einmal schneiden; \
                                wird mit dem Projekt gesichert."),
                        )
                    });
            });
        });
    }

    /// Einstellungen → Allgemein: Sprache, Update-Prüfung und Geräteliste übertragen.
    fn general_page(&mut self, ui: &mut egui::Ui) {
        // Volle Fensterbreite wie die übrigen Einstellungsseiten.
        egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
            ui.set_width(ui.available_width());
            form::section(ui, tr("Sprache"), |ui| {
                let chosen = self
                    .settings
                    .language
                    .as_deref()
                    .and_then(i18n::Language::from_code);
                let mut picked = chosen;
                form::row(ui, tr("Oberfläche"), |ui| {
                    form::Dropdown::from_id_salt("language")
                        .width(200.0)
                        .selected_text(chosen.map_or(tr("Systemsprache"), |l| l.native_name()))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut picked, None, tr("Systemsprache"));
                            ui.separator();
                            for language in i18n::Language::ALL {
                                ui.selectable_value(
                                    &mut picked,
                                    Some(language),
                                    language.native_name(),
                                );
                            }
                        });
                });
                if picked != chosen {
                    self.settings.language = picked.map(|l| l.code().to_owned());
                    i18n::set_language(self.settings.language());
                    self.save_settings();
                }
            });
            form::section(ui, tr("Import"), |ui| {
                if ui
                    .checkbox(
                        &mut self.settings.ghostscript_for_postscript,
                        tr("EPS/PS-Dateien, die VisiCutR selbst nicht liest, mit Ghostscript öffnen"),
                    )
                    .on_hover_text(tr("Ghostscript läuft eingeschränkt (-dSAFER), hatte aber Sicherheitslücken. Nur für Dateien aus vertrauenswürdiger Quelle einschalten."))
                    .changed()
                {
                    import::allow_ghostscript_for_postscript(
                        self.settings.ghostscript_for_postscript,
                    );
                    self.save_settings();
                }
            });
            form::section(ui, tr("Updates"), |ui| {
                let mut auto = !self.settings.update_check_disabled;
                if ui
                    .checkbox(&mut auto, tr("Alle 14 Tage nach Updates suchen"))
                    .on_hover_text(tr("Fragt bei GitHub nach einem neuen Release von VisiCutR; es wird nichts automatisch installiert."))
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
                            self.background.update_check.is_none(),
                            egui::Button::new(tr("Jetzt nach Updates suchen")),
                        )
                        .clicked()
                    {
                        self.start_update_check(true);
                    }
                    if self.background.update_check.is_some() {
                        ui.spinner();
                    }
                });
            });
            if cfg!(windows) {
                self.file_types_section(ui);
            }
            form::section(ui, tr("Lasercutter"), |ui| {
                self.devices
                    .transfer(ui, &mut self.project, &mut self.messages.status);
                form::caption(ui, tr("Importieren liest VisiCut-Gerätedateien, .vcsettings-Archive und eigene Exporte; Herunterladen ruft die FabLab-Einstellungen vom Einstellungs-Server eines FabLabs ab. Neue Geräte werden sofort gesichert."));
            });
            form::section(ui, tr("Info"), |ui| {
                form::row(ui, tr("Programm"), |ui| {
                    ui.label(format!(
                        "VisiCutR {}",
                        visicut_core::update::current_version()
                    ));
                });
                form::row(ui, "GitHub", |ui| {
                    ui.hyperlink_to("github.com/dbambus/VisiCutR", visicut_core::update::REPOSITORY);
                });
                form::row(ui, tr("Lizenz"), |ui| {
                    ui.hyperlink_to(
                        tr("GNU LGPL v3 oder neuer"),
                        "https://www.gnu.org/licenses/lgpl-3.0.html",
                    );
                });
                form::caption(ui, tr("Rust-Neuentwicklung auf Grundlage von VisiCut (Thomas Oster und Mitwirkende, LGPL). Ohne Gewähr; Aufträge vor dem Starten am Gerät prüfen."));
            });
        });
    }

    /// Einstellungen → Software-Add-ins: Erweiterungen, mit denen
    /// Zeichenprogramme ihre Zeichnung direkt an VisiCutR schicken.
    fn add_ins_page(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                form::caption(
                    ui,
                    tr("Erweiterungen für Zeichenprogramme: Sie schicken die Zeichnung direkt an VisiCutR, ohne Speichern und Öffnen."),
                );
                ui.add_space(6.0);
                form::section(ui, tr("Inkscape"), |ui| self.inkscape_section(ui));
                form::section(ui, tr("Adobe Illustrator"), |ui| {
                    if ui.button(tr("Illustrator-Skript speichern …")).clicked() {
                        self.save_add_in(addins::ILLUSTRATOR);
                    }
                    form::caption(
                        ui,
                        tr(
                            "Für Illustrator CS5.1 oder neuer: schickt die Zeichnung oder die Auswahl als Teil an VisiCutR. In Illustrator mit Datei → Skripten → Anderes Skript … (Strg+F12) ausführen; im Skriptordner von Illustrator (z. B. Vorgaben/de_DE/Skripten im Programmordner) erscheint es direkt im Menü.",
                        ),
                    );
                });
                // CorelDRAW-Makros (VBA) gibt es nur unter Windows.
                if cfg!(windows) {
                    form::section(ui, tr("CorelDRAW"), |ui| {
                        if ui.button(tr("CorelDRAW-Makro speichern …")).clicked() {
                            self.save_add_in(addins::CORELDRAW);
                        }
                        form::caption(
                            ui,
                            tr(
                                "Für CorelDRAW 2017 oder neuer mit VBA: im Makro-Editor (Alt+F11) im Projekt GlobalMacros mit Datei → Datei importieren … einfügen. Danach stehen AddToVisiCutR (als Teil hinzufügen) und OpenInVisiCutR (Projekt ersetzen) unter Extras → Makros und lassen sich unter Extras → Optionen → Anpassung als Symbol ablegen.",
                            ),
                        );
                    });
                }
                form::caption(
                    ui,
                    tr(
                        "Skript und Makro finden VisiCutR über den Pfad, den VisiCutR bei jedem Start im Einstellungsordner ablegt. Nach dem Verschieben VisiCutR einmal starten.",
                    ),
                );
            });
    }

    /// Speichert das Illustrator-Skript oder CorelDRAW-Makro, wohin der
    /// Benutzer möchte; beide Programme haben keinen Ordner, den VisiCutR
    /// ohne Administratorrechte beschreiben könnte.
    fn save_add_in(&mut self, (name, content): (&str, &str)) {
        let Some(path) = rfd::FileDialog::new().set_file_name(name).save_file() else {
            return;
        };
        match std::fs::write(&path, content) {
            Ok(()) => self.messages.status = tr!("Gespeichert unter {path}", path = path.display()),
            Err(e) => self.report(tr!("Nicht gespeichert: {error}", error = e)),
        }
    }

    /// Installiert die Inkscape-Erweiterung „Add to VisiCutR“ wie das Menü
    /// „Inkscape-Erweiterung installieren“ in Java-VisiCut.
    fn inkscape_section(&mut self, ui: &mut egui::Ui) {
        let target = inkscape::extensions_dir();
        let program = std::env::current_exe().ok();
        let status = *self
            .inkscape
            .get_or_insert_with(|| match (&target, &program) {
                (Some(dir), Some(program)) => inkscape::status(dir, program),
                _ => inkscape::Status::Missing,
            });
        form::row(ui, tr("Erweiterung"), |ui| {
            ui.label(match status {
                inkscape::Status::Missing => tr("nicht installiert"),
                inkscape::Status::Outdated => tr("veraltet, bitte neu installieren"),
                inkscape::Status::Current => tr("installiert"),
            })
        });
        let label = if status == inkscape::Status::Missing {
            tr("Inkscape-Erweiterung installieren")
        } else {
            tr("Inkscape-Erweiterung neu installieren")
        };
        if ui.button(label).clicked() {
            let result = match (&target, &program) {
                (Some(dir), Some(program)) => {
                    inkscape::install(dir, program).map_err(|e| e.to_string())
                }
                _ => Err(tr("Erweiterungsordner von Inkscape unbekannt").into()),
            };
            self.inkscape = None;
            match result {
                Ok(dir) => {
                    self.messages.status = tr!(
                        "Inkscape-Erweiterung installiert in {path}. Inkscape neu starten.",
                        path = dir.display()
                    )
                }
                Err(e) => self.report(tr!(
                    "Inkscape-Erweiterung nicht installiert: {error}",
                    error = e
                )),
            }
        }
        form::caption(
            ui,
            tr(
                "In Inkscape unter Erweiterungen → Lasercut Path: „Add to VisiCutR“ fügt die Zeichnung oder Auswahl als Teil hinzu, „Open in VisiCutR“ ersetzt das Projekt. Läuft VisiCutR schon, öffnet sich die Datei dort.",
            ),
        );
    }

    /// Windows: Dateitypen der portablen EXE zuordnen und wieder entfernen.
    fn file_types_section(&mut self, ui: &mut egui::Ui) {
        let exe = std::env::current_exe().ok();
        let status = exe
            .as_deref()
            .map_or(file_types::Status::Missing, file_types::status);
        form::section(ui, tr("Dateitypen"), |ui| {
            form::row(ui, tr("Zugeordnet"), |ui| {
                ui.label(match status {
                    file_types::Status::Registered => tr("diesem Programm"),
                    file_types::Status::Elsewhere => tr("einem anderen Speicherort"),
                    file_types::Status::Missing => tr("nein"),
                })
            });
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        exe.is_some() && status != file_types::Status::Registered,
                        egui::Button::new(tr("Dateitypen zuordnen")),
                    )
                    .clicked()
                    && let Some(exe) = &exe
                {
                    match file_types::register(exe) {
                        Ok(()) => self.messages.status = tr("Dateitypen zugeordnet").into(),
                        Err(e) => self.report(e),
                    }
                }
                if ui
                    .add_enabled(
                        status != file_types::Status::Missing,
                        egui::Button::new(tr("Zuordnung entfernen")),
                    )
                    .clicked()
                {
                    match file_types::unregister() {
                        Ok(()) => {
                            self.messages.status = tr("Zuordnung der Dateitypen entfernt").into()
                        }
                        Err(e) => self.report(e),
                    }
                }
            });
            form::caption(
                ui,
                tr(
                    "VisiCutR-Projekte öffnen per Doppelklick, PLF und LaserScript, wenn noch kein anderes Programm sie öffnet. SVG, PDF, DXF und EPS stehen unter „Öffnen mit“. Gilt nur für dieses Benutzerkonto; nach dem Verschieben von VisiCutR erneut zuordnen.",
                ),
            );
        });
    }

    /// Kopf der Seitenleiste „Vorbereiten“: aktiver Lasercutter wie die
    /// Druckerkarte in Bambu Studio.
    fn device_card(&mut self, ui: &mut egui::Ui) -> Option<Command> {
        let mut clicked = None;
        form::panel_header(ui, tr("Lasercutter"), |ui| {
            if ui
                .button("⚙")
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
                    // Hostname und Port stehen in den Einstellungen, hier nur das Bett.
                    form::caption(
                        ui,
                        format!(
                            "{} × {} mm",
                            self.project.bed_width_mm, self.project.bed_height_mm
                        ),
                    );
                });
                // Rotationseinheit direkt unter dem Gerät, ohne eigenen Bereich.
                if self.devices.device().rotary_axis {
                    egui::Frame::new()
                        .inner_margin(egui::Margin::symmetric(4, 2))
                        .show(ui, |ui| {
                            self.devices
                                .rotary(ui, &mut self.project, &mut self.file.dirty);
                        });
                }
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
            Command::Reload => {
                !self.busy()
                    && self
                        .project
                        .all_parts()
                        .iter()
                        .any(|p| p.source_file.is_some())
            }
            Command::Text | Command::Shape | Command::Sign => !self.busy(),
            Command::Array
            | Command::Nest
            | Command::Delete
            | Command::Duplicate
            | Command::DuplicatePart
            | Command::Copy
            | Command::RotateLeft
            | Command::RotateRight
            | Command::RotateBy
            | Command::FlipHorizontal
            | Command::FlipVertical => self.textures.active.is_some() && !self.busy(),
            Command::OpenRecent(_) => !self.busy(),
            Command::ClearRecent => !self.settings.recent.is_empty(),
            Command::ClearCache => !session::states(&session::dir()).is_empty(),
            Command::Device(_) => !self.busy(),
            Command::Paste | Command::PasteInPlace => self.copied.is_some() && !self.busy(),
            Command::Undo => self.history.can_undo() && !self.busy(),
            Command::Redo => self.history.can_redo() && !self.busy(),
            Command::ExportPlf => self.textures.active.is_some() && !self.busy(),
            Command::ToggleCamera => self.devices.has_camera(),
            Command::RefreshCamera => self.devices.camera_shown() && !self.devices.camera_loading(),
            Command::Remnants => self.devices.has_camera() && !self.busy(),
            Command::ZoomIn => self.canvas.zoom < workspace::ZOOM_MAX,
            Command::ZoomOut => self.canvas.zoom > workspace::ZOOM_MIN,
            Command::Save
            | Command::SaveAs
            | Command::Quit
            | Command::ToggleGrid
            | Command::ZoomFit
            | Command::ZoomActual
            | Command::Manual
            | Command::Shortcuts
            | Command::ReportBug
            | Command::About
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
            Command::Reload => self.reload(ctx),
            Command::Demo => self.request(ctx, Replace::Demo),
            Command::Vectorize => self.vectorize.pick(&mut self.messages.status),
            Command::Save | Command::SaveAs => {
                if let Err(e) = self.save(command == Command::SaveAs) {
                    self.report(e);
                }
            }
            Command::Export => self.export(ctx),
            Command::ExportPlf => self.export_plf(),
            Command::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Command::Devices => self.show_settings(SettingsPage::Devices),
            Command::ToggleGrid => self.canvas.show_grid = !self.canvas.show_grid,
            Command::ToggleCamera => self.devices.toggle_camera(&self.project),
            Command::RefreshCamera => self.devices.refresh_camera(&self.project),
            Command::Remnants => self.remnants.open(&mut self.devices, &self.project),
            Command::ZoomIn => self.canvas.zoom = workspace::zoom_in(self.canvas.zoom),
            Command::ZoomOut => self.canvas.zoom = workspace::zoom_out(self.canvas.zoom),
            Command::ZoomFit => self.canvas.zoom = 1.0,
            Command::ZoomActual => {
                self.canvas.zoom =
                    workspace::actual_size_zoom(self.canvas.view_scale / self.canvas.zoom)
            }
            Command::Manual => ctx.open_url(egui::OpenUrl::new_tab(format!(
                "{}#readme",
                visicut_core::update::REPOSITORY
            ))),
            Command::ReportBug => ctx.open_url(egui::OpenUrl::new_tab(format!(
                "{}/issues/new",
                visicut_core::update::REPOSITORY
            ))),
            Command::Shortcuts => self.navigation.show_shortcuts = true,
            Command::About => self.show_settings(SettingsPage::General),
            Command::Materials => self.show_settings(SettingsPage::Materials),
            Command::Preview => self.navigation.show_preview = true,
            Command::Send => self.send(ctx),
            Command::Text => self.compose.open(compose_ui::Tool::Text),
            Command::Shape => self.compose.open(compose_ui::Tool::Shape),
            Command::Sign => self.compose.open(compose_ui::Tool::Sign),
            Command::Array => self.compose.open(compose_ui::Tool::Array),
            Command::Nest => self.compose.open(compose_ui::Tool::Nest),
            Command::Undo => self.undo_last(),
            Command::Redo => self.redo_last(),
            Command::Delete => self.delete_part(),
            Command::DuplicatePart => self.duplicate_part(),
            Command::Copy => self.copy_part(ctx),
            Command::Paste => self.paste_part(ctx, None, false),
            Command::PasteInPlace => self.paste_part(ctx, None, true),
            Command::Duplicate => {
                let result = visicut_core::compose::duplicate(&self.project, 5.0);
                self.composed(ctx, result);
            }
            Command::RotateLeft => self.transform(ctx, -90.0, false, false),
            Command::RotateRight => self.transform(ctx, 90.0, false, false),
            Command::RotateBy => {
                let angle = std::mem::take(&mut self.inspector.rotate_deg) as f64;
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
                self.pending.clear_cache = Some(session::states(&session::dir()).len());
            }
            Command::Device(index) => {
                if self
                    .devices
                    .select(index, &mut self.project, &mut self.messages.status)
                {
                    self.file.dirty = true;
                }
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
        // Auch bei offener Auswahlliste (etwa Material, dort wird getippt)
        // gehören Tasten wie Rücktaste und Pfeile der Liste, nicht dem Motiv.
        if self.busy() || ctx.memory(|m| m.focused().is_some()) || egui::Popup::is_any_open(ctx) {
            return;
        }
        // Widerrufen, Wiederholen, Löschen und Duplizieren nur ohne aktives Eingabefeld,
        // damit diese Tasten dort weiter Text bearbeiten.
        let mut pasted = None;
        let editing = ctx.input_mut(|i| {
            let mut commands = Vec::new();
            // Erst mit Umschalt prüfen: egui ignoriert eine zusätzliche Umschalttaste.
            if i.consume_key(COMMAND_SHIFT, Key::Z) || i.consume_key(Modifiers::COMMAND, Key::Y) {
                commands.push(Command::Redo);
            } else if i.consume_key(Modifiers::COMMAND, Key::Z) {
                commands.push(Command::Undo);
            }
            if i.consume_key(Modifiers::COMMAND, Key::D) {
                commands.push(Command::Duplicate);
            }
            if i.consume_key(Modifiers::COMMAND, Key::F) {
                commands.push(Command::PasteInPlace);
            }
            // ⌘C und ⌘V kommen als Ereignisse, ⌘V nur mit Text in der
            // Zwischenablage (dort liegt das kopierte Teil, siehe `copy_part`).
            for event in &i.events {
                match event {
                    egui::Event::Copy => commands.push(Command::Copy),
                    egui::Event::Paste(text) => {
                        pasted = visicut_core::compose::part_from_clipboard(text);
                    }
                    _ => {}
                }
            }
            // Die Rücktaste löscht nur auf dem Mac (dort heißt sie „Löschen“);
            // unter Windows und Linux löscht allein Entf, damit ein Vertipper
            // nicht das Teil entfernt.
            let backspace = cfg!(target_os = "macos");
            for modifiers in [Modifiers::NONE, Modifiers::COMMAND] {
                if i.consume_key(modifiers, Key::Delete)
                    || (backspace && i.consume_key(modifiers, Key::Backspace))
                {
                    commands.push(Command::Delete);
                }
            }
            commands
        });
        for command in editing {
            self.run(ctx, command);
        }
        if pasted.is_some() {
            self.paste_part(ctx, pasted, false);
        }
        // Pfeiltasten verschieben das Motiv, solange kein Eingabefeld aktiv ist.
        if self.textures.active.is_none() {
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
            self.file.dirty = true;
        }
    }

    fn menu_item(&self, ui: &mut egui::Ui, command: Command, clicked: &mut Option<Command>) {
        let mut button = egui::Button::new(command.title());
        if let Some(shortcut) = command.shortcut() {
            button = button.shortcut_text(ui.ctx().format_shortcut(&shortcut));
        }
        let checked = match command {
            Command::ToggleGrid => Some(self.canvas.show_grid),
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

    /// Inhalt des Datei-Menüs hinter dem Haus-Symbol der dunklen Leiste.
    fn file_menu(&self, ui: &mut egui::Ui, clicked: &mut Option<Command>) {
        for command in [
            Command::New,
            Command::Open,
            Command::AddPart,
            Command::Reload,
            Command::Demo,
            Command::Vectorize,
        ] {
            self.menu_item(ui, command, clicked);
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
                    *clicked = Some(command);
                }
            }
            ui.separator();
            self.menu_item(ui, Command::ClearRecent, clicked);
        });
        self.menu_item(ui, Command::ClearCache, clicked);
        ui.separator();
        for command in [
            Command::Save,
            Command::SaveAs,
            Command::Export,
            Command::ExportPlf,
        ] {
            self.menu_item(ui, command, clicked);
        }
        ui.separator();
        ui.menu_button(tr("Hilfe"), |ui| {
            for command in [
                Command::Manual,
                Command::Shortcuts,
                Command::ReportBug,
                Command::About,
            ] {
                self.menu_item(ui, command, clicked);
            }
        });
        ui.separator();
        self.menu_item(ui, Command::Quit, clicked);
    }

    /// Fenster mit allen Tastenkürzeln.
    fn shortcuts_window(&mut self, ctx: &egui::Context) {
        if !self.navigation.show_shortcuts {
            return;
        }
        let mut open = true;
        form::tool_window(
            ctx,
            "shortcuts",
            tr("Tastenkürzel"),
            360.0,
            &mut open,
            |ui| {
                egui::Grid::new("shortcut_list")
                    .num_columns(2)
                    .spacing([16.0, 4.0])
                    .striped(true)
                    .show(ui, |ui| {
                        for command in SHORTCUT_LIST {
                            if let Some(shortcut) = command.shortcut() {
                                ui.label(command.title().trim_end_matches(" …"));
                                ui.label(
                                    egui::RichText::new(ctx.format_shortcut(&shortcut)).strong(),
                                );
                                ui.end_row();
                            }
                        }
                        for (title, keys) in [
                            (
                                tr("Objekt verschieben"),
                                tr("Pfeiltasten (mit Umschalt: 10 mm)"),
                            ),
                            (tr("Zoomen"), tr("Mausrad über der Arbeitsfläche")),
                        ] {
                            ui.label(title);
                            ui.label(egui::RichText::new(keys).strong());
                            ui.end_row();
                        }
                    });
            },
        );
        self.navigation.show_shortcuts = open;
    }

    /// Seitenleiste in der Reihenfolge der macOS-Oberfläche: Teile, Material,
    /// Zuordnung und Bearbeitung (Rotationseinheit unter dem Lasercutter). Position, Größe
    /// und die Objektbefehle stehen in der Objektleiste über der Arbeitsfläche.
    fn sidebar(&mut self, ui: &mut egui::Ui) -> Option<Command> {
        let mut clicked = None;
        if self.project.has_artwork() {
            form::section(ui, tr("Teile"), |ui| self.parts_panel(ui, &mut clicked));
        }
        if let Some(parametric) = self.project.part_parametric.clone() {
            let values = form::section(ui, tr("Parameter"), |ui| {
                self.parameters.show(ui, &parametric)
            });
            if let Some(values) = values {
                self.set_parameters(ui.ctx(), values);
            }
        }
        form::section(ui, tr("Material"), |ui| {
            self.jobs
                .material(ui, &mut self.project, &mut self.file.dirty);
        });
        self.jobs
            .processing(ui, &mut self.project, &mut self.file.dirty);
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
    /// Aufklappfeld „Position und Größe“ der Objektleiste.
    fn position_panel(&mut self, ui: &mut egui::Ui) {
        let mut reference = self.inspector.reference;
        let mut dirty = false;
        let mut proportional = self.inspector.proportional;
        // Nullpunkt unten links: Y zählt wie am Gerät von der unteren
        // Bettkante nach oben (nur Anzeige, der Auftrag bleibt gleich).
        let bottom = self.devices.origin_bottom_left();
        let p = &mut self.project;
        // Wie das Transformieren-Feld in Grafikprogrammen: Bezugspunkt, X/Y,
        // B/H und rechts die Kette, die das Seitenverhältnis hält.
        let (mut new_x, mut new_y, mut new_w, mut new_h) = (None, None, None, None);
        ui.horizontal(|ui| {
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
            let field_w = 100.0;
            egui::Grid::new("position_size")
                .num_columns(4)
                // Sonst sind die Beschriftungsspalten 40 px breit.
                .min_col_width(0.0)
                .spacing([4.0, 4.0])
                .show(ui, |ui| {
                    // Zeilenweise X, Y, dann B, H: Tab springt von X nach Y.
                    ui.label(egui::RichText::new("X").color(AXIS_X));
                    if ui
                        .add_sized([field_w, 20.0], field(&mut x, -10000.0))
                        .changed()
                    {
                        new_x = Some(x);
                    }
                    ui.label(egui::RichText::new("Y").color(AXIS_Y));
                    if ui
                        .add_sized([field_w, 20.0], field(&mut y, -10000.0))
                        .changed()
                    {
                        new_y = Some(y);
                    }
                    ui.end_row();
                    ui.label(egui::RichText::new("B").color(SIZE_W))
                        .on_hover_text(tr("Breite"));
                    if ui.add_sized([field_w, 20.0], field(&mut w, 0.1)).changed() {
                        new_w = Some(w);
                    }
                    ui.label(egui::RichText::new("H").color(SIZE_H))
                        .on_hover_text(tr("Höhe"));
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
            let icon = if proportional {
                object_bar::Icon::Link
            } else {
                object_bar::Icon::Unlink
            };
            if object_bar::button(ui, icon, true, proportional, tip).clicked() {
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
        let ratio = p.height_mm / p.width_mm;
        if let Some(w) = new_w {
            let h = if proportional { w * ratio } else { p.height_mm };
            resize_about(p, reference, w, h);
            dirty = true;
        }
        if let Some(h) = new_h {
            let w = if proportional { h / ratio } else { p.width_mm };
            resize_about(p, reference, w, h);
            dirty = true;
        }
        self.inspector.reference = reference;
        self.inspector.proportional = proportional;
        self.file.dirty |= dirty;
    }

    /// Aufklappfeld „Skalieren“ der Objektleiste wie in Bambu Studio:
    /// Faktor in % (steht danach wieder auf 100 %) und Größe in mm für X und
    /// Y; „Gleichmäßig skalieren“ hält das Seitenverhältnis (dieselbe
    /// Einstellung wie die Kette unter „Position und Größe“). Der
    /// Bezugspunkt von dort bleibt stehen.
    fn scale_panel(&mut self, ui: &mut egui::Ui) {
        let mut proportional = self.inspector.proportional;
        let reference = self.inspector.reference;
        let p = &mut self.project;
        let (mut sx, mut sy) = (100.0_f32, 100.0_f32);
        let (mut w, mut h) = (p.width_mm, p.height_mm);
        let (mut new_scale, mut new_size) = (None, None);
        let field_w = 80.0;
        let height = ui.spacing().interact_size.y;
        // Rechts neben den Feldern die Kette wie unter „Position und Größe“.
        ui.horizontal(|ui| {
            let grid = egui::Grid::new("scale_size")
                .num_columns(4)
                .min_col_width(0.0)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    ui.label("");
                    for (axis, color) in [("X", AXIS_X), ("Y", AXIS_Y)] {
                        ui.vertical_centered(|ui| {
                            ui.label(egui::RichText::new(axis).color(color));
                        });
                    }
                    ui.label("");
                    ui.end_row();

                    ui.label(tr("Skalieren"));
                    // Kein Ziehen: der Faktor gilt einmal, wenn er eingetippt ist.
                    fn percent(value: &mut f32) -> egui::DragValue<'_> {
                        egui::DragValue::new(value)
                            .speed(0.0)
                            .range(0.1..=10000.0)
                            .fixed_decimals(2)
                    }
                    let hover = tr("Faktor eingeben; Enter skaliert");
                    let changed_x = ui
                        .add_sized([field_w, height], percent(&mut sx))
                        .on_hover_text(hover)
                        .changed();
                    let changed_y = ui
                        .add_sized([field_w, height], percent(&mut sy))
                        .on_hover_text(hover)
                        .changed();
                    ui.label("%");
                    ui.end_row();
                    if changed_x || changed_y {
                        let (fx, fy) = match (proportional, changed_x) {
                            (true, true) => (sx, sx),
                            (true, false) => (sy, sy),
                            (false, _) => (sx, sy),
                        };
                        new_scale = Some((fx / 100.0, fy / 100.0));
                    }

                    ui.label(tr("Größe"));
                    fn size(value: &mut f32) -> egui::DragValue<'_> {
                        egui::DragValue::new(value)
                            .speed(0.1)
                            .range(0.1..=10000.0)
                            .fixed_decimals(2)
                    }
                    let changed_w = ui.add_sized([field_w, height], size(&mut w)).changed();
                    let changed_h = ui.add_sized([field_w, height], size(&mut h)).changed();
                    ui.label("mm");
                    ui.end_row();
                    if changed_w || changed_h {
                        new_size = Some((changed_w, changed_h));
                    }
                });
            // Mittig neben den beiden Feldzeilen, nicht neben der Kopfzeile.
            let rows = 2.0 * height + ui.spacing().item_spacing.y;
            let top = (grid.response.rect.height() - rows / 2.0 - 17.0).max(0.0);
            ui.vertical(|ui| {
                ui.add_space(top);
                let (icon, tip) = if proportional {
                    (object_bar::Icon::Link, tr("Gleichmäßig skalieren"))
                } else {
                    (object_bar::Icon::Unlink, tr("Seitenverhältnis frei"))
                };
                if object_bar::button(ui, icon, true, proportional, tip).clicked() {
                    proportional = !proportional;
                }
            });
        });
        let ratio = p.height_mm / p.width_mm;
        let mut dirty = false;
        if let Some((fx, fy)) = new_scale {
            let (w, h) = (p.width_mm * fx, p.height_mm * fy);
            resize_about(p, reference, w, h);
            dirty = true;
        }
        if let Some((changed_w, _)) = new_size {
            let (w, h) = match (proportional, changed_w) {
                (true, true) => (w, w * ratio),
                (true, false) => (h / ratio, h),
                (false, _) => (w, h),
            };
            resize_about(p, reference, w, h);
            dirty = true;
        }
        self.inspector.proportional = proportional;
        self.file.dirty |= dirty;
    }

    /// Aufklappfeld „Drehen“ der Objektleiste: freier Winkel und 90°-Schritte.
    fn rotate_panel(&mut self, ui: &mut egui::Ui, clicked: &mut Option<Command>) {
        let editable = self.textures.active.is_some() && !self.busy();
        ui.horizontal(|ui| {
            ui.label(tr("Winkel"));
            // Feste Breite und kein Ziehen: anklicken, Zahl tippen, Enter dreht.
            let field = ui
                .add_sized(
                    [80.0, ui.spacing().interact_size.y],
                    egui::DragValue::new(&mut self.inspector.rotate_deg)
                        .range(-360.0..=360.0)
                        .speed(0.0)
                        .fixed_decimals(0)
                        .suffix("°"),
                )
                .on_hover_text(tr(
                    "Anklicken und Winkel eingeben; positive Winkel drehen im Uhrzeigersinn",
                ));
            let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            self.inspector.rotate_deg = self.inspector.rotate_deg.round();
            if (ui
                .add_enabled(
                    editable && self.inspector.rotate_deg != 0.0,
                    egui::Button::new("✔"),
                )
                .on_hover_text(tr("Anwenden"))
                .clicked()
                || enter)
                && editable
                && self.inspector.rotate_deg != 0.0
            {
                *clicked = Some(Command::RotateBy);
            }
        });
        ui.horizontal(|ui| {
            for (command, text) in [
                (Command::RotateLeft, "⟲ 90°"),
                (Command::RotateRight, "⟳ 90°"),
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
        if self.canvas.handle_drag.is_some() {
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
        let shown = (bw.max(bh) * self.canvas.view_scale * ctx.pixels_per_point()).max(1.0);
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
        if self.textures.live.as_ref().is_some_and(|(k, _)| *k == key) {
            return;
        }
        let texture = (self.textures.active.is_some())
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
        self.textures.live = Some((key, texture));
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
        // Berechnen steht oben in der Leiste; hier nur Zeitschätzung und Anzeige.
        if self.preview.has_toolbar() {
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(20, 8))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| self.preview.toolbar(ui));
                });
            form::divider(ui);
        }
        ui.painter()
            .rect_filled(ui.available_rect_before_wrap(), 0.0, form::CANVAS);
        ui.add_space(10.0);
        self.object_bar(ui, &mut clicked);
        if self.textures.active.is_none() && !self.devices.camera_shown() {
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
            ui.vertical(|ui| self.canvas_title(ui));
            // Gruppen von links nach rechts; was nicht neben den Titel passt,
            // wandert von rechts beginnend in das Menü „…“.
            let camera = format!("📷 {}", tr("Kamera"));
            let zoom = format!("{:.0} %", self.canvas.zoom * 100.0);
            let groups = self.view_groups(&camera, &zoom);
            let shown = fitting_groups(ui, &groups);
            let hidden: Vec<ViewGroup> = groups[shown..].iter().map(|(g, _)| *g).collect();
            let visible: Vec<ViewGroup> = groups[..shown].iter().map(|(g, _)| *g).collect();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mut chosen = Vec::new();
                if !hidden.is_empty() {
                    chosen.extend(self.more_view_options(ui, &hidden));
                }
                chosen.extend(self.view_tools(ui, &visible, &camera, &zoom));
                if let Some(command) = chosen.pop() {
                    *clicked = Some(command);
                }
            });
        });
    }

    /// Projektname und Gerät über dem Arbeitsbett.
    fn canvas_title(&mut self, ui: &mut egui::Ui) {
        // Projektname: Doppelklick zum Umbenennen, ⏎ oder Klick
        // daneben übernimmt, Esc bricht ab.
        if let Some(draft) = &mut self.file.name_draft {
            let field = ui.add(
                egui::TextEdit::singleline(draft)
                    .font(egui::FontId::proportional(20.0))
                    .desired_width(320.0),
            );
            field.request_focus();
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.file.name_draft = None;
            } else if field.lost_focus() {
                let name = draft.trim().to_owned();
                if !name.is_empty() && name != self.project.name {
                    self.project.name = name;
                    self.file.dirty = true;
                }
                self.file.name_draft = None;
            }
        } else if self.textures.active.is_some() {
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
                self.file.name_draft = Some(self.project.name.clone());
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
    }

    /// Ansichtsgruppen von links nach rechts mit den Texten ihrer Knöpfe.
    fn view_groups<'a>(&self, camera: &'a str, zoom: &'a str) -> Vec<(ViewGroup, Vec<&'a str>)> {
        let mut groups: Vec<(ViewGroup, Vec<&str>)> = Vec::new();
        if self.devices.has_camera() {
            groups.push((ViewGroup::Remnants, vec![Command::Remnants.title()]));
            let mut texts = vec![camera];
            if self.devices.camera_shown() {
                texts.push("⟳");
            }
            groups.push((ViewGroup::Camera, texts));
        }
        groups.push((ViewGroup::Grid, vec![tr("Raster")]));
        groups.push((ViewGroup::Zoom, vec!["−", zoom, "+"]));
        groups.push((ViewGroup::Actual, vec!["1:1"]));
        groups.push((ViewGroup::Fit, vec![tr("Zurücksetzen")]));
        groups
    }

    /// Menü „…“ mit den Ansichtsgruppen, die nicht in die Leiste passen.
    fn more_view_options(&self, ui: &mut egui::Ui, hidden: &[ViewGroup]) -> Option<Command> {
        let more = ui.button("…").on_hover_text(tr("Weitere Ansichtsoptionen"));
        let mut picked = None;
        egui::Popup::menu(&more).show(|ui| {
            for group in hidden {
                for command in group.commands() {
                    if *command == Command::RefreshCamera && !self.devices.camera_shown() {
                        continue;
                    }
                    self.menu_item(ui, *command, &mut picked);
                }
            }
        });
        picked
    }

    /// Knöpfe der sichtbaren Ansichtsgruppen, von rechts nach links; liefert
    /// die angeklickten Befehle.
    fn view_tools(
        &self,
        ui: &mut egui::Ui,
        visible: &[ViewGroup],
        camera: &str,
        zoom: &str,
    ) -> Vec<Command> {
        let mut chosen = Vec::new();
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
        // Umschalter wie die Symbolschalter unter macOS.
        let toggle = |ui: &mut egui::Ui, command: Command, text: &str, on: bool| {
            ui.add_enabled(self.enabled(command), egui::Button::new(text).selected(on))
                .on_hover_text(command.title())
                .clicked()
                .then_some(command)
        };
        for group in visible.iter().rev() {
            match group {
                ViewGroup::Fit => chosen.extend(tool(ui, Command::ZoomFit, tr("Zurücksetzen"))),
                ViewGroup::Actual => chosen.extend(tool(ui, Command::ZoomActual, "1:1")),
                ViewGroup::Zoom => {
                    chosen.extend(tool(ui, Command::ZoomIn, "+"));
                    ui.label(zoom);
                    chosen.extend(tool(ui, Command::ZoomOut, "−"));
                    ui.add_space(6.0);
                }
                ViewGroup::Grid => chosen.extend(toggle(
                    ui,
                    Command::ToggleGrid,
                    tr("Raster"),
                    self.canvas.show_grid,
                )),
                ViewGroup::Camera => {
                    if self.devices.camera_loading() {
                        ui.spinner();
                    } else if self.devices.camera_shown() {
                        chosen.extend(tool(ui, Command::RefreshCamera, "⟳"));
                    }
                    let shown = self.devices.camera_shown();
                    chosen.extend(toggle(ui, Command::ToggleCamera, camera, shown));
                }
                // Reststücke im Kamerabild erkennen.
                ViewGroup::Remnants => {
                    chosen.extend(tool(ui, Command::Remnants, Command::Remnants.title()))
                }
            }
        }
        chosen
    }

    /// Objektleiste wie in Bambu Studio: Hinzufügen, Transformieren,
    /// Anordnen und Bearbeiten als Symbole; Position, Größe und Drehen
    /// öffnen ein Aufklappfeld.
    fn object_bar(&mut self, ui: &mut egui::Ui, clicked: &mut Option<Command>) {
        use object_bar::Icon;
        let editable = self.textures.active.is_some() && !self.busy();
        let hover = |ui: &egui::Ui, command: Command| {
            let mut text = command.title().trim_end_matches(" …").to_owned();
            if let Some(shortcut) = command.shortcut() {
                text += &format!(" ({})", ui.ctx().format_shortcut(&shortcut));
            }
            text
        };
        object_bar::bar(ui, |ui| {
            let mut tool = |ui: &mut egui::Ui, icon: Icon, command: Command| {
                let text = hover(ui, command);
                if object_bar::button(ui, icon, self.enabled(command), false, &text).clicked() {
                    *clicked = Some(command);
                }
            };
            tool(ui, Icon::AddPart, Command::AddPart);
            tool(ui, Icon::Text, Command::Text);
            tool(ui, Icon::Shape, Command::Shape);
            tool(ui, Icon::Sign, Command::Sign);
            object_bar::separator(ui);
            // Aufklappfelder: hinterlegt, solange sie offen sind.
            let popup = |ui: &mut egui::Ui, icon: Icon, title: &str, hover: &str| {
                let id = ui.make_persistent_id(("object_bar", title));
                let open = egui::Popup::is_id_open(ui.ctx(), id);
                let response = object_bar::button(ui, icon, editable, open, hover);
                (id, response)
            };
            let moving = format!(
                "{}\n{}: {}",
                tr("Position und Größe"),
                tr("Pfeiltasten"),
                tr("1 mm je Tastendruck, mit Umschalt 10 mm")
            );
            let (position_id, position) =
                popup(ui, Icon::Transform, tr("Position und Größe"), &moving);
            let (scale_id, scale) = popup(ui, Icon::Scale, tr("Skalieren"), tr("Skalieren"));
            let (rotate_id, rotate) = popup(ui, Icon::Rotate, tr("Drehen"), tr("Drehen"));
            let mut tool = |ui: &mut egui::Ui, icon: Icon, command: Command| {
                let text = hover(ui, command);
                if object_bar::button(ui, icon, self.enabled(command), false, &text).clicked() {
                    *clicked = Some(command);
                }
            };
            tool(ui, Icon::FlipHorizontal, Command::FlipHorizontal);
            tool(ui, Icon::FlipVertical, Command::FlipVertical);
            object_bar::separator(ui);
            tool(ui, Icon::Array, Command::Array);
            tool(ui, Icon::Nest, Command::Nest);
            object_bar::separator(ui);
            tool(ui, Icon::Duplicate, Command::Duplicate);
            tool(ui, Icon::Delete, Command::Delete);
            object_bar::separator(ui);
            tool(ui, Icon::Undo, Command::Undo);
            tool(ui, Icon::Redo, Command::Redo);
            let popup_frame = egui::Frame::popup(ui.style()).inner_margin(egui::Margin::same(14));
            let panel = |response: &egui::Response, id: egui::Id| {
                egui::Popup::from_toggle_button_response(response)
                    .id(id)
                    .frame(popup_frame)
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                    .gap(6.0)
            };
            panel(&position, position_id).show(|ui| {
                ui.label(egui::RichText::new(tr("Position und Größe")).strong());
                ui.add_space(4.0);
                self.position_panel(ui);
            });
            panel(&scale, scale_id).show(|ui| {
                ui.label(egui::RichText::new(tr("Skalieren")).strong());
                ui.add_space(4.0);
                self.scale_panel(ui);
            });
            panel(&rotate, rotate_id).show(|ui| {
                ui.label(egui::RichText::new(tr("Drehen")).strong());
                ui.add_space(4.0);
                self.rotate_panel(ui, clicked);
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
        const RULER: f32 = 30.0;
        const MARGIN: f32 = 14.0;
        let bed_mm = Vec2::new(self.project.bed_width_mm, self.project.bed_height_mm);
        let available = ui.available_size() - Vec2::splat(RULER + MARGIN);
        self.wheel_zoom(ui);
        let scale = workspace::fit_scale([available.x, available.y], [bed_mm.x, bed_mm.y])
            * self.canvas.zoom;
        self.canvas.view_scale = scale;
        egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            let size = bed_mm * scale + Vec2::splat(RULER + MARGIN);
            let (rect, response) = ui.allocate_exact_size(size, Sense::click());
            self.free_area_menu(ui, &response, clicked);
            let view = BedView {
                rect: Rect::from_min_size(rect.min + Vec2::splat(RULER), bed_mm * scale),
                scale,
            };
            // Neue Text-, Form- und Schild-Teile kommen in den sichtbaren Ausschnitt.
            let shown = view.rect.intersect(ui.clip_rect());
            if shown.is_positive() {
                let min = view.to_mm(shown.min);
                let size = shown.size() / scale;
                self.canvas.visible_mm = [min[0], min[1], size.x, size.y];
            }
            let painter = ui.painter().with_clip_rect(shown);
            self.paint_bed(&painter, view);
            match self.textures.active.as_ref().map(egui::TextureHandle::id) {
                Some(texture) => self.motif_on_bed(ui, &painter, view, texture, clicked),
                None => {
                    let open = ui
                        .ctx()
                        .format_shortcut(&Command::Open.shortcut().expect("Öffnen hat ein Kürzel"));
                    painter.text(
                        view.rect.center(),
                        egui::Align2::CENTER_CENTER,
                        tr!(
                            "Datei öffnen ({shortcut}) oder hierher ziehen",
                            shortcut = open
                        ),
                        egui::FontId::proportional(22.0),
                        Color32::from_gray(145),
                    );
                }
            }
            // Zuletzt, damit die mitlaufenden Lineale über dem Motiv liegen.
            self.rulers(ui.painter(), view, ui.clip_rect(), RULER);
        });
    }

    /// Mausrad zoomt die Arbeitsfläche (nicht scrollen), solange der Zeiger darüber steht.
    fn wheel_zoom(&mut self, ui: &mut egui::Ui) {
        if !ui.rect_contains_pointer(ui.max_rect()) {
            return;
        }
        let wheel = ui.input(|i| i.raw_scroll_delta.y);
        if wheel != 0.0 {
            self.canvas.zoom = (self.canvas.zoom * (wheel * 0.002).exp())
                .clamp(workspace::ZOOM_MIN, workspace::ZOOM_MAX);
            ui.input_mut(|i| {
                i.raw_scroll_delta = Vec2::ZERO;
                i.smooth_scroll_delta = Vec2::ZERO;
            });
        }
    }

    /// Klick auf eine freie Stelle nimmt den Fokus aus Eingabefeldern;
    /// Rechtsklick fügt ein kopiertes Teil ein.
    fn free_area_menu(
        &self,
        ui: &egui::Ui,
        response: &egui::Response,
        clicked: &mut Option<Command>,
    ) {
        if response.clicked()
            && let Some(id) = ui.memory(|m| m.focused())
        {
            ui.memory_mut(|m| m.surrender_focus(id));
        }
        let can_paste = self.enabled(Command::Paste);
        response.context_menu(|ui| {
            for command in [Command::Paste, Command::PasteInPlace] {
                if ui
                    .add_enabled(can_paste, command_button(ui, command))
                    .clicked()
                {
                    *clicked = Some(command);
                }
            }
        });
    }

    /// Weißes Bett mit Kamerabild oder Raster, Rahmen und Reststücken.
    fn paint_bed(&self, painter: &egui::Painter, view: BedView) {
        let BedView { rect: bed, scale } = view;
        painter.rect_filled(bed, 0.0, Color32::WHITE);
        let camera = self.devices.paint_camera(painter, bed);
        if self.canvas.show_grid && !camera {
            let step = workspace::grid_step_mm(scale);
            // Linien an den Linealzahlen kräftiger.
            let labels = workspace::ruler_step_mm(scale);
            let line = |mm: f32| {
                let major = ((mm / labels).round() * labels - mm).abs() < step * 0.01;
                Stroke::new(0.5, Color32::from_gray(if major { 200 } else { 228 }))
            };
            for x in workspace::ruler_marks(self.project.bed_width_mm, step) {
                let px = bed.left() + x * scale;
                painter.line_segment(
                    [Pos2::new(px, bed.top()), Pos2::new(px, bed.bottom())],
                    line(x),
                );
            }
            for y in workspace::ruler_marks(self.project.bed_height_mm, step) {
                let py = self.ruler_y(view, y);
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
        self.remnants.paint(painter, bed, scale);
    }

    /// Motiv und übrige Teile auf dem Bett: zeichnen, wählen, verschieben,
    /// Kontextmenü und Anfasser.
    fn motif_on_bed(
        &mut self,
        ui: &mut egui::Ui,
        painter: &egui::Painter,
        view: BedView,
        texture: egui::TextureId,
        clicked: &mut Option<Command>,
    ) {
        let others = self.paint_parts(painter, view, texture);
        // Die Auftragsvorschau umfasst alle Teile.
        self.preview
            .overlay(painter, view.to_screen(self.project.bounds()));
        if let Some(index) = self.other_parts_clicked(ui, painter, view, &others) {
            self.select_part(index);
            return;
        }
        let p = &self.project;
        let motif = view.to_screen([p.x_mm, p.y_mm, p.width_mm, p.height_mm]);
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
        self.hovered_object(ui, painter, view);
        let response = ui
            .interact(
                motif.intersect(view.rect),
                ui.id().with("motif"),
                Sense::click_and_drag(),
            )
            .on_hover_cursor(egui::CursorIcon::Grab);
        self.motif_menu(&response, clicked);
        // Liegt unter dem gewählten Teil ein anderes, wählt ein Klick
        // dieses; sonst wäre es nicht mehr erreichbar.
        if response.clicked()
            && let Some(pos) = response.interact_pointer_pos()
            && let Some((index, _)) = others.iter().rev().find(|(_, r)| r.contains(pos))
        {
            self.select_part(*index);
            return;
        }
        if response.dragged() && !self.busy() {
            let delta = response.drag_delta() / view.scale;
            let (x, y) = (self.project.x_mm + delta.x, self.project.y_mm + delta.y);
            if workspace::place(&mut self.project, x, y) {
                self.file.dirty = true;
            }
        }
        if !self.busy() {
            self.handles(ui, motif, view, texture, clicked);
        }
    }

    /// Zeichnet das gewählte Teil über den übrigen und gibt die übrigen mit
    /// Index und Bildschirmrechteck zurück.
    fn paint_parts(
        &self,
        painter: &egui::Painter,
        view: BedView,
        texture: egui::TextureId,
    ) -> Vec<(usize, Rect)> {
        let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
        // Bearbeitung in Farbe statt der Originalbilder, schon vor der
        // Auftragsvorschau.
        let live = self.textures.live.as_ref().and_then(|(_, t)| t.as_ref());
        // Die übrigen Teile unter dem gewählten, mit dünnem grauem Rahmen.
        let others = self
            .project
            .parts
            .iter()
            .enumerate()
            .map(|(k, part)| {
                let index = k + usize::from(k >= self.project.active_part);
                let rect = view.to_screen([part.x_mm, part.y_mm, part.width_mm, part.height_mm]);
                match self.textures.parts.get(k).and_then(Option::as_ref) {
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
        let p = &self.project;
        match live {
            Some(live) => painter.image(live.id(), view.to_screen(p.bounds()), uv, Color32::WHITE),
            None => painter.image(
                texture,
                view.to_screen([p.x_mm, p.y_mm, p.width_mm, p.height_mm]),
                uv,
                Color32::WHITE,
            ),
        };
        others
    }

    /// Rahmen und Klickflächen der übrigen Teile; das angeklickte Teil.
    fn other_parts_clicked(
        &self,
        ui: &mut egui::Ui,
        painter: &egui::Painter,
        view: BedView,
        others: &[(usize, Rect)],
    ) -> Option<usize> {
        let mut select = None;
        for &(index, rect) in others {
            painter.rect_stroke(
                rect,
                0.0,
                Stroke::new(1.0, Color32::from_gray(150)),
                egui::StrokeKind::Inside,
            );
            let (name, _, _) = part_summary(&self.project, index);
            let response = ui
                .interact(
                    rect.intersect(view.rect),
                    ui.id().with(("part", index)),
                    Sense::click(),
                )
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(part_title(name, index));
            if response.clicked() {
                select = Some(index);
            }
            // Rechtsklick wählt das Teil und öffnet gleich sein Menü
            // (dasselbe wie beim gewählten Teil, siehe „motif“).
            if response.secondary_clicked() {
                select = Some(index);
                let menu = ui.id().with("motif").with("popup");
                let at = response.interact_pointer_pos();
                // Wie egui's eigenes `context_menu` (Position für den
                // Anker `PointerFixed`); einen anderen Weg gibt es nicht.
                #[allow(deprecated)]
                ui.memory_mut(|m| m.open_popup_at(menu, at));
            }
        }
        select
    }

    /// In „Einzeln“: Objekt unter der Maus auf der Arbeitsfläche finden
    /// (seine Zeile in der Liste gilt als ausgewählt) und das Objekt unter
    /// der Maus (Liste oder Arbeitsfläche) gelb umranden.
    fn hovered_object(&mut self, ui: &egui::Ui, painter: &egui::Painter, view: BedView) {
        use std::hash::{Hash, Hasher};
        let BedView { rect: bed, scale } = view;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.project.svg.hash(&mut hasher);
        self.project.width_mm.to_bits().hash(&mut hasher);
        self.project.height_mm.to_bits().hash(&mut hasher);
        let key = hasher.finish();
        let objects_mode =
            jobs_ui::assignment_mode(&self.project) == jobs_ui::AssignmentMode::Objects;
        let pointer = ui
            .input(|i| i.pointer.hover_pos())
            .filter(|p| bed.contains(*p) && ui.rect_contains_pointer(bed));
        self.jobs.canvas_hover = match pointer {
            Some(pos) if objects_mode && self.canvas.handle_drag.is_none() => {
                let current = self
                    .canvas
                    .all_outlines
                    .as_ref()
                    .is_some_and(|(k, _)| *k == key);
                if !current {
                    let pending = self
                        .canvas
                        .outlines_pending
                        .as_ref()
                        .filter(|(k, _)| *k == key);
                    match pending.map(|(_, receiver)| receiver.try_recv()) {
                        Some(Ok(outlines)) => {
                            self.canvas.all_outlines = Some((key, outlines));
                            self.canvas.outlines_pending = None;
                        }
                        Some(Err(std::sync::mpsc::TryRecvError::Empty)) => {
                            ui.ctx()
                                .request_repaint_after(std::time::Duration::from_millis(100));
                        }
                        Some(Err(std::sync::mpsc::TryRecvError::Disconnected)) => {
                            self.canvas.all_outlines = Some((key, Vec::new()));
                            self.canvas.outlines_pending = None;
                        }
                        None => {
                            let project = self.project.clone();
                            let (sender, receiver) = std::sync::mpsc::channel();
                            let repaint = ui.ctx().clone();
                            std::thread::spawn(move || {
                                let outlines = visicut_core::geometry::object_outlines(&project)
                                    .unwrap_or_default();
                                let _ = sender.send(outlines);
                                repaint.request_repaint();
                            });
                            self.canvas.outlines_pending = Some((key, receiver));
                        }
                    }
                }
                let [x, y] = view.to_mm(pos);
                let mm = [
                    f64::from(x - self.project.x_mm),
                    f64::from(y - self.project.y_mm),
                ];
                let ready = self.canvas.all_outlines.as_ref().filter(|(k, _)| *k == key);
                ready.and_then(|(_, outlines)| {
                    visicut_core::geometry::object_at(outlines, mm, f64::from(4.0 / scale))
                })
            }
            _ => None,
        };
        // Gelb: keine Bearbeitungsfarbe.
        let Some(index) = self.jobs.hovered_object.take().or(self.jobs.canvas_hover) else {
            return;
        };
        if !self
            .canvas
            .hover_outline
            .as_ref()
            .is_some_and(|(k, i, _)| *k == key && *i == index)
        {
            let outline =
                visicut_core::geometry::object_outline(&self.project, index).unwrap_or_default();
            self.canvas.hover_outline = Some((key, index, outline));
        }
        if let Some((_, _, outline)) = &self.canvas.hover_outline {
            let origin = [self.project.x_mm as f64, self.project.y_mm as f64];
            for contour in outline {
                let points: Vec<Pos2> = contour
                    .iter()
                    .map(|p| {
                        bed.min
                            + Vec2::new((origin[0] + p[0]) as f32, (origin[1] + p[1]) as f32)
                                * scale
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

    /// Kontextmenü des gewählten Teils.
    fn motif_menu(&mut self, response: &egui::Response, clicked: &mut Option<Command>) {
        let editable = !self.busy();
        let text = self.project.part_text.clone();
        let copied = self.copied.is_some();
        let mut edit_text = false;
        response.context_menu(|ui| {
            if text.is_some() {
                edit_text = ui
                    .add_enabled(editable, egui::Button::new(tr("Text bearbeiten …")))
                    .clicked();
                ui.separator();
            }
            for command in [Command::Copy, Command::Paste, Command::PasteInPlace] {
                let enabled = editable && (command == Command::Copy || copied);
                if ui
                    .add_enabled(enabled, command_button(ui, command))
                    .clicked()
                {
                    *clicked = Some(command);
                }
            }
            ui.separator();
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
        if edit_text && let Some(text) = &text {
            self.compose.edit_text(text, self.project.active_part);
        }
    }

    /// Anfasser wie in VisiCut: Ecken und Kantenmitten skalieren (Kette im
    /// Feld B/H hält das Seitenverhältnis, Umschalt kehrt das um), der Kreis
    /// über der Mitte dreht (Umschalt rastet in 15°-Schritten ein).
    fn handles(
        &mut self,
        ui: &mut egui::Ui,
        motif: Rect,
        view: BedView,
        texture: egui::TextureId,
        clicked: &mut Option<Command>,
    ) {
        let shift = ui.input(|i| i.modifiers.shift);
        self.resize_handles(ui, motif, view, shift);
        // Drehanfasser über der Mitte, bei wenig Platz darunter.
        let above = motif.top() - 22.0 >= view.rect.top();
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
        self.rotate_handle(ui, motif, knob, shift, clicked);
        let painter = ui.painter();
        painter.line_segment([edge, knob], HANDLE_STROKE);
        painter.circle(knob, 4.5, Color32::WHITE, HANDLE_STROKE);
        // Vorschau der Drehung: Motiv und Rahmen gedreht um die Mitte.
        if let Some(HandleDrag::Rotate { degrees, .. }) = self.canvas.handle_drag {
            paint_rotation(ui, motif, texture, knob, degrees);
        }
    }

    /// Anfasser an Ecken und Kantenmitten: Ziehen ändert Lage und Größe.
    fn resize_handles(&mut self, ui: &egui::Ui, motif: Rect, view: BedView, shift: bool) {
        use egui::CursorIcon;
        let painter = ui.painter();
        let p = &self.project;
        let rect_mm = [p.x_mm, p.y_mm, p.width_mm, p.height_mm];
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
                self.canvas.handle_drag = Some(HandleDrag::Resize(rect_mm));
            }
            if response.dragged()
                && let (Some(HandleDrag::Resize(start)), Some(pointer)) =
                    (self.canvas.handle_drag, response.interact_pointer_pos())
            {
                let proportional = self.inspector.proportional != shift;
                let [x, y, w, h] =
                    workspace::drag_handle(start, handle, view.to_mm(pointer), proportional);
                let p = &mut self.project;
                (p.x_mm, p.y_mm, p.width_mm, p.height_mm) = (x, y, w, h);
                self.file.dirty = true;
            }
            if response.drag_stopped() {
                self.canvas.handle_drag = None;
            }
            painter.rect(
                Rect::from_center_size(pos, Vec2::splat(7.0)),
                1.0,
                Color32::WHITE,
                HANDLE_STROKE,
                egui::StrokeKind::Middle,
            );
        }
    }

    /// Drehanfasser bei `knob`: Ziehen merkt den Winkel, Loslassen dreht wie
    /// „Drehen“ im Inspektor.
    fn rotate_handle(
        &mut self,
        ui: &egui::Ui,
        motif: Rect,
        knob: Pos2,
        shift: bool,
        clicked: &mut Option<Command>,
    ) {
        let response = ui
            .interact(
                Rect::from_center_size(knob, Vec2::splat(16.0)),
                ui.id().with("rotate"),
                Sense::drag(),
            )
            .on_hover_cursor(egui::CursorIcon::Alias)
            .on_hover_text(tr("Ziehen dreht das Motiv; mit Umschalt in 15°-Schritten"));
        if response.drag_started()
            && let Some(start) = response.interact_pointer_pos()
        {
            self.canvas.handle_drag = Some(HandleDrag::Rotate {
                start,
                degrees: 0.0,
            });
        }
        if response.dragged()
            && let (Some(HandleDrag::Rotate { start, .. }), Some(pointer)) =
                (self.canvas.handle_drag, response.interact_pointer_pos())
        {
            let center = motif.center();
            let degrees = workspace::drag_rotation(
                [center.x, center.y],
                [start.x, start.y],
                [pointer.x, pointer.y],
                shift,
            )
            // Nur ganze Grad, wie im Eingabefeld.
            .round();
            self.canvas.handle_drag = Some(HandleDrag::Rotate { start, degrees });
        }
        if response.drag_stopped() {
            if let Some(HandleDrag::Rotate { degrees, .. }) = self.canvas.handle_drag
                && degrees.abs() >= 0.1
            {
                // Dreht wie „Drehen“ im Inspektor um die Mitte.
                self.inspector.rotate_deg = degrees;
                *clicked = Some(Command::RotateBy);
            }
            self.canvas.handle_drag = None;
        }
    }

    /// Senkrechte Bildschirmposition einer Lineal- oder Rastermarke `y` mm;
    /// mit Nullpunkt unten links zählt sie von der unteren Bettkante.
    fn ruler_y(&self, view: BedView, y: f32) -> f32 {
        if self.devices.origin_bottom_left() {
            view.rect.bottom() - y * view.scale
        } else {
            view.rect.top() + y * view.scale
        }
    }

    /// Lineale mit mm-Beschriftung oben und links vom Bett. Ist das Bett
    /// hineingezoomt und gescrollt, bleiben sie am oberen und linken Rand des
    /// sichtbaren Ausschnitts (`visible`, `ruler`: Breite der Linealleiste).
    fn rulers(&self, painter: &egui::Painter, view: BedView, visible: Rect, ruler: f32) {
        let BedView { rect: bed, scale } = view;
        let step = workspace::ruler_step_mm(scale);
        let color = Color32::from_gray(110);
        let font = egui::FontId::monospace(10.0);
        let tick = Stroke::new(1.0, Color32::from_gray(160));
        // Grundlinien der Lineale: Bettkante oder, weggescrollt, Rand des Ausschnitts.
        let top = bed.top().max(visible.top() + ruler);
        let left = bed.left().max(visible.left() + ruler);
        // Hinter mitlaufenden Linealen das Motiv abdecken.
        let top_strip = Rect::from_min_max(
            Pos2::new(visible.left(), top - ruler),
            Pos2::new(visible.right(), top),
        );
        let left_strip = Rect::from_min_max(
            Pos2::new(left - ruler, visible.top()),
            Pos2::new(left, visible.bottom()),
        );
        if top > bed.top() {
            painter.rect_filled(top_strip, 0.0, form::CANVAS);
            painter.hline(top_strip.x_range(), top, tick);
        }
        if left > bed.left() {
            painter.rect_filled(left_strip, 0.0, form::CANVAS);
            painter.vline(left, left_strip.y_range(), tick);
        }
        // Marken nur rechts bzw. unterhalb der Ecke, in der sich die Lineale treffen.
        let top_painter = painter.with_clip_rect(
            Rect::from_min_max(
                Pos2::new(left - 1.0, top - ruler),
                Pos2::new(visible.right(), top),
            )
            .intersect(painter.clip_rect()),
        );
        for x in workspace::ruler_marks(self.project.bed_width_mm, step) {
            let px = bed.left() + x * scale;
            top_painter.line_segment([Pos2::new(px, top - 6.0), Pos2::new(px, top)], tick);
            top_painter.text(
                Pos2::new(px, top - 7.0),
                egui::Align2::CENTER_BOTTOM,
                workspace::ruler_label(x, step),
                font.clone(),
                color,
            );
        }
        let left_painter = painter.with_clip_rect(
            Rect::from_min_max(
                Pos2::new(left - ruler, top - 1.0),
                Pos2::new(left, visible.bottom()),
            )
            .intersect(painter.clip_rect()),
        );
        for y in workspace::ruler_marks(self.project.bed_height_mm, step) {
            let py = self.ruler_y(view, y);
            left_painter.line_segment([Pos2::new(left - 6.0, py), Pos2::new(left, py)], tick);
            left_painter.text(
                Pos2::new(left - 8.0, py),
                egui::Align2::RIGHT_CENTER,
                workspace::ruler_label(y, step),
                font.clone(),
                color,
            );
        }
        if top > bed.top() || left > bed.left() {
            painter.rect_filled(
                Rect::from_min_max(Pos2::new(left - ruler, top - ruler), Pos2::new(left, top)),
                0.0,
                form::CANVAS,
            );
        }
        painter.text(
            Pos2::new(left, top) - Vec2::splat(8.0),
            egui::Align2::RIGHT_BOTTOM,
            "mm",
            font,
            color,
        );
    }

    /// Dialoge: Hinweis, ungesicherte Änderungen, Ersetzen, Senden.
    fn dialogs(&mut self, ctx: &egui::Context) {
        self.library_quit_dialog(ctx);
        self.replace_dialog(ctx);
        self.import_overflow_dialog(ctx);
        self.overwrite_dialog(ctx);
        self.clear_cache_dialog(ctx);
        self.send_dialog(ctx);
        self.reading_dialog(ctx);
        // Hinweise zuletzt, damit sie über anderen Dialogen liegen.
        self.alert_dialog(ctx);
    }

    /// Materialbibliothek vor dem Beenden sichern?
    fn library_quit_dialog(&mut self, ctx: &egui::Context) {
        if !self.pending.library_quit {
            return;
        }
        // Some(true): sichern, Some(false): verwerfen.
        let mut choice = None;
        let modal = egui::Modal::new(egui::Id::new("library_quit")).show(ctx, |ui| {
            ui.set_max_width(380.0);
            ui.heading(tr("Änderungen an der Materialbibliothek sichern?"));
            ui.label(tr("Nicht gesicherte Änderungen gehen verloren."));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(tr("Sichern")).clicked() {
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
            Some(save) => {
                self.pending.library_quit = false;
                let saved = if save {
                    self.jobs.save_library()
                } else {
                    self.jobs.discard_library();
                    Ok(())
                };
                match saved {
                    Ok(()) => self.request(ctx, Replace::Quit),
                    Err(e) => self.report(tr!(
                        "Materialbibliothek nicht gesichert: {error}",
                        error = e
                    )),
                }
            }
            None if modal.should_close() => self.pending.library_quit = false,
            None => {}
        }
    }

    /// Ungesicherte Änderungen vor dem Ersetzen des Projekts.
    fn replace_dialog(&mut self, ctx: &egui::Context) {
        let Some(action) = self.pending.replace.take() else {
            return;
        };
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
        // Abgebrochen: Startoptionen wie `--execute` gelten dem Projekt, das
        // nicht geöffnet wurde, nicht dem alten.
        match choice {
            Some(true) => match self.save(false) {
                Ok(true) => self.perform(ctx, action),
                Ok(false) => self.background.launch_options = None,
                Err(e) => self.report(tr!("Sichern nicht möglich: {error}", error = e)),
            },
            Some(false) => self.perform(ctx, action),
            None if modal.should_close() => self.background.launch_options = None,
            None => self.pending.replace = Some(action),
        }
    }

    /// Zeichnung größer als ihre Zeichenfläche: alles oder nur die Fläche importieren.
    fn import_overflow_dialog(&mut self, ctx: &egui::Context) {
        let Some(pending) = self.pending.import.take() else {
            return;
        };
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
                // Mehrere Dateien auf ein leeres Projekt: alle hinzufügen,
                // nicht nur die erste öffnen.
                if pending.replace && pending.paths.len() == 1 {
                    if let Err(e) = self.open_path(ctx, &pending.paths[0]) {
                        self.report(e);
                    }
                } else {
                    self.drop_files(ctx, pending.paths);
                }
                self.fit_policy = None;
            }
            None if modal.should_close() => {}
            None => self.pending.import = Some(pending),
        }
    }

    /// Vorhandene LTT-Dateien beim Export ersetzen?
    fn overwrite_dialog(&mut self, ctx: &egui::Context) {
        let Some(pending) = self.pending.overwrite.take() else {
            return;
        };
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
            self.pending.overwrite = Some(pending);
        }
    }

    /// Ungesicherte Stände im Zwischenspeicher löschen?
    fn clear_cache_dialog(&mut self, ctx: &egui::Context) {
        let Some(count) = self.pending.clear_cache.take() else {
            return;
        };
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
            self.pending.clear_cache = Some(count);
        }
    }

    /// Lesen im Hintergrund, nach einer kurzen Wartezeit mit „Abbrechen“.
    /// Das Abbrechen verwirft das Ergebnis; der Lesevorgang endet spätestens
    /// mit der Importfrist.
    fn reading_dialog(&mut self, ctx: &egui::Context) {
        const DELAY: std::time::Duration = std::time::Duration::from_millis(300);
        let Some(reading) = &self.background.reading else {
            return;
        };
        if self.pending.asking() {
            return;
        }
        let waited = reading.started.elapsed();
        if waited < DELAY {
            ctx.request_repaint_after(DELAY - waited);
            return;
        }
        let mut cancel = false;
        egui::Modal::new(egui::Id::new("reading")).show(ctx, |ui| {
            ui.set_width(320.0);
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(tr("Dateien werden gelesen …"));
            });
            ui.add_space(10.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                cancel = ui.button(tr("Abbrechen")).clicked();
            });
        });
        if cancel {
            self.background.reading = None;
            self.messages.status = tr("Lesen der Dateien abgebrochen").into();
        }
    }

    /// Zusammenfassung vor dem Senden an den Lasercutter.
    fn send_dialog(&mut self, ctx: &egui::Context) {
        let Some(prepared) = self.pending.send.take() else {
            return;
        };
        let mut send = false;
        let mut cancel = false;
        // Wie der Hinweis: Rahmen ein paar Punkte innerhalb des
        // Fensterrands, in der Akzentfarbe.
        let outer = egui::Frame::popup(&ctx.style()).inner_margin(5.0);
        let modal = egui::Modal::new(egui::Id::new("send"))
            .frame(outer)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .stroke(egui::Stroke::new(2.0, form::ACCENT))
                    .corner_radius(7.0)
                    .inner_margin(egui::Margin::symmetric(18, 14))
                    .show(ui, |ui| {
                        ui.set_width(460.0);
                        self.send_summary(ui, &prepared);
                        ui.add_space(12.0);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            send = ui
                                .add(form::primary_button(&format!("✈ {}", tr("Senden"))))
                                .clicked();
                            cancel = ui
                                .add(
                                    egui::Button::new(tr("Abbrechen"))
                                        .min_size(egui::vec2(0.0, 28.0)),
                                )
                                .clicked();
                        });
                    });
            });
        if send {
            self.transmit(prepared);
        } else if !cancel && !modal.should_close() {
            self.pending.send = Some(prepared);
        }
    }

    /// Hinweis mit rotem Rahmen; schließt bei Bedarf nach einer Frist von selbst.
    fn alert_dialog(&mut self, ctx: &egui::Context) {
        let left = self
            .messages
            .alert_until
            .as_ref()
            .filter(|(_, text)| self.messages.alert.as_ref() == Some(text))
            .map(|(until, _)| until.saturating_duration_since(std::time::Instant::now()));
        if left.is_some_and(|left| left.is_zero()) {
            self.messages.alert = None;
        }
        let Some(text) = &self.messages.alert else {
            return;
        };
        let mut close = false;
        // Roter Rahmen ein paar Punkte innerhalb des Fensterrands, Inhalt
        // zentriert.
        let outer = egui::Frame::popup(&ctx.style()).inner_margin(5.0);
        let modal = egui::Modal::new(egui::Id::new("alert"))
            .frame(outer)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .stroke(egui::Stroke::new(2.0, ALERT_RED))
                    .corner_radius(7.0)
                    .inner_margin(egui::Margin::symmetric(18, 14))
                    .show(ui, |ui| {
                        ui.set_width(440.0);
                        ui.vertical_centered(|ui| {
                            let mut title = egui::text::LayoutJob::default();
                            let size = 20.0;
                            title.append(
                                "⚠  ",
                                0.0,
                                egui::TextFormat::simple(
                                    egui::FontId::proportional(size),
                                    ALERT_RED,
                                ),
                            );
                            title.append(
                                tr("Hinweis"),
                                0.0,
                                egui::TextFormat::simple(
                                    form::bold(size),
                                    ui.visuals().strong_text_color(),
                                ),
                            );
                            ui.label(title);
                            ui.add_space(6.0);
                            egui::ScrollArea::vertical()
                                .max_height(320.0)
                                .show(ui, |ui| ui.label(text));
                            ui.add_space(10.0);
                            close = ui.button(tr("OK")).clicked();
                            if let Some(left) = left {
                                let seconds = left.as_secs_f32().ceil() as u32;
                                ui.weak(tr!("schließt in {seconds} s", seconds = seconds));
                                ctx.request_repaint_after(std::time::Duration::from_millis(250));
                            }
                        });
                    });
            });
        if close || modal.should_close() {
            self.messages.alert = None;
        }
    }

    fn send_summary(&self, ui: &mut egui::Ui, prepared: &PreparedJob) {
        let count = prepared.jobs.len();
        ui.vertical_centered(|ui| {
            let size = 20.0;
            let mut title = egui::text::LayoutJob::default();
            title.append(
                "✈  ",
                0.0,
                egui::TextFormat::simple(egui::FontId::proportional(size), form::ACCENT),
            );
            title.append(
                &if count == 1 {
                    tr!("{count} Auftrag an den Lasercutter senden?", count = count)
                } else {
                    tr!("{count} Aufträge an den Lasercutter senden?", count = count)
                },
                0.0,
                egui::TextFormat::simple(form::bold(size), ui.visuals().strong_text_color()),
            );
            ui.label(title);
            ui.add_space(4.0);
            ui.weak(format!(
                "{} · {}:{}",
                self.devices.device().name,
                self.project.hostname,
                self.project.port
            ));
            ui.weak(tr!(
                "Gesamtzeit ca. {time}",
                time = workspace::duration(prepared.estimated_seconds)
            ));
        });
        ui.add_space(12.0);
        egui::ScrollArea::vertical()
            .max_height(260.0)
            .show(ui, |ui| {
                for (index, step) in prepared.steps.iter().enumerate() {
                    egui::Frame::new()
                        .fill(egui::Color32::from_gray(244))
                        .corner_radius(6.0)
                        .inner_margin(egui::Margin::symmetric(12, 8))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                let (dot, _) = ui.allocate_exact_size(
                                    egui::vec2(10.0, 10.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter().circle_filled(
                                    dot.center(),
                                    4.0,
                                    preview_ui::operation_color(step.operation),
                                );
                                ui.vertical(|ui| {
                                    // Die Datei, die am Gerät erscheint.
                                    let job = prepared
                                        .jobs
                                        .iter()
                                        .find(|job| job.operation == step.operation)
                                        .or(prepared
                                            .jobs
                                            .first()
                                            .filter(|_| prepared.jobs.len() == 1));
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 5.0;
                                        ui.strong(format!(
                                            "{}. {}",
                                            index + 1,
                                            jobs_ui::operation_title(step.operation)
                                        ))
                                        .on_hover_text(&step.description);
                                        if let Some(job) = job {
                                            ui.weak(format!("· {}", job.name));
                                        }
                                    });
                                    form::caption(ui, workspace::step_parameters(step));
                                });
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| ui.label(workspace::duration(step.estimated_seconds)),
                                );
                            });
                        });
                    ui.add_space(4.0);
                }
            });
        for warning in &prepared.warnings {
            ui.colored_label(WARNING, format!("⚠ {warning}"));
        }
    }

    fn poll_send(&mut self, ctx: &egui::Context) {
        let Some(receiver) = &self.background.send_result else {
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
        self.background.send_result = None;
        match result {
            Ok(names) => {
                self.messages.status = tr!(
                    "Übertragen: {jobs}. Aufträge am Gerät prüfen und einzeln starten.",
                    jobs = names.join(", ")
                );
                let text = self
                    .devices
                    .device()
                    .job_sent_text
                    .replace("$jobname", &names.join(", "));
                if !text.trim().is_empty() {
                    let until = std::time::Instant::now() + SENT_NOTICE_DURATION;
                    self.messages.alert_until = Some((until, text.clone()));
                    self.messages.alert = Some(text);
                }
            }
            Err(e) => self.report(tr!("Übertragung fehlgeschlagen: {error}", error = e)),
        }
    }

    #[cfg(feature = "screenshot")]
    fn capture(&mut self, ctx: &egui::Context) {
        let computing = self.preview.computing() || self.background.reading.is_some();
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
            // Ein Fenster ohne Fläche liefert kein Bild.
            let pixmap =
                resvg::tiny_skia::IntSize::from_wh(image.size[0] as u32, image.size[1] as u32)
                    .and_then(|size| resvg::tiny_skia::Pixmap::from_vec(pixels, size));
            match pixmap {
                Some(pixmap) => {
                    if let Err(error) = pixmap.save_png(path) {
                        eprintln!("{error}");
                    }
                }
                None => eprintln!("Bildschirmfoto ist leer"),
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
/// Ein mit tiny-skia gerendertes Bild als egui-Bild.
fn color_image(pixmap: resvg::tiny_skia::Pixmap) -> egui::ColorImage {
    let size = [pixmap.width() as usize, pixmap.height() as usize];
    // tiny-skia speichert vormultipliziertes RGBA; egui bekommt es entmultipliziert.
    let mut pixels = pixmap.take();
    for pixel in pixels.chunks_exact_mut(4) {
        let alpha = pixel[3] as u32;
        if alpha > 0 {
            for channel in &mut pixel[..3] {
                *channel = ((*channel as u32 * 255 + alpha / 2) / alpha).min(255) as u8;
            }
        }
    }
    egui::ColorImage::from_rgba_unmultiplied(size, &pixels)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

/// Farben der Achsenbeschriftung X und Y in den Aufklappfeldern.
const AXIS_X: Color32 = Color32::from_rgb(220, 50, 47);
const AXIS_Y: Color32 = Color32::from_rgb(40, 160, 70);
/// Breite und Höhe in eigenen Farben, damit sie nicht mit X und Y
/// verwechselt werden.
const SIZE_W: Color32 = Color32::from_rgb(30, 110, 210);
const SIZE_H: Color32 = Color32::from_rgb(140, 70, 190);

/// Neue Größe des aktiven Teils; der Bezugspunkt `reference` (Anteile von
/// Breite und Höhe) bleibt stehen.
fn resize_about(p: &mut Project, reference: [f32; 2], width: f32, height: f32) {
    let [fx, fy] = reference;
    let anchor = [p.x_mm + p.width_mm * fx, p.y_mm + p.height_mm * fy];
    p.width_mm = width.max(0.1);
    p.height_mm = height.max(0.1);
    p.x_mm = anchor[0] - p.width_mm * fx;
    p.y_mm = anchor[1] - p.height_mm * fy;
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
        form::unit_value(ui, value, range, suffix.trim(), 110.0).changed()
    })
}

/// Das Arbeitsbett auf dem Bildschirm: Rechteck und Maßstab (Punkte je mm).
#[derive(Clone, Copy)]
struct BedView {
    rect: Rect,
    scale: f32,
}

impl BedView {
    /// Rechteck `[x, y, b, h]` in mm auf dem Bildschirm.
    fn to_screen(self, [x, y, w, h]: [f32; 4]) -> Rect {
        Rect::from_min_size(
            self.rect.min + Vec2::new(x, y) * self.scale,
            Vec2::new(w, h) * self.scale,
        )
    }

    /// Bildschirmpunkt in mm vom Bettursprung.
    fn to_mm(self, pos: Pos2) -> [f32; 2] {
        let mm = (pos - self.rect.min) / self.scale;
        [mm.x, mm.y]
    }
}

/// Menüeintrag eines Befehls mit seinem Tastenkürzel.
fn command_button(ui: &egui::Ui, command: Command) -> egui::Button<'static> {
    let button = egui::Button::new(command.title());
    match command.shortcut() {
        Some(shortcut) => button.shortcut_text(ui.ctx().format_shortcut(&shortcut)),
        None => button,
    }
}

/// Gedrehtes Motiv mit gestricheltem Rahmen und Winkel neben dem
/// Drehanfasser `knob`.
fn paint_rotation(ui: &egui::Ui, motif: Rect, texture: egui::TextureId, knob: Pos2, degrees: f32) {
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

/// Rand der Anfasser.
const HANDLE_STROKE: Stroke = Stroke {
    width: 1.0,
    color: Color32::from_black_alpha(200),
};

/// Ziehen an einem Anfasser: Lage und Größe (mm) beim Start bzw. Startpunkt
/// und aktueller Winkel des Drehanfassers.
#[derive(Clone, Copy)]
enum HandleDrag {
    Resize([f32; 4]),
    Rotate { start: Pos2, degrees: f32 },
}

/// Ergebnis einer Update-Prüfung im Hintergrund.
type UpdateCheck = std::sync::mpsc::Receiver<Result<Option<visicut_core::update::Release>, String>>;

impl eframe::App for VisiCutR {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        #[cfg(feature = "screenshot")]
        self.capture(ctx);
        self.begin_frame_geometry(ctx);
        self.poll_send(ctx);
        self.poll_reading(ctx);
        self.poll_parameters(ctx);
        // Während des Lesens könnte das Ergebnis das Projekt noch ändern.
        self.sendable =
            !self.busy() && self.background.reading.is_none() && self.project.validate().is_ok();
        self.follow_device_origin();
        self.handle_close_request(ctx);
        self.take_dropped_files(ctx);
        self.take_handed_over(ctx);
        self.keyboard(ctx);
        if let Some(object) = self.jobs.take_deleted_object() {
            self.delete_objects(ctx, &[object]);
        }
        self.tool_windows(ctx);
        if self.navigation.show_preview {
            self.navigation.show_preview = false;
            self.devices
                .commit_draft(&mut self.project, &mut self.messages.status);
            self.preview
                .request(ctx, &self.project, &mut self.messages.status);
        }
        let mut commands = Vec::new();
        let send_enabled = self.sendable && !self.busy();
        self.sync_pages(ctx);
        self.poll_after_preview(ctx);
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
        self.status_bar(ctx);
        match self.navigation.page {
            Page::Prepare => commands.extend(self.prepare_page(ctx)),
            Page::Preview => commands.extend(self.preview_page(ctx, send_enabled)),
            Page::Settings => self.settings_tab(ctx),
        }
        for command in commands {
            self.run(ctx, command);
        }
        self.dialogs(ctx);
        self.end_frame_geometry();
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
            "{}{} — VisiCutR",
            self.project.name,
            if self.file.dirty { " *" } else { "" }
        )));
    }
}

impl VisiCutR {
    /// Nullpunkt unten links: Bezugspunkt standardmäßig unten links, wie
    /// am Gerät; eine eigene Wahl bleibt erhalten.
    fn follow_device_origin(&mut self) {
        let bottom = self.devices.origin_bottom_left();
        if self.origin_bottom_left != Some(bottom) {
            let (from, to) = if bottom {
                ([0.0, 0.0], [0.0, 1.0])
            } else {
                ([0.0, 1.0], [0.0, 0.0])
            };
            if self.origin_bottom_left.is_none() || self.inspector.reference == from {
                self.inspector.reference = to;
            }
            self.origin_bottom_left = Some(bottom);
        }
    }

    /// Hält das Schließen des Fensters auf, solange gesendet wird oder
    /// Einstellungen bzw. Änderungen noch nicht gesichert sind.
    fn handle_close_request(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            if self.busy() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.messages.status =
                    tr("Bitte die laufende Übertragung vor dem Beenden abwarten").into();
            } else if !self.settings_saved_before_quit() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            } else if self.file.dirty && !self.stash() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.pending.replace = Some(Replace::Quit);
            }
        }
    }

    /// Ins Fenster gezogene Dateien öffnen oder hinzufügen.
    fn take_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if !dropped.is_empty() && self.background.reading.is_some() {
            self.messages.status = tr("Bitte warten, bis die Dateien gelesen sind").into();
        } else if !dropped.is_empty() && !self.modal_open() {
            self.drop_files(ctx, dropped);
        }
    }

    /// Öffnet Dateien späterer Starts, der Befehlszeile und des Systems,
    /// sobald kein Dialog und keine Übertragung mehr wartet.
    fn take_handed_over(&mut self, ctx: &egui::Context) {
        // Ein späterer Start hat Dateien übergeben: Fenster nach vorn holen.
        // Wartet ein Dialog, bleibt die Anfrage bis zu seinem Ende liegen.
        if self.background.handed_over.is_none()
            && let Some(request) = self.background.instance.as_ref().and_then(|i| i.try_recv())
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            self.background.handed_over = Some(request);
        }
        if !self.modal_open()
            && !self.busy()
            && self.pending.import.is_none()
            && let Some(request) = self.background.handed_over.take()
        {
            self.open_request(ctx, request);
        }
        if !self.modal_open()
            && !self.busy()
            && self.pending.import.is_none()
            && self.background.handed_over.is_none()
            && let Some(options) = self.background.launch_options.take()
        {
            self.apply_launch_options(ctx, options);
        }
        if !self.modal_open() && !self.busy() {
            let opened = open_files::take();
            self.open_files(ctx, opened);
        }
    }

    /// Werkzeugfenster (Text und Formen, Vektorisieren, Reststücke, Geräte)
    /// und Dialoge zu Updates, FabLab-Einstellungen und Tastenkürzeln.
    fn tool_windows(&mut self, ctx: &egui::Context) {
        self.compose_window(ctx);
        // Die Rückfrage bei ungesicherten Änderungen stellt `request`.
        if let Some((svg, name)) = self
            .vectorize
            .windows(ctx, false, &mut self.messages.status)
        {
            self.request(ctx, Replace::Vectorized { svg, name });
        }
        if let Some(remnant_ui::Action::InsertPart(remnants)) = self.remnants.windows(
            ctx,
            &mut self.devices,
            &self.project,
            &mut self.messages.status,
        ) {
            if self.busy() {
                self.messages.status = tr("Bitte die laufende Übertragung abwarten").into();
            } else {
                self.insert_remnants(ctx, &remnants);
            }
        }
        self.remnants
            .forget_stale(self.devices.camera_image().as_ref());
        self.poll_update(ctx);
        self.update_window(ctx);
        self.poll_lab(ctx);
        self.lab_window(ctx);
        self.shortcuts_window(ctx);
        let device_action = self
            .devices
            .windows(ctx, &mut self.project, &mut self.messages.status);
        match device_action {
            Some(devices_ui::Action::CalibrationPage(points)) => {
                self.request(ctx, Replace::Calibration(points));
            }
            Some(devices_ui::Action::ShiftPattern(power)) => {
                self.request(ctx, Replace::ShiftPattern(power));
            }
            None => {}
        }
    }

    /// Fenster für Text und Formen; übernimmt das bearbeitete Projekt.
    fn compose_window(&mut self, ctx: &egui::Context) {
        match self
            .compose
            .windows(ctx, &self.project, self.canvas.visible_mm)
        {
            Some(Ok((mut project, notes))) => {
                // Bearbeiteter Text mit anderer Bearbeitung: Laserwerte aus dem
                // Profil, ohne Profil Markieren und Gravieren mit 0 %.
                if let (Some(old), Some(new)) = (&self.project.part_text, &project.part_text)
                    && project.parts.len() == self.project.parts.len()
                    && project.active_part == self.project.active_part
                    && old.operation != new.operation
                {
                    let operation = new.operation;
                    jobs_ui::select_operation(&mut project, operation);
                    jobs_ui::apply_presets(self.jobs.active(), &mut project);
                }
                self.apply_composed(ctx, project, notes)
            }
            Some(Err(e)) => self.report(e.to_string()),
            None => {}
        }
    }

    /// Vorschau, Seitenwechsel und Entwürfe der Einstellungen abgleichen.
    fn sync_pages(&mut self, ctx: &egui::Context) {
        self.preview.update(
            ctx,
            &self.project,
            &mut self.messages.status,
            self.navigation.page == Page::Preview,
        );
        // Eine angeforderte Vorschau (Menü, Kürzel, Knopf) zeigt ihren Reiter.
        if self.preview.open {
            self.preview.open = false;
            self.navigation.page = Page::Preview;
        }
        if self.navigation.page != Page::Settings {
            self.devices.close_manager_if_clean();
            self.jobs.close_library_if_clean();
        }
        if self.preview.single_job != self.settings.single_job {
            self.settings.single_job = self.preview.single_job;
            self.save_settings();
        }
    }

    /// Statuszeile unten mit Meldung und Hinweis auf Änderungen.
    fn status_bar(&self, ctx: &egui::Context) {
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
                    ui.weak(&self.messages.status);
                    if self.file.dirty {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.weak(tr("geändert"));
                        });
                    }
                });
            });
    }

    /// Reiter „Vorbereiten“: Seitenleiste links, Arbeitsfläche in der Mitte.
    fn prepare_page(&mut self, ctx: &egui::Context) -> Vec<Command> {
        let mut commands = Vec::new();
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
                egui::ScrollArea::vertical().show(ui, |ui| commands.extend(self.sidebar(ui)));
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(&ctx.style()).inner_margin(0.0))
            .show(ctx, |ui| commands.extend(self.canvas(ui)));
        commands
    }

    /// Reiter „Vorschau“; liefert „Senden“ oder „Exportieren“.
    fn preview_page(&mut self, ctx: &egui::Context, send_enabled: bool) -> Option<Command> {
        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(&ctx.style()).inner_margin(0.0))
            .show(ctx, |ui| {
                match self
                    .preview
                    .page(ui, &self.project, &mut self.messages.status, send_enabled)
                {
                    Some(preview_ui::Action::Send) => Some(Command::Send),
                    Some(preview_ui::Action::Export) => Some(Command::Export),
                    None => None,
                }
            })
            .inner
    }
}

/// VisiCut-Symbol für Fenster und Taskleiste (das der Java-Oberfläche).
fn window_icon() -> Option<egui::IconData> {
    // VisiCutR-Logo, erzeugt von scripts/icons.sh.
    let png = include_bytes!("../resources/icon/icon.png");
    let image = image::load_from_memory(png).ok()?.into_rgba8();
    Some(egui::IconData {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    })
}

/// Zählt im Hilfsprozess den Speicher und begrenzt ihn (import::worker).
#[global_allocator]
static ALLOCATOR: import::WorkerAllocator = import::WorkerAllocator;

fn main() -> eframe::Result {
    // Hilfsprozess für Skripte und PDF-Seiten aus Dateien (import::worker).
    if std::env::args().nth(1).as_deref() == Some(import::WORKER_FLAG) {
        std::process::exit(import::serve_worker());
    }
    if let Ok(executable) = std::env::current_exe() {
        import::use_worker_executable(executable);
    }
    let cwd = std::env::current_dir().unwrap_or_default();
    let launch = match instance::parse(std::env::args().skip(1), &cwd) {
        Ok(launch) => launch,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };
    if let Some(text) = &launch.print {
        println!("{text}");
        return Ok(());
    }
    // Läuft VisiCutR schon, öffnet das dortige Fenster die Dateien.
    if launch.forwards() && instance::forward(&visicut_core::device::config_dir(), &launch.request)
    {
        return Ok(());
    }
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1240.0, 820.0])
        .with_min_inner_size([900.0, 620.0])
        .with_drag_and_drop(true);
    if let Some(icon) = window_icon() {
        viewport = viewport.with_icon(icon);
    }
    open_files::install();
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "VisiCutR",
        options,
        Box::new(|cc| Ok(Box::new(VisiCutR::new(cc, launch)))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use visicut_core::project::Part;

    #[test]
    fn history_undoes_redoes_and_drops_redo_after_a_new_step() {
        let mut history = History::default();
        history.record(1);
        history.record(2);
        assert_eq!(history.undo(3), Some(2));
        assert_eq!(history.redo(2), Some(3));
        assert_eq!(history.undo(3), Some(2));
        history.record(2);
        assert!(!history.can_redo());
        for step in 0..UNDO_STEPS + 5 {
            history.record(step);
        }
        assert_eq!(history.undo.len(), UNDO_STEPS);
    }

    #[test]
    fn undo_keeps_target_and_machine_of_the_selected_device() {
        use visicut_core::device::{DeviceStore, LaserDevice};
        let a = LaserDevice::fau();
        let mut b = LaserDevice::fau();
        b.name = "B".into();
        b.hostname = "192.0.2.2".into();
        b.port = 5001;
        b.rotary_axis = false;
        b.machine.curve_acceleration_mm_s2 += 1.0;
        let mut devices = devices_ui::DeviceUi::from_store(DeviceStore {
            devices: vec![a, b.clone()],
            ..DeviceStore::default()
        });
        let mut before = Project::default();
        devices.apply(&mut before);
        before.rotary_axis = devices.device().rotary_axis;
        let mut history = History::default();
        history.record(before);
        devices.store.selected = 1;
        let mut current = Project::default();
        devices.apply(&mut current);
        let project = restored(&devices, history.undo(current).unwrap());
        assert_eq!(project.hostname, b.hostname);
        assert_eq!(project.port, b.port);
        assert_eq!(project.machine, b.machine);
        assert!(!project.rotary_axis);
    }

    #[test]
    fn window_icon_is_the_visicutr_logo() {
        let icon = window_icon().expect("VisiCutR-Logo lesbar");
        assert_eq!((icon.width, icon.height), (256, 256));
        assert_eq!(icon.rgba.len(), 256 * 256 * 4);
        // Mitte der oberen Kante: Akzentfarbe des Logos.
        let at = |x: usize, y: usize| &icon.rgba[(y * 256 + x) * 4..(y * 256 + x) * 4 + 3];
        assert_eq!(
            at(128, 20),
            [form::ACCENT.r(), form::ACCENT.g(), form::ACCENT.b()]
        );
    }

    #[test]
    fn every_listed_shortcut_is_handled() {
        // ⌘R stood in the menu and the shortcut list but did nothing.
        // Undo, Redo, Duplicate, Delete, Copy and Paste are handled outside
        // text fields.
        let editing = [
            Command::Undo,
            Command::Redo,
            Command::Duplicate,
            Command::Delete,
            Command::Copy,
            Command::Paste,
            Command::PasteInPlace,
        ];
        for command in SHORTCUT_LIST {
            assert!(command.shortcut().is_some(), "{}", command.title());
            assert!(
                Command::KEYBOARD.contains(&command) || editing.contains(&command),
                "{}",
                command.title()
            );
        }
    }

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
