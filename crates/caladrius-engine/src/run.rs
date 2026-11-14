//! Running the analyses: from a project and a spec to a result. This is the only place where the
//! numerical crates are called; the worksheet is turned into their inputs here.

use std::collections::BTreeMap;

use caladrius_fit::FitInput;
use caladrius_fit::pop::{PopError, PopRequest, SubjectInput};
use caladrius_models::{ModelId, ModelInput};
use caladrius_nca::{NcaInput, Route};
use caladrius_project::{
    ALL_SUBJECTS, AnalysisResult, AnalysisSpec, CovariateValue, Dataset, FitRun, FitSpec, Layout,
    NcaSpec, NcaSubjectResult, Outcome, PopFitRun, PopFitSpec, PopRefusedSubject, Profile, Project,
    Purpose, SimulationRun, SimulationSpec, SubjectData, Worksheet, WorksheetId,
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
        AnalysisSpec::PopFit(s) => run_pop_fit(project, s),
        AnalysisSpec::Simulation(s) => run_simulation(s),
        AnalysisSpec::Plot(s) => Ok(AnalysisResult::Plot(crate::plot::draw(project, s)?)),
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

/// Whether a route column text names an intravenous infusion.
fn says_infusion(text: &str) -> bool {
    matches!(
        text.trim().to_ascii_lowercase().as_str(),
        "iv_infusion" | "infusion" | "iv infusion"
    )
}

/// The dose of the analysis wins over the dose row of an event table, and a note says so when they
/// differ. The profile layout keeps its old rule without a note (DAT-LEG-02).
fn dose_note(
    layout: Layout,
    option: Option<f64>,
    file: Option<f64>,
    subject: &str,
    notes: &mut Vec<String>,
) {
    if let (Layout::Event, Some(given), Some(row)) = (layout, option, file) {
        if given != row {
            notes.push(format!(
                "subject {subject}: the dose {given} of the analysis is used; the dose row gives {row}"
            ));
        }
    }
}

/// The covariates of `subject` in the dataset, to be copied into its result (DAT-USE-03).
fn covariates_of(ds: &Dataset, subject: &str) -> BTreeMap<String, Option<CovariateValue>> {
    ds.subjects
        .iter()
        .find(|s| s.id == subject)
        .map(|s| s.covariates.clone())
        .unwrap_or_default()
}

/// The route of a subject whose dose row may be an infusion (DAT-USE-01). The route of the
/// analysis wins over the file; a note says so when they disagree about an infusion.
fn nca_route(
    spec: &NcaSpec,
    text: Option<&str>,
    file_duration: Option<f64>,
    subject: &str,
    notes: &mut Vec<String>,
) -> Result<Route, String> {
    match (spec.route, file_duration) {
        (Some(Route::IvInfusion { duration }), Some(file)) => {
            if duration != file {
                notes.push(format!(
                    "subject {subject}: the infusion duration {duration} of the analysis is used; the dose row gives {file}"
                ));
            }
            Ok(Route::IvInfusion { duration })
        }
        (Some(route), Some(file)) => {
            notes.push(format!(
                "subject {subject}: the route of the analysis is used; the dose row is an infusion of duration {file}"
            ));
            Ok(route)
        }
        (Some(route), None) => Ok(route),
        // A dose row that is an infusion and a route column that says something else, or that
        // cannot be read, contradict each other: the person decides (DAT-USE-01).
        (None, Some(duration)) => match text {
            None => Ok(Route::IvInfusion { duration }),
            Some(text) => match route_from_text(text) {
                Ok(_) => Err(format!(
                    "the dose row of subject {subject} is an infusion but the route column says {}; make them agree, or give the route in the analysis",
                    text.trim().to_ascii_lowercase()
                )),
                Err(_) if says_infusion(text) => Ok(Route::IvInfusion { duration }),
                Err(message) => Err(format!(
                    "the dose row of subject {subject} is an infusion, but {message}"
                )),
            },
        },
        (None, None) => match text {
            Some(text) => route_from_text(text),
            None => Err(
                "no route of administration: give `route` in the analysis or add a route column to the worksheet"
                    .to_owned(),
            ),
        },
    }
}

fn nca_subject(ws: &Worksheet, ds: &Dataset, subject: &str, spec: &NcaSpec) -> NcaSubjectResult {
    let covariates = covariates_of(ds, subject);
    let fail = |message: String,
                dose: Option<f64>,
                route: Option<Route>,
                n_before_dose: usize,
                notes: Vec<String>| NcaSubjectResult {
        subject: subject.to_owned(),
        dose,
        route,
        outcome: Outcome::Error(message),
        covariates: covariates.clone(),
        n_before_dose,
        notes,
    };
    let data = match ws.subject_data(ds, Some(subject), Purpose::Nca) {
        Ok(d) => d,
        Err(e) => return fail(e.message, None, None, 0, Vec::new()),
    };
    let n_before_dose = data.n_before_dose;
    let mut notes = data.notes;
    let profile = data.profile;
    dose_note(ds.layout, spec.dose, profile.dose, subject, &mut notes);
    let dose = spec.dose.or(profile.dose);
    let route = match nca_route(
        spec,
        profile.route.as_deref(),
        data.infusion_duration,
        subject,
        &mut notes,
    ) {
        Ok(r) => r,
        Err(message) => return fail(message, dose, None, n_before_dose, notes),
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
        covariates,
        n_before_dose,
        notes,
    }
}

fn run_nca(project: &Project, spec: &NcaSpec) -> Result<AnalysisResult, CommandError> {
    let ws = project.worksheet(spec.worksheet)?;
    let ds = ws.dataset();
    let all = ws.analysis_subjects(&ds);
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
        subjects: subjects
            .iter()
            .map(|s| nca_subject(ws, &ds, s, spec))
            .collect(),
    })
}

