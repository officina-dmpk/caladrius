//! Terminal phase (`specs/nca.md` section 6): eligible points, candidate sets, log-linear
//! regression, automatic or manual selection of λz.

use serde::{Deserialize, Serialize};

use crate::clean::{DOSE_TIME, ProfilePoint};
use crate::result::NcReason;
use crate::{LambdaZManual, LambdaZTieRule, NcaError, NcaOptions, Route};

/// One candidate fit of the terminal phase, kept with the result so that a client can show the
/// choice and let the user change it (NCA-OUT-01).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LambdaZCandidate {
    /// Number of points.
    pub n_points: usize,
    /// Time of the first point used.
    pub time_first: f64,
    /// Time of the last point used.
    pub time_last: f64,
    /// Terminal rate constant (minus the slope of ln C against t).
    #[serde(
        serialize_with = "crate::float::ser",
        deserialize_with = "crate::float::de"
    )]
    pub lambda_z: f64,
    /// Intercept of ln C against t.
    #[serde(
        serialize_with = "crate::float::ser",
        deserialize_with = "crate::float::de"
    )]
    pub intercept: f64,
    /// R²; `None` when ln C is flat (undefined).
    pub r_squared: Option<f64>,
    /// Adjusted R²; `None` for fewer than 3 points or a flat ln C.
    pub adj_r_squared: Option<f64>,
    /// λz > 0 (NCA-LZ-04).
    pub valid: bool,
    /// This fit gives the reported λz.
    pub selected: bool,
    /// Points of the fit whose value was missing or BLQ and replaced by a number.
    #[serde(default)]
    pub replaced_points: usize,
}

impl LambdaZCandidate {
    /// Half-life ln 2 / λz; `None` unless the fit is valid (λz > 0).
    pub fn half_life(&self) -> Option<f64> {
        self.valid
            .then(|| std::f64::consts::LN_2 / self.lambda_z)
            .filter(|h| h.is_finite())
    }
}

/// The terminal phase of a profile: the selected fit or why there is none, and every candidate.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Terminal {
    pub selected: Result<LambdaZCandidate, NcReason>,
    pub candidates: Vec<LambdaZCandidate>,
}

/// Ordinary least squares of y = ln C on x = t, on centred sums (NCA-LZ-01). `None` for fewer
/// than 2 points, no spread in time (impossible with strictly increasing times), or sums that
/// overflow. R² and adjusted R² are `None` when undefined (flat ln C, fewer than 3 points).
fn regress(points: &[(f64, f64)]) -> Option<LambdaZCandidate> {
    let (&(time_first, _), &(time_last, _)) = (points.first()?, points.last()?);
    let n = points.len();
    if n < 2 {
        return None;
    }
    let count = n as f64;
    let x_mean = points.iter().map(|p| p.0).sum::<f64>() / count;
    let y_mean = points.iter().map(|p| p.1).sum::<f64>() / count;
    let (mut sxx, mut sxy, mut syy) = (0.0, 0.0, 0.0);
    for &(x, y) in points {
        let (dx, dy) = (x - x_mean, y - y_mean);
        sxx += dx * dx;
        sxy += dx * dy;
        syy += dy * dy;
    }
    if !(sxx.is_finite() && syy.is_finite() && sxy.is_finite()) || sxx <= 0.0 {
        return None;
    }
    let slope = sxy / sxx;
    let r_squared = (syy > 0.0)
        .then(|| sxy * sxy / (sxx * syy))
        .filter(|r2| r2.is_finite());
    let adj_r_squared = r_squared
        .filter(|_| n >= 3)
        .map(|r2| 1.0 - (1.0 - r2) * (count - 1.0) / (count - 2.0))
        .filter(|a| a.is_finite());
    let lambda_z = -slope;
    Some(LambdaZCandidate {
        n_points: n,
        time_first,
        time_last,
        lambda_z,
        intercept: y_mean - slope * x_mean,
        r_squared,
        adj_r_squared,
        valid: lambda_z > 0.0 && lambda_z.is_finite(),
        selected: false,
        replaced_points: 0,
    })
}

fn ln_points(points: &[&ProfilePoint]) -> Vec<(f64, f64)> {
    points.iter().map(|p| (p.time, p.conc.ln())).collect()
}

/// The fit of `points`, with the count of replaced values among them.
fn fit_points(points: &[&ProfilePoint]) -> Option<LambdaZCandidate> {
    let mut fit = regress(&ln_points(points))?;
    fit.replaced_points = points.iter().filter(|p| p.is_replaced()).count();
    Some(fit)
}

/// Terminal phase of the cleaned profile `points` (which may include a point inserted at the dose
/// time; it is never used, NCA-LZ-13). `tmax` is the observed Tmax.
pub(crate) fn terminal(
    points: &[ProfilePoint],
    route: Route,
    options: &NcaOptions,
    tmax: Option<f64>,
) -> Result<Terminal, NcaError> {
    // No measured positive value: nothing to fit, even if replaced values are positive (T-015).
    if !points.iter().any(ProfilePoint::is_quantifiable) {
        return Ok(Terminal {
            selected: Err(NcReason::NoPositiveConcentration),
            candidates: Vec::new(),
        });
    }
    match &options.lambda_z_selection.manual {
        Some(manual) => manual_fit(points, manual, options.lambda_z_selection.exclude_replaced),
        None => Ok(automatic(points, route, options, tmax)),
    }
}

