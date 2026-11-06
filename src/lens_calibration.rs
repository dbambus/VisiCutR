//! ABGESCHALTET (nicht in `lib.rs` eingebunden): Auf der FAU-Laserkamera
//! erhöhte die Objektivkalibrierung aus `calibration.npz` nur den Fehler
//! (≈ 0,3 mm → ≈ 1,1 mm); die Kreuze der Kamerakalibrierung bestimmen die
//! Verzeichnung genauer.
//!
//! Lens calibration from chessboard photos, as OpenCV `findChessboardCorners`,
//! `cornerSubPix` and `calibrateCamera` with default flags do it in
//! fau-fablab/undistort-webcam: pinhole camera (fx, fy, cx, cy, no skew) with
//! the distortion coefficients k1, k2, p1, p2, k3. The result is written as
//! that tool's `calibration.npz`.
use crate::camera::{self, Matrix};
use crate::device::CameraCalibration;
use image::RgbaImage;
use std::io::{Cursor, Read, Write};

/// Inner corners of the chessboard printed by undistort-webcam.
pub const PATTERN: (usize, usize) = (9, 6);

/// Intrinsics and distortion in OpenCV's order and meaning.
#[derive(Clone, Debug, PartialEq)]
pub struct Intrinsics {
    pub fx: f64,
    pub fy: f64,
    pub cx: f64,
    pub cy: f64,
    /// k1, k2, p1, p2, k3.
    pub dist: [f64; 5],
}

impl Intrinsics {
    pub fn camera_matrix(&self) -> Matrix {
        [
            [self.fx, 0.0, self.cx],
            [0.0, self.fy, self.cy],
            [0.0, 0.0, 1.0],
        ]
    }

    /// Distortion of normalised pinhole coordinates.
    pub fn distort_normalised(&self, x: f64, y: f64) -> [f64; 2] {
        let [k1, k2, p1, p2, k3] = self.dist;
        let r2 = x * x + y * y;
        let radial = 1.0 + r2 * (k1 + r2 * (k2 + r2 * k3));
        [
            x * radial + 2.0 * p1 * x * y + p2 * (r2 + 2.0 * x * x),
            y * radial + p1 * (r2 + 2.0 * y * y) + 2.0 * p2 * x * y,
        ]
    }

    /// Camera pixel showing the ideal pixel `u` (same camera matrix).
    pub fn distort(&self, u: [f64; 2]) -> [f64; 2] {
        let [x, y] =
            self.distort_normalised((u[0] - self.cx) / self.fx, (u[1] - self.cy) / self.fy);
        [self.cx + self.fx * x, self.cy + self.fy * y]
    }

    /// Ideal pixel of the camera pixel `p`, iterated as `cv::undistortPoints`.
    pub fn undistort(&self, p: [f64; 2]) -> [f64; 2] {
        let xd = (p[0] - self.cx) / self.fx;
        let yd = (p[1] - self.cy) / self.fy;
        let [k1, k2, p1, p2, k3] = self.dist;
        let (mut x, mut y) = (xd, yd);
        for _ in 0..20 {
            let r2 = x * x + y * y;
            let radial = 1.0 + r2 * (k1 + r2 * (k2 + r2 * k3));
            let dx = 2.0 * p1 * x * y + p2 * (r2 + 2.0 * x * x);
            let dy = p1 * (r2 + 2.0 * y * y) + 2.0 * p2 * x * y;
            if radial.abs() < 1e-9 {
                break;
            }
            (x, y) = ((xd - dx) / radial, (yd - dy) / radial);
        }
        [self.cx + self.fx * x, self.cy + self.fy * y]
    }
}

/// Result of [`calibrate`].
#[derive(Clone, Debug)]
pub struct Calibration {
    pub intrinsics: Intrinsics,
    pub image_size: [u32; 2],
    /// Pose of each board (Rodrigues vector, translation in squares).
    pub rvecs: Vec<[f64; 3]>,
    pub tvecs: Vec<[f64; 3]>,
    /// Reprojection error over all corners as OpenCV reports it (px).
    pub rms: f64,
    /// Reprojection error of each photo (px).
    pub view_rms: Vec<f64>,
}

// ---------------------------------------------------------------------------
// Chessboard detection

/// Grey values of a picture, row by row.
struct Plane {
    width: usize,
    height: usize,
    values: Vec<f32>,
}

impl Plane {
    fn from_image(image: &RgbaImage) -> Self {
        Self {
            width: image.width() as usize,
            height: image.height() as usize,
            values: image
                .pixels()
                .map(|p| 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32)
                .collect(),
        }
    }

    fn at(&self, x: usize, y: usize) -> f32 {
        self.values[y * self.width + x]
    }

    /// Bilinear value, clamped to the picture.
    fn sample(&self, x: f64, y: f64) -> f32 {
        let x = x.clamp(0.0, (self.width - 1) as f64);
        let y = y.clamp(0.0, (self.height - 1) as f64);
        let (x0, y0) = (x as usize, y as usize);
        let (x1, y1) = ((x0 + 1).min(self.width - 1), (y0 + 1).min(self.height - 1));
        let (fx, fy) = ((x - x0 as f64) as f32, (y - y0 as f64) as f32);
        let top = self.at(x0, y0) * (1.0 - fx) + self.at(x1, y0) * fx;
        let bottom = self.at(x0, y1) * (1.0 - fx) + self.at(x1, y1) * fx;
        top * (1.0 - fy) + bottom * fy
    }

    /// Mean of `factor × factor` blocks.
    fn shrink(&self, factor: usize) -> Self {
        let (width, height) = (self.width / factor, self.height / factor);
        let mut values = vec![0.0; width * height];
        for y in 0..height {
            for x in 0..width {
                let mut sum = 0.0;
                for dy in 0..factor {
                    let row = (y * factor + dy) * self.width + x * factor;
                    sum += self.values[row..row + factor].iter().sum::<f32>();
                }
                values[y * width + x] = sum / (factor * factor) as f32;
            }
        }
        Self {
            width,
            height,
            values,
        }
    }

