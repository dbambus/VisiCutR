use crate::project::Project;
use crate::{i18n::tr, tr};
use resvg::{tiny_skia, usvg};
use std::sync::{Arc, Mutex};

/// Point on the bed in mm. f64 like Java's geometry, so the driver's integer
/// truncations see the same values (tests/java_parity).
pub type Point = [f64; 2];
pub type Contour = Vec<Point>;

/// Pixels per mm of the 500-DPI vector profile (`Util.dpi2dpmm(500)`).
pub const PX_PER_MM: f64 = 500.0 / 25.4;
/// Flatness of curve flattening in pixels: the LTT driver's
/// `getRequiredCurvePrecision` with tangent curves.
const CURVE_FLATNESS_PX: f64 = 0.2;
/// Subdivision limit of Java's `FlatteningPathIterator`.
const CURVE_LIMIT: u32 = 10;

/// Affine matrix `[a, b, c, d, e, f]` as in SVG `matrix()`, in f64. Java maps
/// SVG coordinates to millimetres in double; usvg/tiny-skia work in f32, whose
/// rounding moves single points across 500-DPI pixel boundaries
/// (tests/java_parity), so the transforms are applied here in f64.
type Affine = [f64; 6];

fn affine_of(t: tiny_skia::Transform) -> Affine {
    [
        f64::from(t.sx),
        f64::from(t.ky),
        f64::from(t.kx),
        f64::from(t.sy),
        f64::from(t.tx),
        f64::from(t.ty),
    ]
}

/// `outer` applied after `inner`.
fn affine_mul(outer: Affine, inner: Affine) -> Affine {
    let [a, b, c, d, e, f] = outer;
    let [g, h, i, j, k, l] = inner;
    [
        a * g + c * h,
        b * g + d * h,
        a * i + c * j,
        b * i + d * j,
        a * k + c * l + e,
        b * k + d * l + f,
    ]
}

fn affine_apply(t: Affine, x: f64, y: f64) -> [f64; 2] {
    [t[0] * x + t[2] * y + t[4], t[1] * x + t[3] * y + t[5]]
}

fn affine_near(a: Affine, b: Affine) -> bool {
    a.iter()
        .zip(b)
        .all(|(x, y)| (x - y).abs() <= 1e-3 + 1e-5 * x.abs().max(y.abs()))
}

/// The transform usvg derives from the root `viewBox` and `preserveAspectRatio`
/// (`view_box_to_transform`), computed in f64 from the attributes and the
/// part's size in mm, so that equal aspect ratios give exactly equal scales
/// like Java's `Helper.getTransform` (usvg compares two f32 pixel sizes). usvg
/// stores its f32 version in the first group below the root; `visit` swaps it
/// for this one, expressed in the same pixel units.
fn view_box_transform(source: &str, size: usvg::Size, project: &Project) -> Option<Affine> {
    let document = usvg::roxmltree::Document::parse_with_options(
        source,
        usvg::roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        },
    )
    .ok()?;
    let root = document.root_element();
    let numbers: Vec<f64> = root
        .attribute("viewBox")?
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|t| !t.is_empty())
        .map(|t| t.parse().ok())
        .collect::<Option<_>>()?;
    let [vx, vy, vw, vh] = numbers[..] else {
        return None;
    };
    if vw <= 0.0 || vh <= 0.0 {
        return None;
    }
    let (w, h) = (f64::from(project.width_mm), f64::from(project.height_mm));
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let aspect = root
        .attribute("preserveAspectRatio")
        .unwrap_or("xMidYMid meet");
    let mut words = aspect.split_whitespace().filter(|w| *w != "defer");
    let align = words.next().unwrap_or("xMidYMid");
    let slice = words.next() == Some("slice");
    let (mut sx, mut sy) = (w / vw, h / vh);
    if align != "none" {
        // Sizes that only differ by f32 rounding (38.1 mm is not exact) mean
        // equal scales and no centring, as in Java's independent x/y scales.
        let s = if (sx - sy).abs() <= 1e-6 * sx.abs() {
            sx
        } else if slice {
            sx.max(sy)
        } else {
            sx.min(sy)
        };
        (sx, sy) = (s, s);
    }
    let (x, y) = (-vx * sx, -vy * sy);
    let (dw, dh) = (w - vw * sx, h - vh * sy);
    let (tx, ty) = match align {
        "none" | "xMinYMin" => (x, y),
        "xMidYMin" => (x + dw / 2.0, y),
        "xMaxYMin" => (x + dw, y),
        "xMinYMid" => (x, y + dh / 2.0),
        "xMaxYMid" => (x + dw, y + dh / 2.0),
        "xMinYMax" => (x, y + dh),
        "xMidYMax" => (x + dw / 2.0, y + dh),
        "xMaxYMax" => (x + dw, y + dh),
        _ => (x + dw / 2.0, y + dh / 2.0),
    };
    // Back to usvg's pixel units; `flatten_path` divides them out again.
    let (kx, ky) = (f64::from(size.width()) / w, f64::from(size.height()) / h);
    Some([sx * kx, 0.0, 0.0, sy * ky, tx * kx, ty * ky])
}

/// A copy of the document in which `stroke-width` 0 becomes a hairline, so
/// usvg keeps the stroke: VisiCut cuts such contours (the hairline convention
/// of some CAD and Corel exports), and so does the PDF import (0 w).
fn keep_zero_width_strokes(svg: &str) -> String {
    let mut out = String::with_capacity(svg.len());
    let mut rest = svg;
    while let Some(i) = rest.find("stroke-width") {
        let (head, tail) = rest.split_at(i + "stroke-width".len());
        out.push_str(head);
        rest = tail;
        let bytes = tail.as_bytes();
        let mut j = 0;
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if j >= bytes.len() || !matches!(bytes[j], b'=' | b':') {
            continue;
        }
        j += 1;
        while j < bytes.len()
            && (bytes[j].is_ascii_whitespace() || matches!(bytes[j], b'"' | b'\''))
        {
            j += 1;
        }
        let start = j;
        while j < bytes.len()
            && (bytes[j].is_ascii_digit() || matches!(bytes[j], b'.' | b'-' | b'+' | b'e' | b'E'))
        {
            j += 1;
        }
        let token = &tail[start..j];
        if !token.is_empty() && token.parse::<f64>().is_ok_and(|v| v == 0.0) {
            out.push_str(&tail[..start]);
            out.push_str("0.001");
            rest = &tail[j..];
        }
    }
    out.push_str(rest);
    out
}

pub fn contours(project: &Project) -> Result<Vec<Contour>, String> {
    contours_with_fonts(project, crate::svg::fonts())
}

/// Power and speed in percent after a `set` of a LaserScript; `None` keeps
/// the value of the step.
pub type ScriptSetting = (Option<f32>, Option<f32>);

/// One polyline of a LaserScript part in mm on the bed, with the `set` calls
/// around it (see `import::plf::laserscript::Segment`).
#[derive(Debug)]
pub struct ScriptPath {
    pub points: Contour,
    /// `set` calls before the path, in order.
    pub sets: Vec<ScriptSetting>,
    /// `set` calls after it (only after the last line of the script).
    pub after: Vec<ScriptSetting>,
    /// Continues at the end of the previous path without a move.
    pub joined: bool,
}

/// The paths of a LaserScript part in script order, or `None` for a drawing
/// that does not consist of LaserScript paths only. VisiCut runs the script
/// again for the job; its `set` calls change power and speed there, and the
/// lines keep the script's order.
pub fn laser_script(project: &Project) -> Result<Option<Vec<ScriptPath>>, String> {
    if !project.svg.contains("laserscript") {
        return Ok(None);
    }
    fn walk(
        group: &usvg::Group,
        project: &Project,
        size: usvg::Size,
        paths: &mut Vec<ScriptPath>,
        other: &mut bool,
    ) {
        for node in group.children() {
            match node {
                usvg::Node::Group(g) => walk(g, project, size, paths, other),
                usvg::Node::Path(path) if path.is_visible() => {
                    let Some((sets, after, joined)) = script_settings(path.id()) else {
                        *other = true;
                        continue;
                    };
                    let subpaths =
                        flatten_path(path.data(), affine_of(path.abs_transform()), project, size);
                    let last = subpaths.len().saturating_sub(1);
                    for (i, (points, _)) in subpaths.into_iter().enumerate() {
                        paths.push(ScriptPath {
                            points,
                            sets: if i == 0 { sets.clone() } else { Vec::new() },
                            after: if i == last { after.clone() } else { Vec::new() },
                            joined: joined && i == 0,
                        });
                    }
                }
                usvg::Node::Path(_) => {}
                _ => *other = true,
            }
        }
    }
    let tree =
        usvg::Tree::from_str(&project.svg, &crate::svg::options()).map_err(|e| e.to_string())?;
    let (mut paths, mut other) = (Vec::new(), false);
    walk(tree.root(), project, tree.size(), &mut paths, &mut other);
    Ok((!other && !paths.is_empty()).then_some(paths))
}

