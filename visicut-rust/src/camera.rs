//! Camera background for the bed: port of VisiCut's Homography.java.
//! A homography maps bed positions (mm) to camera pixels; the corrected image
//! samples the camera picture for every bed position.
pub use crate::device::Lens;
use crate::device::{CameraCalibration, LaserDevice};
use crate::{i18n::tr, tr};
use image::RgbaImage;
use std::{io::Read, time::Duration};

pub type Matrix = [[f64; 3]; 3];

/// Port and request of VisiCam's network discovery (VisiCut
/// `Helper.findVisiCamInstances`).
const DISCOVERY_PORT: u16 = 8888;
const DISCOVERY_REQUEST: &[u8] = b"VisiCamDiscover";

/// Searches the local network for VisiCam cameras like VisiCut's „Suchen“:
/// a UDP broadcast, then every answer starting with `http` within three
/// seconds, without duplicates.
pub fn discover() -> Vec<String> {
    let target = std::net::SocketAddr::from(([255, 255, 255, 255], DISCOVERY_PORT));
    discover_at(&[target], Duration::from_secs(3))
}

fn discover_at(targets: &[std::net::SocketAddr], wait: Duration) -> Vec<String> {
    let Ok(socket) = std::net::UdpSocket::bind(("0.0.0.0", 0)) else {
        return Vec::new();
    };
    let _ = socket.set_broadcast(true);
    for target in targets {
        let _ = socket.send_to(DISCOVERY_REQUEST, target);
    }
    let deadline = std::time::Instant::now() + wait;
    let mut found = Vec::new();
    let mut buffer = [0u8; 15000];
    while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
        if left.is_zero() || socket.set_read_timeout(Some(left)).is_err() {
            break;
        }
        let Ok((length, _)) = socket.recv_from(&mut buffer) else {
            break;
        };
        let message = String::from_utf8_lossy(&buffer[..length])
            .trim_matches(|c: char| c.is_whitespace() || c == '\0')
            .to_string();
        if message.starts_with("http") && !found.contains(&message) {
            found.push(message);
        }
    }
    found
}

/// Fetches http(s) URLs or reads local files (`file://` or a plain path).
pub fn fetch(url: &str, limit: u64) -> Result<Vec<u8>, String> {
    fetch_with(url, limit, None)
}

/// HTTP Basic header value, as VisiCut sends it: only with user and password.
fn basic_auth(user: &str, password: &str) -> Option<String> {
    use base64::Engine;
    (!user.is_empty() && !password.is_empty()).then(|| {
        format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(format!("{user}:{password}"))
        )
    })
}

/// [`fetch`] with optional HTTP authentication `(user, password)`.
pub fn fetch_with(
    url: &str,
    limit: u64,
    credentials: Option<(&str, &str)>,
) -> Result<Vec<u8>, String> {
    if url.starts_with("http://") || url.starts_with("https://") {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(20)))
            .build()
            .into();
        let mut request = agent.get(url);
        if let Some(header) = credentials.and_then(|(user, password)| basic_auth(user, password)) {
            request = request.header("Authorization", header);
        }
        let mut response = request.call().map_err(|e| format!("{url}: {e}"))?;
        return response
            .body_mut()
            .with_config()
            .limit(limit)
            .read_to_vec()
            .map_err(|e| format!("{url}: {e}"));
    }
    let path = url.strip_prefix("file://").unwrap_or(url);
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| format!("{path}: {e}"))?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err(tr!("{path}: Datei ist zu groß", path = path));
    }
    Ok(bytes)
}

pub fn transform(h: &Matrix, p: [f64; 2]) -> [f64; 2] {
    let w = h[2][0] * p[0] + h[2][1] * p[1] + h[2][2];
    [
        (h[0][0] * p[0] + h[0][1] * p[1] + h[0][2]) / w,
        (h[1][0] * p[0] + h[1][1] * p[1] + h[1][2]) / w,
    ]
}

fn multiply(a: &Matrix, b: &Matrix) -> Matrix {
    let mut r = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            r[i][j] = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    r
}

/// Similarity transform moving the centroid to 0 and the mean distance to √2
/// (Hartley normalisation), returned with its inverse.
fn normalisation(points: &[[f64; 2]]) -> (Matrix, Matrix) {
    let n = points.len() as f64;
    let cx = points.iter().map(|p| p[0]).sum::<f64>() / n;
    let cy = points.iter().map(|p| p[1]).sum::<f64>() / n;
    let mean = points
        .iter()
        .map(|p| ((p[0] - cx).powi(2) + (p[1] - cy).powi(2)).sqrt())
        .sum::<f64>()
        / n;
    let s = if mean > 0.0 { 2f64.sqrt() / mean } else { 1.0 };
    (
        [[s, 0.0, -s * cx], [0.0, s, -s * cy], [0.0, 0.0, 1.0]],
        [[1.0 / s, 0.0, cx], [0.0, 1.0 / s, cy], [0.0, 0.0, 1.0]],
    )
}

/// Eigenvector of the smallest eigenvalue of a symmetric matrix (cyclic Jacobi).
fn smallest_eigenvector(mut a: [[f64; 9]; 9]) -> [f64; 9] {
    let mut v = [[0.0; 9]; 9];
    for (i, row) in v.iter_mut().enumerate() {
        row[i] = 1.0;
    }
    for _ in 0..100 {
        let off: f64 = (0..9)
            .flat_map(|i| (0..9).filter(move |j| *j != i).map(move |j| (i, j)))
            .map(|(i, j)| a[i][j] * a[i][j])
            .sum();
        if off < 1e-24 {
            break;
        }
        for p in 0..9 {
            for q in p + 1..9 {
                if a[p][q].abs() < 1e-300 {
                    continue;
                }
                let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
                let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
                let t = if theta == 0.0 { 1.0 } else { t };
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                for row in a.iter_mut() {
                    let (akp, akq) = (row[p], row[q]);
                    row[p] = c * akp - s * akq;
                    row[q] = s * akp + c * akq;
                }
                for k in 0..9 {
                    let (apk, aqk) = (a[p][k], a[q][k]);
                    a[p][k] = c * apk - s * aqk;
                    a[q][k] = s * apk + c * aqk;
                }
                for row in v.iter_mut() {
                    let (vp, vq) = (row[p], row[q]);
                    row[p] = c * vp - s * vq;
                    row[q] = s * vp + c * vq;
                }
            }
        }
    }
    let smallest = (0..9)
        .min_by(|i, j| a[*i][*i].total_cmp(&a[*j][*j]))
        .unwrap();
    std::array::from_fn(|i| v[i][smallest])
}

