//! The state of the population-fit page, as data: what the person chose (worksheet, model, random
//! effects, error model, method, optional starting values) and the parameters of `pop.fit` built
//! from it. The answer of the engine is kept as it came (`serde(skip)`, never saved with the page)
//! and read by [`summary`], which the drawing and the tests share. Nothing here computes a number:
//! every value shown is a field of the engine's answer, or of the R outputs of the second look.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::fmt;
use crate::modelinfo::{self, Compartments, Input, ModelInfo};

/// The error models of the population fit, by engine id, with what the person reads.
pub const ERROR_MODELS: [(&str, &str); 4] = [
    ("additive", "Additive"),
    ("proportional", "Proportional"),
    ("combined", "Combined (variances add)"),
    ("combined_sd", "Combined (SDs add)"),
];

/// The standard deviation each error model gives, for the line beside the choice.
pub fn error_formula(id: &str) -> &'static str {
    match id {
        "additive" => "σ = a",
        "proportional" => "σ = b·IPRED",
        "combined" => "σ² = a² + b²·IPRED²",
        "combined_sd" => "σ = a + b·IPRED",
        _ => "",
    }
}

/// The methods of the objective, by engine id.
pub const METHODS: [(&str, &str); 3] = [("foce_i", "FOCEI"), ("foce", "FOCE"), ("fo", "FO")];

/// The method the page starts on (the engine's default).
pub const DEFAULT_METHOD: &str = "foce_i";

/// The error model the page starts on. The engine has no default (the request must name one);
/// the page shows this choice and always sends it.
pub const DEFAULT_ERROR_MODEL: &str = "proportional";

/// The model the page starts on.
pub const DEFAULT_MODEL: &str = "pk1.oral_1";

/// The name of a method as the status line writes it.
pub fn method_label(id: &str) -> &str {
    METHODS
        .iter()
        .find(|(m, _)| *m == id)
        .map_or(id, |(_, label)| label)
}

/// The models the population fit takes: one and two compartments, every route.
pub fn models() -> impl Iterator<Item = &'static ModelInfo> {
    modelinfo::MODELS
        .iter()
        .filter(|m| m.compartments != Compartments::Three)
}

/// The model as the choice lists it: the route, the lag, the number of compartments.
pub fn model_label(model: &ModelInfo) -> String {
    let lag = if model.lag { " with a lag time" } else { "" };
    format!(
        "{}{lag}, {}",
        model.input.label(),
        model.compartments.label().to_lowercase()
    )
}

/// The structural parameters of a model in the population fit (POP-MOD-02, POP-MOD-04): `cl` and
/// `v` for one compartment, the clearance set for two, then the input. The duration of an
/// intravenous infusion is read from the dose row, so it is not a parameter here.
pub fn structural(model: &ModelInfo) -> Vec<&'static str> {
    let mut names: Vec<&'static str> = match model.compartments {
        Compartments::One => vec!["cl", "v"],
        _ => vec!["cl", "vc", "q", "vp"],
    };
    match model.input {
        Input::FirstOrder => names.push("ka"),
        Input::ZeroOrder => names.push("dur"),
        Input::Bolus | Input::Infusion => {}
    }
    if model.lag {
        names.push("tlag");
    }
    names
}

/// Whether a parameter may carry a random effect in this version: not the lag time and not the
/// duration of a zero-order input (POP-MOD-02 rule 3: the prediction has a kink there, and the
/// engine refuses such a request).
pub fn takes_random_effect(name: &str) -> bool {
    name != "tlag" && name != "dur"
}

/// The random effects ticked when a model is picked: every structural parameter that may carry
/// one ([`takes_random_effect`]).
pub fn default_random_effects(model: &ModelInfo) -> Vec<String> {
    structural(model)
        .into_iter()
        .filter(|n| takes_random_effect(n))
        .map(str::to_owned)
        .collect()
}

/// The keys of a stored `pop.fit` specification that the page does not edit and sends again as
/// stored; the options other than the method are kept under `options`.
const KEPT_KEYS: [&str; 6] = [
    "name",
    "omega",
    "omega_shape",
    "error_init",
    "fixed",
    "bounds",
];

/// A stored value that says something: not null, not an empty map or list, not `false` (the
/// stored specification writes every field, its defaults included).
fn says_something(x: &Value) -> bool {
    match x {
        Value::Null | Value::Bool(false) => false,
        Value::Object(m) => !m.is_empty(),
        Value::Array(a) => !a.is_empty(),
        _ => true,
    }
}

/// The error parameters of an error model, as `fixed` names them.
fn error_parameters(error_model: &str) -> &'static [&'static str] {
    match error_model {
        "additive" => &["a"],
        "proportional" => &["b"],
        _ => &["a", "b"],
    }
}

/// One run of the engine: the request it answers and the answer as it came.
#[derive(Debug, Clone, PartialEq)]
pub struct PopRun {
    /// What was sent (`pop.fit`) or the stored specification (`analysis.get`): the model and the
    /// error model of the answer, which the answer itself does not repeat.
    pub request: Value,
    /// The answer: the fields of the population fit at the top level.
    pub answer: Value,
    /// Seconds the call took, measured by the page around it; never stored in the analysis.
    pub wall_seconds: Option<f64>,
    /// The seal of the command, right after the run (the first digits, and the stamp line).
    pub seal: Option<(String, String)>,
    /// The R outputs for the same case, when the fit is one of the reference cases.
    pub second_look: Option<SecondLook>,
}

