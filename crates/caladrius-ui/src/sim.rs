//! The simulation page: a model, a dose, the parameters and a time range give the curve of the
//! engine's `model.simulate` (no worksheet needed). It reuses the model and parameter controls of
//! the fit page. The curve is redrawn at every change; "Save as analysis" stores it in the project.

use std::collections::BTreeMap;

use egui::{RichText, Ui};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::app::Action;
use crate::fit::defaults::PREVIEW_POINTS;
use crate::fitplots;
use crate::fmt;
use crate::model::{Simulation, read};
use crate::modelinfo::{self, Input, ModelInfo};
use crate::modelpick;
use crate::plot::{self, Dot, LineSet, PointSet, Tone, Weight};
use crate::plotdata::Pt;
use crate::theme::Tokens;
use crate::widgets::{error_box, section};

/// The values a new simulation starts from (the engine's own example).
pub mod example {
    pub const DOSE: f64 = 100.0;
    pub const V: f64 = 20.0;
    pub const K: f64 = 0.1;
    pub const KA: f64 = 1.2;
    pub const TLAG: f64 = 0.5;
    pub const DURATION: f64 = 1.0;
    pub const END: f64 = 24.0;
}

/// The derived values shown under the curve, in order.
const SECONDARY: [&str; 9] = [
    "half_life",
    "cl",
    "auc_inf",
    "mrt",
    "mrt_system",
    "vss",
    "c0",
    "tmax_pred",
    "cmax_pred",
];

/// The state of the simulation page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimPage {
    /// The stored analysis this page was opened from or saved as.
    pub analysis: Option<u64>,
    pub input: Input,
    pub lag: bool,
    pub duration: f64,
    pub dose: f64,
    /// Parameter values by engine name (`v`, `k`, `ka`, `tlag`).
    pub params: BTreeMap<String, f64>,
    /// The curve runs from 0 to this time.
    pub end: f64,
    #[serde(skip)]
    pub result: Option<Simulation>,
    #[serde(skip)]
    pub error: Option<String>,
}

impl Default for SimPage {
    fn default() -> Self {
        SimPage::new()
    }
}

impl SimPage {
    pub fn new() -> SimPage {
        SimPage {
            analysis: None,
            input: Input::default(),
            lag: false,
            duration: example::DURATION,
            dose: example::DOSE,
            params: BTreeMap::from([
                ("v".to_owned(), example::V),
                ("k".to_owned(), example::K),
                ("ka".to_owned(), example::KA),
                ("tlag".to_owned(), example::TLAG),
            ]),
            end: example::END,
            result: None,
            error: None,
        }
    }

    /// The page of a stored simulation.
    pub fn from_view(view: &Value) -> Option<SimPage> {
        if view.get("kind").and_then(Value::as_str) != Some("simulation") {
            return None;
        }
        let input = view.pointer("/spec/input")?;
        let model = input
            .get("model")
            .and_then(Value::as_str)
            .and_then(modelinfo::by_id)?;
        let mut page = SimPage::new();
        page.analysis = view.get("id").and_then(Value::as_u64);
        page.input = model.input;
        page.lag = model.lag;
        page.dose = input.get("dose").and_then(Value::as_f64)?;
        if let Some(map) = input.get("params").and_then(Value::as_object) {
            for (k, x) in map {
                if let Some(x) = x.as_f64() {
                    if k == "dur" {
                        page.duration = x;
                    } else {
                        page.params.insert(k.clone(), x);
                    }
                }
            }
        }
        page.end = input
            .get("times")
            .and_then(Value::as_array)
            .map(|t| t.iter().filter_map(Value::as_f64).fold(0.0_f64, f64::max))
            .filter(|t| *t > 0.0)
            .unwrap_or(example::END);
        Some(page)
    }