fn spans_plane(points: &[[f64; 2]]) -> bool {
    let extent = points
        .iter()
        .flat_map(|a| points.iter().map(move |b| (a[0] - b[0]).hypot(a[1] - b[1])))
        .fold(0.0, f64::max);
    points.iter().enumerate().any(|(i, a)| {
        points[i + 1..].iter().enumerate().any(|(j, b)| {
            points[i + j + 2..].iter().any(|c| {
                ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs()
                    > 1e-6 * extent * extent
            })
        })
    })
}

/// Homography from bed mm (reference) to camera pixels (view). Two point
/// pairs give VisiCut's legacy axis-aligned scale and offset.
pub fn homography(calibration: &CameraCalibration) -> Result<Matrix, String> {
    let (reference, view) = (&calibration.reference_points, &calibration.view_points);
    if reference.len() != view.len() || !(reference.len() == 2 || reference.len() >= 4) {
        return Err(tr("Kamerakalibrierung benötigt 2 oder mindestens 4 Punktpaare").into());
    }
    if reference.len() == 2 {
        let sx = (view[1][0] - view[0][0]) / (reference[1][0] - reference[0][0]);
        let sy = (view[1][1] - view[0][1]) / (reference[1][1] - reference[0][1]);
        if !sx.is_finite() || !sy.is_finite() || sx == 0.0 || sy == 0.0 {
            return Err(tr("Kalibrierpunkte dürfen nicht auf einer Achse liegen").into());
        }
        return Ok([
            [sx, 0.0, view[0][0] - sx * reference[0][0]],
            [0.0, sy, view[0][1] - sy * reference[0][1]],
            [0.0, 0.0, 1.0],
        ]);
    }
    if !spans_plane(reference) || !spans_plane(view) {
        return Err(tr("Kalibrierpunkte liegen alle auf einer Linie").into());
    }
    let (tr, _) = normalisation(reference);
    let (tv, tv_inverse) = normalisation(view);
    let mut ata = [[0.0; 9]; 9];
    for (r, v) in reference.iter().zip(view) {
        let r = transform(&tr, *r);
        let v = transform(&tv, *v);
        let rows = [
            [
                r[0],
                r[1],
                1.0,
                0.0,
                0.0,
                0.0,
                -v[0] * r[0],
                -v[0] * r[1],
                -v[0],
            ],
            [
                0.0,
                0.0,
                0.0,
                r[0],
                r[1],
                1.0,
                -v[1] * r[0],
                -v[1] * r[1],
                -v[1],
            ],
        ];
        for row in rows {
            for i in 0..9 {
                for j in 0..9 {
                    ata[i][j] += row[i] * row[j];
                }
            }
        }
    }
    let h = smallest_eigenvector(ata);
    let normalised = [[h[0], h[1], h[2]], [h[3], h[4], h[5]], [h[6], h[7], h[8]]];
    let result = multiply(&multiply(&tv_inverse, &normalised), &tr);
    if result.iter().flatten().any(|v| !v.is_finite()) || result[2][2].abs() < 1e-300 {
        return Err(crate::i18n::tr(
            "Kalibrierpunkte sind degeneriert (z. B. drei auf einer Linie)",
        )
        .into());
    }
    let scale = result[2][2];
    Ok(result.map(|row| row.map(|v| v / scale)))
}

/// Undistortion factor of the Raspberry Pi Camera v2 standard lens, measured
/// on the FAU laser camera (r relative to the focal length); the crosses of a
/// calibration refine it for each camera.
const RPI_V2_K1: f64 = -0.11;

impl Lens {
    /// Raspberry Pi Camera v2 (Sony IMX219, 3280 × 2464 px of 1.12 µm, lens
    /// 3.04 mm) delivering pictures of `size`: the ISP scales the full sensor
    /// and crops it centred to the picture's aspect ratio.
    pub fn raspberry_pi_v2(size: [f64; 2]) -> Self {
        let scale = (size[0] / 3280.0).max(size[1] / 2464.0);
        Self {
            profile: "rpi-v2".into(),
            center: [size[0] / 2.0, size[1] / 2.0],
            focal: 3.04 / 0.00112 * scale,
            k1: RPI_V2_K1,
            k2: 0.0,
        }
    }

    /// Unknown camera: centre of the picture, no distortion until the
    /// calibration crosses determine it.
    pub fn generic(size: [f64; 2]) -> Self {
        Self {
            profile: "generic".into(),
            center: [size[0] / 2.0, size[1] / 2.0],
            focal: size[0].max(size[1]),
            k1: 0.0,
            k2: 0.0,
        }
    }

    fn factor(&self, d: [f64; 2]) -> f64 {
        let r2 = (d[0] * d[0] + d[1] * d[1]) / (self.focal * self.focal);
        1.0 + self.k1 * r2 + self.k2 * r2 * r2
    }

    /// Ideal (pinhole) position of a camera pixel.
    pub fn undistort(&self, p: [f64; 2]) -> [f64; 2] {
        let d = [p[0] - self.center[0], p[1] - self.center[1]];
        let f = self.factor(d);
        [self.center[0] + d[0] * f, self.center[1] + d[1] * f]
    }

    /// Camera pixel showing an ideal position (inverse of [`Self::undistort`]).
    pub fn distort(&self, u: [f64; 2]) -> [f64; 2] {
        let du = [u[0] - self.center[0], u[1] - self.center[1]];
        let mut d = du;
        for _ in 0..20 {
            let f = self.factor(d);
            if f.abs() < 1e-6 {
                break;
            }
            let next = [du[0] / f, du[1] / f];
            let step = (next[0] - d[0]).abs() + (next[1] - d[1]).abs();
            d = next;
            if step < 1e-3 {
                break;
            }
        }
        [self.center[0] + d[0], self.center[1] + d[1]]
    }
}

/// Bed (mm) to camera pixels: homography on undistorted pixels, then the lens.
#[derive(Clone, Debug)]
pub struct Mapping {
    pub h: Matrix,
    pub lens: Option<Lens>,
}

impl Mapping {
    pub fn to_pixel(&self, bed: [f64; 2]) -> [f64; 2] {
        let u = transform(&self.h, bed);
        self.lens.as_ref().map_or(u, |lens| lens.distort(u))
    }

    fn undistort(&self, p: [f64; 2]) -> [f64; 2] {
        self.lens.as_ref().map_or(p, |lens| lens.undistort(p))
    }

    pub fn to_bed(&self, pixel: [f64; 2]) -> Option<[f64; 2]> {
        Some(transform(&invert(&self.h)?, self.undistort(pixel)))
    }
}

