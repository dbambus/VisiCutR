//! Objektivkalibrierung mit Schachbrett wie fau-fablab/undistort-webcam, aber
//! ohne Docker und OpenCV: Fotos von der Kamera-URL aufnehmen oder laden,
//! Schachbrett erkennen, kalibrieren und `calibration.npz` speichern.
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use image::RgbaImage;
use std::sync::mpsc::{Receiver, Sender, channel};
use visicut_core::{
    camera, device,
    i18n::tr,
    lens_calibration::{self, Calibration, Intrinsics, PATTERN},
    tr,
};

const ACCENT: Color32 = Color32::from_rgb(192, 98, 30);
/// Breite der Vorschaubilder; das Original bleibt für die Kalibrierung.
const PREVIEW_WIDTH: u32 = 1600;

struct Photo {
    id: u64,
    name: String,
    /// Datei, wie sie von der Kamera kam (zum Speichern der Fotos).
    bytes: Vec<u8>,
    size: [u32; 2],
    preview: RgbaImage,
    corners: Option<Vec<[f64; 2]>>,
    rms: Option<f64>,
}

impl Photo {
    fn scale(&self) -> f64 {
        self.preview.width() as f64 / self.size[0] as f64
    }
}

enum Job {
    Photo(Result<Photo, String>),
    Calibrated(Result<(Calibration, Vec<u64>), String>),
}

#[derive(Default)]
struct Camera {
    name: String,
    url: String,
    user: String,
    password: String,
}

struct App {
    cameras: Vec<Camera>,
    camera: usize,
    photos: Vec<Photo>,
    selected: Option<u64>,
    next_id: u64,
    busy: usize,
    jobs: (Sender<Job>, Receiver<Job>),
    calibration: Option<Calibration>,
    /// Aus einer Datei geladen oder berechnet.
    intrinsics: Option<Intrinsics>,
    undistorted: bool,
    /// Angezeigtes Bild: Foto, entzerrt?, Textur.
    texture: Option<(u64, bool, egui::TextureHandle)>,
    message: Option<String>,
    /// `--capture BILD.png`: nach dem Laden kalibrieren, Bildschirmfoto, Ende.
    #[cfg(feature = "screenshot")]
    capture: Option<(std::path::PathBuf, u32)>,
}

fn load_photo(id: u64, name: String, bytes: Vec<u8>) -> Result<Photo, String> {
    let image = camera::decode(&bytes).map_err(|e| format!("{name}: {e}"))?;
    let corners = lens_calibration::find_chessboard(&image, PATTERN);
    let size = [image.width(), image.height()];
    let preview = if image.width() > PREVIEW_WIDTH {
        let height = (image.height() as f64 * PREVIEW_WIDTH as f64 / image.width() as f64) as u32;
        image::imageops::resize(
            &image,
            PREVIEW_WIDTH,
            height.max(1),
            image::imageops::FilterType::Triangle,
        )
    } else {
        image
    };
    Ok(Photo {
        id,
        name,
        bytes,
        size,
        preview,
        corners,
        rms: None,
    })
}

/// Kameramatrix für das verkleinerte Vorschaubild.
fn scaled(k: &Intrinsics, s: f64) -> Intrinsics {
    Intrinsics {
        fx: k.fx * s,
        fy: k.fy * s,
        cx: (k.cx + 0.5) * s - 0.5,
        cy: (k.cy + 0.5) * s - 0.5,
        dist: k.dist,
    }
}

impl App {
    fn new() -> Self {
        let mut cameras: Vec<Camera> = device::DeviceStore::load(&device::config_dir())
            .map(|store| store.devices)
            .unwrap_or_default()
            .into_iter()
            .filter(|d| !d.camera_url.trim().is_empty())
            .map(|d| Camera {
                name: d.name,
                url: d.camera_url.trim().to_string(),
                user: d.camera_user,
                password: d.camera_password,
            })
            .collect();
        cameras.push(Camera {
            name: tr("Andere Adresse").into(),
            ..Default::default()
        });
        Self {
            cameras,
            camera: 0,
            photos: Vec::new(),
            selected: None,
            next_id: 0,
            busy: 0,
            jobs: channel(),
            calibration: None,
            intrinsics: None,
            undistorted: false,
            texture: None,
            message: None,
            #[cfg(feature = "screenshot")]
            capture: None,
        }
    }