/// The data of a fit: finite observations of one subject.
pub(crate) struct FitData {
    pub(crate) subject: String,
    pub(crate) dose: f64,
    pub(crate) time: Vec<f64>,
    pub(crate) conc: Vec<f64>,
    pub(crate) n_missing: usize,
    pub(crate) n_blq: usize,
    /// The duration of the dose row, when it is an infusion (event layout).
    pub(crate) duration: Option<f64>,
    pub(crate) covariates: BTreeMap<String, Option<CovariateValue>>,
    pub(crate) n_before_dose: usize,
    pub(crate) notes: Vec<String>,
}

/// [`fit_data_in`] on the dataset of `ws` as it is now.
pub(crate) fn fit_data(
    ws: &Worksheet,
    subject: Option<&str>,
    dose: Option<f64>,
) -> Result<FitData, CommandError> {
    fit_data_in(ws, &ws.dataset(), subject, dose)
}

/// Reads the observations of one subject for a fit. Rows without a concentration, and rows below
/// the limit of quantification (NCA-DAT-05c, FIT-DAT-01), are left out; the counts are those of
/// the dataset (FIT-ML-20). The dose comes from `dose` or from the worksheet.
pub(crate) fn fit_data_in(
    ws: &Worksheet,
    ds: &Dataset,
    subject: Option<&str>,
    dose: Option<f64>,
) -> Result<FitData, CommandError> {
    let SubjectData {
        profile:
            Profile {
                subject,
                time,
                conc,
                blq,
                dose: sheet_dose,
                ..
            },
        infusion_duration,
        counts,
        covariates,
        n_before_dose,
        mut notes,
    } = ws.subject_data(ds, subject, Purpose::Fit)?;
    dose_note(ds.layout, dose, sheet_dose, &subject, &mut notes);
    let dose = dose.or(sheet_dose).ok_or_else(|| {
        CommandError::new(
            "dose_required",
            format!(
                "no dose for subject {subject}: give `dose` or add a dose column to worksheet `{}`",
                ws.name()
            ),
        )
    })?;
    let (time, conc): (Vec<f64>, Vec<f64>) = time
        .into_iter()
        .zip(conc)
        .zip(blq)
        .filter(|((_, c), below)| c.is_finite() && !below)
        .map(|(pair, _)| pair)
        .unzip();
    // The rows that reach the fit are the `n_used` of the dataset. Two readings of one table that
    // disagree are a bug to see, not a number to pick.
    if time.len() != counts.n_used {
        return Err(CommandError::new(
            "count_mismatch",
            format!(
                "subject {subject}: {} observations reach the fit but the dataset counts {}; read the file again",
                time.len(),
                counts.n_used
            ),
        ));
    }
    Ok(FitData {
        subject,
        dose,
        time,
        conc,
        n_missing: counts.n_missing,
        n_blq: counts.n_blq,
        duration: infusion_duration,
        covariates,
        n_before_dose,
        notes,
    })
}

