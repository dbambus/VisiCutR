//! Reststücke im Kamerabild erkennen: Umrisse von Materialresten auf dem
//! Arbeitsbett in Bett-Millimetern.
//!
//! Grundlage ist das entzerrte Kamerabild (`camera::background_image`), das
//! genau das Bett abdeckt. Es wird auf ein Arbeitsraster von höchstens
//! [`PX_PER_MM`] Pixeln je mm gemittelt. Material hebt sich entweder durch
//! seine Helligkeit vom Bett ab (Wabe oder Lamellen sind meist dunkel) oder,
//! zuverlässiger, durch den Farbabstand zu einem Foto des leeren Betts
//! (Referenz). Danach:
//!
//! 1. Weichzeichnen über die Glättung, damit die Wabenstruktur verschwindet.
//! 2. Schwellwert, ohne Vorgabe automatisch nach Otsu.
//! 3. Öffnen und Schließen mit einer Kreisscheibe (euklidische
//!    Distanztransformation): Stege, Krümel und schmale Spalte verschwinden.
//! 4. Sicherheitsabstand: die Fläche schrumpft um den Abstand nach innen.
//! 5. Konturverfolgung wie beim Vektorisieren, Vereinfachung, Umrechnung in mm.
//!
//! Löcher (schon ausgeschnittene Teile) bleiben als eigene Konturen erhalten.
use crate::geometry::Contour;
use crate::project::{Operation, Project};
use crate::vectorize::{simplify_closed, trace};
use image::RgbaImage;
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// Größte Auflösung des Arbeitsrasters (Pixel je mm).
pub const PX_PER_MM: f64 = 2.0;
/// Löcher unter dieser Fläche (mm²) werden ignoriert.
const MIN_HOLE_MM2: f64 = 20.0;
/// Farbe, mit der `camera::correct` Bereiche außerhalb des Kamerabilds füllt.
const OUTSIDE: [u8; 4] = [128, 128, 128, 255];

/// Wie sich das Material vom Bett unterscheidet (ohne Referenzbild).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Contrast {
    /// Material heller als das Bett (Holz, Pappe, helles Acryl auf Wabe).
    #[default]
    Lighter,
    /// Material dunkler als das Bett.
    Darker,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Options {
    pub contrast: Contrast,
    /// Schwellwert 0 … 255 auf dem geglätteten Signal; `None`: automatisch.
    pub threshold: Option<u8>,
    /// Glättung in mm: Weichzeichner und Öffnen/Schließen.
    pub smoothing_mm: f64,
    /// Abstand der Umrisse nach innen zur erkannten Kante in mm.
    pub margin_mm: f64,
    /// Kleinere Reststücke (mm²) werden verworfen.
    pub min_area_mm2: f64,
    /// Toleranz der Vereinfachung in mm.
    pub tolerance_mm: f64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            contrast: Contrast::Lighter,
            threshold: None,
            smoothing_mm: 3.0,
            margin_mm: 2.0,
            min_area_mm2: 400.0,
            tolerance_mm: 0.5,
        }
    }
}

/// Ein erkanntes Reststück in Bett-Millimetern (y nach unten).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Remnant {
    pub outline: Contour,
    pub holes: Vec<Contour>,
    /// Nutzbare Fläche (Umriss minus Löcher) in mm².
    pub area_mm2: f64,
}

impl Remnant {
    /// Umriss und Löcher.
    pub fn contours(&self) -> impl Iterator<Item = &Contour> {
        std::iter::once(&self.outline).chain(&self.holes)
    }