/// The `set` calls before and after a LaserScript path and whether it is
/// joined, from its id (`laserscript-<n>[_e<power>:<speed>]…[_j][_t…]…`,
/// `n` for a value not set, possibly with a prefix for duplicated parts);
/// the single `laserscript` path of older imports has none.
fn script_settings(id: &str) -> Option<(Vec<ScriptSetting>, Vec<ScriptSetting>, bool)> {
    let rest = &id[id.rfind("laserscript")? + "laserscript".len()..];
    if rest.is_empty() {
        return Some((Vec::new(), Vec::new(), false));
    }
    let mut fields = rest.strip_prefix('-')?.split('_');
    fields.next()?.parse::<usize>().ok()?;
    let setting = |text: &str| -> Option<ScriptSetting> {
        let value = |v: &str| -> Option<Option<f32>> {
            if v == "n" {
                Some(None)
            } else {
                v.parse().ok().map(Some)
            }
        };
        let (power, speed) = text.split_once(':')?;
        Some((value(power)?, value(speed)?))
    };
    let (mut sets, mut after, mut joined) = (Vec::new(), Vec::new(), false);
    for field in fields {
        match field.split_at_checked(1)? {
            ("e", value) => sets.push(setting(value)?),
            ("t", value) => after.push(setting(value)?),
            ("j", "") => joined = true,
            _ => return None,
        }
    }
    Some((sets, after, joined))
}

/// Outline of one object (index as in `selection::objects`) in mm relative
/// to the top left corner of the motif: its contours, or for images and
/// other objects without contours their bounding rectangle. For highlighting
/// the object under the mouse in the object list.
pub fn object_outline(project: &Project, index: usize) -> Result<Vec<Contour>, String> {
    let svg = crate::selection::filter(&project.svg, &[index])?;
    let alone = Project {
        svg,
        x_mm: 0.0,
        y_mm: 0.0,
        steps: Vec::new(),
        parts: Vec::new(),
        ..project.clone()
    };
    // Rasterbilder haben keine Konturen (dort meldet `contours` einen Fehler).
    let outline = contours(&alone).unwrap_or_default();
    if !outline.is_empty() {
        return Ok(outline);
    }
    // Sonst der Rahmen der gezeichneten Pixel.
    let rendered = crate::svg::render(&alone.svg)?;
    let [w, h] = rendered.image.size;
    let mut found: Option<[usize; 4]> = None;
    for (i, pixel) in rendered.image.pixels.iter().enumerate() {
        if pixel.a() > 0 {
            let (x, y) = (i % w, i / w);
            found = Some(match found {
                Some([x0, y0, x1, y1]) => [x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1)],
                None => [x, y, x + 1, y + 1],
            });
        }
    }
    let Some([px0, py0, px1, py1]) = found else {
        return Ok(Vec::new());
    };
    let (sx, sy) = (
        f64::from(alone.width_mm) / w as f64,
        f64::from(alone.height_mm) / h as f64,
    );
    let (x0, y0) = (px0 as f64 * sx, py0 as f64 * sy);
    let (x1, y1) = (px1 as f64 * sx, py1 as f64 * sy);
    Ok(vec![vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1], [x0, y0]]])
}

/// Outlines of all objects (see `object_outline`), for hit testing.
pub fn object_outlines(project: &Project) -> Result<Vec<Vec<Contour>>, String> {
    let count = crate::selection::objects(&project.svg)?.len();
    (0..count).map(|i| object_outline(project, i)).collect()
}

/// The object under `point` (mm relative to the motif): first one whose line
/// lies within `tolerance` (topmost first), otherwise the smallest closed
/// outline containing the point, so a shape inside a frame wins over the
/// frame.
pub fn object_at(outlines: &[Vec<Contour>], point: Point, tolerance: f64) -> Option<usize> {
    let near = |a: Point, b: Point| {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let length = dx * dx + dy * dy;
        let t = if length > 0.0 {
            (((point[0] - a[0]) * dx + (point[1] - a[1]) * dy) / length).clamp(0.0, 1.0)
        } else {
            0.0
        };
        (a[0] + t * dx - point[0]).hypot(a[1] + t * dy - point[1]) <= tolerance
    };
    for (index, outline) in outlines.iter().enumerate().rev() {
        if outline
            .iter()
            .any(|contour| contour.windows(2).any(|s| near(s[0], s[1])))
        {
            return Some(index);
        }
    }
    let mut best: Option<(f64, usize)> = None;
    for (index, outline) in outlines.iter().enumerate() {
        for contour in outline {
            let closed = contour.len() > 3 && contour.first() == contour.last();
            if !closed {
                continue;
            }
            // Ray casting (even-odd).
            let mut inside = false;
            for s in contour.windows(2) {
                let (a, b) = (s[0], s[1]);
                if (a[1] > point[1]) != (b[1] > point[1])
                    && point[0] < (b[0] - a[0]) * (point[1] - a[1]) / (b[1] - a[1]) + a[0]
                {
                    inside = !inside;
                }
            }
            if inside {
                let area = contour
                    .windows(2)
                    .map(|s| s[0][0] * s[1][1] - s[1][0] * s[0][1])
                    .sum::<f64>()
                    .abs()
                    / 2.0;
                if best.is_none_or(|(a, _)| area < a) {
                    best = Some((area, index));
                }
            }
        }
    }
    best.map(|(_, index)| index)
}

