//! Writes a project as VisiCut project file (`.plf`) that Java VisiCut opens,
//! the counterpart of `read_plf` (`VisicutModel.savePlfToStream`).
//!
//! Every part becomes an SVG in millimetres (`<name>.svg`, part `i` under
//! `i/`), a `transform.xml` with its position on the bed and a
//! `mappings.xml` with its processing steps as VisiCut mappings. Laser
//! settings are not part of the format: Java VisiCut takes power and speed
//! from its own profiles `cut`, `mark`, `engrave` and `engrave 3d` for the
//! chosen material and thickness.
use super::xml;
use crate::mapping::{Attribute, Filter};
use crate::project::{CutOrder, Operation, Part, Project};
use crate::raster::{Dithering, RasterSettings};
use crate::{i18n::tr, tr};
use resvg::usvg;
use std::collections::HashSet;
use std::io::Write;
use std::path::Path;

/// Writes `project` to `path`; returns notes on what VisiCut does not keep.
pub fn write_plf(project: &Project, path: &Path) -> Result<Vec<String>, String> {
    let (bytes, warnings) = plf_bytes(project)?;
    std::fs::write(path, bytes).map_err(|e| e.to_string())?;
    Ok(warnings)
}

/// The PLF archive and notes on what VisiCut does not keep.
pub fn plf_bytes(project: &Project) -> Result<(Vec<u8>, Vec<String>), String> {
    let parts = project.all_parts();
    if parts.is_empty() {
        return Err(tr("Kein Motiv zum Speichern").into());
    }
    let mut warnings = vec![
        tr(
            "PLF-Dateien enthalten keine Laser-Einstellungen: Java-VisiCut nimmt Leistung, \
            Geschwindigkeit und Durchgänge aus seinen Profilen cut, mark, engrave und \
            engrave 3d für Material und Stärke.",
        )
        .to_string(),
    ];
    let order = match project.cut_order {
        CutOrder::File => "FILE",
        CutOrder::Nearest => "NEAREST",
        CutOrder::SmallestFirst => "SMALLEST_FIRST",
        CutOrder::VisiCut | CutOrder::ShortestTravel => "INNER_FIRST",
    };
    if project.cut_order == CutOrder::ShortestTravel {
        warnings.push(
            tr(
                "„Kürzeste Leerfahrten“ gibt es in VisiCut nicht; gespeichert als \
                „innen zuerst“ (Klassisch)",
            )
            .into(),
        );
    }
    if project.remove_duplicate_lines {
        warnings
            .push(tr("„Doppelte Linien entfernen“ wird in der PLF-Datei nicht gespeichert").into());
    }
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    for (index, part) in parts.iter().enumerate() {
        let prefix = if index > 0 {
            format!("{index}/")
        } else {
            String::new()
        };
        let (svg, mapping) = part_files(part, &project.name, order, &mut warnings)?;
        let file = file_name(&part.name, &project.name);
        for (name, bytes) in [
            (format!("{prefix}{file}"), svg.into_bytes()),
            (
                format!("{prefix}transform.xml"),
                transform_xml(part).into_bytes(),
            ),
            (format!("{prefix}mappings.xml"), mapping.into_bytes()),
        ] {
            zip.start_file(name, options).map_err(|e| e.to_string())?;
            zip.write_all(&bytes).map_err(|e| e.to_string())?;
        }
    }
    let bytes = zip.finish().map_err(|e| e.to_string())?.into_inner();
    Ok((bytes, warnings))
}

