use crate::{i18n::tr, tr};
use resvg::usvg::fontdb;
use resvg::{tiny_skia, usvg};
use std::fmt;
use std::sync::{Arc, OnceLock};

#[derive(Debug)]
pub enum SvgError {
    /// Nested deeper than [`MAX_NESTING`].
    TooDeep,
    TooLarge,
    /// Entity declarations that carry markup or expand too far.
    Entities,
    Unreadable(usvg::Error),
    PreviewTooLarge,
}

impl fmt::Display for SvgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::TooDeep => tr!(
                "SVG ist tiefer als {depth} Ebenen verschachtelt",
                depth = MAX_NESTING
            ),
            Self::TooLarge => tr("SVG ist größer als 20 MB").to_string(),
            Self::Entities => {
                tr("SVG enthält Entity-Definitionen mit Elementen oder zu großer Ausdehnung")
                    .to_string()
            }
            Self::Unreadable(error) => {
                tr!("SVG konnte nicht gelesen werden: {error}", error = error)
            }
            Self::PreviewTooLarge => tr("Vorschau zu groß").to_string(),
        };
        f.write_str(&text)
    }
}

impl std::error::Error for SvgError {}

impl From<SvgError> for String {
    fn from(error: SvgError) -> Self {
        error.to_string()
    }
}

/// Deepest element nesting accepted in XML from files. roxmltree and usvg
/// walk the tree recursively; 256 levels need less than 384 KB of stack,
/// so they also fit the 1 MB main thread on Windows. Far deeper files
/// (100 000 nested groups take 700 KB) overflowed the stack and ended the
/// application.
pub const MAX_NESTING: usize = 256;

/// Refuses XML nested deeper than [`MAX_NESTING`]. Counts start and end
/// tags without building a tree (and without recursion), skipping comments,
/// CDATA, processing instructions, declarations and quoted attribute values.
///
/// Entities are checked too, since roxmltree parses their values as content:
/// elements inside an entity escape the count above, and many references to
/// a large entity are not caught by roxmltree's own limits.
pub fn check_nesting(text: &str) -> Result<(), SvgError> {
    check_entities(text)?;
    if nesting_depth(text) > MAX_NESTING {
        return Err(SvgError::TooDeep);
    }
    Ok(())
}

/// Most text all entity references of a document may expand to.
pub const MAX_ENTITY_EXPANSION: usize = 20 * 1024 * 1024;

/// Internal entities of the DOCTYPE may hold text (Illustrator declares
/// namespace URIs this way) but no markup, no parameter entities, and all
/// references together expand to at most [`MAX_ENTITY_EXPANSION`].
fn check_entities(text: &str) -> Result<(), SvgError> {
    let Some((entities, body_start)) = entity_declarations(text)? else {
        return Ok(());
    };
    if entities.is_empty() {
        return Ok(());
    }
    // Expanded length of each entity; references nest at most 8 deep.
    fn expanded(name: &str, entities: &[(&str, &str)], depth: usize) -> Result<usize, SvgError> {
        let Some((_, value)) = entities.iter().find(|(n, _)| *n == name) else {
            return Ok(0);
        };
        if depth > 8 {
            return Err(SvgError::Entities);
        }
        let mut total = value.len();
        for reference in references(value) {
            let inner = expanded(reference, entities, depth + 1)?;
            total = total
                .checked_add(inner)
                .filter(|t| *t <= MAX_ENTITY_EXPANSION)
                .ok_or(SvgError::Entities)?;
        }
        Ok(total)
    }
    let mut lengths = Vec::with_capacity(entities.len());
    for (name, _) in &entities {
        lengths.push((*name, expanded(name, &entities, 0)?));
    }
    let mut total = 0usize;
    for reference in references(&text[body_start..]) {
        if let Some((_, length)) = lengths.iter().find(|(n, _)| *n == reference) {
            total = total
                .checked_add(*length)
                .filter(|t| *t <= MAX_ENTITY_EXPANSION)
                .ok_or(SvgError::Entities)?;
        }
    }
    Ok(())
}

/// Names of general entity references (`&name;`, not `&#…;`) in `text`.
fn references(text: &str) -> impl Iterator<Item = &str> {
    text.split('&').skip(1).filter_map(|rest| {
        let name = &rest[..rest.find(';')?];
        let valid = !name.is_empty()
            && !name.starts_with('#')
            && name
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ':'));
        valid.then_some(name)
    })
}

