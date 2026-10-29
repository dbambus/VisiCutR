use crate::{i18n::tr, tr};
use resvg::usvg::fontdb;
use resvg::{tiny_skia, usvg};
use std::sync::{Arc, OnceLock};

/// Deepest element nesting accepted in XML from files. roxmltree and usvg
/// walk the tree recursively; 256 levels need less than 384 KB of stack,
/// so they also fit the 1 MB main thread on Windows. Far deeper files
/// (100 000 nested groups take 700 KB) overflowed the stack and ended the
/// application.
pub const MAX_NESTING: usize = 256;

/// Refuses XML nested deeper than [`MAX_NESTING`]. Counts start and end
/// tags without building a tree (and without recursion), skipping comments,
/// CDATA, processing instructions, declarations and quoted attribute values.
pub fn check_nesting(text: &str) -> Result<(), String> {
    if nesting_depth(text) > MAX_NESTING {
        return Err(tr!(
            "SVG ist tiefer als {depth} Ebenen verschachtelt",
            depth = MAX_NESTING
        ));
    }
    Ok(())
}

fn nesting_depth(text: &str) -> usize {
    let bytes = text.as_bytes();
    let find = |from: usize, needle: &[u8]| {
        bytes[from.min(bytes.len())..]
            .windows(needle.len())
            .position(|w| w == needle)
            .map_or(bytes.len(), |p| from + p + needle.len())
    };
    // End of a tag at `from`: the '>' outside quotes, and whether "/>".
    let tag_end = |from: usize| {
        let mut quote = None;
        let mut i = from;
        while i < bytes.len() {
            match (quote, bytes[i]) {
                (None, b'"' | b'\'') => quote = Some(bytes[i]),
                (Some(q), c) if c == q => quote = None,
                (None, b'>') => return (i + 1, i > from && bytes[i - 1] == b'/'),
                _ => {}
            }
            i += 1;
        }
        (bytes.len(), false)
    };
    let (mut depth, mut deepest, mut i) = (0usize, 0usize, 0usize);
    while let Some(offset) = bytes[i..].iter().position(|b| *b == b'<') {
        let start = i + offset + 1;
        let rest = &bytes[start..];
        i = if rest.starts_with(b"!--") {
            find(start, b"-->")
        } else if rest.starts_with(b"![CDATA[") {
            find(start, b"]]>")
        } else if rest.starts_with(b"?") {
            find(start, b"?>")
        } else if rest.starts_with(b"!") {
            tag_end(start).0
        } else if rest.starts_with(b"/") {
            depth = depth.saturating_sub(1);
            tag_end(start).0
        } else {
            let (end, empty) = tag_end(start);
            if !empty {
                depth += 1;
                deepest = deepest.max(depth);
            }
            end
        };
    }
    deepest
}

pub struct Preview {
    pub width_mm: f32,
    pub height_mm: f32,
    pub image: eframe::egui::ColorImage,
}

