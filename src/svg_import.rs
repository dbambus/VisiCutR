//! Embeds external images of an imported SVG as `data:` URIs.
//!
//! Projects store the SVG as text and must stay portable, so images that the
//! SVG references by file path are resolved once, when the SVG file is
//! imported, relative to the SVG file's directory (like VisiCut's Java
//! importer). Only the affected attribute values are replaced; the rest of the
//! document stays byte-identical. Network URLs and network paths (UNC) are
//! never opened, and images from outside the SVG's folder are reported, so
//! a downloaded SVG cannot quietly carry other local files into a project.
use crate::{i18n::tr, tr};
use base64::Engine;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Largest single image file that is embedded.
pub const MAX_IMAGE_BYTES: u64 = 10 * 1024 * 1024;
/// Largest SVG after embedding; matches the limit of [`crate::svg::render`].
pub const MAX_SVG_BYTES: usize = 20 * 1024 * 1024;

const SVG_NS: &str = "http://www.w3.org/2000/svg";
const XLINK_NS: &str = "http://www.w3.org/1999/xlink";
const SODIPODI_NS: &str = "http://sodipodi.sourceforge.net/DTD/sodipodi-0.dtd";

#[derive(Clone, Default)]
pub struct Imported {
    pub svg: String,
    /// German, user-facing notes about images that were not embedded.
    pub warnings: Vec<String>,
    /// Processing steps from the mappings of a VisiCut project (PLF); empty otherwise.
    pub steps: Vec<crate::project::JobStep>,
    /// Template and values of a parametric SVG, whose parameters stay editable.
    pub parametric: Option<crate::project::Parametric>,
    /// Further parts beside the main drawing: parametric parts of a VisiCut
    /// project, which stay editable as parts of their own.
    pub extra_parts: Vec<ExtraPart>,
    /// Where the main drawing's top left corner belongs relative to the
    /// usual import position (mm); zero except for cropped project parts.
    pub offset_mm: [f64; 2],
}

/// A part of an import that keeps its own drawing (see [`Imported::extra_parts`]).
#[derive(Clone)]
pub struct ExtraPart {
    pub name: String,
    pub svg: String,
    pub steps: Vec<crate::project::JobStep>,
    pub parametric: Option<crate::project::Parametric>,
    /// Top left corner relative to the origin of the main drawing (mm).
    pub offset_mm: [f64; 2],
}

/// Reads an SVG file (≤ 25 MB) and embeds its external images.
pub fn read_svg_file(path: &Path) -> Result<Imported, String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if meta.len() > 25 * 1024 * 1024 {
        return Err(tr("Datei ist größer als 25 MB").into());
    }
    let source = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    crate::svg::check_nesting(&source)?;
    Ok(embed_external_images(
        &source,
        path.parent().unwrap_or(Path::new(".")),
    ))
}

