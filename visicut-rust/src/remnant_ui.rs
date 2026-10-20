//! Dialog „Reststücke erkennen“: findet Materialreste im Kamerabild und zeigt
//! ihre Umrisse auf dem Arbeitsbett. Die Erkennung läuft im Hintergrund und
//! wiederholt sich, sobald sich Einstellungen oder das Kamerabild ändern.
use crate::devices_ui::DeviceUi;
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};
use image::RgbaImage;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use visicut_core::remnant::{self, Contrast, Detection, Options, Remnant};
use visicut_core::{device, project::Project};
use visicut_core::{i18n::tr, tr};

/// Umrissfarbe der Reststücke (keine Bearbeitungsfarbe).
const OUTLINE: Color32 = Color32::from_rgb(0, 190, 90);

/// Woran die Erkennung Material vom Bett unterscheidet.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Method {
    Reference,
    Lighter,
    Darker,
}

/// Eingaben einer Erkennung; ändern sie sich, wird neu gerechnet.
#[derive(Clone, PartialEq)]
struct Input {
    options: Options,
    reference: bool,
    /// Adresse des Kamerabilds: jedes neue Bild ist ein neues `Arc`.
    image: usize,
}

type Pending = Receiver<Result<Detection, String>>;

pub enum Action {
    /// Umrisse als neues Teil an ihrer Stelle einfügen.
    InsertPart(Vec<Remnant>),
}

pub struct RemnantUi {
    open: bool,
    method: Method,
    auto_threshold: bool,
    threshold: u8,
    smoothing_mm: f64,
    margin_mm: f64,
    min_area_cm2: f64,
    /// Referenzbild des leeren Betts für den Lasercutter mit diesem Namen.
    reference: Option<(String, Arc<RgbaImage>)>,
    pending: Option<(Input, Pending)>,
    result: Option<(Input, Result<Detection, String>)>,
    /// Umrisse bleiben nach dem Schließen auf dem Arbeitsbett sichtbar.
    keep: bool,
    problem: Option<String>,
}

impl Default for RemnantUi {
    fn default() -> Self {
        let options = Options::default();
        Self {
            open: false,
            method: Method::Lighter,
            auto_threshold: true,
            threshold: 128,
            smoothing_mm: options.smoothing_mm,
            margin_mm: options.margin_mm,
            min_area_cm2: options.min_area_mm2 / 100.0,
            reference: None,
            pending: None,
            result: None,
            keep: false,
            problem: None,
        }
    }
}

impl RemnantUi {
    /// Öffnet den Dialog und blendet das Kamerabild ein.
    pub fn open(&mut self, devices: &mut DeviceUi, project: &Project) {
        self.open = true;
        self.problem = None;
        self.load_reference(devices);
        if self.reference.is_some() && self.method != Method::Reference {
            self.method = Method::Reference;
        }
        devices.show_camera(project);
    }

    /// Lädt das Referenzbild des gewählten Lasercutters, falls gespeichert.
    fn load_reference(&mut self, devices: &DeviceUi) {
        let name = &devices.device().name;
        if self.reference.as_ref().is_some_and(|(n, _)| n == name) {
            return;
        }
        let path = remnant::reference_path(&device::config_dir(), name);
        self.reference =
            remnant::load_reference(&path).map(|image| (name.clone(), Arc::new(image)));
        if self.reference.is_none() && self.method == Method::Reference {
            self.method = Method::Lighter;
        }
    }

    fn options(&self) -> Options {
        Options {
            contrast: match self.method {
                Method::Darker => Contrast::Darker,
                _ => Contrast::Lighter,
            },
            threshold: (!self.auto_threshold).then_some(self.threshold),
            smoothing_mm: self.smoothing_mm,
            margin_mm: self.margin_mm,
            min_area_mm2: self.min_area_cm2 * 100.0,
            ..Options::default()
        }
    }

    /// Reststücke, die auf dem Arbeitsbett gezeigt werden.
    fn shown(&self) -> &[Remnant] {
        match &self.result {
            Some((_, Ok(detection))) if self.open || self.keep => &detection.remnants,
            _ => &[],
        }
    }