impl PopRun {
    pub fn new(request: Value, answer: Value, wall_seconds: Option<f64>) -> PopRun {
        let second_look = second_look_of(&request, &answer);
        PopRun {
            request,
            answer,
            wall_seconds,
            seal: None,
            second_look,
        }
    }

    /// The model id of the request.
    pub fn model(&self) -> &str {
        self.request
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or_default()
    }
}

/// Which part of the results the page scrolls to once (the links under the status line).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Estimates,
    SecondLook,
    Goodness,
    Individual,
}

/// The state of the population-fit page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PopPage {
    pub analysis: Option<u64>,
    pub worksheet: u64,
    /// The model id (`pk1.oral_1`).
    pub model: String,
    /// The parameters that carry a random effect.
    pub random_effects: Vec<String>,
    pub error_model: String,
    /// `foce_i`, `foce` or `fo`.
    pub method: String,
    /// Starting typical values as typed, by parameter; an empty text leaves the value to the
    /// engine's two-stage start.
    #[serde(default)]
    pub initial: BTreeMap<String, String>,
    /// What a stored fit was given that the page does not edit (`omega`, `fixed`, `bounds`,
    /// `error_init`, the options other than the method, set through the CLI or MCP): sent again
    /// as stored, so that "Run again" runs the same fit. A change on the page that makes one of
    /// them wrong drops it ([`PopPage::set_model`], [`PopPage::set_random_effect`],
    /// [`PopPage::set_error_model`]).
    #[serde(default)]
    pub kept: serde_json::Map<String, Value>,
    #[serde(skip)]
    pub run: Option<PopRun>,
    #[serde(skip)]
    pub error: Option<String>,
    #[serde(skip)]
    pub jump: Option<Section>,
}

impl PopPage {
    pub fn new(worksheet: u64) -> PopPage {
        PopPage {
            analysis: None,
            worksheet,
            model: DEFAULT_MODEL.to_owned(),
            random_effects: modelinfo::by_id(DEFAULT_MODEL)
                .map(default_random_effects)
                .unwrap_or_default(),
            error_model: DEFAULT_ERROR_MODEL.to_owned(),
            method: DEFAULT_METHOD.to_owned(),
            initial: BTreeMap::new(),
            kept: serde_json::Map::new(),
            run: None,
            error: None,
            jump: None,
        }
    }

    /// The page of a stored population fit (`analysis.get`), read from its specification.
    pub fn from_view(view: &Value) -> Option<PopPage> {
        if view.get("kind").and_then(Value::as_str) != Some("pop_fit") {
            return None;
        }
        let spec = view.get("spec").cloned().unwrap_or(Value::Null);
        let text = |key: &str| spec.get(key).and_then(Value::as_str).map(str::to_owned);
        let model = text("model")?;
        let random_effects = spec
            .get("random_effects")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        let initial = spec
            .get("initial")
            .and_then(Value::as_object)
            .map(|m| {
                m.iter()
                    .filter_map(|(k, x)| x.as_f64().map(|x| (k.clone(), fmt::exact(x))))
                    .collect()
            })
            .unwrap_or_default();
        let mut kept = serde_json::Map::new();
        for key in KEPT_KEYS {
            if let Some(x) = spec.get(key).filter(|x| says_something(x)) {
                kept.insert(key.to_owned(), x.clone());
            }
        }
        if let Some(options) = spec.get("options").and_then(Value::as_object) {
            let others: serde_json::Map<String, Value> = options
                .iter()
                .filter(|(k, x)| k.as_str() != "method" && says_something(x))
                .map(|(k, x)| (k.clone(), x.clone()))
                .collect();
            if !others.is_empty() {
                kept.insert("options".to_owned(), Value::Object(others));
            }
        }
        let mut page = PopPage {
            analysis: view.get("id").and_then(Value::as_u64),
            worksheet: spec.get("worksheet").and_then(Value::as_u64).unwrap_or(0),
            model,
            random_effects,
            error_model: text("error_model").unwrap_or_else(|| DEFAULT_ERROR_MODEL.to_owned()),
            method: spec
                .pointer("/options/method")
                .and_then(Value::as_str)
                .unwrap_or(DEFAULT_METHOD)
                .to_owned(),
            initial,
            kept,
            run: None,
            error: None,
            jump: None,
        };
        page.adopt_view(view, None);
        Some(page)
    }

