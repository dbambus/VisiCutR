//! Byte comparison with the original Java pipeline.
//!
//! Every folder in `tests/java_parity` holds an input SVG, its settings
//! (`case.properties`) and `expected.ltt`, the job written by Java VisiCut
//! with LibLaserCut's LaserToolsTechnicsCutter for the FAU device. The
//! references are regenerated with `scripts/java-parity/generate.sh`.
//!
//! A case with `deviation=<reason>` is a documented difference (see
//! `docs/audits/Java-Vergleich-2026-10-21.md`): it is reported, not failed, and
//! a note appears when it becomes byte-identical again.

use std::{collections::HashMap, fs, path::Path};
use visicut_core::{
    ltt,
    mapping::{Attribute, Filter},
    project::{CutOrder, JobStep, Operation, ParameterSet, Project},
    raster::{Dithering, RasterSettings},
};

fn properties(text: &str) -> HashMap<String, String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
        .collect()
}

fn project(dir: &Path, case: &HashMap<String, String>) -> Result<Project, String> {
    let get = |key: &str| {
        case.get(key)
            .unwrap_or_else(|| panic!("{}: {key} fehlt", dir.display()))
    };
    let number = |key: &str| -> f32 { get(key).parse().unwrap() };
    // Rust reads `rust_file` (default: the Java file); anything but SVG goes
    // through the app's importers (DXF, EPS, PDF, raster, G-code, PLF).
    let rust_file = dir.join(case.get("rust_file").unwrap_or_else(|| get("svg")));
    // Parametric SVG (`.parametric.svg`) goes through its importer like in the app.
    let plain_svg = rust_file.extension().is_some_and(|e| e == "svg")
        && !rust_file.to_string_lossy().ends_with(".parametric.svg");
    let imported = if plain_svg {
        visicut_core::svg_import::Imported {
            svg: fs::read_to_string(&rust_file).unwrap(),
            ..Default::default()
        }
    } else {
        visicut_core::import::read_file(&rust_file)
            .map_err(|e| format!("Rust-Import lehnt ab: {e}"))?
    };
    let svg_text = imported.svg.clone();
    // Without width_mm/height_mm both sides derive the size from the file
    // (Rust like the app's import: usvg size at 96 DPI).
    let natural = || {
        let preview = visicut_core::svg::render(&svg_text).unwrap();
        (preview.width_mm, preview.height_mm)
    };
    let scale: f32 = case.get("scale").map_or(1.0, |s| s.parse().unwrap());
    // `place=bbox`: the top left corner of the geometry goes to x/y.
    // A PLF part keeps its place relative to the bed (offset of a cropped part).
    let (mut x_mm, mut y_mm) = (
        number("x_mm") + imported.offset_mm[0] as f32,
        number("y_mm") + imported.offset_mm[1] as f32,
    );
    if case.get("place").is_some_and(|p| p == "bbox") {
        let tree = resvg::usvg::Tree::from_str(&svg_text, &visicut_core::svg::options()).unwrap();
        let bbox = tree.root().abs_bounding_box();
        let (w, h) = natural();
        x_mm -= bbox.x() * w / tree.size().width();
        y_mm -= bbox.y() * h / tree.size().height();
    }
    let width_mm = case
        .get("width_mm")
        .map_or_else(|| natural().0 * scale, |v| v.parse().unwrap());
    let height_mm = case
        .get("height_mm")
        .map_or_else(|| natural().1 * scale, |v| v.parse().unwrap());
    let operation = operation_named(get("operation")).unwrap();
    let mut project = Project {
        name: get("name").clone(),
        svg: svg_text.clone(),
        x_mm,
        y_mm,
        width_mm,
        height_mm,
        operation,
        // Java's defaults where a case (PLF) gives its values per step.
        power_percent: case.get("power").map_or(20.0, |p| p.parse().unwrap()),
        speed_percent: case.get("speed").map_or(100.0, |p| p.parse().unwrap()),
        passes: case.get("passes").map_or(1, |p| p.parse().unwrap()),
        thickness_mm: case.get("thickness_mm").map_or(0.0, |t| t.parse().unwrap()),
        raster: raster(dir, case),
        // VisiCut's orders under their Java names; INNER_FIRST is "Klassisch".
        cut_order: match case.get("order").map_or("INNER_FIRST", String::as_str) {
            "INNER_FIRST" => CutOrder::VisiCut,
            "FILE" => CutOrder::File,
            "NEAREST" => CutOrder::Nearest,
            "SMALLEST_FIRST" => CutOrder::SmallestFirst,
            other => panic!("{}: unbekannte Reihenfolge {other}", dir.display()),
        },
        rotary_axis: case.contains_key("rotary_diameter_mm"),
        rotary_diameter_mm: case
            .get("rotary_diameter_mm")
            .map_or(100.0, |d| d.parse().unwrap()),
        // VisiCut schneidet doppelte Linien mehrfach.
        remove_duplicate_lines: false,
        // VisiCut vergleicht Farben exakt.
        similar_colors: false,
        ..Project::default()
    };
    let outline = case.get("outline").is_some_and(|o| o == "true");
    if rust_file.extension().is_some_and(|e| e == "plf") {
        // The PLF's mappings as steps; `settings` gives their laser values in
        // part and mapping order, like the Java harness.
        let settings: Vec<&str> = case
            .get("settings")
            .map_or("", String::as_str)
            .split(';')
            .collect();
        assert_eq!(
            settings.len(),
            imported.steps.len(),
            "{}: settings je Schritt",
            dir.display()
        );
        project.steps = imported
            .steps
            .iter()
            .zip(settings)
            .map(|(step, values)| {
                let v: Vec<&str> = values.trim().split(':').collect();
                JobStep {
                    power_percent: v[0].parse().unwrap(),
                    speed_percent: v[1].parse().unwrap(),
                    passes: v.get(2).map_or(1, |p| p.parse().unwrap()),
                    ..step.clone()
                }
            })
            .collect();
        if !imported.extra_parts.is_empty() {
            visicut_core::compose::add_extra_parts(&mut project, imported.extra_parts)?;
        }
    } else if let Some(mappings) = case.get("mappings") {
        // "filters -> power:speed[:passes]; …" in mapping order (README).
        project.steps = mappings
            .split(';')
            .map(|entry| {
                let (filters, values) = entry.split_once("->").unwrap();
                let v: Vec<&str> = values.trim().split(':').collect();
                // An optional `cut:`, `mark:`, `engrave:` … in front of the filters.
                let (operation, filters) = match filters.trim().split_once(':') {
                    Some((op, rest)) if operation_named(op).is_some() => {
                        (operation_named(op).unwrap(), rest.trim())
                    }
                    _ => (operation, filters.trim()),
                };
                let (filters, rest) = filter_set(filters);
                JobStep {
                    filters,
                    rest,
                    power_percent: v[0].parse().unwrap(),
                    speed_percent: v[1].parse().unwrap(),
                    passes: v.get(2).map_or(1, |p| p.parse().unwrap()),
                    raster: project.raster,
                    outline,
                    ..JobStep::new(operation)
                }
            })
            .collect();
    } else if case.contains_key("additional") || outline {
        // Further parameter sets need a step; filters `[]` select every object.
        project.steps = vec![JobStep {
            filters: Some(Vec::new()),
            power_percent: project.power_percent,
            speed_percent: project.speed_percent,
            passes: project.passes,
            raster: project.raster,
            additional: case
                .get("additional")
                .map_or("", String::as_str)
                .split(';')
                .filter(|set| !set.trim().is_empty())
                .map(|set| {
                    let v: Vec<&str> = set.trim().split(':').collect();
                    ParameterSet {
                        power_percent: v[0].parse().unwrap(),
                        speed_percent: v[1].parse().unwrap(),
                        passes: v[2].parse().unwrap(),
                    }
                })
                .collect(),
            outline,
            ..JobStep::new(operation)
        }];
    }
    Ok(project)
}

