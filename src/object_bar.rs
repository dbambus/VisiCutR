//! Objektleiste über der Arbeitsfläche wie in Bambu Studio: weiße Leiste
//! mit gezeichneten Linien-Symbolen, Gruppen durch dünne Striche getrennt.

use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2, vec2};

use crate::form;

/// Symbole der Leiste.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    AddPart,
    Text,
    Shape,
    Sign,
    Transform,
    /// Skalieren: kleines Quadrat, gestrichelt vergrößert, Pfeil nach rechts oben.
    Scale,
    Rotate,
    /// Seitenverhältnis beibehalten: zwei verbundene Kettenglieder bzw.
    /// getrennte mit Bruchstrichen.
    Link,
    Unlink,
    FlipHorizontal,
    FlipVertical,
    Array,
    Nest,
    Duplicate,
    Delete,
    Undo,
    Redo,
    /// Datei-Menü in der dunklen Leiste.
    Home,
    /// Listen: Eintrag hinzufügen bzw. entfernen.
    Plus,
    Minus,
}

const SIZE: f32 = 34.0;

/// Weiße, abgerundete Leiste mit Rahmen, waagerecht zentriert; `add` füllt
/// sie von links nach rechts.
pub fn bar<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    // egui kennt die Breite erst nach dem Zeichnen: die des letzten Bildes
    // zentriert die Leiste.
    let id = ui.id().with("object_bar_width");
    let width = ui.data(|d| d.get_temp::<f32>(id));
    ui.horizontal(|ui| {
        let row = ui.available_width();
        ui.add_space(((row - width.unwrap_or(0.0)) / 2.0).max(0.0));
        let response = frame(ui, add);
        let bar = response.response.rect.width();
        if width != Some(bar) {
            ui.data_mut(|d| d.insert_temp(id, bar));
            ui.ctx().request_repaint();
        }
        min_window_width(ui.ctx(), row, bar);
        response.inner
    })
    .inner
}

/// Rand links und rechts neben der Leiste bei kleinster Fensterbreite.
const BAR_MARGIN: f32 = 12.0;
/// Kleinste Fensterhöhe (wie beim Start festgelegt).
const MIN_WINDOW_HEIGHT: f32 = 620.0;

/// Das Fenster darf nicht schmaler werden, als die Leiste mit kleinem Rand
/// braucht: Breite des Fensters ohne die Zeile (`row`) plus Leiste.
fn min_window_width(ctx: &egui::Context, row: f32, bar: f32) {
    let window = ctx.content_rect().width();
    let min = (window - row + bar + 2.0 * BAR_MARGIN).ceil();
    let id = egui::Id::new("object_bar_min_window_width");
    if ctx.data(|d| d.get_temp::<f32>(id)) != Some(min) {
        ctx.data_mut(|d| d.insert_temp(id, min));
        ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(egui::vec2(
            min,
            MIN_WINDOW_HEIGHT,
        )));
    }
}

fn frame<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> egui::InnerResponse<R> {
    egui::Frame::new()
        .fill(Color32::WHITE)
        .stroke(Stroke::new(1.0, Color32::from_gray(214)))
        .corner_radius(8.0)
        .inner_margin(egui::Margin::symmetric(6, 4))
        .shadow(egui::Shadow {
            offset: [0, 1],
            blur: 6,
            spread: 0,
            color: Color32::from_black_alpha(18),
        })
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.horizontal(add).inner
        })
}

/// Senkrechter Trennstrich zwischen Gruppen.
pub fn separator(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(11.0, SIZE), egui::Sense::hover());
    ui.painter().vline(
        rect.center().x,
        rect.y_range().shrink(7.0),
        Stroke::new(1.0, Color32::from_gray(218)),
    );
}

/// Symbolschaltfläche; `selected` hinterlegt sie (offenes Aufklappfeld).
pub fn button(
    ui: &mut egui::Ui,
    icon: Icon,
    enabled: bool,
    selected: bool,
    hover: &str,
) -> egui::Response {
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(SIZE), sense);
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        if selected {
            painter.rect_filled(rect, 5.0, form::ACCENT.gamma_multiply(0.16));
        } else if enabled && response.hovered() {
            painter.rect_filled(rect, 5.0, Color32::from_gray(236));
        }
        let color = if !enabled {
            Color32::from_gray(190)
        } else if selected {
            form::ACCENT
        } else {
            Color32::from_gray(50)
        };
        paint(painter, icon, rect, color);
    }
    let response = if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    };
    response.on_hover_text(hover)
}