    /// Separable Gaussian blur.
    fn blur(&self, sigma: f32) -> Self {
        let radius = (3.0 * sigma).ceil() as isize;
        let kernel: Vec<f32> = (-radius..=radius)
            .map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp())
            .collect();
        let total: f32 = kernel.iter().sum();
        let kernel: Vec<f32> = kernel.iter().map(|k| k / total).collect();
        let (w, h) = (self.width as isize, self.height as isize);
        let pass = |source: &[f32], horizontal: bool| {
            let mut out = vec![0.0; source.len()];
            for y in 0..h {
                for x in 0..w {
                    let mut sum = 0.0;
                    for (k, weight) in kernel.iter().enumerate() {
                        let o = k as isize - radius;
                        let (sx, sy) = if horizontal {
                            ((x + o).clamp(0, w - 1), y)
                        } else {
                            (x, (y + o).clamp(0, h - 1))
                        };
                        sum += weight * source[(sy * w + sx) as usize];
                    }
                    out[(y * w + x) as usize] = sum;
                }
            }
            out
        };
        let values = pass(&pass(&self.values, true), false);
        Self {
            width: self.width,
            height: self.height,
            values,
        }
    }
}

/// Saddle points (corners where four squares meet), strongest first, in
/// pixels of `plane`.
fn saddles(plane: &Plane) -> Vec<([f64; 2], f32)> {
    let (w, h) = (plane.width, plane.height);
    let mut response = vec![0.0f32; w * h];
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let c = plane.at(x, y);
            let ixx = plane.at(x + 1, y) - 2.0 * c + plane.at(x - 1, y);
            let iyy = plane.at(x, y + 1) - 2.0 * c + plane.at(x, y - 1);
            let ixy = (plane.at(x + 1, y + 1) - plane.at(x + 1, y - 1) - plane.at(x - 1, y + 1)
                + plane.at(x - 1, y - 1))
                / 4.0;
            response[y * w + x] = ixy * ixy - ixx * iyy;
        }
    }
    let strongest = response.iter().cloned().fold(0.0f32, f32::max);
    let threshold = strongest * 0.01;
    let mut found = Vec::new();
    const NMS: usize = 3;
    for y in NMS..h.saturating_sub(NMS) {
        for x in NMS..w.saturating_sub(NMS) {
            let r = response[y * w + x];
            if r <= threshold {
                continue;
            }
            let peak = (y - NMS..=y + NMS)
                .all(|yy| (x - NMS..=x + NMS).all(|xx| response[yy * w + xx] <= r));
            if peak && has_four_sectors(plane, x as f64, y as f64, 4.0) {
                found.push(([x as f64, y as f64], r));
            }
        }
    }
    found.sort_by(|a, b| b.1.total_cmp(&a.1));
    found.truncate(6000);
    found
}

/// Points sorted into square cells for neighbour searches.
struct Buckets {
    size: f64,
    cells: std::collections::HashMap<(i64, i64), Vec<usize>>,
}

impl Buckets {
    fn new(points: &[([f64; 2], f32)], size: f64) -> Self {
        let mut cells: std::collections::HashMap<(i64, i64), Vec<usize>> = Default::default();
        for (i, (p, _)) in points.iter().enumerate() {
            cells
                .entry(((p[0] / size) as i64, (p[1] / size) as i64))
                .or_default()
                .push(i);
        }
        Self { size, cells }
    }

    /// Indices of the points within `radius` of `p` (and maybe a few more).
    fn around(&self, p: [f64; 2], radius: f64) -> impl Iterator<Item = usize> + '_ {
        let (x0, x1) = (
            ((p[0] - radius) / self.size) as i64,
            ((p[0] + radius) / self.size) as i64,
        );
        let (y0, y1) = (
            ((p[1] - radius) / self.size) as i64,
            ((p[1] + radius) / self.size) as i64,
        );
        (y0..=y1)
            .flat_map(move |y| (x0..=x1).map(move |x| (x, y)))
            .filter_map(|key| self.cells.get(&key))
            .flatten()
            .cloned()
    }
}

/// A chessboard corner has two dark and two light sectors around it.
fn has_four_sectors(plane: &Plane, x: f64, y: f64, radius: f64) -> bool {
    const N: usize = 24;
    let values: Vec<f32> = (0..N)
        .map(|i| {
            let a = i as f64 * std::f64::consts::TAU / N as f64;
            plane.sample(x + radius * a.cos(), y + radius * a.sin())
        })
        .collect();
    let (low, high) = values
        .iter()
        .fold((f32::MAX, f32::MIN), |(l, h), v| (l.min(*v), h.max(*v)));
    if high - low < 20.0 {
        return false;
    }
    let mid = (low + high) / 2.0;
    let dark: Vec<bool> = values.iter().map(|v| *v < mid).collect();
    (0..N).filter(|i| dark[*i] != dark[(i + 1) % N]).count() == 4
}

fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn add(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] + b[0], a[1] + b[1]]
}

fn length(a: [f64; 2]) -> f64 {
    a[0].hypot(a[1])
}

/// Grows a grid of corners from `seed`, each step predicted from the last.
fn grow(
    points: &[([f64; 2], f32)],
    buckets: &Buckets,
    seed: usize,
) -> Option<Vec<Vec<Option<usize>>>> {
    let near = |p: [f64; 2], radius: f64, used: &[bool]| {
        buckets
            .around(p, radius)
            .filter(|i| !used[*i])
            .map(|i| (i, length(sub(points[i].0, p))))
            .filter(|(_, d)| *d < radius)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    };
    let origin = points[seed].0;
    let mut neighbours: Vec<(usize, f64)> = points
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != seed)
        .map(|(i, (q, _))| (i, length(sub(*q, origin))))
        .collect();
    neighbours.sort_by(|a, b| a.1.total_cmp(&b.1));
    let first = *neighbours.first()?;
    let u = sub(points[first.0].0, origin);
    let second = neighbours.iter().take(8).find(|(i, d)| {
        let v = sub(points[*i].0, origin);
        let cos = (u[0] * v[0] + u[1] * v[1]) / (length(u) * d);
        cos.abs() < 0.5 && *d < 2.0 * first.1
    })?;
    let v = sub(points[second.0].0, origin);

    use std::collections::HashMap;
    let mut grid: HashMap<(i32, i32), usize> = HashMap::new();
    let mut used = vec![false; points.len()];
    for (key, index) in [((0, 0), seed), ((1, 0), first.0), ((0, 1), second.0)] {
        grid.insert(key, index);
        used[index] = true;
    }
    let at =
        |grid: &HashMap<(i32, i32), usize>, key: (i32, i32)| grid.get(&key).map(|i| points[*i].0);
    loop {
        let mut added = false;
        let keys: Vec<(i32, i32)> = grid.keys().cloned().collect();
        for (i, j) in keys {
            let p = at(&grid, (i, j)).unwrap();
            for (di, dj) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let target = (i + di, j + dj);
                if grid.contains_key(&target) {
                    continue;
                }
                // Schritt aus dem Vorgänger auf derselben Linie oder dem
                // Nachbarn auf der Nachbarlinie, sonst aus den Startvektoren.
                let step = at(&grid, (i - di, j - dj))
                    .map(|q| sub(p, q))
                    .or_else(|| {
                        let (ni, nj) = (i + dj, j + di);
                        Some(sub(at(&grid, (ni + di, nj + dj))?, at(&grid, (ni, nj))?))
                    })
                    .or_else(|| {
                        let (ni, nj) = (i - dj, j - di);
                        Some(sub(at(&grid, (ni + di, nj + dj))?, at(&grid, (ni, nj))?))
                    })
                    .unwrap_or(match (di, dj) {
                        (1, 0) => u,
                        (-1, 0) => [-u[0], -u[1]],
                        (0, 1) => v,
                        _ => [-v[0], -v[1]],
                    });
                if let Some(found) = near(add(p, step), 0.35 * length(step), &used) {
                    grid.insert(target, found);
                    used[found] = true;
                    added = true;
                }
            }
        }
        if !added || grid.len() > 2000 {
            break;
        }
    }
    let (i0, i1) = (
        grid.keys().map(|k| k.0).min()?,
        grid.keys().map(|k| k.0).max()?,
    );
    let (j0, j1) = (
        grid.keys().map(|k| k.1).min()?,
        grid.keys().map(|k| k.1).max()?,
    );
    Some(
        (j0..=j1)
            .map(|j| (i0..=i1).map(|i| grid.get(&(i, j)).cloned()).collect())
            .collect(),
    )
}