    /// Achsparalleles Rechteck `[x, y, w, h]` in mm.
    pub fn bounds(&self) -> [f64; 4] {
        let (mut min, mut max) = ([f64::MAX; 2], [f64::MIN; 2]);
        for p in &self.outline {
            for axis in 0..2 {
                min[axis] = min[axis].min(p[axis]);
                max[axis] = max[axis].max(p[axis]);
            }
        }
        [min[0], min[1], max[0] - min[0], max[1] - min[1]]
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Detection {
    /// Nach Fläche absteigend.
    pub remnants: Vec<Remnant>,
    /// Verwendeter Schwellwert (bei automatischer Wahl der berechnete).
    pub threshold: u8,
}

/// Arbeitsraster über dem Bett.
struct Grid {
    width: usize,
    height: usize,
    /// Pixel je mm.
    scale: f64,
}

/// Erkennt Reststücke im entzerrten Kamerabild `image`, das das Bett
/// `bed_mm` abdeckt. Mit `reference` (leeres Bett, gleich entzerrt) zählt der
/// Farbabstand zur Referenz, sonst die Helligkeit nach `options.contrast`.
pub fn detect(
    image: &RgbaImage,
    reference: Option<&RgbaImage>,
    bed_mm: [f64; 2],
    options: &Options,
) -> Result<Detection, String> {
    let [bed_w, bed_h] = bed_mm;
    if !(bed_w > 0.0 && bed_h > 0.0) || image.width() == 0 || image.height() == 0 {
        return Err(crate::i18n::tr("Kein Kamerabild für die Erkennung").into());
    }
    let scale = (image.width() as f64 / bed_w).min(PX_PER_MM);
    let grid = Grid {
        width: ((bed_w * scale).round() as usize).max(1),
        height: ((bed_h * scale).round() as usize).max(1),
        scale,
    };
    let pixels = resample(image, &grid);
    let signal: Vec<Option<f64>> = match reference.map(|r| resample(r, &grid)) {
        Some(background) => pixels
            .iter()
            .zip(&background)
            .map(|(p, b)| match (p, b) {
                (Some(p), Some(b)) => Some(
                    ((0..3).map(|c| (p[c] - b[c]).powi(2)).sum::<f64>() / 3.0)
                        .sqrt()
                        .min(255.0),
                ),
                _ => None,
            })
            .collect(),
        None => pixels
            .iter()
            .map(|p| {
                p.map(|[r, g, b]| {
                    let luma = 0.299 * r + 0.587 * g + 0.114 * b;
                    match options.contrast {
                        Contrast::Lighter => luma,
                        Contrast::Darker => 255.0 - luma,
                    }
                })
            })
            .collect(),
    };
    let radius = (options.smoothing_mm.max(0.0) * scale / 2.0).round() as usize;
    let blurred = blur(&signal, &grid, radius);
    let threshold = options.threshold.unwrap_or_else(|| otsu(&blurred));
    let mut mask: Vec<bool> = blurred
        .iter()
        .map(|v| v.is_some_and(|v| v > f64::from(threshold)))
        .collect();
    let morph = options.smoothing_mm.max(0.0) * scale / 2.0;
    if morph >= 0.5 {
        mask = dilate(&erode(&mask, &grid, morph), &grid, morph);
        mask = erode(&dilate(&mask, &grid, morph), &grid, morph);
    }
    let margin = options.margin_mm.max(0.0) * scale;
    if margin > 0.0 {
        mask = erode(&mask, &grid, margin);
    }
    Ok(Detection {
        remnants: remnants(&mask, &grid, options),
        threshold,
    })
}

/// Mittelt das Bild auf das Arbeitsraster (RGB 0 … 255).
fn resample(image: &RgbaImage, grid: &Grid) -> Vec<Option<[f64; 3]>> {
    let (iw, ih) = (image.width() as usize, image.height() as usize);
    let span = |i: usize, n: usize, total: usize| {
        let a = i * total / n;
        let b = ((i + 1) * total / n).max(a + 1).min(total);
        (a.min(total - 1), b)
    };
    let mut out = Vec::with_capacity(grid.width * grid.height);
    for y in 0..grid.height {
        let (y0, y1) = span(y, grid.height, ih);
        for x in 0..grid.width {
            let (x0, x1) = span(x, grid.width, iw);
            let (mut sum, mut count, mut outside) = ([0.0; 3], 0.0, 0);
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let p = image.get_pixel(sx as u32, sy as u32).0;
                    if p == OUTSIDE {
                        outside += 1;
                        continue;
                    }
                    for c in 0..3 {
                        sum[c] += f64::from(p[c]);
                    }
                    count += 1.0;
                }
            }
            out.push((count > 0.0 && count >= f64::from(outside)).then(|| sum.map(|s| s / count)));
        }
    }
    out
}

