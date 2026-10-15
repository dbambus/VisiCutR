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
    let h = homography(calibration)?;
    Ok(correct(
        &capture(device)?,
        &h,
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
        };
        assert!(homography(&line).unwrap_err().contains("Linie"));
        let legacy = CameraCalibration {
            reference_points: vec![[200.0, 120.0], [800.0, 480.0]],
            view_points: vec![[100.0, 60.0], [400.0, 240.0]],
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