/// Drops outer rows and columns that are mostly empty (stray points beside
/// the board) until the grid is complete.
fn trim(mut rows: Vec<Vec<Option<usize>>>) -> Option<Vec<Vec<usize>>> {
    loop {
        let height = rows.len();
        let width = rows.first()?.len();
        if height == 0 || width == 0 {
            return None;
        }
        if rows.iter().flatten().all(Option::is_some) {
            return Some(
                rows.into_iter()
                    .map(|r| r.into_iter().flatten().collect())
                    .collect(),
            );
        }
        let fill = |cells: Vec<&Option<usize>>| {
            cells.iter().filter(|c| c.is_some()).count() as f64 / cells.len() as f64
        };
        let edges = [
            fill(rows[0].iter().collect()),
            fill(rows[height - 1].iter().collect()),
            fill(rows.iter().map(|r| &r[0]).collect()),
            fill(rows.iter().map(|r| &r[width - 1]).collect()),
        ];
        let (worst, value) = edges
            .iter()
            .enumerate()
            .min_by(|a, b| a.1.total_cmp(b.1))
            .unwrap();
        if *value > 0.6 {
            return None;
        }
        match worst {
            0 => {
                rows.remove(0);
            }
            1 => {
                rows.pop();
            }
            2 => rows.iter_mut().for_each(|r| {
                r.remove(0);
            }),
            _ => rows.iter_mut().for_each(|r| {
                r.pop();
            }),
        }
    }
}

/// The squares between the corners alternate between dark and light.
fn alternates(plane: &Plane, grid: &[Vec<[f64; 2]>]) -> bool {
    let mut agree = 0;
    let mut cells = 0;
    let mut reference = None;
    for j in 0..grid.len() - 1 {
        for i in 0..grid[0].len() - 1 {
            let c = [
                (grid[j][i][0] + grid[j][i + 1][0] + grid[j + 1][i][0] + grid[j + 1][i + 1][0])
                    / 4.0,
                (grid[j][i][1] + grid[j][i + 1][1] + grid[j + 1][i][1] + grid[j + 1][i + 1][1])
                    / 4.0,
            ];
            // Hell/dunkel im Vergleich zum Mittel der vier Eckpunkte.
            let corners: f32 = [
                grid[j][i],
                grid[j][i + 1],
                grid[j + 1][i],
                grid[j + 1][i + 1],
            ]
            .iter()
            .map(|p| plane.sample(p[0], p[1]))
            .sum::<f32>()
                / 4.0;
            let dark = plane.sample(c[0], c[1]) < corners;
            let parity = (i + j) % 2 == 0;
            let reference = *reference.get_or_insert(dark == parity);
            cells += 1;
            if (dark == parity) == reference {
                agree += 1;
            }
        }
    }
    agree as f64 >= 0.9 * cells as f64
}

/// Moves each corner to the point where the image gradients around it are
/// orthogonal to the direction from it, as `cv::cornerSubPix` does.
fn refine(plane: &Plane, corner: [f64; 2], half: usize) -> [f64; 2] {
    let mut p = corner;
    let half_f = half as f64;
    for _ in 0..30 {
        let (cx, cy) = (p[0].round() as isize, p[1].round() as isize);
        let (mut a, mut b, mut c, mut bx, mut by) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for dy in -(half as isize)..=half as isize {
            for dx in -(half as isize)..=half as isize {
                let (x, y) = (cx + dx, cy + dy);
                if x < 1 || y < 1 || x >= plane.width as isize - 1 || y >= plane.height as isize - 1
                {
                    continue;
                }
                let (x, y) = (x as usize, y as usize);
                let gx = (plane.at(x + 1, y) - plane.at(x - 1, y)) as f64 / 2.0;
                let gy = (plane.at(x, y + 1) - plane.at(x, y - 1)) as f64 / 2.0;
                let w = (-(dx * dx + dy * dy) as f64 / (half_f * half_f)).exp();
                let (gxx, gxy, gyy) = (w * gx * gx, w * gx * gy, w * gy * gy);
                a += gxx;
                b += gxy;
                c += gyy;
                bx += gxx * x as f64 + gxy * y as f64;
                by += gxy * x as f64 + gyy * y as f64;
            }
        }
        let det = a * c - b * b;
        if det.abs() < 1e-9 {
            break;
        }
        let next = [(c * bx - b * by) / det, (a * by - b * bx) / det];
        let moved = length(sub(next, p));
        if length(sub(next, corner)) > half_f {
            return corner;
        }
        p = next;
        if moved < 0.001 {
            break;
        }
    }
    p
}