/// Quadratischer Mittelwertfilter mit Radius `r` über eine Summentabelle;
/// Pixel außerhalb des Kamerabilds zählen nicht mit.
fn blur(signal: &[Option<f64>], grid: &Grid, r: usize) -> Vec<Option<f64>> {
    if r == 0 {
        return signal.to_vec();
    }
    let (w, h) = (grid.width, grid.height);
    let stride = w + 1;
    let mut sum = vec![0.0f64; stride * (h + 1)];
    let mut count = vec![0u32; stride * (h + 1)];
    for y in 0..h {
        for x in 0..w {
            let (value, n) = signal[y * w + x].map_or((0.0, 0), |v| (v, 1));
            let i = (y + 1) * stride + x + 1;
            sum[i] = value + sum[i - 1] + sum[i - stride] - sum[i - stride - 1];
            count[i] = n + count[i - 1] + count[i - stride] - count[i - stride - 1];
        }
    }
    let mut out = Vec::with_capacity(w * h);
    for y in 0..h {
        let (y0, y1) = (y.saturating_sub(r), (y + r + 1).min(h));
        for x in 0..w {
            if signal[y * w + x].is_none() {
                out.push(None);
                continue;
            }
            let (x0, x1) = (x.saturating_sub(r), (x + r + 1).min(w));
            let at = |x: usize, y: usize| y * stride + x;
            let s = sum[at(x1, y1)] - sum[at(x0, y1)] - sum[at(x1, y0)] + sum[at(x0, y0)];
            let n = count[at(x1, y1)] + count[at(x0, y0)] - count[at(x0, y1)] - count[at(x1, y0)];
            out.push(Some(s / f64::from(n.max(1))));
        }
    }
    out
}

/// Schwellwert nach Otsu: trennt die Werte in zwei Klassen mit größter
/// Varianz zwischen den Klassen.
fn otsu(values: &[Option<f64>]) -> u8 {
    let mut histogram = [0u64; 256];
    for v in values.iter().flatten() {
        histogram[v.round().clamp(0.0, 255.0) as usize] += 1;
    }
    let total: u64 = histogram.iter().sum();
    if total == 0 {
        return 128;
    }
    let weighted: f64 = (0..256).map(|i| i as f64 * histogram[i] as f64).sum();
    let (mut below, mut below_sum) = (0u64, 0.0);
    let (mut best, mut best_variance) = (128u8, -1.0);
    for (t, &n) in histogram.iter().enumerate().take(255) {
        below += n;
        below_sum += t as f64 * n as f64;
        let above = total - below;
        if below == 0 || above == 0 {
            continue;
        }
        let mean_below = below_sum / below as f64;
        let mean_above = (weighted - below_sum) / above as f64;
        let variance = below as f64 * above as f64 * (mean_below - mean_above).powi(2);
        if variance > best_variance {
            best_variance = variance;
            best = t as u8;
        }
    }
    best
}

/// Quadrierter euklidischer Abstand jedes Pixels zum nächsten Pixel, für das
/// `source` gilt (Felzenszwalb & Huttenlocher). Ohne solches Pixel bleibt er
/// unendlich groß. Mit `border` liegt um das Raster ein Rand solcher Pixel.
fn distance_squared(mask: &[bool], grid: &Grid, source: bool, border: bool) -> Vec<f64> {
    const FAR: f64 = 1e20;
    let pad = usize::from(border);
    let (w, h) = (grid.width + 2 * pad, grid.height + 2 * pad);
    let mut d: Vec<f64> = (0..w * h)
        .map(|i| {
            let (x, y) = (i % w, i / w);
            let inside = x >= pad && y >= pad && x < w - pad && y < h - pad;
            let hit = if inside {
                mask[(y - pad) * grid.width + x - pad] == source
            } else {
                true
            };
            if hit { 0.0 } else { FAR }
        })
        .collect();
    let n = w.max(h);
    let (mut f, mut out, mut v, mut z) = (
        vec![0.0; n],
        vec![0.0; n],
        vec![0usize; n],
        vec![0.0; n + 1],
    );
    let mut pass = |d: &mut [f64], len: usize, index: &dyn Fn(usize) -> usize| {
        for q in 0..len {
            f[q] = d[index(q)];
        }
        transform_1d(&f[..len], &mut out[..len], &mut v, &mut z);
        for q in 0..len {
            d[index(q)] = out[q];
        }
    };
    for x in 0..w {
        pass(&mut d, h, &|y| y * w + x);
    }
    for y in 0..h {
        pass(&mut d, w, &|x| y * w + x);
    }
    if pad == 0 {
        return d;
    }
    (0..grid.height)
        .flat_map(|y| (0..grid.width).map(move |x| (y + 1) * w + x + 1))
        .map(|i| d[i])
        .collect()
}

