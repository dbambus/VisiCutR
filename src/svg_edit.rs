//! Text edits of SVG and CSS source for [`crate::compose`]: parsing,
//! embedding a motif, renaming ids and classes, numbers and path data as
//! text. The source is edited as text, so everything not touched stays
//! byte for byte.
use crate::{
    compose::{ComposeError, Shape},
    i18n::tr,
    project::Project,
    selection, svg,
    svg_import::value_range,
};
use resvg::{tiny_skia, usvg};
use std::fmt::Write;

/// Largest composed document, like [`crate::svg::render`].
const MAX_SVG_BYTES: usize = 20 * 1024 * 1024;

pub(crate) fn parse(source: &str) -> Result<roxmltree::Document<'_>, ComposeError> {
    if source.contains("<!ENTITY") {
        return Err(ComposeError::Entities);
    }
    let options = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..Default::default()
    };
    roxmltree::Document::parse_with_options(source, options).map_err(ComposeError::Xml)
}

pub(crate) fn check_size(body: &str) -> Result<(), ComposeError> {
    if body.len() > MAX_SVG_BYTES {
        return Err(ComposeError::TooLarge);
    }
    Ok(())
}

/// Byte index after the `>` of the start tag beginning at `start`.
pub(crate) fn start_tag_end(source: &str, start: usize) -> Option<usize> {
    let mut quote = None;
    for (i, c) in source[start..].char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None, '>') => return Some(start + i + 1),
            _ => {}
        }
    }
    None
}

/// The root start tag with the canvas size in pixels and visible overflow.
pub(crate) fn root_start(
    source: &str,
    root: roxmltree::Node,
    size: usvg::Size,
) -> Result<String, ComposeError> {
    let range = root.range();
    let end = start_tag_end(source, range.start).ok_or(ComposeError::RootIncomplete)?;
    let mut text = String::new();
    let mut position = range.start;
    for attribute in root.attributes() {
        if attribute.namespace().is_none()
            && matches!(
                attribute.name(),
                "x" | "y" | "width" | "height" | "overflow"
            )
        {
            let r = attribute.range();
            let mut start = r.start;
            while start > range.start && source.as_bytes()[start - 1].is_ascii_whitespace() {
                start -= 1;
            }
            text.push_str(&source[position..start]);
            position = r.end;
        }
    }
    let close = if source[..end].ends_with("/>") {
        end - 2
    } else {
        end - 1
    };
    text.push_str(&source[position..close]);
    text += &format!(
        r#" width="{}" height="{}" overflow="visible">"#,
        num(size.width() as f64),
        num(size.height() as f64)
    );
    Ok(text)
}

/// The motif as it lies on the bed: its root `<svg>` sized in canvas pixels
/// inside a group that scales to the project's position and size. `suffix`
/// is appended to all ids (references keep pointing at the first copy);
/// `visible` limits the drawn objects like a step selection.
pub(crate) fn embed(
    project: &Project,
    suffix: Option<&str>,
    visible: Option<&[usize]>,
) -> Result<String, ComposeError> {
    let tree =
        usvg::Tree::from_str(&project.svg, &svg::options()).map_err(svg::SvgError::Unreadable)?;
    let size = tree.size();
    let source = match visible {
        Some(objects) => selection::filter(&project.svg, objects)?,
        None => project.svg.clone(),
    };
    let doc = parse(&source)?;
    let root = doc.root_element();
    let range = root.range();
    let start_end = start_tag_end(&source, range.start).ok_or(ComposeError::RootIncomplete)?;
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    for attribute in root.attributes() {
        if attribute.namespace().is_none()
            && matches!(
                attribute.name(),
                "x" | "y" | "width" | "height" | "overflow"
            )
        {
            // Remove the attribute including the white space before it.
            let r = attribute.range();
            let mut start = r.start;
            while start > range.start && source.as_bytes()[start - 1].is_ascii_whitespace() {
                start -= 1;
            }
            edits.push((start..r.end, String::new()));
        }
    }
    let close = if source[..start_end].ends_with("/>") {
        start_end - 2
    } else {
        start_end - 1
    };
    edits.push((
        close..close,
        format!(
            r#" width="{}" height="{}" overflow="visible""#,
            num(size.width() as f64),
            num(size.height() as f64)
        ),
    ));
    if let Some(suffix) = suffix {
        for node in root.descendants() {
            for attribute in node.attributes() {
                if attribute.namespace().is_none()
                    && attribute.name() == "id"
                    && let Some(value) = value_range(&source, attribute.range())
                {
                    edits.push((value.end..value.end, suffix.to_string()));
                }
            }
        }
    }
    edits.sort_by_key(|(r, _)| (r.start, r.end));
    let mut text = String::with_capacity(range.len() + 256);
    let mut position = range.start;
    for (r, replacement) in edits {
        text.push_str(&source[position..r.start]);
        text.push_str(&replacement);
        position = r.end;
    }
    text.push_str(&source[position..range.end]);
    let (sx, sy) = (
        project.width_mm as f64 / size.width() as f64,
        project.height_mm as f64 / size.height() as f64,
    );
    Ok(format!(
        r#"<g transform="matrix({} 0 0 {} {} {})">{text}</g>"#,
        num(sx),
        num(sy),
        num(project.x_mm as f64),
        num(project.y_mm as f64)
    ))
}

