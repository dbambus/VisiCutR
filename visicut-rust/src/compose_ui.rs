//! Text, shapes, signs, copies and nesting for the egui interface.
use eframe::egui;
use visicut_core::{
    compose::{self, Align, NestRotation, NestSettings, NewObject, Shape, Sign, TextSource},
    i18n::tr,
    project::{Operation, Project},
    tr,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Text,
    Shape,
    Sign,
    Array,
    Nest,
}

impl Tool {
    fn icon(self) -> crate::object_bar::Icon {
        use crate::object_bar::Icon;
        match self {
            Self::Text => Icon::Text,
            Self::Shape => Icon::Shape,
            Self::Sign => Icon::Sign,
            Self::Array => Icon::Array,
            Self::Nest => Icon::Nest,
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::Text => tr("Text einfügen"),
            Self::Shape => tr("Form einfügen"),
            Self::Sign => tr("Schild erstellen"),
            Self::Array => tr("Kopien im Raster"),
            Self::Nest => tr("Teile verschachteln (Nesting)"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ShapeKind {
    Rectangle,
    Ellipse,
    Polygon,
    Star,
}

struct ShapeForm {
    kind: ShapeKind,
    width_mm: f64,
    height_mm: f64,
    corner_mm: f64,
    corners: u32,
    diameter_mm: f64,
    inner_percent: f64,
    operation: Operation,
}

/// Ergebnis eines Werkzeugs: neues Projekt und Hinweise.
pub type Composed = Result<(Project, Vec<String>), String>;

pub struct ComposeUi {
    open: Option<Tool>,
    /// Das Textfenster bearbeitet den Text des gewählten Teils.
    editing: bool,
    text: TextSource,
    shape: ShapeForm,
    sign: Sign,
    columns: u32,
    rows: u32,
    gap_x_mm: f64,
    gap_y_mm: f64,
    nest: NestSettings,
    fonts: Vec<String>,
    samples: FontSamples,
}

impl Default for ComposeUi {
    fn default() -> Self {
        Self {
            open: None,
            editing: false,
            text: TextSource {
                text: tr("Text").into(),
                font: "sans-serif".into(),
                size_mm: 10.0,
                bold: false,
                italic: false,
                align: Align::Left,
                line_spacing: 1.2,
                operation: Operation::Engrave,
                rotate_deg: 0.0,
                mirrored: false,
            },
            shape: ShapeForm {
                kind: ShapeKind::Rectangle,
                width_mm: 50.0,
                height_mm: 30.0,
                corner_mm: 0.0,
                corners: 6,
                diameter_mm: 40.0,
                inner_percent: 50.0,
                operation: Operation::Cut,
            },
            sign: Sign::default(),
            columns: 2,
            rows: 2,
            gap_x_mm: 5.0,
            gap_y_mm: 5.0,
            nest: NestSettings::default(),
            fonts: Vec::new(),
            samples: FontSamples::default(),
        }
    }
}

/// Breite der Felder rechts, wie in der Seitenleiste.
const FIELD: f32 = 150.0;
/// Breite des Inhalts im Rahmen (Fenster 420 pt).
const CONTENT_WIDTH: f32 = 360.0;

fn operation_choice(ui: &mut egui::Ui, id: &str, operation: &mut Operation) {
    crate::form::row(ui, tr("Bearbeitung"), |ui| {
        crate::form::Dropdown::from_id_salt(id)
            .width(FIELD)
            .selected_text(match operation {
                Operation::Cut => tr("Schneiden (rot)"),
                Operation::Mark => tr("Markieren (blau)"),
                _ => tr("Gravieren (schwarz)"),
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(operation, Operation::Engrave, tr("Gravieren (schwarz)"));
                ui.selectable_value(operation, Operation::Cut, tr("Schneiden (rot)"));
                ui.selectable_value(operation, Operation::Mark, tr("Markieren (blau)"));
            });
    });
}

/// Zeile mit Zahlenfeld und Einheit im Feld.
fn number<N: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut N,
    range: std::ops::RangeInclusive<N>,
    unit: &str,
) {
    crate::form::row(ui, label, |ui| {
        crate::form::unit_value(ui, value, range, unit, FIELD);
    });
}

fn mm(ui: &mut egui::Ui, label: &str, value: &mut f64, range: std::ops::RangeInclusive<f64>) {
    number(ui, label, value, range, "mm");
}

/// Schriftproben für die Schriftauswahl, je Schrift einmal gerendert.
#[derive(Default)]
struct FontSamples {
    /// Text, fett, kursiv der vorhandenen Proben.
    key: (String, bool, bool),
    textures: std::collections::HashMap<String, Option<egui::TextureHandle>>,
}

impl FontSamples {
    /// Der Text in der Schrift `font` als Tooltip-Inhalt.
    fn show(&mut self, ui: &mut egui::Ui, font: &str, text: &str, bold: bool, italic: bool) {
        if self.key.0 != text || self.key.1 != bold || self.key.2 != italic {
            self.key = (text.to_string(), bold, italic);
            self.textures.clear();
        }
        let ppp = ui.ctx().pixels_per_point();
        let texture = self.textures.entry(font.to_string()).or_insert_with(|| {
            compose::font_sample(text, font, bold, italic, (28.0 * ppp) as f64).map(|image| {
                ui.ctx().load_texture(
                    format!("font-sample-{font}"),
                    image,
                    egui::TextureOptions::LINEAR,
                )
            })
        });
        match texture {
            Some(texture) => {
                let size = texture.size_vec2() / ppp;
                let max = egui::vec2(CONTENT_WIDTH, 160.0);
                let scale = (max.x / size.x).min(max.y / size.y).min(1.0);
                ui.add(egui::Image::new((texture.id(), size * scale)));
            }
            None => {
                ui.label(tr("Die Schrift hat keine passenden Zeichen"));
            }
        }
    }
}

/// Schriftauswahl; beim Überfahren einer Schrift zeigt ein Tooltip den Text
/// in dieser Schrift.
fn font_choice(
    ui: &mut egui::Ui,
    fonts: &[String],
    font: &mut String,
    samples: &mut FontSamples,
    (text, bold, italic): (&str, bool, bool),
) {
    crate::form::row(ui, tr("Schrift"), |ui| {
        crate::form::Dropdown::from_id_salt("font")
            .width(FIELD)
            .selected_text(font.as_str())
            .height(320.0)
            .show_ui(ui, |ui| {
                for name in fonts {
                    ui.selectable_value(font, name.clone(), name)
                        .on_hover_ui(|ui| samples.show(ui, name, text, bold, italic));
                }
            });
    });
}

/// Zeile „Stil“: die Kästchen links in der Feldspalte, in Flucht mit den
/// Feldern darüber und darunter.
fn style_choice(ui: &mut egui::Ui, bold: &mut bool, italic: Option<&mut bool>) {
    crate::form::row(ui, tr("Stil"), |ui| {
        let size = egui::vec2(FIELD, ui.spacing().interact_size.y);
        ui.allocate_ui_with_layout(
            size,
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.checkbox(bold, bold_label(ui));
                if let Some(italic) = italic {
                    ui.add_space(8.0);
                    ui.checkbox(italic, italic_label());
                }
            },
        );
    });
}

/// „Fett“ in fetter Schrift, damit das Kästchen zeigt, was es bewirkt.
fn bold_label(ui: &egui::Ui) -> egui::RichText {
    let size = egui::TextStyle::Button.resolve(ui.style()).size;
    egui::RichText::new(tr("Fett")).font(crate::form::bold(size))
}

/// „Kursiv“ in kursiver Schrift.
fn italic_label() -> egui::RichText {
    egui::RichText::new(tr("Kursiv")).italics()
}

fn text_field(ui: &mut egui::Ui, text: &mut String, rows: usize) {
    ui.add(
        egui::TextEdit::multiline(text)
            .desired_rows(rows)
            .desired_width(CONTENT_WIDTH),
    );
    ui.add_space(4.0);
}

/// Formen als gezeichnete Symbole statt Wörter; liefert die gewählte.
fn shape_choice(ui: &mut egui::Ui, kind: &mut ShapeKind) {
    ui.horizontal(|ui| {
        for (option, name) in [
            (ShapeKind::Rectangle, tr("Rechteck")),
            (ShapeKind::Ellipse, tr("Ellipse")),
            (ShapeKind::Polygon, tr("Vieleck")),
            (ShapeKind::Star, tr("Stern")),
        ] {
            let selected = *kind == option;
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(56.0, 44.0), egui::Sense::click());
            let painter = ui.painter();
            let fill = if selected {
                crate::form::ACCENT
            } else if response.hovered() {
                egui::Color32::from_gray(225)
            } else {
                egui::Color32::from_gray(238)
            };
            painter.rect_filled(rect, 6.0, fill);
            let color = if selected {
                egui::Color32::WHITE
            } else {
                egui::Color32::from_gray(50)
            };
            let stroke = egui::Stroke::new(1.8, color);
            let c = rect.center();
            match option {
                ShapeKind::Rectangle => {
                    let r = egui::Rect::from_center_size(c, egui::vec2(28.0, 20.0));
                    painter.rect_stroke(r, 0.0, stroke, egui::StrokeKind::Middle);
                }
                ShapeKind::Ellipse => {
                    painter.add(egui::Shape::ellipse_stroke(
                        c,
                        egui::vec2(15.0, 10.0),
                        stroke,
                    ));
                }
                ShapeKind::Polygon | ShapeKind::Star => {
                    let star = option == ShapeKind::Star;
                    let count = if star { 10 } else { 6 };
                    let points = (0..count)
                        .map(|i| {
                            let a = std::f32::consts::TAU * i as f32 / count as f32
                                - std::f32::consts::FRAC_PI_2;
                            let radius = if star && i % 2 == 1 { 5.5 } else { 13.0 };
                            c + egui::vec2(a.cos(), a.sin()) * radius
                        })
                        .collect();
                    painter.add(egui::Shape::closed_line(points, stroke));
                }
            }
            if response.on_hover_text(name).clicked() {
                *kind = option;
            }
        }
    });
    ui.add_space(6.0);
}

impl ComposeUi {
    /// Öffnet ein Werkzeug.
    pub fn open(&mut self, tool: Tool) {
        if self.fonts.is_empty() {
            self.fonts = compose::font_families();
        }
        self.editing = false;
        self.open = Some(tool);
    }