/// If the drawing reaches beyond the canvas (viewBox / width and height), the
/// same SVG with the canvas grown to the whole drawing; `None` when
/// everything lies inside. Scale and position of the drawing are unchanged.
pub fn fit_to_content(source: &str) -> Option<String> {
    use resvg::usvg;
    crate::svg::check_nesting(source).ok()?;
    let tree = usvg::Tree::from_str(source, &crate::svg::options()).ok()?;
    let bbox = tree.root().abs_stroke_bounding_box();
    let canvas = tree.size();
    let tolerance = 0.5;
    if !(bbox.width() > 0.0 || bbox.height() > 0.0)
        || (bbox.left() >= -tolerance
            && bbox.top() >= -tolerance
            && bbox.right() <= canvas.width() + tolerance
            && bbox.bottom() <= canvas.height() + tolerance)
    {
        return None;
    }
    let doc = roxmltree::Document::parse_with_options(
        source,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        },
    )
    .ok()?;
    let root = doc.root_element();
    let view_box: Option<Vec<f32>> = root.attribute("viewBox").map(|v| {
        v.split(|c: char| c == ',' || c.is_whitespace())
            .filter(|t| !t.is_empty())
            .filter_map(|t| t.parse().ok())
            .collect()
    });
    // Canvas pixels → user units of the document (default `xMidYMid meet`).
    let (scale, origin, offset) = match view_box.as_deref() {
        Some(&[x, y, w, h]) if w > 0.0 && h > 0.0 => {
            let scale = (canvas.width() / w).min(canvas.height() / h);
            (
                scale,
                [x, y],
                [
                    (canvas.width() - w * scale) / 2.0,
                    (canvas.height() - h * scale) / 2.0,
                ],
            )
        }
        _ => (1.0, [0.0, 0.0], [0.0, 0.0]),
    };
    let x = origin[0] + (bbox.left() - offset[0]) / scale;
    let y = origin[1] + (bbox.top() - offset[1]) / scale;
    let (w, h) = (bbox.width() / scale, bbox.height() / scale);
    // Cut the old size attributes out of the start tag, back to front.
    let range = root.range();
    let mut tag_end = range.start;
    let mut quote = None;
    for (i, c) in source[range.start..].char_indices() {
        match (quote, c) {
            (None, '"') | (None, '\'') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None, '>') => {
                tag_end = range.start + i;
                break;
            }
            _ => {}
        }
    }
    let mut cuts: Vec<std::ops::Range<usize>> = root
        .attributes()
        .filter(|a| a.namespace().is_none() && matches!(a.name(), "width" | "height" | "viewBox"))
        .map(|a| a.range())
        .collect();
    cuts.sort_by_key(|r| std::cmp::Reverse(r.start));
    let mut tag = source[range.start..tag_end].to_owned();
    for cut in cuts {
        tag.replace_range(cut.start - range.start..cut.end - range.start, "");
    }
    let name_end = tag
        .find(|c: char| c.is_whitespace() || c == '/')
        .unwrap_or(tag.len());
    tag.insert_str(
        name_end,
        &format!(
            " width=\"{}\" height=\"{}\" viewBox=\"{x} {y} {w} {h}\"",
            w * scale,
            h * scale
        ),
    );
    Some(format!(
        "{}{}{}",
        &source[..range.start],
        tag,
        &source[tag_end..]
    ))
}

/// Replaces `href`/`xlink:href` of `<image>` elements that point to local files
/// with `data:` URIs. Paths are resolved relative to `base_dir`. A document
/// that cannot be parsed is returned unchanged; rendering reports the error.
pub fn embed_external_images(source: &str, base_dir: &Path) -> Imported {
    let mut warnings = Vec::new();
    let options = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..Default::default()
    };
    let parsed = crate::svg::check_nesting(source).and_then(|()| {
        roxmltree::Document::parse_with_options(source, options).map_err(|e| e.to_string())
    });
    let Ok(document) = parsed else {
        return Imported {
            svg: source.to_owned(),
            warnings,
            ..Default::default()
        };
    };
    let mut cache: HashMap<PathBuf, Result<String, String>> = HashMap::new();
    let folder = std::fs::canonicalize(base_dir).ok();
    let mut reported = std::collections::HashSet::new();
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    let mut length = source.len();
    let images = document.descendants().filter(|node| {
        node.is_element()
            && node.tag_name().name() == "image"
            && node.tag_name().namespace() == Some(SVG_NS)
    });
    for node in images {
        let absref = node.attribute((SODIPODI_NS, "absref"));
        for attribute in node.attributes() {
            if attribute.name() != "href" || !matches!(attribute.namespace(), None | Some(XLINK_NS))
            {
                continue;
            }
            let href = attribute.value().trim();
            let shown = shorten(href);
            let candidates = match classify(href) {
                Href::Skip => continue,
                Href::Remote => {
                    warnings.push(tr!(
                        "Bild „{name}“ wird nicht aus dem Netz geladen; \
                         bitte als Datei speichern und neu verknüpfen",
                        name = shown
                    ));
                    continue;
                }
                Href::Unsupported => {
                    warnings.push(tr!(
                        "Bild „{name}“: Adresse wird nicht unterstützt",
                        name = shown
                    ));
                    continue;
                }
                Href::Paths(paths) => paths,
            };
            let mut candidates = candidates
                .into_iter()
                .chain(absref.map(PathBuf::from))
                .filter(|path| !is_network_path(path))
                .map(|path| base_dir.join(path));
            let Some(path) = candidates.find(|path| path.is_file()) else {
                warnings.push(tr!(
                    "Bild „{name}“ nicht gefunden und nicht eingebettet",
                    name = shown
                ));
                continue;
            };
            let embedded = cache
                .entry(path.clone())
                .or_insert_with(|| data_uri(&path))
                .clone();
            let uri = match embedded {
                Ok(uri) => uri,
                Err(reason) => {
                    warnings.push(tr!(
                        "Bild „{name}“ nicht eingebettet: {reason}",
                        name = shown,
                        reason = reason
                    ));
                    continue;
                }
            };
            let Some(range) = value_range(source, attribute.range()) else {
                continue;
            };
            let grown = length - range.len() + uri.len();
            if grown > MAX_SVG_BYTES {
                warnings.push(tr!(
                    "Bild „{name}“ nicht eingebettet: Die SVG würde größer als 20 MB",
                    name = shown
                ));
                continue;
            }
            length = grown;
            edits.push((range, uri));
            let canonical = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
            let inside = folder.as_ref().is_some_and(|f| canonical.starts_with(f));
            if !inside && reported.insert(canonical.clone()) {
                warnings.push(tr!(
                    "Bild „{name}“ liegt nicht im Ordner der Zeichnung und wurde \
                     eingebettet: {path}. Vor dem Weitergeben des Projekts prüfen.",
                    name = shown,
                    path = canonical.display()
                ));
            }
        }
    }
    edits.sort_by_key(|(range, _)| range.start);
    let mut svg = String::with_capacity(length);
    let mut position = 0;
    for (range, uri) in edits {
        svg.push_str(&source[position..range.start]);
        svg.push_str(&uri);
        position = range.end;
    }
    svg.push_str(&source[position..]);
    Imported {
        svg,
        warnings,
        ..Default::default()
    }
}

