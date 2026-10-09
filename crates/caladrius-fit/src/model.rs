//! What the fit needs from a model: predictions and, optionally, closed-form partial derivatives.
//! Implemented for the one- and two-compartment models of `caladrius-models`. The closed forms of
//! the one-compartment models are in `closed_forms` (by model id and parameterisation); those of
//! the two-compartment models, in every parameter set, come from `caladrius_models::jacobian`
//! (`specs/models.md` MOD-2C-17 to 19).

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
            (model, _) if model.compartments() == 2 => {
                two_compartment_columns(*model, dose, params, times, names)
            }
            _ => None,
        }
    }
}

/// The closed-form columns of a two-compartment model for the fitted `names` (fixed parameters
/// are values in `params`, not columns); `None` when the model refuses the parameters or a name has
/// no column.
fn two_compartment_columns(
    model: ModelId,
    dose: f64,
    params: &BTreeMap<String, f64>,
    times: &[f64],
    names: &[String],
) -> Option<Vec<Vec<f64>>> {
    let jacobian = caladrius_models::jacobian(
        &ModelInput {
            model,
            dose,
            params: params.clone(),
            times: times.to_vec(),
        },
        caladrius_models::Derivatives::Analytic,
    )
    .ok()?;
    names
        .iter()
        .map(|name| {
            let j = jacobian.parameters.iter().position(|p| p == name)?;
            jacobian.columns.get(j).cloned()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(pairs: &[(&str, f64)]) -> BTreeMap<String, f64> {
        pairs.iter().map(|(n, x)| (n.to_string(), *x)).collect()
    }

    /// The two-compartment columns agree with central differences of the predictions, in the
    /// three parameter sets, and only the fitted names get a column.
    #[test]
    fn two_compartment_derivatives_match_central_differences() {
        let times = [0.25, 1.0, 3.0, 12.0, 30.0];
        let cases: [(ModelId, Vec<(&str, f64)>); 4] = [
            (
                ModelId::Pk2Oral1Lag,
                vec![
                    ("cl", 2.0),
                    ("vc", 10.0),
                    ("q", 4.0),
                    ("vp", 8.0),
                    ("ka", 2.0),
                    ("tlag", 0.1),
                ],
            ),
            (
                ModelId::Pk2IvInfusion,
                vec![
                    ("k10", 0.2),
                    ("k12", 0.4),
                    ("k21", 0.5),
                    ("vc", 10.0),
                    ("dur", 2.0),
                ],
            ),
            (
                ModelId::Pk2IvBolus,
                vec![
                    ("a", 50.0 / 9.0),
                    ("b", 40.0 / 9.0),
                    ("alpha", 1.0),
                    ("beta", 0.1),
                ],
            ),
            (
                ModelId::Pk2Oral0,
                vec![
                    ("cl", 1.2),
                    ("vc", 6.0),
                    ("q", 30.0),
                    ("vp", 45.0),
                    ("dur", 0.5),
                ],
            ),
        ];
        for (model, list) in cases {
            let params = named(&list);
            let names: Vec<String> = params.keys().cloned().collect();
            let exact = model
                .analytic_derivatives(100.0, &params, &times, &names)
                .unwrap();
            for (j, name) in names.iter().enumerate() {
                let h = 1e-6 * params[name];
                let mut up = params.clone();
                let mut down = params.clone();
                up.insert(name.clone(), params[name] + h);
                down.insert(name.clone(), params[name] - h);
                let fu = model.predict(100.0, &up, &times).unwrap();
                let fd = model.predict(100.0, &down, &times).unwrap();
                for i in 0..times.len() {
                    let numeric = (fu[i] - fd[i]) / (2.0 * h);
                    let a = exact[j][i];
                    assert!(
                        (a - numeric).abs() <= 1e-6 * (1e-3 + a.abs()),
                        "{model:?} {name} at t = {}: {a} against {numeric}",
                        times[i]
                    );
                }
            }
            let fitted = vec![names[1].clone()];
            let one = model
                .analytic_derivatives(100.0, &params, &times, &fitted)
                .unwrap();
            assert_eq!(one, vec![exact[1].clone()]);
        }
        // A name the model does not have, or a refused parameter set, has no closed form.
        let p = named(&[("cl", 2.0), ("vc", 10.0), ("q", 4.0), ("vp", 8.0)]);
        let wrong = vec!["ka".to_string()];
        assert!(
            ModelId::Pk2IvBolus
                .analytic_derivatives(100.0, &p, &times, &wrong)
                .is_none()
        );
        let bad = named(&[("cl", 2.0), ("vc", 10.0), ("q", 0.0), ("vp", 8.0)]);
        let names = vec!["cl".to_string()];
        assert!(
            ModelId::Pk2IvBolus
                .analytic_derivatives(100.0, &bad, &times, &names)
                .is_none()
        );
    }

    /// A fit with the closed forms recovers exact two-compartment data from given initial values;
    /// automatic initial estimates are refused with a readable error.
    #[test]
    fn a_two_compartment_fit_recovers_exact_data() {
        let truth = [
            ("cl", 2.0),
            ("vc", 10.0),
            ("q", 4.0),
            ("vp", 8.0),
            ("ka", 2.0),
        ];
        let time = [
            0.1, 0.25, 0.5, 0.75, 1.0, 1.5, 2.0, 3.0, 4.0, 6.0, 8.0, 12.0, 24.0, 36.0, 48.0,
        ];
        let conc = ModelId::Pk2Oral1
            .predict(100.0, &named(&truth), &time)
            .unwrap();
        let input = |initial: &[(&str, f64)]| crate::FitInput {
            model: ModelId::Pk2Oral1,
            dose: 100.0,
            time: time.to_vec(),
            conc: conc.clone(),
            weighting: crate::Weighting::Uniform,
            initial: named(initial),
            options: crate::FitOptions {
                derivatives: crate::Derivatives::Analytic,
                convergence: 1e-12,
                max_iterations: 200,
                ..crate::FitOptions::default()
            },
        };
        let r = crate::run(&input(&[
            ("cl", 2.5),
            ("vc", 8.0),
            ("q", 3.0),
            ("vp", 10.0),
            ("ka", 1.5),
        ]))
        .unwrap();
        for (name, x) in truth {
            let got = r.get(&format!("estimate.{name}")).unwrap();
            assert!((got - x).abs() <= 1e-6 * x, "{name}: {got} against {x}");
        }
        let e = crate::run(&input(&[])).unwrap_err();
        assert!(e.to_string().contains("two-compartment"), "{e}");
    }

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