/// NCA-LZ-08: the user's points, no selection.
/// A point that may enter a regression: C > 0 after cleaning (NCA-LZ-02 rule 1), and not a
/// replaced value when `exclude_replaced` is set (the NCA-LZ-02b reading).
fn usable(p: &ProfilePoint, exclude_replaced: bool) -> bool {
    p.is_positive_sample() && !(exclude_replaced && p.is_replaced())
}

fn manual_fit(
    points: &[ProfilePoint],
    manual: &LambdaZManual,
    exclude_replaced: bool,
) -> Result<Terminal, NcaError> {
    let usable = |p: &ProfilePoint| usable(p, exclude_replaced);
    let chosen: Vec<&ProfilePoint> = match manual {
        LambdaZManual::Times(times) => {
            for &t in times {
                if !points.iter().any(|p| p.time == t && usable(p)) {
                    return Err(NcaError::InvalidOption {
                        option: "lambda_z_selection.manual".to_string(),
                        reason: format!(
                            "the sample at time {t} has no usable concentration (not above zero, removed by cleaning, or a replaced value excluded by `exclude_replaced`); leave it out of the terminal phase"
                        ),
                    });
                }
            }
            points
                .iter()
                .filter(|p| usable(p) && times.contains(&p.time))
                .collect()
        }
        LambdaZManual::Range { start, end } => points
            .iter()
            .filter(|p| usable(p) && p.time >= *start && p.time <= *end)
            .collect(),
    };
    if chosen.len() < 2 {
        return Ok(Terminal {
            selected: Err(NcReason::TooFewPoints),
            candidates: Vec::new(),
        });
    }
    let Some(mut fit) = fit_points(&chosen) else {
        return Ok(Terminal {
            selected: Err(NcReason::TooFewPoints),
            candidates: Vec::new(),
        });
    };
    fit.selected = fit.valid;
    let selected = if fit.valid {
        Ok(fit)
    } else {
        Err(NcReason::NoValidFit)
    };
    Ok(Terminal {
        selected,
        candidates: vec![fit],
    })
}

/// NCA-LZ-02 to 07: eligible points, candidate sets ending at Tlast, selection.
fn automatic(
    points: &[ProfilePoint],
    route: Route,
    options: &NcaOptions,
    tmax: Option<f64>,
) -> Terminal {
    let lz = &options.lambda_z;
    let selection = &options.lambda_z_selection;
    let dose_end = match route {
        Route::IvInfusion { duration } => DOSE_TIME + duration,
        Route::Extravascular | Route::IvBolus => DOSE_TIME,
    };
    let after_tmax = |t: f64| match tmax {
        Some(tm) => t > tm || (lz.allow_tmax && t == tm),
        None => false,
    };
    let eligible: Vec<&ProfilePoint> = points
        .iter()
        .filter(|p| {
            usable(p, selection.exclude_replaced)
                && p.time > dose_end
                && after_tmax(p.time)
                && !selection.exclude.contains(&p.time)
        })
        .collect();
    let m = eligible.len();
    if m < lz.min_points {
        return Terminal {
            selected: Err(NcReason::TooFewPoints),
            candidates: Vec::new(),
        };
    }
    let mut candidates: Vec<LambdaZCandidate> = (lz.min_points..=m)
        .filter_map(|n| eligible.get(m - n..).and_then(fit_points))
        .collect();

    let factor = lz.adj_r_squared_factor;
    // Fits that take part in the comparison: a defined adjusted R², and λz > 0 first when asked.
    let competes = |c: &LambdaZCandidate| {
        c.adj_r_squared.is_some() && (c.valid || !selection.positive_filter_first)
    };
    let chosen_n = match selection.tie_rule {
        LambdaZTieRule::Tolerance => {
            let best = candidates
                .iter()
                .filter(|c| competes(c))
                .filter_map(|c| c.adj_r_squared)
                .reduce(f64::max);
            best.and_then(|best| {
                candidates
                    .iter()
                    .filter(|c| competes(c) && c.valid)
                    // Strict `>` as PKNCA (T-005); the best fit itself always qualifies, which only
                    // matters for a factor of 0.
                    .filter(|c| {
                        c.adj_r_squared
                            .is_some_and(|a| a > best - factor || a >= best)
                    })
                    .map(|c| c.n_points)
                    .max()
            })
        }
        LambdaZTieRule::Bonus => {
            let score = |c: &LambdaZCandidate| {
                c.adj_r_squared.unwrap_or(f64::NEG_INFINITY) + factor * c.n_points as f64
            };
            // NCA-LZ-06: the valid fit with the largest score.
            candidates
                .iter()
                .filter(|c| competes(c) && c.valid)
                .max_by(|a, b| {
                    score(a)
                        .total_cmp(&score(b))
                        .then(a.n_points.cmp(&b.n_points))
                })
                .map(|c| c.n_points)
        }
    };
    let mut selected = Err(NcReason::NoValidFit);
    for c in &mut candidates {
        if Some(c.n_points) == chosen_n {
            c.selected = true;
            selected = Ok(*c);
        }
    }
    Terminal {
        selected,
        candidates,
    }
}