    fn spawn(&mut self, ctx: &egui::Context, work: impl FnOnce() -> Job + Send + 'static) {
        let sender = self.jobs.0.clone();
        let ctx = ctx.clone();
        self.busy += 1;
        std::thread::spawn(move || {
            let _ = sender.send(work());
            ctx.request_repaint();
        });
    }

    fn take_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    fn capture(&mut self, ctx: &egui::Context) {
        let camera = &self.cameras[self.camera];
        let (url, user, password) = (
            camera.url.trim().to_string(),
            camera.user.clone(),
            camera.password.clone(),
        );
        if url.is_empty() {
            self.message = Some(tr("Bitte eine Kamera-URL eintragen").into());
            return;
        }
        let id = self.take_id();
        let number = self.photos.len() + self.busy + 1;
        self.spawn(ctx, move || {
            Job::Photo(
                camera::fetch_with(&url, 64 * 1024 * 1024, Some((&user, &password))).and_then(
                    |bytes| {
                        let extension = match image::guess_format(&bytes) {
                            Ok(image::ImageFormat::Png) => "png",
                            _ => "jpg",
                        };
                        load_photo(id, format!("undistort-calib-{number}.{extension}"), bytes)
                    },
                ),
            )
        });
    }

    fn open_photos(&mut self, ctx: &egui::Context) {
        let Some(paths) = rfd::FileDialog::new()
            .add_filter(tr("Bilder"), &["jpg", "jpeg", "png"])
            .pick_files()
        else {
            return;
        };
        self.load_files(ctx, paths);
    }

    fn load_files(&mut self, ctx: &egui::Context, paths: Vec<std::path::PathBuf>) {
        for path in paths {
            let id = self.take_id();
            self.spawn(ctx, move || {
                let name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                Job::Photo(
                    std::fs::read(&path)
                        .map_err(|e| format!("{}: {e}", path.display()))
                        .and_then(|bytes| load_photo(id, name, bytes)),
                )
            });
        }
    }

    fn save_photos(&mut self) {
        let Some(dir) = rfd::FileDialog::new().pick_folder() else {
            return;
        };
        let result: Result<(), String> = self.photos.iter().try_for_each(|p| {
            let path = dir.join(&p.name);
            std::fs::write(&path, &p.bytes).map_err(|e| format!("{}: {e}", path.display()))
        });
        self.message = Some(match result {
            Ok(()) => tr!("{count} Fotos gespeichert", count = self.photos.len()),
            Err(e) => e,
        });
    }

    fn calibrate(&mut self, ctx: &egui::Context) {
        let mut views = Vec::new();
        let mut ids = Vec::new();
        let mut size = None;
        for photo in &self.photos {
            if let Some(corners) = &photo.corners {
                if size.is_some_and(|s| s != photo.size) {
                    self.message = Some(tr("Alle Fotos müssen gleich groß sein").into());
                    return;
                }
                size = Some(photo.size);
                views.push(corners.clone());
                ids.push(photo.id);
            }
        }
        let Some(size) = size else {
            return;
        };
        self.spawn(ctx, move || {
            Job::Calibrated(lens_calibration::calibrate(&views, PATTERN, size).map(|c| (c, ids)))
        });
    }

    fn save_npz(&mut self) {
        let Some(calibration) = &self.calibration else {
            return;
        };
        let Some(path) = rfd::FileDialog::new()
            .set_file_name("calibration.npz")
            .add_filter("NumPy", &["npz"])
            .save_file()
        else {
            return;
        };
        let result = lens_calibration::to_npz(calibration)
            .and_then(|bytes| std::fs::write(&path, bytes).map_err(|e| e.to_string()));
        self.message = Some(match result {
            Ok(()) => tr!("Gespeichert: {path}", path = path.display()),
            Err(e) => format!("{}: {e}", path.display()),
        });
    }