/// `<name>.svg`; VisiCut shows the file name as the part's name. Each part
/// has its own folder, so names may repeat.
fn file_name(part: &str, project: &str) -> String {
    let base: String = [part, project]
        .into_iter()
        .find(|n| !n.trim().is_empty())
        .unwrap_or("Motiv")
        .chars()
        .map(|c| {
            if c.is_control() || "/\\:*?\"<>|".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    format!("{}.svg", base.trim().trim_end_matches(".svg"))
}

/// `java.beans.XMLEncoder` output of an `AffineTransform`; the part's SVG is
/// in millimetres, so only the position remains. Java reads the numbers with
/// a decimal point and without exponent.
fn transform_xml(part: &Part) -> String {
    let matrix = [
        1.0,
        0.0,
        0.0,
        1.0,
        f64::from(part.x_mm),
        f64::from(part.y_mm),
    ];
    let entries: String = matrix
        .iter()
        .enumerate()
        .map(|(i, v)| format!("   <void index=\"{i}\">\n    <double>{v:.9}</double>\n   </void>\n"))
        .collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<java version=\"1.8.0\" class=\"java.beans.XMLDecoder\">\n <object class=\"java.awt.geom.AffineTransform\">\n  <array class=\"double\" length=\"6\">\n{entries}  </array>\n </object>\n</java>\n"
    )
}

/// The part's SVG in millimetres and its mappings.
fn part_files(
    part: &Part,
    project: &str,
    order: &str,
    warnings: &mut Vec<String>,
) -> Result<(String, String), String> {
    let source = xml::strip_doctype(&part.svg);
    let doc = roxmltree::Document::parse(&source)
        .map_err(|e| tr!("SVG konnte nicht gelesen werden: {error}", error = e))?;
    let root = doc.root_element();
    let nodes = crate::selection::selectable_nodes(&doc);
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    let mut ids: HashSet<String> = doc
        .descendants()
        .filter_map(|n| n.attribute("id"))
        .map(str::to_string)
        .collect();

    // Objects chosen one by one (Einzeln) become Id filters (Group for
    // groups, whose shapes VisiCut sees); objects without id get one.
    let mut object_filter = |index: usize, edits: &mut Vec<(std::ops::Range<usize>, String)>| {
        let node = nodes.get(index)?;
        let group = node.has_tag_name("g");
        let label = node.attribute(("http://www.inkscape.org/namespaces/inkscape", "label"));
        let value = match (group, label, node.attribute("id")) {
            (true, Some(label), _) => label.to_string(),
            (_, _, Some(id)) => id.to_string(),
            _ => {
                let mut id = format!("visicutr-{}", index + 1);
                while ids.contains(&id) {
                    id.push('_');
                }
                ids.insert(id.clone());
                let start = node.range().start;
                let name_end = start + 1 + xml::qualified_name(&source, start).len();
                edits.push((name_end..name_end, format!(" id=\"{id}\"")));
                id
            }
        };
        Some(Filter {
            attribute: if group {
                Attribute::Group
            } else {
                Attribute::Id
            },
            value,
            compare: false,
            inverted: false,
        })
    };

    let mut mappings = Vec::new();
    if part.steps.is_empty() {
        // „Gesamt“: an empty filter set selects every object.
        mappings.push(mapping(
            Some(&[]),
            Some(profile(part.operation, &part.raster, order, false)),
        ));
    }
    for step in &part.steps {
        let profile = Some(profile(step.operation, &step.raster, order, step.outline));
        let note =
            tr("Weitere Parametersätze eines Schritts werden in der PLF-Datei nicht gespeichert");
        if !step.additional.is_empty() && !warnings.iter().any(|w| w == note) {
            warnings.push(note.into());
        }
        match (&step.filters, step.rest) {
            (_, true) => mappings.push(mapping(None, profile)),
            (Some(filters), false) => mappings.push(mapping(Some(filters), profile)),
            (None, false) => {
                for &object in &step.objects {
                    if let Some(filter) = object_filter(object, &mut edits) {
                        mappings.push(mapping(Some(&[filter]), profile.clone()));
                    }
                }
            }
        }
    }
    for filters in &part.ignore_filters {
        mappings.push(mapping(Some(filters), None));
    }

    // Root in millimetres: the content keeps its user units inside a group
    // that maps them to the part's size, as VisiCutR draws it.
    let matrix = user_to_mm(part, &source, root)?;
    let start = root.range().start;
    let tag_end = xml::start_tag_end(&source, start);
    if xml::is_self_closing(&source, tag_end) {
        return Err(tr("SVG enthält keine Objekte").into());
    }
    let replaced = [
        "x",
        "y",
        "width",
        "height",
        "viewBox",
        "preserveAspectRatio",
    ];
    let removed: Vec<_> = root
        .attributes()
        .filter(|a| a.namespace().is_none() && replaced.contains(&a.name()))
        .map(|a| a.range())
        .collect();
    let (w, h) = (part.width_mm, part.height_mm);
    let added = vec![
        ("width".to_string(), format!("{w}mm")),
        ("height".to_string(), format!("{h}mm")),
        ("viewBox".to_string(), format!("0 0 {w} {h}")),
    ];
    let tag = xml::rewrite_start_tag(&source, start, tag_end, &removed, &added);
    let matrix = matrix.map(|v| v.to_string()).join(" ");
    edits.push((
        start..tag_end,
        format!("{tag}<g transform=\"matrix({matrix})\">"),
    ));
    let close = source[..root.range().end]
        .rfind("</")
        .ok_or(tr("SVG konnte nicht gelesen werden"))?;
    edits.push((close..close, "</g>".into()));
    let svg = xml::apply_edits(&source, 0, edits);

    let entries: String = mappings.concat();
    let mapping_xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<com.t_oster.visicut.model.mapping.MappingSet serialization=\"custom\">\n  <unserializable-parents/>\n  <linked-list>\n    <default/>\n    <int>{}</int>\n{entries}  </linked-list>\n  <name>{}</name>\n</com.t_oster.visicut.model.mapping.MappingSet>\n",
        mappings.len(),
        xml::escape_text(project)
    );
    Ok((svg, mapping_xml))
}

