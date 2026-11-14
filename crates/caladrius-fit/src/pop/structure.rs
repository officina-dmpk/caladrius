//! The structural model of one subject as a function of its random effects (POP-MOD-01, POP-MOD-05):
//! the prediction ŷ(η) and the n × q matrix G(η) = ∂ŷ/∂η.
//!
//! For a catalogue model P_k = θ_k·exp(η_k) on the parameters that carry a random effect, so
//! G_jk = (∂ŷ_j/∂P_k)·P_k (the chain rule through the exponential). The partial derivatives are the
//! closed forms of the individual fit (FIT-JAC-01 to 03, `crate::model`). Those of one compartment
//! are written in the (v, k) parameterisation; the population fit takes (v, cl) as well (POP-MOD-02
//! rule 6), so with `cl` the columns are converted by the chain rule through k = cl/v (D):
//! ∂C/∂cl = (∂C/∂k)/v and ∂C/∂v at fixed cl = ∂C/∂v at fixed k − (∂C/∂k)·cl/v², hence
//! G_cl = k·∂C/∂k and G_v = v·∂C/∂v|k − k·∂C/∂k.
//!
//! The test-only model `identity_linear` (POP-MOD-05) has ŷ_j = θ1 + θ2·t_j + Σ_k t_j^(z_k)·η_k and
//! G = Z.

use std::collections::BTreeMap;

use caladrius_models::ModelId;

use crate::arith::powi;
use crate::model::FitModel;

/// Which structural model a run uses.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Structure {
    /// A single-dose model of the catalogue (POP-MOD-04).
    Catalogue(ModelId),
    /// The identity-link test model of POP-MOD-05, with the powers of the columns of Z.
    IdentityLinear { z_powers: Vec<i32> },
}

/// One subject's model: everything but η is held.
#[derive(Debug, Clone)]
pub(crate) struct SubjectModel<'a> {
    pub structure: &'a Structure,
    /// The typical values, with this subject's own values (an infusion duration) merged in.
    pub theta: &'a BTreeMap<String, f64>,
    /// The parameters (or labels, for `identity_linear`) that carry a random effect, in order.
    pub random_effects: &'a [String],
    pub dose: f64,
    pub times: &'a [f64],
}

/// The prediction and its derivatives with respect to η at one point.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Evaluation {
    pub pred: Vec<f64>,
    /// One column per random effect, each of length n.
    pub g: Vec<Vec<f64>>,
}

impl SubjectModel<'_> {
    /// The individual parameters P(η) of a catalogue model.
    fn individual(&self, eta: &[f64]) -> Option<BTreeMap<String, f64>> {
        let mut p = self.theta.clone();
        for (name, e) in self.random_effects.iter().zip(eta) {
            let slot = p.get_mut(name)?;
            *slot *= libm::exp(*e);
        }
        Some(p)
    }

    /// ŷ(η), or `None` when the model refuses the parameters or gives a non-finite value.
    pub fn predict(&self, eta: &[f64]) -> Option<Vec<f64>> {
        let pred = match self.structure {
            Structure::Catalogue(model) => {
                let p = self.individual(eta)?;
                model.predict(self.dose, &p, self.times).ok()?
            }
            Structure::IdentityLinear { z_powers } => {
                let t1 = self.theta.get("theta1").copied()?;
                let t2 = self.theta.get("theta2").copied()?;
                self.times
                    .iter()
                    .map(|&t| {
                        t1 + t2 * t
                            + z_powers
                                .iter()
                                .zip(eta)
                                .map(|(&z, e)| powi(t, z) * e)
                                .sum::<f64>()
                    })
                    .collect()
            }
        };
        pred.iter().all(|x| x.is_finite()).then_some(pred)
    }

    /// ŷ(η) and G(η).
    pub fn evaluate(&self, eta: &[f64]) -> Option<Evaluation> {
        let pred = self.predict(eta)?;
        let g = match self.structure {
            Structure::Catalogue(model) => {
                let p = self.individual(eta)?;
                self.catalogue_columns(*model, &p)?
            }
            Structure::IdentityLinear { z_powers } => z_powers
                .iter()
                .map(|&z| self.times.iter().map(|&t| powi(t, z)).collect())
                .collect(),
        };
        let finite = g.iter().all(|c: &Vec<f64>| c.iter().all(|x| x.is_finite()));
        finite.then_some(Evaluation { pred, g })
    }

    /// G_k = (∂ŷ/∂P_k)·P_k for a catalogue model.
    fn catalogue_columns(
        &self,
        model: ModelId,
        p: &BTreeMap<String, f64>,
    ) -> Option<Vec<Vec<f64>>> {
        let names = self.random_effects;
        let one_compartment = model.compartments() == 1;
        if one_compartment && p.contains_key("cl") {
            return self.clearance_columns(model, p);
        }
        let columns = model.analytic_derivatives(self.dose, p, self.times, names)?;
        names
            .iter()
            .zip(columns)
            .map(|(name, col)| {
                let pk = p.get(name).copied()?;
                Some(col.iter().map(|d| d * pk).collect())
            })
            .collect()
    }

    /// One compartment with (v, cl): the closed forms in (v, k), then the chain rule of the module
    /// header.
    fn clearance_columns(
        &self,
        model: ModelId,
        p: &BTreeMap<String, f64>,
    ) -> Option<Vec<Vec<f64>>> {
        let v = p.get("v").copied()?;
        let cl = p.get("cl").copied()?;
        let k = cl / v;
        let mut pk = p.clone();
        pk.remove("cl");
        pk.insert("k".to_string(), k);
        // The columns of the (v, k) forms that are needed: v and k for either of v and cl, and the
        // others under their own names.
        let mut wanted: Vec<String> = vec!["v".to_string(), "k".to_string()];
        for name in self.random_effects {
            if name != "v" && name != "cl" && !wanted.contains(name) {
                wanted.push(name.clone());
            }
        }
        let columns = model.analytic_derivatives(self.dose, &pk, self.times, &wanted)?;
        let column = |name: &str| -> Option<&Vec<f64>> {
            let j = wanted.iter().position(|w| w == name)?;
            columns.get(j)
        };
        let dv = column("v")?;
        let dk = column("k")?;
        self.random_effects
            .iter()
            .map(|name| match name.as_str() {
                "cl" => Some(dk.iter().map(|d| k * d).collect()),
                "v" => Some(dv.iter().zip(dk).map(|(a, b)| v * a - k * b).collect()),
                other => {
                    let pk_other = pk.get(other).copied()?;
                    Some(column(other)?.iter().map(|d| d * pk_other).collect())
                }
            })
            .collect()
    }
}

/// The parameter names a catalogue model can take (its disposition sets and its input).
pub(crate) fn model_parameters(model: ModelId) -> Vec<&'static str> {
    let mut names: Vec<&'static str> = if model.compartments() == 1 {
        vec!["v", "cl", "k"]
    } else {
        vec!["cl", "vc", "q", "vp"]
    };
    names.extend_from_slice(model.input_parameters());
    names
}