    fn open_npz(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("NumPy", &["npz"])
            .pick_file()
        else {
            return;
        };
        match std::fs::read(&path)
            .map_err(|e| e.to_string())
            .and_then(|b| lens_calibration::read_npz(&b))
        {
            Ok(k) => {
                self.intrinsics = Some(k);
                self.calibration = None;
                self.photos.iter_mut().for_each(|p| p.rms = None);
                self.undistorted = true;
                self.texture = None;
                self.message = Some(tr!("Geladen: {path}", path = path.display()));
            }
            Err(e) => self.message = Some(format!("{}: {e}", path.display())),
        }
    }

    fn receive(&mut self) {
        while let Ok(job) = self.jobs.1.try_recv() {
            self.busy -= 1;
            match job {
                Job::Photo(Ok(photo)) => {
                    if photo.corners.is_none() {
                        self.message =
                            Some(tr!("{name}: kein Schachbrett erkannt", name = photo.name));
                    }
                    self.selected = Some(photo.id);
                    let at = self.photos.partition_point(|p| p.id < photo.id);
                    self.photos.insert(at, photo);
                }
                Job::Photo(Err(e)) => self.message = Some(e),
                Job::Calibrated(Ok((calibration, ids))) => {
                    for photo in &mut self.photos {
                        photo.rms = ids
                            .iter()
                            .position(|id| *id == photo.id)
                            .map(|i| calibration.view_rms[i]);
                    }
                    self.intrinsics = Some(calibration.intrinsics.clone());
                    self.calibration = Some(calibration);
                    self.texture = None;
                    self.message = None;
                }
                Job::Calibrated(Err(e)) => self.message = Some(e),
            }
        }
    }