/// Matrix from the SVG's user units to millimetres of the part: the root
/// viewBox (with preserveAspectRatio) to the document size, which is then
/// stretched to the part's width and height.
fn user_to_mm(part: &Part, source: &str, root: roxmltree::Node) -> Result<[f64; 6], String> {
    let tree = usvg::Tree::from_str(source, &crate::svg::options())
        .map_err(|e| tr!("SVG konnte nicht gelesen werden: {error}", error = e))?;
    let (doc_w, doc_h) = (
        f64::from(tree.size().width()),
        f64::from(tree.size().height()),
    );
    let (sx0, sy0) = (
        f64::from(part.width_mm) / doc_w,
        f64::from(part.height_mm) / doc_h,
    );
    let view_box: Option<[f64; 4]> = root.attribute("viewBox").and_then(|v| {
        let n: Vec<f64> = v
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse().ok())
            .collect();
        (n.len() == 4 && n[2] > 0.0 && n[3] > 0.0).then(|| [n[0], n[1], n[2], n[3]])
    });
    let Some([vx, vy, vw, vh]) = view_box else {
        return Ok([sx0, 0.0, 0.0, sy0, 0.0, 0.0]);
    };
    let aspect = root
        .attribute("preserveAspectRatio")
        .unwrap_or("xMidYMid meet");
    let mut words = aspect.split_whitespace();
    let align = words.next().unwrap_or("xMidYMid");
    let slice = words.next() == Some("slice");
    let (mut sx, mut sy) = (doc_w / vw, doc_h / vh);
    let (mut tx, mut ty) = (0.0, 0.0);
    if align != "none" {
        let s = if slice { sx.max(sy) } else { sx.min(sy) };
        (sx, sy) = (s, s);
        let offset = |free: f64, key: &str| {
            if align.contains(&format!("{key}Mid")) {
                free / 2.0
            } else if align.contains(&format!("{key}Max")) {
                free
            } else {
                0.0
            }
        };
        tx = offset(doc_w - vw * s, "x");
        ty = offset(doc_h - vh * s, "Y");
    }
    Ok([
        sx0 * sx,
        0.0,
        0.0,
        sy0 * sy,
        sx0 * (tx - vx * sx),
        sy0 * (ty - vy * sy),
    ])
}