/// Inner chessboard corners with `pattern.0` per row and `pattern.1` rows,
/// refined to sub-pixels, or `None` if the board is not completely visible.
pub fn find_chessboard(image: &RgbaImage, pattern: (usize, usize)) -> Option<Vec<[f64; 2]>> {
    let full = Plane::from_image(image);
    let factor = (full.width.max(full.height) / 1000).max(1);
    let small = full.shrink(factor).blur(1.2);
    let points = saddles(&small);
    let wanted = pattern.0 * pattern.1;
    if points.len() < wanted {
        return None;
    }
    let buckets = Buckets::new(&points, 16.0);
    let mut visited = vec![false; points.len()];
    let mut best: Option<Vec<Vec<[f64; 2]>>> = None;
    // Jeder Punkt eines schon gewachsenen Gitters (etwa des Wabenbetts)
    // scheidet als neuer Startpunkt aus.
    for seed in 0..points.len() {
        if visited[seed] {
            continue;
        }
        let Some(grown) = grow(&points, &buckets, seed) else {
            visited[seed] = true;
            continue;
        };
        visited[seed] = true;
        for i in grown.iter().flatten().flatten() {
            visited[*i] = true;
        }
        let Some(rows) = trim(grown) else {
            continue;
        };
        let dims = (rows[0].len(), rows.len());
        if dims != pattern && dims != (pattern.1, pattern.0) {
            continue;
        }
        let mut grid: Vec<Vec<[f64; 2]>> = rows
            .iter()
            .map(|r| r.iter().map(|i| points[*i].0).collect())
            .collect();
        if !alternates(&small, &grid) {
            continue;
        }
        if dims != pattern {
            grid = (0..dims.0)
                .map(|i| grid.iter().map(|r| r[i]).collect())
                .collect();
        }
        best = Some(grid);
        break;
    }
    let mut grid = best?;
    // Zeilen von links nach rechts, Spalten von oben nach unten (soweit das
    // Brett nicht stark gedreht ist), wie OpenCV meist liefert.
    let along = sub(grid[0][pattern.0 - 1], grid[0][0]);
    if along[0] < 0.0 {
        grid.iter_mut().for_each(|r| r.reverse());
    }
    let down = sub(grid[pattern.1 - 1][0], grid[0][0]);
    if down[1] < 0.0 {
        grid.reverse();
    }
    let step = length(sub(grid[0][1], grid[0][0])).min(length(sub(grid[1][0], grid[0][0])));
    let full_blurred = full.blur(1.0);
    let scale = factor as f64;
    let half = ((0.4 * step * scale) as usize).clamp(2, 11);
    Some(
        grid.iter()
            .flatten()
            .map(|p| {
                let start = [(p[0] + 0.5) * scale - 0.5, (p[1] + 0.5) * scale - 0.5];
                refine(&full_blurred, start, half)
            })
            .collect(),
    )
}

// ---------------------------------------------------------------------------
// Calibration

fn object_points(pattern: (usize, usize)) -> Vec<[f64; 2]> {
    (0..pattern.1)
        .flat_map(|j| (0..pattern.0).map(move |i| [i as f64, j as f64]))
        .collect()
}

type Vec3 = [f64; 3];
type Mat3 = [[f64; 3]; 3];

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm3(a: Vec3) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

fn scale3(a: Vec3, s: f64) -> Vec3 {
    [a[0] * s, a[1] * s, a[2] * s]
}

/// Rotation matrix of a Rodrigues vector.
fn rotation(r: Vec3) -> Mat3 {
    let theta = norm3(r);
    if theta < 1e-12 {
        return [[1.0, -r[2], r[1]], [r[2], 1.0, -r[0]], [-r[1], r[0], 1.0]];
    }
    let [x, y, z] = scale3(r, 1.0 / theta);
    let (s, c) = theta.sin_cos();
    let t = 1.0 - c;
    [
        [c + x * x * t, x * y * t - z * s, x * z * t + y * s],
        [y * x * t + z * s, c + y * y * t, y * z * t - x * s],
        [z * x * t - y * s, z * y * t + x * s, c + z * z * t],
    ]
}

/// Rodrigues vector of a rotation matrix (via its quaternion, also near π).
fn rodrigues(m: &Mat3) -> Vec3 {
    let trace = m[0][0] + m[1][1] + m[2][2];
    let (w, x, y, z) = if trace > 0.0 {
        let s = (trace + 1.0).sqrt() * 2.0;
        (
            0.25 * s,
            (m[2][1] - m[1][2]) / s,
            (m[0][2] - m[2][0]) / s,
            (m[1][0] - m[0][1]) / s,
        )
    } else if m[0][0] > m[1][1] && m[0][0] > m[2][2] {
        let s = (1.0 + m[0][0] - m[1][1] - m[2][2]).sqrt() * 2.0;
        (
            (m[2][1] - m[1][2]) / s,
            0.25 * s,
            (m[0][1] + m[1][0]) / s,
            (m[0][2] + m[2][0]) / s,
        )
    } else if m[1][1] > m[2][2] {
        let s = (1.0 + m[1][1] - m[0][0] - m[2][2]).sqrt() * 2.0;
        (
            (m[0][2] - m[2][0]) / s,
            (m[0][1] + m[1][0]) / s,
            0.25 * s,
            (m[1][2] + m[2][1]) / s,
        )
    } else {
        let s = (1.0 + m[2][2] - m[0][0] - m[1][1]).sqrt() * 2.0;
        (
            (m[1][0] - m[0][1]) / s,
            (m[0][2] + m[2][0]) / s,
            (m[1][2] + m[2][1]) / s,
            0.25 * s,
        )
    };
    let (w, x, y, z) = if w < 0.0 {
        (-w, -x, -y, -z)
    } else {
        (w, x, y, z)
    };
    let sin = (x * x + y * y + z * z).sqrt();
    if sin < 1e-12 {
        return [0.0; 3];
    }
    let angle = 2.0 * sin.atan2(w);
    scale3([x, y, z], angle / sin)
}

/// Pixel of the board point `(X, Y, 0)` seen with pose `rt` (r and t).
fn project(k: &Intrinsics, rt: &[f64], object: [f64; 2]) -> [f64; 2] {
    let r = rotation([rt[0], rt[1], rt[2]]);
    let c = [
        r[0][0] * object[0] + r[0][1] * object[1] + rt[3],
        r[1][0] * object[0] + r[1][1] * object[1] + rt[4],
        r[2][0] * object[0] + r[2][1] * object[1] + rt[5],
    ];
    let [x, y] = k.distort_normalised(c[0] / c[2], c[1] / c[2]);
    [k.fx * x + k.cx, k.fy * y + k.cy]
}

