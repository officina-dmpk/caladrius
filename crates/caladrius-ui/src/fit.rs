//! The state of the model-fit page, as data: what the person chose (worksheet, subject, route, lag,
//! dose, weighting, starting values, options) and the parameters of the engine commands built from
//! it (`fit.initial_estimates`, `fit.evaluate`, `model.simulate`, `fit.run`). The last answers of the
//! engine are caches and are not saved. Nothing here computes a curve or a statistic.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::model::{FitOk, FitView, Status, Table, WorksheetInfo};
use crate::modelinfo::{self, Compartments, Input, ModelInfo, ParameterSet};
use crate::sheet;

/// The weighting schemes of the engine, by id, as the person reads them.
pub const WEIGHTINGS: [(&str, &str); 5] = [
    ("uniform", "Uniform (every point counts the same)"),
    ("inv_y", "1/y (observed)"),
    ("inv_y2", "1/y² (observed)"),
    ("inv_yhat", "1/ŷ (predicted)"),
    ("inv_yhat2", "1/ŷ² (predicted)"),
];

/// How the partial derivatives are formed.
pub const DERIVATIVES: [(&str, &str); 3] = [
    (
        "auto",
        "Closed forms when the model has them, else forward differences",
    ),
    ("forward_difference", "Forward differences"),
    ("analytic", "Closed forms (where the model has them)"),
];

/// What ends the iterations.
pub const CRITERIA: [(&str, &str); 2] = [
    (
        "relative_decrease",
        "Relative decrease of the sum of squares",
    ),
    ("relative_offset", "Relative offset of the residual"),
];

/// The defaults the page shows before the engine has returned the options of a run. They mirror
/// the engine's (its preset `default`, task T-040); a test compares them with what a run returns,
/// so a change in the engine cannot leave the page showing other numbers.
pub mod defaults {
    pub const MAX_ITERATIONS: u64 = 50;
    pub const CONVERGENCE: f64 = 1.0e-10;
    pub const INCREMENT: f64 = 1.0e-5;
    pub const DERIVATIVES: &str = "auto";
    pub const CONFIDENCE_LEVEL: f64 = 0.95;
    pub const MAX_CV_PERCENT: f64 = 50.0;
    pub const MAX_ABS_CORRELATION: f64 = 0.95;
    pub const MAX_CONDITION_NUMBER: f64 = 1.0e6;
    pub const MIN_DEGREES_OF_FREEDOM: f64 = 2.0;
    /// The duration offered for a model that needs one, until the person sets it.
    pub const DURATION: f64 = 1.0;
    /// Points of the live curve.
    pub const PREVIEW_POINTS: usize = 240;
}

/// What the engine said about the starting values: the live curve and the objective.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Preview {
    /// The curve from `model.simulate` at the starting values.
    pub curve: Vec<[f64; 2]>,
    /// The weighted sum of squares at the starting values, from `fit.evaluate`.
    pub wrss: Option<f64>,
    /// The dose in use, from the worksheet or the person.
    pub dose: Option<f64>,
    /// Secondary parameters of the starting values (half-life, predicted peak...).
    pub secondary: BTreeMap<String, f64>,
    /// Why there is no curve or no objective, in a sentence that says what to fix.
    pub error: Option<String>,
}

/// The state of the fit page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FitPage {
    pub analysis: Option<u64>,
    pub worksheet: u64,
    pub subject: String,
    #[serde(default)]
    pub compartments: Compartments,
    pub input: Input,
    pub lag: bool,
    /// For two compartments: which complete set of parameters the starting values are in.
    #[serde(default)]
    pub set: ParameterSet,
    /// The fixed duration of an infusion or a zero-order input.
    pub duration: f64,
    pub dose_override: Option<f64>,
    /// The weighting id.
    pub weighting: String,
    /// The starting values by parameter name, as the person sees and edits them.
    pub initial: BTreeMap<String, f64>,
    /// Starting values that are placeholders the person has not set yet (two compartments, where
    /// the engine cannot generate them).
    #[serde(default)]
    pub pending: BTreeSet<String>,
    /// The options the person set, as the engine takes them (empty: the engine's defaults).
    pub options: Value,
    #[serde(skip)]
    pub view: Option<FitView>,
    #[serde(skip)]
    pub error: Option<String>,
    #[serde(skip)]
    pub start_error: Option<String>,
    #[serde(skip)]
    pub preview: Preview,
}

