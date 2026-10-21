//! Bausteine der Oberfläche im Stil von Bambu Studio: dunkle Reiterleiste,
//! orange Akzente, Abschnitte als Karten mit Überschrift, Zeilen mit
//! Bezeichnung links und Bedienelement rechts, segmentierte Auswahl.

use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke, Vec2};

/// Akzentfarbe (gedecktes Orange, dunkel genug für weiße Schrift).
pub const ACCENT: Color32 = Color32::from_rgb(192, 98, 30);
/// Dunkle Reiterleiste oben.
pub const TOPBAR: Color32 = Color32::from_rgb(38, 46, 48);
/// Hintergrund der Seitenleiste (weiß wie in Bambu Studio).
pub const SIDEBAR: Color32 = Color32::WHITE;
/// Kopfleisten der Bereiche.
const HEADER: Color32 = Color32::from_gray(233);
/// Linien über und unter den Bereichsköpfen, dunkler als [`DIVIDER`].
const HEADER_LINE: Color32 = Color32::from_gray(178);
/// Hintergrund hinter dem Arbeitsbett.
pub const CANVAS: Color32 = Color32::from_gray(226);
const CARD_STROKE: Color32 = Color32::from_gray(218);
const DIVIDER: Color32 = Color32::from_gray(212);

/// Abstände, Rundungen und Rahmen etwas näher an macOS.
pub fn apply_style(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = Vec2::new(8.0, 4.0);
        style.spacing.button_padding = Vec2::new(8.0, 3.0);
        // Eine Höhe für alle Knöpfe, Felder und Auswahlen.
        style.spacing.interact_size.y = 24.0;
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
        // Dunklere Schrift als egui (Grau 80/60): besser lesbar.
        visuals.widgets.noninteractive.fg_stroke.color = Color32::from_gray(30);
        visuals.widgets.inactive.fg_stroke.color = Color32::from_gray(25);
        visuals.widgets.hovered.fg_stroke.color = Color32::from_gray(10);
        visuals.widgets.active.fg_stroke.color = Color32::BLACK;
        visuals.weak_text_color = Some(Color32::from_gray(95));
        visuals.disabled_alpha = 0.6;
    });
}

/// Systemschrift in normaler Stärke statt egui's dünner Ubuntu-Light;
/// Ubuntu-Light bleibt als Ersatz für fehlende Zeichen.
pub fn apply_fonts(ctx: &egui::Context) {
    const CANDIDATES: &[&str] = &[
        "/System/Library/Fonts/SFNS.ttf",
        "/System/Library/Fonts/Helvetica.ttc",
        "C:\\Windows\\Fonts\\segoeui.ttf",
        "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ];
    // Fetter Schnitt für Überschriften: Datei und Index in einer Sammlung.
    const BOLD: &[(&str, u32)] = &[
        ("/System/Library/Fonts/HelveticaNeue.ttc", 1),
        ("/System/Library/Fonts/Helvetica.ttc", 1),
        ("C:\\Windows\\Fonts\\segoeuib.ttf", 0),
        ("/usr/share/fonts/truetype/noto/NotoSans-Bold.ttf", 0),
        ("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf", 0),
    ];
    let mut fonts = egui::FontDefinitions::default();
    if let Some(data) = CANDIDATES.iter().find_map(|path| std::fs::read(path).ok()) {
        fonts
            .font_data
            .insert("system".to_owned(), egui::FontData::from_owned(data).into());
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "system".to_owned());
    }
    // Die Familie gibt es immer (sonst bricht egui ab); ohne Fettschnitt
    // fällt sie auf die normale Schrift zurück.
    let mut bold = fonts.families[&egui::FontFamily::Proportional].clone();
    if let Some((data, index)) = BOLD
        .iter()
        .find_map(|(path, index)| std::fs::read(path).ok().map(|d| (d, *index)))
    {
        let mut font = egui::FontData::from_owned(data);
        font.index = index;
        fonts.font_data.insert("bold".to_owned(), font.into());
        bold.insert(0, "bold".to_owned());
    }
    fonts
        .families
        .insert(egui::FontFamily::Name(BOLD_FAMILY.into()), bold);
    ctx.set_fonts(fonts);
}