/// What a fit says about its data besides the estimates.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FitMeta {
    pub(crate) subject: String,
    pub(crate) dose: f64,
    /// `n_used` of the dataset (FIT-ML-20): the observations that reach the fit.
    pub(crate) n_observations: usize,
    pub(crate) n_missing: usize,
    pub(crate) n_blq: usize,
    pub(crate) covariates: BTreeMap<String, Option<CovariateValue>>,
    pub(crate) n_before_dose: usize,
    pub(crate) notes: Vec<String>,
}

/// Whether the dose row of the file gives the duration of this model's input: an intravenous
/// infusion. For a model with a zero-order absorption the duration is a parameter, not a fact of
/// the dose.
fn takes_dose_duration(model: ModelId) -> bool {
    matches!(
        model,
        ModelId::IvInfusion | ModelId::Pk2IvInfusion | ModelId::Pk3IvInfusion
    )
}

/// Holds `dur` at the duration of the dose row (DAT-USE-02) unless the person has fixed or fitted
/// it. A model that does not take its duration from the dose, a different fixed value and a `dur`
/// among the initial estimates each get a note, since the file's duration is then not used.
pub(crate) fn file_duration(
    model: ModelId,
    data_duration: Option<f64>,
    subject: &str,
    fixed: &mut BTreeMap<String, f64>,
    initial: &BTreeMap<String, f64>,
    notes: &mut Vec<String>,
) {
    let Some(file) = data_duration else {
        return;
    };
    if !takes_dose_duration(model) {
        notes.push(format!(
            "subject {subject}: the dose row is an infusion of duration {file}, which model {} does not read from the file; the duration of the dose row is not used",
            model.id()
        ));
        return;
    }
    match (fixed.get("dur"), initial.get("dur")) {
        (Some(given), _) if *given != file => notes.push(format!(
            "subject {subject}: the duration `dur` {given} of the options is used; the dose row gives {file}"
        )),
        (Some(_), _) => {}
        (None, Some(start)) => notes.push(format!(
            "subject {subject}: `dur` is among the initial estimates ({start}), so it is fitted from there; the dose row gives {file}"
        )),
        (None, None) => {
            fixed.insert("dur".to_owned(), file);
        }
    }
}

/// Reads the observations of `spec` from the project and starts the fit (the initial estimates of
/// FIT-INI-01 when `spec.initial` is empty). The iterations have not begun.
pub(crate) fn start_fit(
    project: &Project,
    spec: &FitSpec,
) -> Result<(caladrius_fit::FitRun<ModelId>, FitMeta), CommandError> {
    let ws = project.worksheet(spec.worksheet)?;
    let ds = ws.dataset();
    let mut data = fit_data_in(ws, &ds, spec.subject.as_deref(), spec.dose)?;
    if spec.subject.as_deref() == Some(ALL_SUBJECTS) {
        // `all` is the label of a subject of this worksheet, and it names that subject.
        data.notes.push(format!(
            "`all` is the label of a subject of worksheet `{}`, so it names that subject and not every subject",
            ws.name()
        ));
    }
    start_data(spec, data).map_err(|failure| failure.error)
}

/// Why a fit could not start, with what was known of the subject.
pub(crate) struct StartFailure {
    error: CommandError,
    /// True when the request itself is wrong (the model, the options, the initial estimates given),
    /// not the data of this subject: every subject would be refused the same way.
    request_level: bool,
    meta: FitMeta,
}

