#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unreachable
)]
//! Layer L0: non-compartmental analysis (AUC rules, lambda_z, extrapolation, AUMC, MRT, CL/F, Vz/F, Vss).
//!
//! Behaviour: `specs/nca.md`. Rule ids (`NCA-DAT-02`...) are cited in the code and the tests.
//! Parameters are looked up by their PKNCA names with [`NcaResult::get`].
//!
//! Implemented: input validation, data cleaning, C0, Cmax, Tmax, Tlag, Tfirst, Tlast, Clast, AUClast,
//! AUCall, AUMClast, AUMCall, the terminal phase (λz, R², t½, Clast,pred, span ratio) and the
//! extrapolation to infinity (AUCinf, AUMCinf, % extrapolated), MRT, CL, Vz, Vss and
//! dose-normalised values, plain Vss and PKNCA's IV bolus areas (`auciv*`, `aucivpbext*`). A
//! missing or invalid dose only makes the dose-dependent parameters not calculated (NCA-DAT-10).

mod auc;
mod auciv;
mod clean;
mod derived;
mod error;
mod extrapolation;
mod flags;
mod float;
mod lambda_z;
mod observed;
mod options;
mod result;
mod units;
mod validate;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_edge;
#[cfg(test)]
mod tests_lambda_z;

use serde::{Deserialize, Serialize};

pub use clean::{PointOrigin, ProfilePoint, RemovalReason, RemovedPoint};
pub use error::NcaError;
pub use flags::QualityFlag;
pub use lambda_z::LambdaZCandidate;
pub use options::{
    AucMethod, BlqAction, BlqPolicy, LambdaZManual, LambdaZOptions, LambdaZSelection,
    LambdaZTieRule, MissingPolicy, NcaOptions, NegativePolicy, QualityThresholds, Route,
    StartPolicy, TmaxTie,
};
pub use result::{NcReason, NcaResult, ParamValue, Parameter};
pub use units::Units;

use clean::DOSE_TIME;

/// One concentration-time profile (one subject, one analyte, one single dose given at time 0).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NcaInput {
    /// Sampling times, strictly increasing, relative to the dose. A missing time (NaN) is refused.
    #[serde(
        serialize_with = "crate::float::ser_vec",
        deserialize_with = "crate::float::de_vec"
    )]
    pub time: Vec<f64>,
    /// Concentrations, one per time, same unit for the whole profile. NaN means missing.
    #[serde(
        serialize_with = "crate::float::ser_vec",
        deserialize_with = "crate::float::de_vec"
    )]
    pub conc: Vec<f64>,
    /// Dose given at time 0. NaN (JSON `null`) means missing; a missing, non-finite or
    /// non-positive dose makes only the dose-dependent parameters not calculated (NCA-DAT-10).
    #[serde(
        serialize_with = "crate::float::ser",
        deserialize_with = "crate::float::de"
    )]
    pub dose: f64,
    /// Route of administration.
    pub route: Route,
    /// Conventions of the analysis.
    pub options: NcaOptions,
}

