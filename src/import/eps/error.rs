//! Why the built-in interpreter cannot run an EPS or PS file; most name the
//! operator, and the caller then hands the file to Ghostscript.

use crate::{i18n::tr, tr};
use std::fmt;

/// The kind of operand an operator expected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operand {
    Number,
    Bool,
    Procedure,
    Matrix,
    Name,
}

/// Groups of operators outside the supported subset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unsupported {
    Text,
    Font,
    Image,
    Other,
}

#[derive(Debug)]
pub enum EpsError {
    TooManyOperations,
    StackOverflow,
    /// The SVG would be larger than 20 MB.
    TooComplex,
    CallsTooDeep,
    ProceduresTooDeep,
    StackUnderflow(String),
    InvalidNumber(String),
    WrongOperand {
        operator: String,
        expected: Operand,
    },
    /// A count or index below zero.
    NegativeIndex(String),
    NotAnInteger(String),
    NegativeRadius(String),
    InvalidCoordinate(String),
    NoCurrentPoint(String),
    Undefined(String),
    DivisionByZero,
    NegativeSqrt,
    ZeroStep,
    Unsupported {
        operator: String,
        kind: Unsupported,
    },
    EmptyBoundingBox,
    UnexpectedBrace,
    UnclosedProcedure,
    UnclosedString,
    InvalidHexString,
    UnclosedHexString,
}

impl EpsError {
    /// An operator outside the supported subset, grouped by what it does.
    pub(super) fn unsupported(name: &str) -> Self {
        let kind = match name {
            "show" | "ashow" | "widthshow" | "awidthshow" | "xshow" | "yshow" | "xyshow"
            | "cshow" | "kshow" | "stringwidth" | "charpath" | "glyphshow" => Unsupported::Text,
            "findfont" | "setfont" | "scalefont" | "makefont" | "selectfont" | "currentfont"
            | "definefont" | "findencoding" => Unsupported::Font,
            "image" | "imagemask" | "colorimage" | "readimage" => Unsupported::Image,
            _ => Unsupported::Other,
        };
        Self::Unsupported {
            operator: name.into(),
            kind,
        }
    }
}

impl fmt::Display for EpsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::TooManyOperations => tr("Die EPS-Datei braucht zu viele Operationen").to_string(),
            Self::StackOverflow => {
                tr("Stapelüberlauf im eingebauten PostScript-Interpreter").to_string()
            }
            Self::TooComplex => tr("EPS-Datei ist zu komplex (SVG größer als 20 MB)").to_string(),
            Self::CallsTooDeep => {
                tr("Prozeduren sind zu tief ineinander verschachtelt").to_string()
            }
            Self::ProceduresTooDeep => tr("Prozeduren sind zu tief verschachtelt").to_string(),
            Self::StackUnderflow(op) => tr!("Stapelunterlauf bei „{operator}“", operator = op),
            Self::InvalidNumber(op) => tr!("Ungültiger Zahlenwert bei „{operator}“", operator = op),
            Self::WrongOperand { operator, expected } => match expected {
                Operand::Number => tr!(
                    "Falscher Operandtyp bei „{operator}“ (Zahl erwartet)",
                    operator = operator
                ),
                Operand::Bool => tr!(
                    "Falscher Operandtyp bei „{operator}“ (Bool erwartet)",
                    operator = operator
                ),
                Operand::Procedure => tr!(
                    "Falscher Operandtyp bei „{operator}“ (Prozedur erwartet)",
                    operator = operator
                ),
                Operand::Matrix => tr!(
                    "Falscher Operandtyp bei „{operator}“ (Matrix erwartet)",
                    operator = operator
                ),
                Operand::Name => tr!(
                    "Falscher Operandtyp bei „{operator}“ (Name erwartet)",
                    operator = operator
                ),
            },
            Self::NegativeIndex(op) => tr!(
                "Bereichsfehler bei „{operator}“ (ganze Zahl ≥ 0 erwartet)",
                operator = op
            ),
            Self::NotAnInteger(op) => tr!(
                "Bereichsfehler bei „{operator}“ (ganze Zahl erwartet)",
                operator = op
            ),
            Self::NegativeRadius(op) => tr!(
                "Bereichsfehler bei „{operator}“ (negativer Radius)",
                operator = op
            ),
            Self::InvalidCoordinate(op) => {
                tr!("Ungültige Koordinate bei „{operator}“", operator = op)
            }
            Self::NoCurrentPoint(op) => tr!("Kein aktueller Punkt bei „{operator}“", operator = op),
            Self::Undefined(key) => tr!("„{key}“ ist nicht definiert (bei „load“)", key = key),
            Self::DivisionByZero => {
                tr("Ergebnis nicht definiert bei „div“ (Division durch 0)").to_string()
            }
            Self::NegativeSqrt => tr("Bereichsfehler bei „sqrt“ (negativer Wert)").to_string(),
            Self::ZeroStep => tr("Schrittweite 0 bei „for“").to_string(),
            Self::Unsupported { operator, kind } => match kind {
                Unsupported::Text => tr!(
                    "Text-Operator „{operator}“ wird nicht unterstützt",
                    operator = operator
                ),
                Unsupported::Font => tr!(
                    "Schrift-Operator „{operator}“ wird nicht unterstützt",
                    operator = operator
                ),
                Unsupported::Image => tr!(
                    "Bild-Operator „{operator}“ wird nicht unterstützt",
                    operator = operator
                ),
                Unsupported::Other => tr!(
                    "Unbekannter oder nicht unterstützter Operator „{operator}“",
                    operator = operator
                ),
            },
            Self::EmptyBoundingBox => {
                tr("Die BoundingBox der EPS-Datei ist leer oder ungültig").to_string()
            }
            Self::UnexpectedBrace => tr("Unerwartete „}“ in der EPS-Datei").to_string(),
            Self::UnclosedProcedure => {
                tr("Eine Prozedur „{“ in der EPS-Datei ist nicht geschlossen").to_string()
            }
            Self::UnclosedString => {
                tr("Eine Zeichenkette „(...)“ in der EPS-Datei ist nicht geschlossen").to_string()
            }
            Self::InvalidHexString => tr("Ungültige Hex-Zeichenkette in der EPS-Datei").to_string(),
            Self::UnclosedHexString => {
                tr("Eine Hex-Zeichenkette „<...>“ in der EPS-Datei ist nicht geschlossen")
                    .to_string()
            }
        };
        f.write_str(&text)
    }
}

impl std::error::Error for EpsError {}