/// Whether a fit error comes from the request and not from the data of one subject.
///
/// Estimates and fixed values the person gave are the same for every subject, so an error that
/// names one of them refuses the whole request. An estimate that comes from the data (generated
/// when `initial` is empty) and the `dur` that comes from the dose row differ from subject to
/// subject: an error about them is that subject's own.
fn is_request_error(error: &caladrius_fit::FitError, spec: &FitSpec) -> bool {
    use caladrius_fit::FitError as E;
    match error {
        E::NoParameters
        | E::InvalidOption { .. }
        | E::AnalyticDerivativesUnavailable { .. }
        | E::RegimenDerivativesUnavailable { .. } => true,
        E::InitialOutsideBounds { parameter, .. } => spec.initial.contains_key(parameter),
        E::InitialEstimates { source } => match model_error_parameter(source) {
            Some(name) => {
                spec.initial.contains_key(name)
                    || spec.options.fixed.contains_key(name)
                    // With estimates given, the fitted parameters are exactly the ones named and
                    // the rest is fixed by the person; only the file can supply `dur`.
                    || (!spec.initial.is_empty() && name != "dur")
            }
            None => !spec.initial.is_empty(),
        },
        _ => false,
    }
}

/// The parameter a model error names, when it names one.
fn model_error_parameter(error: &caladrius_models::ModelError) -> Option<&str> {
    use caladrius_models::ModelError as M;
    match error {
        M::UnknownParameter { name, .. }
        | M::ParameterNotInModel { name, .. }
        | M::MissingParameter { name, .. }
        | M::ParameterOutOfDomain { name, .. } => Some(name),
        _ => None,
    }
}

/// Starts the fit of a subject whose observations were read.
fn start_data(
    spec: &FitSpec,
    data: FitData,
) -> Result<(caladrius_fit::FitRun<ModelId>, FitMeta), Box<StartFailure>> {
    let mut options = spec.options.clone();
    let mut notes = data.notes;
    file_duration(
        spec.model,
        data.duration,
        &data.subject,
        &mut options.fixed,
        &spec.initial,
        &mut notes,
    );
    let meta = FitMeta {
        subject: data.subject,
        dose: data.dose,
        n_observations: data.time.len(),
        n_missing: data.n_missing,
        n_blq: data.n_blq,
        covariates: data.covariates,
        n_before_dose: data.n_before_dose,
        notes,
    };
    let input = FitInput {
        model: spec.model,
        dose: data.dose,
        time: data.time,
        conc: data.conc,
        weighting: spec.weighting,
        initial: spec.initial.clone(),
        options,
    };
    match caladrius_fit::start(&input) {
        Ok(run) => Ok((run, meta)),
        Err(e) => Err(Box::new(StartFailure {
            request_level: is_request_error(&e, spec),
            error: CommandError::new("fit_error", e.to_string()),
            meta,
        })),
    }
}

/// The result of a fit that ended, as the project stores it.
pub(crate) fn fit_analysis(meta: FitMeta, result: caladrius_fit::FitResult) -> AnalysisResult {
    AnalysisResult::Fit(fit_run_of(meta, result))
}

fn fit_run_of(meta: FitMeta, result: caladrius_fit::FitResult) -> FitRun {
    FitRun {
        subject: meta.subject,
        dose: Some(meta.dose),
        n_observations: meta.n_observations,
        n_missing: meta.n_missing,
        n_blq: meta.n_blq,
        outcome: Outcome::Ok(result),
        covariates: meta.covariates,
        n_before_dose: meta.n_before_dose,
        notes: meta.notes,
    }
}

/// True when `subject` asks for one fit per subject: the keyword, unless a subject of the worksheet
/// is labelled with it (that subject is then the one meant, as it always was).
pub(crate) fn wants_all_subjects(
    project: &Project,
    worksheet: WorksheetId,
    subject: Option<&str>,
) -> bool {
    subject == Some(ALL_SUBJECTS)
        && project
            .worksheet(worksheet)
            .map(|ws| !ws.subjects().iter().any(|s| s == ALL_SUBJECTS))
            // An unknown worksheet is not a request for every subject: the run says it is unknown.
            .unwrap_or(false)
}

