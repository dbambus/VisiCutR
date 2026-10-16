//! Bausteine der Oberfläche im Stil von Bambu Studio: dunkle Reiterleiste,
//! grüne Akzente, Abschnitte als Karten mit Überschrift, Zeilen mit
//! Bezeichnung links und Bedienelement rechts, segmentierte Auswahl.

use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke, Vec2};

/// Akzentfarbe (Grün wie in Bambu Studio).
pub const ACCENT: Color32 = Color32::from_rgb(0, 174, 66);
/// Dunkle Reiterleiste oben.
pub const TOPBAR: Color32 = Color32::from_rgb(38, 46, 48);
/// Hintergrund der Seitenleiste; die Karten heben sich weiß davon ab.
pub const SIDEBAR: Color32 = Color32::from_gray(236);
/// Hintergrund hinter dem Arbeitsbett.
pub const CANVAS: Color32 = Color32::from_gray(226);
const CARD_STROKE: Color32 = Color32::from_gray(218);
const DIVIDER: Color32 = Color32::from_gray(212);

/// Abstände, Rundungen und Rahmen etwas näher an macOS.
pub fn apply_style(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = Vec2::new(8.0, 4.0);
        style.spacing.button_padding = Vec2::new(8.0, 3.0);
        style.spacing.interact_size.y = 20.0;
        let visuals = &mut style.visuals;
        visuals.window_corner_radius = CornerRadius::same(10);
        visuals.menu_corner_radius = CornerRadius::same(8);
        visuals.hyperlink_color = ACCENT;
        visuals.selection.bg_fill = ACCENT;
        visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);
        for widget in [
            &mut visuals.widgets.noninteractive,
            &mut visuals.widgets.inactive,
            &mut visuals.widgets.hovered,
            &mut visuals.widgets.active,
            &mut visuals.widgets.open,
        ] {
            widget.corner_radius = CornerRadius::same(4);
        }
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_gray(205));
        visuals.widgets.inactive.weak_bg_fill = Color32::WHITE;
        visuals.widgets.inactive.bg_fill = Color32::WHITE;
        visuals.widgets.hovered.weak_bg_fill = Color32::from_gray(244);
        visuals.widgets.hovered.bg_fill = Color32::from_gray(244);
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_gray(185));
        visuals.widgets.active.weak_bg_fill = Color32::from_gray(228);
        visuals.widgets.active.bg_fill = Color32::from_gray(228);
    });
}

/// Abschnitt mit kleiner grauer Überschrift und Karte darunter.
pub fn section<R>(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add_space(4.0);
        ui.label(RichText::new(title).strong().color(Color32::from_gray(90)));
    });
    card(ui, add)
}

/// Weiße, abgerundete Karte über die volle Breite.
pub fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let inner = egui::Frame::new()
        .fill(Color32::WHITE)
        .stroke(Stroke::new(1.0, CARD_STROKE))
        .corner_radius(8.0)
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner;
    ui.add_space(4.0);
    inner
}

/// Zeile wie `LabeledContent`: Bezeichnung links, Bedienelement rechtsbündig.
/// Die Elemente in `add` stehen von rechts nach links.
pub fn row<R>(
    ui: &mut egui::Ui,
    label: impl Into<egui::WidgetText>,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), add)
            .inner
    })
    .inner
}

/// Kleiner grauer Erläuterungstext, umbrechend.
pub fn caption(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.add(
        egui::Label::new(
            RichText::new(text.into())
                .size(11.5)
                .color(Color32::from_gray(95)),
        )
        .wrap(),
    );
}

/// Dünne waagerechte Linie ohne Abstand (wie `Divider`).
pub fn divider(ui: &mut egui::Ui) {
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter()
        .hline(rect.x_range(), rect.center().y, Stroke::new(1.0, DIVIDER));
}

/// Segmentierte Auswahl über die volle Breite; liefert die neu gewählte
/// Option.
pub fn segmented<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    current: T,
    options: &[(T, &str)],
) -> Option<T> {
    let mut chosen = None;
    egui::Frame::new()
        .fill(Color32::from_gray(228))
        .corner_radius(7.0)
        .inner_margin(2.0)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let count = options.len().max(1) as f32;
            let width = (ui.available_width() - 2.0 * (count - 1.0)) / count;
            ui.horizontal(|ui| {
                for &(value, title) in options {
                    let selected = value == current;
                    let button = egui::Button::new(RichText::new(title).color(if selected {
                        Color32::from_gray(20)
                    } else {
                        Color32::from_gray(80)
                    }))
                    .fill(if selected {
                        Color32::WHITE
                    } else {
                        Color32::TRANSPARENT
                    })
                    .stroke(if selected {
                        Stroke::new(1.0, Color32::from_gray(200))
                    } else {
                        Stroke::NONE
                    })
                    .corner_radius(5.0)
                    .truncate();
                    if ui.add_sized([width, 22.0], button).clicked() && !selected {
                        chosen = Some(value);
                    }
                }
            });
        });
    chosen
}

/// Farbige Auswahlfläche wie der Verfahrenswähler unter macOS.
pub fn tinted_choice(
    ui: &mut egui::Ui,
    title: &str,
    color: Color32,
    selected: bool,
    width: f32,
) -> bool {
    let alpha = |a: f32| {
        Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), (a * 255.0) as u8)
    };
    let text = RichText::new(title).size(12.0).color(color);
    // Feste Breite: längere Übersetzungen dürfen die Seitenleiste nicht verbreitern.
    ui.add_sized(
        [width, 30.0],
        egui::Button::new(if selected { text.strong() } else { text })
            .fill(alpha(if selected { 0.18 } else { 0.04 }))
            .stroke(Stroke::new(1.0, alpha(if selected { 0.8 } else { 0.2 })))
            .corner_radius(6.0)
            .wrap(),
    )
    .clicked()
}

