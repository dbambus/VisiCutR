//! Schnittmarken zum Drucken und Lasern: Die Druckvorlage legt das Motiv mit
//! drei Marken auf ein A4-Blatt. Liegt das gedruckte Blatt auf dem Bett,
//! findet die Kamera die Marken; Drehung, Maßstab und Lage der Teile folgen
//! dem Blatt.
//!
//! Die Marken sind Ringe mit Punkt wie VisiCams Passmarken, damit
//! [`crate::camera::find_markers`] sie erkennt. Sie liegen in drei Ecken; die
//! vierte bleibt frei, so ist die Lage des Blatts eindeutig.

use crate::project::{Project, Registration};
use crate::{compose, i18n::tr, tr};

/// DIN A4 hochkant (mm).
pub const SHEET_MM: [f64; 2] = [210.0, 297.0];
/// Ring von 8 bis 12 mm Radius mit Punkt von 3 mm Radius: Punkt zu hellem
/// Zwischenraum 0,375, wie es die Markenerkennung erwartet.
const RING_INNER_MM: f64 = 8.0;
const RING_OUTER_MM: f64 = 12.0;
const DOT_MM: f64 = 3.0;
/// Abstand der Markenmitten vom Blattrand.
const MARK_INSET_MM: f64 = 22.0;
/// Fläche für das Motiv zwischen den Marken: `[x, y, Breite, Höhe]`.
pub const PRINT_AREA_MM: [f64; 4] = [10.0, 40.0, 190.0, 217.0];
/// Abweichung der Abstände zwischen den Marken, die noch als Blatt gilt
/// (Druckermaßstab, Erkennung).
const DISTANCE_TOLERANCE: f64 = 0.04;
/// Größte Maßstabsabweichung des Drucks, die übernommen wird.
const MAX_SCALE_ERROR: f64 = 0.05;

/// Mitten der drei Marken auf dem Blatt: oben links, oben rechts, unten links.
pub fn sheet_marks() -> [[f64; 2]; 3] {
    let [w, h] = SHEET_MM;
    let i = MARK_INSET_MM;
    [[i, i], [w - i, i], [i, h - i]]
}

/// Eine Marke als SVG (Mitte in mm).
fn mark_svg([x, y]: [f64; 2]) -> String {
    let ring = (RING_INNER_MM + RING_OUTER_MM) / 2.0;
    let width = RING_OUTER_MM - RING_INNER_MM;
    format!(
        r#"<circle cx="{x}" cy="{y}" r="{ring}" fill="none" stroke="black" stroke-width="{width}"/><circle cx="{x}" cy="{y}" r="{DOT_MM}" fill="black"/>"#
    )
}

/// Druckvorlage: SVG des Blatts und die Lage der Marken auf dem Bett.
pub struct Template {
    pub svg: String,
    pub registration: Registration,
}

/// Legt alle Teile mittig in die Druckfläche und die Marken in die Ecken.
pub fn template(project: &Project) -> Result<Template, String> {
    if !project.has_artwork() {
        return Err(tr("Zuerst ein Motiv öffnen").into());
    }
    let [bx, by, bw, bh] = project.bounds().map(f64::from);
    let [ax, ay, aw, ah] = PRINT_AREA_MM;
    if bw > aw + 1e-6 || bh > ah + 1e-6 {
        return Err(tr!(
            "Das Motiv ({width} × {height} mm) ist größer als die Druckfläche zwischen den Schnittmarken ({area_width} × {area_height} mm)",
            width = format!("{bw:.0}"),
            height = format!("{bh:.0}"),
            area_width = format!("{aw:.0}"),
            area_height = format!("{ah:.0}")
        ));
    }
    // Verschiebung vom Bett aufs Blatt.
    let (dx, dy) = (ax + (aw - bw) / 2.0 - bx, ay + (ah - bh) / 2.0 - by);
    let marks = sheet_marks();
    let mut body = compose::all_parts_svg(project, dx, dy)?;
    for mark in marks {
        body += &mark_svg(mark);
    }
    let [w, h] = SHEET_MM;
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{w}mm" height="{h}mm" viewBox="0 0 {w} {h}">{body}</svg>"#
    );
    Ok(Template {
        svg,
        registration: Registration {
            marks: marks.map(|[x, y]| [x - dx, y - dy]),
        },
    })
}