impl FitPage {
    pub fn new(worksheet: u64, subject: String) -> FitPage {
        FitPage {
            analysis: None,
            worksheet,
            subject,
            compartments: Compartments::default(),
            input: Input::default(),
            lag: false,
            set: ParameterSet::default(),
            duration: defaults::DURATION,
            dose_override: None,
            weighting: "uniform".to_owned(),
            initial: BTreeMap::new(),
            pending: BTreeSet::new(),
            options: Value::Null,
            view: None,
            error: None,
            start_error: None,
            preview: Preview::default(),
        }
    }

    /// The page of a stored fit, read from its options.
    pub fn from_view(view: &Value) -> Option<FitPage> {
        let v = FitView::from_value(view)?;
        let spec = &v.spec;
        let model = spec
            .get("model")
            .and_then(Value::as_str)
            .and_then(modelinfo::by_id)?;
        let mut options = spec.get("options").cloned().unwrap_or(Value::Null);
        let duration = options
            .pointer("/fixed/dur")
            .and_then(Value::as_f64)
            .unwrap_or(defaults::DURATION);
        if let Some(fixed) = options.get_mut("fixed").and_then(Value::as_object_mut) {
            fixed.remove("dur");
        }
        let initial: BTreeMap<String, f64> = spec
            .get("initial")
            .and_then(Value::as_object)
            .map(|m| {
                m.iter()
                    .filter_map(|(k, x)| x.as_f64().map(|x| (k.clone(), x)))
                    .collect()
            })
            .unwrap_or_default();
        Some(FitPage {
            analysis: Some(v.id),
            worksheet: spec.get("worksheet").and_then(Value::as_u64).unwrap_or(0),
            subject: spec
                .get("subject")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            compartments: model.compartments,
            input: model.input,
            lag: model.lag,
            set: ParameterSet::of(initial.keys().map(String::as_str)),
            duration,
            dose_override: spec.get("dose").and_then(Value::as_f64),
            weighting: spec
                .get("weighting")
                .and_then(Value::as_str)
                .unwrap_or("uniform")
                .to_owned(),
            initial,
            pending: BTreeSet::new(),
            options,
            view: Some(v),
            error: None,
            start_error: None,
            preview: Preview::default(),
        })
    }

