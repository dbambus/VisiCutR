//! Why a file cannot be imported or written. `Display` gives the message
//! shown to the user; parts of a PLF file that fail become warnings.

use super::dxf::DxfError;
use super::eps::EpsError;
use crate::{i18n::tr, tr};
use resvg::usvg::{self, roxmltree};
use std::{fmt, io, time::Duration};

#[derive(Debug)]
pub enum ImportError {
    Io(io::Error),
    FileTooLarge,
    UnknownFormat,
    /// A ZIP archive unpacks to more than its budget.
    ArchiveTooLarge,
    Zip(zip::result::ZipError),
    /// The import as a whole ran out of time.
    TimeLimit(Duration),
    Svg(crate::svg::SvgError),
    SvgImport(crate::svg_import::SvgImportError),
    /// The XML of an SVG cannot be parsed.
    SvgXml(roxmltree::Error),
    /// An SVG without the structure it needs (no closing root tag).
    SvgStructure,
    Xml(roxmltree::Error),
    Mapping(crate::mapping::MappingError),
    Dxf(DxfError),
    Eps(EpsError),

    WorkerStart(io::Error),
    /// An error that the worker process or the thread reading ahead
    /// passed on as text.
    Reported(String),
    WorkerAborted,
    /// The worker process used too much memory or crashed.
    WorkerKilled,
    WorkerTimeout(Duration),
    WorkerOutput,
    Encoding(String),

    ImageDamaged,
    ImageDecode(image::ImageError),
    ImageEncode(image::ImageError),
    ImageTooLarge,

    GcodeTooLarge,
    GcodeEmpty,

    PdfUnreadable(io::Error),
    PostScriptUnreadable(io::Error),
    /// The built-in interpreter failed and Ghostscript is not installed.
    NeedsGhostscript(EpsError),
    NotPdf,
    PdfEncrypted,
    PdfDamaged,
    PdfWithoutPages,
    PdfTooComplex,
    PdfConversion(roxmltree::Error),
    PdfConversionFailed,
    PdfTooLarge,
    GhostscriptMissing,
    GhostscriptStart(io::Error),
    GhostscriptTimeout(Duration),
    GhostscriptFailed(io::Error),
    /// Ghostscript's first error line (or its first output).
    GhostscriptConversion(String),

    PlfNotZip(zip::result::ZipError),
    PlfTooManyEntries(usize),
    PlfDamaged(zip::result::ZipError),
    PlfEntryUnreadable {
        name: String,
        error: io::Error,
    },
    PlfEntryTooLarge(String),
    PlfTooLarge,
    PlfEmpty,
    /// No part could be imported; the warnings say why.
    PlfNothingImportable(Vec<String>),
    InvalidTransform,
    ComposeFailed(usvg::Error),
    TempDir(io::Error),
    NothingToSave,
    NoObjects,

    Parametric(Box<ImportError>),
    ParameterNotANumber {
        name: String,
        expression: String,
    },
    ParametricTooLarge,
    Expression {
        shown: String,
        error: Box<ImportError>,
    },
    EachNotAList(String),
    EachTooMany(String),
    MissingClosingBrace(String),
    UnclosedQuote(String),
    UnsupportedThymeleaf {
        expression: String,
        text: String,
    },
    UnsupportedHelper(String),
    MissingPipe(String),
    AssignmentWithoutEquals(String),
    InvalidEach(String),

    /// A JavaScript error as Boa describes it.
    Script(String),
    /// A JavaScript runtime limit (loops, recursion) as Boa describes it.
    ScriptLimit(String),
    ScriptTimeout(Duration),
    ScriptStart(io::Error),
    ScriptAborted,
    LaserScript(Box<ImportError>),
    TooManyPoints(usize),
    NoLines,

