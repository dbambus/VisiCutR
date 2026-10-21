//! Byte comparison with the original Java pipeline.
//!
//! Every folder in `tests/java_parity` holds an input SVG, its settings
//! (`case.properties`) and `expected.ltt`, the job written by Java VisiCut
//! with LibLaserCut's LaserToolsTechnicsCutter for the FAU device. The
//! references are regenerated with `scripts/java-parity/generate.sh`.
//!
//! A case with `deviation=<reason>` is a documented difference (see
//! `scripts/java-parity/AUDIT-2026-10-21.md`): it is reported, not failed, and
//! a note appears when it becomes byte-identical again.

use std::{collections::HashMap, fs, path::Path};
use visicut_core::{
    ltt,
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

fn project(dir: &Path, case: &HashMap<String, String>) -> Project {
    let get = |key: &str| {
        case.get(key)
            .unwrap_or_else(|| panic!("{}: {key} fehlt", dir.display()))
    };
    let number = |key: &str| -> f32 { get(key).parse().unwrap() };
    // Without width_mm/height_mm both sides derive the size from the SVG
    // (Rust like the app's import: usvg size at 96 DPI).
    let svg_text = fs::read_to_string(dir.join(get("svg"))).unwrap();
    let natural = || {
        let preview = visicut_core::svg::render(&svg_text).unwrap();
        (preview.width_mm, preview.height_mm)
    };
    let scale: f32 = case.get("scale").map_or(1.0, |s| s.parse().unwrap());
    let width_mm = case
        .get("width_mm")
        .map_or_else(|| natural().0 * scale, |v| v.parse().unwrap());
    let height_mm = case
        .get("height_mm")
        .map_or_else(|| natural().1 * scale, |v| v.parse().unwrap());
    let operation = match get("operation").as_str() {
        "cut" => Operation::Cut,
        "mark" => Operation::Mark,
        "engrave" => Operation::Engrave,
        "engrave3d" => Operation::Engrave3d,
        other => panic!("{}: unbekanntes Verfahren {other}", dir.display()),
    };
    let mut project = Project {
        name: get("name").clone(),
        svg: fs::read_to_string(dir.join(get("svg"))).unwrap(),
        x_mm: number("x_mm"),
        y_mm: number("y_mm"),
        width_mm,
        height_mm,
        operation,
        power_percent: number("power"),
        speed_percent: number("speed"),
        passes: case.get("passes").map_or(1, |p| p.parse().unwrap()),
        thickness_mm: case.get("thickness_mm").map_or(0.0, |t| t.parse().unwrap()),
        raster: raster(dir, case),
        // Byte-gleich mit Java nur in VisiCuts Reihenfolge.
        cut_order: CutOrder::VisiCut,
        // VisiCut schneidet doppelte Linien mehrfach.
        remove_duplicate_lines: false,
        ..Project::default()
    };
    // Further parameter sets need a step; filters `[]` select every object.
    if let Some(additional) = case.get("additional") {
        project.steps = vec![JobStep {
            filters: Some(Vec::new()),
            power_percent: project.power_percent,
            speed_percent: project.speed_percent,
            passes: project.passes,
            raster: project.raster,
            additional: additional
                .split(';')
                .map(|set| {
                    let v: Vec<&str> = set.trim().split(':').collect();
                    ParameterSet {
                        power_percent: v[0].parse().unwrap(),
                        speed_percent: v[1].parse().unwrap(),
                        passes: v[2].parse().unwrap(),
                    }
                })
                .collect(),
            ..JobStep::new(operation)
        }];
    }
    project
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
    for dir in &dirs {
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        let case = properties(&fs::read_to_string(dir.join("case.properties")).unwrap());
        let known = case.get("deviation");
        match compare(dir, &name, &case) {
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
        dirs.len(),
        failures.join("\n")
    );
}

enum Outcome {
    MissingReference(String),
    Different(String),
}

/// Compares the Rust output of one case with its Java reference.
fn compare(dir: &Path, name: &str, case: &HashMap<String, String>) -> Result<(), Outcome> {
    let java_error = fs::read_to_string(dir.join("expected.error")).ok();
    let expected_file = match fs::read(dir.join("expected.ltt")) {
        Ok(bytes) => bytes,
        Err(_) => {
            let rust = match ltt::prepare(&project(dir, case)) {
                Ok(p) => format!("Rust erzeugt {} Bytes", p.jobs[0].bytes.len()),
                Err(e) => format!("Rust lehnt ab: {e}"),
            };
            return Err(match java_error {
                Some(reason) => {
                    Outcome::Different(format!("{name}: Java lehnt ab: {}; {rust}", reason.trim()))
                }
                None => Outcome::MissingReference(format!(
                    "{name}: expected.ltt fehlt, generate.sh ausführen; {rust}"
                )),
            });
        }
    };
    let prepared = ltt::prepare(&project(dir, case))
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