/// The internal entities `(name, value)` of the DOCTYPE and where the
/// document after it starts; `None` without an internal subset. Values with
/// markup and parameter entities are refused.
#[allow(clippy::type_complexity)]
fn entity_declarations(text: &str) -> Result<Option<(Vec<(&str, &str)>, usize)>, SvgError> {
    let Some(doctype) = doctype_start(text) else {
        return Ok(None);
    };
    let bytes = text.as_bytes();
    // The internal subset starts at '[' before the DOCTYPE's '>'.
    let mut i = doctype + 9;
    let mut quote = None;
    loop {
        match (quote, bytes.get(i)) {
            (_, None) => return Ok(None),
            (None, Some(b'"' | b'\'')) => quote = Some(bytes[i]),
            (Some(q), Some(c)) if *c == q => quote = None,
            (None, Some(b'>')) => return Ok(None),
            (None, Some(b'[')) => break,
            _ => {}
        }
        i += 1;
    }
    i += 1;
    let mut entities = Vec::new();
    // End of a declaration: the '>' outside quotes.
    let declaration_end = |from: usize| {
        let mut quote = None;
        for (offset, c) in bytes[from..].iter().enumerate() {
            match (quote, c) {
                (None, b'"' | b'\'') => quote = Some(*c),
                (Some(q), c) if *c == q => quote = None,
                (None, b'>') => return from + offset + 1,
                _ => {}
            }
        }
        bytes.len()
    };
    while i < bytes.len() {
        let rest = &text[i..];
        if rest.starts_with(']') {
            return Ok(Some((entities, declaration_end(i))));
        } else if rest.starts_with("<!--") {
            i = rest.find("-->").map_or(bytes.len(), |p| i + p + 3);
        } else if let Some(declaration) = rest.strip_prefix("<!ENTITY") {
            let end = declaration_end(i);
            let declaration = declaration.trim_start();
            if declaration.starts_with('%') {
                return Err(SvgError::Entities);
            }
            let name_end = declaration
                .find(char::is_whitespace)
                .unwrap_or(declaration.len());
            let name = &declaration[..name_end];
            let value = declaration[name_end..].trim_start();
            if let Some(q) = value.chars().next().filter(|c| matches!(c, '"' | '\'')) {
                let close = value[1..].find(q).ok_or(SvgError::Entities)?;
                let value = &value[1..1 + close];
                // `&#60;` turns into markup when the entity is declared.
                if value.contains('<') || value.contains('%') || value.contains("&#") {
                    return Err(SvgError::Entities);
                }
                entities.push((name, value));
            }
            i = end;
        } else if rest.starts_with('<') {
            i = declaration_end(i);
        } else {
            i += rest.chars().next().map_or(1, char::len_utf8);
        }
    }
    Ok(Some((entities, bytes.len())))
}

/// Where the DOCTYPE starts: in the prolog, after the byte order mark, the
/// XML declaration, comments, processing instructions and white space.
/// `<!DOCTYPE` inside a comment before it is not the DOCTYPE; searching for
/// the first occurrence let a comment hide the real one from the checks.
pub fn doctype_start(text: &str) -> Option<usize> {
    let mut i = text.len() - text.trim_start_matches('\u{feff}').len();
    loop {
        i = text.len() - text[i..].trim_start().len();
        let rest = &text[i..];
        if rest.starts_with("<!DOCTYPE") {
            return Some(i);
        }
        let (close, from) = if rest.starts_with("<!--") {
            ("-->", 4)
        } else if rest.starts_with("<?") {
            ("?>", 2)
        } else {
            return None;
        };
        i += from + rest[from..].find(close)? + close.len();
    }
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
    /// The drawing rendered at preview resolution (premultiplied RGBA).
    pub pixmap: tiny_skia::Pixmap,
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
            resolve_data: Box::new(|mime, data, _| resolve_data(mime, data)),
            resolve_string: Box::new(|_, _| None),
        },
        fontdb: fonts(),
        ..Default::default()
    }
}

/// Largest embedded raster image decoded (pixels). tiny-skia allocates the
/// whole image from the size in its header: a PNG of a few bytes stating
/// 100 000 × 100 000 pixels asked for 40 GB and ended the application.
pub const MAX_IMAGE_PIXELS: u64 = 64_000_000;

