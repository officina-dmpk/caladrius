#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unreachable
)]
//! Layer L0: closed-form pharmacokinetic model solutions (bolus, infusion, first- and zero-order absorption, lag time).
//!
//! Behaviour: `specs/models.md` (rule ids `MOD-…` are cited in the code and the tests). One input
//! value, one [`run`], results by name, like `caladrius-nca`.

mod curves;
mod error;
mod float;
mod jacobian;
mod model;
mod params;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub use error::ModelError;
pub use jacobian::{DEFAULT_INCREMENT, Derivatives, Jacobian, jacobian};
pub use model::ModelId;

use model::Input;

/// One model evaluation: the model, the dose, the parameters and the time grid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelInput {
    /// Which model.
    pub model: ModelId,
    /// Effective dose F × D (MOD-GEN-03), finite and >= 0; 0 gives 0 everywhere.
    pub dose: f64,
    /// Parameters by the names of MOD-VOC-01: `v`; exactly one of `cl` and `k`; `ka`
    /// (first-order input); `dur` (infusion, zero-order input); `tlag` (lag models).
    pub params: BTreeMap<String, f64>,
    /// Times since the dose, in any order, finite; a time before the dose (or the lag) gives 0.
    pub times: Vec<f64>,
}

/// Concentrations and AUC on the grid, and the secondary parameters of the model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelOutput {
    conc: Vec<f64>,
    auc: Vec<f64>,
    secondary: BTreeMap<String, f64>,
}

impl ModelOutput {
    /// Concentration at each input time, in the order of `times`.
    pub fn conc(&self) -> &[f64] {
        &self.conc
    }

    /// AUC from the dose time (not from the lag) to each input time; 0 for t <= 0.
    pub fn auc(&self) -> &[f64] {
        &self.auc
    }

    /// Secondary parameter by name (MOD-SEC-02, MOD-VOC-01): `v`, `k`, `cl`, `half_life`,
    /// `auc_inf`, `mrt_system` (1/k, the disposition MRT), `mrt` (profile MRT, MOD-SEC-02), `vss`
    /// (= V for one compartment, MOD-IVB-02), and the input parameters
    /// (`ka`, `dur`, `tlag`, `rate`); bolus: `c0`; models with a peak: `tmax_pred`, `cmax_pred`.
    /// `None` when the model has no such quantity.
    pub fn get(&self, name: &str) -> Option<f64> {
        self.secondary.get(name).copied()
    }

    /// Every secondary parameter, by name.
    pub fn secondary(&self) -> &BTreeMap<String, f64> {
        &self.secondary
    }
}

/// Evaluates `input`. Parameters and times are checked first (MOD-GEN-04); the message of the
/// error names what to fix.
pub fn run(input: &ModelInput) -> Result<ModelOutput, ModelError> {
    let p = params::resolve(input.model, &input.params)?;
    let dose = input.dose;
    if !(dose.is_finite() && dose >= 0.0) {
        return Err(ModelError::InvalidDose { value: dose });
    }
    if let Some(index) = input.times.iter().position(|t| !t.is_finite()) {
        return Err(ModelError::NonFiniteTime { index });
    }
    let kind = input.model.input();
    let (conc, auc): (Vec<f64>, Vec<f64>) = input
        .times
        .iter()
        .map(|&t| curves::at(kind, dose, &p, t))
        .unzip();

    let mut secondary = BTreeMap::new();
    let mut put = |name: &str, value: f64| {
        secondary.insert(name.to_string(), value);
    };
    put("v", p.v);
    put("k", p.k);
    put("cl", p.cl);
    put("half_life", std::f64::consts::LN_2 / p.k);
    put("auc_inf", dose / p.cl);
    put("mrt_system", 1.0 / p.k);
    put("vss", p.v);
    if input.model.has_lag() {
        put("tlag", p.tlag);
    }
    match kind {
        Input::Bolus => {
            put("c0", dose / p.v);
            put("mrt", 1.0 / p.k);
        }
        Input::ZeroOrder => {
            put("dur", p.dur);
            put("rate", dose / p.dur);
            put("tmax_pred", p.tlag + p.dur);
            put(
                "cmax_pred",
                dose / (p.dur * p.cl) * -(-p.k * p.dur).exp_m1(),
            );
            put("mrt", 1.0 / p.k + p.dur / 2.0 + p.tlag);
        }
        Input::FirstOrder => {
            let peak = curves::first_order_peak_time(&p);
            put("ka", p.ka);
            put("tmax_pred", p.tlag + peak);
            // MOD-AB1-04: Cmax = (D/V)·e^(−k·Tmax), exact at the peak since ka·e^−ka·T = k·e^−k·T.
            put("cmax_pred", dose / p.v * (-p.k * peak).exp());
            put("mrt", 1.0 / p.k + 1.0 / p.ka + p.tlag);
        }
    }
    // Golden rule 6: valid parameters whose results overflow are an error, never inf or NaN.
    let overflow = |what: String| Err(ModelError::Overflow { what });
    for (t, (c, a)) in input.times.iter().zip(conc.iter().zip(&auc)) {
        if !c.is_finite() {
            return overflow(format!("the concentration at time {t}"));
        }
        if !a.is_finite() {
            return overflow(format!("the AUC at time {t}"));
        }
    }
    if let Some((name, _)) = secondary.iter().find(|(_, v)| !v.is_finite()) {
        return overflow(format!("the secondary parameter `{name}`"));
    }
    Ok(ModelOutput {
        conc,
        auc,
        secondary,
    })
}

#[cfg(test)]
mod tests;
