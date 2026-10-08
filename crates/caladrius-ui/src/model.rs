//! What the UI reads from the engine's answers: small typed views of the JSON, tolerant of extra
//! fields. Nothing here computes; a missing field is an empty value, never a panic.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;

/// Reads `value` as `T`, or the default of `T` when it does not fit.
pub fn read<T: for<'de> Deserialize<'de> + Default>(value: &Value) -> T {
    serde_json::from_value(value.clone()).unwrap_or_default()
}

/// A worksheet in the project tree.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct WorksheetEntry {
    pub id: u64,
    pub name: String,
    pub rows: usize,
    pub columns: usize,
    pub subjects: Vec<String>,
    pub revision: u64,
}

/// Whether an analysis result matches its inputs.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Status {
    /// Never run.
    #[default]
    NoResult,
    /// Up to date.
    Fresh,
    /// Out of date.
    Stale {
        #[serde(default)]
        reasons: Vec<StaleReason>,
    },
}

/// Why a result is stale.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct StaleReason {
    pub code: String,
}

impl Status {
    pub fn is_stale(&self) -> bool {
        matches!(self, Status::Stale { .. })
    }

    /// The sentence that says what changed.
    pub fn sentence(&self) -> Option<String> {
        let Status::Stale { reasons } = self else {
            return None;
        };
        let parts: Vec<&str> = reasons
            .iter()
            .map(|r| match r.code.as_str() {
                "input_changed" => "the data changed",
                "options_changed" => "the options changed",
                "worksheet_missing" => "the worksheet is gone",
                _ => "an input changed",
            })
            .collect();
        Some(format!(
            "Out of date: {} since this result was computed. Run the analysis again to refresh it.",
            parts.join(" and ")
        ))
    }
}

/// An analysis in the project tree.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct AnalysisEntry {
    pub id: u64,
    pub label: String,
    pub kind: String,
    pub status: Status,
}

/// `project.describe`.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Overview {
    pub name: String,
    pub worksheets: Vec<WorksheetEntry>,
    pub analyses: Vec<AnalysisEntry>,
}

/// A column of a worksheet.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct ColumnInfo {
    pub name: String,
    pub role: String,
    pub unit: Option<String>,
    #[serde(rename = "type")]
    pub kind: String,
    pub missing: usize,
}

/// A warning about the units of a worksheet.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Warning {
    pub code: String,
    pub message: String,
}

/// `data.describe`, the `worksheet` part.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct WorksheetInfo {
    pub id: u64,
    pub name: String,
    pub revision: u64,
    pub rows: usize,
    pub subjects: Vec<String>,
    pub columns: Vec<ColumnInfo>,
    pub unit_warnings: Vec<Warning>,
    pub derived_units: BTreeMap<String, String>,
}

impl WorksheetInfo {
    /// The unit of the column with this role.
    pub fn unit_of(&self, role: &str) -> Option<&str> {
        self.columns
            .iter()
            .find(|c| c.role == role)
            .and_then(|c| c.unit.as_deref())
    }

    /// The name of the column with this role.
    pub fn column_of(&self, role: &str) -> Option<&str> {
        self.columns
            .iter()
            .find(|c| c.role == role)
            .map(|c| c.name.as_str())
    }
}

/// A table as `export.table` gives it: names and cells.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Table {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

impl Table {
    pub fn column_index(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c == name)
    }
}

/// One parameter of an NCA result.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct ParamJson {
    pub name: String,
    pub value: ParamValue,
}

/// A value or the reason for its absence.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct ParamValue {
    pub value: Option<f64>,
    pub not_calculated: Option<String>,
}

/// A point of the integrated profile.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct ProfilePoint {
    pub index: Option<usize>,
    pub time: f64,
    pub conc: f64,
    pub origin: String,
}

impl ProfilePoint {
    /// True for a value that a policy put in the place of a missing or BLQ value.
    pub fn is_replaced(&self) -> bool {
        matches!(
            self.origin.as_str(),
            "missing_replaced" | "negative_set_zero" | "blq_set"
        )
    }