/// Knopfzeile am unteren Rand einer Liste: einzelne Symbolknöpfe
/// (Hinzufügen, Duplizieren, Entfernen) nebeneinander, gesetzt mit
/// [`list_button`]; `width` hält die Zeile so breit wie die Liste.
pub fn list_footer<R>(ui: &mut egui::Ui, width: f32, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.horizontal(|ui| {
        ui.set_width(width);
        ui.spacing_mut().item_spacing.x = 6.0;
        add(ui)
    })
    .inner
}

/// Eigener kleiner Symbolknopf mit Rahmen wie die übrigen Knöpfe.
pub fn list_button(ui: &mut egui::Ui, icon: Icon, enabled: bool, hover: &str) -> bool {
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(vec2(32.0, ui.spacing().interact_size.y), sense);
    if ui.is_rect_visible(rect) {
        let visuals = if enabled {
            ui.style().interact(&response)
        } else {
            &ui.style().visuals.widgets.inactive
        };
        ui.painter().rect(
            rect,
            visuals.corner_radius,
            visuals.weak_bg_fill,
            visuals.bg_stroke,
            egui::StrokeKind::Inside,
        );
        let color = if enabled {
            Color32::from_gray(50)
        } else {
            Color32::from_gray(190)
        };
        let icon_rect = Rect::from_center_size(rect.center(), Vec2::splat(22.0));
        paint(ui.painter(), icon, icon_rect, color);
    }
    response.on_hover_text(hover).clicked() && enabled
}

/// Zeichnet ein Symbol in einem Raster von −10 … 10 um die Mitte von `rect`.
pub fn paint(painter: &egui::Painter, icon: Icon, rect: Rect, color: Color32) {
    let pen = Pen {
        painter,
        center: rect.center(),
        scale: rect.width() / 30.0,
        color,
    };
    match icon {
        Icon::AddPart => add_part(pen),
        Icon::Text => text(pen),
        Icon::Shape => shape(pen),
        Icon::Sign => sign(pen),
        Icon::Transform => transform(pen),
        Icon::Scale => scale(pen),
        Icon::Link => chain(pen, Chain::Linked),
        Icon::Unlink => chain(pen, Chain::Broken),
        Icon::Rotate => rotate(pen),
        Icon::FlipHorizontal => {
            pen.dashed((0.0, -10.0), (0.0, 10.0));
            pen.filled(&[(-2.5, -7.0), (-2.5, 7.0), (-10.0, 7.0)]);
            pen.closed_path(&[(2.5, -7.0), (2.5, 7.0), (10.0, 7.0)]);
        }
        Icon::FlipVertical => {
            pen.dashed((-10.0, 0.0), (10.0, 0.0));
            pen.filled(&[(-7.0, -2.5), (7.0, -2.5), (7.0, -10.0)]);
            pen.closed_path(&[(-7.0, 2.5), (7.0, 2.5), (7.0, 10.0)]);
        }
        Icon::Array => array(pen),
        Icon::Nest => nest(pen),
        Icon::Duplicate => duplicate(pen),
        Icon::Undo => undo(pen, 1.0),
        Icon::Redo => undo(pen, -1.0),
        Icon::Home => home(pen),
        Icon::Plus => {
            pen.path(&[(0.0, -7.0), (0.0, 7.0)]);
            pen.path(&[(-7.0, 0.0), (7.0, 0.0)]);
        }
        Icon::Minus => pen.path(&[(-7.0, 0.0), (7.0, 0.0)]),
        Icon::Delete => delete(pen),
    }
}

/// Zeichenstift eines Symbols: Rasterpunkte (−10 … 10) um `center`, je
/// Einheit `scale` Bildpunkte, Linien in `color`.
#[derive(Clone, Copy)]
struct Pen<'a> {
    painter: &'a egui::Painter,
    center: Pos2,
    scale: f32,
    color: Color32,
}