fn run_fit(project: &Project, spec: &FitSpec) -> Result<AnalysisResult, CommandError> {
    if wants_all_subjects(project, spec.worksheet, spec.subject.as_deref()) {
        return run_fit_all(project, spec);
    }
    let (run, meta) = start_fit(project, spec)?;
    match run.run(&mut crate::job::NeverCancel) {
        caladrius_fit::Outcome::Done(result) => Ok(fit_analysis(meta, result)),
        // An observer that never cancels cannot get here; a sentence all the same.
        caladrius_fit::Outcome::Cancelled {
            after_iteration, ..
        } => Err(CommandError::new(
            "fit_cancelled",
            format!("fit cancelled after iteration {after_iteration}; nothing was stored"),
        )),
    }
}

/// One independent fit per subject, with the same model and options (DAT-USE-02). Nothing is shared
/// between the subjects; a subject that cannot be fitted is a result with its error, and the
/// others run.
pub(crate) fn run_fit_all(
    project: &Project,
    spec: &FitSpec,
) -> Result<AnalysisResult, CommandError> {
    let ws = project.worksheet(spec.worksheet)?;
    let ds = ws.dataset();
    let labels = ws.analysis_subjects(&ds);
    if labels.is_empty() {
        return Err(CommandError::new(
            "no_subjects",
            format!(
                "worksheet `{}` has no subject with data; check the subject column",
                ws.name()
            ),
        ));
    }
    // A request that is wrong for every subject (an unknown parameter in `initial`, an option out
    // of its domain) is refused as a whole, as the fit of one subject refuses it, and stores nothing.
    let subjects = labels
        .iter()
        .map(|label| fit_one_of_all(ws, &ds, label, spec))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(AnalysisResult::Fits { subjects })
}

fn fit_one_of_all(
    ws: &Worksheet,
    ds: &Dataset,
    label: &str,
    spec: &FitSpec,
) -> Result<FitRun, CommandError> {
    let failed = |message: String, meta: Option<&FitMeta>| FitRun {
        subject: label.to_owned(),
        dose: meta.map(|m| m.dose),
        n_observations: meta.map_or(0, |m| m.n_observations),
        n_missing: meta.map_or(0, |m| m.n_missing),
        n_blq: meta.map_or(0, |m| m.n_blq),
        outcome: Outcome::Error(message),
        covariates: covariates_of(ds, label),
        n_before_dose: meta.map_or(0, |m| m.n_before_dose),
        notes: meta.map_or_else(Vec::new, |m| m.notes.clone()),
    };
    let data = match fit_data_in(ws, ds, Some(label), spec.dose) {
        Ok(d) => d,
        Err(e) => return Ok(failed(e.message, None)),
    };
    let (run, meta) = match start_data(spec, data) {
        Ok(started) => started,
        Err(failure) if failure.request_level => return Err(failure.error),
        Err(failure) => return Ok(failed(failure.error.message, Some(&failure.meta))),
    };
    Ok(match run.run(&mut crate::job::NeverCancel) {
        caladrius_fit::Outcome::Done(result) => fit_run_of(meta, result),
        caladrius_fit::Outcome::Cancelled {
            after_iteration, ..
        } => failed(
            format!("fit cancelled after iteration {after_iteration}"),
            Some(&meta),
        ),
    })
}