/// Eindimensionale Distanztransformation über die untere Hülle der Parabeln.
fn transform_1d(f: &[f64], d: &mut [f64], v: &mut [usize], z: &mut [f64]) {
    let n = f.len();
    let mut k = 0;
    v[0] = 0;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    for q in 1..n {
        loop {
            let p = v[k];
            let s = ((f[q] + (q * q) as f64) - (f[p] + (p * p) as f64)) / (2.0 * (q - p) as f64);
            if s <= z[k] && k > 0 {
                k -= 1;
            } else {
                k += 1;
                v[k] = q;
                z[k] = s;
                z[k + 1] = f64::INFINITY;
                break;
            }
        }
    }
    k = 0;
    for (q, out) in d.iter_mut().enumerate().take(n) {
        while z[k + 1] < q as f64 {
            k += 1;
        }
        let p = v[k];
        *out = (q as f64 - p as f64).powi(2) + f[p];
    }
}

/// Erosion mit einer Kreisscheibe vom Radius `r` Pixel; außerhalb des Rasters
/// ist kein Material.
fn erode(mask: &[bool], grid: &Grid, r: f64) -> Vec<bool> {
    distance_squared(mask, grid, false, true)
        .into_iter()
        .map(|d| d > r * r)
        .collect()
}

fn dilate(mask: &[bool], grid: &Grid, r: f64) -> Vec<bool> {
    distance_squared(mask, grid, true, false)
        .into_iter()
        .map(|d| d <= r * r)
        .collect()
}

fn signed_area(ring: &[[f64; 2]]) -> f64 {
    let n = ring.len();
    (0..n)
        .map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        / 2.0
}

fn contains(ring: &[[f64; 2]], p: [f64; 2]) -> bool {
    let mut inside = false;
    let mut j = ring.len() - 1;
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[j]);
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Konturen der Maske als Reststücke in mm; Löcher gehören zum kleinsten
/// Umriss, der sie enthält.
fn remnants(mask: &[bool], grid: &Grid, options: &Options) -> Vec<Remnant> {
    let mm = |ring: Vec<[f64; 2]>| -> Contour {
        ring.into_iter()
            .map(|[x, y]| [x / grid.scale, y / grid.scale])
            .collect()
    };
    let tolerance = options.tolerance_mm.max(0.0) * grid.scale;
    let rings: Vec<(Vec<[f64; 2]>, f64)> = trace(mask, grid.width, grid.height)
        .into_iter()
        .map(|ring| {
            let area = signed_area(&ring);
            (ring, area)
        })
        .collect();
    // Umrisse und Löcher sind gegenläufig; der größte Ring ist ein Umriss.
    let Some(outer_sign) = rings
        .iter()
        .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
        .map(|(_, area)| area.signum())
    else {
        return Vec::new();
    };
    let px2 = grid.scale * grid.scale;
    let (outer, holes): (Vec<_>, Vec<_>) = rings
        .into_iter()
        .partition(|(_, area)| area.signum() == outer_sign);
    let mut result: Vec<(Vec<[f64; 2]>, Remnant)> = outer
        .into_iter()
        .filter(|(_, area)| area.abs() / px2 >= options.min_area_mm2.max(0.0))
        .map(|(ring, area)| {
            let simplified = simplify_closed(&ring, tolerance);
            let remnant = Remnant {
                outline: mm(simplified),
                holes: Vec::new(),
                area_mm2: area.abs() / px2,
            };
            (ring, remnant)
        })
        .collect();
    for (ring, area) in holes {
        if area.abs() / px2 < MIN_HOLE_MM2 {
            continue;
        }
        // Mittelpunkt einer Pixelkante des Lochs liegt sicher nicht auf dem Umriss.
        let probe = [
            (ring[0][0] + ring[1][0]) / 2.0,
            (ring[0][1] + ring[1][1]) / 2.0,
        ];
        if let Some((_, remnant)) = result
            .iter_mut()
            .filter(|(outline, _)| contains(outline, probe))
            .min_by(|a, b| a.1.area_mm2.total_cmp(&b.1.area_mm2))
        {
            remnant.area_mm2 -= area.abs() / px2;
            let hole = simplify_closed(&ring, tolerance);
            if hole.len() >= 3 {
                remnant.holes.push(mm(hole));
            }
        }
    }
    let mut remnants: Vec<Remnant> = result
        .into_iter()
        .map(|(_, r)| r)
        .filter(|r| r.outline.len() >= 3)
        .collect();
    remnants.sort_by(|a, b| b.area_mm2.total_cmp(&a.area_mm2));
    remnants
}