/// Initial focal lengths as OpenCV's `initIntrinsicParams2D`: principal
/// point in the centre, focal lengths from the orthogonality of the
/// homographies' first two columns.
fn initial_intrinsics(homographies: &[Matrix], size: [u32; 2]) -> Intrinsics {
    let (cx, cy) = (size[0] as f64 / 2.0 - 0.5, size[1] as f64 / 2.0 - 0.5);
    let (mut a11, mut a12, mut a22, mut b1, mut b2) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for h in homographies {
        // Bildmitte in den Ursprung verschieben.
        let col = |j: usize| [h[0][j] - cx * h[2][j], h[1][j] - cy * h[2][j], h[2][j]];
        let (h1, h2) = (col(0), col(1));
        let n1 = norm3(h1);
        let n2 = norm3(h2);
        let (h1, h2) = (scale3(h1, 1.0 / n1), scale3(h2, 1.0 / n2));
        let rows = [
            ([h1[0] * h2[0], h1[1] * h2[1]], -h1[2] * h2[2]),
            (
                [h1[0] * h1[0] - h2[0] * h2[0], h1[1] * h1[1] - h2[1] * h2[1]],
                -(h1[2] * h1[2] - h2[2] * h2[2]),
            ),
        ];
        for (row, rhs) in rows {
            a11 += row[0] * row[0];
            a12 += row[0] * row[1];
            a22 += row[1] * row[1];
            b1 += row[0] * rhs;
            b2 += row[1] * rhs;
        }
    }
    let det = a11 * a22 - a12 * a12;
    let (a, b) = ((a22 * b1 - a12 * b2) / det, (a11 * b2 - a12 * b1) / det);
    let fallback = size[0].max(size[1]) as f64;
    let focal = |v: f64| {
        if v.is_finite() && v > 0.0 {
            (1.0 / v).sqrt()
        } else {
            fallback
        }
    };
    Intrinsics {
        fx: focal(a),
        fy: focal(b),
        cx,
        cy,
        dist: [0.0; 5],
    }
}

/// Pose of a board from its homography and the camera matrix.
fn initial_pose(h: &Matrix, k: &Intrinsics) -> [f64; 6] {
    let inv = |j: usize| {
        let x = (h[0][j] - k.cx * h[2][j]) / k.fx;
        let y = (h[1][j] - k.cy * h[2][j]) / k.fy;
        [x, y, h[2][j]]
    };
    let (h1, h2, h3) = (inv(0), inv(1), inv(2));
    let mut lambda = 2.0 / (norm3(h1) + norm3(h2));
    if h3[2] * lambda < 0.0 {
        lambda = -lambda;
    }
    let (r1, r2, t) = (scale3(h1, lambda), scale3(h2, lambda), scale3(h3, lambda));
    // r1, r2 symmetrisch orthonormieren.
    let unit = |a: Vec3| scale3(a, 1.0 / norm3(a));
    let (r1, r2) = (unit(r1), unit(r2));
    let c = unit([r1[0] + r2[0], r1[1] + r2[1], r1[2] + r2[2]]);
    let d = unit([r1[0] - r2[0], r1[1] - r2[1], r1[2] - r2[2]]);
    let s = std::f64::consts::FRAC_1_SQRT_2;
    let r1 = scale3([c[0] + d[0], c[1] + d[1], c[2] + d[2]], s);
    let r2 = scale3([c[0] - d[0], c[1] - d[1], c[2] - d[2]], s);
    let r3 = cross(r1, r2);
    let m = [
        [r1[0], r2[0], r3[0]],
        [r1[1], r2[1], r3[1]],
        [r1[2], r2[2], r3[2]],
    ];
    let r = rodrigues(&m);
    [r[0], r[1], r[2], t[0], t[1], t[2]]
}

const INTRINSIC_PARAMS: usize = 9;

fn intrinsics_of(params: &[f64]) -> Intrinsics {
    Intrinsics {
        fx: params[0],
        fy: params[1],
        cx: params[2],
        cy: params[3],
        dist: [params[4], params[5], params[6], params[7], params[8]],
    }
}

/// Residuals (pixels) of one view.
fn view_residuals(
    k: &Intrinsics,
    pose: &[f64],
    objects: &[[f64; 2]],
    seen: &[[f64; 2]],
    out: &mut Vec<f64>,
) {
    for (o, s) in objects.iter().zip(seen) {
        let p = project(k, pose, *o);
        out.push(p[0] - s[0]);
        out.push(p[1] - s[1]);
    }
}

fn all_residuals(params: &[f64], objects: &[[f64; 2]], views: &[Vec<[f64; 2]>]) -> Vec<f64> {
    let k = intrinsics_of(params);
    let mut out = Vec::new();
    for (v, seen) in views.iter().enumerate() {
        let pose = &params[INTRINSIC_PARAMS + 6 * v..INTRINSIC_PARAMS + 6 * v + 6];
        view_residuals(&k, pose, objects, seen, &mut out);
    }
    out
}

/// Solves the symmetric positive definite system `a x = b` (Cholesky).
fn solve(mut a: Vec<f64>, mut b: Vec<f64>, n: usize) -> Option<Vec<f64>> {
    for j in 0..n {
        let mut d = a[j * n + j];
        for k in 0..j {
            d -= a[j * n + k] * a[j * n + k];
        }
        if d <= 0.0 || !d.is_finite() {
            return None;
        }
        let d = d.sqrt();
        a[j * n + j] = d;
        for i in j + 1..n {
            let mut s = a[i * n + j];
            for k in 0..j {
                s -= a[i * n + k] * a[j * n + k];
            }
            a[i * n + j] = s / d;
        }
    }
    for i in 0..n {
        let mut s = b[i];
        for k in 0..i {
            s -= a[i * n + k] * b[k];
        }
        b[i] = s / a[i * n + i];
    }
    for i in (0..n).rev() {
        let mut s = b[i];
        for k in i + 1..n {
            s -= a[k * n + i] * b[k];
        }
        b[i] = s / a[i * n + i];
    }
    Some(b)
}