/// Runs the NCA of one profile. Invalid input gives an [`NcaError`] whose message says what to fix;
/// every parameter that cannot be computed from valid input is "not calculated" with a reason.
pub fn run(input: &NcaInput) -> Result<NcaResult, NcaError> {
    validate::validate(input)?;
    let options = &input.options;
    let cleaned = clean::clean(input);
    let obs = observed::observed(&cleaned.points, options.tmax_tie);
    let c0 = observed::c0(input.route, &cleaned.points);

    // NCA-DAT-08: the concentration at the dose time. A sample there is used as it is.
    let mut profile = cleaned.points;
    let starts_at_dose = profile.first().is_some_and(|p| p.time == DOSE_TIME);
    let start_value = if starts_at_dose || profile.is_empty() {
        None
    } else {
        match options.start {
            StartPolicy::None => None,
            StartPolicy::Zero => Some(0.0),
            StartPolicy::C0 => c0.value(),
        }
    };
    if let Some(conc) = start_value {
        profile.insert(
            0,
            ProfilePoint {
                index: None,
                time: DOSE_TIME,
                conc,
                origin: PointOrigin::InsertedStart,
            },
        );
    }
    let start_missing = !starts_at_dose && start_value.is_none();
    let areas = auc::areas(
        &profile,
        options.auc_method,
        start_missing,
        obs.tmax.value(),
    );

    // Section 6: the terminal phase, then section 7: extrapolation to infinity.
    let terminal = lambda_z::terminal(&profile, input.route, options, obs.tmax.value())?;
    let fit = |f: fn(&LambdaZCandidate) -> f64| match &terminal.selected {
        Ok(c) => ParamValue::of(f(c)),
        Err(reason) => ParamValue::nc(*reason),
    };
    let lambda_z = fit(|c| c.lambda_z);
    let half_life = match &terminal.selected {
        Ok(c) => c
            .half_life()
            .map_or(ParamValue::nc(NcReason::NonFinite), ParamValue::of),
        Err(reason) => ParamValue::nc(*reason),
    };
    let span_ratio =
        fit(|c| c.time_last - c.time_first).zip(half_life, |s, h| ParamValue::of(s / h));
    let adj_r_squared = match &terminal.selected {
        Ok(c) => c
            .adj_r_squared
            .map_or(ParamValue::nc(NcReason::Undefined), ParamValue::of),
        Err(reason) => ParamValue::nc(*reason),
    };
    let r_squared = match &terminal.selected {
        Ok(c) => c
            .r_squared
            .map_or(ParamValue::nc(NcReason::Undefined), ParamValue::of),
        Err(reason) => ParamValue::nc(*reason),
    };
    // NCA-LZ-01: Clast,pred = exp(a − λz·Tlast), at the observed Tlast.
    let clast_pred = match &terminal.selected {
        Ok(c) => obs.tlast.zip(lambda_z, |t, lz| {
            ParamValue::of((c.intercept - lz * t).exp())
        }),
        Err(reason) => ParamValue::nc(*reason),
    };
    let ext = extrapolation::extrapolate(
        lambda_z,
        areas.auclast,
        areas.aumclast,
        obs.clast,
        clast_pred,
        areas.end_time,
    );

    // NCA-EXT-04 to 07, NCA-OBS-05, with the dose of NCA-DAT-10.
    let derived = derived::derived(
        input.route,
        &derived::Inputs {
            dose: derived::dose_value(input.dose),
            lambda_z,
            cmax: obs.cmax,
            clast: obs.clast,
            auclast: areas.auclast,
            aucall: areas.aucall,
            aumclast: areas.aumclast,
            aumcall: areas.aumcall,
            aucinf_obs: ext.aucinf_obs,
            aucinf_pred: ext.aucinf_pred,
            aumcinf_obs: ext.aumcinf_obs,
            aumcinf_pred: ext.aumcinf_pred,
        },
    );

    // NCA-IV-02, IV-03: PKNCA's IV bolus areas and percent back-extrapolated.
    let iv_areas = auciv::iv_areas(
        input.route,
        &profile,
        c0,
        &auciv::Areas {
            auclast: areas.auclast,
            aucall: areas.aucall,
            aucinf_obs: ext.aucinf_obs,
            aucinf_pred: ext.aucinf_pred,
        },
    );
    let tlag = observed::tlag(input.route, &cleaned.before_blq);

    // NCA-IV-01: C0 is reported for an IV bolus only.
    let c0_reported = match input.route {
        Route::IvBolus => c0,
        Route::Extravascular | Route::IvInfusion { .. } => {
            ParamValue::nc(NcReason::NotApplicableToRoute)
        }
    };
    let parameters = [
        ("c0", c0_reported),
        ("cmax", obs.cmax),
        ("tmax", obs.tmax),
        ("tfirst", obs.tfirst),
        ("tlag", tlag),
        ("tlast", obs.tlast),
        ("clast.obs", obs.clast),
        ("auclast", areas.auclast),
        ("aucall", areas.aucall),
        ("aumclast", areas.aumclast),
        ("aumcall", areas.aumcall),
        ("lambda.z", lambda_z),
        ("r.squared", r_squared),
        ("adj.r.squared", adj_r_squared),
        ("lambda.z.time.first", fit(|c| c.time_first)),
        ("lambda.z.time.last", fit(|c| c.time_last)),
        ("lambda.z.n.points", fit(|c| c.n_points as f64)),
        ("clast.pred", clast_pred),
        ("half.life", half_life),
        ("span.ratio", span_ratio),
        ("aucinf.obs", ext.aucinf_obs),
        ("aucinf.pred", ext.aucinf_pred),
        ("aumcinf.obs", ext.aumcinf_obs),
        ("aumcinf.pred", ext.aumcinf_pred),
        ("aucpext.obs", ext.aucpext_obs),
        ("aucpext.pred", ext.aucpext_pred),
        ("aumcpext.obs", ext.aumcpext_obs),
        ("aumcpext.pred", ext.aumcpext_pred),
    ]
    .into_iter()
    .chain(derived)
    .chain(iv_areas)
    .map(|(name, value)| Parameter {
        name: name.to_string(),
        value,
        unit: None,
    })
    .collect::<Vec<_>>();
    // NCA-UNIT-01: units of the results; CL and volumes converted to litres (validated above).
    let checked = options.units.as_ref().and_then(|u| units::check(u).ok());
    let parameters = match &checked {
        None => parameters,
        Some(units) => parameters
            .into_iter()
            .map(|mut p| {
                if units::VOLUME_PARAMETERS.contains(&p.name.as_str()) {
                    if let ParamValue::Value(x) = p.value {
                        p.value = ParamValue::of(x * units.volume_factor);
                    }
                }
                p.unit = units.unit_of(&p.name);
                p
            })
            .collect(),
    };
    // NCA-LZ-12b: flags, computed from the final numbers and never changing them.
    let flags = flags::flags(
        &options.quality,
        &flags::Inputs {
            selected: terminal.selected.as_ref().ok(),
            adj_r_squared,
            span_ratio,
            aucpext_obs: ext.aucpext_obs,
            aucpext_pred: ext.aucpext_pred,
            area_end: areas.end_time,
            tlast: obs.tlast,
        },
    );
    Ok(NcaResult::new(
        parameters,
        profile,
        cleaned.removed,
        terminal.candidates,
        flags,
        checked.is_none(),
    ))
}