/// Renames ids and classes of `source` that also occur in `existing` (as
/// attributes or in style sheets), including references and selectors, so
/// both drawings keep their own definitions and styles in one document.
pub(crate) fn scope_names(existing: &str, source: &str) -> Result<String, ComposeError> {
    use std::collections::{HashMap, HashSet};
    let old = parse(existing)?;
    let mut taken: HashSet<String> = HashSet::new();
    for node in old.descendants().filter(|n| n.is_element()) {
        if let Some(id) = node.attribute("id") {
            taken.insert(id.to_string());
        }
        if let Some(class) = node.attribute("class") {
            taken.extend(class.split_whitespace().map(String::from));
        }
        if node.tag_name().name() == "style" {
            for text in node.children().filter_map(|c| c.text()) {
                css_tokens(text, &mut taken);
            }
        }
    }
    let doc = parse(source)?;
    let mut own: HashSet<String> = HashSet::new();
    for node in doc.descendants().filter(|n| n.is_element()) {
        own.extend(node.attribute("id").map(String::from));
        if let Some(class) = node.attribute("class") {
            own.extend(class.split_whitespace().map(String::from));
        }
    }
    let mut renamed: HashMap<String, String> = HashMap::new();
    let mut fresh = |name: &str, renamed: &mut HashMap<String, String>| {
        if taken.contains(name) && !renamed.contains_key(name) {
            let new = (2..)
                .map(|k| format!("{name}-{k}"))
                .find(|n| !taken.contains(n) && !own.contains(n))
                .expect("unendlich viele Namen");
            taken.insert(new.clone());
            renamed.insert(name.to_string(), new);
        }
    };
    for node in doc.descendants().filter(|n| n.is_element()) {
        if let Some(id) = node.attribute("id") {
            fresh(id, &mut renamed);
        }
        if let Some(class) = node.attribute("class") {
            for token in class.split_whitespace() {
                fresh(token, &mut renamed);
            }
        }
    }
    if renamed.is_empty() {
        return Ok(source.to_string());
    }
    Ok(renamed_source(source, &doc, &renamed))
}

