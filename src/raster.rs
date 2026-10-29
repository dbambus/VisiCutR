//! Engraving rasters: greyscale conversion and dithering, ported from
//! LibLaserCut (utils/BufferedImageAdapter.java, dithering/*.java),
//! LGPL-3.0-or-later. Original authors: Thomas Oster, Max Gaukler.
use crate::{i18n::tr, tr};
use resvg::tiny_skia;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Dithering {
    /// Fixed luminance threshold 128 (VisiCutR ≤ 0.3).
    Threshold,
    FloydSteinberg,
    Average,
    Random,
    Ordered,
    Grid,
    Halftone,
    BrightenedHalftone,
}

impl Dithering {
    pub const ALL: [Dithering; 8] = [
        Self::BrightenedHalftone,
        Self::FloydSteinberg,
        Self::Halftone,
        Self::Ordered,
        Self::Average,
        Self::Grid,
        Self::Random,
        Self::Threshold,
    ];
    pub fn title(self) -> &'static str {
        match self {
            Self::Threshold => tr("Schwellwert 50 %"),
            Self::FloydSteinberg => "Floyd-Steinberg",
            Self::Average => tr("Mittelwert"),
            Self::Random => tr("Zufall"),
            Self::Ordered => tr("Geordnet"),
            Self::Grid => tr("Raster"),
            Self::Halftone => tr("Halbton"),
            Self::BrightenedHalftone => tr("Halbton aufgehellt"),
        }
    }
}

/// Raster options of an engraving step, as in VisiCut's raster profiles and
/// LaosEngraveProperty.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RasterSettings {
    pub dithering: Dithering,
    pub invert: bool,
    /// Added to the grey value (−255 … 255); positive values engrave lighter.
    pub color_shift: i32,
    pub bidirectional: bool,
    pub bottom_up: bool,
    /// Engraving resolution, one of [`RASTER_DPIS`]; older files use 500.
    pub dpi: u32,
    /// Stamp shoulder in mm (0 = off), like "Stempel/Offset" of the LTT
    /// Windows driver: engraved pixels near unengraved ones get less power,
    /// so raised parts keep sloped flanks. Not in Java VisiCut.
    pub shoulder_mm: f32,
}

/// Widest stamp shoulder; the LTT driver allows 14 pixels (0.7 mm at 500 DPI).
pub const MAX_SHOULDER_MM: f32 = 3.0;

/// Engraving resolutions of the iLaser 4000 (the Windows driver's list). The
/// header command `ESC D` sends 4000 / DPI as divisor of the 4000-DPI machine
/// resolution and applies to raster lines only; 1000 DPI is the maximum.
pub const RASTER_DPIS: [u32; 6] = [166, 200, 250, 333, 500, 1000];

/// New steps follow the FAU "engrave" profile.
impl Default for RasterSettings {
    fn default() -> Self {
        Self {
            dithering: Dithering::BrightenedHalftone,
            invert: false,
            color_shift: 0,
            bidirectional: true,
            bottom_up: false,
            dpi: 500,
            shoulder_mm: 0.0,
        }
    }
}