/// One `<mapping>`: `filters` `None` is the rest, `profile` `None` ignores.
fn mapping(filters: Option<&[Filter]>, profile: Option<String>) -> String {
    let mut out = String::from("    <mapping>\n");
    if let Some(filters) = filters {
        out += "      <a class=\"filters\" serialization=\"custom\">\n        <unserializable-parents/>\n        <linked-list>\n          <default/>\n";
        out += &format!("          <int>{}</int>\n", filters.len());
        for filter in filters {
            out += &java_filter(filter);
        }
        out += "        </linked-list>\n      </a>\n";
    }
    if let Some(profile) = profile {
        out += &profile;
    }
    out += "    </mapping>\n";
    out
}

fn java_filter(filter: &Filter) -> String {
    let attribute = match filter.attribute {
        Attribute::Color => "Color",
        Attribute::StrokeColor => "Stroke Color",
        Attribute::FillColor => "Fill Color",
        Attribute::StrokeWidth => "Stroke Width",
        Attribute::Type => "Type",
        Attribute::Id => "Id",
        Attribute::Group => "Group",
    };
    let colour = filter
        .value
        .strip_prefix('#')
        .filter(|hex| hex.len() == 6)
        .and_then(|hex| u32::from_str_radix(hex, 16).ok());
    let value = match (filter.attribute, colour) {
        (Attribute::Color | Attribute::StrokeColor | Attribute::FillColor, Some(rgb)) => format!(
            "<value class=\"awt-color\">\n              <red>{}</red>\n              <green>{}</green>\n              <blue>{}</blue>\n              <alpha>255</alpha>\n            </value>",
            rgb >> 16,
            (rgb >> 8) & 0xff,
            rgb & 0xff
        ),
        (Attribute::StrokeWidth, _) if filter.value.parse::<f64>().is_ok() => {
            let width: f64 = filter.value.parse().unwrap_or_default();
            format!("<value class=\"double\">{width:?}</value>")
        }
        _ => format!(
            "<value class=\"string\">{}</value>",
            xml::escape_text(&filter.value)
        ),
    };
    let compare = if filter.compare {
        "\n            <compare>true</compare>"
    } else {
        ""
    };
    format!(
        "          <filter>\n            <inverted>{}</inverted>\n            <attribute>{attribute}</attribute>\n            {value}{compare}\n          </filter>\n",
        filter.inverted
    )
}