    /// The model picked, from the catalogue of the interface.
    pub fn model_info(&self) -> Option<&'static ModelInfo> {
        modelinfo::by_id(&self.model)
    }

    /// The structural parameters of the model picked.
    pub fn parameters(&self) -> Vec<&'static str> {
        self.model_info().map(structural).unwrap_or_default()
    }

    /// Picks another model: the random effects go back to their defaults, and the starting values,
    /// fixed values and bounds of parameters the model does not have are dropped, with a stored
    /// starting Ω (its size was that of the old random effects).
    pub fn set_model(&mut self, id: &str) {
        let Some(model) = modelinfo::by_id(id) else {
            return;
        };
        if model.id == self.model {
            return;
        }
        self.model = model.id.to_owned();
        self.random_effects = default_random_effects(model);
        let names = structural(model);
        self.initial.retain(|k, _| names.contains(&k.as_str()));
        self.drop_omega();
        let errors = error_parameters(&self.error_model);
        let effects = self.random_effects.clone();
        self.retain_kept("fixed", |k| {
            names.contains(&k)
                || errors.contains(&k)
                || k.strip_prefix("omega.")
                    .is_some_and(|p| effects.iter().any(|r| r == p))
        });
        self.retain_kept("bounds", |k| names.contains(&k));
    }

    /// Ticks or unticks the random effect of a parameter. A stored starting Ω no longer has the
    /// right size and is dropped, and so is a fixed variance of a random effect taken away.
    pub fn set_random_effect(&mut self, name: &str, on: bool) {
        let before = self.random_effects.iter().any(|n| n == name);
        self.random_effects.retain(|n| n != name);
        if on {
            self.random_effects.push(name.to_owned());
        }
        if before != on {
            self.drop_omega();
            if !on {
                let gone = format!("omega.{name}");
                self.retain_kept("fixed", |k| k != gone);
            }
        }
    }

    /// Picks another error model: stored starting error parameters and fixed error parameters
    /// that the new model does not have are dropped.
    pub fn set_error_model(&mut self, id: &str) {
        if id == self.error_model {
            return;
        }
        self.error_model = id.to_owned();
        self.kept.remove("error_init");
        let errors = error_parameters(id);
        self.retain_kept("fixed", |k| !matches!(k, "a" | "b") || errors.contains(&k));
    }

    fn drop_omega(&mut self) {
        self.kept.remove("omega");
        self.kept.remove("omega_shape");
    }

    /// Keeps the entries of a kept map (`fixed`, `bounds`) whose name passes; an empty map goes.
    fn retain_kept(&mut self, key: &str, keep: impl Fn(&str) -> bool) {
        if let Some(map) = self.kept.get_mut(key).and_then(Value::as_object_mut) {
            map.retain(|k, _| keep(k));
            if map.is_empty() {
                self.kept.remove(key);
            }
        }
    }

    /// What is sent as stored without being on the page, for the line under the form; `None`
    /// when nothing is.
    pub fn kept_summary(&self) -> Option<String> {
        let mut names = Vec::new();
        for (key, x) in &self.kept {
            match (key.as_str(), x.as_object()) {
                ("options", Some(options)) => {
                    names.extend(options.keys().map(|k| format!("options.{k}")));
                }
                ("fixed" | "bounds", Some(map)) => {
                    let inner: Vec<&str> = map.keys().map(String::as_str).collect();
                    names.push(format!("{key} ({})", inner.join(", ")));
                }
                _ => names.push(key.clone()),
            }
        }
        (!names.is_empty()).then(|| names.join(", "))
    }

    /// The random effects in the order of the structural parameters (the order of the rows of Ω).
    pub fn ordered_random_effects(&self) -> Vec<String> {
        let names = self.parameters();
        let mut out: Vec<String> = names
            .iter()
            .filter(|n| self.random_effects.iter().any(|r| r == *n))
            .map(|n| (*n).to_owned())
            .collect();
        // A name the catalogue does not list (a stored analysis) keeps its place after them.
        for r in &self.random_effects {
            if !out.contains(r) {
                out.push(r.clone());
            }
        }
        out
    }

    /// The parameters without a random effect and without a starting value: the engine needs a
    /// value for each (its two-stage start computes only those with a random effect).
    pub fn missing_values(&self) -> Vec<&'static str> {
        self.parameters()
            .into_iter()
            .filter(|n| !self.random_effects.iter().any(|r| r == n))
            .filter(|n| self.initial.get(*n).is_none_or(|t| t.trim().is_empty()))
            .collect()
    }

    /// The starting values typed, read as numbers; a text that is not one is an error that names
    /// the parameter.
    pub fn starting_values(&self) -> Result<BTreeMap<String, f64>, String> {
        let names = self.parameters();
        let extravascular = self.model_info().is_some_and(|m| m.input.extravascular());
        let mut out = BTreeMap::new();
        for (name, text) in &self.initial {
            if text.trim().is_empty() || !names.contains(&name.as_str()) {
                continue;
            }
            match fmt::parse_number(text) {
                Some(x) => {
                    out.insert(name.clone(), x);
                }
                None => {
                    return Err(format!(
                        "The starting value of {} is not a number (`{}`): type a number, or leave the field empty for the engine's start.",
                        modelinfo::parameter_label(name, extravascular),
                        text.trim()
                    ));
                }
            }
        }
        Ok(out)
    }

    /// `pop.fit`.
    pub fn params(&self) -> Result<Value, String> {
        let mut params = json!({
            "worksheet": self.worksheet,
            "model": self.model,
            "random_effects": self.ordered_random_effects(),
            "error_model": self.error_model,
            "options": { "method": self.method },
        });
        let initial = self.starting_values()?;
        if !initial.is_empty() {
            params["initial"] = json!(initial);
        }
        if let Some(id) = self.analysis {
            params["analysis"] = json!(id);
        }
        // What the page does not edit goes again as stored; the method of the page wins.
        for (key, x) in &self.kept {
            if key == "options" {
                if let (Some(sent), Some(stored)) =
                    (params["options"].as_object_mut(), x.as_object())
                {
                    for (k, v) in stored {
                        sent.entry(k.clone()).or_insert_with(|| v.clone());
                    }
                }
            } else {
                params[key.as_str()] = x.clone();
            }
        }
        Ok(params)
    }

    /// Takes the answer of `pop.fit` to `request`.
    pub fn adopt_answer(&mut self, request: Value, answer: Value, wall_seconds: Option<f64>) {
        if let Some(id) = answer.get("analysis").and_then(Value::as_u64) {
            self.analysis = Some(id);
        }
        self.run = Some(PopRun::new(request, answer, wall_seconds));
        self.error = None;
    }

    /// Takes an analysis view (`analysis.get`, `analysis.run`): its stored specification and result.
    pub fn adopt_view(&mut self, view: &Value, wall_seconds: Option<f64>) {
        let spec = view.get("spec").cloned().unwrap_or(Value::Null);
        self.run = view
            .get("result")
            .filter(|r| r.is_object())
            .map(|result| PopRun::new(spec, result.clone(), wall_seconds));
        self.error = None;
    }
}