    /// Zeichnet die Umrisse auf das Bett (`scale` Pixel je mm).
    pub fn paint(&self, painter: &egui::Painter, bed: Rect, scale: f32) {
        for remnant in self.shown() {
            for contour in remnant.contours() {
                let points: Vec<Pos2> = contour
                    .iter()
                    .map(|p| bed.min + Vec2::new(p[0] as f32, p[1] as f32) * scale)
                    .collect();
                painter.add(egui::Shape::closed_line(
                    points.clone(),
                    Stroke::new(3.5, Color32::from_black_alpha(150)),
                ));
                painter.add(egui::Shape::closed_line(points, Stroke::new(1.8, OUTLINE)));
            }
            let [x, y, w, h] = remnant.bounds();
            let center = bed.min + Vec2::new((x + w / 2.0) as f32, (y + h / 2.0) as f32) * scale;
            painter.text(
                center,
                egui::Align2::CENTER_CENTER,
                format!("{:.0} cm²", remnant.area_mm2 / 100.0),
                egui::FontId::proportional(12.0),
                OUTLINE,
            );
        }
    }

    fn poll(&mut self, ctx: &egui::Context) {
        let Some((_, receiver)) = &self.pending else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(50));
                return;
            }
            Err(TryRecvError::Disconnected) => Err(tr("Erkennung unerwartet abgebrochen").into()),
        };
        if let Some((input, _)) = self.pending.take() {
            if let Ok(detection) = &result
                && self.auto_threshold
            {
                self.threshold = detection.threshold;
            }
            self.result = Some((input, result));
        }
    }

    /// Startet die Erkennung, wenn sich die Eingaben geändert haben.
    fn start(&mut self, image: Arc<RgbaImage>, bed_mm: [f64; 2]) {
        let reference = match self.method {
            Method::Reference => self.reference.as_ref().map(|(_, r)| r.clone()),
            _ => None,
        };
        let input = Input {
            options: self.options(),
            reference: reference.is_some(),
            image: Arc::as_ptr(&image) as usize,
        };
        if self.pending.is_some() || self.result.as_ref().is_some_and(|(i, _)| *i == input) {
            return;
        }
        let (tx, rx) = channel();
        let options = input.options;
        std::thread::spawn(move || {
            let _ = tx.send(remnant::detect(
                &image,
                reference.as_deref(),
                bed_mm,
                &options,
            ));
        });
        self.pending = Some((input, rx));
    }

    pub fn windows(
        &mut self,
        ctx: &egui::Context,
        devices: &mut DeviceUi,
        project: &Project,
        status: &mut String,
    ) -> Option<Action> {
        self.poll(ctx);
        if !self.open {
            return None;
        }
        self.load_reference(devices);
        let bed_mm = [
            f64::from(project.bed_width_mm),
            f64::from(project.bed_height_mm),
        ];
        let image = devices.camera_image();
        if let Some(image) = &image {
            self.start(image.clone(), bed_mm);
        }
        let mut open = self.open;
        let mut action = None;
        let mut close = false;
        egui::Window::new(tr("Reststücke erkennen"))
            .id(egui::Id::new("remnants"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if devices.camera_loading() {
                        ui.spinner();
                        ui.label(tr("Kamerabild wird geladen …"));
                    } else if image.is_none() {
                        ui.label(tr("Noch kein Kamerabild"));
                    }
                    if ui
                        .add_enabled(
                            !devices.camera_loading(),
                            egui::Button::new(tr("Kamerabild aktualisieren")),
                        )
                        .clicked()
                    {
                        devices.show_camera(project);
                        devices.refresh_camera(project);
                    }
                });
                ui.separator();
                ui.label(tr("Material erkennen"));
                ui.add_enabled_ui(self.reference.is_some(), |ui| {
                    ui.radio_value(
                        &mut self.method,
                        Method::Reference,
                        tr("Unterschied zum Foto des leeren Betts"),
                    )
                    .on_disabled_hover_text(tr(
                        "Zuerst das leere Bett als Referenz speichern",
                    ));
                });
                ui.radio_value(
                    &mut self.method,
                    Method::Lighter,
                    tr("Material heller als das Bett"),
                );
                ui.radio_value(
                    &mut self.method,
                    Method::Darker,
                    tr("Material dunkler als das Bett"),
                );
                ui.horizontal(|ui| {
                    let save = ui
                        .add_enabled(
                            image.is_some(),
                            egui::Button::new(tr("Leeres Bett als Referenz speichern")),
                        )
                        .on_hover_text(tr(
                            "Bett vorher leer räumen. Die Erkennung vergleicht danach jedes Kamerabild mit diesem Foto und findet auch Material in der Farbe des Betts.",
                        ));
                    if save.clicked()
                        && let Some(image) = &image
                    {
                        let name = devices.device().name.clone();
                        let path = remnant::reference_path(&device::config_dir(), &name);
                        match remnant::save_reference(&path, image) {
                            Ok(()) => {
                                self.reference = Some((name, Arc::new((**image).clone())));
                                self.method = Method::Reference;
                                *status = tr("Referenzbild des leeren Betts gespeichert").into();
                            }
                            Err(e) => {
                                self.problem =
                                    Some(tr!("Referenzbild nicht gespeichert: {error}", error = e));
                            }
                        }
                    }
                    if self.reference.is_some() && ui.button(tr("Referenz löschen")).clicked() {
                        let path = remnant::reference_path(
                            &device::config_dir(),
                            &devices.device().name,
                        );
                        let _ = std::fs::remove_file(path);
                        self.reference = None;
                        if self.method == Method::Reference {
                            self.method = Method::Lighter;
                        }
                    }
                });
                ui.separator();
                ui.checkbox(&mut self.auto_threshold, tr("Schwellwert automatisch"));
                ui.add_enabled(
                    !self.auto_threshold,
                    egui::Slider::new(&mut self.threshold, 0..=255).text(tr("Schwellwert")),
                );
                egui::Grid::new("remnant-options")
                    .num_columns(2)
                    .show(ui, |ui| {
                        let row = |ui: &mut egui::Ui, label: &str, help: &str, value: &mut f64, range: std::ops::RangeInclusive<f64>, suffix: &str| {
                            ui.label(label).on_hover_text(help);
                            ui.add(
                                egui::DragValue::new(value)
                                    .range(range)
                                    .speed(0.1)
                                    .suffix(suffix),
                            )
                            .on_hover_text(help);
                            ui.end_row();
                        };
                        row(
                            ui,
                            tr("Glättung"),
                            tr("Gleicht Wabe, Lamellen und Kratzer aus; größere Werte runden Ecken ab."),
                            &mut self.smoothing_mm,
                            0.0..=20.0,
                            " mm",
                        );
                        row(
                            ui,
                            tr("Sicherheitsabstand"),
                            tr("Die Umrisse liegen um diesen Abstand innerhalb der erkannten Kante."),
                            &mut self.margin_mm,
                            0.0..=50.0,
                            " mm",
                        );
                        row(
                            ui,
                            tr("Mindestfläche"),
                            tr("Kleinere Reststücke werden ignoriert."),
                            &mut self.min_area_cm2,
                            0.0..=10000.0,
                            " cm²",
                        );
                    });
                ui.separator();
                match &self.result {
                    Some((_, Ok(detection))) if !detection.remnants.is_empty() => {
                        let largest = detection.remnants[0].bounds();
                        ui.label(tr!(
                            "{count} Reststück(e) · größtes etwa {width} × {height} mm",
                            count = detection.remnants.len(),
                            width = format!("{:.0}", largest[2]),
                            height = format!("{:.0}", largest[3])
                        ));
                    }
                    Some((_, Ok(_))) => {
                        ui.label(tr("Kein Reststück gefunden; Einstellungen anpassen"));
                    }
                    Some((_, Err(error))) => {
                        ui.colored_label(crate::WARNING, error);
                    }
                    None => {}
                }
                if self.pending.is_some() {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(tr("Erkennung läuft …"));
                    });
                }
                if let Some(problem) = &self.problem {
                    ui.colored_label(crate::WARNING, problem);
                }
                crate::form::caption(
                    ui,
                    tr("Die Umrisse stammen aus dem Kamerabild und sind nur so genau wie dessen Kalibrierung."),
                );
                let found = matches!(&self.result, Some((_, Ok(d))) if !d.remnants.is_empty());
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(found, egui::Button::new(tr("Auf dem Bett anzeigen")))
                        .on_hover_text(tr("Umrisse bleiben nach dem Schließen sichtbar, bis sie verworfen werden."))
                        .clicked()
                    {
                        self.keep = true;
                        close = true;
                    }
                    if ui
                        .add_enabled(found, egui::Button::new(tr("Als Teil einfügen")))
                        .on_hover_text(tr("Fügt die Umrisse als Schneid-Teil an ihrer Stelle ein, z. B. zum Zuschneiden. Löschen, wenn es nicht gelasert werden soll."))
                        .clicked()
                        && let Some((_, Ok(detection))) = &self.result
                    {
                        action = Some(Action::InsertPart(detection.remnants.clone()));
                        close = true;
                    }
                    if ui.button(tr("Verwerfen")).clicked() {
                        self.keep = false;
                        close = true;
                    }
                });
            });
        self.open = open && !close;
        action
    }
}
