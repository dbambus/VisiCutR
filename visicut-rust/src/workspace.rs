//! Nicht-grafische Logik der egui-Arbeitsfläche: Zoom, Raster, Lineale,
//! Verschieben des Motivs und das Schreiben von LTT-Dateien.
use std::path::{Path, PathBuf};
use visicut_core::{ltt::PreparedStep, project::Project, tr};

/// Zoomstufen relativ zum ganzen Bett: 50 % bis 800 % in 25-%-Schritten.
pub const ZOOM_MIN: f32 = 0.5;
pub const ZOOM_MAX: f32 = 8.0;
/// Bildschirmpunkte je Millimeter bei Originalgröße (96 dpi wie im Browser;
/// die tatsächliche Pixeldichte des Bildschirms ist nicht bekannt).
const ACTUAL_SIZE: f32 = 96.0 / 25.4;
const ZOOM_STEP: f32 = 0.25;

pub fn zoom_in(zoom: f32) -> f32 {
    snap_zoom(zoom + ZOOM_STEP)
}

pub fn zoom_out(zoom: f32) -> f32 {
    snap_zoom(zoom - ZOOM_STEP)
}

/// Zoom, bei dem ein Millimeter etwa einen Millimeter auf dem Bildschirm
/// einnimmt; `fit` ist der Maßstab für das ganze Bett.
pub fn actual_size_zoom(fit: f32) -> f32 {
    (ACTUAL_SIZE / fit.max(0.01)).clamp(ZOOM_MIN, ZOOM_MAX)
}

fn snap_zoom(zoom: f32) -> f32 {
    ((zoom / ZOOM_STEP).round() * ZOOM_STEP).clamp(ZOOM_MIN, ZOOM_MAX)
}

/// Pixel je Millimeter, bei denen das ganze Bett in `available` passt.
pub fn fit_scale(available: [f32; 2], bed_mm: [f32; 2]) -> f32 {
    (available[0] / bed_mm[0])
        .min(available[1] / bed_mm[1])
        .max(0.01)
}

/// Kleinster Abstand der Reihe 1 – 2 – 5 in mm (0,1 … 1000), der mindestens
/// `min_px` Bildpunkte beträgt.
fn nice_step(scale: f32, min_px: f32) -> f32 {
    (-1..=3)
        .flat_map(|exponent| [1.0, 2.0, 5.0].map(|m| m * 10f32.powi(exponent)))
        .take_while(|step| *step <= 1000.0)
        .find(|step| step * scale >= min_px)
        .unwrap_or(1000.0)
}

/// Rasterabstand in mm: der feinste Wert der Reihe 1 – 2 – 5 mit Linien
/// mindestens 6 px auseinander (wie macOS).
pub fn grid_step_mm(scale: f32) -> f32 {
    nice_step(scale, 6.0)
}

/// Abstand der Linealbeschriftung in mm: Reihe 1 – 2 – 5 mit mindestens
/// 40 px Platz je Zahl. Die Rasterlinien dort sind kräftiger.
pub fn ruler_step_mm(scale: f32) -> f32 {
    nice_step(scale, 40.0)
}

/// Linealzahl mit so vielen Nachkommastellen, wie der Abstand braucht.
pub fn ruler_label(value_mm: f32, step_mm: f32) -> String {
    if step_mm >= 1.0 {
        format!("{value_mm:.0}")
    } else {
        format!("{value_mm:.1}")
    }
}

/// Positionen der Linealbeschriftung von 0 bis einschließlich `length`.
pub fn ruler_marks(length_mm: f32, step_mm: f32) -> Vec<f32> {
    let count = (length_mm / step_mm + 1e-3).floor() as usize;
    (0..=count).map(|i| i as f32 * step_mm).collect()
}

/// Größte zulässige Position, damit das Motiv auf dem Bett bleibt.
fn limit(bed: f32, size: f32) -> f32 {
    (bed - size).max(0.0)
}

