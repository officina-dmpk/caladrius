//! The one-compartment models as the person picks them: by route and input, with the equation and
//! a diagram. The ids and the parameter names are those of the engine (`specs/models.md`); the
//! equations are the specification's, written for reading, not evaluated here.

use egui::{Align2, FontId, Pos2, Rect, Sense, Stroke, Ui, Vec2, vec2};
use serde::{Deserialize, Serialize};

use crate::theme::Tokens;

/// How the drug enters the central compartment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Input {
    /// A bolus into the central compartment.
    Bolus,
    /// A constant infusion over a duration.
    Infusion,
    /// First-order absorption from a depot, after an optional lag.
    #[default]
    FirstOrder,
    /// Zero-order absorption over a duration, after an optional lag.
    ZeroOrder,
}

impl Input {
    pub const ALL: [Input; 4] = [
        Input::Bolus,
        Input::Infusion,
        Input::FirstOrder,
        Input::ZeroOrder,
    ];

    /// The way the route is put to the person.
    pub fn label(self) -> &'static str {
        match self {
            Input::Bolus => "IV bolus",
            Input::Infusion => "IV infusion",
            Input::FirstOrder => "Oral, first-order absorption",
            Input::ZeroOrder => "Oral, zero-order absorption",
        }
    }

    /// Whether a lag time is offered (the extravascular routes).
    pub fn has_lag_choice(self) -> bool {
        matches!(self, Input::FirstOrder | Input::ZeroOrder)
    }

    /// Whether the model needs a duration (a fixed value, not fitted).
    pub fn has_duration(self) -> bool {
        matches!(self, Input::Infusion | Input::ZeroOrder)
    }

    /// True for the routes whose volume and clearance are apparent (divided by F).
    pub fn extravascular(self) -> bool {
        matches!(self, Input::FirstOrder | Input::ZeroOrder)
    }
}

/// A model of the catalogue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelInfo {
    pub id: &'static str,
    pub input: Input,
    pub lag: bool,
    /// The parameters that are fitted (or simulated), by engine name.
    pub parameters: &'static [&'static str],
    /// The equation, one line per piece.
    pub equation: &'static [&'static str],
    /// What the symbols mean when it is not obvious.
    pub note: &'static str,
}

/// The six models (`specs/models.md` MOD-GEN-05), in catalogue order.
pub const MODELS: [ModelInfo; 6] = [
    ModelInfo {
        id: "pk1.iv_bolus",
        input: Input::Bolus,
        lag: false,
        parameters: &["v", "k"],
        equation: &["C(t) = D/V · e^(−k·t)", "CL = V·k,   t½ = ln 2 / k"],
        note: "D is the dose, V the volume of distribution, k the elimination rate constant.",
    },
    ModelInfo {
        id: "pk1.iv_infusion",
        input: Input::Infusion,
        lag: false,
        parameters: &["v", "k"],
        equation: &[
            "0 ≤ t ≤ T:   C(t) = D / (T·V·k) · (1 − e^(−k·t))",
            "t > T:   C(t) = C(T) · e^(−k·(t − T))",
        ],
        note: "T is the infusion duration, fixed (not fitted).",
    },
    ModelInfo {
        id: "pk1.oral_1",
        input: Input::FirstOrder,
        lag: false,
        parameters: &["v", "k", "ka"],
        equation: &["C(t) = D·ka / (V·(ka − k)) · (e^(−k·t) − e^(−ka·t))"],
        note: "D is the dose times F; V and CL are V/F and CL/F. ka is the absorption rate constant.",
    },
    ModelInfo {
        id: "pk1.oral_1_lag",
        input: Input::FirstOrder,
        lag: true,
        parameters: &["v", "k", "ka", "tlag"],
        equation: &[
            "t ≤ tlag:   C(t) = 0",
            "t > tlag:   C(t) = D·ka / (V·(ka − k)) · (e^(−k·(t−tlag)) − e^(−ka·(t−tlag)))",
        ],
        note: "D is the dose times F; V and CL are V/F and CL/F. tlag is the lag time.",
    },
    ModelInfo {
        id: "pk1.oral_0",
        input: Input::ZeroOrder,
        lag: false,
        parameters: &["v", "k"],
        equation: &[
            "0 ≤ t ≤ T:   C(t) = D / (T·V·k) · (1 − e^(−k·t))",
            "t > T:   C(t) = C(T) · e^(−k·(t − T))",
        ],
        note: "T is the absorption duration, fixed (not fitted); D is the dose times F, V is V/F.",
    },
    ModelInfo {
        id: "pk1.oral_0_lag",
        input: Input::ZeroOrder,
        lag: true,
        parameters: &["v", "k", "tlag"],
        equation: &[
            "t ≤ tlag:   C(t) = 0",
            "t > tlag:   the zero-order function above, with t replaced by t − tlag",
        ],
        note: "T is the absorption duration, fixed (not fitted); tlag is the lag time.",
    },
];

