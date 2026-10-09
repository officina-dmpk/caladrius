//! Dosing regimens as data (`specs/models.md` MOD-MD-01, 03, 07, 11, 12; golden rule 5): a single
//! dose (the default), a schedule of doses, a regular regimen of n equal doses, or the steady state
//! of a regular regimen.
//!
//! A regimen travels inside [`ModelInput::params`] under reserved names, so that every client of
//! the model (the fit, the engine commands, the CLI and the MCP server) carries it without a change
//! of its own types: `tau` (interval) and `n_doses` for a regular regimen, `tau` alone for a steady
//! state, and `dose_time[i]`, `dose_amount[i]`, `dose_dur[i]` (`i` from 0) for a schedule. These
//! names are the serialized form of [`Regimen`]; [`ModelInput::regimen`] reads them back, checked,
//! and [`ModelInput::with_regimen`] writes them. With none of them the input is a single dose and is
//! evaluated exactly as before.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{ModelError, ModelInput};

/// Largest number of doses of a regular regimen: beyond it the steady state is the answer (the
/// profile is then within e^(−n·k·τ) of it, MOD-MD-07), and the plain sum would take too long.
pub const MAX_REGULAR_DOSES: u32 = 10_000;

/// One dose of a schedule (MOD-MD-01).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DoseEvent {
    /// Dose time, on the same clock as the evaluation times; finite, any sign.
    pub time: f64,
    /// Effective dose F × D of this dose, finite and >= 0 (0 is allowed and contributes nothing).
    pub amount: f64,
    /// Duration of the input (infusion, zero-order absorption), finite and > 0; when absent, the
    /// model parameter `dur`. Refused for an input without a duration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dur: Option<f64>,
}

/// How the doses are given.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Regimen {
    /// One dose `ModelInput::dose` at time 0; times since that dose.
    #[default]
    Single,
    /// Doses at any times, in any order, overlapping or not; times on the clock of the dose times
    /// (MOD-MD-01). `ModelInput::dose` is not used for the profile (only, for the macro set of a
    /// two-compartment model, as the dose that the coefficients `a` and `b` refer to).
    Schedule {
        /// The doses, at least one.
        doses: Vec<DoseEvent>,
    },
    /// `n_doses` doses `ModelInput::dose` at times 0, τ, 2τ…; times since the first dose
    /// (MOD-MD-07). τ shorter than the input duration is allowed (the inputs overlap).
    Regular {
        /// Interval τ, finite and > 0.
        tau: f64,
        /// Number of doses, 1 to [`MAX_REGULAR_DOSES`].
        n_doses: u32,
    },
    /// The limit of infinitely many doses `ModelInput::dose` every τ (MOD-MD-03 to 06); times since
    /// the last dose, in [0, τ]; τ at least the input duration (MOD-MD-12).
    SteadyState {
        /// Interval τ, finite and > 0.
        tau: f64,
    },
}

/// The reserved parameter names of a regimen.
const TAU: &str = "tau";
const N_DOSES: &str = "n_doses";
const DOSE_TIME: &str = "dose_time";
const DOSE_AMOUNT: &str = "dose_amount";
const DOSE_DUR: &str = "dose_dur";

/// The field and the index of a schedule name such as `dose_time[3]`.
fn indexed(name: &str) -> Option<(&'static str, &str)> {
    [DOSE_TIME, DOSE_AMOUNT, DOSE_DUR]
        .into_iter()
        .find_map(|field| {
            name.strip_prefix(field)
                .and_then(|rest| rest.strip_prefix('['))
                .and_then(|rest| rest.strip_suffix(']'))
                .map(|index| (field, index))
        })
}

/// Whether `name` is one of the reserved names of a regimen (`tau`, `n_doses`, `dose_time[i]`,
/// `dose_amount[i]`, `dose_dur[i]`).
pub fn is_regimen_parameter(name: &str) -> bool {
    name == TAU || name == N_DOSES || indexed(name).is_some()
}

fn invalid(problem: impl Into<String>) -> ModelError {
    ModelError::InvalidRegimen {
        problem: problem.into(),
    }
}

fn out_of_domain(name: &str, value: f64, domain: &str) -> ModelError {
    ModelError::RegimenOutOfDomain {
        name: name.to_string(),
        value,
        domain: domain.to_string(),
    }
}