// ---- reading the answer ------------------------------------------------------------------------

/// One estimate with its standard error (`None` where the engine gives null).
#[derive(Debug, Clone, PartialEq)]
pub struct Estimate {
    pub name: String,
    pub value: Option<f64>,
    pub se: Option<f64>,
}

/// The variability of one random effect.
#[derive(Debug, Clone, PartialEq)]
pub struct Variability {
    pub name: String,
    /// Ω_kk and its standard error.
    pub variance: Option<f64>,
    pub variance_se: Option<f64>,
    /// ω_k = √Ω_kk and its standard error, as the engine gives them.
    pub sd: Option<f64>,
    pub sd_se: Option<f64>,
    /// η-shrinkage of this random effect.
    pub shrinkage: Option<f64>,
}

/// One observation of one subject.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Observation {
    pub time: f64,
    pub y: f64,
    pub pred: f64,
    pub ipred: Option<f64>,
    pub iwres: Option<f64>,
}

/// One subject of the answer.
#[derive(Debug, Clone, PartialEq)]
pub struct Subject {
    pub id: String,
    pub observations: Vec<Observation>,
}

/// What the page shows of an answer, read field by field; nothing is computed.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Summary {
    pub status: String,
    pub method: String,
    pub ofv_convention: String,
    pub n_subjects: Option<u64>,
    pub n_observations: Option<u64>,
    pub iterations: Option<u64>,
    /// The typical values, in the order of the structural parameters.
    pub typical: Vec<Estimate>,
    /// One row per random effect, in the order of the answer's `random_effects`.
    pub variability: Vec<Variability>,
    /// The error parameters (`a`, `b`).
    pub residual: Vec<Estimate>,
    pub eps_shrinkage: Option<f64>,
    pub ofv: Option<f64>,
    pub aic: Option<f64>,
    pub bic: Option<f64>,
    /// Subjects the checks of the data left out, with the sentence of the check.
    pub refused: Vec<(String, String)>,
    pub notes: Vec<String>,
}

/// The keys of `estimate` that are not typical values.
const NOT_TYPICAL: [&str; 5] = [
    "omega_diagonal",
    "omega_full",
    "omega_sd",
    "omega_correlation",
    "xi",
];

fn number(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64)
}

/// Reads an answer. `order` is the order of the structural parameters; a typical value it does not
/// name follows, in the order of the answer.
pub fn summary(answer: &Value, order: &[&str]) -> Summary {
    let text = |key: &str| {
        answer
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let estimate = answer.get("estimate");
    let se = answer.get("se");
    let pick = |root: Option<&Value>, group: Option<&str>, name: &str| {
        let node = match group {
            Some(g) => root.and_then(|r| r.get(g)),
            None => root,
        };
        number(node.and_then(|n| n.get(name)))
    };
    // Typical values: the keys of `estimate` that hold a number (the others are Ω and ξ).
    let mut typical_names: Vec<String> = Vec::new();
    if let Some(map) = estimate.and_then(Value::as_object) {
        for name in order {
            if map.get(*name).is_some_and(Value::is_number) {
                typical_names.push((*name).to_owned());
            }
        }
        for (name, value) in map {
            if value.is_number()
                && !NOT_TYPICAL.contains(&name.as_str())
                && !typical_names.contains(name)
            {
                typical_names.push(name.clone());
            }
        }
    }
    let typical = typical_names
        .into_iter()
        .map(|name| Estimate {
            value: pick(estimate, None, &name),
            se: pick(se, None, &name),
            name,
        })
        .collect();
    let effects: Vec<String> = answer
        .get("random_effects")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    // `eta_shrinkage` follows `random_effects`; Ω and its errors are keyed by name.
    let shrinkage = answer.get("eta_shrinkage").and_then(Value::as_array);
    let variability = effects
        .iter()
        .enumerate()
        .map(|(i, name)| Variability {
            name: name.clone(),
            variance: pick(estimate, Some("omega_diagonal"), name),
            variance_se: pick(se, Some("omega_diagonal"), name),
            sd: pick(estimate, Some("omega_sd"), name),
            sd_se: pick(se, Some("omega_sd"), name),
            shrinkage: number(shrinkage.and_then(|s| s.get(i))),
        })
        .collect();
    let mut residual = Vec::new();
    for key in ["a", "b"] {
        let value = pick(estimate, Some("xi"), key);
        if value.is_some() {
            residual.push(Estimate {
                name: key.to_owned(),
                value,
                se: pick(se, Some("xi"), key),
            });
        }
    }
    let count = |key: &str| answer.get(key).and_then(Value::as_u64);
    let refused = answer
        .get("refused_subjects")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|r| {
                    let field = |k: &str| {
                        r.get(k)
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned()
                    };
                    (field("subject"), fmt::plain(&field("message")))
                })
                .collect()
        })
        .unwrap_or_default();
    let notes = answer
        .get("notes")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(fmt::plain).collect())
        .unwrap_or_default();
    Summary {
        status: text("status"),
        method: text("method"),
        ofv_convention: text("ofv_convention"),
        n_subjects: count("n_subjects"),
        n_observations: count("n_observations"),
        iterations: count("iterations"),
        typical,
        variability,
        residual,
        eps_shrinkage: number(answer.get("eps_shrinkage")),
        ofv: number(answer.get("ofv")),
        aic: number(answer.get("aic")),
        bic: number(answer.get("bic")),
        refused,
        notes,
    }
}

