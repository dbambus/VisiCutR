//! Converts every importable file format into an SVG document.
//!
//! Projects store a single SVG, so each importer produces SVG text whose
//! physical size (width/height in mm) matches the source file, like VisiCut's
//! Java `GraphicFileImporter` does by converting to its own graphic model.
mod dxf;
mod eps;
mod gcode;
mod pdf;
mod plf;
mod raster;

use crate::i18n::tr;
pub use crate::svg_import::Imported;
pub use plf::{
    Parameter, ParameterKind, ParameterValue, SCRIPT_WORKER_FLAG, import_templates, plf_bytes,
    render_parametric, serve_script_worker, use_script_worker, write_plf,
};
use std::path::Path;

/// Largest file that is read for import.
pub const MAX_FILE_BYTES: u64 = 25 * 1024 * 1024;

/// Reads ZIP entries within limits. The sizes an archive states for its
/// entries are not binding: a few kilobytes can inflate to gigabytes (zip
/// bomb), so only the bytes actually read count, per entry and in total.
pub(crate) struct ZipBudget {
    left: u64,
}

impl ZipBudget {
    pub(crate) fn new(total: u64) -> Self {
        Self { left: total }
    }

    /// The entry's content, or `None` for an entry larger than `limit`;
    /// fails once the archive as a whole unpacks to more than the budget.
    pub(crate) fn read(
        &mut self,
        entry: &mut impl std::io::Read,
        limit: u64,
    ) -> Result<Option<Vec<u8>>, String> {
        use std::io::Read;
        let cap = limit.min(self.left);
        let mut bytes = Vec::new();
        entry
            .take(cap + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        let read = bytes.len() as u64;
        self.left = self.left.saturating_sub(read);
        if read > cap {
            return if cap < limit {
                Err(tr("Archiv ist entpackt größer als erlaubt").into())
            } else {
                Ok(None)
            };
        }
        Ok(Some(bytes))
    }
}

/// File extensions offered in the open dialogs (without the project format).
pub const EXTENSIONS: &[&str] = &[
    "svg", "psvg", "dxf", "eps", "ps", "pdf", "png", "jpg", "jpeg", "bmp", "gif", "nc", "gcode",
    "plf", "ls",
];

/// Whether `path` is a graphic to import rather than a saved Rust project.
pub fn is_importable(path: &Path) -> bool {
    extension(path).is_some_and(|e| EXTENSIONS.contains(&e.as_str()))
}

pub fn read_file(path: &Path) -> Result<Imported, String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if meta.len() > MAX_FILE_BYTES {
        return Err(tr("Datei ist größer als 25 MB").into());
    }
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase();
    if name.ends_with(".parametric.svg") {
        return plf::read_parametric_svg(path);
    }
    match extension(path).as_deref() {
        Some("svg") => crate::svg_import::read_svg_file(path),
        Some("psvg") => plf::read_parametric_svg(path),
        Some("dxf") => dxf::read(path),
        Some("pdf") => pdf::read_pdf(path),
        Some("eps" | "ps") => pdf::read_postscript(path),
        Some("png" | "jpg" | "jpeg" | "bmp" | "gif") => raster::read(path),
        Some("nc" | "gcode") => gcode::read(path),
        Some("plf") => plf::read_plf(path),
        Some("ls") => plf::read_laser_script(path),
        _ => Err(tr("Unbekanntes Dateiformat").into()),
    }
}

fn extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_import_formats_case_insensitively() {
        assert!(is_importable(Path::new("a/B.SVG")));
        assert!(is_importable(Path::new("teil.Dxf")));
        assert!(is_importable(Path::new("x.parametric.svg")));
        assert!(!is_importable(Path::new("job.vcr")));
        assert!(!is_importable(Path::new("noextension")));
    }

    #[test]
    fn zip_budget_counts_bytes_read_not_stated_sizes() {
        let mut budget = ZipBudget::new(10);
        assert_eq!(
            budget.read(&mut &b"1234"[..], 5).unwrap(),
            Some(b"1234".to_vec())
        );
        // Larger than the entry limit: skipped, but it still costs.
        assert_eq!(budget.read(&mut &b"123456"[..], 5).unwrap(), None);
        assert!(budget.read(&mut &b"12345"[..], 5).is_err());

        // An entry that claims 100 bytes but inflates to 20 MB stops at the
        // limit instead of filling memory.
        use std::io::Write;
        let mut bytes = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut bytes));
            zip.start_file("bomb.xml", zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(&vec![0; 20 * 1024 * 1024]).unwrap();
            zip.finish().unwrap();
        }
        let real = (20u32 * 1024 * 1024).to_le_bytes();
        let mut patched = 0;
        for i in 0..bytes.len() - 4 {
            if bytes[i..i + 4] == real {
                bytes[i..i + 4].copy_from_slice(&100u32.to_le_bytes());
                patched += 1;
            }
        }
        assert_eq!(patched, 2, "local and central header");
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
        let mut entry = archive.by_index(0).unwrap();
        assert_eq!(entry.size(), 100);
        let mut budget = ZipBudget::new(50 * 1024 * 1024);
        assert_eq!(budget.read(&mut entry, 1024 * 1024).unwrap(), None);
        assert_eq!(budget.left, 49 * 1024 * 1024 - 1, "read 1 MB + 1 byte");
    }
}