/// τ: finite and > 0.
fn interval(value: f64) -> Result<f64, ModelError> {
    if value.is_finite() && value > 0.0 {
        Ok(value)
    } else {
        Err(out_of_domain(
            TAU,
            value,
            "a finite number > 0 (the dosing interval)",
        ))
    }
}

/// `n_doses`: an integer from 1 to [`MAX_REGULAR_DOSES`].
fn number_of_doses(value: f64) -> Result<u32, ModelError> {
    let ok = value.is_finite()
        && value.fract() == 0.0
        && (1.0..=f64::from(MAX_REGULAR_DOSES)).contains(&value);
    if !ok {
        return Err(out_of_domain(
            N_DOSES,
            value,
            &format!(
                "an integer from 1 to {MAX_REGULAR_DOSES} (the number of doses; use the steady state, `tau` alone, for more)"
            ),
        ));
    }
    // Exact: an integer between 1 and MAX_REGULAR_DOSES.
    Ok(value as u32)
}

impl Regimen {
    /// Reads and checks the regimen held in `params` (see the module documentation); the single
    /// dose when `params` holds none of its names. The model parameters are left to the model.
    pub fn from_params(params: &BTreeMap<String, f64>) -> Result<Regimen, ModelError> {
        let tau = params.get(TAU).copied();
        let n_doses = params.get(N_DOSES).copied();
        let mut records: BTreeMap<usize, [Option<f64>; 3]> = BTreeMap::new();
        for (name, &value) in params {
            let Some((field, index)) = indexed(name) else {
                continue;
            };
            let i: usize = index.parse().map_err(|_| {
                invalid(format!(
                    "`{name}`: the index of a dose must be a whole number from 0 (for example `{field}[0]`)"
                ))
            })?;
            let slot = match field {
                DOSE_TIME => 0,
                DOSE_AMOUNT => 1,
                _ => 2,
            };
            if let Some(entry) = records.entry(i).or_default().get_mut(slot) {
                *entry = Some(value);
            }
        }
        if records.is_empty() {
            return match (tau, n_doses) {
                (None, None) => Ok(Regimen::Single),
                (None, Some(_)) => Err(invalid(
                    "`n_doses` needs the interval `tau` between the doses; give `tau`",
                )),
                (Some(tau), None) => Ok(Regimen::SteadyState {
                    tau: interval(tau)?,
                }),
                (Some(tau), Some(n)) => {
                    let tau = interval(tau)?;
                    Ok(Regimen::Regular {
                        tau,
                        n_doses: number_of_doses(n)?,
                    })
                }
            };
        }
        if tau.is_some() || n_doses.is_some() {
            return Err(invalid(
                "give either a schedule (`dose_time[i]`, `dose_amount[i]`) or a regular regimen (`tau`, with `n_doses` for a finite number of doses), not both",
            ));
        }
        let mut doses = Vec::with_capacity(records.len());
        for (expected, (i, [time, amount, dur])) in records.into_iter().enumerate() {
            if i != expected {
                return Err(invalid(format!(
                    "dose {expected} of the schedule is missing: number the doses 0, 1, 2… without a gap (`dose_time[{expected}]`, `dose_amount[{expected}]`)"
                )));
            }
            let time = time.ok_or_else(|| {
                invalid(format!(
                    "dose {i} of the schedule has no time; give `dose_time[{i}]`"
                ))
            })?;
            let amount = amount.ok_or_else(|| {
                invalid(format!(
                    "dose {i} of the schedule has no amount; give `dose_amount[{i}]`"
                ))
            })?;
            if !time.is_finite() {
                return Err(out_of_domain(
                    &format!("{DOSE_TIME}[{i}]"),
                    time,
                    "a finite number (the time of that dose)",
                ));
            }
            if !(amount.is_finite() && amount >= 0.0) {
                return Err(out_of_domain(
                    &format!("{DOSE_AMOUNT}[{i}]"),
                    amount,
                    "a finite number >= 0 (the effective dose F x dose of that dose)",
                ));
            }
            if let Some(d) = dur
                && !(d.is_finite() && d > 0.0)
            {
                return Err(out_of_domain(
                    &format!("{DOSE_DUR}[{i}]"),
                    d,
                    "a finite number > 0 (the input duration of that dose)",
                ));
            }
            doses.push(DoseEvent { time, amount, dur });
        }
        Ok(Regimen::Schedule { doses })
    }