/// The subjects of an answer with their observations (time, y, PRED, IPRED, IWRES), in the order
/// of the answer. An observation without a finite time, y or PRED is left out of the drawing.
pub fn subjects(answer: &Value) -> Vec<Subject> {
    let Some(list) = answer.get("subjects").and_then(Value::as_array) else {
        return Vec::new();
    };
    list.iter()
        .map(|s| Subject {
            id: s
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            observations: s
                .get("observations")
                .and_then(Value::as_array)
                .map(|obs| {
                    obs.iter()
                        .filter_map(|o| {
                            Some(Observation {
                                time: number(o.get("time"))?,
                                y: number(o.get("y"))?,
                                pred: number(o.get("pred"))?,
                                ipred: number(o.get("ipred")),
                                iwres: number(o.get("iwres")),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default(),
        })
        .collect()
}

// ---- the second look in R ----------------------------------------------------------------------

const NLME: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../oracle/expected/pop/second_look/nlme.json"
));
const SAEMIX: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../oracle/expected/pop/second_look/saemix.json"
));

/// The cases of the second look that the page shows, with the number of subjects of their dataset
/// (the files give the observations, not the subjects): Theoph has 12 subjects
/// (`oracle/data/population/event_theoph.csv`), Indometh 6 (`event_indometh.csv`).
const CASES: [(&str, u64); 4] = [
    ("theoph_additive", 12),
    ("theoph_proportional", 12),
    ("indometh_additive", 6),
    ("indometh_proportional", 6),
];

/// What one R tool gave for a case, as its file writes it.
#[derive(Debug, Clone, PartialEq)]
pub struct Reference {
    /// The column heading: the package, its version and its method.
    pub heading: String,
    /// The typical values on the natural scale, by parameter.
    pub theta: BTreeMap<String, f64>,
    /// The variances of Ω, by the parameter of the random effect.
    pub omega: BTreeMap<String, f64>,
    /// The residual parameter of the case's error model (`a` additive, `b` proportional).
    pub residual: Option<f64>,
}

/// The R outputs of the reference case that matches a fit.
#[derive(Debug, Clone, PartialEq)]
pub struct SecondLook {
    pub case: String,
    pub nlme: Reference,
    pub saemix: Reference,
}

fn reference_files() -> &'static Option<(Value, Value)> {
    static FILES: OnceLock<Option<(Value, Value)>> = OnceLock::new();
    FILES.get_or_init(|| {
        let nlme = serde_json::from_str(NLME).ok()?;
        let saemix = serde_json::from_str(SAEMIX).ok()?;
        Some((nlme, saemix))
    })
}

fn case_of<'a>(file: &'a Value, id: &str) -> Option<&'a Value> {
    file.get("cases")?
        .as_array()?
        .iter()
        .find(|c| c.get("id").and_then(Value::as_str) == Some(id))
}

fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn numbers_by_name(v: Option<&Value>) -> BTreeMap<String, f64> {
    v.and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .filter_map(|(k, x)| x.as_f64().map(|x| (k.clone(), x)))
                .collect()
        })
        .unwrap_or_default()
}

/// The diagonal of a matrix written as rows, by the names of its rows.
fn diagonal_by_name(matrix: Option<&Value>, names: &[String]) -> BTreeMap<String, f64> {
    let rows = matrix.and_then(Value::as_array);
    names
        .iter()
        .enumerate()
        .filter_map(|(i, name)| {
            let x = rows?.get(i)?.as_array()?.get(i)?.as_f64()?;
            Some((name.clone(), x))
        })
        .collect()
}

