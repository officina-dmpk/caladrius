//! Running the analyses: from a project and a spec to a result. This is the only place where the
//! numerical crates are called; the worksheet is turned into their inputs here.

use std::collections::BTreeMap;

use caladrius_fit::FitInput;
use caladrius_models::{ModelId, ModelInput};
use caladrius_nca::{NcaInput, Route};
use caladrius_project::{
    AnalysisResult, AnalysisSpec, FitRun, FitSpec, NcaSpec, NcaSubjectResult, Outcome, Profile,
    Project, SimulationRun, SimulationSpec, Worksheet,
};

use crate::error::CommandError;

/// Runs the analysis described by `spec` on the current state of `project`. Nothing is stored.
pub(crate) fn run_spec(
    project: &Project,
    spec: &AnalysisSpec,
) -> Result<AnalysisResult, CommandError> {
    match spec {
        AnalysisSpec::Nca(s) => run_nca(project, s),
        AnalysisSpec::Fit(s) => run_fit(project, s),
        AnalysisSpec::Simulation(s) => run_simulation(s),
    }
}

/// The route named by a route column. An infusion needs its duration, which a text column cannot
/// give, so it is refused with the way out.
fn route_from_text(text: &str) -> Result<Route, String> {
    match text.trim().to_ascii_lowercase().as_str() {
        "extravascular" | "oral" | "po" | "ev" => Ok(Route::Extravascular),
        "iv_bolus" | "iv" | "bolus" | "iv bolus" => Ok(Route::IvBolus),
        "iv_infusion" | "infusion" | "iv infusion" => Err(
            "the route column says infusion, which needs a duration; give the route in the analysis as {\"iv_infusion\": {\"duration\": ...}}"
                .to_owned(),
        ),
        other => Err(format!(
            "the route `{other}` is not recognised; use extravascular or iv_bolus, or give the route in the analysis"
        )),
    }
}

fn nca_subject(ws: &Worksheet, subject: &str, spec: &NcaSpec) -> NcaSubjectResult {
    let fail = |message: String, dose: Option<f64>, route: Option<Route>| NcaSubjectResult {
        subject: subject.to_owned(),
        dose,
        route,
        outcome: Outcome::Error(message),
    };
    let profile = match ws.profile(Some(subject)) {
        Ok(p) => p,
        Err(e) => return fail(e.message, None, None),
    };
    let dose = spec.dose.or(profile.dose);
    let route = match (&spec.route, &profile.route) {
        (Some(r), _) => *r,
        (None, Some(text)) => match route_from_text(text) {
            Ok(r) => r,
            Err(message) => return fail(message, dose, None),
        },
        (None, None) => {
            return fail(
                "no route of administration: give `route` in the analysis or add a route column to the worksheet"
                    .to_owned(),
                dose,
                None,
            );
        }
    };
    let input = NcaInput {
        time: profile.time,
        conc: profile.conc,
        // A missing dose is NaN: only the dose-dependent parameters are then not calculated.
        dose: dose.unwrap_or(f64::NAN),
        route,
        options: spec.options.clone(),
    };
    let outcome = match caladrius_nca::run(&input) {
        Ok(result) => Outcome::Ok(result),
        Err(e) => Outcome::Error(e.to_string()),
    };
    NcaSubjectResult {
        subject: subject.to_owned(),
        dose,
        route: Some(route),
        outcome,
    }
}

fn run_nca(project: &Project, spec: &NcaSpec) -> Result<AnalysisResult, CommandError> {
    let ws = project.worksheet(spec.worksheet)?;
    let all = ws.subjects();
    let subjects = match &spec.subject {
        Some(s) if !all.contains(s) => {
            return Err(CommandError::new(
                "unknown_subject",
                format!(
                    "worksheet `{}` has no subject `{s}`; its subjects are: {}",
                    ws.name(),
                    all.join(", ")
                ),
            ));
        }
        Some(s) => vec![s.clone()],
        None => all,
    };
    if subjects.is_empty() {
        return Err(CommandError::new(
            "no_subjects",
            format!(
                "worksheet `{}` has no subject with data; check the subject column",
                ws.name()
            ),
        ));
    }
    Ok(AnalysisResult::Nca {
        subjects: subjects.iter().map(|s| nca_subject(ws, s, spec)).collect(),
    })
}

/// The data of a fit: finite observations of one subject.
pub(crate) struct FitData {
    pub(crate) subject: String,
    pub(crate) dose: f64,
    pub(crate) time: Vec<f64>,
    pub(crate) conc: Vec<f64>,
    pub(crate) n_missing: usize,
}

/// Reads the observations of one subject for a fit. Rows without a concentration are left out
/// (and counted); the dose comes from `dose` or from the worksheet.
pub(crate) fn fit_data(
    ws: &Worksheet,
    subject: Option<&str>,
    dose: Option<f64>,
) -> Result<FitData, CommandError> {
    let Profile {
        subject,
        time,
        conc,
        dose: sheet_dose,
        ..
    } = ws.profile(subject)?;
    let dose = dose.or(sheet_dose).ok_or_else(|| {
        CommandError::new(
            "dose_required",
            format!(
                "no dose for subject {subject}: give `dose` or add a dose column to worksheet `{}`",
                ws.name()
            ),
        )
    })?;
    let n_rows = time.len();
    let (time, conc): (Vec<f64>, Vec<f64>) = time
        .into_iter()
        .zip(conc)
        .filter(|(_, c)| c.is_finite())
        .unzip();
    let n_missing = n_rows.saturating_sub(time.len());
    Ok(FitData {
        subject,
        dose,
        time,
        conc,
        n_missing,
    })
}

fn run_fit(project: &Project, spec: &FitSpec) -> Result<AnalysisResult, CommandError> {
    let ws = project.worksheet(spec.worksheet)?;
    let data = fit_data(ws, spec.subject.as_deref(), spec.dose)?;
    let input = FitInput {
        model: spec.model,
        dose: data.dose,
        time: data.time.clone(),
        conc: data.conc.clone(),
        weighting: spec.weighting,
        initial: spec.initial.clone(),
        options: spec.options.clone(),
    };
    let result =
        caladrius_fit::run(&input).map_err(|e| CommandError::new("fit_error", e.to_string()))?;
    Ok(AnalysisResult::Fit(FitRun {
        subject: data.subject,
        dose: Some(data.dose),
        n_observations: data.time.len(),
        n_missing: data.n_missing,
        outcome: Outcome::Ok(result),
    }))
}

fn run_simulation(spec: &SimulationSpec) -> Result<AnalysisResult, CommandError> {
    let output = simulate(&spec.input)?;
    Ok(AnalysisResult::Simulation(SimulationRun {
        times: spec.input.times.clone(),
        outcome: Outcome::Ok(output),
    }))
}

pub(crate) fn simulate(input: &ModelInput) -> Result<caladrius_models::ModelOutput, CommandError> {
    caladrius_models::run(input).map_err(|e| CommandError::new("model_error", e.to_string()))
}

/// Initial estimates from the data of one subject (FIT-INI-01).
pub(crate) fn initial_estimates(
    data: &FitData,
    model: ModelId,
    fixed: &BTreeMap<String, f64>,
) -> Result<BTreeMap<String, f64>, CommandError> {
    caladrius_fit::initial_estimates(model, data.dose, &data.time, &data.conc, fixed)
        .map_err(|e| CommandError::new("fit_error", e.to_string()))
}