    /// A sample of the input (not the point added at the dose time).
    pub fn is_sample(&self) -> bool {
        self.origin != "inserted_start"
    }
}

/// An input point left out of the profile.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct RemovedPoint {
    pub index: usize,
    pub time: f64,
    pub reason: String,
}

/// A candidate terminal phase.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Candidate {
    pub n_points: usize,
    pub time_first: f64,
    pub time_last: f64,
    pub lambda_z: f64,
    pub intercept: f64,
    pub r_squared: Option<f64>,
    pub adj_r_squared: Option<f64>,
    pub valid: bool,
    pub selected: bool,
    pub replaced_points: usize,
}

/// A quality flag: its code and the parameter it concerns, from the JSON.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Flag {
    pub code: String,
    pub parameter: Option<String>,
    /// The two parameters of a correlation flag.
    pub first: Option<String>,
    pub second: Option<String>,
}

impl Flag {
    /// The parameter of the summary this flag is about.
    pub fn concerns(&self) -> &str {
        match self.code.as_str() {
            "low_adjusted_r_squared" => "adj.r.squared",
            "short_span" => "span.ratio",
            "few_points" => "lambda.z.n.points",
            "high_extrapolation" => self.parameter.as_deref().unwrap_or("aucpext.obs"),
            "replaced_points_in_lambda_z" => "lambda.z",
            "area_past_tlast" => "auclast",
            // Flags of a fit: the parameter they name (the first of two for a correlation).
            "high_cv" | "interval_contains_zero" | "at_bound" => {
                self.parameter.as_deref().unwrap_or("")
            }
            "high_correlation" => self.first.as_deref().unwrap_or(""),
            _ => "",
        }
    }
}

/// The result of one subject, as the NCA page reads it.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct NcaOk {
    pub parameters: Vec<ParamJson>,
    pub profile: Vec<ProfilePoint>,
    pub removed: Vec<RemovedPoint>,
    pub lambda_z_candidates: Vec<Candidate>,
    pub flags: Vec<Flag>,
}

impl NcaOk {
    /// A parameter by its PKNCA name.
    pub fn get(&self, name: &str) -> Option<&ParamValue> {
        self.parameters
            .iter()
            .find(|p| p.name == name)
            .map(|p| &p.value)
    }

    /// The number of a parameter, when it was calculated.
    pub fn number(&self, name: &str) -> Option<f64> {
        self.get(name).and_then(|v| v.value)
    }

    /// The selected terminal phase.
    pub fn selected(&self) -> Option<&Candidate> {
        self.lambda_z_candidates.iter().find(|c| c.selected)
    }
}

/// An NCA outcome: a result or the message of the error.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Ok(Box<NcaOk>),
    Error(String),
    #[default]
    Missing,
}

/// One subject of an NCA result.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct SubjectResult {
    pub subject: String,
    pub dose: Option<f64>,
    pub outcome: Outcome,
    pub flag_messages: Vec<String>,
    pub not_calculated_messages: BTreeMap<String, String>,
}

/// An analysis as `analysis.get` and `nca.run` return it, for an NCA.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NcaView {
    pub id: u64,
    pub label: String,
    pub status: Status,
    pub spec: Value,
    pub subject: Option<SubjectResult>,
}

impl NcaView {
    /// Reads an analysis view; `None` when it is not an NCA.
    pub fn from_value(view: &Value) -> Option<NcaView> {
        if view.get("kind").and_then(Value::as_str) != Some("nca") {
            return None;
        }
        let subject = view
            .pointer("/result/subjects/0")
            .map(|v| serde_json::from_value::<SubjectResult>(v.clone()).unwrap_or_default());
        Some(NcaView {
            id: view.get("id").and_then(Value::as_u64).unwrap_or(0),
            label: view
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            status: read(view.get("status").unwrap_or(&Value::Null)),
            spec: view.get("spec").cloned().unwrap_or(Value::Null),
            subject,
        })
    }
}

// ---- fits --------------------------------------------------------------------------------