fn invert(m: &Matrix) -> Option<Matrix> {
    let c = |r: usize, s: usize| {
        let (r1, r2, s1, s2) = ((r + 1) % 3, (r + 2) % 3, (s + 1) % 3, (s + 2) % 3);
        m[r1][s1] * m[r2][s2] - m[r1][s2] * m[r2][s1]
    };
    let det = m[0][0] * c(0, 0) + m[0][1] * c(0, 1) + m[0][2] * c(0, 2);
    (det.abs() > 1e-300).then(|| std::array::from_fn(|i| std::array::from_fn(|j| c(j, i) / det)))
}

/// Bed-to-pixel mapping of a calibration, with its lens if it has one.
pub fn mapping(calibration: &CameraCalibration) -> Result<Mapping, String> {
    let lens = calibration.lens.clone();
    let view = calibration
        .view_points
        .iter()
        .map(|p| lens.as_ref().map_or(*p, |lens| lens.undistort(*p)))
        .collect();
    let h = homography(&CameraCalibration {
        reference_points: calibration.reference_points.clone(),
        view_points: view,
        ..Default::default()
    })?;
    Ok(Mapping { h, lens })
}

/// Distance (pixels) between every view point and where the calibration maps
/// its reference point.
pub fn residuals(calibration: &CameraCalibration) -> Result<Vec<f64>, String> {
    let mapping = mapping(calibration)?;
    Ok(calibration
        .reference_points
        .iter()
        .zip(&calibration.view_points)
        .map(|(r, v)| {
            let p = mapping.to_pixel(*r);
            (p[0] - v[0]).hypot(p[1] - v[1])
        })
        .collect())
}

/// Lens of `calibration`, starting from the profile `start`: from 6 points the
/// crosses determine k1, from 10 also k2 (least squares in camera pixels).
pub fn fit_lens(calibration: &CameraCalibration, start: Lens) -> Lens {
    let n = calibration
        .reference_points
        .len()
        .min(calibration.view_points.len());
    let cost = |lens: &Lens| {
        let mut c = calibration.clone();
        c.lens = Some(lens.clone());
        residuals(&c).map_or(f64::MAX, |r| r.iter().map(|d| d * d).sum())
    };
    // Grob absuchen, dann mit Goldenem Schnitt verfeinern.
    let search = |lens: &mut Lens, k2: bool| {
        let get = |l: &Lens| if k2 { l.k2 } else { l.k1 };
        let set = |l: &mut Lens, v: f64| if k2 { l.k2 = v } else { l.k1 = v };
        let at = |l: &Lens, v: f64| {
            let mut l = l.clone();
            set(&mut l, v);
            cost(&l)
        };
        let mut best = (cost(lens), get(lens));
        for i in -50..=50 {
            let v = i as f64 * 0.01;
            let c = at(lens, v);
            if c < best.0 {
                best = (c, v);
            }
        }
        let (mut a, mut b) = (best.1 - 0.01, best.1 + 0.01);
        let g = (5f64.sqrt() - 1.0) / 2.0;
        for _ in 0..40 {
            let (x1, x2) = (b - g * (b - a), a + g * (b - a));
            if at(lens, x1) < at(lens, x2) {
                b = x2;
            } else {
                a = x1;
            }
        }
        let v = (a + b) / 2.0;
        if at(lens, v) < best.0 {
            best.1 = v;
        }
        set(lens, best.1);
    };
    let mut lens = start;
    if n >= 6 {
        search(&mut lens, false);
    }
    if n >= 10 {
        for _ in 0..6 {
            search(&mut lens, true);
            search(&mut lens, false);
        }
    }
    lens
}

/// Projects the camera picture onto the bed. Like VisiCut, the output scale is
/// the larger of the camera's x/y resolution on the bed, capped at `max_width`.
pub fn correct(
    input: &RgbaImage,
    mapping: &Mapping,
    bed_width_mm: f64,
    bed_height_mm: f64,
    max_width: u32,
) -> RgbaImage {
    let top = mapping.to_pixel([0.0, 0.0]);
    let bottom = mapping.to_pixel([bed_width_mm, bed_height_mm]);
    let scale = ((top[0] - bottom[0]).abs() / bed_width_mm)
        .max((top[1] - bottom[1]).abs() / bed_height_mm)
        .min(max_width as f64 / bed_width_mm)
        .max(0.1);
    let width = (bed_width_mm * scale) as u32;
    let height = (bed_height_mm * scale) as u32;
    RgbaImage::from_fn(width.max(1), height.max(1), |x, y| {
        let p = mapping.to_pixel([(x as f64 + 0.5) / scale, (y as f64 + 0.5) / scale]);
        if p[0] >= 0.0 && p[1] >= 0.0 && p[0] < input.width() as f64 && p[1] < input.height() as f64
        {
            *input.get_pixel(p[0] as u32, p[1] as u32)
        } else {
            image::Rgba([128, 128, 128, 255])
        }
    })
}

pub fn encode_png(image: &RgbaImage) -> Result<Vec<u8>, String> {
    let mut png = Vec::new();
    image
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(png)
}

pub fn decode(bytes: &[u8]) -> Result<RgbaImage, String> {
    image::load_from_memory(bytes)
        .map(|i| i.to_rgba8())
        .map_err(|e| tr!("Kamerabild nicht lesbar: {error}", error = e))
}

pub fn capture(device: &LaserDevice) -> Result<RgbaImage, String> {
    if device.camera_url.trim().is_empty() {
        return Err(tr!(
            "{device}: keine Kamera-URL eingetragen",
            device = device.name
        ));
    }
    decode(&fetch_with(
        device.camera_url.trim(),
        30 * 1024 * 1024,
        Some((&device.camera_user, &device.camera_password)),
    )?)
}

/// Connected area of equal pixels (dark or light) with its bounding box.
struct Blob {
    area: f64,
    sum: [f64; 2],
    min: [usize; 2],
    max: [usize; 2],
}

impl Blob {
    fn size(&self) -> [f64; 2] {
        [
            (self.max[0] - self.min[0] + 1) as f64,
            (self.max[1] - self.min[1] + 1) as f64,
        ]
    }

    fn center(&self) -> [f64; 2] {
        [self.sum[0] / self.area, self.sum[1] / self.area]
    }

    fn diameter(&self) -> f64 {
        let [w, h] = self.size();
        w.max(h)
    }

    fn fill(&self) -> f64 {
        let [w, h] = self.size();
        self.area / (w * h)
    }
}