/// usvg's `data:` resolver with limits: raster images up to
/// [`MAX_IMAGE_PIXELS`], SVG images unpacked to at most 20 MB and checked
/// like any other drawing (usvg would unpack gzip without limit and parse
/// the inner document unchecked).
fn resolve_data(mime: &str, data: std::sync::Arc<Vec<u8>>) -> Option<usvg::ImageKind> {
    use usvg::ImageKind;
    let raster = |data: std::sync::Arc<Vec<u8>>| {
        let size = imagesize::blob_size(&data).ok()?;
        let pixels = (size.width as u64).checked_mul(size.height as u64)?;
        let kind = match imagesize::image_type(&data).ok()? {
            imagesize::ImageType::Jpeg => ImageKind::JPEG(data),
            imagesize::ImageType::Png => ImageKind::PNG(data),
            imagesize::ImageType::Gif => ImageKind::GIF(data),
            imagesize::ImageType::Webp => ImageKind::WEBP(data),
            _ => return None,
        };
        (pixels <= MAX_IMAGE_PIXELS).then_some(kind)
    };
    match mime {
        "image/jpg" | "image/jpeg" | "image/png" | "image/gif" | "image/webp" => raster(data),
        "image/svg+xml" => sub_svg(&data),
        "text/plain" => raster(data.clone()).or_else(|| sub_svg(&data)),
        _ => None,
    }
}