/// System fonts plus the fonts bundled with egui (Ubuntu Light, Hack), loaded once.
///
/// Preview, engraving, cutting and the mapping all share this database so text
/// is laid out identically everywhere. The bundled fonts guarantee that the
/// generic families (`serif`, `sans-serif`, …) resolve even on machines without
/// any system fonts, e.g. minimal CI images.
pub fn fonts() -> Arc<fontdb::Database> {
    static FONTS: OnceLock<Arc<fontdb::Database>> = OnceLock::new();
    FONTS
        .get_or_init(|| {
            let mut db = fontdb::Database::new();
            db.load_system_fonts();
            let mut family = |data: &'static [u8]| {
                let ids = db.load_font_source(fontdb::Source::Binary(Arc::new(data)));
                ids.first()
                    .and_then(|id| db.face(*id))
                    .and_then(|face| face.families.first())
                    .map(|(name, _)| name.clone())
            };
            let sans = family(epaint_default_fonts::UBUNTU_LIGHT).unwrap_or_default();
            let mono = family(epaint_default_fonts::HACK_REGULAR).unwrap_or_default();
            let pick = |db: &fontdb::Database, candidates: &[&str], fallback: &str| {
                candidates
                    .iter()
                    .find(|name| {
                        db.query(&fontdb::Query {
                            families: &[fontdb::Family::Name(name)],
                            ..Default::default()
                        })
                        .is_some()
                    })
                    .map_or(fallback.to_string(), |name| name.to_string())
            };
            let sans = pick(
                &db,
                &[
                    "Arial",
                    "Helvetica",
                    "Liberation Sans",
                    "DejaVu Sans",
                    "Noto Sans",
                ],
                &sans,
            );
            let serif = pick(
                &db,
                &[
                    "Times New Roman",
                    "Times",
                    "Liberation Serif",
                    "DejaVu Serif",
                    "Noto Serif",
                ],
                &sans,
            );
            let mono = pick(
                &db,
                &[
                    "Courier New",
                    "Courier",
                    "Liberation Mono",
                    "DejaVu Sans Mono",
                ],
                &mono,
            );
            let cursive = pick(&db, &["Comic Sans MS"], &sans);
            let fantasy = pick(&db, &["Impact"], &sans);
            db.set_sans_serif_family(sans);
            db.set_serif_family(serif);
            db.set_monospace_family(mono);
            db.set_cursive_family(cursive);
            db.set_fantasy_family(fantasy);
            Arc::new(db)
        })
        .clone()
}

/// Size of the document in mm, as VisiCut's SVG importer sees it
/// (`determineResolution`, `determineTransformation`): width and height in
/// absolute units together with a viewBox are exact; user units (px,
/// unitless, missing or percent sizes, no viewBox) count at the resolution
/// of the editor that wrote the file: 90 DPI (Inkscape before 0.92 and files
/// without editor hints), 96 DPI from Inkscape 0.92 on, 72 DPI for Adobe
/// Illustrator exports.
pub fn document_size_mm(source: &str, size: usvg::Size) -> (f32, f32) {
    let dpi = editor_dpi(source);
    let fallback = || (size.width() * 25.4 / dpi, size.height() * 25.4 / dpi);
    let Ok(document) = usvg::roxmltree::Document::parse_with_options(
        source,
        usvg::roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        },
    ) else {
        return fallback();
    };
    let root = document.root_element();
    let view_box: Option<Vec<f32>> = root.attribute("viewBox").map(|v| {
        v.split(|c: char| c == ',' || c.is_whitespace())
            .filter(|t| !t.is_empty())
            .filter_map(|t| t.parse().ok())
            .collect()
    });
    if !matches!(view_box.as_deref(), Some(&[_, _, w, h]) if w > 0.0 && h > 0.0) {
        return fallback();
    }
    match (
        root.attribute("width").and_then(|v| length_mm(v, dpi)),
        root.attribute("height").and_then(|v| length_mm(v, dpi)),
    ) {
        (Some(w), Some(h)) if w > 0.0 && h > 0.0 => (w, h),
        _ => fallback(),
    }
}

/// An SVG length in mm; `None` for percentages or unknown units.
fn length_mm(value: &str, dpi: f32) -> Option<f32> {
    let value = value.trim();
    let digits = value
        .find(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E')))
        .unwrap_or(value.len());
    let number: f32 = value[..digits].parse().ok()?;
    let per_unit = match value[digits..].trim() {
        "" | "px" => 25.4 / dpi,
        "mm" => 1.0,
        "cm" => 10.0,
        "in" => 25.4,
        "pt" => 25.4 / 72.0,
        "pc" => 25.4 / 6.0,
        _ => return None,
    };
    Some(number * per_unit)
}

/// VisiCut's `determineResolution`: the resolution the writing program used
/// for user units, from comments and namespaces in the file.
fn editor_dpi(source: &str) -> f32 {
    let mut illustrator = false;
    let mut inkscape = false;
    let mut inkscape_092 = false;
    for line in source.lines() {
        if line.starts_with("<!-- Generator: Adobe Illustrator") {
            illustrator = true;
        }
        if line.contains("www.inkscape.org/namespaces") {
            inkscape = true;
        }
        if let Some(at) = line.find("inkscape:version") {
            let rest = line[at + "inkscape:version".len()..]
                .trim_start()
                .trim_start_matches('=')
                .trim_start()
                .trim_start_matches(['"', '\'']);
            let mut parts = rest.split(|c: char| !c.is_ascii_digit());
            let major: i32 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(-1);
            let minor: i32 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(-1);
            if major > 0 || (major == 0 && minor >= 92) {
                inkscape_092 = true;
            }
        }
    }
    if inkscape_092 {
        96.0
    } else if inkscape {
        90.0
    } else if illustrator {
        72.0
    } else {
        90.0
    }
}

/// usvg options shared by preview, engraving, cutting and the mapping.
pub fn options() -> usvg::Options<'static> {
    // Embedded projects must remain portable: external resources are not resolved.
    usvg::Options {
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: usvg::ImageHrefResolver::default_data_resolver(),
            resolve_string: Box::new(|_, _| None),
        },
        fontdb: fonts(),
        ..Default::default()
    }
}