/// Labels the 4-connected areas where `mask` equals `value`.
fn label(mask: &[bool], w: usize, value: bool) -> (Vec<u32>, Vec<Blob>) {
    let mut labels = vec![u32::MAX; mask.len()];
    let mut blobs = Vec::new();
    let mut stack = Vec::new();
    for start in 0..mask.len() {
        if mask[start] != value || labels[start] != u32::MAX {
            continue;
        }
        let id = blobs.len() as u32;
        let mut b = Blob {
            area: 0.0,
            sum: [0.0; 2],
            min: [usize::MAX; 2],
            max: [0; 2],
        };
        labels[start] = id;
        stack.push(start);
        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            b.area += 1.0;
            b.sum = [b.sum[0] + x as f64, b.sum[1] + y as f64];
            b.min = [b.min[0].min(x), b.min[1].min(y)];
            b.max = [b.max[0].max(x), b.max[1].max(y)];
            let neighbours = [
                (x > 0).then(|| i - 1),
                (x + 1 < w).then(|| i + 1),
                (y > 0).then(|| i - w),
                (i + w < mask.len()).then(|| i + w),
            ];
            for j in neighbours.into_iter().flatten() {
                if mask[j] == value && labels[j] == u32::MAX {
                    labels[j] = id;
                    stack.push(j);
                }
            }
        }
        blobs.push(b);
    }
    (labels, blobs)
}

/// Finds VisiCam-style fiducials (dark ring with a dark dot in its centre on a
/// light sticker) and returns the dot centres in image pixels. The light gap
/// between ring and dot is searched, because the ring itself often touches a
/// dark surrounding.
pub fn find_markers(image: &RgbaImage) -> Vec<[f64; 2]> {
    let (w, h) = (image.width() as usize, image.height() as usize);
    if w < 32 || h < 32 {
        return Vec::new();
    }
    let gray: Vec<f32> = image
        .pixels()
        .map(|p| 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32)
        .collect();
    // Schwelle: Mitte zwischen hellstem und dunkelstem Wert der Umgebung
    // (Blöcke, je 3 × 3 Blöcke um den Pixel); kontrastarme Flächen sind hell.
    let block = (w.max(h) / 125).max(8);
    let (bw, bh) = (w.div_ceil(block), h.div_ceil(block));
    let (mut low, mut high) = (vec![f32::MAX; bw * bh], vec![f32::MIN; bw * bh]);
    for (i, v) in gray.iter().enumerate() {
        let b = (i / w / block) * bw + (i % w) / block;
        low[b] = low[b].min(*v);
        high[b] = high[b].max(*v);
    }
    let around = |values: &[f32], bx: usize, by: usize, pick: fn(f32, f32) -> f32| {
        let mut r = values[by * bw + bx];
        for y in by.saturating_sub(1)..(by + 2).min(bh) {
            for x in bx.saturating_sub(1)..(bx + 2).min(bw) {
                r = pick(r, values[y * bw + x]);
            }
        }
        r
    };
    let threshold: Vec<Option<f32>> = (0..bw * bh)
        .map(|b| {
            let (lo, hi) = (
                around(&low, b % bw, b / bw, f32::min),
                around(&high, b % bw, b / bw, f32::max),
            );
            (hi - lo > 25.0).then_some((hi + lo) / 2.0)
        })
        .collect();
    let dark: Vec<bool> = gray
        .iter()
        .enumerate()
        .map(|(i, v)| threshold[(i / w / block) * bw + (i % w) / block].is_some_and(|t| *v < t))
        .collect();
    let (dark_labels, dark_blobs) = label(&dark, w, true);
    let (_, light_blobs) = label(&dark, w, false);
    let largest = w.max(h) as f64;
    let mut markers = Vec::new();
    for gap in &light_blobs {
        let ([gw, gh], d, c) = (gap.size(), gap.diameter(), gap.center());
        let middle = [
            (gap.min[0] + gap.max[0]) as f64 / 2.0,
            (gap.min[1] + gap.max[1]) as f64 / 2.0,
        ];
        // Heller Kreisring: rund, mittig hohl, sinnvolle Größe.
        if d < 0.008 * largest
            || d > largest / 15.0
            || gw / gh > 1.8
            || gh / gw > 1.8
            || !(0.3..0.8).contains(&gap.fill())
            || (c[0] - middle[0]).hypot(c[1] - middle[1]) > 0.1 * d
        {
            continue;
        }
        // In der Mitte der dunkle Punkt …
        let Some(dot) = dark_labels
            .get(c[1].round() as usize * w + c[0].round() as usize)
            .and_then(|id| dark_blobs.get(*id as usize))
        else {
            continue;
        };
        let o = dot.center();
        if !(0.2..0.55).contains(&(dot.diameter() / d))
            || dot.fill() < 0.5
            || (o[0] - c[0]).hypot(o[1] - c[1]) > 0.1 * d
        {
            continue;
        }
        // … außen herum der dunkle Ring.
        let ring = (0..16)
            .filter(|k| {
                let a = *k as f64 * std::f64::consts::TAU / 16.0;
                let (x, y) = (c[0] + a.cos() * 0.65 * d, c[1] + a.sin() * 0.65 * d);
                x >= 0.0
                    && y >= 0.0
                    && (x as usize) < w
                    && (y as usize) < h
                    && dark[y as usize * w + x as usize]
            })
            .count();
        if ring >= 15 {
            markers.push([o[0] + 0.5, o[1] + 0.5]);
        }
    }
    markers
}

/// Grauwerte eines Fotos für [`find_cross`].
pub struct Gray {
    width: usize,
    height: usize,
    values: Vec<f32>,
}

impl Gray {
    pub fn new(image: &RgbaImage) -> Self {
        Self {
            width: image.width() as usize,
            height: image.height() as usize,
            values: image
                .pixels()
                .map(|p| 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32)
                .collect(),
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }

    fn at(&self, x: f64, y: f64) -> Option<f32> {
        (x >= 0.0 && y >= 0.0 && (x as usize) < self.width && (y as usize) < self.height)
            .then(|| self.values[y as usize * self.width + x as usize])
    }
}

/// Contrast of the weakest of the four arms of a cross at `(x, y)`: how much
/// darker each arm is than the strips beside it, along most of its length
/// (lower third of the samples). Lines, corners and rows of tick marks lack
/// at least one continuous arm and score low.
fn cross_score(gray: &Gray, x: f64, y: f64, arm: f64, side: f64) -> f32 {
    let mut worst = f32::MAX;
    let mut contrast = Vec::with_capacity(arm as usize);
    for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
        contrast.clear();
        let mut t = 3.0;
        while t <= arm {
            let (px, py) = (x + dx * t, y + dy * t);
            let (Some(a), Some(b), Some(c)) = (
                gray.at(px, py),
                gray.at(px - dy * side, py - dx * side),
                gray.at(px + dy * side, py + dx * side),
            ) else {
                return 0.0;
            };
            contrast.push((b + c) / 2.0 - a);
            t += 1.0;
        }
        if contrast.is_empty() {
            return 0.0;
        }
        contrast.sort_by(f32::total_cmp);
        worst = worst.min(contrast[contrast.len() / 3]);
    }
    worst
}