fn operation_named(name: &str) -> Option<Operation> {
    Some(match name {
        "cut" => Operation::Cut,
        "mark" => Operation::Mark,
        "engrave" => Operation::Engrave,
        "engrave3d" => Operation::Engrave3d,
        _ => return None,
    })
}

/// Filters of a mapping under VisiCut's attribute names (LttParity.java):
/// `*` is "everything else", `all` everything, otherwise filters joined by
/// `&`, each `[!]Attribute=value` or `[!]Attribute<=value`.
fn filter_set(spec: &str) -> (Option<Vec<Filter>>, bool) {
    match spec {
        "*" => return (None, true),
        "all" => return (Some(Vec::new()), false),
        _ => {}
    }
    let filters = spec
        .split('&')
        .map(|f| {
            let f = f.trim();
            let (inverted, f) = f.strip_prefix('!').map_or((false, f), |f| (true, f));
            let (compare, (attribute, value)) = match f.split_once("<=") {
                Some(kv) => (true, kv),
                None => (false, f.split_once('=').unwrap()),
            };
            let attribute = match attribute.trim() {
                "Color" => Attribute::Color,
                "Stroke Color" => Attribute::StrokeColor,
                "Fill Color" => Attribute::FillColor,
                "Stroke Width" => Attribute::StrokeWidth,
                "Type" => Attribute::Type,
                "Id" => Attribute::Id,
                "Group" => Attribute::Group,
                other => panic!("unbekanntes Attribut {other}"),
            };
            Filter {
                attribute,
                value: value.trim().into(),
                compare,
                inverted,
            }
        })
        .collect();
    (Some(filters), false)
}