/// Setzt die Position, auf das Arbeitsbett begrenzt; meldet eine Änderung.
pub fn place(project: &mut Project, x_mm: f32, y_mm: f32) -> bool {
    let x = x_mm.clamp(0.0, limit(project.bed_width_mm, project.width_mm));
    let y = y_mm.clamp(0.0, limit(project.bed_height_mm, project.height_mm));
    let changed = x != project.x_mm || y != project.y_mm;
    (project.x_mm, project.y_mm) = (x, y);
    changed
}

/// Verschiebt das Motiv (Pfeiltasten: 1 mm, mit Umschalt 10 mm).
pub fn nudge(project: &mut Project, dx_mm: f32, dy_mm: f32) -> bool {
    place(project, project.x_mm + dx_mm, project.y_mm + dy_mm)
}

/// Zentriert das Motiv auf dem Bett, wie macOS.
pub fn center(project: &mut Project) {
    project.x_mm = limit(project.bed_width_mm, project.width_mm) / 2.0;
    project.y_mm = limit(project.bed_height_mm, project.height_mm) / 2.0;
}

/// Kleinste Seitenlänge beim Ziehen an einem Anfasser.
const MIN_SIZE_MM: f32 = 0.1;

/// Anfasser des gewählten Teils wie VisiCuts `EditRectangle`: Ecken und
/// Kantenmitten als Anteil von Breite und Höhe (0, ½ oder 1).
pub const HANDLES: [[f32; 2]; 8] = [
    [0.0, 0.0],
    [0.5, 0.0],
    [1.0, 0.0],
    [1.0, 0.5],
    [1.0, 1.0],
    [0.5, 1.0],
    [0.0, 1.0],
    [0.0, 0.5],
];

/// Rechteck `[x, y, Breite, Höhe]` in mm, wenn der Anfasser `handle` von
/// `rect` auf `pointer` gezogen wird. Die gegenüberliegende Ecke bzw. Kante
/// bleibt stehen. Ecken skalieren `proportional` entlang der Diagonale, sonst
/// frei; Kanten ändern eine Seite, `proportional` wächst die andere um ihre
/// Mitte mit. Über den festen Punkt hinaus bleibt die kleinste Größe.
pub fn drag_handle(
    rect: [f32; 4],
    handle: [f32; 2],
    pointer: [f32; 2],
    proportional: bool,
) -> [f32; 4] {
    let [x, y, w, h] = rect;
    let side = |t: f32| (t - 0.5).signum() * f32::from(u8::from(t != 0.5));
    let (sx, sy) = (side(handle[0]), side(handle[1]));
    let fixed = [x + w * (1.0 - handle[0]), y + h * (1.0 - handle[1])];
    let dx = (pointer[0] - fixed[0]) * sx;
    let dy = (pointer[1] - fixed[1]) * sy;
    let (nw, nh) = match (sx != 0.0, sy != 0.0) {
        (true, true) if proportional => {
            // Projektion auf die Diagonale zur gezogenen Ecke.
            let t = ((dx * w + dy * h) / (w * w + h * h)).max(MIN_SIZE_MM / w.min(h));
            (w * t, h * t)
        }
        (true, true) => (dx.max(MIN_SIZE_MM), dy.max(MIN_SIZE_MM)),
        (true, false) => {
            let nw = dx.max(MIN_SIZE_MM);
            (
                nw,
                if proportional {
                    (h * nw / w).max(MIN_SIZE_MM)
                } else {
                    h
                },
            )
        }
        (false, true) => {
            let nh = dy.max(MIN_SIZE_MM);
            (
                if proportional {
                    (w * nh / h).max(MIN_SIZE_MM)
                } else {
                    w
                },
                nh,
            )
        }
        (false, false) => (w, h),
    };
    let place = |fixed: f32, side: f32, size: f32| match side {
        s if s > 0.0 => fixed,
        s if s < 0.0 => fixed - size,
        _ => fixed - size / 2.0,
    };
    [place(fixed[0], sx, nw), place(fixed[1], sy, nh), nw, nh]
}