/// Finds the centre of a marked calibration cross near `around` (pixels).
/// `arm` is half the cross length and `radius` the search distance, both in
/// pixels. Returns the sub-pixel centre, or `None` without a clear cross.
pub fn find_cross(gray: &Gray, around: [f64; 2], radius: f64, arm: f64) -> Option<[f64; 2]> {
    let side = (arm / 4.0).clamp(3.0, 8.0);
    let arm = arm * 0.9;
    let r = radius.round() as i64;
    let scores: Vec<(f32, [f64; 2])> = (-r..=r)
        .flat_map(|dy| (-r..=r).map(move |dx| (dx, dy)))
        .map(|(dx, dy)| {
            let (x, y) = (around[0].round() + dx as f64, around[1].round() + dy as f64);
            (cross_score(gray, x, y, arm, side), [x, y])
        })
        .collect();
    let best = scores
        .iter()
        .cloned()
        .fold((0.0f32, [0.0; 2]), |a, b| if b.0 > a.0 { b } else { a });
    // Blass gelaserte Kreuze haben wenig Kontrast: es zählt, dass das Kreuz
    // sich klar vom Zweitbesten abseits davon abhebt.
    let second = scores
        .iter()
        .filter(|(_, p)| (p[0] - best.1[0]).abs().max((p[1] - best.1[1]).abs()) > 1.5 * arm)
        .map(|(s, _)| *s)
        .fold(0.0f32, f32::max);
    if best.0 < 3.0 || best.0 < 1.8 * second {
        return None;
    }
    // Feinlage: Schwerpunkt der Dunkelheit quer zu den Armen.
    let [x, y] = best.1;
    let (mut sx, mut wx, mut sy, mut wy) = (0.0, 0.0, 0.0, 0.0);
    let mut t = 3.0;
    while t <= arm {
        for sign in [-1.0, 1.0] {
            for k in -side as i64..=side as i64 {
                let k = k as f64;
                let base_v = (gray.at(x - side - 2.0, y + sign * t)?
                    + gray.at(x + side + 2.0, y + sign * t)?)
                    / 2.0;
                let w = (base_v - gray.at(x + k, y + sign * t)?).max(0.0) as f64;
                sx += (x + k) * w;
                wx += w;
                let base_h = (gray.at(x + sign * t, y - side - 2.0)?
                    + gray.at(x + sign * t, y + side + 2.0)?)
                    / 2.0;
                let w = (base_h - gray.at(x + sign * t, y + k)?).max(0.0) as f64;
                sy += (y + k) * w;
                wy += w;
            }
        }
        t += 1.0;
    }
    (wx > 0.0 && wy > 0.0).then(|| [sx / wx + 0.5, sy / wy + 0.5])
}

/// Bed positions (mm) of markers seen at `pixels` in the calibration photo,
/// so later pictures can be re-aligned on them.
pub fn markers_on_bed(
    pixels: &[[f64; 2]],
    calibration: &CameraCalibration,
) -> Result<Vec<[f64; 2]>, String> {
    let mapping = mapping(calibration)?;
    pixels
        .iter()
        .map(|p| {
            mapping.to_bed(*p).ok_or_else(|| {
                tr("Kalibrierpunkte sind degeneriert (z. B. drei auf einer Linie)").into()
            })
        })
        .collect()
}

/// Affine (3 points) or similarity (2 points) transform mapping `from` onto `to`.
fn fit_affine(from: &[[f64; 2]], to: &[[f64; 2]]) -> Option<Matrix> {
    match from.len() {
        2 => {
            let (a, b) = (
                [from[1][0] - from[0][0], from[1][1] - from[0][1]],
                [to[1][0] - to[0][0], to[1][1] - to[0][1]],
            );
            let n = a[0] * a[0] + a[1] * a[1];
            if n < 1e-9 {
                return None;
            }
            let (c, s) = (
                (a[0] * b[0] + a[1] * b[1]) / n,
                (a[0] * b[1] - a[1] * b[0]) / n,
            );
            Some([
                [c, -s, to[0][0] - c * from[0][0] + s * from[0][1]],
                [s, c, to[0][1] - s * from[0][0] - c * from[0][1]],
                [0.0, 0.0, 1.0],
            ])
        }
        3 => {
            let [p, q, r] = [from[0], from[1], from[2]];
            let det = (q[0] - p[0]) * (r[1] - p[1]) - (r[0] - p[0]) * (q[1] - p[1]);
            if det.abs() < 1e-9 {
                return None;
            }
            // Spalten: Bild der x- und y-Einheit aus den beiden Kantenvektoren.
            let inv = [
                [(r[1] - p[1]) / det, -(r[0] - p[0]) / det],
                [-(q[1] - p[1]) / det, (q[0] - p[0]) / det],
            ];
            let mut m = [[0.0; 3]; 3];
            for (i, row) in m.iter_mut().take(2).enumerate() {
                let (dq, dr) = (to[1][i] - to[0][i], to[2][i] - to[0][i]);
                row[0] = dq * inv[0][0] + dr * inv[1][0];
                row[1] = dq * inv[0][1] + dr * inv[1][1];
                row[2] = to[0][i] - row[0] * p[0] - row[1] * p[1];
            }
            m[2][2] = 1.0;
            Some(m)
        }
        _ => None,
    }
}