/// Boundary of everything the objects cover, like VisiCut's
/// `VectorProfile.calculateOuterShape` for "useOutline": the union of the
/// areas of all shapes (each with its fill rule; open subpaths count as
/// closed, so plain lines add nothing) and of the rectangles of images. The
/// result are closed rings in mm, holes of the union included. Clip paths,
/// masks and dashes do not change the area.
pub fn outline_contours(project: &Project) -> Result<Vec<Contour>, String> {
    use i_overlay::core::fill_rule::FillRule;
    use i_overlay::float::simplify::SimplifyShape;
    let missing = Mutex::new(Vec::new());
    let options = usvg::Options {
        fontdb: crate::svg::fonts(),
        font_resolver: outline_fonts(&missing),
        ..crate::svg::options()
    };
    let source = with_ids(&project.svg).unwrap_or_else(|| project.svg.clone());
    let mut tree = usvg::Tree::from_str(&source, &options).map_err(|e| e.to_string())?;
    if let Some(problem) = missing.lock().map_err(|e| e.to_string())?.first() {
        return Err(problem.clone());
    }
    let even_odd = even_odd_ids(&source, &options);
    if contains_text(tree.root()) {
        let flattened = tree.to_string(&usvg::WriteOptions::default());
        tree = usvg::Tree::from_str(&flattened, &options).map_err(|e| e.to_string())?;
    }
    // Each object's own area, oriented consistently; together with the
    // non-zero rule their rings then form the union.
    let mut rings: Vec<Vec<[f64; 2]>> = Vec::new();
    let mut add = |subpaths: Vec<(Contour, bool)>, rule: FillRule| {
        let shape: Vec<Vec<[f64; 2]>> = subpaths
            .into_iter()
            .map(|(c, _)| c)
            .filter(|c| c.len() >= 3)
            .collect();
        for area in shape.simplify_shape(rule) {
            rings.extend(area);
        }
    };
    collect_areas(tree.root(), project, tree.size(), &even_odd, &mut add);
    let union = rings.simplify_shape(FillRule::NonZero);
    // Each area is its outer ring followed by its holes.
    let mut result: Vec<Contour> = union
        .into_iter()
        .flat_map(|area| area.into_iter().enumerate().map(|(i, ring)| (ring, i > 0)))
        .filter(|(ring, _)| ring.len() >= 3)
        .map(|(ring, hole)| {
            let mut ring = java_ring(ring, hole);
            ring.push(ring[0]);
            ring
        })
        .collect();
    if result.is_empty() {
        return Err(tr("Nur Umriss: Die Objekte umschließen keine Fläche").into());
    }
    // Outer rings first, from the top left, so the order does not depend on
    // the overlay library.
    result.sort_by(|a, b| {
        let key = |c: &Contour| {
            c.iter().fold([f64::MAX, f64::MAX], |m, p| {
                [m[0].min(p[1]), m[1].min(p[0])]
            })
        };
        key(a)
            .partial_cmp(&key(b))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(result)
}

/// Calls `add` with the flattened subpaths and fill rule of every visible
/// shape and with the rectangle of every image.
/// A ring as `java.awt.geom.Area` (VisiCut's outline) returns it: outer
/// rings counter-clockwise on screen, holes clockwise, each starting at the
/// top vertex from which an edge leads downwards (the leftmost of several).
fn java_ring(mut ring: Contour, hole: bool) -> Contour {
    let n = ring.len();
    // Shoelace sum with y downwards: negative is counter-clockwise on screen.
    let area: f64 = (0..n)
        .map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum();
    if (area < 0.0) == hole {
        ring.reverse();
    }
    // Within 1e-6 mm, so rounding in the transform does not pick a corner.
    let top = ring.iter().map(|p| p[1]).fold(f64::MAX, f64::min) + 1e-6;
    let start = (0..n)
        .filter(|&i| ring[i][1] <= top && ring[(i + 1) % n][1] > top)
        .min_by(|&a, &b| ring[a][0].total_cmp(&ring[b][0]));
    if let Some(start) = start {
        ring.rotate_left(start);
    }
    ring
}

/// The document with an `id` on every element that has none, so the two
/// parses in [`even_odd_ids`] can be matched; `None` if it is not XML.
fn with_ids(svg: &str) -> Option<String> {
    let document = usvg::roxmltree::Document::parse_with_options(
        svg,
        usvg::roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        },
    )
    .ok()?;
    let mut inserts: Vec<usize> = document
        .descendants()
        .filter(|n| n.is_element() && n.attribute("id").is_none())
        .map(|n| {
            // Right behind the tag name as written (prefix included).
            let start = n.range().start + 1;
            start
                + svg[start..]
                    .find(|c: char| c.is_whitespace() || c == '/' || c == '>')
                    .unwrap_or(0)
        })
        .collect();
    inserts.sort_unstable();
    let mut result = svg.to_owned();
    for (k, at) in inserts.iter().enumerate().rev() {
        result.insert_str(*at, &format!(" id=\"visicut-outline-{k}\""));
    }
    Some(result)
}

/// Ids of the paths whose fill rule is `evenodd`. usvg drops the fill of
/// `fill="none"` shapes and with it the rule, which still shapes their area
/// (SVG; Java's outline uses it too); a second parse forces a fill to read it.
fn even_odd_ids(source: &str, options: &usvg::Options) -> std::collections::HashSet<String> {
    fn walk(group: &usvg::Group, ids: &mut std::collections::HashSet<String>) {
        for node in group.children() {
            match node {
                usvg::Node::Group(g) => walk(g, ids),
                usvg::Node::Path(path)
                    if path
                        .fill()
                        .is_some_and(|f| f.rule() == usvg::FillRule::EvenOdd) =>
                {
                    ids.insert(path.id().to_owned());
                }
                _ => {}
            }
        }
    }
    let options = usvg::Options {
        style_sheet: Some("* { fill: #000 !important; }".into()),
        fontdb: options.fontdb.clone(),
        ..crate::svg::options()
    };
    let mut ids = std::collections::HashSet::new();
    if let Ok(tree) = usvg::Tree::from_str(source, &options) {
        walk(tree.root(), &mut ids);
    }
    ids.remove("");
    ids
}

fn collect_areas(
    group: &usvg::Group,
    project: &Project,
    size: usvg::Size,
    even_odd: &std::collections::HashSet<String>,
    add: &mut impl FnMut(Vec<(Contour, bool)>, i_overlay::core::fill_rule::FillRule),
) {
    use i_overlay::core::fill_rule::FillRule;
    if group.opacity().get() == 0.0 {
        return;
    }
    for node in group.children() {
        match node {
            usvg::Node::Group(group) => collect_areas(group, project, size, even_odd, add),
            usvg::Node::Path(path) if path.is_visible() => {
                let rule = match path.fill().map(|f| f.rule()) {
                    Some(usvg::FillRule::EvenOdd) => FillRule::EvenOdd,
                    Some(_) => FillRule::NonZero,
                    None if even_odd.contains(path.id()) => FillRule::EvenOdd,
                    None => FillRule::NonZero,
                };
                add(
                    flatten_path(path.data(), affine_of(path.abs_transform()), project, size),
                    rule,
                );
            }
            usvg::Node::Image(image) => {
                let rect = tiny_skia::Rect::from_xywh(
                    0.0,
                    0.0,
                    image.size().width(),
                    image.size().height(),
                );
                if let Some(rect) = rect {
                    let data = tiny_skia::PathBuilder::from_rect(rect);
                    add(
                        flatten_path(&data, affine_of(image.abs_transform()), project, size),
                        FillRule::NonZero,
                    );
                }
            }
            _ => {}
        }
    }
}

fn contours_with_fonts(
    project: &Project,
    fonts: Arc<usvg::fontdb::Database>,
) -> Result<Vec<Contour>, String> {
    let document = usvg::roxmltree::Document::parse(&project.svg).map_err(|e| e.to_string())?;
    for node in document.descendants().filter(|node| node.is_element()) {
        if node.tag_name().name() == "image"
            && !node.ancestors().any(|n| {
                matches!(
                    n.tag_name().name(),
                    "defs" | "symbol" | "clipPath" | "mask" | "pattern" | "marker"
                )
            })
        {
            return Err(tr(
                "SVG enthält Rasterbilder; zuerst in Pfade umwandeln oder Gravieren wählen",
            )
            .into());
        }
    }
    check_clip_references(&document)?;
    // Text is cut along its glyph outlines, like the Java importer does. Glyphs
    // usvg cannot place would otherwise vanish silently, so they are reported.
    let missing = Mutex::new(Vec::new());
    let options = usvg::Options {
        fontdb: fonts,
        font_resolver: outline_fonts(&missing),
        ..crate::svg::options()
    };
    let source = keep_zero_width_strokes(&project.svg);
    let mut tree = usvg::Tree::from_str(&source, &options).map_err(|e| e.to_string())?;
    if let Some(problem) = missing.lock().map_err(|e| e.to_string())?.first() {
        return Err(problem.clone());
    }
    let view_box = view_box_transform(&source, tree.size(), project);
    if contains_text(tree.root()) {
        // usvg keeps glyph outlines relative to their <text> element (the flattened
        // paths carry an identity transform); writing the tree back out places them
        // under their ancestors' transforms like ordinary paths.
        let flattened = tree.to_string(&usvg::WriteOptions::default());
        tree = usvg::Tree::from_str(&flattened, &options).map_err(|e| e.to_string())?;
    }
    let mut result = Vec::new();
    visit(
        tree.root(),
        project,
        tree.size(),
        &[],
        affine_of(tree.root().transform()),
        view_box,
        &mut result,
    )?;
    if result.is_empty() {
        return Err(tr("Keine schneidbaren Vektorpfade in der SVG").into());
    }
    if result.iter().map(Vec::len).sum::<usize>() > 1_000_000 {
        return Err(tr("Zu viele Vektorpunkte").into());
    }
    Ok(result)
}

fn outline_fonts(missing: &Mutex<Vec<String>>) -> usvg::FontResolver<'_> {
    let select = usvg::FontResolver::default_font_selector();
    let fallback = usvg::FontResolver::default_fallback_selector();
    let report = move |problem: String| {
        if let Ok(mut list) = missing.lock() {
            list.push(problem);
        }
    };
    usvg::FontResolver {
        select_font: Box::new(move |font, db| {
            let id = select(font, db);
            if id.is_none() {
                let families: Vec<String> = font.families().iter().map(|f| f.to_string()).collect();
                report(tr!(
                    "Schriftart für Text nicht gefunden ({fonts}); Schriftart installieren \
                     oder Text in Pfade umwandeln",
                    fonts = families.join(", ")
                ));
            }
            id
        }),
        select_fallback: Box::new(move |c, used, db| {
            let id = fallback(c, used, db);
            let invisible = c.is_whitespace()
                || c.is_control()
                || matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{206F}' | '\u{FE00}'..='\u{FE0F}' | '\u{FEFF}');
            if id.is_none() && !invisible {
                report(tr!(
                    "Zeichen „{character}“ (U+{code}) ist in keiner Schriftart enthalten; \
                     Schriftart installieren oder Text in Pfade umwandeln",
                    character = c,
                    code = format!("{:04X}", c as u32)
                ));
            }
            id
        }),
    }
}

fn contains_text(group: &usvg::Group) -> bool {
    group.children().iter().any(|node| match node {
        usvg::Node::Text(_) => true,
        usvg::Node::Group(group) => contains_text(group),
        _ => false,
    })
}

/// Masks and filters cannot be cut: a mask only sets transparency and a filter
/// changes the picture, so neither defines a cut line.
fn mask_rejected() -> &'static str {
    tr(
        "Masken können nicht geschnitten werden, weil sie nur Transparenz festlegen und keine \
        Schnittlinie ergeben; Maske entfernen oder Objekt vorher in Pfade umwandeln",
    )
}
fn filter_rejected() -> &'static str {
    tr(
        "Filter wie Weichzeichnen oder Schlagschatten verändern das Motiv nur als Pixelbild und \
        ergeben keine Schnittlinie; Filter entfernen oder Objekt vorher in Pfade umwandeln",
    )
}
fn nested_clip() -> &'static str {
    tr(
        "Verschachtelte Clip-Pfade (Clipping auf einem Clip-Pfad oder innerhalb eines \
        Clip-Pfads) werden beim Schneiden nicht unterstützt",
    )
}
fn empty_clip() -> &'static str {
    tr(
        "Clip-Pfad ist leer, daher bliebe nichts zu schneiden; Clip-Pfad mit Formen füllen \
        oder Clipping entfernen",
    )
}
fn clip_image() -> &'static str {
    tr(
        "Rasterbild im Clip-Pfad: Schneiden unterstützt keine Rasterbilder; Clip-Pfad nur mit \
        Vektorformen verwenden",
    )
}
fn clip_text() -> &'static str {
    tr("Text im Clip-Pfad wird beim Schneiden nicht unterstützt; Text vorher in Pfade umwandeln")
}

fn reject_effects(group: &usvg::Group) -> Result<(), String> {
    if group.mask().is_some() {
        return Err(mask_rejected().into());
    }
    if !group.filters().is_empty() {
        return Err(filter_rejected().into());
    }
    Ok(())
}

/// A clip region in mm: the union of the shapes of one clipPath. A point is
/// inside the region when one of its shapes contains it.
#[derive(Clone)]
struct Clip {
    shapes: Vec<ClipShape>,
}

#[derive(Clone)]
struct ClipShape {
    /// Closed rings in mm, last point equal to the first.
    rings: Vec<Contour>,
    even_odd: bool,
    bbox: [f64; 4],
}