/// Drehwinkel in Grad beim Ziehen des Drehanfassers von `start` nach
/// `pointer` um `center` (Bildschirm, y nach unten: positiv im
/// Uhrzeigersinn wie `compose::transform`), zwischen −180° und 180°; `snap`
/// rastet in 15°-Schritten ein (Umschalt wie in VisiCut).
pub fn drag_rotation(center: [f32; 2], start: [f32; 2], pointer: [f32; 2], snap: bool) -> f32 {
    let angle = |p: [f32; 2]| (p[1] - center[1]).atan2(p[0] - center[0]);
    let degrees = ((angle(pointer) - angle(start)).to_degrees() + 180.0).rem_euclid(360.0) - 180.0;
    if snap {
        (degrees / 15.0).round() * 15.0
    } else {
        degrees
    }
}

/// Zieldateien eines Exports in einen Ordner.
pub struct ExportPlan {
    pub targets: Vec<PathBuf>,
    /// Dateinamen, die schon existieren und ersetzt würden.
    pub existing: Vec<String>,
}

pub fn export_plan<'a>(folder: &Path, names: impl IntoIterator<Item = &'a str>) -> ExportPlan {
    let targets: Vec<PathBuf> = names
        .into_iter()
        .map(|name| folder.join(format!("{name}.ltt")))
        .collect();
    let existing = targets
        .iter()
        .filter(|path| path.exists())
        .map(|path| {
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into()
        })
        .collect();
    ExportPlan { targets, existing }
}

/// Schreibt neben das Ziel und ersetzt es dann in einem Schritt.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{}.tmp", std::process::id()));
    let temporary = path.with_file_name(name);
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result.map_err(|e| format!("{}: {e}", path.display()))
}

fn percent(value: f32) -> String {
    let text = format!("{value:.1}");
    text.trim_end_matches(".0").replace('.', ",")
}

/// Parameter eines Schritts für die Sende-Bestätigung.
pub fn step_parameters(step: &PreparedStep) -> String {
    let power = percent(step.power_percent);
    let speed = percent(step.speed_percent);
    let mut text = if step.passes == 1 {
        tr!(
            "Leistung {power} % · Geschwindigkeit {speed} % · 1 Durchgang",
            power = power,
            speed = speed
        )
    } else {
        tr!(
            "Leistung {power} % · Geschwindigkeit {speed} % · {passes} Durchgänge",
            power = power,
            speed = speed,
            passes = step.passes
        )
    };
    if step.parameter_sets > 1 {
        text += " · ";
        text += &tr!("{count} Parametersätze", count = step.parameter_sets);
    }
    text
}