/// The population request of `spec`, with the subjects read from its worksheet, and what was
/// decided about the data: the subjects that the checks refused (left out, each with its sentence)
/// and the notes.
///
/// Each subject is read as `fit.run` reads it (DAT-USE-02: one dose row, the time measured from the
/// dose, rows without a concentration or below the limit of quantification left out), so the
/// request is the inline one of POP-API-01 item 10. A subject listed in `options.exclude_subjects`
/// is left to the population fit to exclude, and is not reported when the checks refuse it.
fn pop_request(
    project: &Project,
    spec: &PopFitSpec,
) -> Result<(PopRequest, Vec<PopRefusedSubject>, Vec<String>), CommandError> {
    let ws = project.worksheet(spec.worksheet)?;
    let ds = ws.dataset();
    let labels = ws.analysis_subjects(&ds);
    if labels.is_empty() {
        return Err(CommandError::new(
            "no_subjects",
            format!(
                "worksheet `{}` has no subject with data; check the subject column",
                ws.name()
            ),
        ));
    }
    let excluded = &spec.options.exclude_subjects;
    let mut subjects: Vec<SubjectInput> = Vec::with_capacity(labels.len());
    let mut refused = Vec::new();
    let mut notes = Vec::new();
    for label in &labels {
        let data = match fit_data_in(ws, &ds, Some(label), None) {
            Ok(d) => d,
            Err(e) => {
                if !excluded.contains(label) {
                    refused.push(PopRefusedSubject {
                        subject: label.clone(),
                        message: e.message,
                    });
                }
                continue;
            }
        };
        let mut subject_notes = data.notes;
        let duration = match data.duration {
            Some(d) if takes_dose_duration(spec.model) => Some(d),
            Some(d) => {
                subject_notes.push(format!(
                    "subject {}: the dose row is an infusion of duration {d}, which model {} does not read from the file; the duration of the dose row is not used",
                    data.subject,
                    spec.model.id()
                ));
                None
            }
            None => None,
        };
        if !excluded.contains(label) {
            notes.extend(subject_notes);
        }
        subjects.push(SubjectInput {
            id: data.subject,
            dose: data.dose,
            time: data.time,
            y: data.conc.into_iter().map(Some).collect(),
            duration,
        });
    }
    let mut options = spec.options.clone();
    // A subject of the worksheet that the checks refused is not in the list, and excluding it would
    // name a subject that is not there; a label that is no subject of the worksheet stays, and the
    // population fit refuses it.
    options
        .exclude_subjects
        .retain(|l| subjects.iter().any(|s| &s.id == l) || !labels.contains(l));
    let request = PopRequest {
        model: Some(spec.model.id().to_owned()),
        initial: spec.initial.clone(),
        random_effects: spec.random_effects.clone(),
        omega: spec.omega.clone(),
        omega_shape: spec.omega_shape.clone(),
        error_model: spec.error_model.clone(),
        error_init: spec.error_init.clone(),
        model_options: Default::default(),
        fixed: spec.fixed.clone(),
        bounds: spec.bounds.clone(),
        dataset_file: None,
        dataset_text: None,
        options,
        subjects: Some(subjects),
    };
    Ok((request, refused, notes))
}

/// The population fit of `spec` on the current state of `project` (`specs/population.md`). A
/// refusal of the fit is an error with the code of POP-INN-03 and stores nothing.
pub(crate) fn run_pop_fit(
    project: &Project,
    spec: &PopFitSpec,
) -> Result<AnalysisResult, CommandError> {
    let (request, refused, notes) = pop_request(project, spec)?;
    match caladrius_fit::pop::run(&request) {
        Ok(answer) => Ok(AnalysisResult::PopFit(Box::new(PopFitRun {
            answer,
            refused_subjects: refused,
            notes,
        }))),
        Err(e) => Err(pop_error(e, &refused)),
    }
}

/// A refusal of the population fit as a command error: its code and its sentence, followed by the
/// subjects the checks of the data left out, since the refusal may come from their absence.
fn pop_error(e: PopError, refused: &[PopRefusedSubject]) -> CommandError {
    let mut message = e.message;
    if !refused.is_empty() {
        let list: Vec<String> = refused
            .iter()
            .map(|r| format!("subject {}: {}", r.subject, r.message))
            .collect();
        message = format!(
            "{} (the checks of the data left out {} subject(s): {})",
            message.trim_end_matches('.'),
            refused.len(),
            list.join("; ")
        );
    }
    CommandError::new(e.code.id(), message)
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