    pub fn model(&self) -> &'static ModelInfo {
        modelinfo::pick(self.input, self.lag)
    }

    /// `model.simulate` for what the page holds.
    pub fn params(&self, store: bool) -> Value {
        let model = self.model();
        let mut params: BTreeMap<String, f64> = model
            .parameters
            .iter()
            .filter_map(|n| self.params.get(*n).map(|x| ((*n).to_owned(), *x)))
            .collect();
        if self.input.has_duration() {
            params.insert("dur".to_owned(), self.duration);
        }
        let mut out = json!({
            "model": model.id,
            "dose": self.dose,
            "params": params,
            "grid": { "start": 0.0, "end": self.end, "points": PREVIEW_POINTS },
        });
        if store {
            out["store"] = json!(true);
        }
        out
    }

    /// Takes the answer of `model.simulate`.
    pub fn adopt(&mut self, answer: Result<Value, String>) {
        match answer {
            Ok(v) => {
                let sim: Simulation = read(&v);
                if let Some(id) = sim.analysis {
                    self.analysis = Some(id);
                }
                self.result = Some(sim);
                self.error = None;
            }
            Err(message) => {
                self.result = None;
                self.error = Some(message);
            }
        }
    }
}

/// The page body.
pub fn central(ui: &mut Ui, tokens: &Tokens, page: &mut SimPage, actions: &mut Vec<Action>) {
    let c = &tokens.colors;
    ui.label(
        RichText::new("Model simulation")
            .size(tokens.font.title)
            .strong(),
    );
    ui.label(
        RichText::new(
            "No data needed: choose a model, give a dose and parameters, and see the curve.",
        )
        .color(c.text_muted.color()),
    );
    let mut changed = false;

    section(ui, tokens, "Model", |ui| {
        changed |= modelpick::picker(ui, tokens, &mut page.input, &mut page.lag, "Parameters");
        if page.input.has_duration() {
            ui.horizontal(|ui| {
                ui.label("Duration of the input");
                changed |= modelpick::number_field(ui, &mut page.duration, true);
                ui.label(RichText::new("time units").color(c.text_muted.color()));
            });
        }
    });

    section(ui, tokens, "Dose and parameters", |ui| {
        ui.horizontal(|ui| {
            ui.label("Dose");
            changed |= modelpick::number_field(ui, &mut page.dose, true);
            ui.label(RichText::new("dose units").color(c.text_muted.color()));
        });
        let model = page.model();
        changed |=
            modelpick::parameter_rows(ui, tokens, "simulation", model, &mut page.params, None);
    });

    section(ui, tokens, "Time", |ui| {
        ui.horizontal(|ui| {
            ui.label("From 0 to");
            changed |= modelpick::number_field(ui, &mut page.end, true);
            ui.label(RichText::new("time units").color(c.text_muted.color()));
        });
    });

    section(ui, tokens, "Results", |ui| {
        if let Some(error) = &page.error {
            error_box(ui, tokens, error);
            return;
        }
        let Some(result) = &page.result else {
            ui.label(RichText::new("The curve appears here.").color(c.text_muted.color()));
            return;
        };
        let extravascular = page.input.extravascular();
        egui::Grid::new("simulation-secondary")
            .num_columns(2)
            .spacing([tokens.spacing.large, tokens.spacing.small])
            .striped(true)
            .show(ui, |ui| {
                for name in SECONDARY {
                    if let Some(x) = result.secondary.get(name) {
                        ui.label(modelinfo::parameter_label(name, extravascular));
                        ui.label(RichText::new(fmt::number(*x)).strong());
                        ui.end_row();
                    }
                }
            });
        ui.add_space(tokens.spacing.medium);
        ui.horizontal(|ui| {
            let text = if page.analysis.is_some() {
                "Save as another analysis"
            } else {
                "Save as analysis"
            };
            if ui.add(tokens.primary_button(text)).clicked() {
                actions.push(Action::SimSave);
            }
            if let Some(id) = page.analysis {
                ui.label(
                    RichText::new(format!("saved as analysis {id}")).color(c.text_muted.color()),
                );
            }
        });
    });

    if changed {
        actions.push(Action::SimChanged);
    }
}

