//! Quality flags of a fit (`specs/fit.md` FIT-FLG-01): a poor fit is flagged, not hidden. Flags
//! never change a number; each says what to check.

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::{FitStatus, FlagThresholds};

/// One quality problem of a fit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "code", deny_unknown_fields)]
pub enum FitFlag {
    /// The fit did not converge; the statistics describe the best iterate only.
    NotConverged {
        /// How it ended.
        status: FitStatus,
    },
    /// Converged on a damped or shortened last step: the criterion was met while progress was
    /// slow, and the minimum may be further away than the criterion suggests.
    WeakConvergence {
        /// Damping λ of the last step.
        #[serde(
            serialize_with = "crate::float::ser",
            deserialize_with = "crate::float::de"
        )]
        lambda: f64,
        /// Step factor ν of the last step.
        #[serde(
            serialize_with = "crate::float::ser",
            deserialize_with = "crate::float::de"
        )]
        step: f64,
    },
    /// A parameter ends on a bound (FIT-BND-02); its statistics are not computed and the others
    /// are conditional on it.
    AtBound {
        /// The parameter.
        parameter: String,
        /// The bound.
        #[serde(
            serialize_with = "crate::float::ser",
            deserialize_with = "crate::float::de"
        )]
        bound: f64,
    },
    /// CV% above the threshold.
    HighCv {
        /// The parameter.
        parameter: String,
        /// CV%.
        #[serde(
            serialize_with = "crate::float::ser",
            deserialize_with = "crate::float::de"
        )]
        value: f64,
        /// Threshold.
        #[serde(
            serialize_with = "crate::float::ser",
            deserialize_with = "crate::float::de"
        )]
        threshold: f64,
    },
    /// The univariate interval of a parameter that must be positive contains 0.
    IntervalContainsZero {
        /// The parameter.
        parameter: String,
    },
    /// Two estimates are strongly correlated.
    HighCorrelation {
        /// First parameter.
        first: String,
        /// Second parameter.
        second: String,
        /// Correlation.
        #[serde(
            serialize_with = "crate::float::ser",
            deserialize_with = "crate::float::de"
        )]
        value: f64,
        /// Threshold on its absolute value.
        #[serde(
            serialize_with = "crate::float::ser",
            deserialize_with = "crate::float::de"
        )]
        threshold: f64,
    },
    /// The correlation matrix is ill-conditioned.
    IllConditioned {
        /// Condition number.
        #[serde(
            serialize_with = "crate::float::ser",
            deserialize_with = "crate::float::de"
        )]
        value: f64,
        /// Threshold.
        #[serde(
            serialize_with = "crate::float::ser",
            deserialize_with = "crate::float::de"
        )]
        threshold: f64,
    },
    /// Few degrees of freedom (an exact or nearly exact fit carries little precision information).
    FewDegreesOfFreedom {
        /// Degrees of freedom.
        value: usize,
        /// Threshold.
        threshold: usize,
    },
}

impl fmt::Display for FitFlag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotConverged { status } => write!(f, "{}", status.message()),
            Self::WeakConvergence { lambda, step } => write!(
                f,
                "converged on a damped or shortened step (λ = {lambda}, step {step}); refit with a tighter criterion or from other initial estimates to confirm the minimum"
            ),
            Self::AtBound { parameter, bound } => write!(
                f,
                "`{parameter}` ends on its bound {bound}; the data do not determine it inside the bounds: check the model, the bounds and the initial estimates"
            ),
            Self::HighCv {
                parameter,
                value,
                threshold,
            } => write!(
                f,
                "`{parameter}` is imprecise (CV {value:.1} % > {threshold} %); fewer parameters, more informative data or another weighting may help"
            ),
            Self::IntervalContainsZero { parameter } => write!(
                f,
                "the confidence interval of `{parameter}` contains 0 although it must be positive; the parameter is not determined by the data"
            ),
            Self::HighCorrelation {
                first,
                second,
                value,
                threshold,
            } => write!(
                f,
                "`{first}` and `{second}` are strongly correlated ({value:.3}, |r| > {threshold}); they cannot be estimated separately with confidence"
            ),
            Self::IllConditioned { value, threshold } => write!(
                f,
                "the estimates are ill-conditioned (condition number {value:.3e} > {threshold:e}); simplify the model or fix a parameter"
            ),
            Self::FewDegreesOfFreedom { value, threshold } => write!(
                f,
                "{value} degree(s) of freedom (< {threshold}); the precision of the estimates is barely or not determined"
            ),
        }
    }
}