const BOLD_FAMILY: &str = "bold";

/// Fette Schrift in `size` Punkten (Überschriften).
pub fn bold(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name(BOLD_FAMILY.into()))
}

/// Bereich wie „Process“ in Bambu Studio: graue Kopfleiste über die volle
/// Breite, darunter der Inhalt mit Rand.
pub fn section<R>(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    panel_header(ui, title, |_| {});
    egui::Frame::new()
        .inner_margin(egui::Margin {
            left: 14,
            right: 14,
            top: 8,
            bottom: 10,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// Gruppentitel wie „Layer height ———“: fett, rechts eine dünne Linie.
pub fn group(ui: &mut egui::Ui, title: &str) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(title)
                .strong()
                .size(13.5)
                .color(Color32::from_gray(30)),
        );
        let (rect, _) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), ui.spacing().interact_size.y),
            egui::Sense::hover(),
        );
        ui.painter()
            .hline(rect.x_range(), rect.center().y, Stroke::new(1.0, DIVIDER));
    });
}

/// Zahlenfeld wie in Bambu Studio: feste Breite, Einheit grau rechts im Feld.
pub fn unit_value<N: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    value: &mut N,
    range: std::ops::RangeInclusive<N>,
    unit: &str,
    width: f32,
) -> egui::Response {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 24.0), egui::Sense::hover());
    let unit_galley = ui.painter().layout_no_wrap(
        unit.to_owned(),
        egui::FontId::proportional(12.0),
        Color32::from_gray(95),
    );
    let field = egui::Rect::from_min_max(
        rect.min + Vec2::new(1.0, 1.0),
        egui::pos2(
            rect.right() - unit_galley.size().x - 10.0,
            rect.bottom() - 1.0,
        ),
    );
    let response = ui
        .scope_builder(egui::UiBuilder::new().max_rect(field), |ui| {
            // Den Rahmen zeichnet das Feld selbst.
            let visuals = &mut ui.visuals_mut().widgets;
            for widget in [
                &mut visuals.inactive,
                &mut visuals.hovered,
                &mut visuals.active,
            ] {
                widget.bg_stroke = Stroke::NONE;
                widget.weak_bg_fill = Color32::TRANSPARENT;
                widget.bg_fill = Color32::TRANSPARENT;
            }
            ui.add_sized(
                field.size(),
                egui::DragValue::new(value)
                    .speed(0.1)
                    .range(range)
                    .max_decimals(2),
            )
        })
        .inner;
    let active = response.hovered() || response.has_focus() || response.dragged();
    ui.painter().rect_stroke(
        rect,
        3.0,
        Stroke::new(
            1.0,
            if active {
                ACCENT
            } else {
                Color32::from_gray(205)
            },
        ),
        egui::StrokeKind::Inside,
    );
    let unit_pos = egui::pos2(
        rect.right() - unit_galley.size().x - 7.0,
        rect.center().y - unit_galley.size().y / 2.0,
    );
    ui.painter()
        .galley(unit_pos, unit_galley, Color32::from_gray(95));
    response
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
                .color(Color32::from_gray(75)),
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

/// Umschalter wie „Global / Objects“ in Bambu Studio: graue Pille, die
/// gewählte Option orange; liefert die neu gewählte Option.
pub fn segmented<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    current: T,
    options: &[(T, &str)],
) -> Option<T> {
    let options: Vec<(T, &str, Color32)> = options.iter().map(|&(v, t)| (v, t, ACCENT)).collect();
    segmented_colored(ui, current, &options)
}