    NoMappingSet,
    MappingListMissing,
    FilterSetWithoutList,
    FilterWithoutAttribute,
    FilterWithoutValue,
    UnknownProfile(String),
    UnknownFilter(String),
    StrokeWidthNotANumber(String),
    UnsupportedFilterValue(String),
    NoMappingsInArchive,
    /// No mapping of an archive could be used; the warnings say why.
    NoUsableMappings(Vec<String>),
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::Io(error) => error.to_string(),
            Self::FileTooLarge => tr("Datei ist größer als 25 MB").to_string(),
            Self::UnknownFormat => tr("Unbekanntes Dateiformat").to_string(),
            Self::ArchiveTooLarge => tr("Archiv ist entpackt größer als erlaubt").to_string(),
            Self::Zip(error) => error.to_string(),
            Self::TimeLimit(limit) | Self::WorkerTimeout(limit) => tr!(
                "Import nach {seconds} s abgebrochen (Datei zu aufwendig)",
                seconds = limit.as_secs_f32()
            ),
            Self::Svg(error) => error.to_string(),
            Self::SvgImport(error) => error.to_string(),
            Self::SvgXml(error) => tr!("SVG konnte nicht gelesen werden: {error}", error = error),
            Self::SvgStructure => tr("SVG konnte nicht gelesen werden").to_string(),
            Self::Xml(error) => error.to_string(),
            Self::Mapping(error) => error.to_string(),
            Self::Dxf(error) => error.to_string(),
            Self::Eps(error) => error.to_string(),

            Self::WorkerStart(error) => tr!(
                "Hilfsprozess für den Import konnte nicht gestartet werden: {error}",
                error = error
            ),
            Self::Reported(text) => text.clone(),
            Self::WorkerAborted => tr("Import ist unerwartet abgebrochen").to_string(),
            Self::WorkerKilled => {
                tr("Import abgebrochen: zu viel Speicher belegt oder unerwartet beendet")
                    .to_string()
            }
            Self::WorkerOutput => "stdout".into(),
            Self::Encoding(error) => error.clone(),

            Self::ImageDamaged => image_damaged().to_string(),
            Self::ImageDecode(error) => format!("{}: {error}", image_damaged()),
            Self::ImageEncode(error) => error.to_string(),
            Self::ImageTooLarge => tr!(
                "Bild ist eingebettet größer als {size} MB; bitte verkleinern oder stärker komprimieren",
                size = crate::svg_import::MAX_SVG_BYTES / (1024 * 1024)
            ),

            Self::GcodeTooLarge => tr("G-Code ergibt eine SVG über 20 MB").to_string(),
            Self::GcodeEmpty => {
                tr("G-Code enthält keine Bahnen (G1, G2 oder G3 mit Koordinaten)").to_string()
            }

            Self::PdfUnreadable(error) => {
                tr!("PDF konnte nicht gelesen werden: {error}", error = error)
            }
            Self::PostScriptUnreadable(error) => tr!(
                "EPS/PS-Datei konnte nicht gelesen werden: {error}",
                error = error
            ),
            Self::NeedsGhostscript(reason) => format!("{reason}. {}", ghostscript_missing()),
            Self::NotPdf => tr("Datei ist keine gültige PDF-Datei").to_string(),
            Self::PdfEncrypted => tr("PDF ist verschlüsselt oder passwortgeschützt; \
                bitte eine ungeschützte Kopie speichern")
            .to_string(),
            Self::PdfDamaged => {
                tr("PDF ist beschädigt und konnte nicht gelesen werden").to_string()
            }
            Self::PdfWithoutPages => tr("PDF enthält keine Seiten").to_string(),
            Self::PdfTooComplex => {
                tr("PDF-Seite ist zu komplex (SVG größer als 20 MB)").to_string()
            }
            Self::PdfConversion(error) => {
                tr!(
                    "PDF konnte nicht umgewandelt werden: {error}",
                    error = error
                )
            }
            Self::PdfConversionFailed => tr("PDF konnte nicht umgewandelt werden").to_string(),
            Self::PdfTooLarge => tr("PDF ist größer als 100 MB").to_string(),
            Self::GhostscriptMissing => ghostscript_missing().to_string(),
            Self::GhostscriptStart(error) => tr!(
                "Ghostscript konnte nicht gestartet werden: {error}",
                error = error
            ),
            Self::GhostscriptTimeout(timeout) => tr!(
                "Ghostscript hat die EPS/PS-Datei nicht innerhalb von {seconds} Sekunden umgewandelt",
                seconds = timeout.as_secs()
            ),
            Self::GhostscriptFailed(error) => {
                tr!("Ghostscript ist fehlgeschlagen: {error}", error = error)
            }
            Self::GhostscriptConversion(detail) => tr!(
                "Ghostscript konnte die EPS/PS-Datei nicht umwandeln: {detail}",
                detail = detail
            ),