    /// The reserved parameters that encode this regimen (none for the single dose), the inverse
    /// of [`Regimen::from_params`].
    pub fn to_params(&self) -> Vec<(String, f64)> {
        let mut out = Vec::new();
        match self {
            Regimen::Single => {}
            Regimen::SteadyState { tau } => out.push((TAU.to_string(), *tau)),
            Regimen::Regular { tau, n_doses } => {
                out.push((TAU.to_string(), *tau));
                out.push((N_DOSES.to_string(), f64::from(*n_doses)));
            }
            Regimen::Schedule { doses } => {
                for (i, d) in doses.iter().enumerate() {
                    out.push((format!("{DOSE_TIME}[{i}]"), d.time));
                    out.push((format!("{DOSE_AMOUNT}[{i}]"), d.amount));
                    if let Some(dur) = d.dur {
                        out.push((format!("{DOSE_DUR}[{i}]"), dur));
                    }
                }
            }
        }
        out
    }
}

impl ModelInput {
    /// The checked regimen of this input ([`Regimen::Single`] when it has none).
    pub fn regimen(&self) -> Result<Regimen, ModelError> {
        Regimen::from_params(&self.params)
    }

    /// This input with `regimen` in place of the regimen it had (the model parameters are kept).
    pub fn with_regimen(mut self, regimen: &Regimen) -> ModelInput {
        self.params.retain(|name, _| !is_regimen_parameter(name));
        self.params.extend(regimen.to_params());
        self
    }

    /// Whether this input holds any regimen parameter (otherwise it is a plain single dose).
    pub fn has_regimen(&self) -> bool {
        self.params.keys().any(|name| is_regimen_parameter(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ModelId;

    fn params(p: &[(&str, f64)]) -> BTreeMap<String, f64> {
        p.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    #[test]
    fn every_regimen_round_trips_through_its_parameters() {
        let regimens = [
            Regimen::Single,
            Regimen::SteadyState { tau: 12.0 },
            Regimen::Regular {
                tau: 6.0,
                n_doses: 4,
            },
            Regimen::Schedule {
                doses: vec![
                    DoseEvent {
                        time: 4.0,
                        amount: 50.0,
                        dur: Some(1.5),
                    },
                    DoseEvent {
                        time: 0.0,
                        amount: 0.0,
                        dur: None,
                    },
                ],
            },
        ];
        for r in regimens {
            let input = ModelInput {
                model: ModelId::IvInfusion,
                dose: 100.0,
                params: params(&[("v", 10.0), ("cl", 2.0), ("dur", 2.0), ("tau", 3.0)]),
                times: vec![1.0],
            }
            .with_regimen(&r);
            assert_eq!(input.regimen().unwrap(), r);
            assert_eq!(input.has_regimen(), r != Regimen::Single);
            assert_eq!(input.params.get("v"), Some(&10.0));
            // And through JSON.
            let json = serde_json::to_string(&r).unwrap();
            assert_eq!(serde_json::from_str::<Regimen>(&json).unwrap(), r);
        }
    }

    #[test]
    fn malformed_regimens_are_refused_with_a_message_that_says_what_to_fix() {
        let cases: [(&[(&str, f64)], &str); 8] = [
            (&[("n_doses", 3.0)], "needs the interval `tau`"),
            (&[("tau", 6.0), ("n_doses", 2.5)], "`n_doses` = 2.5"),
            (&[("tau", 6.0), ("n_doses", 1e9)], "use the steady state"),
            (&[("dose_time[x]", 0.0)], "whole number"),
            (
                &[("dose_time[0]", 0.0), ("dose_amount[0]", 1.0), ("tau", 2.0)],
                "not both",
            ),
            (
                &[("dose_time[1]", 0.0), ("dose_amount[1]", 1.0)],
                "dose 0 of the schedule is missing",
            ),
            (&[("dose_time[0]", 0.0)], "`dose_amount[0]`"),
            (&[("dose_amount[0]", 1.0)], "`dose_time[0]`"),
        ];
        for (p, words) in cases {
            let e = Regimen::from_params(&params(p)).unwrap_err().to_string();
            assert!(e.contains(words), "{p:?}: {e}");
        }
    }
}
