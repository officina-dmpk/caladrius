//! Initial estimates from the data (`specs/fit.md` FIT-INI-01 to 04): log-linear regression for
//! the IV bolus, curve stripping (method of residuals) for first-order absorption, terminal phase
//! after the end of the input for zero-order input. The terminal phase is chosen by the rules of
//! `specs/nca.md` NCA-LZ-02 to 05, through `caladrius-nca`.

use std::collections::BTreeMap;

use caladrius_models::ModelId;
use caladrius_nca::{NcaInput, NcaOptions, Route};

use crate::FitError;

fn unavailable(reason: impl Into<String>) -> FitError {
    FitError::InitialEstimatesUnavailable {
        reason: reason.into(),
    }
}

/// Ordinary least squares of y on x: (intercept, slope), `None` without two distinct x.
fn line(points: &[(f64, f64)]) -> Option<(f64, f64)> {
    let n = points.len() as f64;
    if points.len() < 2 {
        return None;
    }
    let mx = points.iter().map(|p| p.0).sum::<f64>() / n;
    let my = points.iter().map(|p| p.1).sum::<f64>() / n;
    let sxx: f64 = points.iter().map(|p| (p.0 - mx).powi(2)).sum();
    let sxy: f64 = points.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum();
    if sxx.is_nan() || sxx <= 0.0 {
        return None;
    }
    let slope = sxy / sxx;
    Some((my - slope * mx, slope))
}

/// Terminal line ln y = a − k·t chosen by the NCA rules on the samples after `input_end`:
/// (a, k, first time of the window).
fn terminal_line(
    dose: f64,
    data: &[(f64, f64)],
    input_end: f64,
) -> Result<(f64, f64, f64), FitError> {
    let route = if input_end > 0.0 {
        Route::IvInfusion {
            duration: input_end,
        }
    } else {
        Route::Extravascular
    };
    let result = caladrius_nca::run(&NcaInput {
        time: data.iter().map(|p| p.0).collect(),
        conc: data.iter().map(|p| p.1).collect(),
        dose,
        route,
        options: NcaOptions::default(),
    })
    .map_err(|e| unavailable(format!("the data cannot be analysed ({e})")))?;
    let fit = result
        .lambda_z_candidates()
        .iter()
        .find(|c| c.selected)
        .ok_or_else(|| {
            unavailable(
                "the terminal phase has fewer than 3 usable declining points after the peak",
            )
        })?;
    Ok((fit.intercept, fit.lambda_z, fit.time_first))
}