impl ClipShape {
    fn new(rings: Vec<Contour>, even_odd: bool) -> Self {
        let bbox = rings
            .iter()
            .flatten()
            .fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |b, p| {
                [
                    b[0].min(p[0]),
                    b[1].min(p[1]),
                    b[2].max(p[0]),
                    b[3].max(p[1]),
                ]
            });
        Self {
            rings,
            even_odd,
            bbox,
        }
    }

    /// Winding and crossing counts of a ray from `p` to the right, summed over all rings.
    fn contains(&self, p: Point) -> bool {
        let [x0, y0, x1, y1] = self.bbox;
        if p[0] < x0 || p[0] > x1 || p[1] < y0 || p[1] > y1 {
            return false;
        }
        let (mut winding, mut crossings) = (0i32, 0u32);
        for ring in &self.rings {
            for edge in ring.windows(2) {
                let (a, b) = (edge[0], edge[1]);
                if a[1] <= p[1] {
                    if b[1] > p[1] && side(a, b, p) > 0.0 {
                        winding += 1;
                        crossings += 1;
                    }
                } else if b[1] <= p[1] && side(a, b, p) < 0.0 {
                    winding -= 1;
                    crossings += 1;
                }
            }
        }
        if self.even_odd {
            crossings % 2 == 1
        } else {
            winding != 0
        }
    }
}

impl Clip {
    fn contains(&self, p: Point) -> bool {
        self.shapes.iter().any(|shape| shape.contains(p))
    }
}

/// Positive when `p` lies left of the directed edge `a` → `b`.
fn side(a: Point, b: Point, p: Point) -> f64 {
    (b[0] - a[0]) * (p[1] - a[1]) - (p[0] - a[0]) * (b[1] - a[1])
}

/// Parameter `t` along `a` → `b` where it crosses the edge `c` → `d`, if the crossing
/// lies strictly inside the segment. Parallel edges are not reported; midpoint tests
/// decide those pieces.
fn crossing(a: Point, b: Point, c: Point, d: Point) -> Option<f64> {
    let p = |q: Point| (q[0], q[1]);
    let ((ax, ay), (bx, by), (cx, cy), (dx, dy)) = (p(a), p(b), p(c), p(d));
    let r = (bx - ax, by - ay);
    let s = (dx - cx, dy - cy);
    let denominator = r.0 * s.1 - r.1 * s.0;
    if denominator.abs() < 1e-12 {
        return None;
    }
    let qp = (cx - ax, cy - ay);
    let t = (qp.0 * s.1 - qp.1 * s.0) / denominator;
    let u = (qp.0 * r.1 - qp.1 * r.0) / denominator;
    ((0.0..1.0).contains(&t) && (0.0..=1.0).contains(&u)).then_some(t)
}

fn point_at(a: Point, b: Point, t: f64) -> Point {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

/// Work limit for clipping: contour segments times clip edges.
const CLIP_WORK_LIMIT: usize = 100_000_000;

/// The parts of an open or closed polyline that lie inside all clip regions.
/// Each segment is split where it crosses a clip edge; the pieces between
/// splits are kept when their midpoint is inside. A closed contour that stays
/// inside as a whole is returned unchanged; pieces that meet at the start point
/// of a closed contour are joined again.
fn clip_contour(contour: &Contour, clips: &[Clip]) -> Result<Vec<Contour>, String> {
    let edges: Vec<(Point, Point)> = clips
        .iter()
        .flat_map(|clip| &clip.shapes)
        .flat_map(|shape| &shape.rings)
        .flat_map(|ring| ring.windows(2).map(|w| (w[0], w[1])))
        .collect();
    if edges.len().saturating_mul(contour.len()) > CLIP_WORK_LIMIT {
        return Err(tr("Clip-Pfad ist zu komplex für die Schnittberechnung; \
                       Clip vereinfachen oder Objekt in Pfade umwandeln")
        .into());
    }
    let inside = |p: Point| clips.iter().all(|clip| clip.contains(p));
    let mut pieces: Vec<Contour> = Vec::new();
    let mut current: Option<Contour> = None;
    for segment in contour.windows(2) {
        let (a, b) = (segment[0], segment[1]);
        let mut ts = vec![0.0, 1.0];
        ts.extend(edges.iter().filter_map(|&(c, d)| crossing(a, b, c, d)));
        ts.sort_by(f64::total_cmp);
        ts.dedup();
        for pair in ts.windows(2) {
            let (t0, t1) = (pair[0], pair[1]);
            if inside(point_at(a, b, (t0 + t1) / 2.0)) {
                let end = if t1 == 1.0 { b } else { point_at(a, b, t1) };
                if let Some(piece) = current.as_mut() {
                    if piece.last() != Some(&end) {
                        piece.push(end);
                    }
                } else {
                    let start = if t0 == 0.0 { a } else { point_at(a, b, t0) };
                    current = Some(vec![start, end]);
                }
            } else if let Some(piece) = current.take() {
                pieces.push(piece);
            }
        }
    }
    pieces.extend(current);
    if contour.len() > 1 && contour.first() == contour.last() && pieces.len() >= 2 {
        let first_at_start = pieces.first().and_then(|piece| piece.first()) == contour.first();
        let last_at_end = pieces.last().and_then(|piece| piece.last()) == contour.last();
        if first_at_start && last_at_end {
            let first = pieces.remove(0);
            if let Some(last) = pieces.last_mut() {
                last.extend(first.into_iter().skip(1));
            }
        }
    }
    pieces.retain(|piece| piece.len() >= 2);
    Ok(pieces)
}

/// Rejects a `clipPath` reference whose target is missing, is not a clipPath or
/// has no children. usvg would drop such elements silently, so this is checked
/// on the XML before the tree is built.
fn check_clip_references(document: &usvg::roxmltree::Document) -> Result<(), String> {
    for node in document.descendants().filter(|n| n.is_element()) {
        let Some(id) = clip_reference(node) else {
            continue;
        };
        let target = document
            .descendants()
            .find(|n| n.is_element() && n.attribute("id") == Some(id.as_str()));
        match target {
            None => {
                return Err(tr!(
                    "Clip-Pfad „#{id}“ ist nicht vorhanden; Clipping entfernen oder Objekt \
                     vorher in Pfade umwandeln",
                    id = id
                ));
            }
            Some(target) if target.tag_name().name() != "clipPath" => {
                return Err(tr!("„#{id}“ ist kein Clip-Pfad", id = id));
            }
            Some(target) if !target.children().any(|child| child.is_element()) => {
                return Err(empty_clip().into());
            }
            Some(_) => {}
        }
    }
    Ok(())
}

/// Id of the element referenced by `clip-path`, as attribute or style property.
fn clip_reference(node: usvg::roxmltree::Node) -> Option<String> {
    let value = node.attribute("clip-path").or_else(|| {
        node.attribute("style")?.split(';').find_map(|declaration| {
            let (name, value) = declaration.split_once(':')?;
            (name.trim() == "clip-path").then(|| value.trim())
        })
    })?;
    let id = value
        .strip_prefix("url(")?
        .strip_suffix(')')?
        .trim()
        .trim_matches(|c| c == '\'' || c == '"')
        .strip_prefix('#')?;
    Some(id.to_string())
}

/// Region of a clipPath in mm; `base` maps the clipPath's user space (the
/// referencing element's transform) to user units.
fn clip_region(
    clip: &usvg::ClipPath,
    base: tiny_skia::Transform,
    project: &Project,
    size: usvg::Size,
) -> Result<Clip, String> {
    if clip.clip_path().is_some() {
        return Err(nested_clip().into());
    }
    let mut shapes = Vec::new();
    clip_shapes(
        clip.root(),
        base.pre_concat(clip.transform()),
        project,
        size,
        &mut shapes,
    )?;
    if shapes.is_empty() {
        return Err(empty_clip().into());
    }
    Ok(Clip { shapes })
}

fn clip_shapes(
    group: &usvg::Group,
    base: tiny_skia::Transform,
    project: &Project,
    size: usvg::Size,
    shapes: &mut Vec<ClipShape>,
) -> Result<(), String> {
    reject_effects(group)?;
    if group.clip_path().is_some() {
        return Err(nested_clip().into());
    }
    for node in group.children() {
        match node {
            usvg::Node::Group(group) => clip_shapes(group, base, project, size, shapes)?,
            // The clip geometry counts whatever its fill or stroke, like SVG's clip-path.
            usvg::Node::Path(path) if path.is_visible() => {
                let rings: Vec<Contour> = flatten_path(
                    path.data(),
                    affine_of(base.pre_concat(path.abs_transform())),
                    project,
                    size,
                )
                .into_iter()
                .map(|(mut ring, _)| {
                    if ring.first() != ring.last()
                        && let Some(first) = ring.first().copied()
                    {
                        ring.push(first);
                    }
                    ring
                })
                .filter(|ring| ring.len() >= 4)
                .collect();
                if !rings.is_empty() {
                    let even_odd = path
                        .fill()
                        .is_some_and(|fill| fill.rule() == usvg::FillRule::EvenOdd);
                    shapes.push(ClipShape::new(rings, even_odd));
                }
            }
            usvg::Node::Image(_) => return Err(clip_image().into()),
            usvg::Node::Text(_) => return Err(clip_text().into()),
            _ => {}
        }
    }
    Ok(())
}

fn visit(
    group: &usvg::Group,
    project: &Project,
    size: usvg::Size,
    clips: &[Clip],
    transform: Affine,
    view_box: Option<Affine>,
    result: &mut Vec<Contour>,
) -> Result<(), String> {
    reject_effects(group)?;
    let stacked: Vec<Clip>;
    let clips = match group.clip_path() {
        Some(clip) => {
            let mut all = clips.to_vec();
            all.push(clip_region(clip, group.abs_transform(), project, size)?);
            stacked = all;
            stacked.as_slice()
        }
        None => clips,
    };
    if group.opacity().get() == 0.0 {
        return Ok(());
    }
    for node in group.children() {
        match node {
            usvg::Node::Group(group) => {
                let mut own = affine_of(group.transform());
                if let Some(exact) = view_box
                    && affine_near(own, exact)
                {
                    own = exact;
                }
                visit(
                    group,
                    project,
                    size,
                    clips,
                    affine_mul(transform, own),
                    view_box,
                    result,
                )?
            }
            usvg::Node::Path(path) if path.is_visible() => {
                if path.fill().is_none_or(|fill| fill.opacity().get() == 0.0)
                    && path
                        .stroke()
                        .is_none_or(|stroke| stroke.opacity().get() == 0.0)
                {
                    continue;
                }
                let subpaths = flatten_path(path.data(), transform, project, size);
                // Like Java's SVGShape/DashedShape: the dash pattern is applied in the path's
                // user space, so it scales with the element's transform and the viewBox.
                let dash = path.stroke().and_then(|stroke| {
                    let pattern = dash_pattern(stroke.dasharray()?)?;
                    let metric = user_length_metric(path.abs_transform(), project, size)?;
                    Some((pattern, f64::from(stroke.dashoffset()), metric))
                });
                let start = result.len();
                for (contour, closed) in subpaths {
                    if contour.len() < 2 {
                        continue;
                    }
                    match &dash {
                        Some((pattern, offset, metric)) => {
                            dash_contour(&contour, closed, pattern, *offset, metric, result)?
                        }
                        None => result.push(contour),
                    }
                }
                if !clips.is_empty() {
                    for piece in result.split_off(start) {
                        result.extend(clip_contour(&piece, clips)?);
                    }
                }
            }
            usvg::Node::Image(_) => {
                return Err(
                    tr("Schneiden unterstützt keine Rasterbilder; bitte Gravieren wählen").into(),
                );
            }
            // `contours` re-imports text as paths before visiting.
            usvg::Node::Text(_) => {
                return Err(tr("Text konnte nicht in Pfade umgewandelt werden").into());
            }
            _ => {}
        }
    }
    Ok(())
}

/// Flattens the subpaths of a path into mm contours, each with its closed flag.
/// `transform` maps the path's coordinates to pixels of the whole document.
fn flatten_path(
    data: &tiny_skia::Path,
    transform: Affine,
    project: &Project,
    size: usvg::Size,
) -> Vec<(Contour, bool)> {
    // Java: translate(x, y) · scale(document → mm), applied in double.
    let (sx, sy) = (
        f64::from(project.width_mm) / f64::from(size.width()),
        f64::from(project.height_mm) / f64::from(size.height()),
    );
    let map = |point: tiny_skia::Point| {
        let [x, y] = affine_apply(transform, f64::from(point.x), f64::from(point.y));
        [
            sx * x + f64::from(project.x_mm),
            sy * y + f64::from(project.y_mm),
        ]
    };
    let mut subpaths = Vec::new();
    let mut contour = Vec::new();
    let mut closed = false;
    let mut current = [0.0; 2];
    let mut start = [0.0; 2];
    let mut start_point = tiny_skia::Point::zero();
    let mut after_close = false;
    for segment in data.segments() {
        if !matches!(segment, tiny_skia::PathSegment::MoveTo(_)) {
            closed = false;
        }
        match segment {
            tiny_skia::PathSegment::MoveTo(point) => {
                // tiny-skia inserts a MoveTo to the subpath start when drawing
                // goes on after Close; Java2D (and so VisiCut) keeps cutting
                // from that point without lifting the laser.
                if after_close && point == start_point {
                    after_close = false;
                    current = start;
                    continue;
                }
                after_close = false;
                subpaths.push((std::mem::take(&mut contour), closed));
                closed = false;
                current = map(point);
                start = current;
                start_point = point;
                contour.push(current);
            }
            tiny_skia::PathSegment::LineTo(point) => {
                after_close = false;
                current = map(point);
                contour.push(current);
            }
            tiny_skia::PathSegment::QuadTo(a, end) => {
                after_close = false;
                let end = map(end);
                flatten_curve(&[current, map(a), end], &mut contour);
                current = end;
            }
            tiny_skia::PathSegment::CubicTo(a, b, end) => {
                after_close = false;
                let end = map(end);
                flatten_curve(&[current, map(a), map(b), end], &mut contour);
                current = end;
            }
            tiny_skia::PathSegment::Close => {
                contour.push(start);
                current = start;
                closed = true;
                after_close = true;
            }
        }
    }
    subpaths.push((contour, closed));
    subpaths
}

/// Upper bound for contours a dash pattern may add; every contour has at least two
/// points, so this matches the global limit of one million points.
const MAX_DASH_CONTOURS: usize = 500_000;

/// Normalizes `stroke-dasharray` per SVG: odd lists repeat, negative or all-zero lists
/// mean a solid stroke.
fn dash_pattern(dasharray: &[f32]) -> Option<Vec<f64>> {
    if dasharray.is_empty() || dasharray.iter().any(|v| !v.is_finite() || *v < 0.0) {
        return None;
    }
    let mut pattern: Vec<f64> = dasharray.iter().map(|v| f64::from(*v)).collect();
    if pattern.iter().sum::<f64>() <= 0.0 {
        return None;
    }
    if pattern.len() % 2 == 1 {
        pattern.extend_from_within(..);
    }
    Some(pattern)
}

/// Inverse of the linear part of the user space → mm mapping, used to measure mm
/// segments in the path's user units. `None` for a degenerate transform.
fn user_length_metric(
    transform: tiny_skia::Transform,
    project: &Project,
    size: usvg::Size,
) -> Option<[f64; 4]> {
    let scale_x = f64::from(project.width_mm) / f64::from(size.width());
    let scale_y = f64::from(project.height_mm) / f64::from(size.height());
    let a = scale_x * f64::from(transform.sx);
    let b = scale_x * f64::from(transform.kx);
    let c = scale_y * f64::from(transform.ky);
    let d = scale_y * f64::from(transform.sy);
    let det = a * d - b * c;
    if !det.is_finite() || det.abs() < 1e-12 {
        return None;
    }
    Some([d / det, -b / det, -c / det, a / det])
}

fn user_distance(metric: &[f64; 4], a: Point, b: Point) -> f64 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    (metric[0] * dx + metric[1] * dy).hypot(metric[2] * dx + metric[3] * dy)
}

