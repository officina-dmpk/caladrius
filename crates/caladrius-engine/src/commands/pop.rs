//! `pop.fit`: the population fit (`specs/population.md`, FOCE with interaction by default) of a model
//! to every subject of a worksheet.

use std::collections::BTreeMap;

use caladrius_fit::Bounds;
use caladrius_fit::pop::PopOptions;
use caladrius_models::ModelId;
use caladrius_project::{
    AnalysisId, AnalysisResult, AnalysisSpec, PopFitSpec, Project, WorksheetId,
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::analysis::{check_target, store};
use super::{CommandDef, parse, respond};
use crate::Engine;
use crate::error::CommandError;
use crate::run;
use crate::schema::{
    array_of, described, nullable, number, object, one_of_strings, pop_fit_result, reference, root,
    string,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PopFitParams {
    #[serde(default)]
    analysis: Option<AnalysisId>,
    #[serde(default)]
    name: Option<String>,
    worksheet: WorksheetId,
    model: ModelId,
    #[serde(default)]
    initial: BTreeMap<String, f64>,
    #[serde(default)]
    random_effects: Option<Vec<String>>,
    #[serde(default)]
    omega: Option<Vec<Vec<f64>>>,
    #[serde(default)]
    omega_shape: Option<String>,
    #[serde(default)]
    error_model: Option<String>,
    #[serde(default)]
    error_init: Option<Vec<f64>>,
    #[serde(default)]
    fixed: BTreeMap<String, f64>,
    #[serde(default)]
    bounds: BTreeMap<String, Bounds>,
    #[serde(default)]
    options: PopOptions,
}

/// `execute("pop.fit")`: reads the subjects, fits, stores the analysis and answers with its result.
/// Synchronous: no job, no progress, no cancel.
fn pop_fit(engine: &mut Engine, params: Value) -> Result<Value, CommandError> {
    let p: PopFitParams = parse("pop.fit", params)?;
    check_target(&engine.project, p.analysis, "pop_fit")?;
    let spec = PopFitSpec {
        worksheet: p.worksheet,
        model: p.model,
        initial: p.initial,
        random_effects: p.random_effects,
        omega: p.omega,
        omega_shape: p.omega_shape,
        error_model: p.error_model,
        error_init: p.error_init,
        fixed: p.fixed,
        bounds: p.bounds,
        options: p.options,
    };
    let result = run::run_pop_fit(&engine.project, &spec)?;
    let id = store(
        &mut engine.project,
        p.analysis,
        p.name.as_deref(),
        AnalysisSpec::PopFit(spec),
        result,
    )?;
    answer(&engine.project, id)
}

/// The stored result, its fields at the top level, with the id of the analysis (`analysis`).
fn answer(project: &Project, id: AnalysisId) -> Result<Value, CommandError> {
    let Some(AnalysisResult::PopFit(run)) = project.analysis(id)?.result() else {
        return Err(CommandError::new(
            "internal_error",
            format!("analysis {id} holds no population fit after the fit was stored"),
        ));
    };
    let mut value = respond(run)?;
    match value.as_object_mut() {
        Some(map) => {
            map.insert("analysis".to_owned(), json!(id));
            Ok(value)
        }
        None => Err(CommandError::new(
            "internal_error",
            "the result of the population fit is not an object",
        )),
    }
}

pub(crate) const POP_FIT: CommandDef = CommandDef {
    id: "pop.fit",
    title: "Fit a population model",
    description: "Population fit of one model to every subject of a worksheet, by FOCE with interaction by default. The subjects are read as `fit.run` reads them (one dose row each, the time measured from the dose, rows without a concentration or below the limit of quantification left out); a subject that the data check refused is left out of the fit and listed in `refused_subjects` with the sentence of its check, and the others are fitted. The other parameters are those of the population request: `model`; `random_effects`, the parameters that carry a log-normal random effect, in the order of the rows of omega; `omega` (q by q) and `omega_shape` (diagonal or block); `error_model` (additive, proportional, combined or combined_sd; required, there is no default) and `error_init` in the units of the data ([a], [b] or [a, b]); `initial`, the starting typical values; `fixed` (a typical value, `a`, `b`, or `omega.<parameter>` held at a value); `bounds`; and `options` (`method` foce_i, foce or fo; `ofv_convention`; `covariance`, the kind printed in `se`; `exclude_subjects`; `evaluate_only`, the objective and the diagnostics at the given parameters with no iteration). A typical value with a random effect left out of `initial`, or an `omega` left out, comes from a two-stage start (one fit per subject; `start` says where each starting value came from). The answer has the status, the OFV, AIC and BIC, `estimate` (each typical value by name, `omega_diagonal`, `omega_full`, `omega_sd`, `omega_correlation`, `xi`) and `se` with the same keys (`se_by_kind` has the sandwich, r and s kinds), the covariance block, the trace, the eta- and epsilon-shrinkage, and per subject its OFV, its conditional mode `eta_hat` and its `observations` (time, y, PRED, IPRED, IWRES), with the id of the analysis in `analysis`; the result is stored in an analysis object, as `fit.run` stores one. A refusal is an error with the code of the population fit (`invalid_option`, `too_few_subjects`, `too_few_observations`, `non_positive_sigma`, `unbounded_likelihood`, `unsupported_history`, `inner_failed`) and a sentence that says what to change; it stores nothing. It is not a job: it runs in one step, with no progress and no cancel.",
    mutates: true,
    params: || {
        let bound = object(
            vec![("lower", nullable(number())), ("upper", nullable(number()))],
            &[],
        );
        root(
            "pop.fit parameters",
            object(
                vec![
                    (
                        "analysis",
                        described(
                            reference("Id"),
                            "Update and re-run this population fit instead of creating a new analysis.",
                        ),
                    ),
                    ("name", string()),
                    ("worksheet", reference("Id")),
                    ("model", reference("ModelId")),
                    (
                        "initial",
                        described(
                            json!({ "type": "object", "additionalProperties": number() }),
                            "Starting typical values by parameter name. A parameter with a random effect that is left out starts at the two-stage value.",
                        ),
                    ),
                    (
                        "random_effects",
                        described(
                            array_of(string()),
                            "The parameters that carry a random effect, in the order of the rows of omega.",
                        ),
                    ),
                    (
                        "omega",
                        described(
                            nullable(array_of(array_of(number()))),
                            "The starting covariance matrix of the random effects, q by q, in the order of random_effects. Omitted or null: the two-stage start.",
                        ),
                    ),
                    (
                        "omega_shape",
                        nullable(one_of_strings(&["diagonal", "block"])),
                    ),
                    (
                        "error_model",
                        one_of_strings(&["additive", "proportional", "combined", "combined_sd"]),
                    ),
                    (
                        "error_init",
                        described(
                            nullable(array_of(number())),
                            "The starting error parameters in the units of the data: [a] (additive), [b] (proportional) or [a, b]. Omitted or null: computed from the data.",
                        ),
                    ),
                    (
                        "fixed",
                        described(
                            json!({ "type": "object", "additionalProperties": number() }),
                            "Values held during the fit: a typical value by name, the error parameters a and b, a variance of omega as omega.<parameter>.",
                        ),
                    ),
                    (
                        "bounds",
                        json!({ "type": "object", "additionalProperties": bound }),
                    ),
                    ("options", reference("PopOptions")),
                ],
                &["worksheet", "model", "random_effects", "error_model"],
            ),
        )
    },
    result: || {
        root(
            "pop.fit result",
            pop_fit_result(vec![("analysis", reference("Id"))], &["analysis"]),
        )
    },
    // The examples run on the one-subject worksheet of `data.import`: a fit needs two subjects at
    // least, so the example evaluates the objective and the diagnostics at the given parameters.
    example: || {
        json!({
            "worksheet": 1,
            "model": "pk1.oral_1",
            "initial": { "v": 30.0, "cl": 3.0, "ka": 1.5 },
            "random_effects": ["cl", "v"],
            "omega": [[0.09, 0.0], [0.0, 0.04]],
            "error_model": "additive",
            "error_init": [0.2],
            "options": { "evaluate_only": true },
        })
    },
    run: pop_fit,
};
