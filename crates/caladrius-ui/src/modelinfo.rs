//! The models as the person picks them: by route, number of compartments and input, with the
//! equation and a diagram. The ids and the parameter names are those of the engine
//! (`specs/models.md`); the equations are the specification's, written for reading, not evaluated
//! here.

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

/// How many compartments the disposition has (the number is data, not an assumption).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Compartments {
    #[default]
    One,
    Two,
}

impl Compartments {
    pub const ALL: [Compartments; 2] = [Compartments::One, Compartments::Two];

    /// The way the choice is put to the person.
    pub fn label(self) -> &'static str {
        match self {
            Compartments::One => "One compartment",
            Compartments::Two => "Two compartments",
        }
    }

    /// The number itself.
    pub fn count(self) -> u8 {
        match self {
            Compartments::One => 1,
            Compartments::Two => 2,
        }
    }
}

/// The names of the clearance set of a two-compartment model (the default set of the engine).
const CLEARANCE_SET: [&str; 4] = ["cl", "vc", "q", "vp"];

/// The parameters of a two-compartment model are given as one of three complete sets
/// (`specs/models.md` MOD-2C-02). The engine's schemas list them in the description of the model
/// id; a test compares this table with that description.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParameterSet {
    /// Clearances and volumes: `cl`, `vc`, `q`, `vp` (the default).
    #[default]
    Clearance,
    /// Micro-constants: `k10`, `k12`, `k21`, `vc`.
    Micro,
    /// Macro-constants: `a`, `b`, `alpha`, `beta` (the dose is then required).
    Macro,
}

/// The exponents and coefficients from the micro-constants, shared by the clearance and micro sets.
const EXPONENTS: [&str; 3] = [
    "α + β = k10 + k12 + k21,   α·β = k10·k21   (α > β)",
    "A = D·(α − k21) / (Vc·(α − β))",
    "B = D·(k21 − β) / (Vc·(α − β))",
];

impl ParameterSet {
    pub const ALL: [ParameterSet; 3] = [
        ParameterSet::Clearance,
        ParameterSet::Micro,
        ParameterSet::Macro,
    ];

    /// The way the choice is put to the person.
    pub fn label(self) -> &'static str {
        match self {
            ParameterSet::Clearance => "Clearances and volumes (CL, Vc, Q, Vp)",
            ParameterSet::Micro => "Micro-constants (k10, k12, k21, Vc)",
            ParameterSet::Macro => "Macro-constants (A, B, α, β)",
        }
    }

    /// The engine names of the set.
    pub fn names(self) -> &'static [&'static str] {
        match self {
            ParameterSet::Clearance => &CLEARANCE_SET,
            ParameterSet::Micro => &["k10", "k12", "k21", "vc"],
            ParameterSet::Macro => &["a", "b", "alpha", "beta"],
        }
    }

    /// How the set gives the micro-constants, the two exponents and the two coefficients.
    pub fn relations(self) -> Vec<&'static str> {
        match self {
            ParameterSet::Clearance => {
                let mut lines = vec!["k10 = CL/Vc,   k12 = Q/Vc,   k21 = Q/Vp"];
                lines.extend(EXPONENTS);
                lines
            }
            ParameterSet::Micro => {
                let mut lines = vec!["CL = k10·Vc,   Q = k12·Vc,   Vp = Vc·k12/k21"];
                lines.extend(EXPONENTS);
                lines
            }
            ParameterSet::Macro => vec![
                "A + B = D/Vc, so Vc = D/(A + B): the dose D is required",
                "A and B are the intravenous coefficients, for every route",
            ],
        }
    }

    /// The set that a list of parameter names belongs to (the default when it is not recognised).
    pub fn of<'a>(names: impl IntoIterator<Item = &'a str>) -> ParameterSet {
        let names: Vec<&str> = names.into_iter().collect();
        if names.iter().any(|n| matches!(*n, "k10" | "k12" | "k21")) {
            ParameterSet::Micro
        } else if names
            .iter()
            .any(|n| matches!(*n, "a" | "b" | "alpha" | "beta"))
        {
            ParameterSet::Macro
        } else {
            ParameterSet::Clearance
        }
    }

    /// A value to start editing from for a two-compartment parameter, from the public worked
    /// example of the specification (a dose of 100 with cl 2, vc 10, q 4, vp 8): a place to start,
    /// not an estimate. `None` for a name that has none.
    pub fn example(name: &str) -> Option<f64> {
        Some(match name {
            "cl" => 2.0,
            "vc" => 10.0,
            "q" => 4.0,
            "vp" => 8.0,
            "k10" => 0.2,
            "k12" => 0.4,
            "k21" => 0.5,
            "a" => 5.5,
            "b" => 4.5,
            "alpha" => 1.0,
            "beta" => 0.1,
            "ka" => 2.0,
            "tlag" => 0.5,
            _ => return None,
        })
    }
}