/// Splits a flattened subpath (in mm) into its dashes. Lengths are measured in user
/// units via `metric`; the pattern restarts at every subpath, and on closed subpaths
/// the last dash continues into the first one across the start point.
fn dash_contour(
    contour: &[Point],
    closed: bool,
    pattern: &[f64],
    offset: f64,
    metric: &[f64; 4],
    result: &mut Vec<Contour>,
) -> Result<(), String> {
    let period: f64 = pattern.iter().sum();
    let lengths: Vec<f64> = contour
        .windows(2)
        .map(|w| user_distance(metric, w[0], w[1]))
        .collect();
    let total: f64 = lengths.iter().sum();
    if !total.is_finite() || total <= 0.0 {
        return Ok(());
    }
    let estimate = (total / period).ceil() * (pattern.len() / 2) as f64 + 1.0;
    if !estimate.is_finite() || result.len() as f64 + estimate > MAX_DASH_CONTOURS as f64 {
        return Err(tr("Strichmuster erzeugt zu viele Vektorpunkte").into());
    }

    let mut index = 0;
    let mut phase = offset.rem_euclid(period);
    for _ in 0..2 * pattern.len() {
        if phase < pattern[index] {
            break;
        }
        phase -= pattern[index];
        index = (index + 1) % pattern.len();
    }
    let mut remaining = (pattern[index] - phase).max(0.0);
    let on_at_start = index % 2 == 0;
    let epsilon = period * 1e-6;

    let mut dashes: Vec<Contour> = Vec::new();
    let mut dash: Contour = Vec::new();
    if on_at_start {
        dash.push(contour[0]);
    }
    for (segment, &length) in contour.windows(2).zip(&lengths) {
        let (a, b) = (segment[0], segment[1]);
        if length <= 0.0 {
            continue;
        }
        let mut t = 0.0;
        while length - t > remaining + epsilon {
            t += remaining;
            let point = mix(a, b, t / length);
            if dash.last() != Some(&point) {
                dash.push(point);
            }
            if index % 2 == 0 {
                dashes.push(std::mem::take(&mut dash));
            }
            index = (index + 1) % pattern.len();
            remaining = pattern[index];
        }
        remaining = (remaining - (length - t)).max(0.0);
        if index % 2 == 0 && dash.last() != Some(&b) {
            dash.push(b);
        }
    }
    if index % 2 == 0 {
        if closed && on_at_start && !dashes.is_empty() {
            let first = dashes.remove(0);
            dash.extend_from_slice(&first[1..]);
        }
        dashes.push(dash);
    }
    result.extend(dashes.into_iter().filter(|dash| dash.len() > 1));
    Ok(())
}