/// VisiCut laser profile with the FAU profile names, so Java VisiCut finds
/// its settings for material and thickness.
fn profile(operation: Operation, raster: &RasterSettings, order: &str, outline: bool) -> String {
    let common = |class: &str, name: &str, description: &str, dpi: u32| {
        format!(
            "      <b class=\"{class}\">\n        <DPI>{dpi}.0</DPI>\n        <description>{}</description>\n        <name>{name}</name>\n",
            xml::escape_text(description)
        )
    };
    match operation {
        Operation::Cut | Operation::Mark => {
            let cut = operation == Operation::Cut;
            let (name, description, width) = if cut {
                ("cut", tr("Schneiden"), "1.0")
            } else {
                ("mark", tr("Markieren"), "0.3")
            };
            common("vectorProfile", name, description, 500)
                + &format!(
                    "        <orderStrategy>{order}</orderStrategy>\n        <useOutline>{outline}</useOutline>\n        <isCut>{cut}</isCut>\n        <width>{width}</width>\n      </b>\n"
                )
        }
        Operation::Engrave => {
            let dithering = match raster.dithering {
                Dithering::Threshold => "Threshold",
                Dithering::FloydSteinberg => "FloydSteinberg",
                Dithering::Average => "Average",
                Dithering::Random => "Random",
                Dithering::Ordered => "Ordered",
                Dithering::Grid => "Grid",
                Dithering::Halftone => "Halftone",
                Dithering::BrightenedHalftone => "BrightenedHalftone",
            };
            common("rasterProfile", "engrave", tr("Gravieren"), raster.dpi)
                + &format!(
                    "        <invertColors>{}</invertColors>\n        <colorShift>{}</colorShift>\n        <ditherAlgorithm class=\"com.t_oster.liblasercut.dithering.{dithering}\"/>\n      </b>\n",
                    raster.invert, raster.color_shift
                )
        }
        Operation::Engrave3d => {
            common("raster3dProfile", "engrave 3d", tr("3D-Gravur"), raster.dpi)
                + &format!(
                    "        <invertColors>{}</invertColors>\n        <colorShift>{}</colorShift>\n      </b>\n",
                    raster.invert, raster.color_shift
                )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::JobStep;

    /// Bounds `[left, top, right, bottom]` of the content in mm on the bed,
    /// for an SVG drawn into `[x, y, width, height]`.
    fn content_mm(svg: &str, place: [f64; 4]) -> [f64; 4] {
        let tree = usvg::Tree::from_str(svg, &crate::svg::options()).unwrap();
        let b = tree.root().abs_bounding_box();
        let (sx, sy) = (
            place[2] / f64::from(tree.size().width()),
            place[3] / f64::from(tree.size().height()),
        );
        [
            place[0] + f64::from(b.left()) * sx,
            place[1] + f64::from(b.top()) * sy,
            place[0] + f64::from(b.right()) * sx,
            place[1] + f64::from(b.bottom()) * sy,
        ]
    }

    fn round_trip(project: &Project) -> (crate::import::Imported, Vec<String>) {
        let path = std::env::temp_dir().join(format!(
            "visicutr-plf-{}-{:?}.plf",
            std::process::id(),
            std::thread::current().id()
        ));
        let warnings = write_plf(project, &path).unwrap();
        let imported = super::super::read_plf(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        (imported, warnings)
    }

    fn assert_close(a: [f64; 4], b: [f64; 4]) {
        assert!(
            a.iter().zip(b).all(|(a, b)| (a - b).abs() < 0.01),
            "{a:?} vs {b:?}"
        );
    }

    // px units, a viewBox with another aspect ratio (meet) and no ids.
    const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="400" height="200" viewBox="0 0 100 100"><rect x="10" y="10" width="30" height="20" fill="none" stroke="#ff0000"/><circle cx="70" cy="50" r="15" fill="#000000"/><path d="M 10 80 L 90 80" stroke="#0000ff" fill="none"/></svg>"##;

    fn project(steps: Vec<JobStep>) -> Project {
        Project {
            name: "Test".into(),
            svg: SVG.into(),
            x_mm: 100.0,
            y_mm: 50.0,
            width_mm: 80.0,
            height_mm: 40.0,
            steps,
            ..Project::default()
        }
    }

    #[test]
    fn single_objects_keep_position_and_assignment() {
        let step = |operation, objects: Vec<usize>| JobStep {
            objects,
            ..JobStep::new(operation)
        };
        let mut engrave = step(Operation::Engrave, vec![1]);
        engrave.raster.dpi = 1000;
        let mut cut = step(Operation::Cut, vec![0]);
        cut.outline = true;
        let original = project(vec![cut, engrave, step(Operation::Mark, vec![2])]);
        let (imported, warnings) = round_trip(&original);
        assert!(warnings.iter().any(|w| w.contains("Laser-Einstellungen")));
        let place = |p: &Project| [p.x_mm, p.y_mm, p.width_mm, p.height_mm].map(f64::from);
        // The composed PLF document starts at the bed origin, in mm.
        let width: f64 = imported
            .svg
            .split("width=\"")
            .nth(1)
            .unwrap()
            .split("mm")
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let height: f64 = imported
            .svg
            .split("height=\"")
            .nth(1)
            .unwrap()
            .split("mm")
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert_close(
            content_mm(&imported.svg, [0.0, 0.0, width, height]),
            content_mm(&original.svg, place(&original)),
        );
        let reopened = Project {
            svg: imported.svg.clone(),
            steps: imported.steps.clone(),
            ..Project::default()
        };
        let operations: Vec<Operation> = imported.steps.iter().map(|s| s.operation).collect();
        assert_eq!(
            operations,
            vec![Operation::Cut, Operation::Engrave, Operation::Mark]
        );
        assert_eq!(
            crate::mapping::resolve(&reopened).unwrap(),
            vec![vec![0], vec![1], vec![2]],
            "{:?}",
            imported.warnings
        );
        assert_eq!(imported.steps[1].raster.dpi, 1000);
        assert!(imported.steps[0].outline && !imported.steps[2].outline);
    }

    #[test]
    fn rules_rest_and_ignore_become_vis_cut_mappings() {
        let mut red = JobStep::new(Operation::Cut);
        red.filters = Some(vec![Filter {
            attribute: Attribute::StrokeColor,
            value: "#ff0000".into(),
            compare: false,
            inverted: false,
        }]);
        let mut rest = JobStep::new(Operation::Engrave);
        rest.rest = true;
        let mut original = project(vec![red, rest]);
        original.ignore_filters = vec![vec![Filter {
            attribute: Attribute::StrokeColor,
            value: "#0000ff".into(),
            compare: false,
            inverted: false,
        }]];
        let (bytes, _) = plf_bytes(&original).unwrap();
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut mapping = String::new();
        std::io::Read::read_to_string(&mut archive.by_name("mappings.xml").unwrap(), &mut mapping)
            .unwrap();
        let entries = super::super::mappings::parse(mapping.as_bytes()).unwrap();
        assert_eq!(entries.len(), 3);
        assert!(entries[1].filters.is_none() && entries[2].profile.is_none());
        let mut transform = String::new();
        std::io::Read::read_to_string(
            &mut archive.by_name("transform.xml").unwrap(),
            &mut transform,
        )
        .unwrap();
        // Java reads plain decimals only.
        let numbers: Vec<&str> = transform
            .split("<double>")
            .skip(1)
            .map(|t| t.split("</double>").next().unwrap())
            .collect();
        assert_eq!(numbers.len(), 6);
        assert!(numbers.iter().all(|n| {
            let (int, frac) = n.trim_start_matches('-').split_once('.').unwrap();
            !int.is_empty()
                && !frac.is_empty()
                && (int.to_string() + frac).bytes().all(|b| b.is_ascii_digit())
        }));
        assert_eq!(numbers[4], "100.000000000");
        let (imported, _) = round_trip(&original);
        let operations: Vec<Operation> = imported.steps.iter().map(|s| s.operation).collect();
        assert_eq!(operations, vec![Operation::Cut, Operation::Engrave]);
        assert_eq!(imported.steps[0].filters.as_ref().map(Vec::len), Some(1));
        // The rest is the circle; the blue line is ignored.
        assert_eq!(imported.steps[1].objects, vec![1]);
    }

    #[test]
    fn whole_motif_and_several_parts() {
        let mut first = project(Vec::new());
        first.operation = Operation::Engrave;
        let mut second = first.clone();
        second.x_mm = 300.0;
        second.operation = Operation::Cut;
        first.parts = vec![second.active()];
        let (bytes, _) = plf_bytes(&first).unwrap();
        let archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut names: Vec<String> = archive
            .file_names()
            .map(|n| n.unwrap().into_owned())
            .collect();
        names.sort();
        assert_eq!(
            names,
            vec![
                "1/Test.svg",
                "1/mappings.xml",
                "1/transform.xml",
                "Test.svg",
                "mappings.xml",
                "transform.xml"
            ]
        );
        let (imported, _) = round_trip(&first);
        let operations: Vec<(Operation, usize)> = imported
            .steps
            .iter()
            .map(|s| (s.operation, s.objects.len()))
            .collect();
        assert_eq!(
            operations,
            vec![(Operation::Engrave, 3), (Operation::Cut, 3)],
            "{:?}",
            imported.warnings
        );
    }
}