/// Calibrates the camera from the corners of `pattern` found in each photo
/// of `size` pixels (OpenCV `calibrateCamera`, flags 0, square size 1).
pub fn calibrate(
    views: &[Vec<[f64; 2]>],
    pattern: (usize, usize),
    size: [u32; 2],
) -> Result<Calibration, String> {
    if views.len() < 3 {
        return Err(
            crate::i18n::tr("Mindestens drei Fotos mit erkanntem Schachbrett nötig").into(),
        );
    }
    let objects = object_points(pattern);
    let homographies: Vec<Matrix> = views
        .iter()
        .map(|seen| {
            camera::homography(&CameraCalibration {
                reference_points: objects.clone(),
                view_points: seen.clone(),
                ..Default::default()
            })
        })
        .collect::<Result<_, _>>()?;
    let k = initial_intrinsics(&homographies, size);
    let mut params = vec![k.fx, k.fy, k.cx, k.cy, 0.0, 0.0, 0.0, 0.0, 0.0];
    for h in &homographies {
        params.extend(initial_pose(h, &k));
    }

    // Levenberg-Marquardt; die Ableitungen numerisch, je Foto nur über
    // dessen eigene Lage.
    let n = params.len();
    let cost = |r: &[f64]| r.iter().map(|v| v * v).sum::<f64>();
    let mut residuals = all_residuals(&params, &objects, views);
    let mut lambda = 1e-3;
    for _ in 0..200 {
        let m = residuals.len();
        let mut jacobian = vec![0.0; m * n];
        for p in 0..n {
            let step = 1e-6 * params[p].abs().max(1e-2);
            let mut shifted = params.clone();
            shifted[p] += step;
            let (rows, values) = if p < INTRINSIC_PARAMS {
                (0..m, all_residuals(&shifted, &objects, views))
            } else {
                let v = (p - INTRINSIC_PARAMS) / 6;
                let pose = &shifted[INTRINSIC_PARAMS + 6 * v..INTRINSIC_PARAMS + 6 * v + 6];
                let mut out = Vec::new();
                view_residuals(
                    &intrinsics_of(&shifted),
                    pose,
                    &objects,
                    &views[v],
                    &mut out,
                );
                let start = 2 * objects.len() * v;
                (start..start + out.len(), out)
            };
            for (row, value) in rows.zip(values) {
                jacobian[row * n + p] = (value - residuals[row]) / step;
            }
        }
        let mut jtj = vec![0.0; n * n];
        let mut jtr = vec![0.0; n];
        for row in 0..m {
            let j = &jacobian[row * n..(row + 1) * n];
            let nonzero: Vec<usize> = (0..n).filter(|c| j[*c] != 0.0).collect();
            for &a in &nonzero {
                jtr[a] += j[a] * residuals[row];
                for &b in &nonzero {
                    jtj[a * n + b] += j[a] * j[b];
                }
            }
        }
        let current = cost(&residuals);
        let mut improved = false;
        for _ in 0..10 {
            let mut damped = jtj.clone();
            for i in 0..n {
                damped[i * n + i] += lambda * jtj[i * n + i].max(1e-12);
            }
            let Some(delta) = solve(damped, jtr.iter().map(|v| -v).collect(), n) else {
                lambda *= 10.0;
                continue;
            };
            let candidate: Vec<f64> = params.iter().zip(&delta).map(|(p, d)| p + d).collect();
            let trial = all_residuals(&candidate, &objects, views);
            let trial_cost = cost(&trial);
            if trial_cost.is_finite() && trial_cost < current {
                let gain = (current - trial_cost) / current.max(1e-300);
                params = candidate;
                residuals = trial;
                lambda = (lambda / 10.0).max(1e-12);
                improved = gain > 1e-12;
                break;
            }
            lambda *= 10.0;
        }
        if !improved {
            break;
        }
    }

    let points = objects.len();
    let rms = (cost(&residuals) / (points * views.len()) as f64).sqrt();
    let view_rms = residuals
        .chunks(2 * points)
        .map(|r| (cost(r) / points as f64).sqrt())
        .collect();
    let pose = |v: usize, offset: usize| {
        let s = INTRINSIC_PARAMS + 6 * v + offset;
        [params[s], params[s + 1], params[s + 2]]
    };
    Ok(Calibration {
        intrinsics: intrinsics_of(&params),
        image_size: size,
        rvecs: (0..views.len()).map(|v| pose(v, 0)).collect(),
        tvecs: (0..views.len()).map(|v| pose(v, 3)).collect(),
        rms,
        view_rms,
    })
}

/// Undistorted picture with the same camera matrix (`cv::undistort` without
/// a new camera matrix), bilinear, black outside the photo.
pub fn undistort_image(image: &RgbaImage, k: &Intrinsics) -> RgbaImage {
    let (w, h) = (image.width(), image.height());
    let get = |x: i64, y: i64, c: usize| -> f64 {
        if x < 0 || y < 0 || x >= w as i64 || y >= h as i64 {
            if c == 3 { 255.0 } else { 0.0 }
        } else {
            image.get_pixel(x as u32, y as u32)[c] as f64
        }
    };
    let mut raw = vec![0u8; w as usize * h as usize * 4];
    camera::parallel_rows(&mut raw, w as usize * 4, |y, row| {
        for (x, out) in row.chunks_exact_mut(4).enumerate() {
            let [sx, sy] = k.distort([x as f64, y as f64]);
            let (x0, y0) = (sx.floor(), sy.floor());
            let (fx, fy) = (sx - x0, sy - y0);
            let (x0, y0) = (x0 as i64, y0 as i64);
            for (c, value) in out.iter_mut().enumerate() {
                let top = get(x0, y0, c) * (1.0 - fx) + get(x0 + 1, y0, c) * fx;
                let bottom = get(x0, y0 + 1, c) * (1.0 - fx) + get(x0 + 1, y0 + 1, c) * fx;
                *value = (top * (1.0 - fy) + bottom * fy).round().clamp(0.0, 255.0) as u8;
            }
        }
    });
    RgbaImage::from_raw(w, h, raw).expect("buffer has the image size")
}

// ---------------------------------------------------------------------------
// calibration.npz