impl Pen<'_> {
    fn p(self, x: f32, y: f32) -> Pos2 {
        self.center + vec2(x, y) * self.scale
    }

    fn r(self, x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
        Rect::from_min_max(self.p(x0, y0), self.p(x1, y1))
    }

    /// Normale Linie.
    fn line(self) -> Stroke {
        Stroke::new(1.4, self.color)
    }

    fn bold(self) -> Stroke {
        Stroke::new(2.0, self.color)
    }

    fn points(self, points: &[(f32, f32)]) -> Vec<Pos2> {
        points.iter().map(|&(x, y)| self.p(x, y)).collect()
    }

    fn outline(self, rect: Rect, radius: f32) {
        self.painter
            .rect_stroke(rect, radius, self.line(), egui::StrokeKind::Middle);
    }

    fn path(self, points: &[(f32, f32)]) {
        self.painter
            .add(egui::Shape::line(self.points(points), self.line()));
    }

    fn closed_path(self, points: &[(f32, f32)]) {
        self.painter
            .add(egui::Shape::closed_line(self.points(points), self.line()));
    }

    fn filled(self, points: &[(f32, f32)]) {
        self.painter.add(egui::Shape::convex_polygon(
            self.points(points),
            self.color,
            Stroke::NONE,
        ));
    }

    fn dashed(self, a: (f32, f32), b: (f32, f32)) {
        self.painter.add(egui::Shape::dashed_line(
            &[self.p(a.0, a.1), self.p(b.0, b.1)],
            Stroke::new(1.0, self.color),
            2.0 * self.scale,
            2.0 * self.scale,
        ));
    }
}

fn add_part(pen: Pen) {
    let s = pen.scale;
    pen.outline(pen.r(-9.0, -9.0, 4.0, 4.0), 1.5 * s);
    pen.painter
        .circle(pen.p(4.5, 4.5), 5.0 * s, Color32::WHITE, pen.line());
    pen.path(&[(4.5, 1.5), (4.5, 7.5)]);
    pen.path(&[(1.5, 4.5), (7.5, 4.5)]);
}

fn text(pen: Pen) {
    let painter = pen.painter;
    painter.line_segment([pen.p(-7.0, -8.0), pen.p(7.0, -8.0)], pen.bold());
    painter.line_segment([pen.p(0.0, -8.0), pen.p(0.0, 8.5)], pen.bold());
    painter.line_segment([pen.p(-3.0, 8.5), pen.p(3.0, 8.5)], pen.line());
}

fn shape(pen: Pen) {
    pen.painter
        .circle_stroke(pen.p(-3.0, -3.0), 6.0 * pen.scale, pen.line());
    pen.outline(pen.r(-1.0, -1.0, 9.0, 9.0), 0.0);
}

fn sign(pen: Pen) {
    let s = pen.scale;
    pen.outline(pen.r(-10.0, -6.5, 10.0, 6.5), 2.5 * s);
    pen.path(&[(-5.5, -1.5), (5.5, -1.5)]);
    pen.path(&[(-5.5, 2.5), (2.0, 2.5)]);
    pen.painter
        .circle_filled(pen.p(-7.5, -4.0), 0.9 * s, pen.color);
    pen.painter
        .circle_filled(pen.p(7.5, -4.0), 0.9 * s, pen.color);
}

fn transform(pen: Pen) {
    let frame = [(-7.0, -7.0), (7.0, -7.0), (7.0, 7.0), (-7.0, 7.0)];
    for i in 0..4 {
        pen.dashed(frame[i], frame[(i + 1) % 4]);
    }
    for (x, y) in frame {
        pen.painter
            .rect_filled(pen.r(x - 2.0, y - 2.0, x + 2.0, y + 2.0), 0.0, pen.color);
    }
}

fn scale(pen: Pen) {
    pen.outline(pen.r(-9.0, 1.0, -1.0, 9.0), 0.0);
    for (a, b) in [
        ((-9.0, -9.0), (9.0, -9.0)),
        ((9.0, -9.0), (9.0, 9.0)),
        ((-1.0, 9.0), (9.0, 9.0)),
        ((-9.0, -9.0), (-9.0, 1.0)),
    ] {
        pen.dashed(a, b);
    }
    pen.path(&[(1.0, -1.0), (5.0, -5.0)]);
    pen.filled(&[(6.5, -6.5), (1.5, -5.5), (5.5, -1.5)]);
}

/// Kettensymbol verbunden (Seitenverhältnis beibehalten) oder getrennt.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Chain {
    Linked,
    Broken,
}

fn chain(pen: Pen, state: Chain) {
    // Wie das übliche Kettensymbol: zwei zur Mitte offene Glieder,
    // verbunden durch einen Steg; schräg nach rechts oben gedreht.
    let linked = state == Chain::Linked;
    let (sin, cos) = (-45.0_f32).to_radians().sin_cos();
    let turn = |(x, y): (f32, f32)| (x * cos - y * sin, x * sin + y * cos);
    let (r, center, open) = if linked {
        (3.4, 4.6, 1.6)
    } else {
        (3.4, 5.6, 2.8)
    };
    for side in [-1.0_f32, 1.0] {
        let mut points = vec![(side * open, -r)];
        for i in 0..=16 {
            // Von oben über die Außenseite nach unten.
            let a = (-90.0 + side * 180.0 * i as f32 / 16.0).to_radians();
            points.push((side * center + r * a.cos(), r * a.sin()));
        }
        points.push((side * open, r));
        let points: Vec<(f32, f32)> = points.into_iter().map(turn).collect();
        pen.path(&points);
    }
    if linked {
        pen.path(&[turn((-2.8, 0.0)), turn((2.8, 0.0))]);
    } else {
        // Bruchstriche quer im Spalt.
        pen.path(&[turn((-0.9, -4.0)), turn((-0.9, -6.0))]);
        pen.path(&[turn((0.9, 4.0)), turn((0.9, 6.0))]);
    }
}

