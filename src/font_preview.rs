//! Schriftnamen in ihrer eigenen Schrift (Schriftauswahl im Text-Werkzeug).
//!
//! Geladen wird nur, was in der aufgeklappten Liste gerade zu sehen ist, und
//! dann behalten: wer die Liste nie öffnet, braucht keinen Speicher dafür.
//! Sehr große Dateien (CJK-Sammlungen bis 74 MB) bleiben in der
//! Oberflächenschrift, ebenso alles, was nach 150 MB geladener Schriften
//! noch dazukäme: so bleibt der Speicher auf jedem System begrenzt, egal wie
//! viele Schriften installiert sind.

use eframe::egui;
use resvg::usvg::fontdb;
use std::collections::HashMap;
use std::sync::Mutex;
use visicut_core::svg;

/// Neue Schriften je Bild, damit das Scrollen nicht ruckelt.
const PER_PASS: usize = 3;
/// Größere Schriftdateien werden für die Vorschau nicht geladen.
const MAX_FILE: usize = 15 * 1024 * 1024;
/// Höchstens so viel Schriftdaten insgesamt.
const MAX_TOTAL: usize = 150 * 1024 * 1024;

#[derive(Default)]
struct Cache {
    /// Schriftname → Familie zum Zeichnen; `None`, wenn die Schrift fehlt,
    /// ihre Datei zu groß ist oder die Gesamtgrenze erreicht.
    families: HashMap<String, Option<egui::FontFamily>>,
    /// Dateien, geteilt von allen Schriften einer `.ttc`-Sammlung.
    files: HashMap<String, &'static [u8]>,
    /// Summe der Dateien in `files`.
    total: usize,
    pass: u64,
    loaded: usize,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

/// Systemschrift als Ersatz für fehlende Zeichen; von [`crate::form::apply_fonts`] gesetzt.
pub static FALLBACK: std::sync::OnceLock<&'static [u8]> = std::sync::OnceLock::new();

/// Familie, in der `name` gezeichnet wird; `None` heißt Oberflächenschrift
/// (noch nicht geladen, nicht gefunden oder zu groß).
pub fn family(ctx: &egui::Context, name: &str) -> Option<egui::FontFamily> {
    let mut guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let cache = guard.get_or_insert_with(Cache::default);
    if let Some(family) = cache.families.get(name) {
        let family = family.clone()?;
        // `add_font` wirkt erst im nächsten Durchgang; bis dahin Oberflächenschrift.
        let ready = ctx.fonts(|f| f.definitions().families.contains_key(&family));
        if !ready {
            return None;
        }
        return Some(family);
    }
    let pass = ctx.cumulative_pass_nr();
    if cache.pass != pass {
        (cache.pass, cache.loaded) = (pass, 0);
    }
    if cache.loaded >= PER_PASS {
        ctx.request_repaint();
        return None;
    }
    cache.loaded += 1;
    let family = load(ctx, cache, name);
    cache.families.insert(name.to_owned(), family);
    ctx.request_repaint();
    None
}

fn load(ctx: &egui::Context, cache: &mut Cache, name: &str) -> Option<egui::FontFamily> {
    let db = svg::fonts();
    let query = match name {
        "sans-serif" => fontdb::Family::SansSerif,
        "serif" => fontdb::Family::Serif,
        "monospace" => fontdb::Family::Monospace,
        name => fontdb::Family::Name(name),
    };
    let id = db.query(&fontdb::Query {
        families: &[query],
        ..Default::default()
    })?;
    let face = db.face(id)?;
    let key = match &face.source {
        fontdb::Source::File(path) | fontdb::Source::SharedFile(path, _) => {
            path.display().to_string()
        }
        fontdb::Source::Binary(_) => format!("{id:?}"),
    };
    let data = match cache.files.get(&key) {
        Some(data) => *data,
        None => {
            // Bleibt bis zum Programmende: egui braucht `'static`, und eine
            // einmal gezeigte Schrift wird beim Scrollen wieder gebraucht.
            let room = MAX_TOTAL.saturating_sub(cache.total).min(MAX_FILE);
            let data =
                db.with_face_data(id, |data, _| (data.len() <= room).then(|| data.to_vec()))??;
            cache.total += data.len();
            let data: &'static [u8] = Box::leak(data.into_boxed_slice());
            cache.files.insert(key, data);
            data
        }
    };
    let family = egui::FontFamily::Name(format!("preview:{name}").into());
    let mut font = egui::FontData::from_static(data);
    font.index = face.index;
    ctx.add_font(egui::epaint::text::FontInsert::new(
        &format!("preview:{name}"),
        font,
        vec![insert(family.clone())],
    ));
    let fallback = FALLBACK
        .get()
        .copied()
        .unwrap_or(epaint_default_fonts::UBUNTU_LIGHT);
    ctx.add_font(egui::epaint::text::FontInsert::new(
        &format!("preview-fallback:{name}"),
        egui::FontData::from_static(fallback),
        vec![insert(family.clone())],
    ));
    Some(family)
}

fn insert(family: egui::FontFamily) -> egui::epaint::text::InsertFontFamily {
    egui::epaint::text::InsertFontFamily {
        family,
        priority: egui::epaint::text::FontPriority::Lowest,
    }
}