impl RasterSettings {
    /// Behaviour of projects saved before raster options existed.
    pub fn legacy() -> Self {
        Self {
            dithering: Dithering::Threshold,
            bidirectional: false,
            ..Self::default()
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if !(-255..=255).contains(&self.color_shift) {
            return Err(tr("Helligkeitsverschiebung muss zwischen −255 und 255 liegen").into());
        }
        if !RASTER_DPIS.contains(&self.dpi) {
            return Err(tr!(
                "Gravurauflösung {dpi} DPI wird nicht unterstützt (166, 200, 250, 333, 500 oder 1000 DPI)",
                dpi = self.dpi
            ));
        }
        if !(0.0..=MAX_SHOULDER_MM).contains(&self.shoulder_mm) {
            return Err(tr("Stempel-Schulter muss zwischen 0 und 3 mm liegen").into());
        }
        Ok(())
    }

    /// `ESC D` divisor: 4000 / DPI (24, 20, 16, 12, 8 or 4).
    pub fn pitch(&self) -> i32 {
        (4000.0 / f64::from(self.dpi)).round() as i32
    }
}

/// Grey values 0 (black) … 255 (white) as in BufferedImageAdapter.getGreyScale.
pub fn greyscale(pixmap: &tiny_skia::Pixmap, settings: &RasterSettings) -> Vec<u8> {
    pixmap
        .data()
        .chunks_exact(4)
        .map(|p| {
            let value = if settings.dithering == Dithering::Threshold {
                // Legacy luminance, so old projects keep their exact output.
                ((p[0] as u32 * 2126 + p[1] as u32 * 7152 + p[2] as u32 * 722) / 10000) as i32
            } else {
                (0.3 * p[0] as f64 + 0.59 * p[1] as f64 + 0.11 * p[2] as f64) as i32
            };
            let value = (value + settings.color_shift).clamp(0, 255) as u8;
            if settings.invert { 255 - value } else { value }
        })
        .collect()
}

/// Stamp shoulder: engraved pixels (grey < 128) closer than `pixels` to an
/// unengraved one get power rising linearly with the distance, so a raised
/// part gets a sloped flank instead of a vertical one. The LTT driver erodes
/// in 3 × 3 steps; this uses the chamfer distance (1, √2), which keeps round
/// shapes round. Outside the image counts as engraved.
pub fn shoulder(grey: &mut [u8], width: usize, height: usize, pixels: f32) {
    if pixels <= 0.0 || width == 0 {
        return;
    }
    let mut distance: Vec<f32> = grey
        .iter()
        .map(|g| if *g < 128 { f32::INFINITY } else { 0.0 })
        .collect();
    let diagonal = std::f32::consts::SQRT_2;
    for y in 0..height {
        for x in 0..width {
            let mut d = distance[y * width + x];
            if x > 0 {
                d = d.min(distance[y * width + x - 1] + 1.0);
            }
            if y > 0 {
                let up = (y - 1) * width + x;
                d = d.min(distance[up] + 1.0);
                if x > 0 {
                    d = d.min(distance[up - 1] + diagonal);
                }
                if x + 1 < width {
                    d = d.min(distance[up + 1] + diagonal);
                }
            }
            distance[y * width + x] = d;
        }
    }
    for y in (0..height).rev() {
        for x in (0..width).rev() {
            let mut d = distance[y * width + x];
            if x + 1 < width {
                d = d.min(distance[y * width + x + 1] + 1.0);
            }
            if y + 1 < height {
                let down = (y + 1) * width + x;
                d = d.min(distance[down] + 1.0);
                if x + 1 < width {
                    d = d.min(distance[down + 1] + diagonal);
                }
                if x > 0 {
                    d = d.min(distance[down - 1] + diagonal);
                }
            }
            distance[y * width + x] = d;
        }
    }
    for (g, d) in grey.iter_mut().zip(distance) {
        // The pixel next to the edge (distance 1) gets 1 / (pixels + 1).
        let factor = d / (pixels + 1.0);
        if factor < 1.0 {
            let power = (255.0 - f32::from(*g)) * factor;
            *g = 255 - power.round() as u8;
        }
    }
}

/// Grey values with the step's stamp shoulder applied.
fn shaped_greyscale(pixmap: &tiny_skia::Pixmap, settings: &RasterSettings) -> Vec<u8> {
    let mut grey = greyscale(pixmap, settings);
    let pixels = settings.shoulder_mm * settings.dpi as f32 / 25.4;
    shoulder(
        &mut grey,
        pixmap.width() as usize,
        pixmap.height() as usize,
        pixels,
    );
    grey
}

/// Black (engraved) pixels, row-major.
pub fn dither(grey: &[u8], width: usize, height: usize, algorithm: Dithering) -> Vec<bool> {
    let at = |x: usize, y: usize| grey[y * width + x] as i32;
    match algorithm {
        Dithering::Threshold => grey.iter().map(|g| *g < 128).collect(),
        Dithering::Average | Dithering::Grid => {
            let total: u64 = grey.iter().map(|g| *g as u64).sum();
            let threshold = (total / height.max(1) as u64 / width.max(1) as u64) as i32;
            let (size, gap) = (10, 5);
            (0..width * height)
                .map(|i| {
                    let (x, y) = (i % width, i / width);
                    let inside = algorithm == Dithering::Average
                        || (y % (size + gap) <= size && x % (size + gap) <= size);
                    inside && at(x, y) < threshold
                })
                .collect()
        }
        Dithering::Random => {
            // Seeded so that preview and job are identical; LibLaserCut uses
            // an unseeded generator.
            let mut state = 0x2545_f491_4f6c_dd1du64;
            grey.iter()
                .map(|g| {
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    (*g as u64) < state % 256
                })
                .collect()
        }
        Dithering::Ordered | Dithering::Halftone | Dithering::BrightenedHalftone => {
            let matrix = threshold_matrix(algorithm);
            let n = matrix.len();
            (0..width * height)
                .map(|i| {
                    let (x, y) = (i % width, i / width);
                    at(x, y) < matrix[x % n][y % n]
                })
                .collect()
        }
        Dithering::FloydSteinberg => {
            let mut black = vec![false; width * height];
            let mut current: Vec<i32> = (0..width).map(|x| at(x, 0)).collect();
            let mut next = vec![0; width];
            for y in 0..height {
                if y + 1 < height {
                    next.iter_mut()
                        .enumerate()
                        .for_each(|(x, v)| *v = at(x, y + 1));
                }
                for x in 0..width {
                    let is_black = current[x] <= 127;
                    black[y * width + x] = is_black;
                    // Integer division truncates toward zero, as in Java.
                    let error = current[x] - if is_black { 0 } else { 255 };
                    if x + 1 < width {
                        current[x + 1] += 7 * error / 16;
                        if y + 1 < height {
                            next[x + 1] += error / 16;
                        }
                    }
                    if y + 1 < height {
                        next[x] += 5 * error / 16;
                        if x > 0 {
                            next[x - 1] += 3 * error / 16;
                        }
                    }
                }
                std::mem::swap(&mut current, &mut next);
            }
            black
        }
    }
}

fn threshold_matrix(algorithm: Dithering) -> Vec<Vec<i32>> {
    if algorithm == Dithering::Ordered {
        // LibLaserCut uses 256 at [3][0], which engraves dots on pure white;
        // 255 keeps white untouched.
        return vec![
            vec![16, 144, 48, 176],
            vec![208, 80, 240, 112],
            vec![64, 192, 32, 160],
            vec![255, 128, 224, 96],
        ];
    }
    // 8×8 clustered dot matrix from http://caca.zoy.org/study/part2.html.
    let base = [
        [24, 10, 12, 26, 35, 47, 49, 37],
        [8, 0, 2, 14, 45, 59, 61, 51],
        [22, 6, 4, 16, 43, 57, 63, 53],
        [30, 20, 18, 28, 33, 41, 55, 39],
        [34, 46, 48, 36, 25, 11, 13, 27],
        [44, 58, 60, 50, 9, 1, 3, 15],
        [42, 56, 62, 52, 23, 7, 5, 17],
        [32, 40, 54, 38, 31, 21, 19, 29],
    ];
    base.iter()
        .map(|row| {
            row.iter()
                .map(|v| {
                    let t = (1 + v) * 256 / 65;
                    if algorithm == Dithering::BrightenedHalftone {
                        brighten(t)
                    } else {
                        t
                    }
                })
                .collect()
        })
        .collect()
}

/// Inverse brightness curve applied to the threshold matrix (BrightenedHalftone).
fn brighten(value: i32) -> i32 {
    let output = [0.0, 192.0, 250.0, 255.0];
    let input = [0.0, 64.0, 230.0, 255.0];
    let v = value as f64;
    let mapped = if v < output[0] {
        input[0]
    } else if v > output[3] {
        input[3]
    } else {
        let i = (1..4)
            .find(|i| output[i - 1] <= v && v <= output[*i])
            .unwrap();
        (v - output[i - 1]) / (output[i] - output[i - 1]) * (input[i] - input[i - 1]) + input[i - 1]
    };
    mapped.round() as i32
}

/// One engraving line: packed bits (MSB first) for normal engraving, or one
/// power byte per pixel (0 = off, 255 = full) for 3D engraving.
pub struct Raster {
    pub width: usize,
    pub height: usize,
    pub bits_per_pixel: u8,
    pub rows: Vec<Vec<u8>>,
}

impl Raster {
    pub fn engrave(pixmap: &tiny_skia::Pixmap, settings: &RasterSettings) -> Self {
        let (width, height) = (pixmap.width() as usize, pixmap.height() as usize);
        let black = dither(
            &shaped_greyscale(pixmap, settings),
            width,
            height,
            settings.dithering,
        );
        let rows = (0..height)
            .map(|y| {
                let mut row = vec![0u8; width.div_ceil(8)];
                for x in 0..width {
                    if black[y * width + x] {
                        row[x / 8] |= 0x80 >> (x % 8);
                    }
                }
                row
            })
            .collect();
        Self {
            width,
            height,
            bits_per_pixel: 1,
            rows,
        }
    }

