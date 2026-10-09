//! The model controls shared by the fit page and the simulation page: the route and lag choice
//! with the compartment diagram and the equation of the picked model, and one editable row per
//! parameter. Neither evaluates anything; the curve comes from the engine.

use std::collections::{BTreeMap, BTreeSet};

use egui::{RichText, Ui};

use crate::fmt;
use crate::model::WorksheetInfo;
use crate::modelinfo::{self, Compartments, Input, ParameterSet};
use crate::theme::Tokens;

/// How far one drag step moves a value, as a share of the value.
const DRAG_FRACTION: f64 = 0.01;
/// The smallest step of a value that is zero.
const DRAG_FLOOR: f64 = 1.0e-4;

/// The model controls of a page, as data: what the person chose.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Choice {
    pub compartments: Compartments,
    pub input: Input,
    pub lag: bool,
    pub set: ParameterSet,
}

/// The route, the number of compartments and the lag choice (and for two compartments the
/// parameter set), then the diagram, the equation and the parameters of the model they pick.
/// Returns true when the choice changed.
pub fn picker(ui: &mut Ui, tokens: &Tokens, choice: &mut Choice, what: &str) -> bool {
    let c = &tokens.colors;
    let before = *choice;
    ui.horizontal(|ui| {
        ui.label("Route");
        egui::ComboBox::from_id_salt("model-route")
            .selected_text(choice.input.label())
            .show_ui(ui, |ui| {
                for option in Input::ALL {
                    ui.selectable_value(&mut choice.input, option, option.label());
                }
            });
        egui::ComboBox::from_id_salt("model-compartments")
            .selected_text(choice.compartments.label())
            .show_ui(ui, |ui| {
                for option in Compartments::ALL {
                    ui.selectable_value(&mut choice.compartments, option, option.label());
                }
            });
        if choice.input.has_lag_choice() {
            ui.checkbox(&mut choice.lag, "with a lag time");
        }
    });
    let two = choice.compartments == Compartments::Two;
    if two {
        ui.horizontal(|ui| {
            ui.label("Parameters as");
            egui::ComboBox::from_id_salt("model-parameter-set")
                .selected_text(choice.set.label())
                .show_ui(ui, |ui| {
                    for option in ParameterSet::ALL {
                        ui.selectable_value(&mut choice.set, option, option.label());
                    }
                });
        });
    }
    let model = modelinfo::pick(choice.compartments, choice.input, choice.lag);
    ui.label(
        RichText::new(format!(
            "Compartments: {}.  Model {}.  {what}: {}.",
            model.compartments.count(),
            model.id,
            model
                .parameters_in(choice.set)
                .iter()
                .map(|p| modelinfo::parameter_label(p, choice.input.extravascular()))
                .collect::<Vec<_>>()
                .join(", ")
        ))
        .color(c.text_muted.color()),
    );
    tokens.card_frame().show(ui, |ui| {
        modelinfo::diagram(ui, tokens, model);
        let equation = |ui: &mut Ui, line: &str| {
            ui.label(
                RichText::new(line)
                    .monospace()
                    .size(tokens.font.monospace)
                    .color(c.text.color()),
            );
        };
        for line in model.equation {
            equation(ui, line);
        }
        if two {
            for line in choice.set.relations() {
                equation(ui, line);
            }
        }
        ui.label(
            RichText::new(model.note)
                .small()
                .color(c.text_muted.color()),
        );
    });
    *choice != before
}

/// The unit shown after a parameter, from the units of the worksheet when there is one.
pub fn unit_of(name: &str, info: Option<&WorksheetInfo>) -> String {
    let Some(info) = info else {
        return match name {
            "k" | "ka" | "k10" | "k12" | "k21" | "alpha" | "beta" => "1/time".to_owned(),
            _ => String::new(),
        };
    };
    modelinfo::parameter_unit(
        name,
        info.unit_of("time"),
        info.unit_of("concentration"),
        &info.derived_units,
    )
    .unwrap_or_default()
}

/// What the editable rows of parameters need to know.
pub struct Rows<'a> {
    /// Keeps the grid of one page apart from another.
    pub id: &'a str,
    /// The engine names, in the order shown.
    pub names: Vec<&'static str>,
    /// Whether the volumes and clearances are apparent (divided by F).
    pub extravascular: bool,
    pub info: Option<&'a WorksheetInfo>,
}

impl<'a> Rows<'a> {
    /// The rows of the parameters of the model a choice picks.
    pub fn of(id: &'a str, choice: &Choice, info: Option<&'a WorksheetInfo>) -> Rows<'a> {
        let model = modelinfo::pick(choice.compartments, choice.input, choice.lag);
        Rows {
            id,
            names: model.parameters_in(choice.set),
            extravascular: choice.input.extravascular(),
            info,
        }
    }
}

/// One row per parameter: its label, a field for the value (type it, or drag it) and its unit.
/// `values` holds the numbers by engine name. A name in `pending` is a placeholder the person has
/// not set yet: its value is shown muted and leaves `pending` when it is edited. Returns true when
/// one changed.
pub fn parameter_rows(
    ui: &mut Ui,
    tokens: &Tokens,
    rows: &Rows,
    values: &mut BTreeMap<String, f64>,
    mut pending: Option<&mut BTreeSet<String>>,
) -> bool {
    let c = &tokens.colors;
    let mut changed = false;
    egui::Grid::new(("parameters", rows.id))
        .num_columns(3)
        .spacing([tokens.spacing.large, tokens.spacing.small])
        .show(ui, |ui| {
            for name in &rows.names {
                let label = modelinfo::parameter_label(name, rows.extravascular);
                ui.label(label)
                    .on_hover_text(modelinfo::parameter_meaning(name));
                let value = values.entry((*name).to_owned()).or_insert(1.0);
                let is_pending = pending.as_ref().is_some_and(|p| p.contains(*name));
                let edited = ui
                    .scope(|ui| {
                        if is_pending {
                            ui.visuals_mut().override_text_color = Some(c.text_muted.color());
                        }
                        number_field(ui, value, *name != "tlag")
                    })
                    .inner;
                if edited {
                    changed = true;
                    if let Some(p) = pending.as_mut() {
                        p.remove(*name);
                    }
                }
                ui.label(RichText::new(unit_of(name, rows.info)).color(c.text_muted.color()));
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