/// SVG über das ganze Bett mit den Umrissen als Haarlinien (Schneiden).
pub fn svg(remnants: &[Remnant], bed_mm: [f64; 2]) -> String {
    svg_area(remnants, [0.0, 0.0], bed_mm)
}

/// Gemeinsames Rechteck `[x, y, w, h]` aller Umrisse in mm.
fn union_bounds(remnants: &[Remnant]) -> [f64; 4] {
    let (mut min, mut max) = ([f64::MAX; 2], [f64::MIN; 2]);
    for [x, y, w, h] in remnants.iter().map(Remnant::bounds) {
        min = [min[0].min(x), min[1].min(y)];
        max = [max[0].max(x + w), max[1].max(y + h)];
    }
    [min[0], min[1], max[0] - min[0], max[1] - min[1]]
}

/// SVG des Bettausschnitts ab `origin` (mm) mit Größe `size`; die Umrisse
/// werden dorthin verschoben.
fn svg_area(remnants: &[Remnant], origin: [f64; 2], size: [f64; 2]) -> String {
    let bed_mm = size;
    let number = |v: f64| {
        let text = format!("{v:.3}");
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    };
    let mut svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}mm" height="{h}mm" viewBox="0 0 {w} {h}">"#,
        w = number(bed_mm[0]),
        h = number(bed_mm[1])
    );
    for (index, remnant) in remnants.iter().enumerate() {
        let mut d = String::new();
        for contour in remnant.contours() {
            for (i, p) in contour.iter().enumerate() {
                let command = if i == 0 { 'M' } else { 'L' };
                let _ = write!(
                    d,
                    "{command}{} {} ",
                    number(p[0] - origin[0]),
                    number(p[1] - origin[1])
                );
            }
            d.push_str("Z ");
        }
        let _ = write!(
            svg,
            r#"<path id="remnant{}" d="{}" fill="none" stroke="red" stroke-width="0.1"/>"#,
            index + 1,
            d.trim_end()
        );
    }
    svg.push_str("</svg>");
    svg
}

/// Fügt die Umrisse als Schneid-Teil an ihrer Stelle auf dem Bett ein; das
/// neue Teil wird aktiv.
pub fn add_part(project: &Project, remnants: &[Remnant]) -> Result<Project, String> {
    if remnants.is_empty() {
        return Err(crate::i18n::tr("Kein Reststück gefunden; Einstellungen anpassen").into());
    }
    // Nur das Rechteck um die Reststücke: ein bettgroßes Teil ließe für
    // später hinzugefügte Teile keinen freien Platz mehr.
    let [x, y, w, h] = union_bounds(remnants);
    let (mut result, _) = crate::compose::add_part(
        project,
        crate::i18n::tr("Reststücke"),
        &svg_area(remnants, [x, y], [w.max(0.1), h.max(0.1)]),
        Vec::new(),
    )?;
    // An der erkannten Stelle, nicht neben dem bisherigen Motiv.
    result.x_mm = x as f32;
    result.y_mm = y as f32;
    // Schneiden; Leistung und Geschwindigkeit setzt die Oberfläche aus dem
    // Materialprofil (`add_part` übernimmt sie vom bisherigen Teil).
    result.operation = Operation::Cut;
    Ok(result)
}

/// Ablage des Referenzbilds (leeres Bett) eines Lasercutters.
pub fn reference_path(config_dir: &Path, device: &str) -> PathBuf {
    let name: String = device
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    config_dir
        .join("camera-references")
        .join(format!("{name}.png"))
}