/// Mapping for this picture: the calibration, re-aligned on the markers
/// found in it when the calibration knows their bed positions (the camera
/// moves a little whenever the lid is closed). Returns how many matched.
pub fn tracked_mapping(
    calibration: &CameraCalibration,
    picture: &RgbaImage,
) -> Result<(Mapping, usize), String> {
    let base = mapping(calibration)?;
    if calibration.marker_points.len() < 2 {
        return Ok((base, 0));
    }
    let found = find_markers(picture);
    let tolerance = 0.08 * picture.width().max(picture.height()) as f64;
    let mut used = vec![false; found.len()];
    let (mut bed, mut expected, mut seen) = (Vec::new(), Vec::new(), Vec::new());
    for marker in &calibration.marker_points {
        let p = base.to_pixel(*marker);
        let nearest = found
            .iter()
            .enumerate()
            .filter(|(i, _)| !used[*i])
            .map(|(i, f)| (i, (f[0] - p[0]).hypot(f[1] - p[1])))
            .filter(|(_, d)| *d < tolerance)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((i, _)) = nearest {
            used[i] = true;
            bed.push(*marker);
            // Nachführen auf unverzerrten Pixeln.
            expected.push(transform(&base.h, *marker));
            seen.push(base.undistort(found[i]));
        }
    }
    // Ab 5 Marken die schlechteste verwerfen, solange sie nicht passt (z. B.
    // eine Marke auf dem Portal, das seit der Kalibrierung verfahren ist).
    let tolerance = 0.004 * picture.width().max(picture.height()) as f64;
    while seen.len() >= 5 {
        let Ok(h) = homography(&CameraCalibration {
            reference_points: bed.clone(),
            view_points: seen.clone(),
            ..Default::default()
        }) else {
            break;
        };
        let (worst, error) = bed
            .iter()
            .zip(&seen)
            .map(|(b, s)| {
                let p = transform(&h, *b);
                (p[0] - s[0]).hypot(p[1] - s[1])
            })
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap();
        if error <= tolerance {
            break;
        }
        bed.remove(worst);
        expected.remove(worst);
        seen.remove(worst);
    }
    let tracked = match seen.len() {
        0 | 1 => None,
        2 | 3 => fit_affine(&expected, &seen).map(|a| multiply(&a, &base.h)),
        _ => homography(&CameraCalibration {
            reference_points: bed,
            view_points: seen.clone(),
            ..Default::default()
        })
        .ok(),
    };
    Ok(match tracked {
        Some(h) => (Mapping { h, lens: base.lens }, seen.len()),
        None => (base, 0),
    })
}

/// Ring marks in a fresh camera picture, in bed millimetres: the cut marks of
/// a printed sheet. Marks fixed on the bed (tracking markers of the
/// calibration) are left out.
pub fn sheet_marks_on_bed(device: &LaserDevice) -> Result<Vec<[f64; 2]>, String> {
    let calibration = device
        .camera_calibration
        .as_ref()
        .ok_or(tr("Kamera ist noch nicht kalibriert"))?;
    let picture = capture(device)?;
    let (mapping, _) = tracked_mapping(calibration, &picture)?;
    Ok(find_markers(&picture)
        .into_iter()
        .filter_map(|p| mapping.to_bed(p))
        .filter(|p| {
            !calibration
                .marker_points
                .iter()
                .any(|m| (m[0] - p[0]).hypot(m[1] - p[1]) < 20.0)
        })
        .collect())
}

/// Camera picture projected onto the bed.
pub fn background_image(
    device: &LaserDevice,
    bed_width_mm: f64,
    bed_height_mm: f64,
) -> Result<RgbaImage, String> {
    let calibration = device
        .camera_calibration
        .as_ref()
        .ok_or(tr("Kamera ist noch nicht kalibriert"))?;
    let picture = capture(device)?;
    let (mapping, _) = tracked_mapping(calibration, &picture)?;
    Ok(correct(
        &picture,
        &mapping,
        bed_width_mm,
        bed_height_mm,
        2400,
    ))
}

/// Bed-aligned camera background as PNG.
pub fn background(
    device: &LaserDevice,
    bed_width_mm: f64,
    bed_height_mm: f64,
) -> Result<Vec<u8>, String> {
    encode_png(&background_image(device, bed_width_mm, bed_height_mm)?)
}