            Self::PlfNotZip(error) => tr!(
                "PLF-Datei ist kein gültiges ZIP-Archiv: {error}",
                error = error
            ),
            Self::PlfTooManyEntries(count) => {
                tr!("PLF-Datei enthält mehr als {count} Einträge", count = count)
            }
            Self::PlfDamaged(error) => tr!("PLF-Datei ist beschädigt: {error}", error = error),
            Self::PlfEntryUnreadable { name, error } => tr!(
                "„{name}“ in der PLF-Datei ist nicht lesbar: {error}",
                name = name,
                error = error
            ),
            Self::PlfEntryTooLarge(name) => tr!(
                "„{name}“ in der PLF-Datei ist größer als 25 MB",
                name = name
            ),
            Self::PlfTooLarge => tr("PLF-Datei ist entpackt größer als 100 MB").to_string(),
            Self::PlfEmpty => tr("PLF-Datei enthält keine Grafik").to_string(),
            Self::PlfNothingImportable(warnings) => tr!(
                "PLF-Datei enthält keine importierbare Grafik ({warnings})",
                warnings = warnings.join("; ")
            ),
            Self::InvalidTransform => tr("keine gültige Transformation").to_string(),
            Self::ComposeFailed(error) => tr!(
                "PLF-Grafik konnte nicht zusammengesetzt werden: {error}",
                error = error
            ),
            Self::TempDir(error) => tr!(
                "Temporärer Ordner konnte nicht angelegt werden: {error}",
                error = error
            ),
            Self::NothingToSave => tr("Kein Motiv zum Speichern").to_string(),
            Self::NoObjects => tr("SVG enthält keine Objekte").to_string(),

            Self::Parametric(error) => tr!("Parametrische SVG: {details}", details = error),
            Self::ParameterNotANumber { name, expression } => tr!(
                "Parameter „{name}“: Vorgabe „{expression}“ ist keine Zahl",
                name = name,
                expression = expression
            ),
            Self::ParametricTooLarge => tr("Parametrische SVG würde größer als 20 MB").to_string(),
            Self::Expression { shown, error } => {
                tr!("Ausdruck „{shown}“: {error}", shown = shown, error = error)
            }
            Self::EachNotAList(expression) => tr!(
                "th:each „{expression}“ ergibt keine Liste",
                expression = expression
            ),
            Self::EachTooMany(expression) => tr!(
                "th:each „{expression}“ ergibt zu viele Elemente",
                expression = expression
            ),
            Self::MissingClosingBrace(text) => {
                tr!("Ausdruck „{text}“: schließende Klammer fehlt", text = text)
            }
            Self::UnclosedQuote(text) => tr!(
                "Ausdruck „{text}“: Anführungszeichen nicht geschlossen",
                text = text
            ),
            Self::UnsupportedThymeleaf { expression, text } => tr!(
                "Thymeleaf-Ausdruck „{expression}“ in „{text}“ wird nicht unterstützt",
                expression = expression,
                text = text
            ),
            Self::UnsupportedHelper(name) => tr!(
                "Thymeleaf-Hilfsobjekt „{name}“ wird nicht unterstützt",
                name = name
            ),
            Self::MissingPipe(text) => {
                tr!("Ausdruck „{text}“: schließendes | fehlt", text = text)
            }
            Self::AssignmentWithoutEquals(part) => {
                tr!("Zuweisung „{part}“ ohne „=“", part = part)
            }
            Self::InvalidEach(text) => tr!("th:each „{text}“ ist ungültig", text = text),