fn rotate(pen: Pen) {
    let arc: Vec<(f32, f32)> = (0..=24)
        .map(|i| {
            let a = (-60.0 + 270.0 * i as f32 / 24.0).to_radians();
            (7.5 * a.cos(), 7.5 * a.sin())
        })
        .collect();
    pen.path(&arc);
    // Pfeilspitze am Anfang des Bogens, im Uhrzeigersinn.
    let (ex, ey) = arc[0];
    pen.filled(&[
        (ex - 3.5, ey - 1.0),
        (ex + 2.8, ey - 2.2),
        (ex + 0.8, ey + 3.8),
    ]);
}

fn array(pen: Pen) {
    for row in 0..2 {
        for col in 0..3 {
            let x = -9.5 + col as f32 * 7.0;
            let y = -6.5 + row as f32 * 7.0;
            pen.outline(pen.r(x, y, x + 5.0, y + 5.0), 0.8 * pen.scale);
        }
    }
}

fn nest(pen: Pen) {
    pen.outline(pen.r(-10.0, -9.0, 10.0, 9.0), 1.5 * pen.scale);
    pen.closed_path(&[(-7.0, 6.0), (-1.0, 6.0), (-7.0, -6.0)]);
    pen.painter
        .circle_stroke(pen.p(4.0, -2.5), 3.5 * pen.scale, pen.line());
    pen.outline(pen.r(1.0, 3.0, 7.0, 6.5), 0.0);
}

fn duplicate(pen: Pen) {
    let s = pen.scale;
    pen.outline(pen.r(-9.0, -9.0, 3.0, 3.0), 1.5 * s);
    pen.painter.rect(
        pen.r(-3.0, -3.0, 9.0, 9.0),
        1.5 * s,
        Color32::WHITE,
        pen.line(),
        egui::StrokeKind::Middle,
    );
}

/// Widerrufen (`m` = 1) oder Wiederholen (`m` = −1).
fn undo(pen: Pen, m: f32) {
    // Bogen von oben über rechts nach unten, Pfeil links oben;
    // Wiederholen gespiegelt.
    let arc = (0..=20).map(|i| {
        let a = (-90.0 + 180.0 * i as f32 / 20.0).to_radians();
        (m * (1.0 + 6.5 * a.cos()), 0.5 + 6.5 * a.sin())
    });
    let points: Vec<(f32, f32)> = std::iter::once((m * -6.0, -6.0))
        .chain(arc)
        .chain(std::iter::once((m * -5.0, 7.0)))
        .collect();
    pen.path(&points);
    pen.filled(&[(m * -9.5, -6.0), (m * -4.5, -9.5), (m * -4.5, -2.5)]);
}

fn home(pen: Pen) {
    let p = |x, y| pen.p(x, y);
    let line = Stroke::new(1.7, pen.color);
    let roof = [p(-10.0, -0.5), p(0.0, -9.5), p(10.0, -0.5)];
    pen.painter.add(egui::Shape::line(roof.to_vec(), line));
    let body = [p(-7.0, -3.0), p(-7.0, 9.0), p(7.0, 9.0), p(7.0, -3.0)];
    pen.painter.add(egui::Shape::line(body.to_vec(), line));
    pen.painter.add(egui::Shape::line(
        vec![p(-2.5, 9.0), p(-2.5, 3.0), p(2.5, 3.0), p(2.5, 9.0)],
        line,
    ));
}

fn delete(pen: Pen) {
    pen.path(&[(-8.0, -6.0), (8.0, -6.0)]);
    pen.path(&[(-3.0, -6.0), (-2.0, -9.0), (2.0, -9.0), (3.0, -6.0)]);
    pen.path(&[(-6.0, -6.0), (-5.0, 9.0), (5.0, 9.0), (6.0, -6.0)]);
    pen.path(&[(-2.0, -2.5), (-1.7, 5.5)]);
    pen.path(&[(2.0, -2.5), (1.7, 5.5)]);
}