/// Hervorgehobene Schaltfläche (wie `.borderedProminent`).
pub fn prominent(ui: &mut egui::Ui, enabled: bool, title: &str) -> bool {
    ui.add_enabled(enabled, primary_button(title)).clicked()
}

/// Grüne Hauptschaltfläche (Bambu Studio „Slice“ / „Print“).
pub fn primary_button(title: &str) -> egui::Button<'static> {
    egui::Button::new(
        RichText::new(title.to_owned())
            .color(Color32::WHITE)
            .strong(),
    )
    .fill(ACCENT)
    .stroke(Stroke::NONE)
    .min_size(Vec2::new(0.0, 28.0))
}

/// Reiter der dunklen Leiste: gewählt grün hinterlegt, sonst hell auf dunkel.
pub fn top_tab(ui: &mut egui::Ui, selected: bool, title: &str) -> egui::Response {
    let text = RichText::new(title).size(15.0).color(if selected {
        Color32::WHITE
    } else {
        Color32::from_gray(215)
    });
    ui.add(
        egui::Button::new(text)
            .fill(if selected {
                ACCENT
            } else {
                Color32::TRANSPARENT
            })
            .stroke(Stroke::NONE)
            .corner_radius(0.0)
            .min_size(Vec2::new(150.0, 40.0)),
    )
}

/// Schlichte Schaltfläche auf der dunklen Leiste (Öffnen, Sichern …).
pub fn top_button(ui: &mut egui::Ui, enabled: bool, title: &str) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(
            RichText::new(title)
                .size(14.0)
                .color(Color32::from_gray(225)),
        )
        .fill(Color32::TRANSPARENT)
        .stroke(Stroke::NONE)
        .min_size(Vec2::new(0.0, 40.0)),
    )
}

/// Kopfzeile eines Seitenleisten-Bereichs wie „Printer“ / „Process“ in
/// Bambu Studio: graue Leiste über die volle Breite mit Titel.
pub fn panel_header(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(Color32::from_gray(246))
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(title)
                        .size(14.0)
                        .strong()
                        .color(Color32::from_gray(40)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), add);
            });
        });
}

/// Auswahlfeld wie in Bambu Studio: dünner heller Rahmen, Winkel links vor
/// dem Text, grüner Rahmen beim Überfahren. Gleiche Bedienung wie
/// `egui::ComboBox` (`from_id_salt`, `selected_text`, `width`, `height`,
/// `show_ui`).
pub struct Dropdown {
    id: egui::Id,
    text: egui::WidgetText,
    width: Option<f32>,
    height: Option<f32>,
}

impl Dropdown {
    pub fn from_id_salt(salt: impl std::hash::Hash) -> Self {
        Self {
            id: egui::Id::new(salt),
            text: egui::WidgetText::default(),
            width: None,
            height: None,
        }
    }

    pub fn selected_text(mut self, text: impl Into<egui::WidgetText>) -> Self {
        self.text = text.into();
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    pub fn show_ui<R>(
        self,
        ui: &mut egui::Ui,
        add: impl FnOnce(&mut egui::Ui) -> R,
    ) -> egui::InnerResponse<Option<R>> {
        let id = ui.make_persistent_id(self.id);
        let width = self.width.unwrap_or(ui.spacing().combo_width);
        let height = ui.spacing().interact_size.y.max(24.0);
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(width, height), egui::Sense::click());
        let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
        let popup = id.with("popup");
        let open = egui::Popup::is_id_open(ui.ctx(), popup);
        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            let enabled = ui.is_enabled();
            let border = if enabled && (response.hovered() || open) {
                ACCENT
            } else {
                Color32::from_gray(205)
            };
            painter.rect(
                rect,
                3.0,
                if enabled {
                    Color32::WHITE
                } else {
                    Color32::from_gray(245)
                },
                Stroke::new(1.0, border),
                egui::StrokeKind::Inside,
            );
            // Winkel „﹀“ links, offen „︿“.
            let ink = if enabled {
                Color32::from_gray(90)
            } else {
                Color32::from_gray(170)
            };
            let c = egui::pos2(rect.left() + 13.0, rect.center().y);
            let (dy, w) = (if open { -2.0 } else { 2.0 }, 4.0);
            painter.line(
                vec![
                    egui::pos2(c.x - w, c.y - dy),
                    egui::pos2(c.x, c.y + dy),
                    egui::pos2(c.x + w, c.y - dy),
                ],
                Stroke::new(1.2, ink),
            );
            let left = rect.left() + 25.0;
            let galley = self.text.into_galley(
                ui,
                Some(egui::TextWrapMode::Truncate),
                (rect.right() - 8.0 - left).max(0.0),
                egui::TextStyle::Button,
            );
            let text_color = if enabled {
                Color32::from_gray(30)
            } else {
                Color32::from_gray(150)
            };
            painter.galley(
                egui::pos2(left, rect.center().y - galley.size().y / 2.0),
                galley,
                text_color,
            );
        }
        let max_height = self.height.unwrap_or(ui.spacing().combo_height);
        let inner = egui::Popup::menu(&response)
            .id(popup)
            .width(width)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClick)
            .show(|ui| {
                egui::ScrollArea::vertical()
                    .max_height(max_height)
                    .show(ui, add)
                    .inner
            })
            .map(|r| r.inner);
        egui::InnerResponse::new(inner, response)
    }
}