    pub fn model(&self) -> &'static ModelInfo {
        modelinfo::pick(self.compartments, self.input, self.lag)
    }

    /// What the model controls hold.
    pub fn choice(&self) -> crate::modelpick::Choice {
        crate::modelpick::Choice {
            compartments: self.compartments,
            input: self.input,
            lag: self.lag,
            set: self.set,
        }
    }

    pub fn set_choice(&mut self, choice: crate::modelpick::Choice) {
        self.compartments = choice.compartments;
        self.input = choice.input;
        self.lag = choice.lag;
        self.set = choice.set;
    }

    /// The parameters the model fits, in the parameter set the page holds.
    pub fn parameters(&self) -> Vec<&'static str> {
        self.model().parameters_in(self.set)
    }

    /// Whether the engine can generate the starting values from the data (one compartment only,
    /// `specs/fit.md` OF-08); for two compartments the person gives every one.
    pub fn automatic_estimates(&self) -> bool {
        self.compartments == Compartments::One
    }

    /// Two compartments: makes sure there is a value to edit for every parameter of the model in
    /// the chosen set. A value already there is kept (it may be one the person set); a missing
    /// one is a placeholder from the public worked example, shown as not set yet. Values of
    /// parameters the set does not have are dropped.
    pub fn seed_placeholders(&mut self) {
        // Values left by a one-compartment model (V, k) mean nothing here.
        if self.initial.contains_key("v") || self.initial.contains_key("k") {
            self.initial.clear();
        }
        let names = self.parameters();
        self.initial.retain(|k, _| names.contains(&k.as_str()));
        self.pending.retain(|k| names.contains(&k.as_str()));
        for name in names {
            if !self.initial.contains_key(name) {
                self.initial
                    .insert(name.to_owned(), ParameterSet::example(name).unwrap_or(1.0));
                self.pending.insert(name.to_owned());
            }
        }
    }

    /// The starting values are all set by the person (always so when the engine generated them).
    pub fn starting_values_set(&self) -> bool {
        !self.initial.is_empty() && self.pending.is_empty()
    }

    /// The person accepts the values shown.
    pub fn accept_placeholders(&mut self) {
        self.pending.clear();
    }

    /// The parameters held at a value (the duration of the input).
    pub fn fixed(&self) -> BTreeMap<String, f64> {
        let mut fixed = BTreeMap::new();
        if self.input.has_duration() {
            fixed.insert("dur".to_owned(), self.duration);
        }
        fixed
    }

    /// The dose in use: the person's, else the worksheet's.
    pub fn dose(&self) -> Option<f64> {
        self.dose_override.or(self.preview.dose)
    }

    pub fn status(&self) -> Status {
        self.view
            .as_ref()
            .map(|v| v.status.clone())
            .unwrap_or_default()
    }

    pub fn ok(&self) -> Option<&FitOk> {
        self.view.as_ref().and_then(FitView::ok)
    }

    // ---- options ---------------------------------------------------------------------------

    pub fn option_f64(&self, path: &str, default: f64) -> f64 {
        self.options
            .pointer(path)
            .and_then(Value::as_f64)
            .unwrap_or(default)
    }

    pub fn option_str(&self, path: &str, default: &str) -> String {
        self.options
            .pointer(path)
            .and_then(Value::as_str)
            .unwrap_or(default)
            .to_owned()
    }

    /// A flag threshold that is on (`Some`), off (`null`) or not set (the engine's default).
    pub fn threshold(&self, key: &str, default: f64) -> Option<f64> {
        match self.options.pointer(&format!("/flags/{key}")) {
            Some(Value::Null) => None,
            Some(v) => v.as_f64().or(Some(default)),
            None => Some(default),
        }
    }

    /// Sets a value in the options, creating the objects on the way.
    pub fn set_option(&mut self, path: &[&str], value: Value) {
        if !self.options.is_object() {
            self.options = json!({});
        }
        let mut node = &mut self.options;
        for (i, key) in path.iter().enumerate() {
            if !node.is_object() {
                *node = json!({});
            }
            let Some(map) = node.as_object_mut() else {
                return;
            };
            if i + 1 == path.len() {
                map.insert((*key).to_owned(), value);
                return;
            }
            node = map.entry((*key).to_owned()).or_insert_with(|| json!({}));
        }
    }

    /// The options for a command: what the person set, plus the fixed duration.
    fn command_options(&self) -> Value {
        let mut options = if self.options.is_object() {
            self.options.clone()
        } else {
            json!({})
        };
        let fixed = self.fixed();
        if !fixed.is_empty() {
            let mut merged = options
                .get("fixed")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            for (k, x) in fixed {
                merged.insert(k, json!(x));
            }
            options["fixed"] = Value::Object(merged);
        }
        options
    }

    // ---- the commands ----------------------------------------------------------------------

    /// The starting values only for the parameters the model fits.
    fn fitted_initial(&self) -> BTreeMap<&String, f64> {
        let names = self.parameters();
        self.initial
            .iter()
            .filter(|(k, _)| names.contains(&k.as_str()))
            .map(|(k, v)| (k, *v))
            .collect()
    }

    /// `fit.initial_estimates`.
    pub fn initial_params(&self) -> Value {
        json!({
            "worksheet": self.worksheet,
            "subject": self.subject,
            "model": self.model().id,
            "dose": self.dose_override,
            "fixed": self.fixed(),
        })
    }

    /// `fit.evaluate`.
    pub fn evaluate_params(&self) -> Value {
        json!({
            "worksheet": self.worksheet,
            "subject": self.subject,
            "model": self.model().id,
            "dose": self.dose_override,
            "weighting": self.weighting,
            "initial": self.fitted_initial(),
            "options": self.command_options(),
        })
    }

    /// `fit.run`.
    pub fn run_params(&self) -> Value {
        let mut params = self.evaluate_params();
        if let Some(id) = self.analysis {
            params["analysis"] = json!(id);
        }
        params
    }

    /// `model.simulate` over `0..=end` at the starting values; `None` when there is no dose yet
    /// or the starting values are incomplete.
    pub fn simulate_params(&self, end: f64) -> Option<Value> {
        let dose = self.dose()?;
        let mut params: BTreeMap<String, f64> = BTreeMap::new();
        for name in self.parameters() {
            params.insert(name.to_owned(), *self.initial.get(name)?);
        }
        params.extend(self.fixed());
        Some(json!({
            "model": self.model().id,
            "dose": dose,
            "params": params,
            "grid": { "start": 0.0, "end": end, "points": defaults::PREVIEW_POINTS },
        }))
    }

    // ---- taking answers --------------------------------------------------------------------

    /// Takes the generated starting values from `fit.initial_estimates`.
    pub fn adopt_initial(&mut self, answer: &Value) {
        self.initial.clear();
        self.pending.clear();
        if let Some(map) = answer.get("initial").and_then(Value::as_object) {
            for name in self.parameters() {
                if let Some(x) = map.get(name).and_then(Value::as_f64) {
                    self.initial.insert(name.to_owned(), x);
                }
            }
        }
        self.preview.dose = answer.get("dose").and_then(Value::as_f64);
    }

    /// Takes the answers of `model.simulate` and `fit.evaluate` (each an error sentence or a value).
    pub fn adopt_preview(
        &mut self,
        simulated: Option<Result<Value, String>>,
        evaluated: Result<Value, String>,
    ) {
        let mut error = None;
        match simulated {
            Some(Ok(v)) => {
                let sim: crate::model::Simulation = crate::model::read(&v);
                self.preview.curve = sim.curve();
                self.preview.secondary = sim.secondary;
            }
            Some(Err(message)) => {
                self.preview.curve.clear();
                self.preview.secondary.clear();
                error = Some(message);
            }
            None => {
                self.preview.curve.clear();
                self.preview.secondary.clear();
            }
        }
        match evaluated {
            Ok(v) => {
                self.preview.wrss = v.get("wrss").and_then(Value::as_f64);
                if let Some(d) = v.get("dose").and_then(Value::as_f64) {
                    self.preview.dose = Some(d);
                }
            }
            Err(message) => {
                // The objective's refusal is the sentence `fit.run` gives for the same values (both
                // come from the fit), so it wins over the one of the curve (the model's wording).
                self.preview.wrss = None;
                error = Some(message);
            }
        }
        self.preview.error = error;
    }

    /// Takes the engine's answer to a run.
    pub fn adopt(&mut self, view: &Value) {
        if let Some(v) = FitView::from_value(view) {
            self.analysis = Some(v.id);
            self.view = Some(v);
            self.error = None;
        }
    }
}