/// An SVG image inside a drawing, without images of its own (like usvg).
fn sub_svg(data: &[u8]) -> Option<usvg::ImageKind> {
    const LIMIT: usize = 20 * 1024 * 1024;
    let plain = if data.starts_with(&[0x1f, 0x8b]) {
        use std::io::Read;
        let mut plain = Vec::new();
        flate2::read::GzDecoder::new(data)
            .take(LIMIT as u64 + 1)
            .read_to_end(&mut plain)
            .ok()?;
        plain
    } else {
        data.to_vec()
    };
    if plain.len() > LIMIT {
        return None;
    }
    let text = std::str::from_utf8(&plain).ok()?;
    check_nesting(text).ok()?;
    let options = usvg::Options {
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        fontdb: fonts(),
        ..Default::default()
    };
    usvg::Tree::from_str(text, &options)
        .ok()
        .map(usvg::ImageKind::SVG)
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
    // A panic while holding the lock leaves a valid cache behind.
    let lock = || {
        CACHE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    };
    if let Some((_, fraction)) = lock().iter().find(|(k, _)| *k == key) {
        return *fraction;
    }
    let fraction = measure_content(source).unwrap_or([0.0, 0.0, 1.0, 1.0]);
    let mut cache = lock();
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

pub fn render(source: &str) -> Result<Preview, SvgError> {
    if source.len() > 20 * 1024 * 1024 {
        return Err(SvgError::TooLarge);
    }
    check_nesting(source)?;
    let tree = usvg::Tree::from_str(source, &options()).map_err(SvgError::Unreadable)?;
    let size = tree.size();
    let scale = (1600.0 / size.width().max(size.height())).min(2.0);
    let width = (size.width() * scale).ceil().max(1.0) as u32;
    let height = (size.height() * scale).ceil().max(1.0) as u32;
    let mut pixmap = tiny_skia::Pixmap::new(width, height).ok_or(SvgError::PreviewTooLarge)?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let (width_mm, height_mm) = document_size_mm(source, size);
    Ok(Preview {
        width_mm,
        height_mm,
        pixmap,
    })
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
        assert!(matches!(
            super::render(&deep(100_000)),
            Err(super::SvgError::TooDeep)
        ));
    }

    #[test]
    fn entities_with_elements_or_large_expansion_are_refused() {
        use super::{SvgError, check_nesting};
        // Illustrator: namespace URIs as entities.
        let illustrator = r#"<?xml version="1.0"?><!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "x.dtd" [
  <!ENTITY ns_svg "http://www.w3.org/2000/svg">
  <!-- a comment with ] and > -->
  <!ENTITY ns_flows "http://ns.adobe.com/Flows/1.0/">
]><svg xmlns="&ns_svg;" xmlns:f="&ns_flows;">&amp;&#38;</svg>"#;
        assert!(check_nesting(illustrator).is_ok());
        // Elements in an entity overflowed the stack in roxmltree.
        let markup = format!(
            r#"<!DOCTYPE svg [<!ENTITY a "{}{}">]><svg>&a;</svg>"#,
            "<g>".repeat(1000),
            "</g>".repeat(1000)
        );
        assert!(matches!(check_nesting(&markup), Err(SvgError::Entities)));
        // 12 MB entity referenced 255 times from a second one.
        let big = format!(
            r#"<!DOCTYPE svg [<!ENTITY a "{}"><!ENTITY b "{}">]><svg x="&b;&b;"/>"#,
            "x".repeat(12 * 1024 * 1024),
            "&a;".repeat(255)
        );
        assert!(matches!(check_nesting(&big), Err(SvgError::Entities)));
        let many = format!(
            r#"<!DOCTYPE svg [<!ENTITY a "{}">]><svg x="{}"/>"#,
            "x".repeat(1024 * 1024),
            "&a;".repeat(21)
        );
        assert!(matches!(check_nesting(&many), Err(SvgError::Entities)));
        let escaped = r#"<!DOCTYPE svg [<!ENTITY a "&#60;g>&#60;/g>">]><svg>&a;</svg>"#;
        assert!(matches!(check_nesting(escaped), Err(SvgError::Entities)));
        let parameter = r#"<!DOCTYPE svg [<!ENTITY % p "<!ENTITY a 'x'>"> %p;]><svg/>"#;
        assert!(matches!(check_nesting(parameter), Err(SvgError::Entities)));
        // A comment or processing instruction before the DOCTYPE that
        // mentions `<!DOCTYPE` does not hide the real one.
        for prefix in [
            "<!-- <!DOCTYPE x> -->",
            "\u{feff}<?xml version=\"1.0\"?>\n<!-- a -->\n<?pi <!DOCTYPE x> ?>",
        ] {
            let hidden = format!("{prefix}{markup}");
            assert!(
                roxmltree::Document::parse_with_options(
                    &hidden.replace("&a;", ""),
                    roxmltree::ParsingOptions {
                        allow_dtd: true,
                        ..Default::default()
                    }
                )
                .is_ok()
            );
            assert!(
                matches!(check_nesting(&hidden), Err(SvgError::Entities)),
                "{prefix}"
            );
            let hidden = format!("{prefix}{many}");
            assert!(
                matches!(check_nesting(&hidden), Err(SvgError::Entities)),
                "{prefix}"
            );
        }
    }

    #[test]
    fn doctype_only_in_the_prolog() {
        assert_eq!(
            super::doctype_start("<!-- <!DOCTYPE x> --> <!DOCTYPE y>"),
            Some(22)
        );
        assert_eq!(super::doctype_start("<svg/><!DOCTYPE x>"), None);
        assert_eq!(super::doctype_start("<!-- unclosed <!DOCTYPE x>"), None);
    }

    #[test]
    fn embedded_images_are_checked_before_decoding() {
        use base64::Engine;
        let encode = |bytes: &[u8]| base64::engine::general_purpose::STANDARD.encode(bytes);
        let image = |mime: &str, data: &[u8]| {
            format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><image width="10" height="10" href="data:{mime};base64,{}"/></svg>"#,
                encode(data)
            )
        };
        let images = |source: &str| {
            let tree = usvg::Tree::from_str(source, &super::options()).unwrap();
            tree.root().children().len()
        };
        // A real PNG is drawn.
        let mut pixmap = tiny_skia::Pixmap::new(2, 2).unwrap();
        pixmap.fill(tiny_skia::Color::BLACK);
        let png = pixmap.encode_png().unwrap();
        assert_eq!(images(&image("image/png", &png)), 1);
        // The same PNG claiming 100 000 × 100 000 pixels is not decoded.
        let mut bomb = png.clone();
        bomb[16..20].copy_from_slice(&100_000u32.to_be_bytes());
        bomb[20..24].copy_from_slice(&100_000u32.to_be_bytes());
        assert_eq!(images(&image("image/png", &bomb)), 0);
        assert_eq!(images(&image("text/plain", &bomb)), 0);
        // SVG images: deep nesting and gzip beyond 20 MB are refused.
        let small = r#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><rect width="1" height="1"/></svg>"#;
        assert_eq!(images(&image("image/svg+xml", small.as_bytes())), 1);
        let deep = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg">{}{}</svg>"#,
            "<g>".repeat(100_000),
            "</g>".repeat(100_000)
        );
        assert_eq!(images(&image("image/svg+xml", deep.as_bytes())), 0);
        let gzip = |bytes: &[u8]| {
            use std::io::Write;
            let mut encoder =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
            encoder.write_all(bytes).unwrap();
            encoder.finish().unwrap()
        };
        assert_eq!(images(&image("image/svg+xml", &gzip(small.as_bytes()))), 1);
        let padded = small.replace("<rect", &format!("<!--{}--><rect", " ".repeat(21 << 20)));
        assert_eq!(images(&image("image/svg+xml", &gzip(padded.as_bytes()))), 0);
    }

    use super::*;
    #[test]
    fn respects_physical_svg_dimensions() {
        let preview = render(include_str!("../examples/demo.svg")).unwrap();
        assert!((preview.width_mm - 100.0).abs() < 0.01);
        assert!((preview.height_mm - 60.0).abs() < 0.01);
        assert!(preview.pixmap.pixels().iter().any(|p| p.alpha() > 0));
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
