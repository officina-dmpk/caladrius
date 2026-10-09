//! Partial derivatives of the concentration with respect to the parameters (`specs/fit.md`
//! FIT-JAC-01, JAC-02), for the fitting crate and for tests.
//!
//! Forward differences are the default of the fit. Analytic derivatives: the one-compartment IV
//! bolus (FIT-JAC-02) and every two-compartment model in every parameter set (MOD-2C-17 to 19);
//! for the other one-compartment models `Analytic` falls back to forward differences with the
//! default increment (their closed forms live in `caladrius-fit`), and the result says which
//! method was used. For an input with a dosing regimen `Analytic` is refused (no closed form is
//! derived yet) and forward differences give one column per model parameter, none for the
//! regimen.

use serde::{Deserialize, Serialize};

use crate::{ModelError, ModelId, ModelInput, run};

/// Default relative increment of the forward differences (FIT-JAC-01).
pub const DEFAULT_INCREMENT: f64 = 0.001;

/// How the derivatives are formed.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Derivatives {
    /// [f(θ + Δ·e_j) − f(θ)]/Δ with Δ = increment·|θ_j|, or the increment itself when θ_j = 0.
    ForwardDifference {
        /// Relative increment, finite and > 0.
        #[serde(
            serialize_with = "crate::float::ser",
            deserialize_with = "crate::float::de"
        )]
        increment: f64,
    },
    /// Closed form where the specs give it (one-compartment IV bolus, every two-compartment
    /// model), else forward differences with [`DEFAULT_INCREMENT`].
    Analytic,
}

/// One column per parameter, one row per time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Jacobian {
    /// Parameter names, in the order of the columns (the order of `ModelInput::params`).
    pub parameters: Vec<String>,
    /// ∂C(t_i)/∂θ_j: `columns[j][i]`.
    pub columns: Vec<Vec<f64>>,
    /// The method actually used.
    pub method: Derivatives,
}

/// The derivatives of the concentrations of `input` with respect to each of its parameters.
pub fn jacobian(input: &ModelInput, method: Derivatives) -> Result<Jacobian, ModelError> {
    let base = run(input)?;
    // MOD-MD: no closed form is derived for a regimen yet (card T-049); differences work.
    if input.has_regimen() && method == Derivatives::Analytic {
        return Err(ModelError::DerivativesUnavailable {
            model: input.model.id().to_string(),
            reason: "they are not derived for a dosing regimen (`tau`, `n_doses` or a schedule); use forward differences".to_string(),
        });
    }
    match method {
        Derivatives::Analytic if input.model == ModelId::IvBolus => Ok(bolus(input, base.conc())),
        Derivatives::Analytic if input.model.compartments() == 2 => crate::two::jacobian(input),
        Derivatives::Analytic => forward(input, base.conc(), DEFAULT_INCREMENT),
        Derivatives::ForwardDifference { increment } => {
            if !(increment.is_finite() && increment > 0.0) {
                return Err(ModelError::ParameterOutOfDomain {
                    name: "increment".to_string(),
                    value: increment,
                    domain: "a finite number > 0".to_string(),
                });
            }
            forward(input, base.conc(), increment)
        }
    }
}

fn forward(input: &ModelInput, base: &[f64], h: f64) -> Result<Jacobian, ModelError> {
    let mut parameters = Vec::new();
    let mut columns = Vec::new();
    // The regimen (dose times, amounts, interval) is data, not a fitted parameter: no column.
    for (name, &theta) in input
        .params
        .iter()
        .filter(|(name, _)| !crate::is_regimen_parameter(name))
    {
        let step = if theta == 0.0 { h } else { h * theta.abs() };
        if !(theta + step).is_finite() {
            return Err(ModelError::Overflow {
                what: format!(
                    "the forward-difference step of parameter `{name}` ({theta} + {step})"
                ),
            });
        }
        let mut moved = input.clone();
        moved.params.insert(name.clone(), theta + step);
        // The base point is valid, so a failure here comes from the step itself.
        let shifted = run(&moved).map_err(|e| ModelError::Overflow {
            what: format!(
                "the forward-difference step of parameter `{name}` (to {}: {e})",
                theta + step
            ),
        })?;
        columns.push(
            shifted
                .conc()
                .iter()
                .zip(base)
                .map(|(up, at)| (up - at) / step)
                .collect(),
        );
        parameters.push(name.clone());
    }
    Ok(Jacobian {
        parameters,
        columns,
        method: Derivatives::ForwardDifference { increment: h },
    })
}

/// FIT-JAC-02 for C = (D/V)·e^(−k·t): ∂C/∂V = −C/V and ∂C/∂k = −t·C with (V, k); with (V, CL),
/// k = CL/V gives ∂C/∂CL = −t·C/V and ∂C/∂V = C·(−1/V + CL·t/V²). Zero before the dose.
fn bolus(input: &ModelInput, conc: &[f64]) -> Jacobian {
    let v = input.params.get("v").copied().unwrap_or(f64::NAN);
    let cl = input.params.get("cl").copied();
    let mut parameters = Vec::new();
    let mut columns = Vec::new();
    for name in input.params.keys() {
        let column = input
            .times
            .iter()
            .zip(conc)
            .map(|(&t, &c)| match (name.as_str(), cl) {
                (_, _) if t < 0.0 => 0.0,
                ("v", None) => -c / v,
                ("v", Some(cl)) => c * (-1.0 / v + cl * t / (v * v)),
                ("k", _) => -t * c,
                ("cl", _) => -t * c / v,
                _ => 0.0,
            })
            .collect();
        parameters.push(name.clone());
        columns.push(column);
    }
    Jacobian {
        parameters,
        columns,
        method: Derivatives::Analytic,
    }
}
