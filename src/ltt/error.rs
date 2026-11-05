//! Why a job cannot be prepared or sent. The message in the chosen language
//! comes from `Display`, so callers can tell the cases apart and still show
//! them as text.

use crate::{i18n::tr, project::Operation, tr};
use std::{fmt, io, net::SocketAddr};

#[derive(Debug)]
pub enum LttError {
    /// No material chosen, so the laser parameters are missing.
    MaterialMissing,
    /// The FAU profile only fits a 1000 × 600 mm bed.
    WrongBed,
    /// Taller than the circumference of the rotary workpiece.
    TallerThanCircumference {
        diameter_mm: f32,
    },
    /// A part fails its own check; `error` names why.
    InvalidPart {
        part: String,
        error: String,
    },
    NothingSelected,
    /// Engravings of one job with different DPI.
    MixedResolutions,
    /// A step with 0 % power.
    ZeroPower(Operation),
    /// Removing duplicate lines removed every line.
    NothingLeftAfterDuplicates,
    NoVectorPaths,
    /// A vector path leaves the bed.
    OutsideBed,
    /// A raster line (with overscan) would reach past the bed edge.
    TooCloseToEdge,
    NoDarkPixels,
    TooManyRasterPixels,
    RenderFailed,
    PreviewFailed,
    RasterPreviewFailed,
    VectorJobTooLarge,
    RasterJobTooLarge,
    OrderTooLarge,
    /// One file longer than its length field can hold.
    FileTooLarge,
    CurveNotConverging,
    CurveNoSpeedRange,
    CurveSpeedRangeInvalid,
    CurveBrakeSpeedInvalid,
    CurveDuplicatePoint,
    CurveAcceleration {
        segment: usize,
    },
    MissingAddress,
    HostLookup(io::Error),
    NoConnection,
    Connect {
        address: SocketAddr,
        error: io::Error,
    },
    /// Writing the job failed; part of it may be on the device.
    Transfer(io::Error),
    Io(io::Error),
    /// A batch stopped at `job`; the jobs in `sent` arrived before.
    PartlySent {
        job: String,
        error: Box<LttError>,
        sent: Vec<String>,
    },
    Selection(crate::selection::SelectionError),
    /// Errors from the drawing, the mapping or the project, already as text.
    Other(String),
}