/// NumPy `.npy` (format 1.0) of a little-endian float64 array.
fn npy(shape: &[usize], values: &[f64]) -> Vec<u8> {
    let shape = match shape {
        [n] => format!("({n},)"),
        _ => format!(
            "({})",
            shape
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    let mut header = format!("{{'descr': '<f8', 'fortran_order': False, 'shape': {shape}, }}");
    while (10 + header.len() + 1) % 64 != 0 {
        header.push(' ');
    }
    header.push('\n');
    let mut out = b"\x93NUMPY\x01\x00".to_vec();
    out.extend((header.len() as u16).to_le_bytes());
    out.extend(header.as_bytes());
    for v in values {
        out.extend(v.to_le_bytes());
    }
    out
}

/// `calibration.npz` as undistort-webcam's `save_calibration` writes it.
pub fn to_npz(calibration: &Calibration) -> Result<Vec<u8>, String> {
    let k = &calibration.intrinsics;
    let matrix: Vec<f64> = k.camera_matrix().iter().flatten().cloned().collect();
    let n = calibration.rvecs.len();
    let arrays = [
        ("camera_matrix", npy(&[3, 3], &matrix)),
        ("dist_coeffs", npy(&[1, 5], &k.dist)),
        ("rvecs", npy(&[n, 3, 1], &calibration.rvecs.concat())),
        ("tvecs", npy(&[n, 3, 1], &calibration.tvecs.concat())),
    ];
    let mut bytes = Vec::new();
    let mut zip = zip::ZipWriter::new(Cursor::new(&mut bytes));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, data) in arrays {
        zip.start_file(format!("{name}.npy"), options)
            .map_err(|e| e.to_string())?;
        zip.write_all(&data).map_err(|e| e.to_string())?;
    }
    zip.finish().map_err(|e| e.to_string())?;
    Ok(bytes)
}

/// Float64 values and shape of a `.npy` array.
fn read_npy(bytes: &[u8]) -> Result<(Vec<usize>, Vec<f64>), String> {
    let bad = || crate::i18n::tr("Keine gültige NumPy-Datei").to_string();
    if bytes.len() < 10 || &bytes[..6] != b"\x93NUMPY" {
        return Err(bad());
    }
    let (header_len, start) = if bytes[6] == 1 {
        (u16::from_le_bytes([bytes[8], bytes[9]]) as usize, 10)
    } else {
        (
            u32::from_le_bytes(bytes[8..12].try_into().map_err(|_| bad())?) as usize,
            12,
        )
    };
    let header = std::str::from_utf8(bytes.get(start..start + header_len).ok_or_else(bad)?)
        .map_err(|_| bad())?;
    let big_endian = header.contains("'>f8'");
    if !(header.contains("'<f8'") || big_endian) || header.contains("'fortran_order': True") {
        return Err(bad());
    }
    let shape_text = header.split("'shape':").nth(1).ok_or_else(bad)?;
    let shape_text = &shape_text[shape_text.find('(').ok_or_else(bad)? + 1..];
    let shape: Vec<usize> = shape_text[..shape_text.find(')').ok_or_else(bad)?]
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.parse().map_err(|_| bad()))
        .collect::<Result<_, _>>()?;
    let values = bytes[start + header_len..]
        .chunks_exact(8)
        .map(|c| {
            let c: [u8; 8] = c.try_into().unwrap();
            if big_endian {
                f64::from_be_bytes(c)
            } else {
                f64::from_le_bytes(c)
            }
        })
        .collect::<Vec<_>>();
    if values.len() != shape.iter().product::<usize>() {
        return Err(bad());
    }
    Ok((shape, values))
}

