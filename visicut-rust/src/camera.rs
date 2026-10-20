//! Camera background for the bed: port of VisiCut's Homography.java.
//! A homography maps bed positions (mm) to camera pixels; the corrected image
//! samples the camera picture for every bed position.
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

fn transform(h: &Matrix, p: [f64; 2]) -> [f64; 2] {
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

/// Projects the camera picture onto the bed. Like VisiCut, the output scale is
/// the larger of the camera's x/y resolution on the bed, capped at `max_width`.
pub fn correct(
    input: &RgbaImage,
    h: &Matrix,
    bed_width_mm: f64,
    bed_height_mm: f64,
    max_width: u32,
) -> RgbaImage {
    let top = transform(h, [0.0, 0.0]);
    let bottom = transform(h, [bed_width_mm, bed_height_mm]);
    let scale = ((top[0] - bottom[0]).abs() / bed_width_mm)
        .max((top[1] - bottom[1]).abs() / bed_height_mm)
        .min(max_width as f64 / bed_width_mm)
        .max(0.1);
    let width = (bed_width_mm * scale) as u32;
    let height = (bed_height_mm * scale) as u32;
    RgbaImage::from_fn(width.max(1), height.max(1), |x, y| {
        let p = transform(h, [(x as f64 + 0.5) / scale, (y as f64 + 0.5) / scale]);
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

/// Bed positions (mm) of markers seen at `pixels` in the calibration photo,
/// so later pictures can be re-aligned on them.
pub fn markers_on_bed(
    pixels: &[[f64; 2]],
    calibration: &CameraCalibration,
) -> Result<Vec<[f64; 2]>, String> {
    let inverse = homography(&CameraCalibration {
        reference_points: calibration.view_points.clone(),
        view_points: calibration.reference_points.clone(),
        marker_points: Vec::new(),
    })?;
    Ok(pixels.iter().map(|p| transform(&inverse, *p)).collect())
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

/// Homography for this picture: the calibration, re-aligned on the markers
/// found in it when the calibration knows their bed positions (the camera
/// moves a little whenever the lid is closed). Returns how many matched.
pub fn tracked_homography(
    calibration: &CameraCalibration,
    picture: &RgbaImage,
) -> Result<(Matrix, usize), String> {
    let h = homography(calibration)?;
    if calibration.marker_points.len() < 2 {
        return Ok((h, 0));
    }
    let found = find_markers(picture);
    let tolerance = 0.08 * picture.width().max(picture.height()) as f64;
    let mut used = vec![false; found.len()];
    let (mut bed, mut expected, mut seen) = (Vec::new(), Vec::new(), Vec::new());
    for marker in &calibration.marker_points {
        let p = transform(&h, *marker);
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
            expected.push(p);
            seen.push(found[i]);
        }
    }
    let tracked = match seen.len() {
        0 | 1 => None,
        2 | 3 => fit_affine(&expected, &seen).map(|a| multiply(&a, &h)),
        _ => homography(&CameraCalibration {
            reference_points: bed,
            view_points: seen.clone(),
            marker_points: Vec::new(),
        })
        .ok(),
    };
    Ok(match tracked {
        Some(m) => (m, seen.len()),
        None => (h, 0),
    })
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
    let (h, _) = tracked_homography(calibration, &picture)?;
    Ok(correct(&picture, &h, bed_width_mm, bed_height_mm, 2400))
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
        };
        calibration.marker_points = markers_on_bed(&centers, &calibration).unwrap();
        assert!((calibration.marker_points[0][0] - 200.0 / 1.5).abs() < 1e-6);
        // Deckel anders geschlossen: alles 40 px nach rechts, 25 px nach unten.
        let moved: Vec<_> = centers.iter().map(|c| [c[0] + 40.0, c[1] + 25.0]).collect();
        let picture = picture_with_markers(&moved);
        for keep in [4, 3, 2] {
            let mut partial = calibration.clone();
            partial.marker_points.truncate(keep);
            let (h, matched) = tracked_homography(&partial, &picture).unwrap();
            assert_eq!(matched, keep);
            let p = transform(&h, [500.0, 300.0]);
            assert!(
                (p[0] - 790.0).abs() < 1.5 && (p[1] - 475.0).abs() < 1.5,
                "{keep}: {p:?}"
            );
        }
        // Ohne Passmarken im Bild bleibt die Kalibrierung.
        let (h, matched) = tracked_homography(&calibration, &picture_with_markers(&[])).unwrap();
        let p = transform(&h, [500.0, 300.0]);
        assert!(matched == 0 && (p[0] - 750.0).abs() < 1e-6 && (p[1] - 450.0).abs() < 1e-6);
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
        let bed = correct(&camera, &h, 100.0, 50.0, 400);
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
        };
        assert!(homography(&line).unwrap_err().contains("Linie"));
        let legacy = CameraCalibration {
            reference_points: vec![[200.0, 120.0], [800.0, 480.0]],
            view_points: vec![[100.0, 60.0], [400.0, 240.0]],
            marker_points: Vec::new(),
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