/// The model for a route and a lag choice (the lag is ignored where it does not exist).
pub fn pick(input: Input, lag: bool) -> &'static ModelInfo {
    let lag = lag && input.has_lag_choice();
    MODELS
        .iter()
        .find(|m| m.input == input && m.lag == lag)
        .unwrap_or(&MODELS[0])
}

/// The model with this engine id.
pub fn by_id(id: &str) -> Option<&'static ModelInfo> {
    MODELS.iter().find(|m| m.id == id)
}

/// The label of a parameter, with the apparent form for extravascular routes.
pub fn parameter_label(name: &str, extravascular: bool) -> String {
    let f = if extravascular { "/F" } else { "" };
    match name {
        "v" => format!("V{f}"),
        "k" => "k".to_owned(),
        "ka" => "ka".to_owned(),
        "tlag" => "tlag".to_owned(),
        "dur" => "T".to_owned(),
        "cl" => format!("CL{f}"),
        "half_life" => "t½".to_owned(),
        "auc_inf" => "AUC to infinity".to_owned(),
        "mrt" => "MRT".to_owned(),
        "mrt_system" => "MRT of the system".to_owned(),
        "vss" => format!("Vss{f}"),
        "c0" => "C0".to_owned(),
        "tmax_pred" => "Tmax (predicted)".to_owned(),
        "cmax_pred" => "Cmax (predicted)".to_owned(),
        "rate" => "Input rate".to_owned(),
        other => other.to_owned(),
    }
}

/// What the parameter means, for a tooltip.
pub fn parameter_meaning(name: &str) -> &'static str {
    match name {
        "v" => "Volume of distribution (apparent, V/F, for the oral routes)",
        "k" => "Elimination rate constant",
        "ka" => "First-order absorption rate constant",
        "tlag" => "Lag time before the input starts",
        "dur" => "Duration of the zero-order input (fixed)",
        _ => "",
    }
}

/// The unit of a parameter from the units of the worksheet: volumes take the derived volume unit,
/// rate constants 1/time, times the time unit.
pub fn parameter_unit(
    name: &str,
    time: Option<&str>,
    derived: &std::collections::BTreeMap<String, String>,
) -> Option<String> {
    match name {
        "v" | "vss" => derived.get("v").cloned(),
        "k" | "ka" => time.map(|t| format!("1/{t}")),
        "tlag" | "dur" | "half_life" | "mrt" | "mrt_system" | "tmax_pred" => {
            time.map(str::to_owned)
        }
        "cl" => derived.get("cl").cloned(),
        "auc_inf" => derived.get("auc").cloned(),
        _ => None,
    }
}