/// Die Druckvorlage als PDF in Originalgröße (A4).
pub fn pdf(svg: &str) -> Result<Vec<u8>, String> {
    let tree = resvg::usvg::Tree::from_str(svg, &crate::svg::options())
        .map_err(|e| tr!("SVG konnte nicht gelesen werden: {error}", error = e))?;
    // usvg rechnet mit 96 px je Zoll.
    let page = svg2pdf::PageOptions { dpi: 96.0 };
    svg2pdf::to_pdf(&tree, svg2pdf::ConversionOptions::default(), page)
        .map_err(|e| tr!("PDF konnte nicht erzeugt werden: {error}", error = e))
}

/// Ergebnis des Ausrichtens.
pub struct Alignment {
    pub project: Project,
    /// Drehung des Blatts gegenüber der Vorlage (Grad, im Uhrzeigersinn).
    pub angle_deg: f64,
    /// Maßstab des Drucks (1 = genau).
    pub scale: f64,
    /// Größter Abstand einer Marke zur ausgeglichenen Lage (mm).
    pub residual_mm: f64,
}

fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn cross(o: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

/// Sucht unter den gefundenen Marken (mm) die drei des Blatts: Ihre Abstände
/// müssen zu `expected` passen. Liefert sie in derselben Reihenfolge.
pub fn match_marks(found: &[[f64; 2]], expected: [[f64; 2]; 3]) -> Result<[[f64; 2]; 3], String> {
    let lengths = [
        distance(expected[0], expected[1]),
        distance(expected[0], expected[2]),
        distance(expected[1], expected[2]),
    ];
    let mut best: Option<(f64, [usize; 3])> = None;
    for i in 0..found.len() {
        for j in 0..found.len() {
            for k in 0..found.len() {
                if i == j || i == k || j == k {
                    continue;
                }
                let error = [
                    distance(found[i], found[j]) / lengths[0],
                    distance(found[i], found[k]) / lengths[1],
                    distance(found[j], found[k]) / lengths[2],
                ]
                .iter()
                .map(|r| (r - 1.0).abs())
                .fold(0.0, f64::max);
                if best.is_none_or(|(e, _)| error < e) {
                    best = Some((error, [i, j, k]));
                }
            }
        }
    }
    let Some((error, [i, j, k])) = best.filter(|(e, _)| *e <= DISTANCE_TOLERANCE) else {
        return Err(match found.len() {
            0..=2 => tr!(
                "Nur {count} von 3 Schnittmarken im Kamerabild gefunden; liegt das ganze Blatt im Bild?",
                count = found.len()
            ),
            _ => tr("Die gefundenen Marken passen nicht zur Druckvorlage (Abstände)").into(),
        });
    };
    let _ = error;
    let marks = [found[i], found[j], found[k]];
    // Gleiche Abstände, aber gespiegelt: das Blatt liegt mit der Rückseite oben.
    if cross(expected[0], expected[1], expected[2]).signum()
        != cross(marks[0], marks[1], marks[2]).signum()
    {
        return Err(tr(
            "Das Blatt liegt gespiegelt; bitte mit der bedruckten Seite nach oben auflegen",
        )
        .into());
    }
    Ok(marks)
}

/// Ähnlichkeitsabbildung (Drehung, gleichmäßiger Maßstab, Verschiebung), die
/// `from` am besten auf `to` legt: `(Maßstab, Winkel im Bogenmaß, Verschiebung)`.
fn similarity(from: &[[f64; 2]], to: &[[f64; 2]]) -> (f64, f64, [f64; 2]) {
    let n = from.len() as f64;
    let mean = |points: &[[f64; 2]]| {
        let [x, y] = points
            .iter()
            .fold([0.0, 0.0], |a, p| [a[0] + p[0], a[1] + p[1]]);
        [x / n, y / n]
    };
    let (mf, mt) = (mean(from), mean(to));
    let (mut dot, mut det, mut norm) = (0.0, 0.0, 0.0);
    for (f, t) in from.iter().zip(to) {
        let (a, b) = ([f[0] - mf[0], f[1] - mf[1]], [t[0] - mt[0], t[1] - mt[1]]);
        dot += a[0] * b[0] + a[1] * b[1];
        det += a[0] * b[1] - a[1] * b[0];
        norm += a[0] * a[0] + a[1] * a[1];
    }
    let angle = det.atan2(dot);
    let scale = dot.hypot(det) / norm;
    let (sin, cos) = angle.sin_cos();
    let shift = [
        mt[0] - scale * (cos * mf[0] - sin * mf[1]),
        mt[1] - scale * (sin * mf[0] + cos * mf[1]),
    ];
    (scale, angle, shift)
}

fn apply(scale: f64, angle: f64, shift: [f64; 2], p: [f64; 2]) -> [f64; 2] {
    let (sin, cos) = angle.sin_cos();
    [
        shift[0] + scale * (cos * p[0] - sin * p[1]),
        shift[1] + scale * (sin * p[0] + cos * p[1]),
    ]
}

/// Richtet alle Teile an den gefundenen Marken (Bett-mm) aus.
pub fn align(project: &Project, found: &[[f64; 2]]) -> Result<Alignment, String> {
    let registration = project
        .registration
        .as_ref()
        .ok_or(tr("Das Projekt hat keine Druckvorlage mit Schnittmarken"))?;
    let marks = match_marks(found, registration.marks)?;
    let (scale, angle, shift) = similarity(&registration.marks, &marks);
    if (scale - 1.0).abs() > MAX_SCALE_ERROR {
        return Err(tr!(
            "Der Druck ist um {percent} % verkleinert oder vergrößert; bitte in Originalgröße (100 %) drucken",
            percent = format!("{:.0}", (scale - 1.0).abs() * 100.0)
        ));
    }
    let residual_mm = registration
        .marks
        .iter()
        .zip(&marks)
        .map(|(m, p)| distance(apply(scale, angle, shift, *m), *p))
        .fold(0.0, f64::max);
    let angle_deg = angle.to_degrees();
    let mut parts = Vec::new();
    for part in project.all_parts() {
        let view = project.part_view(&part);
        let turned = if angle_deg.abs() > 1e-6 {
            compose::transform(&view, angle_deg, false, false)?.active()
        } else {
            part.clone()
        };
        let center = [
            (part.x_mm + part.width_mm / 2.0) as f64,
            (part.y_mm + part.height_mm / 2.0) as f64,
        ];
        let [cx, cy] = apply(scale, angle, shift, center);
        let (w, h) = (
            turned.width_mm as f64 * scale,
            turned.height_mm as f64 * scale,
        );
        parts.push(crate::project::Part {
            x_mm: (cx - w / 2.0) as f32,
            y_mm: (cy - h / 2.0) as f32,
            width_mm: w as f32,
            height_mm: h as f32,
            ..turned
        });
    }
    let mut result = project.clone();
    result.set_all_parts(parts, project.active_part);
    result.registration = Some(Registration { marks });
    Ok(Alignment {
        project: result,
        angle_deg,
        scale,
        residual_mm,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> Project {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="40mm" height="20mm" viewBox="0 0 40 20"><rect width="40" height="20" fill="none" stroke="red" stroke-width="0.1"/></svg>"#;
        Project {
            svg: svg.into(),
            part_name: "Rahmen".into(),
            x_mm: 100.0,
            y_mm: 50.0,
            width_mm: 40.0,
            height_mm: 20.0,
            ..Project::default()
        }
    }

    #[test]
    fn template_places_motif_between_marks_and_remembers_them() {
        let template = template(&project()).unwrap();
        assert!(template.svg.contains(r#"width="210mm""#));
        // Motiv mittig in der Druckfläche: oben links bei (85, 138.5) auf dem Blatt.
        let [ax, ay, aw, ah] = PRINT_AREA_MM;
        let (dx, dy) = (
            ax + (aw - 40.0) / 2.0 - 100.0,
            ay + (ah - 20.0) / 2.0 - 50.0,
        );
        assert_eq!(template.registration.marks[0], [22.0 - dx, 22.0 - dy]);
        let pdf = pdf(&template.svg).unwrap();
        assert!(pdf.starts_with(b"%PDF"));
        let mut big = project();
        big.width_mm = 300.0;
        assert!(super::template(&big).is_err());
    }

    #[test]
    fn marks_found_on_a_turned_sheet_move_and_turn_the_parts() {
        let mut p = project();
        p.registration = Some(template(&p).unwrap().registration);
        let expected = p.registration.as_ref().unwrap().marks;
        // Blatt um 10° gedreht und verschoben aufgelegt; dazu eine Passmarke am Bett.
        let (s, a, t) = (1.0, 10f64.to_radians(), [40.0, -15.0]);
        let mut found: Vec<[f64; 2]> = expected.iter().map(|m| apply(s, a, t, *m)).collect();
        found.reverse();
        found.push([900.0, 500.0]);
        let aligned = align(&p, &found).unwrap();
        assert!((aligned.angle_deg - 10.0).abs() < 1e-6);
        assert!((aligned.scale - 1.0).abs() < 1e-9);
        assert!(aligned.residual_mm < 1e-6);
        // Die Mitte des Teils folgt dem Blatt.
        let q = &aligned.project;
        let center = [
            (q.x_mm + q.width_mm / 2.0) as f64,
            (q.y_mm + q.height_mm / 2.0) as f64,
        ];
        let want = apply(s, a, t, [120.0, 60.0]);
        assert!(distance(center, want) < 0.01, "{center:?} {want:?}");
        // Gedreht wird es größer (umschließendes Rechteck).
        assert!(q.width_mm > 40.0 && q.height_mm > 20.0);
        // Ein zweites Ausrichten an denselben Marken ändert nichts mehr.
        let again = align(q, &found).unwrap();
        assert!(again.angle_deg.abs() < 1e-6);
    }

    /// Ein gedruckt aufgelegtes Blatt im Kamerabild (1,5 Pixel je mm, etwa
    /// eine Kamera über dem ganzen Bett): Die Markenerkennung findet die
    /// Marken, das Ausrichten trifft die Lage auf Bruchteile eines Millimeters.
    #[test]
    fn printed_marks_are_found_in_a_bed_picture() {
        let mut p = project();
        let template = template(&p).unwrap();
        p.registration = Some(template.registration);
        let expected = p.registration.as_ref().unwrap().marks;
        let (a, t) = (-7.5f64, [310.0, 140.0]);
        // Blattkoordinaten → Bett: erst das Blatt drehen, dann verschieben.
        let marks: String = sheet_marks().iter().map(|m| mark_svg(*m)).collect();
        let [w, h] = SHEET_MM;
        let bed = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="1500" height="900" viewBox="0 0 1000 600"><rect width="1000" height="600" fill="rgb(90,90,90)"/><g transform="translate({} {}) rotate({a})"><rect width="{w}" height="{h}" fill="white"/>{marks}</g></svg>"#,
            t[0], t[1]
        );
        let tree = resvg::usvg::Tree::from_str(&bed, &crate::svg::options()).unwrap();
        let mut pixmap = resvg::tiny_skia::Pixmap::new(1500, 900).unwrap();
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::default(),
            &mut pixmap.as_mut(),
        );
        let image = image::RgbaImage::from_raw(1500, 900, pixmap.take()).unwrap();
        let found: Vec<[f64; 2]> = crate::camera::find_markers(&image)
            .into_iter()
            .map(|[x, y]| [x / 1.5, y / 1.5])
            .collect();
        assert_eq!(found.len(), 3, "{found:?}");
        let aligned = align(&p, &found).unwrap();
        assert!((aligned.angle_deg - a).abs() < 0.2, "{}", aligned.angle_deg);
        assert!(aligned.residual_mm < 0.5, "{}", aligned.residual_mm);
        // Die Marken der Vorlage liegen jetzt auf denen des Blatts.
        let sheet = sheet_marks();
        let (sin, cos) = a.to_radians().sin_cos();
        for (m, s) in aligned
            .project
            .registration
            .unwrap()
            .marks
            .iter()
            .zip(sheet)
        {
            let want = [
                t[0] + cos * s[0] - sin * s[1],
                t[1] + sin * s[0] + cos * s[1],
            ];
            assert!(distance(*m, want) < 0.5, "{m:?} {want:?}");
        }
        let _ = expected;
    }


    #[test]
    fn mirrored_or_incomplete_sheets_are_reported() {
        let mut p = project();
        p.registration = Some(template(&p).unwrap().registration);
        let expected = p.registration.as_ref().unwrap().marks;
        let mirrored: Vec<[f64; 2]> = expected.iter().map(|m| [500.0 - m[0], m[1]]).collect();
        assert!(align(&p, &mirrored).err().unwrap().contains("gespiegelt"));
        assert!(align(&p, &expected[..2]).err().unwrap().contains('2'));
        let scaled: Vec<[f64; 2]> = expected.iter().map(|m| [m[0] * 0.9, m[1] * 0.9]).collect();
        assert!(align(&p, &scaled).is_err());
    }
}