/// Raster options under the names of the Java harness (LttParity.java).
fn raster(dir: &Path, case: &HashMap<String, String>) -> RasterSettings {
    let flag = |key: &str| case.get(key).is_some_and(|v| v == "true");
    let dithering = match case.get("dither").map_or("FloydSteinberg", String::as_str) {
        "FloydSteinberg" => Dithering::FloydSteinberg,
        "Average" => Dithering::Average,
        "Random" => Dithering::Random,
        "Ordered" => Dithering::Ordered,
        "Grid" => Dithering::Grid,
        "Halftone" => Dithering::Halftone,
        "BrightenedHalftone" => Dithering::BrightenedHalftone,
        other => panic!("{}: unbekanntes Rasterverfahren {other}", dir.display()),
    };
    RasterSettings {
        dithering,
        invert: flag("invert"),
        color_shift: case.get("color_shift").map_or(0, |v| v.parse().unwrap()),
        bidirectional: !flag("unidirectional"),
        bottom_up: flag("bottom_up"),
        // Java's LTT driver only offers 500 DPI.
        dpi: 500,
        // Java has no stamp shoulder.
        shoulder_mm: 0.0,
    }
}

/// First differing offset with a little context, for a readable failure.
fn difference(actual: &[u8], expected: &[u8]) -> Option<String> {
    let at = actual
        .iter()
        .zip(expected)
        .position(|(a, e)| a != e)
        .or_else(|| (actual.len() != expected.len()).then(|| actual.len().min(expected.len())))?;
    let window = |bytes: &[u8]| {
        bytes[at.saturating_sub(8)..(at + 24).min(bytes.len())]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    Some(format!(
        "erste Abweichung bei Byte {at} (Rust {} Bytes, Java {} Bytes)\n  Rust: {}\n  Java: {}",
        actual.len(),
        expected.len(),
        window(actual),
        window(expected)
    ))
}

#[test]
fn ltt_output_matches_java_byte_for_byte() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/java_parity");
    let mut dirs: Vec<_> = fs::read_dir(&root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.join("case.properties").exists())
        .collect();
    dirs.sort();
    assert!(!dirs.is_empty(), "keine Vergleichsfälle gefunden");
    let mut failures = Vec::new();
    let mut deviations = Vec::new();
    let mut resolved = Vec::new();
    let mut count = 0;
    for dir in &dirs {
        let base = dir.file_name().unwrap().to_string_lossy().into_owned();
        let case = properties(&fs::read_to_string(dir.join("case.properties")).unwrap());
        for (suffix, case) in variants(&case) {
            count += 1;
            let name = format!("{base}{suffix}");
            let known = case.get("deviation");
            match compare(dir, &name, &case, &suffix) {
                Ok(()) => {
                    if let Some(reason) = known {
                        resolved.push(format!(
                            "{name}: byte-gleich, deviation-Eintrag ({reason}) kann entfallen"
                        ));
                    }
                }
                // A missing reference is a setup problem, never a documented deviation.
                Err(Outcome::MissingReference(message)) => failures.push(message),
                Err(Outcome::Different(message)) => match known {
                    Some(reason) => deviations.push(format!("{message}\n  bekannt: {reason}")),
                    None => failures.push(message),
                },
            }
        }
    }
    if !deviations.is_empty() {
        println!(
            "{} dokumentierte Abweichungen von Java:\n{}",
            deviations.len(),
            deviations.join("\n")
        );
    }
    if !resolved.is_empty() {
        println!("{}", resolved.join("\n"));
    }
    assert!(
        failures.is_empty(),
        "{} von {} Fällen weichen von Java ab:\n{}",
        failures.len(),
        count,
        failures.join("\n")
    );
}

enum Outcome {
    MissingReference(String),
    Different(String),
}