/// Draws the compartment diagram of a model: the input arrow (bolus, infusion, or a depot with
/// its rate constant), the central compartment with its volume, and the elimination arrow.
pub fn diagram(ui: &mut Ui, tokens: &Tokens, model: &ModelInfo) {
    let c = &tokens.colors;
    let (rect, _) = ui.allocate_exact_size(
        vec2(tokens.size.diagram_width, tokens.size.diagram_height),
        Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    let line = Stroke::new(tokens.stroke.medium, c.text_muted.color());
    let edge = Stroke::new(tokens.stroke.medium, c.accent.color());
    let font = FontId::proportional(tokens.font.small);
    let box_size = vec2(
        tokens.size.diagram_box_width,
        tokens.size.diagram_box_height,
    );
    let centre_y = rect.center().y;
    let arrow = tokens.size.diagram_arrow;
    let draw_box = |center: Pos2, label: &str| {
        let r = Rect::from_center_size(center, box_size);
        painter.rect(
            r,
            tokens.radius.medium,
            c.panel_alt.color(),
            edge,
            egui::StrokeKind::Inside,
        );
        painter.text(
            r.center(),
            Align2::CENTER_CENTER,
            label,
            FontId::proportional(tokens.font.body),
            c.text.color(),
        );
        r
    };
    let caption = |at: Pos2, text: &str, align: Align2| {
        painter.text(at, align, text, font.clone(), c.text_muted.color());
    };
    // Layout from the right: the elimination arrow leaves the central compartment on the right.
    let central = Pos2::new(
        rect.right() - arrow - tokens.size.diagram_box_width / 2.0,
        centre_y,
    );
    let central_box = draw_box(central, "Central (V)");
    painter.arrow(
        Pos2::new(central_box.right(), centre_y),
        Vec2::X * arrow,
        line,
    );
    caption(
        Pos2::new(
            central_box.right() + arrow / 2.0,
            centre_y - tokens.spacing.small,
        ),
        "k",
        Align2::CENTER_BOTTOM,
    );
    match model.input {
        Input::Bolus | Input::Infusion => {
            painter.arrow(
                Pos2::new(central_box.left() - arrow, centre_y),
                Vec2::X * arrow,
                line,
            );
            let text = if model.input == Input::Bolus {
                "D (bolus)"
            } else {
                "dose D over T"
            };
            caption(
                Pos2::new(
                    central_box.left() - arrow / 2.0,
                    centre_y - tokens.spacing.small,
                ),
                text,
                Align2::CENTER_BOTTOM,
            );
        }
        Input::FirstOrder | Input::ZeroOrder => {
            let gap = tokens.size.diagram_gap;
            let depot = Pos2::new(
                central_box.left() - gap - tokens.size.diagram_box_width / 2.0,
                centre_y,
            );
            let depot_label = if model.input == Input::FirstOrder {
                "Depot (dose × F)"
            } else {
                "Dose × F"
            };
            let depot_box = draw_box(depot, depot_label);
            painter.arrow(Pos2::new(depot_box.right(), centre_y), Vec2::X * gap, line);
            let rate = if model.input == Input::FirstOrder {
                "ka"
            } else {
                "rate D/T for T"
            };
            caption(
                Pos2::new(
                    depot_box.right() + gap / 2.0,
                    centre_y - tokens.spacing.small,
                ),
                rate,
                Align2::CENTER_BOTTOM,
            );
            if model.lag {
                caption(
                    Pos2::new(
                        depot_box.center().x,
                        depot_box.bottom() + tokens.spacing.small,
                    ),
                    "starts after tlag",
                    Align2::CENTER_TOP,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_route_and_a_lag_choice_pick_one_model_of_the_engine() {
        assert_eq!(pick(Input::Bolus, false).id, "pk1.iv_bolus");
        assert_eq!(
            pick(Input::Bolus, true).id,
            "pk1.iv_bolus",
            "no lag for an IV bolus"
        );
        assert_eq!(pick(Input::Infusion, false).id, "pk1.iv_infusion");
        assert_eq!(pick(Input::FirstOrder, false).id, "pk1.oral_1");
        assert_eq!(pick(Input::FirstOrder, true).id, "pk1.oral_1_lag");
        assert_eq!(pick(Input::ZeroOrder, false).id, "pk1.oral_0");
        assert_eq!(pick(Input::ZeroOrder, true).id, "pk1.oral_0_lag");
        for m in MODELS {
            assert_eq!(by_id(m.id), Some(&m));
            assert_eq!(pick(m.input, m.lag).id, m.id);
            assert!(!m.equation.is_empty() && !m.note.is_empty());
        }
    }

    #[test]
    fn the_models_match_the_engine_catalogue_and_its_parameters() {
        // The catalogue of the engine lists the same six ids; the fitted parameters are the ones
        // the engine accepts besides `dur`.
        let described = caladrius_engine::describe();
        let schema = described
            .iter()
            .find(|c| c.id == "fit.run")
            .map(|c| c.params_schema.clone())
            .unwrap();
        let ids: Vec<&str> = schema["$defs"]["ModelId"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        let ours: Vec<&str> = MODELS.iter().map(|m| m.id).collect();
        assert_eq!(ids, ours);
        for m in MODELS {
            assert!(m.parameters.contains(&"v") && m.parameters.contains(&"k"));
            assert_eq!(
                m.input.has_duration(),
                !m.parameters.contains(&"ka") && m.input != Input::Bolus
            );
        }
    }

    #[test]
    fn parameters_have_labels_units_and_meanings() {
        assert_eq!(parameter_label("v", true), "V/F");
        assert_eq!(parameter_label("v", false), "V");
        assert_eq!(parameter_label("cl", true), "CL/F");
        assert_eq!(parameter_label("unknown", true), "unknown");
        let derived = [("v".to_owned(), "L".to_owned())].into();
        assert_eq!(
            parameter_unit("v", Some("h"), &derived).as_deref(),
            Some("L")
        );
        assert_eq!(
            parameter_unit("k", Some("h"), &derived).as_deref(),
            Some("1/h")
        );
        assert_eq!(
            parameter_unit("tlag", Some("h"), &derived).as_deref(),
            Some("h")
        );
        assert_eq!(parameter_unit("k", None, &derived), None);
        assert!(!parameter_meaning("ka").is_empty());
    }
}