/// `source` with the names in `renamed` replaced: attribute values, style
/// sheets and `url(#…)` references.
fn renamed_source(
    source: &str,
    doc: &roxmltree::Document,
    renamed: &std::collections::HashMap<String, String>,
) -> String {
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    for node in doc.descendants().filter(|n| n.is_element()) {
        for attribute in node.attributes() {
            let Some(value) = value_range(source, attribute.range()) else {
                continue;
            };
            let text = &source[value.clone()];
            let replacement = match (attribute.namespace(), attribute.name()) {
                (None, "id") => renamed.get(text).cloned(),
                (None, "class") => Some(
                    text.split_whitespace()
                        .map(|t| renamed.get(t).map_or(t, String::as_str))
                        .collect::<Vec<_>>()
                        .join(" "),
                ),
                (_, "href") => text
                    .strip_prefix('#')
                    .and_then(|id| renamed.get(id))
                    .map(|id| format!("#{id}")),
                _ => None,
            };
            if let Some(replacement) = replacement.filter(|r| r != text) {
                edits.push((value, replacement));
            }
        }
        if node.tag_name().name() == "style" {
            let range = node.range();
            if let Some(start) = start_tag_end(source, range.start)
                && let Some(end) = source[range.clone()].rfind("</").map(|i| range.start + i)
                && start <= end
            {
                edits.push((start..end, rename_css(&source[start..end], renamed)));
            }
        }
    }
    edits.sort_by_key(|(r, _)| r.start);
    let mut text = String::with_capacity(source.len() + 64);
    let mut position = 0;
    for (r, replacement) in edits {
        text.push_str(&source[position..r.start]);
        text.push_str(&replacement);
        position = r.end;
    }
    text.push_str(&source[position..]);
    // References in presentation attributes and style attributes.
    for (old, new) in renamed {
        for quote in ["", "'", "\""] {
            text = text.replace(
                &format!("url({quote}#{old}{quote})"),
                &format!("url({quote}#{new}{quote})"),
            );
        }
    }
    text
}

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

/// Names after `#` and `.` in a style sheet.
fn css_tokens(css: &str, names: &mut std::collections::HashSet<String>) {
    for (i, c) in css.char_indices() {
        if c == '#' || c == '.' {
            let rest = &css[i + 1..];
            let len = rest.find(|c: char| !is_name_char(c)).unwrap_or(rest.len());
            if len > 0 {
                names.insert(rest[..len].to_string());
            }
        }
    }
}

/// Style sheet with `#id` and `.class` names renamed.
fn rename_css(css: &str, renamed: &std::collections::HashMap<String, String>) -> String {
    let mut text = String::with_capacity(css.len());
    let mut position = 0;
    let mut i = 0;
    while i < css.len() {
        let c = css[i..]
            .chars()
            .next()
            .expect("i liegt vor dem Ende auf einer Zeichengrenze");
        if c == '#' || c == '.' {
            let rest = &css[i + 1..];
            let len = rest.find(|c: char| !is_name_char(c)).unwrap_or(rest.len());
            if let Some(new) = renamed.get(&rest[..len]) {
                text.push_str(&css[position..=i]);
                text.push_str(new);
                position = i + 1 + len;
            }
            i += 1 + len;
        } else {
            i += c.len_utf8();
        }
    }
    text.push_str(&css[position..]);
    text
}

pub(crate) fn unique_id(svg: &str, shape: &Shape, number: usize) -> String {
    // The id names the object in the object list, so it uses the chosen
    // language; it is stored in the document and stays when the language
    // changes.
    let base = id_base(match shape {
        Shape::Rectangle { .. } => tr("Rechteck"),
        Shape::Ellipse { .. } => tr("Ellipse"),
        Shape::Polygon { .. } => tr("Vieleck"),
        Shape::Star { .. } => tr("Stern"),
        Shape::Text { .. } => tr("Text"),
    });
    let mut n = number + 1;
    while svg.contains(&format!("id=\"{base}{n}\"")) {
        n += 1;
    }
    format!("{base}{n}")
}

/// A (translated) name as the start of an XML id: letters, digits, `-` and
/// `_` stay, everything else becomes `-`; it starts with a letter or `_`.
pub(crate) fn id_base(name: &str) -> String {
    let mut base: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    if !base.starts_with(|c: char| c.is_alphabetic() || c == '_') {
        base.insert(0, '_');
    }
    base
}