/// The observed points of one subject: (time, concentration) of the rows with both numbers.
pub fn observed(info: &WorksheetInfo, table: &Table, subject: &str) -> Vec<[f64; 2]> {
    let index = |role: &str| info.column_of(role).and_then(|c| table.column_index(c));
    let (Some(t), Some(c)) = (index("time"), index("concentration")) else {
        return Vec::new();
    };
    sheet::visible_rows(table, info, Some(subject))
        .into_iter()
        .filter_map(|r| table.rows.get(r))
        .filter_map(|row| {
            let time = row.get(t)?.as_f64()?;
            let conc = row.get(c)?.as_f64()?;
            (time.is_finite() && conc.is_finite()).then_some([time, conc])
        })
        .collect()
}

/// The last observation time: the live curve is drawn up to it.
pub fn last_time(points: &[[f64; 2]]) -> Option<f64> {
    points
        .iter()
        .map(|p| p[0])
        .filter(|t| t.is_finite())
        .fold(None, |best: Option<f64>, t| {
            Some(best.map_or(t, |b| b.max(t)))
        })
        .filter(|t| *t > 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page() -> FitPage {
        let mut p = FitPage::new(2, "A".to_owned());
        p.initial = BTreeMap::from([
            ("v".to_owned(), 20.0),
            ("k".to_owned(), 0.1),
            ("ka".to_owned(), 1.2),
            ("tlag".to_owned(), 0.4),
        ]);
        p.preview.dose = Some(100.0);
        p
    }

    #[test]
    fn the_page_builds_the_parameters_of_the_commands() {
        let mut p = page();
        let run = p.run_params();
        assert_eq!(run["model"], "pk1.oral_1");
        assert_eq!(run["weighting"], "uniform");
        // Only the fitted parameters of the model: no lag for the model without one.
        assert_eq!(run["initial"], json!({ "v": 20.0, "k": 0.1, "ka": 1.2 }));
        assert!(run.get("analysis").is_none());
        p.analysis = Some(9);
        p.lag = true;
        assert_eq!(p.run_params()["analysis"], 9);
        assert_eq!(p.run_params()["initial"]["tlag"], 0.4);
        // A duration is fixed, not fitted, and goes with the options and the generation.
        p.input = Input::Infusion;
        p.duration = 2.0;
        assert_eq!(p.run_params()["options"]["fixed"]["dur"], 2.0);
        assert_eq!(p.initial_params()["fixed"]["dur"], 2.0);
        assert_eq!(p.run_params()["initial"], json!({ "v": 20.0, "k": 0.1 }));
        let sim = p.simulate_params(24.0).unwrap();
        assert_eq!(sim["params"]["dur"], 2.0);
        assert_eq!(sim["grid"]["end"], 24.0);
        assert!(sim["params"].get("ka").is_none());
    }

    #[test]
    fn two_compartments_have_placeholders_a_set_and_their_own_parameter_names() {
        let mut p = FitPage::new(2, "A".to_owned());
        p.compartments = Compartments::Two;
        p.input = Input::FirstOrder;
        assert!(!p.automatic_estimates());
        p.seed_placeholders();
        assert_eq!(p.model().id, "pk2.oral_1");
        assert_eq!(p.pending.len(), 5);
        assert!(!p.starting_values_set());
        let run = p.run_params();
        assert_eq!(run["model"], "pk2.oral_1");
        assert_eq!(
            run["initial"],
            json!({ "cl": 2.0, "vc": 10.0, "q": 4.0, "vp": 8.0, "ka": 2.0 })
        );
        // A value the person set stays when the set changes and the name is still there.
        p.initial.insert("vc".to_owned(), 12.5);
        p.pending.remove("vc");
        p.set = ParameterSet::Micro;
        p.seed_placeholders();
        let mut names: Vec<&str> = p.initial.keys().map(String::as_str).collect();
        names.sort_unstable();
        assert_eq!(names, ["k10", "k12", "k21", "ka", "vc"]);
        assert_eq!(p.initial["vc"], 12.5);
        assert!(!p.pending.contains("vc") && p.pending.contains("k10"));
        p.accept_placeholders();
        assert!(p.starting_values_set());
        p.preview.dose = Some(100.0);
        let sim = p.simulate_params(24.0).unwrap();
        assert_eq!(sim["model"], "pk2.oral_1");
        assert_eq!(sim["params"]["k21"], 0.5);
        // Values of a one-compartment model are dropped.
        let mut q = page();
        q.compartments = Compartments::Two;
        q.seed_placeholders();
        assert!(!q.initial.contains_key("v") && q.initial.contains_key("cl"));
    }

    #[test]
    fn no_dose_or_missing_values_give_no_curve_request() {
        let mut p = page();
        p.preview.dose = None;
        assert!(p.simulate_params(10.0).is_none());
        p.dose_override = Some(50.0);
        assert_eq!(p.simulate_params(10.0).unwrap()["dose"], 50.0);
        p.initial.remove("ka");
        assert!(p.simulate_params(10.0).is_none());
    }

    #[test]
    fn options_are_set_and_read_by_path_and_thresholds_can_be_off() {
        let mut p = page();
        assert_eq!(p.option_f64("/max_iterations", 50.0), 50.0);
        p.set_option(&["max_iterations"], json!(80));
        p.set_option(&["flags", "max_cv_percent"], Value::Null);
        assert_eq!(p.option_f64("/max_iterations", 50.0), 80.0);
        assert_eq!(p.threshold("max_cv_percent", 50.0), None);
        assert_eq!(p.threshold("max_abs_correlation", 0.95), Some(0.95));
        assert_eq!(p.run_params()["options"]["max_iterations"], 80);
    }

    #[test]
    fn the_state_of_the_page_round_trips_as_data() {
        let mut p = page();
        p.set_option(&["convergence"], json!(0.001));
        p.view = None;
        p.preview = Preview::default();
        let text = serde_json::to_string(&p).unwrap();
        let back: FitPage = serde_json::from_str(&text).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn a_stored_fit_gives_back_its_page() {
        let view = json!({
            "id": 5, "label": "Fit", "kind": "fit", "status": { "state": "fresh" },
            "spec": { "kind": "fit", "worksheet": 2, "subject": "7", "model": "pk1.iv_infusion",
                      "dose": null, "weighting": "inv_y",
                      "initial": { "v": 12.0, "k": 0.2 },
                      "options": { "max_iterations": 30, "fixed": { "dur": 0.5 } } },
            "result": null,
        });
        let p = FitPage::from_view(&view).unwrap();
        assert_eq!(
            (p.analysis, p.worksheet, p.subject.as_str()),
            (Some(5), 2, "7")
        );
        assert_eq!((p.input, p.lag, p.duration), (Input::Infusion, false, 0.5));
        assert_eq!(p.weighting, "inv_y");
        assert_eq!(p.initial.get("v"), Some(&12.0));
        assert_eq!(p.options["fixed"], json!({}));
        assert_eq!(p.option_f64("/max_iterations", 50.0), 30.0);
        assert!(FitPage::from_view(&json!({ "kind": "nca" })).is_none());
        assert!(
            FitPage::from_view(&json!({ "kind": "fit", "spec": { "model": "nope" } })).is_none()
        );
    }

    #[test]
    fn the_preview_shows_the_objectives_sentence_and_clears_the_curve() {
        let mut p = page();
        p.adopt_preview(
            Some(Ok(json!({ "times": [0.0, 1.0], "conc": [0.0, 2.0], "secondary": { "half_life": 6.9 } }))),
            Ok(json!({ "wrss": 3.5, "dose": 100.0 })),
        );
        assert_eq!(p.preview.curve, vec![[0.0, 0.0], [1.0, 2.0]]);
        assert_eq!(p.preview.wrss, Some(3.5));
        assert_eq!(p.preview.error, None);
        p.adopt_preview(Some(Err("bad v".to_owned())), Err("bad too".to_owned()));
        assert!(p.preview.curve.is_empty() && p.preview.wrss.is_none());
        // The sentence of fit.evaluate is the one fit.run gives for the same values.
        assert_eq!(p.preview.error.as_deref(), Some("bad too"));
        p.adopt_preview(Some(Err("bad v".to_owned())), Ok(json!({ "wrss": 1.0 })));
        assert_eq!(p.preview.error.as_deref(), Some("bad v"));
    }

    #[test]
    fn observed_points_are_those_of_the_subject_with_both_numbers() {
        use crate::model::ColumnInfo;
        let col = |name: &str, role: &str| ColumnInfo {
            name: name.to_owned(),
            role: role.to_owned(),
            ..ColumnInfo::default()
        };
        let info = WorksheetInfo {
            columns: vec![
                col("id", "subject"),
                col("t", "time"),
                col("c", "concentration"),
            ],
            ..WorksheetInfo::default()
        };
        let table = Table {
            columns: vec!["id".into(), "t".into(), "c".into()],
            rows: vec![
                vec![json!("1"), json!(0.0), json!(0.0)],
                vec![json!("1"), json!(2.0), json!(null)],
                vec![json!("1"), json!(4.0), json!(1.5)],
                vec![json!("2"), json!(1.0), json!(9.0)],
            ],
        };
        let pts = observed(&info, &table, "1");
        assert_eq!(pts, vec![[0.0, 0.0], [4.0, 1.5]]);
        assert_eq!(last_time(&pts), Some(4.0));
        assert_eq!(last_time(&[]), None);
        assert!(observed(&WorksheetInfo::default(), &table, "1").is_empty());
    }
}