/// SVG covering the bed with VisiCut's calibration marks: a 10 mm cross at
/// every reference point and i+1 counting ticks below cross i (every fifth
/// tick is offset, as in CamCalibrationDialog).
pub fn calibration_svg(
    reference_points: &[[f64; 2]],
    bed_width_mm: f64,
    bed_height_mm: f64,
) -> String {
    let mut d = String::new();
    for (i, p) in reference_points.iter().enumerate() {
        let (x, y) = (p[0], p[1]);
        d += &format!(
            "M{} {}H{}M{} {}V{}",
            x - 5.0,
            y,
            x + 5.0,
            x,
            y - 5.0,
            y + 5.0
        );
        for j in 0..=i {
            let tick = x - i as f64 + j as f64 * 2.0;
            let start = if (j + 1) % 5 == 0 {
                x - i as f64 + (j as f64 - 5.0) * 2.0
            } else {
                tick
            };
            d += &format!("M{} {}L{} {}", start, y + 10.0, tick, y + 14.0);
        }
    }
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{bed_width_mm}mm" height="{bed_height_mm}mm" viewBox="0 0 {bed_width_mm} {bed_height_mm}"><path id="Kalibrierkreuze" d="{d}" fill="none" stroke="black" stroke-width="0.2"/></svg>"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Dark picture with VisiCam-style markers (sticker, ring, dot) at `centers`.
    fn picture_with_markers(centers: &[[f64; 2]]) -> RgbaImage {
        RgbaImage::from_fn(1500, 1000, |x, y| {
            let v = centers.iter().find_map(|c| {
                let (dx, dy) = (x as f64 - c[0], y as f64 - c[1]);
                let r = dx.hypot(dy);
                (dx.abs() < 30.0 && dy.abs() < 30.0).then_some(match r {
                    r if r < 5.0 => 40,
                    r if r < 11.0 => 210,
                    r if r < 20.0 => 50,
                    _ => 220,
                })
            });
            let v = v.unwrap_or(if (x / 25 + y / 25) % 2 == 0 { 60 } else { 90 });
            image::Rgba([v, v, v, 255])
        })
    }

    #[test]
    fn finds_cross_centre_but_not_ticks_or_paper_corners() {
        cross_on_paper(120.0);
        // Blass gelasert, kaum Kontrast zum grauen Papier.
        cross_on_paper(18.0);
    }

    fn cross_on_paper(depth: f64) {
        // Papier (hell) mit Ecke bei (60, 60), Kreuz um (150.3, 120.6), Striche darunter.
        let (cx, cy) = (150.3, 120.6);
        let picture = RgbaImage::from_fn(300, 220, |x, y| {
            let (x, y) = (x as f64 + 0.5, y as f64 + 0.5);
            let mut v = if x < 60.0 || y < 60.0 { 40.0 } else { 220.0 };
            let line = |d: f64| (1.0 - d.abs()).max(0.0);
            if (x - cx).abs() < 12.0 {
                v -= depth * line(y - cy);
            }
            if (y - cy).abs() < 12.0 {
                v -= depth * line(x - cx);
            }
            if (y - cy - 24.0).abs() < 5.0
                && [cx - 4.0, cx, cx + 4.0].iter().any(|t| (x - t).abs() < 1.0)
            {
                v -= depth;
            }
            let v = v.clamp(0.0, 255.0) as u8;
            image::Rgba([v, v, v, 255])
        });
        let gray = Gray::new(&picture);
        let found = find_cross(&gray, [cx + 30.0, cy + 25.0], 50.0, 11.5).unwrap();
        assert!(
            (found[0] - cx).abs() < 0.3 && (found[1] - cy).abs() < 0.3,
            "{found:?}"
        );
        // Nur Papierecke und Striche in Reichweite: nichts.
        assert_eq!(find_cross(&gray, [60.0, 60.0], 20.0, 11.5), None);
        assert_eq!(find_cross(&gray, [cx, cy + 26.0], 6.0, 11.5), None);
    }

    #[test]
    fn finds_markers_and_tracks_a_moved_camera() {
        let centers = [
            [200.0, 150.0],
            [1300.0, 160.0],
            [1290.0, 850.0],
            [210.0, 840.0],
        ];
        let mut found = find_markers(&picture_with_markers(&centers));
        found.sort_by(|a, b| {
            (a[1] > 500.0, a[0])
                .partial_cmp(&(b[1] > 500.0, b[0]))
                .unwrap()
        });
        let expected = [centers[0], centers[1], centers[3], centers[2]];
        assert_eq!(found.len(), 4, "{found:?}");
        for (f, e) in found.iter().zip(expected) {
            assert!(
                (f[0] - e[0]).abs() < 1.0 && (f[1] - e[1]).abs() < 1.0,
                "{f:?} vs {e:?}"
            );
        }
        // Kalibrierung: 1 mm = 1.5 px; die Marken liegen dabei bei centers.
        let mut calibration = CameraCalibration {
            reference_points: vec![[0.0, 0.0], [1000.0, 0.0], [1000.0, 600.0], [0.0, 600.0]],
            view_points: vec![[0.0, 0.0], [1500.0, 0.0], [1500.0, 900.0], [0.0, 900.0]],
            marker_points: Vec::new(),
            lens: None,
        };
        calibration.marker_points = markers_on_bed(&centers, &calibration).unwrap();
        assert!((calibration.marker_points[0][0] - 200.0 / 1.5).abs() < 1e-6);
        // Deckel anders geschlossen: alles 40 px nach rechts, 25 px nach unten.
        let moved: Vec<_> = centers.iter().map(|c| [c[0] + 40.0, c[1] + 25.0]).collect();
        let picture = picture_with_markers(&moved);
        for keep in [4, 3, 2] {
            let mut partial = calibration.clone();
            partial.marker_points.truncate(keep);
            let (m, matched) = tracked_mapping(&partial, &picture).unwrap();
            assert_eq!(matched, keep);
            let p = m.to_pixel([500.0, 300.0]);
            assert!(
                (p[0] - 790.0).abs() < 1.5 && (p[1] - 475.0).abs() < 1.5,
                "{keep}: {p:?}"
            );
        }
        // Ohne Passmarken im Bild bleibt die Kalibrierung.
        let (m, matched) = tracked_mapping(&calibration, &picture_with_markers(&[])).unwrap();
        let p = m.to_pixel([500.0, 300.0]);
        assert!(matched == 0 && (p[0] - 750.0).abs() < 1e-6 && (p[1] - 450.0).abs() < 1e-6);
        // Eine fünfte Marke, die sich anders bewegt hat (z. B. auf dem Portal),
        // wird verworfen.
        let mut five = calibration.clone();
        five.marker_points
            .push(markers_on_bed(&[[750.0, 80.0]], &calibration).unwrap()[0]);
        let mut moved_five = moved.clone();
        moved_five.push([750.0 + 40.0 + 60.0, 80.0 + 25.0]);
        let (m, matched) = tracked_mapping(&five, &picture_with_markers(&moved_five)).unwrap();
        let p = m.to_pixel([500.0, 300.0]);
        assert_eq!(matched, 4);
        assert!(
            (p[0] - 790.0).abs() < 1.5 && (p[1] - 475.0).abs() < 1.5,
            "{p:?}"
        );
    }

    #[test]
    fn lens_profile_and_fit_remove_radial_distortion() {
        let size = [3000.0, 2100.0];
        let pi = Lens::raspberry_pi_v2(size);
        assert!((pi.focal - 2482.6).abs() < 1.0, "{}", pi.focal);
        assert_eq!(pi.center, [1500.0, 1050.0]);
        let truth = Lens {
            k1: -0.15,
            k2: 0.03,
            ..pi.clone()
        };
        for p in [
            [10.0, 20.0],
            [1500.0, 1050.0],
            [2990.0, 2090.0],
            [700.0, 1900.0],
        ] {
            let back = truth.undistort(truth.distort(p));
            assert!(
                (back[0] - p[0]).abs() < 1e-2 && (back[1] - p[1]).abs() < 1e-2,
                "{p:?} {back:?}"
            );
        }
        // Kamerabild eines 5 × 3-Rasters durch die verzeichnende Linse.
        let h: Matrix = [[2.3, 0.05, 350.0], [-0.04, 2.35, 380.0], [1e-5, 2e-5, 1.0]];
        let reference: Vec<[f64; 2]> = [50.0, 300.0, 550.0]
            .iter()
            .flat_map(|y| [50.0, 275.0, 500.0, 725.0, 950.0].map(|x| [x, *y]))
            .collect();
        let view = reference
            .iter()
            .map(|r| truth.distort(transform(&h, *r)))
            .collect();
        let mut calibration = CameraCalibration {
            reference_points: reference,
            view_points: view,
            ..Default::default()
        };
        let without = residuals(&calibration)
            .unwrap()
            .into_iter()
            .fold(0.0, f64::max);
        calibration.lens = Some(fit_lens(&calibration, Lens::raspberry_pi_v2(size)));
        let with = residuals(&calibration)
            .unwrap()
            .into_iter()
            .fold(0.0, f64::max);
        assert!(without > 5.0 && with < 0.2, "{without} {with}");
        // Die Abbildung trifft auch Punkte zwischen den Kreuzen.
        let m = mapping(&calibration).unwrap();
        let (p, q) = (
            m.to_pixel([160.0, 420.0]),
            truth.distort(transform(&h, [160.0, 420.0])),
        );
        assert!((p[0] - q[0]).hypot(p[1] - q[1]) < 0.3, "{p:?} {q:?}");
        let bed = m.to_bed(q).unwrap();
        assert!(
            (bed[0] - 160.0).abs() < 0.15 && (bed[1] - 420.0).abs() < 0.15,
            "{bed:?}"
        );
    }

    fn fau() -> CameraCalibration {
        LaserDevice::fau().camera_calibration.unwrap()
    }

    #[test]
    fn fau_homography_matches_stored_visicut_matrix() {
        // Matrix stored by VisiCut in FAU-LTT-iLaser-4000.xml (any scale).
        let stored: Matrix = [
            [
                -0.002279943143889108,
                -5.653216852574587E-7,
                -0.8977332688859805,
            ],
            [
                1.552885058779451E-4,
                -0.004344847074525539,
                -0.4405094107842953,
            ],
            [
                1.966192053363422E-7,
                1.3178101972094041E-8,
                -0.0015287230114782674,
            ],
        ];
        let computed = homography(&fau()).unwrap();
        for p in [[0.0, 0.0], [500.0, 300.0], [1000.0, 600.0], [200.0, 480.0]] {
            let a = transform(&stored, p);
            let b = transform(&computed, p);
            assert!(
                (a[0] - b[0]).abs() < 0.5 && (a[1] - b[1]).abs() < 0.5,
                "{p:?}: {a:?} vs {b:?}"
            );
        }
    }

    #[test]
    fn exact_four_point_homography_reproduces_points_and_warps() {
        let calibration = CameraCalibration {
            reference_points: vec![[0.0, 0.0], [100.0, 0.0], [100.0, 50.0], [0.0, 50.0]],
            view_points: vec![[10.0, 20.0], [210.0, 25.0], [205.0, 130.0], [12.0, 120.0]],
            marker_points: Vec::new(),
            lens: None,
        };
        let h = homography(&calibration).unwrap();
        for (r, v) in calibration
            .reference_points
            .iter()
            .zip(&calibration.view_points)
        {
            let p = transform(&h, *r);
            assert!((p[0] - v[0]).abs() < 1e-6 && (p[1] - v[1]).abs() < 1e-6);
        }
        let mut camera = RgbaImage::from_pixel(220, 140, image::Rgba([0, 0, 0, 255]));
        camera.put_pixel(110, 72, image::Rgba([255, 0, 0, 255]));
        let bed = correct(&camera, &Mapping { h, lens: None }, 100.0, 50.0, 400);
        assert!(bed.width() >= 190 && bed.width() <= 400);
        assert!(bed.pixels().any(|p| p.0 == [255, 0, 0, 255]));
        assert!(decode(&encode_png(&bed).unwrap()).is_ok());
    }

    #[test]
    fn rejects_degenerate_points_and_two_point_legacy_is_affine() {
        let line = CameraCalibration {
            reference_points: vec![[0.0, 0.0], [1.0, 1.0], [2.0, 2.0], [3.0, 3.0]],
            view_points: vec![[0.0, 0.0], [1.0, 1.0], [2.0, 2.0], [3.0, 3.0]],
            marker_points: Vec::new(),
            lens: None,
        };
        assert!(homography(&line).unwrap_err().contains("Linie"));
        let legacy = CameraCalibration {
            reference_points: vec![[200.0, 120.0], [800.0, 480.0]],
            view_points: vec![[100.0, 60.0], [400.0, 240.0]],
            marker_points: Vec::new(),
            lens: None,
        };
        let h = homography(&legacy).unwrap();
        assert_eq!(transform(&h, [500.0, 300.0]), [250.0, 150.0]);
    }

    #[test]
    fn discovery_collects_visicam_answers() {
        // Two fake cameras on loopback answer the request; one sends junk.
        let mut targets = Vec::new();
        let mut threads = Vec::new();
        for answer in [
            "http://192.168.0.5:8080/image\0\0",
            "kein Link",
            "http://192.168.0.5:8080/image",
        ] {
            let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
            targets.push(socket.local_addr().unwrap());
            threads.push(std::thread::spawn(move || {
                let mut buffer = [0u8; 64];
                let (length, from) = socket.recv_from(&mut buffer).unwrap();
                assert_eq!(&buffer[..length], DISCOVERY_REQUEST);
                socket.send_to(answer.as_bytes(), from).unwrap();
            }));
        }
        let found = discover_at(&targets, Duration::from_millis(500));
        for thread in threads {
            thread.join().unwrap();
        }
        assert_eq!(found, vec!["http://192.168.0.5:8080/image".to_string()]);
    }

    #[test]
    fn camera_login_is_sent_as_http_basic_like_visicut() {
        assert_eq!(basic_auth("", "geheim"), None);
        assert_eq!(basic_auth("laser", ""), None);
        // A one-shot HTTP server that answers only with the right header.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/bild.png", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            use std::io::{BufRead, Write};
            let mut answers = Vec::new();
            for _ in 0..2 {
                let (stream, _) = listener.accept().unwrap();
                let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
                let mut authorized = false;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line.trim().is_empty() {
                        break;
                    }
                    let lower = line.to_ascii_lowercase();
                    authorized |= lower.starts_with("authorization: basic bgfzzxi6z2vozwlt");
                }
                answers.push(authorized);
                let body = if authorized { &b"ok"[..] } else { &b"no"[..] };
                let status = if authorized {
                    "200 OK"
                } else {
                    "401 Unauthorized"
                };
                let mut stream = stream;
                write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .unwrap();
                stream.write_all(body).unwrap();
            }
            answers
        });
        assert!(fetch_with(&url, 1024, None).is_err());
        assert_eq!(
            fetch_with(&url, 1024, Some(("laser", "geheim"))).unwrap(),
            b"ok"
        );
        assert_eq!(server.join().unwrap(), vec![false, true]);
    }

    #[test]
    fn local_camera_file_and_calibration_page() {
        let dir = std::env::temp_dir().join(format!("visicut-camera-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("camera.png");
        encode_png(&RgbaImage::from_pixel(40, 30, image::Rgba([1, 2, 3, 255])))
            .and_then(|png| std::fs::write(&path, png).map_err(|e| e.to_string()))
            .unwrap();
        let mut device = LaserDevice {
            camera_url: format!("file://{}", path.display()),
            camera_calibration: Some(CameraCalibration {
                reference_points: vec![[0.0, 0.0], [1000.0, 600.0]],
                view_points: vec![[0.0, 0.0], [40.0, 30.0]],
                marker_points: Vec::new(),
                lens: None,
            }),
            ..Default::default()
        };
        let png = background(&device, 1000.0, 600.0).unwrap();
        assert!(decode(&png).unwrap().width() >= 40);
        device.camera_url.clear();
        assert!(background(&device, 1000.0, 600.0).is_err());
        assert!(fetch(path.to_str().unwrap(), 10).is_err());
        std::fs::remove_dir_all(dir).unwrap();

        let svg = calibration_svg(&fau().reference_points, 1000.0, 600.0);
        let project = crate::project::Project {
            svg,
            x_mm: 0.0,
            y_mm: 0.0,
            width_mm: 1000.0,
            height_mm: 600.0,
            ..Default::default()
        };
        // Two strokes per cross plus 1 + 2 + ... + 6 ticks.
        assert_eq!(
            crate::geometry::contours(&project).unwrap().len(),
            6 * 2 + 21
        );
    }
}