/// Camera matrix and distortion from a `calibration.npz`.
pub fn read_npz(bytes: &[u8]) -> Result<Intrinsics, String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let mut array = |name: &str| -> Result<Vec<f64>, String> {
        let mut file = archive
            .by_name(&format!("{name}.npy"))
            .map_err(|_| crate::tr!("{name} fehlt in der Kalibrierdatei", name = name))?;
        let mut data = Vec::new();
        file.read_to_end(&mut data).map_err(|e| e.to_string())?;
        Ok(read_npy(&data)?.1)
    };
    let m = array("camera_matrix")?;
    let d = array("dist_coeffs")?;
    if m.len() != 9 || d.len() < 4 {
        return Err(crate::i18n::tr("Keine gültige NumPy-Datei").to_string());
    }
    let mut dist = [0.0; 5];
    dist[..d.len().min(5)].copy_from_slice(&d[..d.len().min(5)]);
    Ok(Intrinsics {
        fx: m[0],
        fy: m[4],
        cx: m[2],
        cy: m[5],
        dist,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn truth() -> Intrinsics {
        Intrinsics {
            fx: 1400.0,
            fy: 1390.0,
            cx: 805.0,
            cy: 590.0,
            dist: [-0.18, 0.06, 0.001, -0.0007, -0.01],
        }
    }

    /// [`truth`] for photos of 800 × 600 pixels, small enough to render
    /// quickly in unoptimised test builds.
    fn small() -> Intrinsics {
        let k = truth();
        Intrinsics {
            fx: k.fx / 2.0,
            fy: k.fy / 2.0,
            cx: k.cx / 2.0,
            cy: k.cy / 2.0,
            dist: k.dist,
        }
    }

    /// Photo of a 10 × 7 squares board (9 × 6 inner corners) with pose `rt`
    /// through the lens `k`, slightly blurred by 3 × 3 supersampling.
    fn render(k: &Intrinsics, rt: &[f64; 6], size: [u32; 2]) -> RgbaImage {
        let r = rotation([rt[0], rt[1], rt[2]]);
        // Bildpunkt -> Brettpunkt: entzerren, dann Strahl mit der Ebene schneiden.
        let to_board = |u: [f64; 2]| {
            let p = k.undistort(u);
            let ray = [(p[0] - k.cx) / k.fx, (p[1] - k.cy) / k.fy, 1.0];
            // Ebene z=0 im Brett: n = r3, Punkt t.
            let n = [r[0][2], r[1][2], r[2][2]];
            let t = [rt[3], rt[4], rt[5]];
            let s = (n[0] * t[0] + n[1] * t[1] + n[2] * t[2])
                / (n[0] * ray[0] + n[1] * ray[1] + n[2] * ray[2]);
            let c = [ray[0] * s - t[0], ray[1] * s - t[1], ray[2] * s - t[2]];
            [
                r[0][0] * c[0] + r[1][0] * c[1] + r[2][0] * c[2],
                r[0][1] * c[0] + r[1][1] * c[1] + r[2][1] * c[2],
            ]
        };
        RgbaImage::from_fn(size[0], size[1], |x, y| {
            let mut sum = 0.0;
            for sy in 0..3 {
                for sx in 0..3 {
                    let b = to_board([
                        x as f64 + (sx as f64 + 0.5) / 3.0 - 0.5,
                        y as f64 + (sy as f64 + 0.5) / 3.0 - 0.5,
                    ]);
                    let (i, j) = ((b[0] + 1.0).floor(), (b[1] + 1.0).floor());
                    let on_board = (0.0..10.0).contains(&i) && (0.0..7.0).contains(&j);
                    let margin = (-1.6..10.6).contains(&b[0]) && (-1.6..7.6).contains(&b[1]);
                    sum += if on_board && (i + j) as i64 % 2 == 0 {
                        30.0
                    } else if margin {
                        225.0
                    } else {
                        110.0
                    };
                }
            }
            let v = (sum / 9.0) as u8;
            image::Rgba([v, v, v, 255])
        })
    }

    fn poses() -> Vec<[f64; 6]> {
        vec![
            [0.05, -0.1, 0.02, -4.0, -2.5, 14.0],
            [0.35, 0.1, 0.05, -5.5, -2.0, 15.0],
            [-0.3, 0.25, -0.1, -2.5, -3.5, 13.0],
            [0.1, -0.4, 0.3, -6.0, -4.5, 16.0],
            [-0.2, -0.3, -0.25, -1.0, -1.0, 15.5],
            [0.3, 0.35, 0.1, -7.0, -1.0, 17.0],
        ]
    }

    #[test]
    fn rodrigues_round_trip_also_near_pi() {
        for r in [
            [0.1, -0.2, 0.3],
            [3.1, 0.05, -0.02],
            [0.0, 0.0, 0.0],
            [-1.0, 2.0, 0.5],
        ] {
            let back = rodrigues(&rotation(r));
            let same = (0..3).all(|i| (back[i] - r[i]).abs() < 1e-9);
            assert!(same, "{r:?} -> {back:?}");
        }
    }

    #[test]
    fn undistort_inverts_distort() {
        let k = truth();
        for p in [
            [10.0, 10.0],
            [800.0, 600.0],
            [1500.0, 1100.0],
            [200.0, 900.0],
        ] {
            let back = k.distort(k.undistort(p));
            assert!(length(sub(back, p)) < 1e-6, "{p:?} -> {back:?}");
        }
    }

    #[test]
    fn finds_corners_of_rendered_board() {
        let k = small();
        let pose = poses()[1];
        let image = render(&k, &pose, [800, 600]);
        let found = find_chessboard(&image, PATTERN).expect("Schachbrett");
        assert_eq!(found.len(), 54);
        for (o, f) in object_points(PATTERN).iter().zip(&found) {
            let expected = project(&k, &pose, *o);
            assert!(
                length(sub(*f, expected)) < 0.3,
                "{o:?}: {f:?} statt {expected:?}"
            );
        }
    }

    #[test]
    fn no_board_in_plain_picture() {
        let image = RgbaImage::from_fn(800, 600, |x, y| {
            let v = ((x / 40 + y / 60) % 3 * 60) as u8;
            image::Rgba([v, v, v, 255])
        });
        assert!(find_chessboard(&image, PATTERN).is_none());
    }

    #[test]
    fn calibration_recovers_lens_from_exact_corners() {
        let k = truth();
        let objects = object_points(PATTERN);
        let views: Vec<Vec<[f64; 2]>> = poses()
            .iter()
            .map(|pose| objects.iter().map(|o| project(&k, pose, *o)).collect())
            .collect();
        let c = calibrate(&views, PATTERN, [1600, 1200]).unwrap();
        assert!(c.rms < 1e-4, "rms {}", c.rms);
        let e = &c.intrinsics;
        assert!(
            (e.fx - k.fx).abs() < 0.1 && (e.fy - k.fy).abs() < 0.1,
            "{e:?}"
        );
        assert!(
            (e.cx - k.cx).abs() < 0.1 && (e.cy - k.cy).abs() < 0.1,
            "{e:?}"
        );
        for p in [[0.0, 0.0], [1599.0, 1199.0], [800.0, 100.0]] {
            assert!(length(sub(e.undistort(p), k.undistort(p))) < 0.05, "{p:?}");
        }
    }

    #[test]
    fn calibration_from_rendered_photos() {
        let k = small();
        let views: Vec<Vec<[f64; 2]>> = poses()
            .iter()
            .map(|pose| find_chessboard(&render(&k, pose, [800, 600]), PATTERN).unwrap())
            .collect();
        let c = calibrate(&views, PATTERN, [800, 600]).unwrap();
        assert!(c.rms < 0.15, "rms {}", c.rms);
        for p in [[50.0, 50.0], [750.0, 550.0], [400.0, 300.0]] {
            let error = length(sub(c.intrinsics.undistort(p), k.undistort(p)));
            assert!(error < 0.5, "{p:?}: {error}");
        }
    }

    #[test]
    fn npz_round_trip() {
        let calibration = Calibration {
            intrinsics: truth(),
            image_size: [1600, 1200],
            rvecs: vec![[0.1, 0.2, 0.3], [0.4, 0.5, 0.6]],
            tvecs: vec![[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]],
            rms: 0.2,
            view_rms: vec![0.2, 0.2],
        };
        let bytes = to_npz(&calibration).unwrap();
        assert_eq!(read_npz(&bytes).unwrap(), truth());
        let mut archive = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
        let mut data = Vec::new();
        archive
            .by_name("rvecs.npy")
            .unwrap()
            .read_to_end(&mut data)
            .unwrap();
        let (shape, values) = read_npy(&data).unwrap();
        assert_eq!(shape, [2, 3, 1]);
        assert_eq!(values[3], 0.4);
        // Kopf auf 64 Byte ausgerichtet wie bei NumPy.
        assert_eq!(
            (10 + u16::from_le_bytes([data[8], data[9]]) as usize) % 64,
            0
        );
    }

    /// Vergleich mit OpenCV auf den Fotos von fau-fablab/undistort-webcam:
    /// `UNDISTORT_TESTDATA=…/undistort/testdata cargo test --release
    /// lens_calibration -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn fau_testdata() {
        let dir = std::env::var("UNDISTORT_TESTDATA").expect("UNDISTORT_TESTDATA");
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == "jpg"))
            .collect();
        files.sort();
        let mut views = Vec::new();
        let mut size = [0, 0];
        for file in files {
            let image = image::open(&file).unwrap().to_rgba8();
            size = [image.width(), image.height()];
            let start = std::time::Instant::now();
            let found = find_chessboard(&image, PATTERN);
            println!(
                "{}: {} ({:?})",
                file.display(),
                found.is_some(),
                start.elapsed()
            );
            views.extend(found);
        }
        if let Ok(path) = std::env::var("UNDISTORT_CORNERS") {
            std::fs::write(path, serde_json::to_string(&views).unwrap()).unwrap();
        }
        let start = std::time::Instant::now();
        let c = calibrate(&views, PATTERN, size).unwrap();
        println!("{} Fotos, {:?}: {c:#?}", views.len(), start.elapsed());
        if let Ok(path) = std::env::var("UNDISTORT_NPZ") {
            std::fs::write(path, to_npz(&c).unwrap()).unwrap();
        }
    }
}