pub fn save_reference(path: &Path, image: &RgbaImage) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    image.save(path).map_err(|e| e.to_string())
}

/// Referenzbild, falls gespeichert.
pub fn load_reference(path: &Path) -> Option<RgbaImage> {
    image::open(path).ok().map(|i| i.to_rgba8())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    const BED: [f64; 2] = [200.0, 100.0];

    /// Dunkles Bett mit Wabenmuster, 2 px/mm, und hellen Rechtecken (mm).
    fn photo(pieces: &[[f64; 4]]) -> RgbaImage {
        RgbaImage::from_fn(400, 200, |x, y| {
            let (mx, my) = (x as f64 / 2.0, y as f64 / 2.0);
            if pieces
                .iter()
                .any(|[px, py, w, h]| mx >= *px && mx < px + w && my >= *py && my < py + h)
            {
                Rgba([200, 170, 120, 255])
            } else if x % 12 == 0 || y % 12 == 0 {
                Rgba([90, 90, 90, 255])
            } else {
                Rgba([30, 30, 30, 255])
            }
        })
    }

    fn close(a: f64, b: f64, tolerance: f64) -> bool {
        (a - b).abs() <= tolerance
    }

    #[test]
    fn finds_light_pieces_on_dark_honeycomb() {
        let image = photo(&[[10.0, 10.0, 80.0, 50.0], [120.0, 30.0, 60.0, 60.0]]);
        let options = Options {
            margin_mm: 0.0,
            ..Options::default()
        };
        let found = detect(&image, None, BED, &options).unwrap();
        assert_eq!(found.remnants.len(), 2, "{found:?}");
        let [x, y, w, h] = found.remnants[0].bounds();
        assert!(close(x, 10.0, 1.0) && close(y, 10.0, 1.0), "{x} {y}");
        assert!(close(w, 80.0, 1.5) && close(h, 50.0, 1.5), "{w} {h}");
        assert!(close(found.remnants[0].area_mm2, 4000.0, 200.0));
        let [x, y, ..] = found.remnants[1].bounds();
        assert!(close(x, 120.0, 1.0) && close(y, 30.0, 1.0));
        assert!(found.remnants.iter().all(|r| r.holes.is_empty()));
    }

    #[test]
    fn margin_shrinks_outline() {
        let image = photo(&[[20.0, 20.0, 100.0, 60.0]]);
        let options = Options {
            margin_mm: 5.0,
            ..Options::default()
        };
        let found = detect(&image, None, BED, &options).unwrap();
        assert_eq!(found.remnants.len(), 1);
        let [x, y, w, h] = found.remnants[0].bounds();
        assert!(close(x, 25.0, 1.0) && close(y, 25.0, 1.0), "{x} {y}");
        assert!(close(w, 90.0, 1.5) && close(h, 50.0, 1.5), "{w} {h}");
    }

    #[test]
    fn cut_out_hole_and_small_scraps() {
        // Platte mit ausgeschnittenem Loch und ein Krümel unter der Mindestfläche.
        let mut image = photo(&[[10.0, 10.0, 120.0, 80.0], [160.0, 10.0, 10.0, 10.0]]);
        for y in 60..120 {
            for x in 80..160 {
                image.put_pixel(x, y, Rgba([30, 30, 30, 255]));
            }
        }
        let options = Options {
            margin_mm: 0.0,
            ..Options::default()
        };
        let found = detect(&image, None, BED, &options).unwrap();
        assert_eq!(found.remnants.len(), 1, "{found:?}");
        let remnant = &found.remnants[0];
        assert_eq!(remnant.holes.len(), 1);
        assert!(
            close(remnant.area_mm2, 9600.0 - 1200.0, 300.0),
            "{}",
            remnant.area_mm2
        );
        let svg = svg(&found.remnants, BED);
        assert!(svg.starts_with(r#"<svg xmlns="http://www.w3.org/2000/svg" width="200mm""#));
        assert_eq!(svg.matches('M').count(), 2);
    }

    #[test]
    fn reference_difference_finds_piece_of_bed_colour_brightness() {
        // Graues Stück auf grauem Bett: gleiche Helligkeit, andere Farbe.
        let bed = RgbaImage::from_pixel(400, 200, Rgba([120, 120, 120, 255]));
        let mut image = bed.clone();
        for y in 40..140 {
            for x in 100..300 {
                image.put_pixel(x, y, Rgba([160, 115, 70, 255]));
            }
        }
        let options = Options {
            margin_mm: 0.0,
            ..Options::default()
        };
        let found = detect(&image, Some(&bed), BED, &options).unwrap();
        assert_eq!(found.remnants.len(), 1);
        let [x, y, w, h] = found.remnants[0].bounds();
        assert!(
            close(x, 50.0, 1.0)
                && close(y, 20.0, 1.0)
                && close(w, 100.0, 1.5)
                && close(h, 50.0, 1.5)
        );
        // Ohne Unterschied zur Referenz: nichts.
        let empty = detect(&bed, Some(&bed), BED, &Options::default()).unwrap();
        assert!(empty.remnants.is_empty());
    }

    #[test]
    fn darker_material_and_area_outside_camera() {
        let mut image = RgbaImage::from_pixel(400, 200, Rgba([220, 220, 220, 255]));
        for y in 20..100 {
            for x in 20..120 {
                image.put_pixel(x, y, Rgba([40, 40, 40, 255]));
            }
        }
        // Rechter Rand außerhalb des Kamerabilds.
        for y in 0..200 {
            for x in 350..400 {
                image.put_pixel(x, y, Rgba(OUTSIDE));
            }
        }
        let options = Options {
            contrast: Contrast::Darker,
            margin_mm: 0.0,
            ..Options::default()
        };
        let found = detect(&image, None, BED, &options).unwrap();
        assert_eq!(found.remnants.len(), 1, "{found:?}");
        let [x, y, w, h] = found.remnants[0].bounds();
        assert!(
            close(x, 10.0, 1.0)
                && close(y, 10.0, 1.0)
                && close(w, 50.0, 1.5)
                && close(h, 40.0, 1.5)
        );
    }

    #[test]
    fn otsu_splits_two_levels() {
        let values: Vec<Option<f64>> = (0..100)
            .map(|i| Some(if i < 60 { 40.0 } else { 200.0 }))
            .collect();
        let t = otsu(&values);
        assert!((40..200).contains(&t));
    }

    #[test]
    fn remnants_become_a_cut_part_in_place() {
        let mut project = Project::default();
        (project.bed_width_mm, project.bed_height_mm) = (200.0, 100.0);
        let found = detect(
            &photo(&[[20.0, 30.0, 60.0, 40.0]]),
            None,
            BED,
            &Options::default(),
        )
        .unwrap();
        let result = add_part(&project, &found.remnants).unwrap();
        assert_eq!(result.part_count(), 1);
        // Das Teil umfasst nur das Reststück und liegt an dessen Stelle.
        let [x, y, w, h] = found.remnants[0].bounds();
        assert!((result.x_mm as f64 - x).abs() < 0.01 && (result.y_mm as f64 - y).abs() < 0.01);
        assert!(
            (result.width_mm as f64 - w).abs() < 0.01 && (result.height_mm as f64 - h).abs() < 0.01
        );
        assert!(
            (20.0..25.0).contains(&x) && (30.0..35.0).contains(&y),
            "{x} {y}"
        );
        // Daneben ist noch Platz für weitere Teile.
        let small = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10mm" height="10mm" viewBox="0 0 10 10"><path d="M0 0H10" stroke="red"/></svg>"#;
        let (_, notes) = crate::compose::add_part(&result, "klein", small, Vec::new()).unwrap();
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!(result.operation, Operation::Cut);
        assert!(result.svg.contains("remnant1"));
        assert!(add_part(&project, &[]).is_err());
    }

    #[test]
    fn reference_round_trip() {
        let dir = std::env::temp_dir().join(format!("visicut-remnant-{}", std::process::id()));
        let path = reference_path(&dir, "LTT iLaser/4000");
        assert!(path.ends_with("camera-references/LTT_iLaser_4000.png"));
        let image = RgbaImage::from_pixel(3, 2, Rgba([1, 2, 3, 255]));
        save_reference(&path, &image).unwrap();
        assert_eq!(load_reference(&path).unwrap(), image);
        let _ = std::fs::remove_dir_all(dir);
    }
}
