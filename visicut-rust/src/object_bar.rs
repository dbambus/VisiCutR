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
    Rotate,
    FlipHorizontal,
    FlipVertical,
    Center,
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
        ui.add_space(((ui.available_width() - width.unwrap_or(0.0)) / 2.0).max(0.0));
        let response = frame(ui, add);
        if width != Some(response.response.rect.width()) {
            ui.data_mut(|d| d.insert_temp(id, response.response.rect.width()));
            ui.ctx().request_repaint();
        }
        response.inner
    })
    .inner
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
    let (rect, response) = ui.allocate_exact_size(vec2(32.0, 26.0), sense);
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
    let c = rect.center();
    let s = rect.width() / 30.0;
    let p = |x: f32, y: f32| -> Pos2 { c + vec2(x, y) * s };
    let line = Stroke::new(1.4, color);
    let bold = Stroke::new(2.0, color);
    let r = |x0: f32, y0: f32, x1: f32, y1: f32| Rect::from_min_max(p(x0, y0), p(x1, y1));
    let outline = |rect: Rect, radius: f32| {
        painter.rect_stroke(rect, radius, line, egui::StrokeKind::Middle);
    };
    let path = |points: &[(f32, f32)], closed: bool| {
        let points: Vec<Pos2> = points.iter().map(|&(x, y)| p(x, y)).collect();
        if closed {
            painter.add(egui::Shape::closed_line(points, line));
        } else {
            painter.add(egui::Shape::line(points, line));
        }
    };
    let filled = |points: &[(f32, f32)]| {
        let points: Vec<Pos2> = points.iter().map(|&(x, y)| p(x, y)).collect();
        painter.add(egui::Shape::convex_polygon(points, color, Stroke::NONE));
    };
    let dashed = |a: (f32, f32), b: (f32, f32)| {
        painter.add(egui::Shape::dashed_line(
            &[p(a.0, a.1), p(b.0, b.1)],
            Stroke::new(1.0, color),
            2.0 * s,
            2.0 * s,
        ));
    };
    match icon {
        Icon::AddPart => {
            outline(r(-9.0, -9.0, 4.0, 4.0), 1.5 * s);
            painter.circle(p(4.5, 4.5), 5.0 * s, Color32::WHITE, line);
            path(&[(4.5, 1.5), (4.5, 7.5)], false);
            path(&[(1.5, 4.5), (7.5, 4.5)], false);
        }
        Icon::Text => {
            painter.line_segment([p(-7.0, -8.0), p(7.0, -8.0)], bold);
            painter.line_segment([p(0.0, -8.0), p(0.0, 8.5)], bold);
            painter.line_segment([p(-3.0, 8.5), p(3.0, 8.5)], line);
        }
        Icon::Shape => {
            painter.circle_stroke(p(-3.0, -3.0), 6.0 * s, line);
            outline(r(-1.0, -1.0, 9.0, 9.0), 0.0);
        }
        Icon::Sign => {
            outline(r(-10.0, -6.5, 10.0, 6.5), 2.5 * s);
            path(&[(-5.5, -1.5), (5.5, -1.5)], false);
            path(&[(-5.5, 2.5), (2.0, 2.5)], false);
            painter.circle_filled(p(-7.5, -4.0), 0.9 * s, color);
            painter.circle_filled(p(7.5, -4.0), 0.9 * s, color);
        }
        Icon::Transform => {
            let frame = [(-7.0, -7.0), (7.0, -7.0), (7.0, 7.0), (-7.0, 7.0)];
            for i in 0..4 {
                dashed(frame[i], frame[(i + 1) % 4]);
            }
            for (x, y) in frame {
                painter.rect_filled(r(x - 2.0, y - 2.0, x + 2.0, y + 2.0), 0.0, color);
            }
        }
        Icon::Rotate => {
            let arc: Vec<(f32, f32)> = (0..=24)
                .map(|i| {
                    let a = (-60.0 + 270.0 * i as f32 / 24.0).to_radians();
                    (7.5 * a.cos(), 7.5 * a.sin())
                })
                .collect();
            path(&arc, false);
            // Pfeilspitze am Anfang des Bogens, im Uhrzeigersinn.
            let (ex, ey) = arc[0];
            filled(&[
                (ex - 3.5, ey - 1.0),
                (ex + 2.8, ey - 2.2),
                (ex + 0.8, ey + 3.8),
            ]);
        }
        Icon::FlipHorizontal => {
            dashed((0.0, -10.0), (0.0, 10.0));
            filled(&[(-2.5, -7.0), (-2.5, 7.0), (-10.0, 7.0)]);
            path(&[(2.5, -7.0), (2.5, 7.0), (10.0, 7.0)], true);
        }
        Icon::FlipVertical => {
            dashed((-10.0, 0.0), (10.0, 0.0));
            filled(&[(-7.0, -2.5), (7.0, -2.5), (7.0, -10.0)]);
            path(&[(-7.0, 2.5), (7.0, 2.5), (7.0, 10.0)], true);
        }
        Icon::Center => {
            outline(r(-9.5, -9.5, 9.5, 9.5), 1.5 * s);
            outline(r(-4.0, -4.0, 4.0, 4.0), 0.0);
            for (a, b) in [
                ((0.0, -9.5), (0.0, -6.0)),
                ((0.0, 6.0), (0.0, 9.5)),
                ((-9.5, 0.0), (-6.0, 0.0)),
                ((6.0, 0.0), (9.5, 0.0)),
            ] {
                path(&[a, b], false);
            }
        }
        Icon::Array => {
            for row in 0..2 {
                for col in 0..3 {
                    let x = -9.5 + col as f32 * 7.0;
                    let y = -6.5 + row as f32 * 7.0;
                    outline(r(x, y, x + 5.0, y + 5.0), 0.8 * s);
                }
            }
        }
        Icon::Nest => {
            outline(r(-10.0, -9.0, 10.0, 9.0), 1.5 * s);
            path(&[(-7.0, 6.0), (-1.0, 6.0), (-7.0, -6.0)], true);
            painter.circle_stroke(p(4.0, -2.5), 3.5 * s, line);
            outline(r(1.0, 3.0, 7.0, 6.5), 0.0);
        }
        Icon::Duplicate => {
            outline(r(-9.0, -9.0, 3.0, 3.0), 1.5 * s);
            painter.rect(
                r(-3.0, -3.0, 9.0, 9.0),
                1.5 * s,
                Color32::WHITE,
                line,
                egui::StrokeKind::Middle,
            );
        }
        Icon::Undo | Icon::Redo => {
            // Bogen von oben über rechts nach unten, Pfeil links oben;
            // Wiederholen gespiegelt.
            let m = if icon == Icon::Redo { -1.0 } else { 1.0 };
            let arc = (0..=20).map(|i| {
                let a = (-90.0 + 180.0 * i as f32 / 20.0).to_radians();
                (m * (1.0 + 6.5 * a.cos()), 0.5 + 6.5 * a.sin())
            });
            let points: Vec<(f32, f32)> = std::iter::once((m * -6.0, -6.0))
                .chain(arc)
                .chain(std::iter::once((m * -5.0, 7.0)))
                .collect();
            path(&points, false);
            filled(&[(m * -9.5, -6.0), (m * -4.5, -9.5), (m * -4.5, -2.5)]);
        }
        Icon::Home => {
            let line = Stroke::new(1.7, color);
            let roof = [p(-10.0, -0.5), p(0.0, -9.5), p(10.0, -0.5)];
            painter.add(egui::Shape::line(roof.to_vec(), line));
            let body = [p(-7.0, -3.0), p(-7.0, 9.0), p(7.0, 9.0), p(7.0, -3.0)];
            painter.add(egui::Shape::line(body.to_vec(), line));
            painter.add(egui::Shape::line(
                vec![p(-2.5, 9.0), p(-2.5, 3.0), p(2.5, 3.0), p(2.5, 9.0)],
                line,
            ));
        }
        Icon::Plus => {
            path(&[(0.0, -7.0), (0.0, 7.0)], false);
            path(&[(-7.0, 0.0), (7.0, 0.0)], false);
        }
        Icon::Minus => path(&[(-7.0, 0.0), (7.0, 0.0)], false),
        Icon::Delete => {
            path(&[(-8.0, -6.0), (8.0, -6.0)], false);
            path(
                &[(-3.0, -6.0), (-2.0, -9.0), (2.0, -9.0), (3.0, -6.0)],
                false,
            );
            path(&[(-6.0, -6.0), (-5.0, 9.0), (5.0, 9.0), (6.0, -6.0)], false);
            path(&[(-2.0, -2.5), (-1.7, 5.5)], false);
            path(&[(2.0, -2.5), (1.7, 5.5)], false);
        }
    }
}
