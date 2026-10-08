//! What the fit needs from a model: predictions and, optionally, closed-form partial derivatives.
//! Implemented for the one-compartment models of `caladrius-models`.

use std::collections::BTreeMap;

use caladrius_models::{ModelError, ModelId, ModelInput};

use crate::closed_forms;

/// A model the fit can drive.
pub trait FitModel {
    /// Concentrations at `times` for `params` and `dose`.
    fn predict(
        &self,
        dose: f64,
        params: &BTreeMap<String, f64>,
        times: &[f64],
    ) -> Result<Vec<f64>, ModelError>;

    /// ∂C(t_i)/∂θ_j for the parameters `names`, one column per name, when a closed form exists
    /// for this model and parameterisation (FIT-JAC-02); `None` otherwise.
    fn analytic_derivatives(
        &self,
        dose: f64,
        params: &BTreeMap<String, f64>,
        times: &[f64],
        names: &[String],
    ) -> Option<Vec<Vec<f64>>>;

    /// Secondary parameters of the model at `params` (FIT-OUT-06), each with its gradient by
    /// parameter name for the delta method. None by default.
    fn secondary(&self, dose: f64, params: &BTreeMap<String, f64>) -> Vec<Secondary> {
        let _ = (dose, params);
        Vec::new()
    }
}

/// A derived quantity g(θ) and ∂g/∂θ by parameter name (parameters not named have 0).
#[derive(Debug, Clone, PartialEq)]
pub struct Secondary {
    /// Name, e.g. `cl`.
    pub name: String,
    /// Value at the estimates.
    pub value: f64,
    /// Partial derivatives by parameter name.
    pub gradient: BTreeMap<String, f64>,
}

impl FitModel for ModelId {
    /// One-compartment relations (MOD-SEC-01, SEC-02) in the (v, k) parameterisation: CL = V·k,
    /// t½ = ln 2/k, AUC∞ = D/(V·k). None when `v` or `k` is not a parameter.
    fn secondary(&self, dose: f64, params: &BTreeMap<String, f64>) -> Vec<Secondary> {
        let (Some(&v), Some(&k)) = (params.get("v"), params.get("k")) else {
            return Vec::new();
        };
        let ln2 = std::f64::consts::LN_2;
        let make = |name: &str, value: f64, d_v: f64, d_k: f64| Secondary {
            name: name.to_string(),
            value,
            gradient: BTreeMap::from([("v".to_string(), d_v), ("k".to_string(), d_k)]),
        };
        vec![
            make("cl", v * k, k, v),
            make("half_life", ln2 / k, 0.0, -ln2 / (k * k)),
            make(
                "auc_inf",
                dose / (v * k),
                -dose / (v * v * k),
                -dose / (v * k * k),
            ),
        ]
    }

    fn predict(
        &self,
        dose: f64,
        params: &BTreeMap<String, f64>,
        times: &[f64],
    ) -> Result<Vec<f64>, ModelError> {
        let output = caladrius_models::run(&ModelInput {
            model: *self,
            dose,
            params: params.clone(),
            times: times.to_vec(),
        })?;
        Ok(output.conc().to_vec())
    }

    fn analytic_derivatives(
        &self,
        dose: f64,
        params: &BTreeMap<String, f64>,
        times: &[f64],
        names: &[String],
    ) -> Option<Vec<Vec<f64>>> {
        let get = |n: &str| params.get(n).copied();
        let keys: Vec<&str> = params.keys().map(String::as_str).collect();
        match (self, keys.as_slice()) {
            (ModelId::IvBolus, ["k", "v"]) => {
                let (v, k) = (get("v")?, get("k")?);
                let conc = |t: f64| {
                    if t < 0.0 {
                        0.0
                    } else {
                        dose / v * (-k * t).exp()
                    }
                };
                names
                    .iter()
                    .map(|name| match name.as_str() {
                        // FIT-JAC-02: ∂C/∂V = −C/V, ∂C/∂k = −t·C.
                        "v" => Some(times.iter().map(|&t| -conc(t) / v).collect()),
                        "k" => Some(times.iter().map(|&t| -t * conc(t)).collect()),
                        _ => None,
                    })
                    .collect()
            }
            (ModelId::Oral1, ["k", "ka", "v"]) => {
                closed_forms::first_order_columns(dose, params, times, names, false)
            }
            (ModelId::IvInfusion | ModelId::Oral0, ["dur", "k", "v"]) => {
                closed_forms::zero_order_columns(dose, params, times, names, false)
            }
            (ModelId::Oral0Lag, ["dur", "k", "tlag", "v"]) => {
                closed_forms::zero_order_columns(dose, params, times, names, true)
            }
            (ModelId::Oral1Lag, ["k", "ka", "tlag", "v"]) => {
                closed_forms::first_order_columns(dose, params, times, names, true)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The closed forms agree with central differences of the model itself.
    #[test]
    fn oral_derivatives_match_central_differences() {
        let names: Vec<String> = ["v", "k", "ka"].iter().map(|s| s.to_string()).collect();
        let times = [0.25, 1.0, 3.0, 12.0];
        for (v, k, ka) in [(10.0, 0.2, 1.0), (10.0, 0.2, 0.2 + 1e-7), (2.0, 1.0, 0.2)] {
            let params: BTreeMap<String, f64> = [("v", v), ("k", k), ("ka", ka)]
                .iter()
                .map(|(n, x)| (n.to_string(), *x))
                .collect();
            let exact = ModelId::Oral1
                .analytic_derivatives(100.0, &params, &times, &names)
                .unwrap();
            for (j, name) in names.iter().enumerate() {
                let h = 1e-6 * params[name];
                let mut up = params.clone();
                let mut down = params.clone();
                up.insert(name.clone(), params[name] + h);
                down.insert(name.clone(), params[name] - h);
                let fu = ModelId::Oral1.predict(100.0, &up, &times).unwrap();
                let fd = ModelId::Oral1.predict(100.0, &down, &times).unwrap();
                for i in 0..times.len() {
                    let numeric = (fu[i] - fd[i]) / (2.0 * h);
                    let a = exact[j][i];
                    assert!(
                        (a - numeric).abs() <= 1e-6 * (1.0 + a.abs()),
                        "{name} at t = {}: {a} against {numeric}",
                        times[i]
                    );
                }
            }
        }
    }
}