            Self::Script(error) => tr!("Skriptfehler: {error}", error = error),
            Self::ScriptLimit(error) => tr!(
                "Skript abgebrochen: Laufzeitgrenze überschritten ({error})",
                error = error
            ),
            Self::ScriptTimeout(limit) => tr!(
                "Skript nach {seconds} s abgebrochen (Endlosschleife oder zu aufwendig)",
                seconds = limit.as_secs_f32()
            ),
            Self::ScriptStart(error) => {
                tr!(
                    "Skript konnte nicht gestartet werden: {error}",
                    error = error
                )
            }
            Self::ScriptAborted => tr("Skriptausführung ist unerwartet abgebrochen").to_string(),
            Self::LaserScript(error) => format!("LaserScript: {error}"),
            Self::TooManyPoints(count) => tr!(
                "LaserScript zeichnet mehr als {count} Punkte; Import abgebrochen",
                count = count
            ),
            Self::NoLines => tr("LaserScript hat keine Linien gezeichnet").to_string(),

            Self::NoMappingSet => tr("kein Zuordnungssatz").to_string(),
            Self::MappingListMissing => tr("Liste der Zuordnungen fehlt").to_string(),
            Self::FilterSetWithoutList => tr("Filtersatz ohne Liste").to_string(),
            Self::FilterWithoutAttribute => tr("Filter ohne Attribut").to_string(),
            Self::FilterWithoutValue => tr("Filter ohne Wert").to_string(),
            Self::UnknownProfile(other) => {
                tr!("Profil „{other}“ ist unbekannt", other = other)
            }
            Self::UnknownFilter(other) => tr!(
                "Filter „{other}“ ist in VisiCutR nicht bekannt",
                other = other
            ),
            Self::StrokeWidthNotANumber(text) => {
                tr!("Linienstärke „{text}“ ist keine Zahl", text = text)
            }
            Self::UnsupportedFilterValue(name) => tr!(
                "Wert des Filters „{name}“ wird nicht unterstützt",
                name = name
            ),
            Self::NoMappingsInArchive => tr("Archiv enthält keine VisiCut-Zuordnungen").to_string(),
            Self::NoUsableMappings(warnings) => warnings.join("; "),
        };
        f.write_str(&text)
    }
}

fn image_damaged() -> &'static str {
    tr("Bilddatei ist beschädigt oder hat ein nicht unterstütztes Format")
}

fn ghostscript_missing() -> &'static str {
    tr(
        "Für EPS/PS-Dateien wird Ghostscript benötigt, es wurde aber nicht gefunden. \
        Bitte Ghostscript installieren (https://ghostscript.com) oder die Datei als SVG oder \
        PDF speichern",
    )
}

impl std::error::Error for ImportError {}

impl From<io::Error> for ImportError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<crate::svg::SvgError> for ImportError {
    fn from(error: crate::svg::SvgError) -> Self {
        Self::Svg(error)
    }
}

impl From<crate::svg_import::SvgImportError> for ImportError {
    fn from(error: crate::svg_import::SvgImportError) -> Self {
        Self::SvgImport(error)
    }
}

impl From<crate::mapping::MappingError> for ImportError {
    fn from(error: crate::mapping::MappingError) -> Self {
        Self::Mapping(error)
    }
}

impl From<DxfError> for ImportError {
    fn from(error: DxfError) -> Self {
        Self::Dxf(error)
    }
}

impl From<EpsError> for ImportError {
    fn from(error: EpsError) -> Self {
        Self::Eps(error)
    }
}

impl From<ImportError> for String {
    fn from(error: ImportError) -> Self {
        error.to_string()
    }
}
