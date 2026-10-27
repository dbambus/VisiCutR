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
mod worker;

pub use plf::{
    Parameter, ParameterKind, ParameterValue, import_templates, plf_bytes, render_parametric,
    write_plf,
};
use std::path::Path;
pub use worker::{
    FLAG as WORKER_FLAG, serve as serve_worker, use_executable as use_worker_executable,
};

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

/// Time for one file including everything it contains: a VisiCut project
/// (PLF) holds up to 1000 parts, each of which could otherwise use the
/// whole time of a script (5 s) or Ghostscript (60 s).
pub const TIME_LIMIT: std::time::Duration = std::time::Duration::from_secs(90);

thread_local! {
    static DEADLINE: std::cell::Cell<Option<std::time::Instant>> = const { std::cell::Cell::new(None) };
}

/// `limit`, shortened to what is left of the running import's time.
pub(crate) fn time_left(limit: std::time::Duration) -> Result<std::time::Duration, String> {
    let Some(deadline) = DEADLINE.get() else {
        return Ok(limit);
    };
    match deadline.checked_duration_since(std::time::Instant::now()) {
        Some(left) if !left.is_zero() => Ok(limit.min(left)),
        _ => Err(crate::tr!(
            "Import nach {seconds} s abgebrochen (Datei zu aufwendig)",
            seconds = TIME_LIMIT.as_secs_f32()
        )),
    }
}

/// Files read ahead on another thread (see [`prefetch`]).
pub struct Prefetched(std::collections::HashMap<std::path::PathBuf, Result<Imported, String>>);

thread_local! {
    static PREFETCHED: std::cell::RefCell<Option<Prefetched>> = const { std::cell::RefCell::new(None) };
}

/// Reads the importable files among `paths`, meant for a background thread:
/// the window stays responsive while a large or slow file is read. The
/// application then [`install`]s the result and runs its usual import, whose
/// [`read_file`] calls take the files from it.
pub fn prefetch(paths: &[std::path::PathBuf]) -> Prefetched {
    Prefetched(
        paths
            .iter()
            .filter(|p| is_importable(p))
            .map(|p| (p.clone(), read_file(p)))
            .collect(),
    )
}

/// Makes `prefetched` the source of [`read_file`] on this thread until
/// [`uninstall`].
pub fn install(prefetched: Prefetched) {
    PREFETCHED.set(Some(prefetched));
}

pub fn uninstall() {
    PREFETCHED.set(None);
}

/// Whether every importable file among `paths` has been read ahead.
pub fn is_prefetched<'a>(paths: impl IntoIterator<Item = &'a Path>) -> bool {
    PREFETCHED.with_borrow(|prefetched| {
        paths.into_iter().filter(|p| is_importable(p)).all(|p| {
            prefetched
                .as_ref()
                .is_some_and(|prefetched| prefetched.0.contains_key(p))
        })
    })
}

pub fn read_file(path: &Path) -> Result<Imported, String> {
    let ready = PREFETCHED.with_borrow(|prefetched| {
        prefetched
            .as_ref()
            .and_then(|prefetched| prefetched.0.get(path).cloned())
    });
    if let Some(result) = ready {
        return result;
    }
    struct Outermost;
    impl Drop for Outermost {
        fn drop(&mut self) {
            DEADLINE.set(None);
        }
    }
    let _outermost = DEADLINE.get().is_none().then(|| {
        DEADLINE.set(Some(std::time::Instant::now() + TIME_LIMIT));
        Outermost
    });
    read_file_within(path)
}

fn read_file_within(path: &Path) -> Result<Imported, String> {
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
    fn nested_imports_share_the_time_of_the_outermost() {
        let limit = std::time::Duration::from_secs(5);
        assert_eq!(time_left(limit), Ok(limit), "outside an import");
        DEADLINE.set(Some(
            std::time::Instant::now() + std::time::Duration::from_secs(2),
        ));
        assert!(time_left(limit).unwrap() <= std::time::Duration::from_secs(2));
        DEADLINE.set(Some(std::time::Instant::now()));
        assert!(time_left(limit).unwrap_err().contains("abgebrochen"));
        // read_file sets the deadline only when none is running and clears it.
        DEADLINE.set(None);
        let _ = read_file(Path::new("does-not-exist.svg"));
        assert_eq!(DEADLINE.get(), None);
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