enum Href {
    Skip,
    Remote,
    Unsupported,
    /// Candidate paths, in order of preference.
    Paths(Vec<PathBuf>),
}

fn classify(href: &str) -> Href {
    if href.is_empty() || href.starts_with('#') {
        return Href::Skip;
    }
    // A single letter before ':' is a Windows drive, not a URL scheme.
    let scheme = href
        .split_once(':')
        .map(|(scheme, _)| scheme)
        .filter(|scheme| {
            scheme.len() > 1
                && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        });
    match scheme.map(str::to_ascii_lowercase).as_deref() {
        Some("data") => Href::Skip,
        Some("file") if file_url_host(href).is_some() => Href::Remote,
        Some("file") => file_url_path(href).map_or(Href::Unsupported, |p| Href::Paths(vec![p])),
        Some("http" | "https" | "ftp") => Href::Remote,
        Some(_) => Href::Unsupported,
        None if is_network_path(Path::new(href)) => Href::Remote,
        None => {
            // A relative reference is a URI, but tools often write plain paths.
            let mut paths = vec![PathBuf::from(href)];
            if href.contains('%')
                && let Some(decoded) = percent_decode(href)
            {
                paths.push(decoded);
            }
            Href::Paths(paths)
        }
    }
}

/// The host of `file://host/…` unless it is empty or `localhost`.
fn file_url_host(url: &str) -> Option<&str> {
    let rest = url[5..].split(['?', '#']).next().unwrap_or_default();
    let authority_and_path = rest.strip_prefix("//")?;
    let host = &authority_and_path[..authority_and_path
        .find('/')
        .unwrap_or(authority_and_path.len())];
    (!host.is_empty() && !host.eq_ignore_ascii_case("localhost")).then_some(host)
}

/// A path on another computer (`\\server\share`, `//server/share`, `\\?\UNC\…`).
/// Windows connects to it over SMB and sends the user's credentials, so such
/// paths are never opened; the check holds on every platform.
fn is_network_path(path: &Path) -> bool {
    let text = path.to_string_lossy();
    let mut start = text.chars();
    if matches!(
        (start.next(), start.next()),
        (Some('\\' | '/'), Some('\\' | '/'))
    ) {
        return true;
    }
    #[cfg(windows)]
    {
        use std::path::{Component, Prefix};
        if let Some(Component::Prefix(prefix)) = path.components().next() {
            return matches!(
                prefix.kind(),
                Prefix::UNC(..)
                    | Prefix::VerbatimUNC(..)
                    | Prefix::DeviceNS(..)
                    | Prefix::Verbatim(..)
            );
        }
    }
    false
}

fn file_url_path(url: &str) -> Option<PathBuf> {
    let rest = url[5..].split(['?', '#']).next().unwrap_or_default();
    let path = match rest.strip_prefix("//") {
        // Hosts other than localhost are rejected by `classify`.
        Some(authority_and_path) => {
            &authority_and_path[authority_and_path
                .find('/')
                .unwrap_or(authority_and_path.len())..]
        }
        None => rest,
    };
    let decoded = percent_decode(path)?;
    if cfg!(windows) {
        // file:///C:/dir/x.png decodes to /C:/dir/x.png.
        let text = decoded.to_string_lossy();
        let bytes = text.as_bytes();
        if bytes.len() >= 3
            && bytes[0] == b'/'
            && bytes[1].is_ascii_alphabetic()
            && bytes[2] == b':'
        {
            return Some(PathBuf::from(&text[1..]));
        }
    }
    Some(decoded)
}