fn mix(a: Point, b: Point, t: f64) -> Point {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

/// Flattens a quadratic (3 points) or cubic (4 points) Bézier curve given in
/// mm like Java's `FlatteningPathIterator` on the 500-DPI laser shape: in
/// pixels, flatness 0.2 px, at most 10 subdivision levels. Appends the end
/// points of the line segments, the curve end last.
fn flatten_curve(points: &[Point], out: &mut Contour) {
    let mut coords: Vec<f64> = points
        .iter()
        .flat_map(|p| [p[0] * PX_PER_MM, p[1] * PX_PER_MM])
        .collect();
    let mut px = Vec::new();
    subdivide_flat(&mut coords, 0, &mut px);
    out.extend(px.into_iter().map(|[x, y]| [x / PX_PER_MM, y / PX_PER_MM]));
}

fn subdivide_flat(curve: &mut [f64], level: u32, out: &mut Vec<[f64; 2]>) {
    let flatness_sq = if curve.len() == 6 {
        // QuadCurve2D.getFlatnessSq
        pt_seg_dist_sq(curve[0], curve[1], curve[4], curve[5], curve[2], curve[3])
    } else {
        // CubicCurve2D.getFlatnessSq
        pt_seg_dist_sq(curve[0], curve[1], curve[6], curve[7], curve[2], curve[3]).max(
            pt_seg_dist_sq(curve[0], curve[1], curve[6], curve[7], curve[4], curve[5]),
        )
    };
    if level >= CURVE_LIMIT || flatness_sq < CURVE_FLATNESS_PX * CURVE_FLATNESS_PX {
        let n = curve.len();
        out.push([curve[n - 2], curve[n - 1]]);
        return;
    }
    let (mut left, mut right) = if curve.len() == 6 {
        subdivide_quad(curve)
    } else {
        subdivide_cubic(curve)
    };
    subdivide_flat(&mut left, level + 1, out);
    subdivide_flat(&mut right, level + 1, out);
}

/// `Line2D.ptSegDistSq`.
fn pt_seg_dist_sq(x1: f64, y1: f64, x2: f64, y2: f64, px: f64, py: f64) -> f64 {
    let (x2, y2) = (x2 - x1, y2 - y1);
    let (mut px, mut py) = (px - x1, py - y1);
    let mut dot = px * x2 + py * y2;
    let projected_sq = if dot <= 0.0 {
        0.0
    } else {
        px = x2 - px;
        py = y2 - py;
        dot = px * x2 + py * y2;
        if dot <= 0.0 {
            0.0
        } else {
            dot * dot / (x2 * x2 + y2 * y2)
        }
    };
    (px * px + py * py - projected_sq).max(0.0)
}

/// `QuadCurve2D.subdivide`.
fn subdivide_quad(c: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let (x1, y1, cx, cy, x2, y2) = (c[0], c[1], c[2], c[3], c[4], c[5]);
    let (lx, ly) = ((x1 + cx) / 2.0, (y1 + cy) / 2.0);
    let (rx, ry) = ((x2 + cx) / 2.0, (y2 + cy) / 2.0);
    let (mx, my) = ((lx + rx) / 2.0, (ly + ry) / 2.0);
    (vec![x1, y1, lx, ly, mx, my], vec![mx, my, rx, ry, x2, y2])
}

/// `CubicCurve2D.subdivide`.
fn subdivide_cubic(c: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let (x1, y1, c1x, c1y, c2x, c2y, x2, y2) = (c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]);
    let (ax, ay) = ((x1 + c1x) / 2.0, (y1 + c1y) / 2.0);
    let (bx, by) = ((x2 + c2x) / 2.0, (y2 + c2y) / 2.0);
    let (centre_x, centre_y) = ((c1x + c2x) / 2.0, (c1y + c2y) / 2.0);
    let (d1x, d1y) = ((ax + centre_x) / 2.0, (ay + centre_y) / 2.0);
    let (d2x, d2y) = ((bx + centre_x) / 2.0, (by + centre_y) / 2.0);
    let (mx, my) = ((d1x + d2x) / 2.0, (d1y + d2y) / 2.0);
    (
        vec![x1, y1, ax, ay, d1x, d1y, mx, my],
        vec![mx, my, d2x, d2y, bx, by, x2, y2],
    )
}

#[cfg(test)]
mod dash_tests {
    use super::*;

    /// Default project: 100×60 user units map to 100×60 mm at offset (10, 10).
    fn cut(body: &str) -> Result<Vec<Contour>, String> {
        cut_with_viewbox("", body)
    }