/// `single_job=true`: all operations in one job, as Java writes them.
fn prepare(project: &Project, case: &HashMap<String, String>) -> Result<ltt::PreparedJob, String> {
    ltt::prepare_with(project, case.get("single_job").is_some_and(|s| s == "true"))
}

/// The case itself (suffix "") and each of its `variants`, a comma-separated
/// list of `key=value` overrides such as `order=FILE, rotary_diameter_mm=100`.
/// A variant has the suffix `@key-value` (other characters than letters,
/// digits, `-`, `_` and `.` become `_`), its references
/// `expected@key-value.ltt`/`.error` and its own `deviation@key-value`.
/// Variants run in release builds only (`cargo test --release`, also in CI):
/// unoptimised, the 570 of them add three minutes.
fn variants(case: &HashMap<String, String>) -> Vec<(String, HashMap<String, String>)> {
    let mut all = vec![(String::new(), case.clone())];
    let list = match case.get("variants") {
        Some(list) if !cfg!(debug_assertions) => list.as_str(),
        _ => "",
    };
    for (key, value) in list.split(',').filter_map(|v| v.trim().split_once('=')) {
        // File-name safe on every system (no `:` on Windows).
        let suffix: String = format!("@{key}-{value}")
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || "@-_.".contains(c) {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let mut variant = case.clone();
        variant.remove("variants");
        variant.remove("deviation");
        if let Some(reason) = case.get(&format!("deviation{suffix}")) {
            variant.insert("deviation".into(), reason.clone());
        }
        variant.insert(key.into(), value.into());
        all.push((suffix, variant));
    }
    all
}

/// Compares the Rust output of one case (or variant) with its Java reference.
fn compare(
    dir: &Path,
    name: &str,
    case: &HashMap<String, String>,
    suffix: &str,
) -> Result<(), Outcome> {
    let java_error = fs::read_to_string(dir.join(format!("expected{suffix}.error"))).ok();
    // `expect=reject`: both sides must refuse the job (e.g. engraving rows
    // outside the bed).
    let reject = case.get("expect").is_some_and(|e| e == "reject");
    let expected_file = match fs::read(dir.join(format!("expected{suffix}.ltt"))) {
        Ok(_) if reject => {
            return Err(Outcome::Different(format!(
                "{name}: Java erzeugt einen Auftrag, erwartet ist eine Ablehnung"
            )));
        }
        Ok(bytes) => bytes,
        Err(_) => {
            let rust = match project(dir, case).and_then(|p| prepare(&p, case)) {
                Ok(p) => format!("Rust erzeugt {} Bytes", p.jobs[0].bytes.len()),
                Err(_) if reject && java_error.is_some() => return Ok(()),
                Err(e) => format!("Rust lehnt ab: {e}"),
            };
            return Err(match java_error {
                Some(reason) => {
                    Outcome::Different(format!("{name}: Java lehnt ab: {}; {rust}", reason.trim()))
                }
                None => Outcome::MissingReference(format!(
                    "{name}: expected{suffix}.ltt fehlt, generate.sh ausführen; {rust}"
                )),
            });
        }
    };
    let prepared = project(dir, case)
        .and_then(|p| prepare(&p, case))
        .map_err(|e| Outcome::Different(format!("{name}: Rust lehnt ab: {e}")))?;
    if prepared.jobs.len() != 1 {
        return Err(Outcome::Different(format!(
            "{name}: {} Aufträge statt 1",
            prepared.jobs.len()
        )));
    }
    let mut actual = prepared.jobs[0].bytes.as_slice();
    let mut expected = expected_file.as_slice();
    // `compare=header`: only up to the first raster line, for documented
    // deliberate differences in the lines (reason in case.properties).
    if case.get("compare").is_some_and(|c| c == "header") {
        let header = |bytes: &[u8]| {
            bytes
                .windows(2)
                .position(|w| w == [0x1b, b'0'] || w == [0x1b, b'1'])
                .unwrap_or(bytes.len())
        };
        expected = &expected[..header(expected)];
        actual = &actual[..header(actual).max(expected.len()).min(actual.len())];
    }
    if let Some(diff) = difference(actual, expected) {
        // Kept for inspection next to the build output.
        let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}.ltt"));
        fs::write(&path, actual).unwrap();
        return Err(Outcome::Different(format!(
            "{name}: {diff}\n  Rust-Ausgabe: {}",
            path.display()
        )));
    }
    Ok(())
}