/// Names of parameters that must be positive (MOD-GEN-04).
fn must_be_positive(name: &str) -> bool {
    matches!(name, "v" | "cl" | "k" | "ka" | "dur")
}

/// What the flags look at.
pub(crate) struct Inputs<'a> {
    pub status: FitStatus,
    /// λ and ν of the last accepted step, if any.
    pub last_step: Option<(f64, f64)>,
    /// Parameters on a bound, with the bound.
    pub at_bound: Vec<(String, f64)>,
    /// Freely estimated parameters.
    pub free: &'a [String],
    pub df: usize,
    pub get: &'a dyn Fn(&str) -> Option<f64>,
}

/// Every flag raised under `t`, in a stable order.
pub(crate) fn flags(t: &FlagThresholds, x: &Inputs<'_>) -> Vec<FitFlag> {
    let mut out = Vec::new();
    match x.status {
        FitStatus::Converged | FitStatus::AtBound => {
            if let Some((lambda, step)) = x.last_step {
                if lambda > 0.0 || step < 1.0 {
                    out.push(FitFlag::WeakConvergence { lambda, step });
                }
            }
        }
        status => out.push(FitFlag::NotConverged { status }),
    }
    for (parameter, bound) in &x.at_bound {
        out.push(FitFlag::AtBound {
            parameter: parameter.clone(),
            bound: *bound,
        });
    }
    for name in x.free {
        if let (Some(threshold), Some(value)) =
            (t.max_cv_percent, (x.get)(&format!("cv_percent.{name}")))
        {
            if value > threshold {
                out.push(FitFlag::HighCv {
                    parameter: name.clone(),
                    value,
                    threshold,
                });
            }
        }
        if must_be_positive(name) {
            let lo = (x.get)(&format!("ci_lo.{name}"));
            let hi = (x.get)(&format!("ci_hi.{name}"));
            if let (Some(lo), Some(hi)) = (lo, hi) {
                if lo <= 0.0 && hi >= 0.0 {
                    out.push(FitFlag::IntervalContainsZero {
                        parameter: name.clone(),
                    });
                }
            }
        }
    }
    if let Some(threshold) = t.max_abs_correlation {
        for (a, first) in x.free.iter().enumerate() {
            for second in x.free.iter().skip(a + 1) {
                if let Some(value) = (x.get)(&format!("correlation.{first}.{second}")) {
                    if value.abs() > threshold {
                        out.push(FitFlag::HighCorrelation {
                            first: first.clone(),
                            second: second.clone(),
                            value,
                            threshold,
                        });
                    }
                }
            }
        }
    }
    if let (Some(threshold), Some(value)) = (t.max_condition_number, (x.get)("condition_number")) {
        if value > threshold {
            out.push(FitFlag::IllConditioned { value, threshold });
        }
    }
    if let Some(threshold) = t.min_degrees_of_freedom {
        if x.df < threshold {
            out.push(FitFlag::FewDegreesOfFreedom {
                value: x.df,
                threshold,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_damped_or_shortened_last_step_is_weak_convergence() {
        let get = |_: &str| None;
        let free = ["v".to_string()];
        let inputs = |last_step| Inputs {
            status: FitStatus::Converged,
            last_step,
            at_bound: Vec::new(),
            free: &free,
            df: 5,
            get: &get,
        };
        let t = FlagThresholds::default();
        assert!(flags(&t, &inputs(Some((0.0, 1.0)))).is_empty());
        assert_eq!(
            flags(&t, &inputs(Some((1e-3, 0.5)))),
            vec![FitFlag::WeakConvergence {
                lambda: 1e-3,
                step: 0.5
            }]
        );
    }
}