/// Initial estimates of the parameters of `model` from the observations (FIT-INI-01). Fixed
/// parameters (`fixed`) are used and not returned; zero-order input needs `dur` among them.
pub fn initial_estimates(
    model: ModelId,
    dose: f64,
    time: &[f64],
    conc: &[f64],
    fixed: &BTreeMap<String, f64>,
) -> Result<BTreeMap<String, f64>, FitError> {
    if time.len() != conc.len() {
        return Err(FitError::LengthMismatch {
            times: time.len(),
            concs: conc.len(),
        });
    }
    if !(dose.is_finite() && dose > 0.0) {
        return Err(FitError::InvalidDose { value: dose });
    }
    let mut data: Vec<(f64, f64)> = time
        .iter()
        .copied()
        .zip(conc.iter().copied())
        .filter(|(t, c)| t.is_finite() && c.is_finite() && *t >= 0.0)
        .collect();
    data.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out = BTreeMap::new();
    match model {
        ModelId::IvBolus => {
            // FIT-INI-02: ln y = a − k·t on every positive point; V = D/e^a.
            let logs: Vec<(f64, f64)> = data
                .iter()
                .filter(|p| p.1 > 0.0)
                .map(|&(t, c)| (t, c.ln()))
                .collect();
            let (a, slope) =
                line(&logs).ok_or_else(|| unavailable("fewer than 2 positive concentrations"))?;
            if slope.is_nan() || slope >= 0.0 {
                return Err(unavailable("the concentrations do not decline"));
            }
            out.insert("v".to_string(), dose / a.exp());
            out.insert("k".to_string(), -slope);
        }
        ModelId::Oral1 | ModelId::Oral1Lag => {
            // FIT-INI-03, curve stripping.
            let (a1, k, window_start) = terminal_line(dose, &data, 0.0)?;
            let residuals: Vec<(f64, f64)> = data
                .iter()
                .filter(|p| p.0 < window_start)
                .map(|&(t, c)| (t, (a1 - k * t).exp() - c))
                .filter(|p| p.1 > 0.0)
                .map(|(t, r)| (t, r.ln()))
                .collect();
            let (a2, slope2) = line(&residuals).ok_or_else(|| {
                unavailable(
                    "fewer than 2 points before the terminal phase have a positive residual",
                )
            })?;
            if slope2.is_nan() || slope2 >= 0.0 {
                return Err(unavailable("the residual line rises"));
            }
            let ka_raw = -slope2;
            if ka_raw == k {
                return Err(unavailable("the absorption and elimination lines coincide"));
            }
            let fitted_lag = model == ModelId::Oral1Lag && !fixed.contains_key("tlag");
            let tlag = match model {
                ModelId::Oral1Lag => match fixed.get("tlag") {
                    Some(&t) => t,
                    // The two lines meet at the lag time; 0 if they meet before the dose.
                    // Kept below the first sample, so that the model predicts something there.
                    None => ((a1 - a2) / (k - ka_raw))
                        .max(0.0)
                        .min(0.9 * data.first().map_or(0.0, |p| p.0)),
                },
                _ => 0.0,
            };
            // Flip-flop (MOD-AB1-06): the faster constant is taken as ka.
            let (k, ka) = if ka_raw > k { (k, ka_raw) } else { (ka_raw, k) };
            let amplitude = (a1 - k * tlag).exp();
            out.insert("v".to_string(), dose * ka / ((ka - k) * amplitude));
            out.insert("k".to_string(), k);
            out.insert("ka".to_string(), ka);
            if fitted_lag {
                out.insert("tlag".to_string(), tlag);
            }
        }
        ModelId::Pk2IvBolus
        | ModelId::Pk2IvInfusion
        | ModelId::Pk2Oral1
        | ModelId::Pk2Oral1Lag
        | ModelId::Pk2Oral0
        | ModelId::Pk2Oral0Lag => {
            // FIT-INI-01 has no stripping rule for two compartments yet (specs/fit.md OF-08).
            return Err(unavailable(
                "automatic initial estimates are not available for two-compartment models yet; give an initial value for every parameter",
            ));
        }
        ModelId::IvInfusion | ModelId::Oral0 | ModelId::Oral0Lag => {
            // FIT-INI-04: terminal phase after the end of the input.
            let dur = fixed.get("dur").copied().ok_or_else(|| {
                unavailable("the input duration `dur` must be given as a fixed parameter")
            })?;
            if !(dur.is_finite() && dur > 0.0) {
                return Err(unavailable(format!(
                    "the fixed input duration `dur` = {dur} must be a finite number > 0"
                )));
            }
            let fitted_lag = model == ModelId::Oral0Lag && !fixed.contains_key("tlag");
            let tlag = fixed.get("tlag").copied().unwrap_or(0.0);
            let (a1, k, _) = terminal_line(dose, &data, dur + tlag)?;
            let c_end = (a1 - k * (dur + tlag)).exp();
            let cl = dose / dur * -(-k * dur).exp_m1() / c_end;
            out.insert("v".to_string(), cl / k);
            out.insert("k".to_string(), k);
            if fitted_lag {
                out.insert("tlag".to_string(), 0.0);
            }
        }
    }
    if out.values().any(|x| !x.is_finite()) {
        return Err(unavailable("the estimates are not finite numbers"));
    }
    Ok(out)
}