    fn controls(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        ui.heading(tr("Objektiv kalibrieren"));
        ui.label(tr(
            "Schachbrett mit 10 × 7 Feldern (9 × 6 inneren Ecken) ausdrucken und flach auf \
            eine feste Platte kleben. 10 bis 20 Fotos aufnehmen: das Brett an verschiedenen \
            Stellen, auch in den Bildecken, und um bis zu 30° geneigt, nicht nur flach \
            liegend. Das Brett muss ganz im Bild sein.",
        ));
        ui.separator();
        egui::ComboBox::from_label(tr("Kamera"))
            .selected_text(self.cameras[self.camera].name.clone())
            .show_ui(ui, |ui| {
                for (i, camera) in self.cameras.iter().enumerate() {
                    ui.selectable_value(&mut self.camera, i, camera.name.clone());
                }
            });
        let camera = &mut self.cameras[self.camera];
        egui::Grid::new("camera").num_columns(2).show(ui, |ui| {
            ui.label(tr("Adresse"));
            ui.text_edit_singleline(&mut camera.url);
            ui.end_row();
            ui.label(tr("Benutzer"));
            ui.text_edit_singleline(&mut camera.user);
            ui.end_row();
            ui.label(tr("Kennwort"));
            ui.add(egui::TextEdit::singleline(&mut camera.password).password(true));
            ui.end_row();
        });
        ui.horizontal(|ui| {
            if ui.button(tr("Foto aufnehmen")).clicked() {
                self.capture(&ctx);
            }
            if ui.button(tr("Fotos laden …")).clicked() {
                self.open_photos(&ctx);
            }
            if self.busy > 0 {
                ui.spinner();
            }
        });
        ui.separator();

        let found = self.photos.iter().filter(|p| p.corners.is_some()).count();
        ui.label(tr!(
            "{found} von {count} Fotos mit Schachbrett",
            found = found,
            count = self.photos.len()
        ));
        let mut remove = None;
        egui::ScrollArea::vertical()
            .max_height(260.0)
            .show(ui, |ui| {
                for photo in &self.photos {
                    ui.horizontal(|ui| {
                        if ui
                            .small_button("×")
                            .on_hover_text(tr("Foto entfernen"))
                            .clicked()
                        {
                            remove = Some(photo.id);
                        }
                        let status = match (&photo.corners, photo.rms) {
                            (None, _) => tr("kein Schachbrett").to_string(),
                            (Some(_), Some(rms)) => format!("{rms:.2} px"),
                            (Some(_), None) => "✓".into(),
                        };
                        let text = format!("{}  {status}", photo.name);
                        if ui
                            .selectable_label(self.selected == Some(photo.id), text)
                            .clicked()
                        {
                            self.selected = Some(photo.id);
                        }
                    });
                }
            });
        if let Some(id) = remove {
            self.photos.retain(|p| p.id != id);
            if self.selected == Some(id) {
                self.selected = self.photos.last().map(|p| p.id);
            }
        }
        ui.horizontal(|ui| {
            let ready = found >= 3 && self.busy == 0;
            if ui
                .add_enabled(ready, egui::Button::new(tr("Kalibrieren")))
                .on_disabled_hover_text(tr("Mindestens drei Fotos mit erkanntem Schachbrett nötig"))
                .clicked()
            {
                self.calibrate(&ctx);
            }
            if ui
                .add_enabled(
                    !self.photos.is_empty(),
                    egui::Button::new(tr("Fotos speichern …")),
                )
                .clicked()
            {
                self.save_photos();
            }
        });
        ui.separator();

        if let Some(k) = &self.intrinsics {
            egui::Grid::new("result").num_columns(2).show(ui, |ui| {
                if let Some(c) = &self.calibration {
                    ui.label(tr("Restfehler"));
                    ui.label(format!("{:.3} px", c.rms));
                    ui.end_row();
                }
                ui.label("fx, fy");
                ui.label(format!("{:.1}, {:.1}", k.fx, k.fy));
                ui.end_row();
                ui.label("cx, cy");
                ui.label(format!("{:.1}, {:.1}", k.cx, k.cy));
                ui.end_row();
                ui.label("k1, k2, k3");
                ui.label(format!(
                    "{:.4}, {:.4}, {:.4}",
                    k.dist[0], k.dist[1], k.dist[4]
                ));
                ui.end_row();
                ui.label("p1, p2");
                ui.label(format!("{:.5}, {:.5}", k.dist[2], k.dist[3]));
                ui.end_row();
            });
            if self.calibration.as_ref().is_some_and(|c| c.rms > 1.0) {
                ui.colored_label(
                    ACCENT,
                    tr("Restfehler über 1 px: Fotos mit großem Fehler entfernen und neu kalibrieren."),
                );
            }
            if ui
                .checkbox(&mut self.undistorted, tr("Entzerrt anzeigen"))
                .changed()
            {
                self.texture = None;
            }
        }
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    self.calibration.is_some(),
                    egui::Button::new(tr("calibration.npz speichern …")),
                )
                .clicked()
            {
                self.save_npz();
            }
            if ui.button(tr("calibration.npz öffnen …")).clicked() {
                self.open_npz();
            }
        });
        if let Some(message) = &self.message {
            ui.separator();
            ui.label(message.as_str());
        }
    }

    fn view(&mut self, ui: &mut egui::Ui) {
        let Some(photo) = self
            .selected
            .and_then(|id| self.photos.iter().find(|p| p.id == id))
        else {
            ui.centered_and_justified(|ui| ui.label(tr("Noch kein Foto")));
            return;
        };
        let undistorted = self.undistorted && self.intrinsics.is_some();
        let s = photo.scale();
        if self
            .texture
            .as_ref()
            .is_none_or(|(id, u, _)| *id != photo.id || *u != undistorted)
        {
            let image = match (&self.intrinsics, undistorted) {
                (Some(k), true) => lens_calibration::undistort_image(&photo.preview, &scaled(k, s)),
                _ => photo.preview.clone(),
            };
            let pixels = egui::ColorImage::from_rgba_unmultiplied(
                [image.width() as usize, image.height() as usize],
                image.as_raw(),
            );
            let texture = ui
                .ctx()
                .load_texture("photo", pixels, egui::TextureOptions::LINEAR);
            self.texture = Some((photo.id, undistorted, texture));
        }
        let texture = &self.texture.as_ref().unwrap().2;
        let image_size = Vec2::new(photo.preview.width() as f32, photo.preview.height() as f32);
        let zoom = (ui.available_width() / image_size.x).min(ui.available_height() / image_size.y);
        let (rect, _) = ui.allocate_exact_size(image_size * zoom, Sense::hover());
        let painter = ui.painter_at(rect);
        painter.image(
            texture.id(),
            rect,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
        // Ecken aller Fotos zeigen, welche Bildbereiche schon abgedeckt sind.
        let k = undistorted.then(|| self.intrinsics.clone()).flatten();
        let to_screen = |p: [f64; 2]| {
            let p = k.as_ref().map_or(p, |k| k.undistort(p));
            let x = ((p[0] + 0.5) * s - 0.5) as f32 * zoom;
            let y = ((p[1] + 0.5) * s - 0.5) as f32 * zoom;
            rect.min + Vec2::new(x, y)
        };
        for other in self
            .photos
            .iter()
            .filter(|p| p.id != photo.id && p.size == photo.size)
        {
            for c in other.corners.iter().flatten() {
                painter.circle_filled(
                    to_screen(*c),
                    2.0,
                    Color32::from_rgba_unmultiplied(80, 160, 255, 160),
                );
            }
        }
        if let Some(corners) = &photo.corners {
            for row in corners.chunks(PATTERN.0) {
                for pair in row.windows(2) {
                    painter.line_segment(
                        [to_screen(pair[0]), to_screen(pair[1])],
                        Stroke::new(1.5, ACCENT),
                    );
                }
            }
            for (i, c) in corners.iter().enumerate() {
                let color = if i == 0 { Color32::RED } else { ACCENT };
                painter.circle_stroke(to_screen(*c), 4.0, Stroke::new(1.5, color));
            }
        }
    }
}