    /// Öffnet das Textfenster mit Text und Einstellungen des gewählten Teils.
    pub fn edit_text(&mut self, source: &TextSource) {
        self.open(Tool::Text);
        self.text = source.clone();
        self.editing = true;
    }

    /// Zeigt das offene Werkzeug; liefert das neue Projekt nach „Einfügen“.
    /// Neue Elemente kommen in den sichtbaren Teil des Betts `view` (x, y,
    /// Breite, Höhe in mm), wie er beim Einfügen ist.
    pub fn windows(
        &mut self,
        ctx: &egui::Context,
        project: &Project,
        view: [f32; 4],
    ) -> Option<Composed> {
        let tool = self.open?;
        let title = if self.editing {
            tr("Text bearbeiten")
        } else {
            tool.title()
        };
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;
        crate::form::tool_window(ctx, "compose_tool", title, 420.0, &mut open, |ui| {
            // Wie Hinweis und Sende-Bestätigung: Rahmen in der Akzentfarbe,
            // Titel mit Symbol, Knöpfe rechts unten.
            egui::Frame::new()
                .stroke(egui::Stroke::new(2.0, crate::form::ACCENT))
                .corner_radius(7.0)
                .inner_margin(egui::Margin::symmetric(18, 14))
                .show(ui, |ui| {
                    // Feste Breite: Textfelder wachsen sonst mit dem Fenster.
                    ui.set_width(CONTENT_WIDTH);
                    ui.vertical_centered(|ui| {
                        ui.horizontal(|ui| {
                            let title = egui::RichText::new(title).font(crate::form::bold(20.0));
                            let galley = ui.painter().layout_no_wrap(
                                title.text().to_owned(),
                                crate::form::bold(20.0),
                                egui::Color32::BLACK,
                            );
                            let width = 30.0 + galley.size().x;
                            ui.add_space(((ui.available_width() - width) / 2.0).max(0.0));
                            let (icon, _) = ui
                                .allocate_exact_size(egui::vec2(26.0, 26.0), egui::Sense::hover());
                            crate::object_bar::paint(
                                ui.painter(),
                                tool.icon(),
                                icon,
                                crate::form::ACCENT,
                            );
                            ui.label(title);
                        });
                    });
                    ui.add_space(10.0);
                    match tool {
                        Tool::Text => self.text_form(ui),
                        Tool::Shape => self.shape_form(ui),
                        Tool::Sign => self.sign_form(ui),
                        Tool::Array => self.array_form(ui, project),
                        Tool::Nest => self.nest_form(ui, project),
                    }
                    ui.add_space(12.0);
                    let label = match tool {
                        Tool::Array | Tool::Nest => tr("Anwenden"),
                        Tool::Text if self.editing => tr("Übernehmen"),
                        _ => tr("Einfügen"),
                    };
                    // Nur eine Zeile hoch, sonst wächst das Fenster mit.
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), 30.0),
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            apply = ui.add(crate::form::primary_button(label)).clicked();
                            if ui
                                .add(
                                    egui::Button::new(tr("Abbrechen"))
                                        .min_size(egui::vec2(0.0, 28.0)),
                                )
                                .clicked()
                            {
                                cancel = true;
                            }
                        },
                    );
                });
        });
        if cancel {
            open = false;
        }
        if !open {
            self.open = None;
        }
        if !apply {
            return None;
        }
        let result = self.compose(tool, project, view);
        if result.is_ok() {
            self.open = None;
        }
        Some(result)
    }

    fn compose(&self, tool: Tool, project: &Project, view: [f32; 4]) -> Composed {
        // Jedes neue Element wird ein eigenes Teil (siehe `add_objects_as_part`).
        let name = match tool {
            Tool::Text => tr("Text"),
            Tool::Shape => tr("Form"),
            _ => tr("Schild"),
        };
        let add = |objects: Vec<NewObject>| {
            compose::add_objects_as_part(project, &objects, name, view).map(|p| (p, Vec::new()))
        };
        match tool {
            Tool::Text if self.editing => {
                compose::set_text(project, &self.text).map(|p| (p, Vec::new()))
            }
            Tool::Text => compose::add_text(project, &self.text, view).map(|p| (p, Vec::new())),
            Tool::Shape => {
                let s = &self.shape;
                let shape = match s.kind {
                    ShapeKind::Rectangle => Shape::Rectangle {
                        width_mm: s.width_mm,
                        height_mm: s.height_mm,
                        corner_mm: s.corner_mm,
                    },
                    ShapeKind::Ellipse => Shape::Ellipse {
                        width_mm: s.width_mm,
                        height_mm: s.height_mm,
                    },
                    ShapeKind::Polygon => Shape::Polygon {
                        sides: s.corners,
                        diameter_mm: s.diameter_mm,
                    },
                    ShapeKind::Star => Shape::Star {
                        points: s.corners,
                        diameter_mm: s.diameter_mm,
                        inner_percent: s.inner_percent,
                    },
                };
                add(vec![NewObject {
                    shape,
                    operation: s.operation,
                    x_mm: 0.0,
                    y_mm: 0.0,
                }])
            }
            Tool::Sign => add(compose::sign(&self.sign)?),
            Tool::Array => compose::array(
                project,
                self.columns,
                self.rows,
                self.gap_x_mm,
                self.gap_y_mm,
            )
            .map(|p| (p, Vec::new())),
            Tool::Nest => compose::nest(project, &self.nest).map(|n| {
                let mut notes = Vec::new();
                if n.missing > 0 {
                    notes.push(tr!(
                        "{missing} von {total} Teilen passen nicht in die Fläche",
                        missing = n.missing,
                        total = n.missing + n.placed
                    ));
                }
                (n.project, notes)
            }),
        }
    }

    fn text_form(&mut self, ui: &mut egui::Ui) {
        let t = &mut self.text;
        text_field(ui, &mut t.text, 3);
        font_choice(
            ui,
            &self.fonts,
            &mut t.font,
            &mut self.samples,
            (&t.text, t.bold, t.italic),
        );
        mm(ui, tr("Schriftgröße"), &mut t.size_mm, 0.5..=1000.0);
        style_choice(ui, &mut t.bold, Some(&mut t.italic));
        crate::form::row(ui, tr("Ausrichtung"), |ui| {
            let title = |a: Align| match a {
                Align::Left => tr("Links"),
                Align::Center => tr("Mitte"),
                Align::Right => tr("Rechts"),
            };
            crate::form::Dropdown::from_id_salt("text_align")
                .width(FIELD)
                .selected_text(title(t.align))
                .show_ui(ui, |ui| {
                    for a in [Align::Left, Align::Center, Align::Right] {
                        ui.selectable_value(&mut t.align, a, title(a));
                    }
                });
        });
        number(ui, tr("Zeilenabstand"), &mut t.line_spacing, 0.5..=5.0, "×");
        operation_choice(ui, "text_operation", &mut t.operation);
    }

    fn shape_form(&mut self, ui: &mut egui::Ui) {
        let s = &mut self.shape;
        shape_choice(ui, &mut s.kind);
        match s.kind {
            ShapeKind::Rectangle | ShapeKind::Ellipse => {
                mm(ui, tr("Breite"), &mut s.width_mm, 0.1..=1000.0);
                mm(ui, tr("Höhe"), &mut s.height_mm, 0.1..=1000.0);
                if s.kind == ShapeKind::Rectangle {
                    mm(ui, tr("Eckenradius"), &mut s.corner_mm, 0.0..=500.0);
                }
            }
            ShapeKind::Polygon | ShapeKind::Star => {
                let label = if s.kind == ShapeKind::Star {
                    tr("Zacken")
                } else {
                    tr("Ecken")
                };
                number(ui, label, &mut s.corners, 3..=100, "");
                mm(ui, tr("Durchmesser"), &mut s.diameter_mm, 0.1..=1000.0);
                if s.kind == ShapeKind::Star {
                    number(ui, tr("Innenradius"), &mut s.inner_percent, 5.0..=95.0, "%");
                }
            }
        }
        operation_choice(ui, "shape_operation", &mut s.operation);
    }

    fn sign_form(&mut self, ui: &mut egui::Ui) {
        let s = &mut self.sign;
        text_field(ui, &mut s.text, 2);
        mm(ui, tr("Breite"), &mut s.width_mm, 5.0..=1000.0);
        mm(ui, tr("Höhe"), &mut s.height_mm, 5.0..=600.0);
        mm(ui, tr("Eckenradius"), &mut s.corner_mm, 0.0..=300.0);
        crate::form::row(ui, tr("Löcher"), |ui| {
            crate::form::Dropdown::from_id_salt("sign_holes")
                .width(FIELD)
                .selected_text(s.holes.to_string())
                .show_ui(ui, |ui| {
                    for n in [0, 2, 4] {
                        ui.selectable_value(&mut s.holes, n, n.to_string());
                    }
                });
        });
        if s.holes > 0 {
            mm(
                ui,
                tr("Lochdurchmesser"),
                &mut s.hole_diameter_mm,
                0.5..=50.0,
            );
            mm(
                ui,
                tr("Lochabstand zum Rand"),
                &mut s.hole_inset_mm,
                0.5..=200.0,
            );
        }
        mm(
            ui,
            tr("Rahmen (markiert, 0 = ohne)"),
            &mut s.frame_inset_mm,
            0.0..=100.0,
        );
        font_choice(
            ui,
            &self.fonts,
            &mut s.font,
            &mut self.samples,
            (&s.text, s.bold, false),
        );
        mm(ui, tr("Schriftgröße"), &mut s.size_mm, 0.5..=500.0);
        style_choice(ui, &mut s.bold, None);
        crate::form::row(ui, tr("Text"), |ui| {
            crate::form::Dropdown::from_id_salt("sign_text")
                .width(FIELD)
                .selected_text(match s.text_operation {
                    Operation::Cut => tr("Schneiden (rot)"),
                    Operation::Mark => tr("Markieren (blau)"),
                    _ => tr("Gravieren (schwarz)"),
                })
                .show_ui(ui, |ui| {
                    for (o, t) in [
                        (Operation::Engrave, tr("Gravieren (schwarz)")),
                        (Operation::Cut, tr("Schneiden (rot)")),
                        (Operation::Mark, tr("Markieren (blau)")),
                    ] {
                        ui.selectable_value(&mut s.text_operation, o, t);
                    }
                });
        });
        crate::form::caption(
            ui,
            tr("Platte und Löcher werden geschnitten, der Text mittig gesetzt."),
        );
    }

    fn array_form(&mut self, ui: &mut egui::Ui, project: &Project) {
        number(ui, tr("Spalten"), &mut self.columns, 1..=50, "");
        number(ui, tr("Zeilen"), &mut self.rows, 1..=50, "");
        mm(ui, tr("Abstand X"), &mut self.gap_x_mm, -500.0..=500.0);
        mm(ui, tr("Abstand Y"), &mut self.gap_y_mm, -500.0..=500.0);
        let (w, h) = (
            self.columns as f64 * project.width_mm as f64
                + (self.columns as f64 - 1.0) * self.gap_x_mm,
            self.rows as f64 * project.height_mm as f64 + (self.rows as f64 - 1.0) * self.gap_y_mm,
        );
        crate::form::caption(
            ui,
            tr!(
                "{count} Exemplare · {width} × {height} mm; Zuordnungen gelten für alle Kopien.",
                count = self.columns * self.rows,
                width = format!("{w:.0}"),
                height = format!("{h:.0}")
            ),
        );
    }

    fn nest_form(&mut self, ui: &mut egui::Ui, project: &Project) {
        let n = &mut self.nest;
        number(ui, tr("Anzahl je Teil"), &mut n.quantity, 1..=400, "");
        mm(ui, tr("Abstand"), &mut n.gap_mm, 0.0..=50.0);
        let label = |r: NestRotation| match r {
            NestRotation::None => tr("Keine Drehung"),
            NestRotation::Quarter => tr("Nur um 90°"),
            NestRotation::Free => tr("Beliebig (ganze Grad)"),
        };
        crate::form::row(ui, tr("Drehen"), |ui| {
            crate::form::Dropdown::from_id_salt("nest_rotation")
                .width(FIELD)
                .selected_text(label(n.rotation))
                .show_ui(ui, |ui| {
                    for r in [NestRotation::None, NestRotation::Quarter, NestRotation::Free] {
                        ui.selectable_value(&mut n.rotation, r, label(r));
                    }
                })
                .response
                .on_hover_text(tr("Beliebig: jedes Teil wird zusätzlich so gedreht, dass sein umschließendes Rechteck am kleinsten ist."));
        });
        mm(ui, tr("Fläche X"), &mut n.area_mm[0], 0.0..=1000.0);
        mm(ui, tr("Fläche Y"), &mut n.area_mm[1], 0.0..=600.0);
        mm(ui, tr("Fläche Breite"), &mut n.area_mm[2], 1.0..=1000.0);
        mm(ui, tr("Fläche Höhe"), &mut n.area_mm[3], 1.0..=600.0);
        ui.horizontal(|ui| {
            if ui.button(tr("Ganzes Arbeitsbett")).clicked() {
                n.area_mm = NestSettings::default().area_mm;
            }
            if ui.button(tr("Fläche des Motivs")).clicked() {
                n.area_mm = [
                    project.x_mm as f64,
                    project.y_mm as f64,
                    project.width_mm as f64,
                    project.height_mm as f64,
                ];
            }
        });
        crate::form::caption(
            ui,
            tr(
                "Teile sind die Objekte der obersten Ebene (bzw. einer Inkscape-Ebene); \
             sie werden nach ihren Umrissrechtecken angeordnet, größte zuerst.",
            ),
        );
    }
}
