use crate::{i18n::tr, tr};
use std::fmt;

#[derive(Debug)]
pub enum DxfError {
    Io(std::io::Error),
    NotDxf,
    /// The line of an ASCII file that is no group code.
    Damaged(usize),
    BinaryDamaged,
    TooManyObjects,
    TooManyBlockReferences,
    NothingToShow,
    /// Width and height in mm, already formatted.
    TooLarge {
        width: String,
        height: String,
    },
}

impl fmt::Display for DxfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::Io(error) => error.to_string(),
            Self::NotDxf => tr("Keine gültige DXF-Datei").to_string(),
            Self::Damaged(line) => tr!("DXF-Datei ist beschädigt (Zeile {line})", line = line),
            Self::BinaryDamaged => tr("Binäre DXF-Datei ist beschädigt").to_string(),
            Self::TooManyObjects => tr("DXF-Datei enthält zu viele Objekte").to_string(),
            Self::TooManyBlockReferences => {
                tr("DXF-Datei enthält zu viele Blockreferenzen").to_string()
            }
            Self::NothingToShow => tr("DXF-Datei enthält keine darstellbaren Objekte").to_string(),
            Self::TooLarge { width, height } => tr!(
                "DXF-Zeichnung ist zu groß ({width} × {height} mm); Einheiten prüfen",
                width = width,
                height = height
            ),
        };
        f.write_str(&text)
    }
}

impl std::error::Error for DxfError {}

impl From<std::io::Error> for DxfError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}
