//! Structural validation of the input and of the options (NCA-DAT-02, DAT-04, DAT-11). It runs
//! before any computation; the first problem found is returned.

use crate::{
    BlqAction, BlqPolicy, LambdaZManual, LambdaZSelection, MissingPolicy, NcaError, NcaInput,
    NcaOptions, NegativePolicy, QualityThresholds, Route,
};

/// Checks `input`; `Ok` means every later step may assume: equal non-zero lengths, finite strictly
/// increasing times, no infinite concentration, no negative concentration under the `Error` policy,
/// a valid route and valid options. The dose is not checked here (NCA-DAT-10).
pub(crate) fn validate(input: &NcaInput) -> Result<(), NcaError> {
    if input.time.len() != input.conc.len() {
        return Err(NcaError::LengthMismatch {
            times: input.time.len(),
            concs: input.conc.len(),
        });
    }
    if input.time.is_empty() {
        return Err(NcaError::EmptyProfile);
    }
    validate_times(&input.time)?;
    validate_concentrations(&input.time, &input.conc, input.options.negative)?;
    if let Route::IvInfusion { duration } = input.route {
        if !(duration.is_finite() && duration > 0.0) {
            return Err(NcaError::InvalidInfusionDuration { value: duration });
        }
    }
    validate_options(&input.options)?;
    validate_selection(&input.time, &input.options.lambda_z_selection)
}

fn validate_times(time: &[f64]) -> Result<(), NcaError> {
    if let Some(index) = time.iter().position(|t| !t.is_finite()) {
        return Err(NcaError::NonFiniteTime { index });
    }
    for (offset, pair) in time.windows(2).enumerate() {
        let &[previous, current] = pair else {
            continue;
        };
        let index = offset + 1;
        if current == previous {
            return Err(NcaError::DuplicateTime {
                index,
                time: current,
            });
        }
        if current < previous {
            return Err(NcaError::UnsortedTime {
                index,
                time: current,
                previous,
            });
        }
    }
    Ok(())
}

fn validate_concentrations(
    time: &[f64],
    conc: &[f64],
    negative: NegativePolicy,
) -> Result<(), NcaError> {
    for (index, (&t, &c)) in time.iter().zip(conc).enumerate() {
        if c.is_infinite() {
            return Err(NcaError::InfiniteConcentration { index, time: t });
        }
        if c < 0.0 && negative == NegativePolicy::Error {
            return Err(NcaError::NegativeConcentration {
                index,
                time: t,
                value: c,
            });
        }
    }
    Ok(())
}

fn invalid(option: &str, reason: String) -> NcaError {
    NcaError::InvalidOption {
        option: option.to_string(),
        reason,
    }
}

/// A substituted concentration must be a finite number >= 0.
fn check_substitute(option: &str, value: f64) -> Result<(), NcaError> {
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        Err(invalid(
            option,
            format!("the replacement concentration {value} must be a finite number >= 0"),
        ))
    }
}

fn check_blq_action(option: &str, action: BlqAction) -> Result<(), NcaError> {
    match action {
        BlqAction::Set(value) => check_substitute(option, value),
        BlqAction::Keep | BlqAction::Drop => Ok(()),
    }
}

fn validate_options(options: &NcaOptions) -> Result<(), NcaError> {
    if let MissingPolicy::Replace(value) = options.missing {
        check_substitute("missing", value)?;
    }
    match options.blq {
        BlqPolicy::Position {
            first,
            middle,
            last,
        } => {
            check_blq_action("blq.first", first)?;
            check_blq_action("blq.middle", middle)?;
            check_blq_action("blq.last", last)?;
        }
        BlqPolicy::Tmax { before, after } => {
            check_blq_action("blq.before", before)?;
            check_blq_action("blq.after", after)?;
        }
    }
    let lz = &options.lambda_z;
    if lz.min_points < 3 {
        return Err(invalid(
            "lambda_z.min_points",
            format!(
                "{} is too few; automatic selection compares adjusted R², which needs at least 3 points (choose the points manually to use 2)",
                lz.min_points
            ),
        ));
    }
    if !(lz.adj_r_squared_factor.is_finite() && lz.adj_r_squared_factor >= 0.0) {
        return Err(invalid(
            "lambda_z.adj_r_squared_factor",
            format!(
                "{} must be a finite number >= 0 (1e-4 is usual)",
                lz.adj_r_squared_factor
            ),
        ));
    }
    validate_quality(&options.quality)
}

/// Quality thresholds (NCA-LZ-12b): a given threshold must be a finite number >= 0.
fn validate_quality(q: &QualityThresholds) -> Result<(), NcaError> {
    let thresholds = [
        ("quality.min_adj_r_squared", q.min_adj_r_squared),
        ("quality.min_span_ratio", q.min_span_ratio),
        (
            "quality.max_extrapolated_percent",
            q.max_extrapolated_percent,
        ),
    ];
    for (option, value) in thresholds {
        if let Some(v) = value {
            if !(v.is_finite() && v >= 0.0) {
                return Err(invalid(
                    option,
                    format!("{v} must be a finite number >= 0, or null to turn the flag off"),
                ));
            }
        }
    }
    Ok(())
}

/// Every time named by a λz option must be one of the sample times (NCA-LZ-08, LZ-09).
fn check_sample_time(option: &str, time: &[f64], t: f64) -> Result<(), NcaError> {
    if time.contains(&t) {
        Ok(())
    } else {
        Err(invalid(
            option,
            format!("{t} is not a sample time of the profile; name the time of an existing sample"),
        ))
    }
}

fn validate_selection(time: &[f64], selection: &LambdaZSelection) -> Result<(), NcaError> {
    for &t in &selection.exclude {
        check_sample_time("lambda_z_selection.exclude", time, t)?;
    }
    match &selection.manual {
        None => Ok(()),
        Some(LambdaZManual::Times(times)) => {
            for &t in times {
                check_sample_time("lambda_z_selection.manual", time, t)?;
            }
            Ok(())
        }
        Some(LambdaZManual::Range { start, end }) => {
            if start.is_finite() && end.is_finite() && start <= end {
                Ok(())
            } else {
                Err(invalid(
                    "lambda_z_selection.manual",
                    format!(
                        "the range from {start} to {end} is not valid; give two finite times with start <= end"
                    ),
                ))
            }
        }
    }
}