/// [`segmented`] mit eigener Farbe je Wahl: der gewählte Teil nimmt seine
/// Farbe an (z. B. die Verfahrensfarbe).
pub fn segmented_colored<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    current: T,
    options: &[(T, &str, Color32)],
) -> Option<T> {
    let mut chosen = None;
    egui::Frame::new()
        .fill(Color32::from_gray(232))
        .corner_radius(13.0)
        .inner_margin(2.0)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let count = options.len().max(1) as f32;
            let width = ui.available_width() / count;
            ui.horizontal(|ui| {
                for &(value, title, color) in options {
                    let selected = value == current;
                    let text = RichText::new(title).color(if selected {
                        Color32::WHITE
                    } else {
                        Color32::from_gray(55)
                    });
                    let button = egui::Button::new(if selected { text.strong() } else { text })
                        .fill(if selected {
                            color
                        } else {
                            Color32::TRANSPARENT
                        })
                        .stroke(Stroke::NONE)
                        .corner_radius(11.0)
                        .truncate();
                    if ui.add_sized([width, 22.0], button).clicked() && !selected {
                        chosen = Some(value);
                    }
                }
            });
        });
    chosen
}

/// Hervorgehobene Schaltfläche (wie `.borderedProminent`).
pub fn prominent(ui: &mut egui::Ui, enabled: bool, title: &str) -> bool {
    ui.add_enabled(enabled, primary_button(title)).clicked()
}

/// Orange Hauptschaltfläche (Bambu Studio „Slice“ / „Print“).
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

/// Reiter der dunklen Leiste: gewählt orange hinterlegt, sonst hell auf dunkel.
pub fn top_tab(ui: &mut egui::Ui, selected: bool, title: &str) -> egui::Response {
    let text = RichText::new(title).size(15.0).color(if selected {
        Color32::WHITE
    } else {
        Color32::from_gray(232)
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

/// Symbol auf der dunklen Leiste (Haus für das Datei-Menü).
pub fn top_icon(ui: &mut egui::Ui, icon: crate::object_bar::Icon) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(48.0, 40.0), egui::Sense::click());
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&response));
    if response.hovered() || open {
        ui.painter().rect_filled(rect, 0.0, Color32::from_gray(62));
    }
    let icon_rect = egui::Rect::from_center_size(rect.center(), Vec2::splat(30.0));
    crate::object_bar::paint(ui.painter(), icon, icon_rect, Color32::from_gray(235));
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Kopfzeile eines Seitenleisten-Bereichs wie „Printer“ / „Process“ in
/// Bambu Studio: graue Leiste über die volle Breite mit Titel.
pub fn panel_header(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    let response = egui::Frame::new()
        .fill(HEADER)
        .inner_margin(egui::Margin::symmetric(12, 7))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(title)
                        .size(14.5)
                        .color(Color32::from_gray(25)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), add);
            });
        })
        .response;
    let rect = response.rect;
    let stroke = Stroke::new(1.0, HEADER_LINE);
    ui.painter().hline(rect.x_range(), rect.top(), stroke);
    ui.painter().hline(rect.x_range(), rect.bottom(), stroke);
}

/// Auswahlfeld wie in Bambu Studio: dünner heller Rahmen, Winkel links vor
/// dem Text, orangefarbener Rahmen beim Überfahren. Gleiche Bedienung wie
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
        // Ohne feste Breite so breit wie der Text (mindestens `combo_width`).
        let natural = self
            .text
            .clone()
            .into_galley(
                ui,
                Some(egui::TextWrapMode::Extend),
                f32::INFINITY,
                egui::TextStyle::Button,
            )
            .size()
            .x
            + 35.0;
        let width = self
            .width
            .unwrap_or_else(|| natural.max(ui.spacing().combo_width));
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
                Color32::from_gray(130)
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

/// Überschrift einer Einstellungsseite, darunter eine doppelte Linie über
/// die ganze Breite.
pub fn page_heading(ui: &mut egui::Ui, title: &str) {
    let size = ui
        .style()
        .text_styles
        .get(&egui::TextStyle::Heading)
        .map_or(20.0, |font| font.size);
    ui.label(RichText::new(title).font(bold(size)));
    ui.add_space(4.0);
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 5.0), egui::Sense::hover());
    let stroke = egui::Stroke::new(1.0, Color32::from_gray(150));
    ui.painter().hline(rect.x_range(), rect.top() + 0.5, stroke);
    ui.painter()
        .hline(rect.x_range(), rect.bottom() - 0.5, stroke);
    ui.add_space(8.0);
}