/// One observation with its prediction at the estimates.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct FitObservation {
    pub time: f64,
    pub observed: f64,
    pub predicted: f64,
    pub residual: f64,
    pub weighted_residual: f64,
    pub weight: f64,
}

/// A point of the fitted curve, as the engine computed it.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct CurvePoint {
    pub time: f64,
    pub conc: f64,
}

/// One row of the minimisation trace.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct TraceRow {
    pub iteration: usize,
    pub wrss: f64,
    pub estimates: Vec<f64>,
    pub step: Option<f64>,
    pub lambda: f64,
    pub halvings: usize,
    pub relative_decrease: Option<f64>,
}

/// The result of a fit.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct FitOk {
    pub status: String,
    pub parameters: Vec<String>,
    pub values: BTreeMap<String, f64>,
    pub observations: Vec<FitObservation>,
    pub curve: Vec<CurvePoint>,
    pub trace: Vec<TraceRow>,
    pub partials: Vec<Vec<f64>>,
    pub flags: Vec<Flag>,
}

impl FitOk {
    /// A named value (`estimate.v`, `se.k`, `wrss`...).
    pub fn value(&self, name: &str) -> Option<f64> {
        self.values.get(name).copied()
    }
}

/// A fit outcome: a result or the message of the error.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FitOutcome {
    Ok(Box<FitOk>),
    Error(String),
    #[default]
    Missing,
}

/// A fit analysis as `fit.run` and `analysis.get` return it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FitView {
    pub id: u64,
    pub label: String,
    pub status: Status,
    pub spec: Value,
    pub subject: String,
    pub dose: Option<f64>,
    pub n_observations: usize,
    pub n_missing: usize,
    pub outcome: FitOutcome,
    pub flag_messages: Vec<String>,
    pub status_message: String,
}

impl FitView {
    /// Reads an analysis view; `None` when it is not a fit.
    pub fn from_value(view: &Value) -> Option<FitView> {
        if view.get("kind").and_then(Value::as_str) != Some("fit") {
            return None;
        }
        let result = view.get("result").cloned().unwrap_or(Value::Null);
        let text = |key: &str| {
            result
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        Some(FitView {
            id: view.get("id").and_then(Value::as_u64).unwrap_or(0),
            label: view
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            status: read(view.get("status").unwrap_or(&Value::Null)),
            spec: view.get("spec").cloned().unwrap_or(Value::Null),
            subject: text("subject"),
            dose: result.get("dose").and_then(Value::as_f64),
            n_observations: result
                .get("n_observations")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize,
            n_missing: result.get("n_missing").and_then(Value::as_u64).unwrap_or(0) as usize,
            outcome: result
                .get("outcome")
                .map(|o| serde_json::from_value(o.clone()).unwrap_or_default())
                .unwrap_or_default(),
            flag_messages: read(result.get("flag_messages").unwrap_or(&Value::Null)),
            status_message: text("status_message"),
        })
    }

    pub fn ok(&self) -> Option<&FitOk> {
        match &self.outcome {
            FitOutcome::Ok(ok) => Some(ok),
            _ => None,
        }
    }
}

/// The answer of `model.simulate`.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Simulation {
    pub analysis: Option<u64>,
    pub times: Vec<f64>,
    pub conc: Vec<f64>,
    pub auc: Vec<f64>,
    pub secondary: BTreeMap<String, f64>,
}