/// A model of the catalogue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelInfo {
    pub id: &'static str,
    pub compartments: Compartments,
    pub input: Input,
    pub lag: bool,
    /// The parameters that are fitted (or simulated), by engine name; for two compartments, those
    /// of the default (clearance) set.
    pub parameters: &'static [&'static str],
    /// The equation, one line per piece.
    pub equation: &'static [&'static str],
    /// What the symbols mean when it is not obvious.
    pub note: &'static str,
}

impl ModelInfo {
    /// The parameters of the model when a two-compartment model is given in `set` (a
    /// one-compartment model has one set only).
    pub fn parameters_in(&self, set: ParameterSet) -> Vec<&'static str> {
        if self.compartments == Compartments::One {
            return self.parameters.to_vec();
        }
        set.names()
            .iter()
            .copied()
            .chain(
                self.parameters
                    .iter()
                    .copied()
                    .filter(|p| !CLEARANCE_SET.contains(p)),
            )
            .collect()
    }
}

/// The twelve models (`specs/models.md` MOD-GEN-05 and MOD-2C-20), in the order of the engine's
/// catalogue.
pub const MODELS: [ModelInfo; 12] = [
    ModelInfo {
        id: "pk1.iv_bolus",
        compartments: Compartments::One,
        input: Input::Bolus,
        lag: false,
        parameters: &["v", "k"],
        equation: &["C(t) = D/V · e^(−k·t)", "CL = V·k,   t½ = ln 2 / k"],
        note: "D is the dose, V the volume of distribution, k the elimination rate constant.",
    },
    ModelInfo {
        id: "pk1.iv_infusion",
        compartments: Compartments::One,
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
        compartments: Compartments::One,
        input: Input::FirstOrder,
        lag: false,
        parameters: &["v", "k", "ka"],
        equation: &["C(t) = D·ka / (V·(ka − k)) · (e^(−k·t) − e^(−ka·t))"],
        note: "D is the dose times F; V and CL are V/F and CL/F. ka is the absorption rate constant.",
    },
    ModelInfo {
        id: "pk1.oral_1_lag",
        compartments: Compartments::One,
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
        compartments: Compartments::One,
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
        compartments: Compartments::One,
        input: Input::ZeroOrder,
        lag: true,
        parameters: &["v", "k", "tlag"],
        equation: &[
            "t ≤ tlag:   C(t) = 0",
            "t > tlag:   the zero-order function above, with t replaced by t − tlag",
        ],
        note: "T is the absorption duration, fixed (not fitted); tlag is the lag time.",
    },
    ModelInfo {
        id: "pk2.iv_bolus",
        compartments: Compartments::Two,
        input: Input::Bolus,
        lag: false,
        parameters: &["cl", "vc", "q", "vp"],
        equation: &["C(t) = A·e^(−α·t) + B·e^(−β·t)"],
        note: "D is the dose. A and B are the coefficients and α > β the two exponents given by the parameter set; elimination is from the central compartment only.",
    },
    ModelInfo {
        id: "pk2.iv_infusion",
        compartments: Compartments::Two,
        input: Input::Infusion,
        lag: false,
        parameters: &["cl", "vc", "q", "vp"],
        equation: &[
            "0 ≤ t ≤ T:   C(t) = A/(α·T)·(1 − e^(−α·t)) + B/(β·T)·(1 − e^(−β·t))",
            "t > T:   C(t) = A/(α·T)·(1 − e^(−α·T))·e^(−α·(t−T)) + B/(β·T)·(1 − e^(−β·T))·e^(−β·(t−T))",
        ],
        note: "T is the infusion duration, fixed (not fitted). A, B, α and β come from the parameter set.",
    },
    ModelInfo {
        id: "pk2.oral_1",
        compartments: Compartments::Two,
        input: Input::FirstOrder,
        lag: false,
        parameters: &["cl", "vc", "q", "vp", "ka"],
        equation: &[
            "C(t) = Ao·e^(−α·t) + Bo·e^(−β·t) − (Ao + Bo)·e^(−ka·t)",
            "Ao = ka·A / (ka − α),   Bo = ka·B / (ka − β)",
        ],
        note: "D is the dose times F; Vc, Vp, CL and Q are the apparent values (divided by F). ka is the absorption rate constant.",
    },
    ModelInfo {
        id: "pk2.oral_1_lag",
        compartments: Compartments::Two,
        input: Input::FirstOrder,
        lag: true,
        parameters: &["cl", "vc", "q", "vp", "ka", "tlag"],
        equation: &[
            "t ≤ tlag:   C(t) = 0",
            "t > tlag:   the function above, with t replaced by t − tlag",
        ],
        note: "D is the dose times F; Vc, Vp, CL and Q are apparent. tlag is the lag time.",
    },
    ModelInfo {
        id: "pk2.oral_0",
        compartments: Compartments::Two,
        input: Input::ZeroOrder,
        lag: false,
        parameters: &["cl", "vc", "q", "vp"],
        equation: &[
            "0 ≤ t ≤ T:   C(t) = A/(α·T)·(1 − e^(−α·t)) + B/(β·T)·(1 − e^(−β·t))",
            "t > T:   C(t) = A/(α·T)·(1 − e^(−α·T))·e^(−α·(t−T)) + B/(β·T)·(1 − e^(−β·T))·e^(−β·(t−T))",
        ],
        note: "T is the absorption duration, fixed (not fitted); D is the dose times F; Vc, Vp, CL and Q are apparent.",
    },
    ModelInfo {
        id: "pk2.oral_0_lag",
        compartments: Compartments::Two,
        input: Input::ZeroOrder,
        lag: true,
        parameters: &["cl", "vc", "q", "vp", "tlag"],
        equation: &[
            "t ≤ tlag:   C(t) = 0",
            "t > tlag:   the zero-order function above, with t replaced by t − tlag",
        ],
        note: "T is the absorption duration, fixed (not fitted); tlag is the lag time.",
    },
];