pub(crate) fn num(value: f64) -> String {
    let text = format!("{value:.4}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" {
        "0".into()
    } else {
        text.into()
    }
}

pub(crate) fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub(crate) fn collect(group: &usvg::Group, d: &mut String, bounds: &mut Option<tiny_skia::Rect>) {
    for node in group.children() {
        match node {
            usvg::Node::Group(g) => collect(g, d, bounds),
            usvg::Node::Text(t) => collect(t.flattened(), d, bounds),
            usvg::Node::Path(p) => {
                let Some(path) = p.data().clone().transform(p.abs_transform()) else {
                    continue;
                };
                let tight = path.compute_tight_bounds();
                *bounds = match (*bounds, tight) {
                    (Some(a), Some(b)) => tiny_skia::Rect::from_ltrb(
                        a.left().min(b.left()),
                        a.top().min(b.top()),
                        a.right().max(b.right()),
                        a.bottom().max(b.bottom()),
                    ),
                    (a, b) => a.or(b),
                };
                write_path(&path, d);
            }
            usvg::Node::Image(_) => {}
        }
    }
}

fn write_path(path: &tiny_skia::Path, d: &mut String) {
    use tiny_skia::PathSegment as S;
    let p = |pt: tiny_skia::Point| format!("{} {}", num(pt.x as f64), num(pt.y as f64));
    for segment in path.segments() {
        match segment {
            S::MoveTo(a) => write!(d, "M{}", p(a)),
            S::LineTo(a) => write!(d, "L{}", p(a)),
            S::QuadTo(a, b) => write!(d, "Q{} {}", p(a), p(b)),
            S::CubicTo(a, b, c) => write!(d, "C{} {} {}", p(a), p(b), p(c)),
            S::Close => write!(d, "Z"),
        }
        .expect("Schreiben in einen String gelingt immer");
    }
}

/// Moves absolute path data (as written here: M, L, H, V, A, Q, C, Z with
/// absolute coordinates) by `(dx, dy)`.
pub(crate) fn transform_path(d: &str, dx: f64, dy: f64, out: &mut String) {
    let mut command = ' ';
    let mut index = 0;
    let mut number = String::new();
    let flush = |number: &mut String, command: char, index: &mut usize, out: &mut String| {
        if number.is_empty() {
            return;
        }
        let value: f64 = number.parse().unwrap_or(0.0);
        // Which argument of the command is an x or y coordinate.
        let shifted = match command {
            'H' => value + dx,
            'V' => value + dy,
            'A' => match *index % 7 {
                5 => value + dx,
                6 => value + dy,
                _ => value,
            },
            _ => {
                if (*index).is_multiple_of(2) {
                    value + dx
                } else {
                    value + dy
                }
            }
        };
        if !out.ends_with(|c: char| c.is_ascii_alphabetic()) && !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&num(shifted));
        *index += 1;
        number.clear();
    };
    for c in d.chars() {
        if c.is_ascii_alphabetic() && c != 'e' {
            flush(&mut number, command, &mut index, out);
            command = c;
            index = 0;
            out.push(c);
        } else if c == ' ' || c == ',' {
            flush(&mut number, command, &mut index, out);
        } else if c == '-' && !number.is_empty() && !number.ends_with('e') {
            flush(&mut number, command, &mut index, out);
            number.push(c);
        } else {
            number.push(c);
        }
    }
    flush(&mut number, command, &mut index, out);
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const STYLED: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="40mm" height="20mm" viewBox="0 0 40 20"><defs><style>.cls-1{fill:#00ff00}</style><linearGradient id="g"><stop offset="0" stop-color="#000"/></linearGradient></defs><rect id="r" class="cls-1" width="10" height="10"/><rect width="10" height="10" x="20" fill="url(#g)"/><use xlink:href="#r" x="12"/></svg>"##;

    #[test]
    fn unrelated_names_stay_unchanged() {
        let other = STYLED
            .replace("cls-1", "x")
            .replace("\"r\"", "\"q\"")
            .replace("#r", "#q")
            .replace("\"g\"", "\"h\"")
            .replace("#g", "#h");
        assert_eq!(scope_names(STYLED, &other).unwrap(), other);
    }

    #[test]
    fn moving_path_data_shifts_coordinates_only() {
        let mut out = String::new();
        transform_path("M1 2H5V-3A2 2 0 0 1 4 5Z", 10.0, 100.0, &mut out);
        assert_eq!(out, "M11 102H15V97A2 2 0 0 1 14 105Z");
    }
}