/// The version of a package as its file records it.
fn version(file: &Value, package: &str) -> String {
    file.pointer(&format!("/versions/{package}"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn nlme_reference(file: &Value, case: &Value, error_model: &str) -> Reference {
    // nlme names the rows of Ω by its log-scale parameters (`lcl`): the name of the parameter
    // follows the `l`.
    let names: Vec<String> = strings(case.get("omega_names"))
        .into_iter()
        .map(|n| n.strip_prefix('l').map_or(n.clone(), str::to_owned))
        .collect();
    let residual = match error_model {
        "additive" | "proportional" => number(case.get("sigma_hat")),
        _ => None,
    };
    Reference {
        heading: format!("nlme {} (Lindstrom-Bates)", version(file, "nlme")),
        theta: numbers_by_name(case.get("theta_hat")),
        omega: diagonal_by_name(case.get("omega_hat"), &names),
        residual,
    }
}

fn saemix_reference(file: &Value, case: &Value, error_model: &str) -> Reference {
    // saemix gives Ω in the order of the case's random effects, and `respar` as [a, b].
    let names = strings(case.get("random_effects"));
    let respar = case.get("respar").and_then(Value::as_array);
    let residual = match error_model {
        "additive" => number(respar.and_then(|r| r.first())),
        "proportional" => number(respar.and_then(|r| r.get(1))),
        _ => None,
    };
    Reference {
        heading: format!("saemix {} (SAEM)", version(file, "saemix")),
        theta: numbers_by_name(case.get("theta_hat")),
        omega: diagonal_by_name(case.get("omega_hat"), &names),
        residual,
    }
}

/// The reference case of a fit: same model, same error model, same set of random effects, same
/// number of observations and of subjects. `None` when no case matches.
pub fn second_look(
    model: &str,
    error_model: &str,
    random_effects: &[String],
    n_observations: u64,
    n_subjects: u64,
) -> Option<SecondLook> {
    let (nlme, saemix) = reference_files().as_ref()?;
    let mut wanted: Vec<&str> = random_effects.iter().map(String::as_str).collect();
    wanted.sort_unstable();
    for (id, subjects) in CASES {
        let Some(case) = case_of(nlme, id) else {
            continue;
        };
        let text = |key: &str| case.get(key).and_then(Value::as_str);
        let mut effects = strings(case.get("random_effects"));
        effects.sort_unstable();
        let same = text("model") == Some(model)
            && text("error_model") == Some(error_model)
            && effects
                .iter()
                .map(String::as_str)
                .eq(wanted.iter().copied())
            && case.get("n_obs").and_then(Value::as_u64) == Some(n_observations)
            && subjects == n_subjects;
        if !same {
            continue;
        }
        let other = case_of(saemix, id)?;
        return Some(SecondLook {
            case: id.to_owned(),
            nlme: nlme_reference(nlme, case, error_model),
            saemix: saemix_reference(saemix, other, error_model),
        });
    }
    None
}

/// The second look of a run, from the request (model, error model) and the answer (random
/// effects, counts).
fn second_look_of(request: &Value, answer: &Value) -> Option<SecondLook> {
    let model = request.get("model").and_then(Value::as_str)?;
    let error_model = request.get("error_model").and_then(Value::as_str)?;
    let effects = strings(answer.get("random_effects"));
    let n_observations = answer.get("n_observations").and_then(Value::as_u64)?;
    let n_subjects = answer.get("n_subjects").and_then(Value::as_u64)?;
    second_look(model, error_model, &effects, n_observations, n_subjects)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_page_builds_the_demo_request() {
        let page = PopPage::new(1);
        assert_eq!(
            page.params().unwrap(),
            json!({
                "worksheet": 1,
                "model": "pk1.oral_1",
                "random_effects": ["cl", "v", "ka"],
                "error_model": "proportional",
                "options": { "method": "foce_i" },
            })
        );
    }

    #[test]
    fn the_structural_parameters_are_those_of_the_population_fit() {
        let names = |id: &str| structural(modelinfo::by_id(id).unwrap());
        assert_eq!(names("pk1.oral_1"), ["cl", "v", "ka"]);
        assert_eq!(names("pk1.oral_1_lag"), ["cl", "v", "ka", "tlag"]);
        assert_eq!(names("pk1.iv_infusion"), ["cl", "v"]);
        assert_eq!(names("pk1.oral_0"), ["cl", "v", "dur"]);
        assert_eq!(names("pk2.iv_bolus"), ["cl", "vc", "q", "vp"]);
        assert_eq!(
            names("pk2.oral_1_lag"),
            ["cl", "vc", "q", "vp", "ka", "tlag"]
        );
        assert_eq!(
            default_random_effects(modelinfo::by_id("pk1.oral_1_lag").unwrap()),
            ["cl", "v", "ka"]
        );
        assert!(models().all(|m| m.compartments != Compartments::Three));
        assert_eq!(models().count(), 12);
    }

    #[test]
    fn starting_values_are_optional_text_and_a_parameter_without_effect_needs_one() {
        let mut page = PopPage::new(1);
        page.set_model("pk1.oral_1_lag");
        assert_eq!(page.missing_values(), ["tlag"]);
        page.initial.insert("tlag".to_owned(), "0,5".to_owned());
        page.initial.insert("cl".to_owned(), "  ".to_owned());
        assert!(page.missing_values().is_empty());
        let params = page.params().unwrap();
        assert_eq!(params["initial"], json!({ "tlag": 0.5 }));
        // The order of Ω follows the parameters, whatever the order of the ticks.
        page.set_random_effect("cl", false);
        page.set_random_effect("cl", true);
        assert_eq!(page.ordered_random_effects(), ["cl", "v", "ka"]);
        page.initial.insert("v".to_owned(), "abc".to_owned());
        let e = page.params().unwrap_err();
        assert!(e.contains("V/F") && e.contains("abc"), "{e}");
        // A model change drops the values of parameters it does not have.
        page.set_model("pk1.oral_1");
        assert!(!page.initial.contains_key("tlag"));
        page.analysis = Some(4);
        page.method = "fo".to_owned();
        page.initial.clear();
        let params = page.params().unwrap();
        assert_eq!(
            (
                params["analysis"].as_u64(),
                params["options"]["method"].as_str()
            ),
            (Some(4), Some("fo"))
        );
    }

    #[test]
    fn the_state_of_the_page_round_trips_as_data() {
        let mut page = PopPage::new(3);
        page.initial.insert("cl".to_owned(), "2.5".to_owned());
        let text = serde_json::to_string(&page).unwrap();
        let back: PopPage = serde_json::from_str(&text).unwrap();
        assert_eq!(back, page);
    }

    #[test]
    fn the_second_look_maps_omega_by_name_and_matches_on_every_field() {
        let effects = |names: &[&str]| names.iter().map(|n| (*n).to_owned()).collect::<Vec<_>>();
        let look = second_look(
            "pk1.oral_1",
            "proportional",
            &effects(&["ka", "cl", "v"]),
            120,
            12,
        )
        .unwrap();
        assert_eq!(look.case, "theoph_proportional");
        assert_eq!(look.nlme.heading, "nlme 3.1.168 (Lindstrom-Bates)");
        assert_eq!(look.saemix.heading, "saemix 3.5 (SAEM)");
        assert_eq!(look.nlme.omega["ka"], 0.381401388765892);
        assert_eq!(look.nlme.omega["cl"], 0.0551777436426188);
        assert_eq!(look.saemix.omega["v"], 0.0140442484055292);
        assert_eq!(look.nlme.theta["cl"], 2.75135779404378);
        assert_eq!(look.nlme.residual, Some(0.166126153771662));
        assert_eq!(look.saemix.residual, Some(0.160761512401916));
        let additive = second_look(
            "pk1.oral_1",
            "additive",
            &effects(&["cl", "v", "ka"]),
            120,
            12,
        )
        .unwrap();
        assert_eq!(additive.saemix.residual, Some(0.739020737749894));
        let all = effects(&["cl", "v", "ka"]);
        assert!(second_look("pk1.oral_1", "additive", &all, 132, 12).is_none());
        assert!(second_look("pk1.oral_1", "additive", &all, 120, 11).is_none());
        assert!(second_look("pk1.oral_1", "combined", &all, 120, 12).is_none());
        assert!(second_look("pk1.oral_1_lag", "additive", &all, 120, 12).is_none());
        assert!(second_look("pk1.oral_1", "additive", &effects(&["cl", "v"]), 120, 12).is_none());
        // The synthetic cases are not shown (their number of subjects is not in the files).
        assert!(second_look("pk1.iv_bolus", "additive", &effects(&["cl", "v"]), 30, 6).is_none());
        let indometh =
            second_look("pk2.iv_bolus", "additive", &effects(&["cl", "vc"]), 66, 6).unwrap();
        assert_eq!(indometh.nlme.omega["vc"], 0.0557237754370592);
    }

    #[test]
    fn no_default_random_effect_on_the_lag_time_or_the_zero_order_duration() {
        let zero_order: Vec<&ModelInfo> =
            models().filter(|m| m.input == Input::ZeroOrder).collect();
        assert!(!zero_order.is_empty());
        for model in models() {
            let effects = default_random_effects(model);
            assert!(
                !effects.iter().any(|n| n == "dur" || n == "tlag"),
                "{}: {effects:?}",
                model.id
            );
            assert!(!effects.is_empty(), "{}", model.id);
        }
        for model in zero_order {
            assert!(structural(model).contains(&"dur"), "{}", model.id);
            let mut page = PopPage::new(1);
            page.set_model(model.id);
            let sent = page.params().unwrap();
            assert!(
                !sent["random_effects"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|n| n == "dur"),
                "{}",
                model.id
            );
            // Without a random effect the engine needs a starting value for dur: the page says so.
            assert!(page.missing_values().contains(&"dur"), "{}", model.id);
        }
        assert!(
            takes_random_effect("cl")
                && !takes_random_effect("dur")
                && !takes_random_effect("tlag")
        );
    }

    fn stored_view() -> Value {
        json!({
            "id": 4, "kind": "pop_fit",
            "spec": {
                "worksheet": 1, "model": "pk1.oral_1", "random_effects": ["cl", "v", "ka"],
                "initial": { "cl": 2.8, "v": 32.0, "ka": 1.5 },
                "omega": [[0.06, 0.0, 0.0], [0.0, 0.016, 0.0], [0.0, 0.0, 0.4]],
                "omega_shape": null, "error_model": "proportional", "error_init": [0.15],
                "fixed": { "omega.ka": 0.4, "b": 0.15 }, "bounds": { "cl": { "lower": 1.0, "upper": null } },
                "options": { "method": "foce", "ofv_convention": null, "covariance": "r",
                             "exclude_subjects": ["12"], "evaluate_only": false },
            },
            "result": null,
        })
    }

    #[test]
    fn a_stored_fit_keeps_what_the_page_does_not_edit_and_sends_it_again() {
        let page = PopPage::from_view(&stored_view()).unwrap();
        let sent = page.params().unwrap();
        assert_eq!(sent["omega"], stored_view()["spec"]["omega"]);
        assert_eq!(sent["error_init"], json!([0.15]));
        assert_eq!(sent["fixed"], json!({ "omega.ka": 0.4, "b": 0.15 }));
        assert_eq!(sent["bounds"]["cl"]["lower"], 1.0);
        assert_eq!(sent["options"]["method"], "foce");
        assert_eq!(sent["options"]["covariance"], "r");
        assert_eq!(sent["options"]["exclude_subjects"], json!(["12"]));
        assert_eq!(sent["analysis"], 4);
        // The defaults the stored specification writes are not sent as if someone had set them.
        for key in ["omega_shape", "kind"] {
            assert!(sent.get(key).is_none(), "{key}");
        }
        for key in ["ofv_convention", "evaluate_only"] {
            assert!(sent["options"].get(key).is_none(), "{key}");
        }
        assert_eq!(
            page.kept_summary().as_deref(),
            Some(
                "bounds (cl), error_init, fixed (b, omega.ka), omega, options.covariance, options.exclude_subjects"
            )
        );
        // A page made here has nothing kept.
        assert_eq!(PopPage::new(1).kept_summary(), None);
    }

    #[test]
    fn a_change_on_the_page_drops_what_it_makes_wrong() {
        // A random effect taken away: Omega no longer has the right size, its fixed variance goes.
        let mut page = PopPage::from_view(&stored_view()).unwrap();
        page.set_random_effect("ka", false);
        let sent = page.params().unwrap();
        assert!(sent.get("omega").is_none());
        assert_eq!(sent["fixed"], json!({ "b": 0.15 }));
        assert_eq!(sent["options"]["exclude_subjects"], json!(["12"]));
        // Ticking what is already ticked changes nothing.
        let mut same = PopPage::from_view(&stored_view()).unwrap();
        same.set_random_effect("cl", true);
        assert!(same.params().unwrap().get("omega").is_some());
        // Another error model: its starting values and the fixed b go.
        let mut page = PopPage::from_view(&stored_view()).unwrap();
        page.set_error_model("additive");
        let sent = page.params().unwrap();
        assert!(sent.get("error_init").is_none());
        assert_eq!(sent["fixed"], json!({ "omega.ka": 0.4 }));
        // Another model: Omega goes, and the fixed and bounds of parameters it does not have.
        let mut page = PopPage::from_view(&stored_view()).unwrap();
        page.set_model("pk1.iv_bolus");
        let sent = page.params().unwrap();
        assert!(sent.get("omega").is_none());
        assert_eq!(sent["fixed"], json!({ "b": 0.15 }));
        assert_eq!(sent["bounds"]["cl"]["lower"], 1.0);
        assert_eq!(sent["options"]["covariance"], "r");
        // Picking the model already there changes nothing.
        let mut page = PopPage::from_view(&stored_view()).unwrap();
        page.set_model("pk1.oral_1");
        assert!(page.params().unwrap().get("omega").is_some());
    }

    #[test]
    fn the_summary_reads_omega_by_name_and_shrinkage_by_position() {
        let answer = json!({
            "status": "converged", "method": "foce_i", "ofv": 10.5, "aic": 12.0, "bic": null,
            "n_subjects": 3, "n_observations": 20, "iterations": 4,
            "random_effects": ["v", "cl"],
            "eta_shrinkage": [0.2, null],
            "eps_shrinkage": 0.1,
            "estimate": { "cl": 2.0, "v": 30.0, "omega_diagonal": { "cl": 0.04, "v": 0.09 },
                          "omega_sd": { "cl": 0.2, "v": 0.3 }, "omega_full": [[0.09, 0], [0, 0.04]],
                          "omega_correlation": [[1, 0], [0, 1]], "xi": { "b": 0.15 } },
            "se": { "cl": 0.1, "v": null, "omega_diagonal": { "cl": 0.01, "v": 0.02 },
                    "omega_sd": { "cl": 0.025, "v": null }, "omega_full": [[null, null], [null, null]],
                    "xi": { "b": 0.01 } },
            "refused_subjects": [{ "subject": "9", "message": "no `dose` row" }],
        });
        let s = summary(&answer, &["cl", "v"]);
        let names: Vec<&str> = s.typical.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["cl", "v"]);
        assert_eq!(s.typical[1].se, None);
        assert_eq!(s.variability[0].name, "v");
        assert_eq!(s.variability[0].variance, Some(0.09));
        assert_eq!(s.variability[0].shrinkage, Some(0.2));
        assert_eq!(s.variability[1].shrinkage, None);
        assert_eq!(
            s.residual,
            vec![Estimate {
                name: "b".to_owned(),
                value: Some(0.15),
                se: Some(0.01)
            }]
        );
        assert_eq!((s.aic, s.bic), (Some(12.0), None));
        assert_eq!(s.refused, vec![("9".to_owned(), "no dose row".to_owned())]);
        assert!(summary(&Value::Null, &[]).typical.is_empty());
    }
}