impl fmt::Display for LttError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::MaterialMissing => tr(
                "Kein Material zum Lasern gewählt: unter „Material“ ein Material wählen (oder „Eigenes Material …“ mit eigener Leistung).",
            )
            .to_string(),
            Self::WrongBed => {
                tr("Das FAU-LTT-Profil benötigt ein Arbeitsbett von 1000 × 600 mm").to_string()
            }
            Self::TallerThanCircumference { diameter_mm } => tr!(
                "Motiv ist höher als der Umfang des Werkstücks ({circumference} mm bei {diameter} mm Durchmesser)",
                circumference = format!("{:.1}", std::f32::consts::PI * diameter_mm),
                diameter = format!("{diameter_mm:.1}")
            ),
            Self::InvalidPart { part, error } => format!("{part}: {error}"),
            Self::NothingSelected => tr("Keine Objekte zur Bearbeitung ausgewählt").to_string(),
            Self::MixedResolutions => tr(
                "Gravuren in einem Auftrag brauchen dieselbe Auflösung; die DPI der Gravurschritte angleichen",
            )
            .to_string(),
            Self::ZeroPower(operation) => tr!(
                "{operation}: Leistung ist 0 %; im Schritt eine Leistung eintragen (mindestens 0,1 %)",
                operation = super::operation_name(*operation)
            ),
            Self::NothingLeftAfterDuplicates => {
                tr("Nach dem Entfernen doppelter Linien bleibt nichts zu schneiden").to_string()
            }
            Self::NoVectorPaths => tr("Keine Vektorpfade zum Bearbeiten").to_string(),
            Self::OutsideBed => {
                tr("SVG-Pfad liegt außerhalb des tatsächlichen LTT-Arbeitsbetts").to_string()
            }
            Self::TooCloseToEdge => {
                tr("Gravur zu nahe am Bettrand; bitte Motiv weiter nach innen verschieben")
                    .to_string()
            }
            Self::NoDarkPixels => tr("SVG enthält keine dunklen Gravurpixel").to_string(),
            Self::TooManyRasterPixels => {
                tr("Gravur ist größer als 40 Millionen Pixel; Motiv verkleinern").to_string()
            }
            Self::RenderFailed => tr("Gravur konnte nicht gerendert werden").to_string(),
            Self::PreviewFailed => tr("Vorschau konnte nicht erstellt werden").to_string(),
            Self::RasterPreviewFailed => {
                tr("Gravurvorschau konnte nicht erstellt werden").to_string()
            }
            Self::VectorJobTooLarge => tr("Vektorjob ist größer als 256 MB").to_string(),
            Self::RasterJobTooLarge => tr("Gravurjob ist größer als 256 MB").to_string(),
            Self::OrderTooLarge => tr("Gesamtauftrag ist größer als 256 MB").to_string(),
            Self::FileTooLarge => tr("Job ist zu groß").to_string(),
            Self::CurveNotConverging => tr("Kurvenplanung konvergiert nicht").to_string(),
            Self::CurveNoSpeedRange => {
                tr("Kurvenplanung: kein gültiger Geschwindigkeitsbereich").to_string()
            }
            Self::CurveSpeedRangeInvalid => {
                tr("Kurvenplanung: Geschwindigkeitsbereich ungültig").to_string()
            }
            Self::CurveBrakeSpeedInvalid => {
                tr("Kurvenplanung: Bremsgeschwindigkeit ungültig").to_string()
            }
            Self::CurveDuplicatePoint => tr("Kurve enthält einen doppelten Punkt").to_string(),
            Self::CurveAcceleration { segment } => tr!(
                "Beschleunigung im Kurvensegment {segment} zu hoch",
                segment = segment
            ),
            Self::MissingAddress => tr("Hostname und Port müssen gesetzt sein").to_string(),
            Self::HostLookup(error) => tr!("Hostname nicht erreichbar: {error}", error = error),
            Self::NoConnection => tr("Keine Verbindung zum LTT möglich").to_string(),
            Self::Connect { address, error } => connect_message(*address, error),
            Self::Transfer(error) => tr!(
                "Übertragung fehlgeschlagen; Teiljob könnte am Gerät liegen: {error}",
                error = error
            ),
            Self::Io(error) => error.to_string(),
            Self::PartlySent { job, error, sent } => tr!(
                "Übertragung von {job} fehlgeschlagen: {error}\nBereits übertragen: {sent}. Dieser Job kann teilweise am Gerät liegen. Weitere Aufträge wurden nicht gesendet; vor erneutem Senden Gerätespeicher prüfen.",
                job = job,
                error = error,
                sent = if sent.is_empty() {
                    tr("keine").to_string()
                } else {
                    sent.join(", ")
                }
            ),
            Self::Selection(error) => error.to_string(),
            Self::Other(text) => text.clone(),
        };
        f.write_str(&text)
    }
}

/// Names the address and the system error. macOS rejects connections into
/// the local network with EHOSTUNREACH while the app lacks the "Local
/// Network" privacy permission, even though the cutter answers.
fn connect_message(address: SocketAddr, error: &io::Error) -> String {
    let message = tr!(
        "Keine Verbindung zu {address} möglich: {error}",
        address = address,
        error = error
    );
    if cfg!(target_os = "macos") && error.kind() == io::ErrorKind::HostUnreachable {
        format!(
            "{message}\n{}",
            tr(
                "Ist der Lasercutter eingeschaltet, blockiert macOS vermutlich den Zugriff auf das lokale Netzwerk. Unter Systemeinstellungen → Datenschutz & Sicherheit → Lokales Netzwerk VisiCutR erlauben und erneut senden."
            )
        )
    } else {
        message
    }
}

impl std::error::Error for LttError {}

impl From<String> for LttError {
    fn from(text: String) -> Self {
        Self::Other(text)
    }
}

impl From<crate::selection::SelectionError> for LttError {
    fn from(error: crate::selection::SelectionError) -> Self {
        Self::Selection(error)
    }
}

impl From<io::Error> for LttError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<LttError> for String {
    fn from(error: LttError) -> Self {
        error.to_string()
    }
}
