//! Parameter parametrischer Teile wie VisiCuts `ParameterPanel`: ein Feld je
//! Parameter; ist eine Eingabe abgeschlossen, wird die Grafik neu erzeugt.

use crate::form;
use eframe::egui;
use visicut_core::{
    i18n::tr,
    import::{Parameter, ParameterKind, ParameterValue},
    project::Parametric,
};

const FIELD_WIDTH: f32 = 140.0;
/// Ohne `min`/`max` in der Vorlage (VisiCut begrenzt dann gar nicht).
const LIMIT: f64 = 1.0e6;

/// Felder des aktiven Teils; berechnet sie neu, sobald sich Vorlage oder
/// Werte ändern.
#[derive(Default)]
pub struct ParametersUi {
    cached: Option<(Parametric, Result<Vec<Parameter>, String>)>,
}

impl ParametersUi {
    /// Zeigt die Felder und liefert die neuen Werte, sobald eine Eingabe
    /// abgeschlossen ist und sich etwas geändert hat.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        parametric: &Parametric,
    ) -> Option<Vec<(String, ParameterValue)>> {
        if self.cached.as_ref().is_none_or(|(p, _)| p != parametric) {
            let parameters = visicut_core::import::render_parametric(parametric)
                .map(|(_, parameters)| parameters);
            self.cached = Some((parametric.clone(), parameters));
        }
        let (_, parameters) = self.cached.as_mut()?;
        let parameters = match parameters {
            Ok(parameters) => parameters,
            Err(error) => {
                form::caption(ui, error.clone());
                return None;
            }
        };
        let mut done = false;
        for parameter in parameters.iter_mut() {
            done |= field(ui, parameter);
        }
        let values: Vec<_> = parameters
            .iter()
            .map(|p| (p.name.clone(), p.value.clone()))
            .collect();
        (done && values != parametric.values).then_some(values)
    }

    /// Verwirft eingegebene Werte, etwa wenn die Grafik nicht entstand.
    pub fn reset(&mut self) {
        self.cached = None;
    }
}

/// Ein Feld; `true`, wenn die Eingabe abgeschlossen ist.
fn field(ui: &mut egui::Ui, parameter: &mut Parameter) -> bool {
    let label = parameter.label.clone();
    let (min, max) = (
        parameter.min.unwrap_or(-LIMIT),
        parameter.max.unwrap_or(LIMIT),
    );
    form::row(ui, label, |ui| {
        if !parameter.options.is_empty() {
            let mut chosen = None;
            egui::ComboBox::from_id_salt(("parameter", &parameter.name))
                .width(FIELD_WIDTH)
                .selected_text(text(&parameter.value))
                .show_ui(ui, |ui| {
                    for option in &parameter.options {
                        if ui
                            .selectable_label(*option == parameter.value, text(option))
                            .clicked()
                        {
                            chosen = Some(option.clone());
                        }
                    }
                });
            return chosen.is_some_and(|value| {
                parameter.value = value;
                true
            });
        }
        let response = match (parameter.kind, &mut parameter.value) {
            (Some(ParameterKind::Boolean), ParameterValue::Boolean(value)) => {
                return ui.checkbox(value, "").changed();
            }
            (Some(ParameterKind::Number), ParameterValue::Number(value)) => {
                form::unit_value(ui, value, min..=max, "", FIELD_WIDTH)
            }
            (Some(ParameterKind::Integer), ParameterValue::Integer(value)) => form::unit_value(
                ui,
                value,
                min.ceil() as i64..=max.floor() as i64,
                "",
                FIELD_WIDTH,
            ),
            (Some(ParameterKind::Text), ParameterValue::Text(value)) => {
                ui.add(egui::TextEdit::singleline(value).desired_width(FIELD_WIDTH))
            }
            (_, value) => {
                ui.weak(text(value))
                    .on_hover_text(tr("Dieser Parametertyp lässt sich nicht bearbeiten"));
                return false;
            }
        };
        // Tippen und Ziehen erst am Ende übernehmen, nicht bei jedem Schritt.
        response.lost_focus()
            || response.drag_stopped()
            || (response.changed() && !response.has_focus() && !response.dragged())
    })
}

fn text(value: &ParameterValue) -> String {
    match value {
        ParameterValue::Text(text) => text.clone(),
        other => other.display(),
    }
}