    /// 3D engraving: laser power follows darkness (Raster3dPart).
    pub fn engrave_3d(pixmap: &tiny_skia::Pixmap, settings: &RasterSettings) -> Self {
        let (width, height) = (pixmap.width() as usize, pixmap.height() as usize);
        let grey = shaped_greyscale(pixmap, settings);
        let rows = grey
            .chunks(width)
            .map(|r| r.iter().map(|g| 255 - g).collect())
            .collect();
        Self {
            width,
            height,
            bits_per_pixel: 8,
            rows,
        }
    }

    /// Engraving intensity 0 … 255 of a pixel, for previews.
    pub fn intensity(&self, x: usize, y: usize) -> u8 {
        if self.bits_per_pixel == 8 {
            self.rows[y][x]
        } else if self.rows[y][x / 8] & (0x80 >> (x % 8)) != 0 {
            255
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_without_resolution_engrave_at_500_dpi() {
        let mut value = serde_json::to_value(RasterSettings::legacy()).unwrap();
        value.as_object_mut().unwrap().remove("dpi");
        let settings: RasterSettings = serde_json::from_value(value).unwrap();
        assert_eq!((settings.dpi, settings.pitch()), (500, 8));
        assert!(settings.validate().is_ok());
        let pitches: Vec<i32> = RASTER_DPIS
            .iter()
            .map(|&dpi| RasterSettings { dpi, ..settings }.pitch())
            .collect();
        assert_eq!(pitches, vec![24, 20, 16, 12, 8, 4]);
    }

    fn gradient(width: usize) -> Vec<u8> {
        (0..width * 16)
            .map(|i| ((i % width) * 255 / (width - 1)) as u8)
            .collect()
    }

    #[test]
    fn halftone_tables_match_liblasercut() {
        assert_eq!(threshold_matrix(Dithering::Halftone)[1][1], 3);
        assert_eq!(threshold_matrix(Dithering::Halftone)[2][6], 252);
        // Brightened: the inverse curve maps the knots 192 → 64, 250 → 230.
        assert_eq!(brighten(0), 0);
        assert_eq!(brighten(192), 64);
        assert_eq!(brighten(250), 230);
        assert_eq!(brighten(255), 255);
    }

    #[test]
    fn all_algorithms_keep_black_and_white_and_follow_gradient_density() {
        let width = 64;
        let grey = gradient(width);
        for algorithm in Dithering::ALL {
            let black = dither(&grey, width, 16, algorithm);
            let dark: usize = (0..16)
                .map(|y| {
                    black[y * width..y * width + 8]
                        .iter()
                        .filter(|b| **b)
                        .count()
                })
                .sum();
            let light: usize = (0..16)
                .map(|y| {
                    black[y * width + 56..y * width + 64]
                        .iter()
                        .filter(|b| **b)
                        .count()
                })
                .sum();
            assert!(dark > light, "{algorithm:?}: {dark} ≤ {light}");
            let solid = dither(&[0; 64], 8, 8, algorithm);
            // Average and Grid compare with the image mean, so solid black stays
            // unengraved; Random spares black with probability 1/256 (both as
            // in LibLaserCut).
            if !matches!(
                algorithm,
                Dithering::Grid | Dithering::Average | Dithering::Random
            ) {
                assert!(solid.iter().all(|b| *b), "{algorithm:?} black");
            }
            assert!(
                dither(&[255; 64], 8, 8, algorithm).iter().all(|b| !*b),
                "{algorithm:?} white"
            );
        }
    }

    #[test]
    fn floyd_steinberg_matches_reference_values() {
        // 50 % grey alternates in a checkerboard-like pattern.
        let black = dither(&[128, 128, 128, 128], 4, 1, Dithering::FloydSteinberg);
        assert_eq!(black, [false, true, false, true]);
    }

    #[test]
    fn greyscale_applies_shift_and_inversion_and_3d_power() {
        let mut pixmap = tiny_skia::Pixmap::new(2, 1).unwrap();
        pixmap
            .data_mut()
            .copy_from_slice(&[0, 0, 0, 255, 200, 200, 200, 255]);
        let settings = RasterSettings {
            color_shift: 20,
            ..Default::default()
        };
        assert_eq!(greyscale(&pixmap, &settings), [20, 220]);
        let inverted = RasterSettings {
            invert: true,
            ..Default::default()
        };
        assert_eq!(greyscale(&pixmap, &inverted), [255, 55]);
        let raster = Raster::engrave_3d(&pixmap, &RasterSettings::default());
        assert_eq!(raster.rows, [vec![255, 55]]);
        assert_eq!(raster.intensity(1, 0), 55);
        let bw = Raster::engrave(&pixmap, &RasterSettings::legacy());
        assert_eq!(bw.rows, [vec![0x80]]);
    }

    #[test]
    fn stamp_shoulder_ramps_power_away_from_raised_parts() {
        // Black (engraved) row with one white (raised) pixel in the middle.
        let mut grey = vec![0u8; 11];
        grey[5] = 255;
        shoulder(&mut grey, 11, 1, 3.0);
        // Power 255 · d / 4 next to the raised pixel (grey = 255 − power),
        // full from distance 4; the image border counts as engraved and stays
        // at full power.
        assert_eq!(grey, [0, 0, 64, 127, 191, 255, 191, 127, 64, 0, 0]);
        let mut untouched = vec![0u8, 255, 0];
        shoulder(&mut untouched, 3, 1, 0.0);
        assert_eq!(untouched, [0, 255, 0]);
        // Diagonal neighbours are √2 away, not 1 as in a 3 × 3 erosion.
        let mut square = vec![0u8; 9];
        square[4] = 255;
        shoulder(&mut square, 3, 3, 3.0);
        assert_eq!((square[1], square[0]), (191, 165));
    }

    #[test]
    fn stamp_shoulder_follows_the_engraving_resolution() {
        let mut pixmap = tiny_skia::Pixmap::new(9, 1).unwrap();
        for (i, p) in pixmap.data_mut().chunks_exact_mut(4).enumerate() {
            let v = if i == 0 { 255 } else { 0 };
            p.copy_from_slice(&[v, v, v, 255]);
        }
        // 0.1524 mm at 500 DPI are 3 pixels.
        let settings = RasterSettings {
            shoulder_mm: 0.1524,
            ..Default::default()
        };
        let raster = Raster::engrave_3d(&pixmap, &settings);
        assert_eq!(raster.rows[0][..6], [0, 64, 128, 191, 255, 255]);
        assert!(settings.validate().is_ok());
        let too_wide = RasterSettings {
            shoulder_mm: MAX_SHOULDER_MM + 0.1,
            ..settings
        };
        assert!(too_wide.validate().is_err());
        // Older files without the field have no shoulder.
        let mut value = serde_json::to_value(RasterSettings::default()).unwrap();
        value.as_object_mut().unwrap().remove("shoulder_mm");
        let old: RasterSettings = serde_json::from_value(value).unwrap();
        assert_eq!(old.shoulder_mm, 0.0);
    }
}