/// The model for a number of compartments, a route and a lag choice (the lag is ignored where it
/// does not exist).
pub fn pick(compartments: Compartments, input: Input, lag: bool) -> &'static ModelInfo {
    let lag = lag && input.has_lag_choice();
    MODELS
        .iter()
        .find(|m| m.compartments == compartments && m.input == input && m.lag == lag)
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
        "vc" => format!("Vc{f}"),
        "vp" => format!("Vp{f}"),
        "q" => format!("Q{f}"),
        "k10" => "k10".to_owned(),
        "k12" => "k12".to_owned(),
        "k21" => "k21".to_owned(),
        "a" => "A".to_owned(),
        "b" => "B".to_owned(),
        "alpha" => "α".to_owned(),
        "beta" => "β".to_owned(),
        "half_life" => "t½".to_owned(),
        "half_life_alpha" => "t½ α".to_owned(),
        "auc_inf" => "AUC to infinity".to_owned(),
        "mrt" => "MRT".to_owned(),
        "mrt_system" => "MRT of the system".to_owned(),
        "vss" => format!("Vss{f}"),
        "vz" => format!("Vz{f}"),
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
        "cl" => "Clearance from the central compartment (apparent, CL/F, for the oral routes)",
        "vc" => "Volume of the central compartment (apparent, Vc/F, for the oral routes)",
        "q" => "Intercompartmental clearance between the central and the peripheral compartment",
        "vp" => "Volume of the peripheral compartment (apparent, Vp/F, for the oral routes)",
        "k10" => "Elimination rate constant from the central compartment, CL/Vc",
        "k12" => "Rate constant from the central to the peripheral compartment, Q/Vc",
        "k21" => "Rate constant from the peripheral to the central compartment, Q/Vp",
        "a" => "Coefficient of the faster exponential after an intravenous bolus, D·wα/Vc",
        "b" => "Coefficient of the slower exponential after an intravenous bolus, D·wβ/Vc",
        "alpha" => "The faster exponent (the distribution phase), larger than beta",
        "beta" => "The slower exponent (the terminal phase)",
        _ => "",
    }
}