    fn cut_with_viewbox(view_box: &str, body: &str) -> Result<Vec<Contour>, String> {
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="60" {view_box}>{body}</svg>"#
        );
        contours(&Project {
            svg,
            ..Default::default()
        })
    }

    fn length(contour: &Contour) -> f64 {
        contour
            .windows(2)
            .map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]))
            .sum()
    }

    fn assert_dashes(paths: &[Contour], expected: &[(f64, f64)]) {
        assert_eq!(paths.len(), expected.len(), "{paths:?}");
        for (path, &(start, len)) in paths.iter().zip(expected) {
            assert!((path[0][0] - 10.0 - start).abs() < 1e-3, "{paths:?}");
            assert!((length(path) - len).abs() < 1e-3, "{paths:?}");
        }
    }

    fn line(attributes: &str) -> String {
        format!(r#"<path d="M0 0H30" fill="none" stroke="black" {attributes}/>"#)
    }

    #[test]
    fn splits_a_line_into_dashes() {
        let paths = cut(&line(r#"stroke-dasharray="5 5""#)).unwrap();
        assert_dashes(&paths, &[(0.0, 5.0), (10.0, 5.0), (20.0, 5.0)]);
        assert!(paths.iter().flatten().all(|p| (p[1] - 10.0).abs() < 1e-6));
    }

    #[test]
    fn keeps_dashes_and_transforms_when_text_is_present() {
        let body = format!(
            r#"<g transform="translate(0 20)">{}</g><text x="50" y="50" font-size="10">H</text>"#,
            line(r#"stroke-dasharray="5 5""#)
        );
        let paths = cut(&body).unwrap();
        let dashes: Vec<_> = paths.iter().filter(|p| p[0][1] < 40.0).cloned().collect();
        assert_dashes(&dashes, &[(0.0, 5.0), (10.0, 5.0), (20.0, 5.0)]);
        assert!(dashes.iter().flatten().all(|p| (p[1] - 30.0).abs() < 1e-3));
        assert!(paths.len() > dashes.len());
    }

    #[test]
    fn honours_positive_and_negative_dashoffset() {
        let paths = cut(&line(r#"stroke-dasharray="5 5" stroke-dashoffset="2""#)).unwrap();
        assert_dashes(&paths, &[(0.0, 3.0), (8.0, 5.0), (18.0, 5.0), (28.0, 2.0)]);
        let paths = cut(&line(r#"stroke-dasharray="5 5" stroke-dashoffset="-2""#)).unwrap();
        assert_dashes(&paths, &[(2.0, 5.0), (12.0, 5.0), (22.0, 5.0)]);
        // Offsets larger than the pattern wrap around.
        let paths = cut(&line(r#"stroke-dasharray="5 5" stroke-dashoffset="22""#)).unwrap();
        assert_dashes(&paths, &[(0.0, 3.0), (8.0, 5.0), (18.0, 5.0), (28.0, 2.0)]);
    }

    #[test]
    fn repeats_odd_dasharrays() {
        let paths = cut(&line(r#"stroke-dasharray="5""#)).unwrap();
        assert_dashes(&paths, &[(0.0, 5.0), (10.0, 5.0), (20.0, 5.0)]);
        // "4 2 1" becomes "4 2 1 4 2 1": on 4, off 2, on 1, off 4, on 2, off 1.
        let paths = cut(&line(r#"stroke-dasharray="4,2,1""#)).unwrap();
        assert_dashes(
            &paths,
            &[
                (0.0, 4.0),
                (6.0, 1.0),
                (11.0, 2.0),
                (14.0, 4.0),
                (20.0, 1.0),
                (25.0, 2.0),
                (28.0, 2.0),
            ],
        );
    }

    #[test]
    fn dashes_run_continuously_around_closed_shapes() {
        let paths = cut(r#"<rect width="10" height="10" fill="none" stroke="black" stroke-dasharray="6 4" stroke-dashoffset="3"/>"#).unwrap();
        // Perimeter 40: [0,3] and [37,40] join into one dash across the start corner.
        assert_eq!(paths.len(), 4, "{paths:?}");
        assert!(
            paths.iter().all(|p| (length(p) - 6.0).abs() < 1e-3),
            "{paths:?}"
        );
        let across_start = paths
            .iter()
            .find(|p| p[1..p.len() - 1].contains(&[10.0, 10.0]))
            .expect("one dash passes through the start point");
        assert_eq!(across_start.first(), Some(&[10.0, 13.0]));
        assert_eq!(across_start.last(), Some(&[13.0, 10.0]));

        // A dash longer than the perimeter keeps the whole closed outline.
        let paths = cut(
            r#"<rect width="10" height="10" fill="none" stroke="black" stroke-dasharray="50 5"/>"#,
        )
        .unwrap();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].first(), paths[0].last());
        assert!((length(&paths[0]) - 40.0).abs() < 1e-3);
    }

    #[test]
    fn scales_dash_lengths_with_viewbox_and_transforms() {
        // viewBox halves the user space: 5 user units become 10 mm.
        let paths = cut_with_viewbox(
            r#"viewBox="0 0 50 30""#,
            r#"<path d="M0 0H20" fill="none" stroke="black" stroke-dasharray="5 5"/>"#,
        )
        .unwrap();
        assert_dashes(&paths, &[(0.0, 10.0), (20.0, 10.0)]);
        let paths = cut(&format!(
            r#"<g transform="scale(2)">{}</g>"#,
            line(r#"stroke-dasharray="5 5""#)
        ))
        .unwrap();
        assert_dashes(&paths, &[(0.0, 10.0), (20.0, 10.0), (40.0, 10.0)]);
    }

    #[test]
    fn stays_solid_without_a_usable_dasharray() {
        for attributes in [
            "",
            r#"stroke-dasharray="none""#,
            r#"stroke-dasharray="0 0""#,
            r#"stroke-dasharray="0""#,
            r#"stroke-dasharray="5 -1""#,
        ] {
            let paths = cut(&line(attributes)).unwrap();
            assert_dashes(&paths, &[(0.0, 30.0)]);
        }
        // dasharray only affects strokes; a fill-only shape is cut along its full outline.
        let paths =
            cut(r#"<rect width="10" height="10" fill="black" stroke-dasharray="2 2"/>"#).unwrap();
        assert_eq!(paths.len(), 1);
        assert!((length(&paths[0]) - 40.0).abs() < 1e-3);
    }

    #[test]
    fn drops_zero_length_dashes_and_rejects_explosive_patterns() {
        let paths = cut(&line(r#"stroke-dasharray="0 10""#)).unwrap_or_default();
        assert!(paths.is_empty());
        assert!(cut(&line(r#"stroke-dasharray="0.00001""#)).is_err());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transforms_curves_and_closes_shapes() {
        let project = Project {
            svg: include_str!("../examples/demo.svg").into(),
            ..Default::default()
        };
        let paths = contours(&project).unwrap();
        assert_eq!(paths.len(), 3);
        assert!(paths[1].len() > 30);
        assert_eq!(paths[1].first(), paths[1].last());
        assert!(paths.iter().flatten().all(|p| p[0] >= 10.0 && p[1] >= 10.0));
    }

    #[test]
    fn applies_viewbox_and_parent_transform() {
        let project = Project { svg: r#"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="60mm" viewBox="0 0 100 60"><g transform="translate(20 5)"><path d="M0 0L10 10" fill="none" stroke="black"/></g></svg>"#.into(), ..Default::default() };
        let paths = contours(&project).unwrap();
        assert!((paths[0][0][0] - 30.0).abs() < 0.001);
        assert!((paths[0][0][1] - 15.0).abs() < 0.001);
        assert!((paths[0][1][0] - 40.0).abs() < 0.001);
    }

    #[test]
    fn cuts_text_along_glyph_outlines_in_mm() {
        let project = Project { svg: r#"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="60mm" viewBox="0 0 100 60"><g transform="translate(20 30) scale(2)"><text x="0" y="0" font-family="sans-serif" font-size="10" fill="none" stroke="red">H</text></g><rect x="90" y="50" width="5" height="5"/></svg>"#.into(), x_mm: 0.0, y_mm: 0.0, width_mm: 100.0, height_mm: 60.0, ..Default::default() };
        let paths = contours(&project).unwrap();
        let glyph: Vec<Point> = paths
            .iter()
            .flatten()
            .copied()
            .filter(|p| p[0] < 80.0)
            .collect();
        assert!(!glyph.is_empty());
        let (min_x, max_x) = glyph
            .iter()
            .fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p[0]), b.max(p[0])));
        let (min_y, max_y) = glyph
            .iter()
            .fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p[1]), b.max(p[1])));
        // A 20 mm "H" sits on the baseline at y = 30 mm, starting at x = 20 mm.
        assert!(min_x >= 20.0 && max_x < 40.0, "x {min_x}..{max_x}");
        assert!(max_y <= 30.5 && max_y > 29.0, "y max {max_y}");
        assert!(min_y > 10.0 && min_y < 25.0, "y min {min_y}");
        assert!(paths.iter().any(|p| p.iter().all(|q| q[0] >= 90.0)));
    }

    #[test]
    fn keeps_cutting_after_close_and_zero_width_strokes() {
        let project = Project { svg: r##"<svg xmlns="http://www.w3.org/2000/svg" width="70mm" height="50mm" viewBox="0 0 70 50"><path d="M 5 5 L 25 5 L 25 20 Z L 40 40" fill="none" stroke="#f00"/><rect x="50" y="5" width="10" height="10" fill="none" stroke="#f00" stroke-width="0"/><rect x="50" y="30" width="10" height="10" style="fill:none;stroke:#f00;stroke-width:0.0"/></svg>"##.into(), x_mm: 0.0, y_mm: 0.0, width_mm: 70.0, height_mm: 50.0, ..Default::default() };
        let paths = contours(&project).unwrap();
        // Java2D continues the contour after Z without lifting: one path of five points.
        assert_eq!(paths.len(), 3, "{paths:?}");
        assert_eq!(paths[0].len(), 5);
        let near = |p: Point, q: [f64; 2]| (p[0] - q[0]).abs() < 1e-9 && (p[1] - q[1]).abs() < 1e-9;
        assert!(near(paths[0][3], [5.0, 5.0]), "{:?}", paths[0]);
        assert!(near(paths[0][4], [40.0, 40.0]), "{:?}", paths[0]);
        // Hairline strokes of width 0 are cut like in VisiCut.
        assert!(near(paths[1][0], [50.0, 5.0]), "{:?}", paths[1]);
        assert!(near(paths[2][0], [50.0, 30.0]), "{:?}", paths[2]);
        assert_eq!(
            keep_zero_width_strokes("a stroke-width:0.5 b stroke-width=\"0px\" c"),
            "a stroke-width:0.5 b stroke-width=\"0.001px\" c"
        );
    }

    #[test]
    fn maps_user_units_to_mm_exactly() {
        // 1 user unit = 1 mm: the coordinates must come out exact in f64, not
        // with the f32 rounding of usvg's viewBox transform.
        let project = Project { svg: r#"<svg xmlns="http://www.w3.org/2000/svg" width="70mm" height="50mm" viewBox="0 0 70 50"><g transform="translate(3 4)"><rect x="10" y="20" width="30" height="10"/></g></svg>"#.into(), x_mm: 100.0, y_mm: 200.0, width_mm: 70.0, height_mm: 50.0, ..Default::default() };
        let paths = contours(&project).unwrap();
        assert_eq!(paths[0][0], [113.0, 224.0]);
        assert_eq!(paths[0][2], [143.0, 234.0]);
    }

    #[test]
    fn reports_text_without_font() {
        let project = Project { svg: r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="60"><text x="10" y="20">Job</text><rect width="10" height="10"/></svg>"#.into(), ..Default::default() };
        let error =
            contours_with_fonts(&project, Arc::new(usvg::fontdb::Database::new())).unwrap_err();
        assert!(error.contains("Schriftart"), "{error}");
    }

    #[test]
    fn reports_characters_missing_from_every_font() {
        let mut fonts = usvg::fontdb::Database::new();
        fonts.load_font_data(epaint_default_fonts::UBUNTU_LIGHT.to_vec());
        let project = Project { svg: "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100\" height=\"60\"><text x=\"10\" y=\"20\" font-family=\"Ubuntu\">A \u{10FFFD}</text></svg>".into(), ..Default::default() };
        let error = contours_with_fonts(&project, Arc::new(fonts)).unwrap_err();
        assert!(error.contains("U+10FFFD"), "{error}");
    }

    #[test]
    fn bundled_fonts_cover_generic_families() {
        let fonts = crate::svg::fonts();
        for family in [
            usvg::fontdb::Family::Serif,
            usvg::fontdb::Family::SansSerif,
            usvg::fontdb::Family::Monospace,
            usvg::fontdb::Family::Cursive,
            usvg::fontdb::Family::Fantasy,
        ] {
            let query = usvg::fontdb::Query {
                families: &[family],
                ..Default::default()
            };
            assert!(fonts.query(&query).is_some(), "{family:?}");
        }
    }
}

#[cfg(test)]
mod clip_tests {
    use super::*;

    /// Default project: 100×60 user units at offset (10, 10) mm.
    fn cut(body: &str) -> Result<Vec<Contour>, String> {
        contours(&Project {
            svg: format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="60">{body}</svg>"#
            ),
            ..Default::default()
        })
    }

    fn length(contour: &Contour) -> f64 {
        contour
            .windows(2)
            .map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]))
            .sum()
    }

    /// Clip square covering user units 0..20, i.e. mm 10..30.
    const CLIP: &str = r#"<defs><clipPath id="c"><rect width="20" height="20"/></clipPath></defs>"#;

    #[test]
    fn keeps_the_part_of_a_square_inside_the_clip() {
        // The square spans mm 20..60; the clip covers mm 10..30 in both axes.
        let paths = cut(&format!(
            r#"{CLIP}<rect x="10" y="10" width="40" height="40" fill="none" stroke="black" clip-path="url(#c)"/>"#
        ))
        .unwrap();
        // Top and left edge inside the clip, 10 mm each.
        assert_eq!(paths.len(), 1, "{paths:?}");
        assert!((length(&paths[0]) - 20.0).abs() < 1e-3, "{paths:?}");
        assert!(
            paths
                .iter()
                .flatten()
                .all(|p| (19.99..=30.01).contains(&p[0]) && (19.99..=30.01).contains(&p[1]))
        );
    }

    #[test]
    fn keeps_closed_shapes_that_lie_inside_unchanged() {
        // User 2..7 is mm 12..17: entirely inside, so the closed outline stays whole.
        let paths = cut(&format!(
            r#"{CLIP}<rect x="2" y="2" width="5" height="5" fill="none" stroke="black" clip-path="url(#c)"/>"#
        ))
        .unwrap();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].first(), paths[0].last());
        assert!((length(&paths[0]) - 20.0).abs() < 1e-3);
    }

    #[test]
    fn path_entirely_outside_the_clip_gives_nothing() {
        // User 70..90 is mm 80..100, outside the clip.
        let error = cut(&format!(
            r#"{CLIP}<path d="M70 5H90" stroke="black" clip-path="url(#c)"/>"#
        ))
        .unwrap_err();
        assert!(error.contains("Keine schneidbaren"), "{error}");

        // Inside the group, the outside path is dropped and the inside one kept.
        let paths = cut(&format!(
            r#"{CLIP}<g clip-path="url(#c)"><path d="M70 5H90" stroke="black"/><path d="M5 5H15" stroke="black"/></g>"#
        ))
        .unwrap();
        assert_eq!(paths.len(), 1);
        assert!((length(&paths[0]) - 10.0).abs() < 1e-3);
    }

    #[test]
    fn empty_clip_path_is_an_error() {
        let error = cut(
            r#"<defs><clipPath id="c"/></defs><rect width="10" height="10" clip-path="url(#c)"/>"#,
        )
        .unwrap_err();
        assert!(error.contains("leer"), "{error}");
    }

    #[test]
    fn missing_clip_path_is_an_error() {
        let error = cut(r#"<rect width="10" height="10" clip-path="url(#missing)"/>"#).unwrap_err();
        assert!(error.contains("nicht vorhanden"), "{error}");
    }

    #[test]
    fn masks_and_filters_are_rejected_with_reasons() {
        let mask = cut(
            r#"<defs><mask id="m"><rect width="50" height="50" fill="white"/></mask></defs><rect width="10" height="10" mask="url(#m)"/>"#,
        )
        .unwrap_err();
        assert!(
            mask.contains("Masken") && mask.contains("Transparenz"),
            "{mask}"
        );

        let filter = cut(
            r#"<defs><filter id="f"><feGaussianBlur stdDeviation="2"/></filter></defs><rect width="10" height="10" filter="url(#f)"/>"#,
        )
        .unwrap_err();
        assert!(
            filter.contains("Filter") && filter.contains("Pixelbild"),
            "{filter}"
        );
    }

    #[test]
    fn object_outline_is_relative_to_the_motif_and_covers_images() {
        let project = Project {
            x_mm: 100.0,
            y_mm: 50.0,
            width_mm: 100.0,
            height_mm: 50.0,
            svg: r#"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="50mm" viewBox="0 0 100 50"><rect x="10" y="5" width="20" height="10" fill="none" stroke="black"/><image x="50" y="20" width="30" height="20" preserveAspectRatio="none" href="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg=="/></svg>"#.into(),
            ..Project::default()
        };
        let rect = object_outline(&project, 0).unwrap();
        let xs: Vec<f64> = rect.iter().flatten().map(|p| p[0]).collect();
        assert!(
            (xs.iter().cloned().fold(f64::MAX, f64::min) - 10.0).abs() < 0.01,
            "{xs:?}"
        );
        assert!((xs.iter().cloned().fold(f64::MIN, f64::max) - 30.0).abs() < 0.01);
        // The image has no contours: its rectangle instead (pixel accurate).
        let image = object_outline(&project, 1).unwrap();
        let expected = [
            [50.0, 20.0],
            [80.0, 20.0],
            [80.0, 40.0],
            [50.0, 40.0],
            [50.0, 20.0],
        ];
        assert_eq!(image.len(), 1);
        for (p, e) in image[0].iter().zip(expected) {
            assert!(
                (p[0] - e[0]).abs() < 0.5 && (p[1] - e[1]).abs() < 0.5,
                "{image:?}"
            );
        }
    }

    fn outline_of(body: &str) -> Result<Vec<Contour>, String> {
        let project = Project {
            x_mm: 0.0,
            y_mm: 0.0,
            width_mm: 100.0,
            height_mm: 100.0,
            svg: format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="100mm" viewBox="0 0 100 100">{body}</svg>"#
            ),
            ..Project::default()
        };
        outline_contours(&project)
    }

    fn area(ring: &Contour) -> f64 {
        ring.windows(2)
            .map(|s| s[0][0] * s[1][1] - s[1][0] * s[0][1])
            .sum::<f64>()
            .abs()
            / 2.0
    }

    #[test]
    fn outline_rings_start_and_turn_like_java_area() {
        // Square with a square hole: the outer ring from its top left corner
        // downwards (counter-clockwise on screen), the hole from its top
        // right corner downwards (clockwise).
        let rings = outline_of(
            r#"<path d="M 10 10 H 90 V 90 H 10 Z M 70 30 H 30 V 70 H 70 Z" fill="black"/>"#,
        )
        .unwrap();
        assert_eq!(rings.len(), 2, "{rings:?}");
        let start = |ring: &Contour| -> Vec<[f64; 2]> {
            ring[..2]
                .iter()
                .map(|p| [(p[0] * 1e3).round() / 1e3, (p[1] * 1e3).round() / 1e3])
                .collect()
        };
        assert_eq!(start(&rings[0]), [[10.0, 10.0], [10.0, 90.0]]);
        assert_eq!(start(&rings[1]), [[70.0, 30.0], [70.0, 70.0]]);
        // A triangle starts at its apex, then down the left side.
        let triangle = outline_of(r#"<path d="M 50 10 L 90 90 L 10 90 Z" fill="black"/>"#).unwrap();
        assert_eq!(start(&triangle[0]), [[50.0, 10.0], [10.0, 90.0]]);
    }

    #[test]
    fn outline_keeps_the_even_odd_hole_of_an_unfilled_shape() {
        // Only stroked, so usvg has no fill and no fill rule; the hole stays.
        for style in [
            r#"fill="none" fill-rule="evenodd""#,
            r#"style="fill:none;fill-rule:evenodd""#,
        ] {
            let rings = outline_of(&format!(
                r#"<g fill-rule="nonzero"><path d="M 10 10 H 90 V 90 H 10 Z M 30 30 H 70 V 70 H 30 Z" {style} stroke="red"/></g>"#
            ))
            .unwrap();
            assert_eq!(rings.len(), 2, "{style}: {rings:?}");
        }
        // Without evenodd the inner square (same direction) is filled.
        let rings = outline_of(
            r#"<path d="M 10 10 H 90 V 90 H 10 Z M 30 30 H 70 V 70 H 30 Z" fill="none" stroke="red"/>"#,
        )
        .unwrap();
        assert_eq!(rings.len(), 1, "{rings:?}");
    }

    #[test]
    fn outline_unites_overlapping_shapes_into_one_ring() {
        // Two 20 × 20 squares overlapping by 10 mm, both only stroked: one
        // L-free ring of 30 × 20 mm, the shared lines inside are gone.
        let rings = outline_of(
            r#"<rect x="10" y="10" width="20" height="20" fill="none" stroke="red"/><rect x="20" y="10" width="20" height="20" fill="none" stroke="red"/>"#,
        )
        .unwrap();
        assert_eq!(rings.len(), 1, "{rings:?}");
        // The overlay works on a fine integer grid: well below 0.001 mm.
        assert!(
            (area(&rings[0]) - 600.0).abs() < 1e-3,
            "{} {rings:?}",
            area(&rings[0])
        );
        assert_eq!(rings[0].first(), rings[0].last());
        // A circle inside a frame disappears; a separate one stays.
        let rings = outline_of(
            r#"<rect x="10" y="10" width="40" height="40" fill="none" stroke="red"/><circle cx="30" cy="30" r="5" fill="none" stroke="red"/><circle cx="80" cy="80" r="5" fill="none" stroke="red"/>"#,
        )
        .unwrap();
        assert_eq!(rings.len(), 2, "{rings:?}");
    }

    #[test]
    fn outline_keeps_holes_lines_add_nothing_and_images_count_as_rectangles() {
        // A frame with an even-odd hole keeps the hole as second ring.
        let rings = outline_of(
            r#"<path d="M 10 10 H 50 V 50 H 10 Z M 20 20 H 40 V 40 H 20 Z" fill-rule="evenodd" stroke="red"/>"#,
        )
        .unwrap();
        assert_eq!(rings.len(), 2, "{rings:?}");
        let mut areas: Vec<f64> = rings.iter().map(area).collect();
        areas.sort_by(f64::total_cmp);
        assert!((areas[0] - 400.0).abs() < 1e-3 && (areas[1] - 1600.0).abs() < 1e-3);
        assert!(outline_of(r#"<path d="M 10 10 L 90 90" stroke="red"/>"#).is_err());
        let image = r#"<image x="60" y="10" width="20" height="10" preserveAspectRatio="none" href="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg=="/>"#;
        let rings = outline_of(image).unwrap();
        assert_eq!(rings.len(), 1);
        assert!((area(&rings[0]) - 200.0).abs() < 1e-3, "{rings:?}");
    }

    #[test]
    fn object_at_prefers_lines_then_the_smallest_enclosing_shape() {
        let square = |x: f64, y: f64, s: f64| {
            vec![vec![[x, y], [x + s, y], [x + s, y + s], [x, y + s], [x, y]]]
        };
        // A frame, a small square inside it and an open line.
        let outlines = vec![
            square(0.0, 0.0, 100.0),
            square(20.0, 20.0, 10.0),
            vec![vec![[60.0, 50.0], [90.0, 50.0]]],
        ];
        assert_eq!(
            object_at(&outlines, [25.0, 25.0], 1.0),
            Some(1),
            "inside the small square"
        );
        assert_eq!(
            object_at(&outlines, [50.0, 80.0], 1.0),
            Some(0),
            "inside the frame only"
        );
        assert_eq!(
            object_at(&outlines, [75.0, 50.5], 1.0),
            Some(2),
            "near the line"
        );
        assert_eq!(
            object_at(&outlines, [0.5, 50.0], 1.0),
            Some(0),
            "on the frame edge"
        );
        assert_eq!(object_at(&outlines, [150.0, 50.0], 1.0), None);
    }
}