fn percent_decode(text: &str) -> Option<PathBuf> {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = text.get(index + 1..index + 3)?;
            decoded.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        Some(PathBuf::from(std::ffi::OsString::from_vec(decoded)))
    }
    #[cfg(not(unix))]
    {
        String::from_utf8(decoded).ok().map(PathBuf::from)
    }
}

fn data_uri(path: &Path) -> Result<String, String> {
    let size = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    if size > MAX_IMAGE_BYTES {
        return Err(tr("Die Datei ist größer als 10 MB").into());
    }
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    let svgz = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("svgz"));
    let mime = mime_type(&data, svgz).ok_or(tr(
        "Format wird nicht unterstützt (nur PNG, JPEG, GIF, WebP oder SVG)",
    ))?;
    Ok(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&data)
    ))
}

/// Formats resvg can draw, detected from content rather than the file name.
/// gzip data counts as compressed SVG only in a `.svgz` file, so that other
/// archives are not embedded.
fn mime_type(data: &[u8], svgz: bool) -> Option<&'static str> {
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if data.len() >= 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        Some("image/webp")
    } else if svgz && data.starts_with(&[0x1F, 0x8B]) {
        // Compressed SVG (svgz); usvg decompresses nested SVG data.
        Some("image/svg+xml")
    } else {
        let head = String::from_utf8_lossy(&data[..data.len().min(4096)]);
        let head = head.trim_start_matches('\u{feff}').trim_start();
        (head.starts_with('<') && head.contains("<svg")).then_some("image/svg+xml")
    }
}

/// The value range of an attribute, without quotes. Derived from the full
/// attribute range because roxmltree's value range is unreliable for very
/// long names or spacing.
pub(crate) fn value_range(
    source: &str,
    attribute: std::ops::Range<usize>,
) -> Option<std::ops::Range<usize>> {
    let text = source.get(attribute.clone())?;
    let equals = text.find('=')?;
    let quote = equals + 1 + text[equals + 1..].find(['"', '\''])?;
    let closing = text.as_bytes()[text.len() - 1];
    (closing == text.as_bytes()[quote] && quote < text.len() - 1)
        .then(|| attribute.start + quote + 1..attribute.end - 1)
}

