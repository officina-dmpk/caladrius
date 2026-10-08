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
//! Implemented: input validation, data cleaning, C0, Cmax, Tmax, Tfirst, Tlast, Clast, AUClast,
//! AUCall, AUMClast, AUMCall. The terminal phase (λz) and everything derived from it come later;
//! until then `get` returns `None` for those names.

mod auc;
mod clean;
mod error;
mod float;
mod observed;
mod options;
mod result;
mod validate;

#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};

pub use clean::{PointOrigin, ProfilePoint, RemovalReason, RemovedPoint};
pub use error::NcaError;
pub use options::{
    AucMethod, BlqAction, BlqPolicy, LambdaZOptions, MissingPolicy, NcaOptions, NegativePolicy,
    Route, StartPolicy, TmaxTie,
};
pub use result::{NcReason, NcaResult, ParamValue, Parameter};

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
    /// Dose given at time 0, > 0.
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
        obs.tlast.value(),
    );

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
        ("tlast", obs.tlast),
        ("clast.obs", obs.clast),
        ("auclast", areas.auclast),
        ("aucall", areas.aucall),
        ("aumclast", areas.aumclast),
        ("aumcall", areas.aumcall),
    ]
    .into_iter()
    .map(|(name, value)| Parameter {
        name: name.to_string(),
        value,
    })
    .collect();
    Ok(NcaResult::new(parameters, profile, cleaned.removed))
}