#[cfg(feature = "screenshot")]
impl App {
    fn take_screenshot(&mut self, ctx: &egui::Context) {
        if self.capture.is_none() || self.busy > 0 {
            return;
        }
        if self.calibration.is_none()
            && self.photos.iter().filter(|p| p.corners.is_some()).count() >= 3
        {
            self.calibrate(ctx);
            return;
        }
        let image = ctx.input(|i| {
            i.events.iter().find_map(|event| match event {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        let (path, frames) = self.capture.as_mut().unwrap();
        if let Some(image) = image {
            let pixels = image.pixels.iter().flat_map(|p| p.to_array()).collect();
            let shot =
                RgbaImage::from_raw(image.size[0] as u32, image.size[1] as u32, pixels).unwrap();
            if let Err(error) = shot.save(&*path) {
                eprintln!("{error}");
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        *frames += 1;
        if *frames == 10 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(50));
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.receive();
        #[cfg(feature = "screenshot")]
        self.take_screenshot(ctx);
        egui::SidePanel::left("controls")
            .resizable(true)
            .default_width(360.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.controls(ui));
            });
        egui::CentralPanel::default().show(ctx, |ui| self.view(ui));
    }
}

fn main() -> eframe::Result {
    // Ohne Einstellungen der Oberfläche gilt die Systemsprache.
    visicut_core::i18n::set_language(visicut_core::i18n::Language::system());
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(tr("VisiCutR Objektivkalibrierung"))
        .with_inner_size([1280.0, 820.0]);
    let png = include_bytes!("../../resources/icon/icon.png");
    if let Ok(icon) = image::load_from_memory(png) {
        let icon = icon.into_rgba8();
        viewport = viewport.with_icon(egui::IconData {
            width: icon.width(),
            height: icon.height(),
            rgba: icon.into_raw(),
        });
    }
    eframe::run_native(
        "VisiCutR lens calibration",
        eframe::NativeOptions {
            viewport,
            ..Default::default()
        },
        Box::new(|cc| {
            let mut app = App::new();
            // Fotos aus der Befehlszeile gleich laden.
            let mut arguments = std::env::args_os().skip(1);
            let mut files = Vec::new();
            while let Some(argument) = arguments.by_ref().next() {
                #[cfg(feature = "screenshot")]
                if argument == "--capture" {
                    app.capture = arguments.next().map(|path| (path.into(), 0));
                    continue;
                }
                #[cfg(feature = "screenshot")]
                if argument == "--undistorted" {
                    app.undistorted = true;
                    continue;
                }
                files.push(std::path::PathBuf::from(argument));
            }
            app.load_files(&cc.egui_ctx, files);
            Ok(Box::new(app))
        }),
    )
}