/// Bounding box of the drawing without stroke widths as fractions of the
/// page, `[left, top, right, bottom]`; the whole page `[0, 0, 1, 1]` for
/// documents without content or that cannot be read. The placement is
/// checked every frame, so results are cached by document content.
pub fn content_fraction(source: &str) -> [f32; 4] {
    use std::hash::{Hash, Hasher};
    static CACHE: std::sync::Mutex<Vec<(u64, [f32; 4])>> = std::sync::Mutex::new(Vec::new());
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    let key = hasher.finish();
    if let Some((_, fraction)) = CACHE.lock().unwrap().iter().find(|(k, _)| *k == key) {
        return *fraction;
    }
    let fraction = measure_content(source).unwrap_or([0.0, 0.0, 1.0, 1.0]);
    let mut cache = CACHE.lock().unwrap();
    if cache.len() >= 64 {
        cache.remove(0);
    }
    cache.push((key, fraction));
    fraction
}

fn measure_content(source: &str) -> Option<[f32; 4]> {
    if source.len() > 20 * 1024 * 1024 {
        return None;
    }
    check_nesting(source).ok()?;
    let tree = usvg::Tree::from_str(source, &options()).ok()?;
    let size = tree.size();
    if !tree.root().has_children() {
        return None;
    }
    let bbox = tree.root().abs_bounding_box();
    Some([
        bbox.left() / size.width(),
        bbox.top() / size.height(),
        bbox.right() / size.width(),
        bbox.bottom() / size.height(),
    ])
    .filter(|f| f.iter().all(|v| v.is_finite()))
}

pub fn render(source: &str) -> Result<Preview, String> {
    if source.len() > 20 * 1024 * 1024 {
        return Err(tr("SVG ist größer als 20 MB").into());
    }
    check_nesting(source)?;
    let tree = usvg::Tree::from_str(source, &options())
        .map_err(|e| tr!("SVG konnte nicht gelesen werden: {error}", error = e))?;
    let size = tree.size();
    let scale = (1600.0 / size.width().max(size.height())).min(2.0);
    let width = (size.width() * scale).ceil().max(1.0) as u32;
    let height = (size.height() * scale).ceil().max(1.0) as u32;
    let mut pixmap = tiny_skia::Pixmap::new(width, height).ok_or(tr("Vorschau zu groß"))?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let (width_mm, height_mm) = document_size_mm(source, size);
    Ok(Preview {
        width_mm,
        height_mm,
        image: color_image(pixmap),
    })
}

/// The pixmap as an egui image.
pub fn color_image(pixmap: tiny_skia::Pixmap) -> eframe::egui::ColorImage {
    let size = [pixmap.width() as usize, pixmap.height() as usize];
    // tiny-skia stores premultiplied RGBA; convert before handing pixels to egui.
    let mut pixels = pixmap.take();
    for pixel in pixels.chunks_exact_mut(4) {
        let alpha = pixel[3] as u32;
        if alpha > 0 {
            for channel in &mut pixel[..3] {
                *channel = ((*channel as u32 * 255 + alpha / 2) / alpha).min(255) as u8;
            }
        }
    }
    eframe::egui::ColorImage::from_rgba_unmultiplied(size, &pixels)
}