/// The unit of a parameter from the units of the worksheet: volumes take the derived volume unit,
/// clearances the derived clearance unit, rate constants and exponents 1/time, times the time
/// unit, and the coefficients A and B the concentration unit.
pub fn parameter_unit(
    name: &str,
    time: Option<&str>,
    conc: Option<&str>,
    derived: &std::collections::BTreeMap<String, String>,
) -> Option<String> {
    match name {
        "v" | "vss" | "vc" | "vp" | "vz" => derived.get("v").cloned(),
        "k" | "ka" | "k10" | "k12" | "k21" | "alpha" | "beta" => time.map(|t| format!("1/{t}")),
        "tlag" | "dur" | "half_life" | "half_life_alpha" | "mrt" | "mrt_system" | "tmax_pred" => {
            time.map(str::to_owned)
        }
        "cl" | "q" => derived.get("cl").cloned(),
        "a" | "b" => conc.map(str::to_owned),
        "auc_inf" => derived.get("auc").cloned(),
        _ => None,
    }
}

/// Draws the compartment diagram of a model: the input arrow (bolus, infusion, or a depot with
/// its rate constant), the central compartment with its volume, the elimination arrow, and for two
/// compartments the peripheral compartment below the central one with the two exchange arrows.
pub fn diagram(ui: &mut Ui, tokens: &Tokens, model: &ModelInfo) {
    let c = &tokens.colors;
    let two = model.compartments == Compartments::Two;
    let height = if two {
        tokens.size.diagram_height_two
    } else {
        tokens.size.diagram_height
    };
    let (rect, _) = ui.allocate_exact_size(vec2(tokens.size.diagram_width, height), Sense::hover());
    let painter = ui.painter_at(rect);
    let line = Stroke::new(tokens.stroke.medium, c.text_muted.color());
    let edge = Stroke::new(tokens.stroke.medium, c.accent.color());
    let font = FontId::proportional(tokens.font.small);
    let box_size = vec2(
        tokens.size.diagram_box_width,
        tokens.size.diagram_box_height,
    );
    // One compartment: the row is centred. Two: the central row is on top, the peripheral below.
    let centre_y = if two {
        rect.top() + tokens.spacing.small + tokens.size.diagram_box_height / 2.0
    } else {
        rect.center().y
    };
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
    let central_box = draw_box(central, if two { "Central (Vc)" } else { "Central (V)" });
    painter.arrow(
        Pos2::new(central_box.right(), centre_y),
        Vec2::X * arrow,
        line,
    );
    caption(
        Pos2::new(
            central_box.right() + arrow / 2.0,
            centre_y - tokens.spacing.medium,
        ),
        if two { "k10 (CL)" } else { "k" },
        Align2::CENTER_BOTTOM,
    );
    if two {
        let row_gap = tokens.size.diagram_row_gap;
        let peripheral = draw_box(
            Pos2::new(central.x, central_box.bottom() + row_gap + box_size.y / 2.0),
            "Peripheral (Vp)",
        );
        // Two arrows between the compartments: down with k12, up with k21 (both carry Q).
        let side = tokens.spacing.large;
        let middle_y = central_box.bottom() + row_gap / 2.0;
        painter.arrow(
            Pos2::new(central.x - side, central_box.bottom()),
            Vec2::Y * row_gap,
            line,
        );
        painter.arrow(
            Pos2::new(central.x + side, peripheral.top()),
            -Vec2::Y * row_gap,
            line,
        );
        caption(
            Pos2::new(central.x - side - tokens.spacing.small, middle_y),
            "k12 = Q/Vc",
            Align2::RIGHT_CENTER,
        );
        caption(
            Pos2::new(central.x + side + tokens.spacing.small, middle_y),
            "k21 = Q/Vp",
            Align2::LEFT_CENTER,
        );
    }
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
                    centre_y - tokens.spacing.medium,
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
                    centre_y - tokens.spacing.medium,
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
    fn a_route_compartments_and_a_lag_choice_pick_one_model_of_the_engine() {
        let one = Compartments::One;
        let two = Compartments::Two;
        assert_eq!(pick(one, Input::Bolus, false).id, "pk1.iv_bolus");
        assert_eq!(
            pick(one, Input::Bolus, true).id,
            "pk1.iv_bolus",
            "no lag for an IV bolus"
        );
        assert_eq!(pick(one, Input::Infusion, false).id, "pk1.iv_infusion");
        assert_eq!(pick(one, Input::FirstOrder, false).id, "pk1.oral_1");
        assert_eq!(pick(one, Input::FirstOrder, true).id, "pk1.oral_1_lag");
        assert_eq!(pick(one, Input::ZeroOrder, false).id, "pk1.oral_0");
        assert_eq!(pick(one, Input::ZeroOrder, true).id, "pk1.oral_0_lag");
        assert_eq!(pick(two, Input::Bolus, false).id, "pk2.iv_bolus");
        assert_eq!(pick(two, Input::Infusion, false).id, "pk2.iv_infusion");
        assert_eq!(pick(two, Input::FirstOrder, false).id, "pk2.oral_1");
        assert_eq!(pick(two, Input::FirstOrder, true).id, "pk2.oral_1_lag");
        assert_eq!(pick(two, Input::ZeroOrder, false).id, "pk2.oral_0");
        assert_eq!(pick(two, Input::ZeroOrder, true).id, "pk2.oral_0_lag");
        for m in MODELS {
            assert_eq!(by_id(m.id), Some(&m));
            assert_eq!(pick(m.compartments, m.input, m.lag).id, m.id);
            assert!(!m.equation.is_empty() && !m.note.is_empty());
            assert!(
                m.id.starts_with(&format!("pk{}.", m.compartments.count())),
                "{}",
                m.id
            );
        }
    }

    /// The parameter sets the engine's schema describes: the names between `one set of` and the
    /// closing parenthesis, split on `or` and on commas.
    fn engine_sets(description: &str) -> Vec<Vec<String>> {
        let after = description.split("one set of ").nth(1).unwrap_or_default();
        let list = after.split(" (").next().unwrap_or_default();
        list.split(" or ")
            .map(|set| set.split(',').map(|n| n.trim().to_owned()).collect())
            .collect()
    }

    #[test]
    fn the_models_match_the_engine_catalogue_and_its_parameters() {
        // The catalogue of the engine lists the twelve ids, in the order of this table.
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
        // The parameter sets of the two-compartment ids are the ones the schema describes.
        let description = schema["$defs"]["ModelId"]["description"]
            .as_str()
            .unwrap_or_default();
        let sets = engine_sets(description);
        assert_eq!(sets.len(), ParameterSet::ALL.len(), "{description}");
        for (set, names) in ParameterSet::ALL.iter().zip(&sets) {
            assert_eq!(set.names(), names.as_slice(), "{set:?}");
        }
        // The fitted parameters are the ones the engine accepts besides `dur`.
        for m in MODELS {
            match m.compartments {
                Compartments::One => {
                    assert!(m.parameters.contains(&"v") && m.parameters.contains(&"k"));
                }
                Compartments::Two => {
                    assert_eq!(m.parameters[..4], CLEARANCE_SET);
                }
            }
            assert_eq!(
                m.input.has_duration(),
                !m.parameters.contains(&"ka") && m.input != Input::Bolus
            );
        }
    }

    #[test]
    fn a_two_compartment_model_takes_the_names_of_its_parameter_set() {
        let m = pick(Compartments::Two, Input::FirstOrder, true);
        assert_eq!(
            m.parameters_in(ParameterSet::Clearance),
            ["cl", "vc", "q", "vp", "ka", "tlag"]
        );
        assert_eq!(
            m.parameters_in(ParameterSet::Micro),
            ["k10", "k12", "k21", "vc", "ka", "tlag"]
        );
        assert_eq!(
            m.parameters_in(ParameterSet::Macro),
            ["a", "b", "alpha", "beta", "ka", "tlag"]
        );
        // One compartment has one set only.
        let one = pick(Compartments::One, Input::FirstOrder, false);
        assert_eq!(one.parameters_in(ParameterSet::Macro), ["v", "k", "ka"]);
        for set in ParameterSet::ALL {
            assert_eq!(ParameterSet::of(set.names().iter().copied()), set);
            assert!(!set.relations().is_empty());
            for name in set.names() {
                assert!(ParameterSet::example(name).is_some(), "{name}");
                assert!(!parameter_meaning(name).is_empty(), "{name}");
            }
        }
    }

    #[test]
    fn parameters_have_labels_units_and_meanings() {
        assert_eq!(parameter_label("v", true), "V/F");
        assert_eq!(parameter_label("v", false), "V");
        assert_eq!(parameter_label("cl", true), "CL/F");
        assert_eq!(parameter_label("vc", false), "Vc");
        assert_eq!(parameter_label("vp", true), "Vp/F");
        assert_eq!(parameter_label("q", false), "Q");
        assert_eq!(parameter_label("alpha", false), "α");
        assert_eq!(parameter_label("unknown", true), "unknown");
        let derived = [
            ("v".to_owned(), "L".to_owned()),
            ("cl".to_owned(), "L/h".to_owned()),
        ]
        .into();
        let unit = |name: &str| parameter_unit(name, Some("h"), Some("mg/L"), &derived);
        assert_eq!(unit("v").as_deref(), Some("L"));
        assert_eq!(unit("vc").as_deref(), Some("L"));
        assert_eq!(unit("vp").as_deref(), Some("L"));
        assert_eq!(unit("cl").as_deref(), Some("L/h"));
        assert_eq!(unit("q").as_deref(), Some("L/h"));
        assert_eq!(unit("k").as_deref(), Some("1/h"));
        assert_eq!(unit("k12").as_deref(), Some("1/h"));
        assert_eq!(unit("alpha").as_deref(), Some("1/h"));
        assert_eq!(unit("a").as_deref(), Some("mg/L"));
        assert_eq!(unit("tlag").as_deref(), Some("h"));
        assert_eq!(parameter_unit("k", None, None, &derived), None);
        assert_eq!(parameter_unit("a", Some("h"), None, &derived), None);
        assert!(!parameter_meaning("ka").is_empty());
    }
}