impl Simulation {
    /// The curve as (time, concentration) pairs.
    pub fn curve(&self) -> Vec<[f64; 2]> {
        self.times
            .iter()
            .copied()
            .zip(self.conc.iter().copied())
            .map(|(t, c)| [t, c])
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn status_and_overview_are_read_from_the_engine_json() {
        let o: Overview = read(&json!({
            "name": "p",
            "worksheets": [{ "id": 1, "name": "w", "rows": 3, "columns": 2, "subjects": ["1"], "revision": 0 }],
            "analyses": [
                { "id": 2, "label": "NCA", "kind": "nca", "status": { "state": "stale", "reasons": [{ "code": "input_changed" }] } },
                { "id": 3, "label": "NCA", "kind": "nca", "status": { "state": "fresh" } },
                { "id": 4, "label": "NCA", "kind": "nca", "status": { "state": "no_result" } },
            ],
            "history_length": 3,
        }));
        assert_eq!(o.worksheets[0].subjects, ["1"]);
        assert!(o.analyses[0].status.is_stale());
        assert_eq!(o.analyses[1].status, Status::Fresh);
        assert_eq!(o.analyses[2].status, Status::NoResult);
        let s = o.analyses[0].status.sentence().unwrap();
        assert!(
            s.contains("the data changed") && s.contains("Run the analysis again"),
            "{s}"
        );
        assert_eq!(Status::Fresh.sentence(), None);
    }

    #[test]
    fn garbage_reads_as_empty_not_as_a_panic() {
        let o: Overview = read(&json!("nonsense"));
        assert!(o.worksheets.is_empty());
        let o: Overview = read(&json!({ "worksheets": 5 }));
        assert!(o.worksheets.is_empty());
        assert_eq!(NcaView::from_value(&json!({ "kind": "fit" })), None);
        let v = NcaView::from_value(&json!({ "kind": "nca", "id": 2, "result": { "subjects": [{ "subject": "1", "outcome": { "error": "bad" } }] } })).unwrap();
        assert_eq!(v.subject.unwrap().outcome, Outcome::Error("bad".to_owned()));
    }

    #[test]
    fn a_fit_view_is_read_from_the_engine_json() {
        let view = json!({
            "id": 3, "label": "Fit pk1.oral_1 to w, subject 1", "kind": "fit",
            "status": { "state": "fresh" }, "spec": { "kind": "fit", "model": "pk1.oral_1" },
            "result": { "kind": "fit", "subject": "1", "dose": 100.0, "n_observations": 9, "n_missing": 1,
                "flag_messages": ["ka is imprecise"], "status_message": "converged",
                "outcome": { "ok": { "status": "converged", "parameters": ["v", "k"],
                    "values": { "estimate.v": 20.5, "se.v": 1.0 },
                    "observations": [{ "time": 1.0, "observed": 3.0, "predicted": 2.9, "residual": 0.1, "weighted_residual": 0.1, "weight": 1.0 }],
                    "curve": [{ "time": 0.0, "conc": 0.0 }], "trace": [], "partials": [[1.0]],
                    "flags": [{ "code": "high_cv", "parameter": "ka", "value": 80.0, "threshold": 50.0 }] } } },
        });
        let v = FitView::from_value(&view).unwrap();
        assert_eq!((v.id, v.subject.as_str(), v.n_missing), (3, "1", 1));
        let ok = v.ok().unwrap();
        assert_eq!(ok.value("estimate.v"), Some(20.5));
        assert_eq!(ok.flags[0].concerns(), "ka");
        assert_eq!(v.status_message, "converged");
        assert!(FitView::from_value(&json!({ "kind": "nca" })).is_none());
        let failed = json!({ "id": 4, "kind": "fit", "result": { "outcome": { "error": "bad" } } });
        assert_eq!(
            FitView::from_value(&failed).unwrap().outcome,
            FitOutcome::Error("bad".to_owned())
        );
        let sim: Simulation =
            read(&json!({ "times": [0.0, 1.0], "conc": [0.0, 2.0], "secondary": { "cl": 1.0 } }));
        assert_eq!(sim.curve(), vec![[0.0, 0.0], [1.0, 2.0]]);
    }

    #[test]
    fn flags_say_which_parameter_they_concern() {
        let f = |code: &str, parameter: Option<&str>| Flag {
            code: code.to_owned(),
            parameter: parameter.map(str::to_owned),
            ..Flag::default()
        };
        assert_eq!(f("short_span", None).concerns(), "span.ratio");
        assert_eq!(
            f("high_extrapolation", Some("aucpext.pred")).concerns(),
            "aucpext.pred"
        );
        assert_eq!(f("nope", None).concerns(), "");
    }
}