#[cfg(test)]
mod tests {
    #[test]
    fn nesting_counts_elements_but_not_markup_in_quotes_comments_or_cdata() {
        use super::nesting_depth;
        assert_eq!(nesting_depth("<svg/>"), 0);
        assert_eq!(nesting_depth("<svg><g><rect/></g><g></g></svg>"), 2);
        assert_eq!(
            nesting_depth(
                r#"<?xml version="1.0"?><!DOCTYPE svg [<!ENTITY a "x">]><!-- <g><g> --><svg a="1 > 0" b='<'><![CDATA[<g><g>]]><g th:if="a > b"/></svg>"#
            ),
            1
        );
        let deep = |n: usize| format!("<svg>{}{}</svg>", "<g>".repeat(n), "</g>".repeat(n));
        assert!(super::check_nesting(&deep(super::MAX_NESTING - 1)).is_ok());
        assert!(super::check_nesting(&deep(super::MAX_NESTING)).is_err());
        // 100 000 levels ended the application in roxmltree.
        let error = super::render(&deep(100_000)).err().expect("refused");
        assert!(error.contains("verschachtelt"), "{error}");
    }

    use super::*;
    #[test]
    fn respects_physical_svg_dimensions() {
        let preview = render(include_str!("../examples/demo.svg")).unwrap();
        assert!((preview.width_mm - 100.0).abs() < 0.01);
        assert!((preview.height_mm - 60.0).abs() < 0.01);
        assert!(preview.image.pixels.iter().any(|p| p.a() > 0));
    }
    #[test]
    fn rejects_malformed_svg() {
        assert!(render("not an SVG").is_err());
    }

    #[test]
    fn sizes_user_units_like_visicut() {
        let size = |svg: &str| {
            let preview = render(svg).unwrap();
            (preview.width_mm, preview.height_mm)
        };
        // Unitless without hints: 90 DPI; the same file from Inkscape 0.92: 96 DPI.
        let (w, _) = size(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="90" height="45"><rect width="9" height="9"/></svg>"#,
        );
        assert!((w - 25.4).abs() < 1e-4, "{w}");
        let (w, _) = size(
            r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:inkscape="http://www.inkscape.org/namespaces/inkscape" inkscape:version="0.92.4 (5da689c313, 2019-01-14)" width="96" height="48"><rect width="9" height="9"/></svg>"#,
        );
        assert!((w - 25.4).abs() < 1e-4, "{w}");
        let (w, _) = size(
            r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:inkscape="http://www.inkscape.org/namespaces/inkscape" inkscape:version="0.91 r13725" width="90" height="45"><rect width="9" height="9"/></svg>"#,
        );
        assert!((w - 25.4).abs() < 1e-4, "{w}");
        let (w, _) = size(
            "<!-- Generator: Adobe Illustrator 24.0.0, SVG Export Plug-In -->\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"72\" height=\"36\"><rect width=\"9\" height=\"9\"/></svg>",
        );
        assert!((w - 25.4).abs() < 1e-4, "{w}");
        // Absolute units with a viewBox are exact; without a viewBox the user
        // units count at 90 DPI even if the width is given in mm.
        let (w, h) = size(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="7cm" height="50mm" viewBox="0 0 70 50"><rect width="9" height="9"/></svg>"#,
        );
        assert!(
            (w - 70.0).abs() < 1e-4 && (h - 50.0).abs() < 1e-4,
            "{w} {h}"
        );
        let (w, _) = size(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="90mm" height="45mm"><rect width="9" height="9"/></svg>"#,
        );
        assert!((w - 90.0 * 96.0 / 90.0).abs() < 1e-3, "{w}");
        // Percent sizes fall back to the viewBox in user units.
        let (w, _) = size(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="100%" height="100%" viewBox="0 0 90 45"><rect width="9" height="9"/></svg>"#,
        );
        assert!((w - 25.4).abs() < 1e-4, "{w}");
    }
}