/// Dauer wie „ca. 3 min 05 s“.
pub fn duration(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u64;
    match (total / 3600, total / 60 % 60, total % 60) {
        (0, 0, s) => format!("{s} s"),
        (0, m, s) => format!("{m} min {s:02} s"),
        (h, m, _) => format!("{h} h {m:02} min"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use visicut_core::project::Operation;

    #[test]
    fn zoom_steps_are_bounded_and_snapped() {
        assert_eq!(zoom_in(1.0), 1.25);
        assert_eq!(zoom_out(1.0), 0.75);
        assert_eq!(zoom_in(ZOOM_MAX), ZOOM_MAX);
        assert_eq!(zoom_out(ZOOM_MIN), ZOOM_MIN);
        assert_eq!(zoom_in(1.1), 1.25);
        // 1:1 bei 96 dpi: Bett passt mit 1 Punkt je mm, also gut 3,8-fach.
        assert!((actual_size_zoom(1.0) - 3.7795).abs() < 1e-3);
        assert_eq!(actual_size_zoom(100.0), ZOOM_MIN);
    }

    fn close(a: [f32; 4], b: [f32; 4]) -> bool {
        a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-4)
    }

    #[test]
    fn corner_handles_keep_the_opposite_corner_and_the_ratio() {
        let rect = [10.0, 20.0, 40.0, 20.0];
        // Unten rechts nach außen: oben links bleibt, Verhältnis 2 : 1.
        let r = drag_handle(rect, [1.0, 1.0], [90.0, 60.0], true);
        assert!(close(r, [10.0, 20.0, 80.0, 40.0]), "{r:?}");
        // Oben links nach innen: unten rechts (50, 40) bleibt.
        let r = drag_handle(rect, [0.0, 0.0], [30.0, 30.0], true);
        assert!(close(r, [30.0, 30.0, 20.0, 10.0]), "{r:?}");
        // Ohne Kette folgt die Ecke dem Zeiger.
        let r = drag_handle(rect, [1.0, 0.0], [70.0, 0.0], false);
        assert!(close(r, [10.0, 0.0, 60.0, 40.0]), "{r:?}");
        // Über die feste Ecke hinaus bleibt die kleinste Größe.
        let r = drag_handle(rect, [1.0, 1.0], [0.0, 0.0], true);
        assert!(
            r[2] >= 0.1 - 1e-6 && r[3] >= 0.1 - 1e-6 && r[0] == 10.0 && r[1] == 20.0,
            "{r:?}"
        );
    }

    #[test]
    fn edge_handles_change_one_side_or_scale_around_the_middle() {
        let rect = [10.0, 20.0, 40.0, 20.0];
        let r = drag_handle(rect, [1.0, 0.5], [70.0, 0.0], false);
        assert!(close(r, [10.0, 20.0, 60.0, 20.0]), "{r:?}");
        let r = drag_handle(rect, [0.0, 0.5], [30.0, 0.0], false);
        assert!(close(r, [30.0, 20.0, 20.0, 20.0]), "{r:?}");
        // Mit Kette wächst die Höhe um die Mitte (y = 30) mit.
        let r = drag_handle(rect, [1.0, 0.5], [90.0, 0.0], true);
        assert!(close(r, [10.0, 10.0, 80.0, 40.0]), "{r:?}");
        let r = drag_handle(rect, [0.5, 0.0], [0.0, 0.0], true);
        assert!(close(r, [-10.0, 0.0, 80.0, 40.0]), "{r:?}");
    }

    #[test]
    fn rotation_is_clockwise_on_screen_and_snaps() {
        let c = [0.0, 0.0];
        // Von rechts nach unten (y nach unten): 90° im Uhrzeigersinn.
        assert!((drag_rotation(c, [10.0, 0.0], [0.0, 10.0], false) - 90.0).abs() < 1e-4);
        assert!((drag_rotation(c, [10.0, 0.0], [0.0, -10.0], false) + 90.0).abs() < 1e-4);
        // Über ±180° hinweg bleibt der Winkel klein.
        let a = drag_rotation(c, [-10.0, 1.0], [-10.0, -1.0], false);
        assert!(a.abs() < 15.0, "{a}");
        assert_eq!(drag_rotation(c, [10.0, 0.0], [10.0, 2.0], true), 15.0);
        assert_eq!(drag_rotation(c, [10.0, 0.0], [10.0, 1.0], true), 0.0);
    }

    #[test]
    fn fit_scale_uses_the_tighter_axis() {
        assert_eq!(fit_scale([1000.0, 1200.0], [1000.0, 600.0]), 1.0);
        assert_eq!(fit_scale([2000.0, 600.0], [1000.0, 600.0]), 1.0);
        assert_eq!(fit_scale([0.0, 0.0], [1000.0, 600.0]), 0.01);
    }

    #[test]
    fn grid_and_ruler_steps_stay_readable() {
        // Raster und Lineal folgen dem Zoom in der Reihe 1 – 2 – 5.
        assert_eq!(grid_step_mm(0.85), 10.0);
        assert_eq!(grid_step_mm(1.7), 5.0);
        assert_eq!(grid_step_mm(3.4), 2.0);
        assert_eq!(grid_step_mm(6.8), 1.0);
        assert_eq!(grid_step_mm(100.0), 0.1);
        assert_eq!(ruler_step_mm(0.85), 50.0);
        assert_eq!(ruler_step_mm(3.4), 20.0);
        assert_eq!(ruler_step_mm(6.8), 10.0);
        assert_eq!(ruler_step_mm(0.01), 1000.0);
        // Beschriftung liegt immer auf einer Rasterlinie.
        for scale in [0.3, 0.85, 1.7, 3.4, 6.8, 13.0, 40.0] {
            let ratio = ruler_step_mm(scale) / grid_step_mm(scale);
            assert!((ratio - ratio.round()).abs() < 1e-4, "{scale}: {ratio}");
        }
        assert_eq!(ruler_label(250.0, 10.0), "250");
        assert_eq!(ruler_label(12.5, 0.1), "12.5");
        assert_eq!(ruler_marks(600.0, 200.0), vec![0.0, 200.0, 400.0, 600.0]);
        assert_eq!(ruler_marks(650.0, 100.0).len(), 7);
    }

    fn motif() -> Project {
        Project {
            x_mm: 10.0,
            y_mm: 10.0,
            width_mm: 100.0,
            height_mm: 60.0,
            ..Project::default()
        }
    }

    #[test]
    fn nudge_moves_and_stops_at_the_bed_edges() {
        let mut p = motif();
        assert!(nudge(&mut p, 1.0, -10.0));
        assert_eq!((p.x_mm, p.y_mm), (11.0, 0.0));
        assert!(!nudge(&mut p, 0.0, -1.0));
        assert!(nudge(&mut p, 10_000.0, 10_000.0));
        assert_eq!((p.x_mm, p.y_mm), (900.0, 540.0));
        // Ein Motiv größer als das Bett bleibt am Ursprung.
        p.width_mm = 1200.0;
        assert!(nudge(&mut p, 5.0, 0.0));
        assert_eq!(p.x_mm, 0.0);
    }

    #[test]
    fn center_matches_macos() {
        let mut p = motif();
        center(&mut p);
        assert_eq!((p.x_mm, p.y_mm), (450.0, 270.0));
        p.height_mm = 700.0;
        center(&mut p);
        assert_eq!(p.y_mm, 0.0);
    }

    #[test]
    fn export_plan_reports_existing_files_and_writes_atomically() {
        let folder =
            std::env::temp_dir().join(format!("visicut-export-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("Cut.ltt"), b"alt").unwrap();
        let plan = export_plan(&folder, ["Engrav", "Cut"]);
        assert_eq!(plan.targets.len(), 2);
        assert_eq!(plan.existing, vec!["Cut.ltt".to_string()]);
        write_atomic(&plan.targets[1], b"neu").unwrap();
        assert_eq!(std::fs::read(folder.join("Cut.ltt")).unwrap(), b"neu");
        let leftovers = std::fs::read_dir(&folder).unwrap().count();
        assert_eq!(leftovers, 1, "keine temporären Dateien zurücklassen");
        assert!(write_atomic(&folder.join("fehlt/x.ltt"), b"").is_err());
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn step_parameters_and_duration_are_readable() {
        let step = PreparedStep {
            operation: Operation::Cut,
            name: "Cut".into(),
            description: String::new(),
            estimated_seconds: 0.0,
            power_percent: 80.0,
            speed_percent: 1.5,
            passes: 2,
            parameter_sets: 3,
        };
        assert_eq!(
            step_parameters(&step),
            "Leistung 80 % · Geschwindigkeit 1,5 % · 2 Durchgänge · 3 Parametersätze"
        );
        assert_eq!(duration(42.4), "42 s");
        assert_eq!(duration(185.0), "3 min 05 s");
        assert_eq!(duration(3_900.0), "1 h 05 min");
    }
}