fn shorten(href: &str) -> String {
    const LIMIT: usize = 80;
    if href.chars().count() <= LIMIT {
        href.to_owned()
    } else {
        format!("{}…", href.chars().take(LIMIT).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn fit_to_content_grows_the_canvas_around_the_drawing() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 50"><rect x="-20" y="10" width="40" height="20"/></svg>"#;
        let fitted = fit_to_content(svg).expect("drawing leaves the canvas");
        let tree = resvg::usvg::Tree::from_str(&fitted, &crate::svg::options()).expect("valid svg");
        assert!((tree.size().width() - 40.0).abs() < 0.01, "{fitted}");
        assert!((tree.size().height() - 20.0).abs() < 0.01, "{fitted}");
        assert!(fit_to_content(&fitted).is_none());
        let inside = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10mm" height="10mm" viewBox="0 0 10 10"><rect width="10" height="10"/></svg>"#;
        assert!(fit_to_content(inside).is_none());
    }

    use super::*;
    use resvg::tiny_skia;

    struct TempDir(PathBuf);
    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("visicut-svg-import-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn red_png() -> Vec<u8> {
        let mut pixmap = tiny_skia::Pixmap::new(4, 4).unwrap();
        pixmap.fill(tiny_skia::Color::from_rgba8(255, 0, 0, 255));
        pixmap.encode_png().unwrap()
    }

    fn png_uri() -> String {
        format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(red_png())
        )
    }

    fn svg(image_attributes: &str) -> String {
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="10mm" height="10mm" viewBox="0 0 10 10">
  <rect width="1" height="1" fill="blue"/>
  <image x="0" y="0" width="10" height="10" {image_attributes}/>
</svg>"#
        )
    }

    fn file_url(path: &Path) -> String {
        let text = path.to_string_lossy().replace('\\', "/");
        let text = if text.starts_with('/') {
            text
        } else {
            format!("/{text}")
        };
        format!("file://{}", text.replace(' ', "%20"))
    }

    #[test]
    fn embeds_relative_path_and_keeps_rest_identical() {
        let dir = TempDir::new("relative");
        std::fs::create_dir_all(dir.0.join("img")).unwrap();
        std::fs::write(dir.0.join("img/red.png"), red_png()).unwrap();
        let source = svg(r#"href="img/red.png""#);
        let result = embed_external_images(&source, &dir.0);
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        assert_eq!(
            result.svg,
            source.replace("img/red.png", &png_uri()),
            "only the attribute value may change"
        );
    }

    #[test]
    fn embeds_parent_directory_path() {
        let dir = TempDir::new("parent");
        std::fs::create_dir_all(dir.0.join("svg")).unwrap();
        std::fs::write(dir.0.join("red.png"), red_png()).unwrap();
        let source = svg(r#"href="../red.png""#);
        let result = embed_external_images(&source, &dir.0.join("svg"));
        assert!(result.svg.contains(&png_uri()));
        // Outside the drawing's folder: embedded, but not silently.
        assert_eq!(result.warnings.len(), 1, "{:?}", result.warnings);
        assert!(result.warnings[0].contains("nicht im Ordner der Zeichnung"));
        assert!(result.warnings[0].contains("red.png"));
        // The same file twice is reported once.
        let twice = source.replace(
            "<rect",
            r#"<image width="1" height="1" href="../red.png"/><rect"#,
        );
        assert_eq!(
            embed_external_images(&twice, &dir.0.join("svg"))
                .warnings
                .len(),
            1
        );
    }

    #[test]
    fn network_paths_are_never_opened() {
        let dir = TempDir::new("network");
        for href in [
            r"\\attacker.example\share\red.png",
            "//attacker.example/share/red.png",
            r"\\?\UNC\attacker.example\share\red.png",
            "%5C%5Cattacker.example%5Cshare%5Cred.png",
            "file://attacker.example/share/red.png",
            "file:////attacker.example/share/red.png",
        ] {
            let source = svg(&format!(r#"href="{href}""#));
            let result = embed_external_images(&source, &dir.0);
            assert_eq!(result.svg, source, "{href}");
            assert_eq!(result.warnings.len(), 1, "{href}: {:?}", result.warnings);
        }
        assert!(is_network_path(Path::new(r"\\server\share")));
        assert!(is_network_path(Path::new("//server/share")));
        assert!(!is_network_path(Path::new("/home/user/red.png")));
        assert!(!is_network_path(Path::new("img/red.png")));
        // A sodipodi:absref on another computer is not tried either.
        let source = svg(r#"href="missing.png" sodipodi:absref="//attacker.example/s/red.png""#)
            .replace(
                "<svg ",
                r#"<svg xmlns:sodipodi="http://sodipodi.sourceforge.net/DTD/sodipodi-0.dtd" "#,
            );
        let result = embed_external_images(&source, &dir.0);
        assert!(result.warnings[0].contains("nicht gefunden"));
    }

    #[test]
    fn deeply_nested_files_are_refused_before_parsing() {
        let dir = TempDir::new("deep");
        let path = dir.0.join("deep.svg");
        let depth = 100_000;
        let deep = format!("<svg>{}{}</svg>", "<g>".repeat(depth), "</g>".repeat(depth));
        std::fs::write(&path, &deep).unwrap();
        let error = read_svg_file(&path).err().expect("refused");
        assert!(error.contains("verschachtelt"), "{error}");
        assert_eq!(embed_external_images(&deep, &dir.0).svg, deep);
        assert!(fit_to_content(&deep).is_none());
    }

    #[test]
    fn other_archives_are_not_embedded_as_compressed_svg() {
        let dir = TempDir::new("gzip");
        std::fs::write(dir.0.join("backup.tar.gz"), [0x1F, 0x8B, 8, 0, 0, 0]).unwrap();
        let source = svg(r#"href="backup.tar.gz""#);
        let result = embed_external_images(&source, &dir.0);
        assert_eq!(result.svg, source);
        assert!(result.warnings[0].contains("Format"));
    }

    #[test]
    fn embeds_percent_encoded_file_url() {
        let dir = TempDir::new("file url");
        let image = dir.0.join("my red.png");
        std::fs::write(&image, red_png()).unwrap();
        let url = file_url(&image);
        assert!(url.contains("%20"));
        let source = svg(&format!(r#"xlink:href="{url}""#));
        let result = embed_external_images(&source, &dir.0);
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        assert_eq!(result.svg, source.replace(&url, &png_uri()));
    }

    #[test]
    fn embeds_xlink_href_with_single_quotes() {
        let dir = TempDir::new("xlink");
        std::fs::write(dir.0.join("red.png"), red_png()).unwrap();
        let source = svg("xlink:href = 'red.png'");
        let result = embed_external_images(&source, &dir.0);
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        assert!(
            result
                .svg
                .contains(&format!("xlink:href = '{}'", png_uri()))
        );
    }

    #[test]
    fn decodes_entity_escaped_attribute_values() {
        let dir = TempDir::new("entity");
        std::fs::write(dir.0.join("a&b.png"), red_png()).unwrap();
        let source = svg(r#"href="a&amp;b&#46;png""#);
        let result = embed_external_images(&source, &dir.0);
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        assert_eq!(result.svg, source.replace("a&amp;b&#46;png", &png_uri()));
    }

    #[test]
    fn missing_file_is_reported_without_failing() {
        let dir = TempDir::new("missing");
        let source = svg(r#"href="missing.png""#);
        let result = embed_external_images(&source, &dir.0);
        assert_eq!(result.svg, source);
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("missing.png"));
        assert!(result.warnings[0].contains("nicht gefunden"));
    }

    #[test]
    fn network_and_embedded_images_stay_untouched() {
        let dir = TempDir::new("untouched");
        let remote = svg(r#"href="https://example.com/red.png""#);
        let result = embed_external_images(&remote, &dir.0);
        assert_eq!(result.svg, remote);
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("Netz"));

        let embedded = svg(&format!(r#"href="{}""#, png_uri()));
        let result = embed_external_images(&embedded, &dir.0);
        assert_eq!(result.svg, embedded);
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn rejects_unsupported_formats_and_oversized_images() {
        let dir = TempDir::new("formats");
        std::fs::write(dir.0.join("notes.txt"), "plain text").unwrap();
        let source = svg(r#"href="notes.txt""#);
        let result = embed_external_images(&source, &dir.0);
        assert_eq!(result.svg, source);
        assert!(result.warnings[0].contains("Format"));

        let big = std::fs::File::create(dir.0.join("big.png")).unwrap();
        big.set_len(MAX_IMAGE_BYTES + 1).unwrap();
        let source = svg(r#"href="big.png""#);
        let result = embed_external_images(&source, &dir.0);
        assert_eq!(result.svg, source);
        assert!(result.warnings[0].contains("10 MB"));
    }

    #[test]
    fn recognises_resvg_formats() {
        assert_eq!(mime_type(&red_png(), false), Some("image/png"));
        assert_eq!(
            mime_type(&[0xFF, 0xD8, 0xFF, 0xE0], false),
            Some("image/jpeg")
        );
        assert_eq!(mime_type(b"GIF89a....", false), Some("image/gif"));
        assert_eq!(
            mime_type(b"RIFF\0\0\0\0WEBPVP8 ", false),
            Some("image/webp")
        );
        assert_eq!(
            mime_type(b"\xEF\xBB\xBF<?xml version=\"1.0\"?>\n<svg/>", false),
            Some("image/svg+xml")
        );
        assert_eq!(mime_type(b"hello", false), None);
        assert_eq!(mime_type(&[0x1F, 0x8B, 8, 0], false), None);
        assert_eq!(mime_type(&[0x1F, 0x8B, 8, 0], true), Some("image/svg+xml"));
    }

    #[test]
    fn inlined_external_png_renders() {
        let dir = TempDir::new("render");
        std::fs::write(dir.0.join("red.png"), red_png()).unwrap();
        let path = dir.0.join("art.svg");
        // Only the image is drawn, so visible pixels must come from it.
        std::fs::write(
            &path,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="10mm" height="10mm" viewBox="0 0 10 10"><image width="10" height="10" href="red.png"/></svg>"#,
        )
        .unwrap();
        let before = crate::svg::render(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(before.image.pixels.iter().all(|p| p.a() == 0));
        let imported = read_svg_file(&path).unwrap();
        assert!(imported.warnings.is_empty(), "{:?}", imported.warnings);
        let after = crate::svg::render(&imported.svg).unwrap();
        assert!(
            after
                .image
                .pixels
                .iter()
                .any(|p| p.a() > 0 && p.r() > 200 && p.g() < 50)
        );
    }
}
