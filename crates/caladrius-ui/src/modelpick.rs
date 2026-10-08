//! The model controls shared by the fit page and the simulation page: the route and lag choice
//! with the compartment diagram and the equation of the picked model, and one editable row per
//! parameter. Neither evaluates anything; the curve comes from the engine.

use std::collections::BTreeMap;

use egui::{RichText, Ui};

use crate::fmt;
use crate::model::WorksheetInfo;
use crate::modelinfo::{self, Input, ModelInfo};
use crate::theme::Tokens;

/// How far one drag step moves a value, as a share of the value.
const DRAG_FRACTION: f64 = 0.01;
/// The smallest step of a value that is zero.
const DRAG_FLOOR: f64 = 1.0e-4;

/// The route and the lag choice, then the diagram, the equation and the parameters of the model
/// they pick. Returns true when the choice changed.
pub fn picker(ui: &mut Ui, tokens: &Tokens, input: &mut Input, lag: &mut bool, what: &str) -> bool {
    let c = &tokens.colors;
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label("Route");
        let before = *input;
        egui::ComboBox::from_id_salt("model-route")
            .selected_text(input.label())
            .show_ui(ui, |ui| {
                for choice in Input::ALL {
                    ui.selectable_value(input, choice, choice.label());
                }
            });
        if *input != before {
            changed = true;
        }
        if input.has_lag_choice() && ui.checkbox(lag, "with a lag time").changed() {
            changed = true;
        }
    });
    let model = modelinfo::pick(*input, *lag);
    ui.label(
        RichText::new(format!(
            "Compartments: 1.  Model `{}`.  {what}: {}.",
            model.id,
            model
                .parameters
                .iter()
                .map(|p| modelinfo::parameter_label(p, input.extravascular()))
                .collect::<Vec<_>>()
                .join(", ")
        ))
        .color(c.text_muted.color()),
    );
    tokens.card_frame().show(ui, |ui| {
        modelinfo::diagram(ui, tokens, model);
        for line in model.equation {
            ui.label(
                RichText::new(*line)
                    .monospace()
                    .size(tokens.font.monospace)
                    .color(c.text.color()),
            );
        }
        ui.label(
            RichText::new(model.note)
                .small()
                .color(c.text_muted.color()),
        );
    });
    changed
}

/// The unit shown after a parameter, from the units of the worksheet when there is one.
pub fn unit_of(name: &str, info: Option<&WorksheetInfo>) -> String {
    let Some(info) = info else {
        return match name {
            "k" | "ka" => "1/time".to_owned(),
            _ => String::new(),
        };
    };
    modelinfo::parameter_unit(name, info.unit_of("time"), &info.derived_units).unwrap_or_default()
}

/// One row per parameter of `model`: its label, a field for the value (type it, or drag it) and
/// its unit. `values` holds the numbers by engine name. Returns true when one changed.
pub fn parameter_rows(
    ui: &mut Ui,
    tokens: &Tokens,
    id: &str,
    model: &ModelInfo,
    values: &mut BTreeMap<String, f64>,
    info: Option<&WorksheetInfo>,
) -> bool {
    let c = &tokens.colors;
    let mut changed = false;
    egui::Grid::new(("parameters", id))
        .num_columns(3)
        .spacing([tokens.spacing.large, tokens.spacing.small])
        .show(ui, |ui| {
            for name in model.parameters {
                let label = modelinfo::parameter_label(name, model.input.extravascular());
                ui.label(label)
                    .on_hover_text(modelinfo::parameter_meaning(name));
                let value = values.entry((*name).to_owned()).or_insert(1.0);
                if number_field(ui, value, *name != "tlag") {
                    changed = true;
                }
                ui.label(RichText::new(unit_of(name, info)).color(c.text_muted.color()));
                ui.end_row();
            }
        });
    changed
}

/// A number field that shows four significant digits and keeps what was typed.
/// `positive`: the value must stay above zero (a volume or a rate constant).
pub fn number_field(ui: &mut Ui, value: &mut f64, positive: bool) -> bool {
    let speed = (value.abs() * DRAG_FRACTION).max(DRAG_FLOOR);
    let range = if positive {
        DRAG_FLOOR * DRAG_FLOOR..=f64::MAX
    } else {
        0.0..=f64::MAX
    };
    ui.add(
        egui::DragValue::new(value)
            .speed(speed)
            .range(range)
            .custom_formatter(|n, _| fmt::number(n)),
    )
    .changed()
}