/// The plot beside the page: the curve from the engine.
pub fn plot_panel(ui: &mut Ui, tokens: &Tokens, page: &SimPage, log_axis: &mut bool) {
    let c = &tokens.colors;
    fitplots::scale_switch(ui, tokens, "Concentration over time", log_axis);
    let Some(result) = &page.result else {
        ui.label(RichText::new("The curve appears here.").color(c.text_muted.color()));
        return;
    };
    let curve = result.curve();
    let data: Vec<Pt> = curve
        .iter()
        .map(|p| Pt {
            time: p[0],
            conc: p[1],
            replaced: false,
        })
        .collect();
    if *log_axis && !plot::log_note(ui, tokens, "simulation", &data, log_axis) {
        return;
    }
    let height =
        (ui.available_height() - tokens.size.plot_reserved_height).max(tokens.size.plot_min_height);
    let _ = plot::profile(
        ui,
        tokens,
        "simulation-plot",
        ("Time".to_owned(), "Concentration".to_owned()),
        &[PointSet::new("Peak", peak(&curve), Tone::Fit, Dot::Large)],
        &[LineSet::new(
            "Simulated",
            curve,
            Tone::Selected,
            Weight::Thick,
        )],
        *log_axis,
        &[],
        height,
    );
}

/// The highest point of the curve the engine gave, to mark it (a pick, not a calculation).
fn peak(curve: &[[f64; 2]]) -> Vec<[f64; 2]> {
    curve
        .iter()
        .copied()
        .filter(|p| p[1].is_finite())
        .fold(None, |best: Option<[f64; 2]>, p| match best {
            Some(b) if b[1] >= p[1] => Some(b),
            _ => Some(p),
        })
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_builds_the_parameters_of_model_simulate() {
        let mut p = SimPage::new();
        let q = p.params(false);
        assert_eq!(q["model"], "pk1.oral_1");
        assert_eq!(q["params"], json!({ "v": 20.0, "k": 0.1, "ka": 1.2 }));
        assert_eq!(q["grid"]["end"], 24.0);
        assert!(q.get("store").is_none());
        p.input = Input::ZeroOrder;
        p.lag = true;
        p.duration = 3.0;
        let q = p.params(true);
        assert_eq!(q["model"], "pk1.oral_0_lag");
        assert_eq!(
            q["params"],
            json!({ "v": 20.0, "k": 0.1, "tlag": 0.5, "dur": 3.0 })
        );
        assert_eq!(q["store"], true);
    }

    #[test]
    fn a_stored_simulation_gives_back_its_page_and_the_state_round_trips() {
        let view = json!({ "id": 4, "kind": "simulation", "spec": { "kind": "simulation",
            "input": { "model": "pk1.iv_infusion", "dose": 50.0,
                       "params": { "v": 10.0, "k": 0.2, "dur": 2.0 },
                       "times": [0.0, 6.0, 12.0] } } });
        let p = SimPage::from_view(&view).unwrap();
        assert_eq!(
            (p.analysis, p.input, p.dose, p.duration, p.end),
            (Some(4), Input::Infusion, 50.0, 2.0, 12.0)
        );
        assert_eq!(p.params.get("v"), Some(&10.0));
        let text = serde_json::to_string(&p).unwrap();
        assert_eq!(serde_json::from_str::<SimPage>(&text).unwrap(), p);
        assert!(SimPage::from_view(&json!({ "kind": "fit" })).is_none());
    }

    #[test]
    fn the_peak_is_picked_from_the_engine_curve() {
        assert_eq!(
            peak(&[[0.0, 0.0], [1.0, 3.0], [2.0, 2.0]]),
            vec![[1.0, 3.0]]
        );
        assert!(peak(&[]).is_empty());
    }
}
