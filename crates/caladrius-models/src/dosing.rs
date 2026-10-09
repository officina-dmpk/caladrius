//! Evaluation of a dosing regimen (`specs/models.md` section 12): a schedule or n regular doses by
//! superposition of the single-dose kernels (MOD-MD-01, 07), the steady state by the closed forms
//! of `steady` with the lag as a phase shift (MOD-MD-03 to 06), and the derived quantities of
//! MOD-MD-08 to 10.
//!
//! Every model of this crate is a positive combination of one-compartment kernels Φ(λ, s) of
//! volume 1 (MOD-2C-06): one compartment, C = Φ(k)/V; two compartments, C = [wα·Φ(α) + wβ·Φ(β)]/Vc.
//! A regimen is the same combination of the kernels of each dose (or of their steady-state sums).

use std::collections::BTreeMap;

use crate::kernel::Kernel;
use crate::model::{Input, ModelId};
use crate::regimen::{DoseEvent, Regimen, is_regimen_parameter};
use crate::steady::{Steady, bisect};
use crate::{ModelError, ModelInput, ModelOutput, params, params2};

/// The disposition as kernel terms: C = Σ weight·Φ(rate)/volume, and the input parameters.
struct Terms {
    input: Input,
    /// (weight, rate) of each exponential mode.
    modes: Vec<(f64, f64)>,
    volume: f64,
    cl: f64,
    ka: f64,
    dur: f64,
    tlag: f64,
}

impl Terms {
    fn new(model: ModelId, params: &BTreeMap<String, f64>, dose: f64) -> Result<Terms, ModelError> {
        let input = model.input();
        if model.compartments() == 2 {
            let p = params2::resolve(model, params, dose)?;
            Ok(Terms {
                input,
                modes: vec![(p.w_alpha, p.alpha), (p.w_beta, p.beta)],
                volume: p.vc,
                cl: p.cl,
                ka: p.ka,
                dur: p.dur,
                tlag: p.tlag,
            })
        } else {
            let p = params::resolve(model, params)?;
            Ok(Terms {
                input,
                modes: vec![(1.0, p.k)],
                volume: p.v,
                cl: p.cl,
                ka: p.ka,
                dur: p.dur,
                tlag: p.tlag,
            })
        }
    }

    fn combine(&self, each: impl Fn(f64) -> f64) -> f64 {
        self.modes
            .iter()
            .map(|&(w, rate)| w * each(rate))
            .sum::<f64>()
            / self.volume
    }

    /// Concentration and area of one dose at `t` on the clock of the dose times.
    fn dose_at(&self, dose: &DoseEvent, t: f64) -> (f64, f64) {
        let kernel = Kernel {
            input: self.input,
            dose: dose.amount,
            ka: self.ka,
            dur: dose.dur.unwrap_or(self.dur),
        };
        let s = (t - dose.time) - self.tlag;
        (
            self.combine(|rate| kernel.conc_auc(rate, s).0),
            self.combine(|rate| kernel.conc_auc(rate, s).1),
        )
    }

    fn steady(&self, dose: f64, tau: f64) -> Steady {
        Steady {
            input: self.input,
            dose,
            ka: self.ka,
            dur: self.dur,
            tau,
        }
    }
}

/// Secondary parameters of the single dose that describe one dose's profile, not the regimen.
const SINGLE_DOSE_ONLY: [&str; 7] = [
    "auc_inf",
    "aumc_inf",
    "mrt",
    "c0",
    "tmax_pred",
    "cmax_pred",
    "rate",
];

/// Evaluates an input that holds a regimen (MOD-MD-01 to 12).
pub(crate) fn run(input: &ModelInput) -> Result<ModelOutput, ModelError> {
    let regimen = input.regimen()?;
    let model = input.model;
    let id = model.id();
    let mut params: BTreeMap<String, f64> = input
        .params
        .iter()
        .filter(|(name, _)| !is_regimen_parameter(name))
        .map(|(name, value)| (name.clone(), *value))
        .collect();
    let kind = model.input();
    // A schedule carries the durations in its records; the model parameter `dur` is their default.
    let mut durations_in_records = false;
    if let Regimen::Schedule { doses } = &regimen {
        for (i, d) in doses.iter().enumerate() {
            match (kind, d.dur) {
                (Input::ZeroOrder, Some(_)) => durations_in_records = true,
                (Input::ZeroOrder, None) if !params.contains_key("dur") => {
                    return Err(ModelError::InvalidRegimen {
                        problem: format!(
                            "dose {i} of the schedule has no input duration, and model {id} needs one: give `dose_dur[{i}]` (or the model parameter `dur` for every dose)"
                        ),
                    });
                }
                (Input::Bolus | Input::FirstOrder, Some(_)) => {
                    return Err(ModelError::ParameterNotInModel {
                        name: format!("dose_dur[{i}]"),
                        model: id.to_string(),
                    });
                }
                _ => {}
            }
        }
        if kind == Input::ZeroOrder && !params.contains_key("dur") {
            if let Some(dur) = doses.iter().find_map(|d| d.dur) {
                params.insert("dur".to_string(), dur);
            }
        }
    }
    let dose = input.dose;
    if let Some(index) = input.times.iter().position(|t| !t.is_finite()) {
        return Err(ModelError::NonFiniteTime { index });
    }
    // The single dose of the same parameters: checks them, the dose and the times (MOD-GEN-04),
    // and gives the reference of the accumulation ratios (MOD-MD-08).
    let reference_times = match regimen {
        Regimen::SteadyState { tau } => {
            let mut times = input.times.clone();
            times.push(tau);
            times
        }
        _ => Vec::new(),
    };
    let single = crate::run(&ModelInput {
        model,
        dose,
        params: params.clone(),
        times: reference_times,
    })?;
    let terms = Terms::new(model, &params, dose)?;
    let mut secondary: BTreeMap<String, f64> = single
        .secondary()
        .iter()
        .filter(|(name, _)| !SINGLE_DOSE_ONLY.contains(&name.as_str()))
        .map(|(name, value)| (name.clone(), *value))
        .collect();
    if durations_in_records {
        secondary.remove("dur");
    }
    let (conc, auc, accum_c) = match &regimen {
        Regimen::Single => {
            // Not reached (a single dose has no regimen parameter); evaluated all the same.
            return crate::run(&ModelInput {
                model,
                dose,
                params,
                times: input.times.clone(),
            });
        }
        Regimen::Schedule { doses } => {
            let (conc, auc) = superpose(&terms, doses, &input.times);
            let total: f64 = doses.iter().map(|d| d.amount).sum();
            secondary.insert("auc_inf".to_string(), total / terms.cl);
            (conc, auc, Vec::new())
        }
        Regimen::Regular { tau, n_doses } => {
            let doses: Vec<DoseEvent> = (0..*n_doses)
                .map(|j| DoseEvent {
                    time: f64::from(j) * tau,
                    amount: dose,
                    dur: None,
                })
                .collect();
            let (conc, auc) = superpose(&terms, &doses, &input.times);
            secondary.insert("auc_inf".to_string(), f64::from(*n_doses) * dose / terms.cl);
            (conc, auc, Vec::new())
        }
        Regimen::SteadyState { tau } => steady_state(&terms, &single, input, *tau, &mut secondary)?,
    };
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
        aumc: Vec::new(),
        accum_c,
        secondary,
    })
}

/// MOD-MD-01: the sum over the doses of the single-dose concentrations and areas (each area from
/// its own dose time), at each time.
fn superpose(terms: &Terms, doses: &[DoseEvent], times: &[f64]) -> (Vec<f64>, Vec<f64>) {
    times
        .iter()
        .map(|&t| {
            doses
                .iter()
                .filter(|d| d.time <= t)
                .map(|d| terms.dose_at(d, t))
                .fold((0.0, 0.0), |(c, a), (dc, da)| (c + dc, a + da))
        })
        .unzip()
}

type Profiles = (Vec<f64>, Vec<f64>, Vec<Option<f64>>);

/// MOD-MD-03 to 10: the periodic profile at times since the last dose, its area from s = 0, the
/// accumulation by time, and the scalars of MOD-MD-11.
fn steady_state(
    terms: &Terms,
    single: &ModelOutput,
    input: &ModelInput,
    tau: f64,
    secondary: &mut BTreeMap<String, f64>,
) -> Result<Profiles, ModelError> {
    let dose = input.dose;
    if terms.input == Input::ZeroOrder && tau < terms.dur {
        return Err(ModelError::IntervalShorterThanInput {
            tau,
            dur: terms.dur,
        });
    }
    if let Some((index, &time)) = input
        .times
        .iter()
        .enumerate()
        .find(|(_, s)| !(0.0..=tau).contains(*s))
    {
        return Err(ModelError::TimeOutsideInterval { index, time, tau });
    }
    let steady = terms.steady(dose, tau);
    let conc_at = |u: f64| terms.combine(|rate| steady.conc(rate, u));
    let area = |u1: f64, du: f64| terms.combine(|rate| steady.area(rate, u1, du));
    // MOD-MD-05: the lag moves the phase; φ = tlag mod τ (exact), u = s − φ brought into [0, τ].
    let phi = terms.tlag % tau;
    let back = tau - phi;
    let phase = |s: f64| {
        if phi == 0.0 || s >= phi {
            s - phi
        } else {
            back + s
        }
    };
    let mut conc = Vec::with_capacity(input.times.len());
    let mut auc = Vec::with_capacity(input.times.len());
    let mut accum_c = Vec::with_capacity(input.times.len());
    for (i, &s) in input.times.iter().enumerate() {
        let c = conc_at(phase(s));
        conc.push(c);
        auc.push(if phi == 0.0 {
            area(0.0, s)
        } else if s <= phi {
            area(back, s)
        } else {
            area(back, phi) + area(0.0, s - phi)
        });
        // MOD-MD-08: R_C(s) = C_ss(s)/C_1(s), not available where the single dose gives 0.
        let first = single.conc().get(i).copied().unwrap_or(0.0);
        accum_c.push((first > 0.0).then(|| c / first));
    }

    // MOD-MD-10: the peak, at u* since the start of the last input, and the trough.
    let (peak, unique) = match terms.input {
        Input::Bolus => (0.0, true),
        Input::ZeroOrder => (terms.dur, tau > terms.dur),
        Input::FirstOrder => {
            let peaks: Vec<f64> = terms
                .modes
                .iter()
                .map(|&(_, rate)| steady.first_order_peak(rate))
                .collect();
            let lo = peaks.iter().copied().fold(f64::INFINITY, f64::min);
            let hi = peaks.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let slope = |u: f64| terms.combine(|rate| steady.slope(rate, u));
            (if hi > lo { bisect(lo, hi, slope) } else { lo }, true)
        }
    };
    let cmax = conc_at(peak);
    let trough = if terms.input == Input::Bolus {
        tau
    } else {
        0.0
    };
    let mut put = |name: &str, value: f64| {
        secondary.insert(name.to_string(), value);
    };
    put("cmax_ss", cmax);
    // A flat profile has no peak time: τ = T (a continuous infusion) or a zero dose (as the
    // single dose of two compartments, card T-032).
    if unique && dose > 0.0 {
        let shifted = peak + phi;
        put(
            "tmax_ss",
            if shifted >= tau {
                shifted - tau
            } else {
                shifted
            },
        );
    }
    put("cmin_ss", conc_at(trough));
    let auc_tau = dose / terms.cl;
    put("auc_tau_ss", auc_tau);
    put("cav_ss", auc_tau / tau);
    // MOD-MD-08: R_max = Cmax,ss/Cmax,1 and R_AUC = AUC_ss(0, τ)/AUC_1(0, τ), where defined.
    let single_cmax = match terms.input {
        Input::Bolus => single.get("c0"),
        _ => single.get("cmax_pred"),
    };
    if let Some(c1) = single_cmax.filter(|c| *c > 0.0) {
        put("accum_cmax", cmax / c1);
    }
    if let Some(a1) = single.auc().last().copied().filter(|a| *a > 0.0) {
        put("accum_auc", auc_tau / a1);
    }
    Ok((conc, auc, accum_c))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run as run_model;

    fn input(model: ModelId, p: &[(&str, f64)], times: &[f64]) -> ModelInput {
        ModelInput {
            model,
            dose: 100.0,
            params: p.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
            times: times.to_vec(),
        }
    }

    #[track_caller]
    fn printed(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() <= 0.5e-10 + 1e-12 * expected.abs(),
            "expected {expected}, got {actual}"
        );
    }

    /// Worked example P1 of `specs/models.md` section 12.2 (one compartment, τ = 6).
    #[test]
    fn p1_one_compartment_steady_state() {
        let ss = Regimen::SteadyState { tau: 6.0 };
        let bolus = run_model(
            &input(ModelId::IvBolus, &[("v", 10.0), ("cl", 2.0)], &[0.0, 6.0]).with_regimen(&ss),
        )
        .unwrap();
        printed(bolus.conc()[0], 14.3101276069);
        printed(bolus.get("cmin_ss").unwrap(), 4.3101276069);
        printed(bolus.get("accum_cmax").unwrap(), 1.4310127607);
        printed(bolus.get("cav_ss").unwrap(), 8.3333333333);
        assert_eq!(bolus.get("tmax_ss"), Some(0.0));
        let infusion = run_model(
            &input(
                ModelId::IvInfusion,
                &[("v", 10.0), ("cl", 2.0), ("dur", 2.0)],
                &[0.0, 1.0, 2.0, 4.0, 6.0],
            )
            .with_regimen(&ss),
        )
        .unwrap();
        for (c, e) in infusion.conc().iter().zip([
            5.2995680177,
            8.8706504872,
            11.7944055267,
            7.9060264556,
            5.2995680177,
        ]) {
            printed(*c, e);
        }
        printed(infusion.auc()[4], 50.0);
        let oral = run_model(
            &input(
                ModelId::Oral1,
                &[("v", 10.0), ("cl", 2.0), ("ka", 1.0)],
                &[0.0, 1.0, 2.0],
            )
            .with_regimen(&ss),
        )
        .unwrap();
        printed(oral.conc()[1], 10.0352570768);
        printed(oral.get("tmax_ss").unwrap(), 1.5669216549);
        printed(oral.get("cmax_ss").unwrap(), 10.4602585873);
        printed(oral.get("accum_cmax").unwrap(), 1.5641734930);
        printed(oral.get("accum_auc").unwrap(), 1.6022382033);
        let equal = run_model(
            &input(
                ModelId::Oral1,
                &[("v", 10.0), ("cl", 2.0), ("ka", 0.2)],
                &[0.0, 1.0],
            )
            .with_regimen(&ss),
        )
        .unwrap();
        printed(equal.conc()[0], 7.4014171269);
        printed(equal.get("tmax_ss").unwrap(), 2.4139234358);
        printed(equal.get("cmax_ss").unwrap(), 8.8302455575);
        assert!(crate::kernel::first_order_peak(0.2, 1.0) > oral.get("tmax_ss").unwrap());
    }

    /// Worked examples P3 (schedules) and P4 (lag at steady state).
    #[test]
    fn p3_schedules_and_p4_lag() {
        let schedule = Regimen::Schedule {
            doses: vec![
                DoseEvent {
                    time: 10.0,
                    amount: 100.0,
                    dur: None,
                },
                DoseEvent {
                    time: 0.0,
                    amount: 100.0,
                    dur: None,
                },
                DoseEvent {
                    time: 4.0,
                    amount: 50.0,
                    dur: None,
                },
            ],
        };
        let r = run_model(
            &input(ModelId::IvBolus, &[("v", 10.0), ("cl", 2.0)], &[12.0]).with_regimen(&schedule),
        )
        .unwrap();
        printed(r.conc()[0], 8.6198625832);
        printed(r.get("auc_inf").unwrap(), 125.0);
        let overlap = Regimen::Schedule {
            doses: vec![
                DoseEvent {
                    time: 0.0,
                    amount: 100.0,
                    dur: Some(3.0),
                },
                DoseEvent {
                    time: 2.0,
                    amount: 100.0,
                    dur: Some(3.0),
                },
            ],
        };
        let r = run_model(
            &input(ModelId::IvInfusion, &[("v", 10.0), ("cl", 2.0)], &[4.0]).with_regimen(&overlap),
        )
        .unwrap();
        printed(r.conc()[0], 11.6513623821);
        let ss = |tau: f64| Regimen::SteadyState { tau };
        let lag = run_model(
            &input(
                ModelId::Oral1Lag,
                &[("v", 10.0), ("cl", 2.0), ("ka", 1.0), ("tlag", 0.5)],
                &[0.0, 0.2, 0.5, 1.0],
            )
            .with_regimen(&ss(6.0)),
        )
        .unwrap();
        for (c, e) in
            lag.conc()
                .iter()
                .zip([5.9030730217, 5.6788852681, 5.3565981130, 8.5849507093])
        {
            printed(*c, e);
        }
        let long_lag = run_model(
            &input(
                ModelId::Oral1Lag,
                &[("v", 10.0), ("cl", 2.0), ("ka", 1.0), ("tlag", 8.5)],
                &[0.0],
            )
            .with_regimen(&ss(6.0)),
        )
        .unwrap();
        printed(long_lag.conc()[0], 8.5043435656);
        let zero = |tau: f64, tlag: f64| {
            run_model(
                &input(
                    ModelId::Oral0Lag,
                    &[("v", 10.0), ("cl", 2.0), ("dur", 2.0), ("tlag", tlag)],
                    &[0.0],
                )
                .with_regimen(&ss(tau)),
            )
            .unwrap()
            .conc()[0]
        };
        printed(zero(2.5, 1.0), 20.5207326577);
        printed(zero(3.0, 2.0), 16.7766769428);
        printed(zero(6.0, 1.0), 6.4729069939);
    }

    /// Worked example P2 (two compartments, τ = 12).
    #[test]
    fn p2_two_compartment_steady_state() {
        let p = [
            ("a", 50.0 / 9.0),
            ("b", 40.0 / 9.0),
            ("alpha", 1.0),
            ("beta", 0.1),
        ];
        let ss = Regimen::SteadyState { tau: 12.0 };
        let bolus =
            run_model(&input(ModelId::Pk2IvBolus, &p, &[0.0, 12.0]).with_regimen(&ss)).unwrap();
        printed(bolus.conc()[0], 11.9156464045);
        printed(bolus.conc()[1], 1.9156464045);
        let mut oral = p.to_vec();
        oral.push(("ka", 2.0));
        let r =
            run_model(&input(ModelId::Pk2Oral1, &oral, &[0.0, 1.0, 4.0, 12.0]).with_regimen(&ss))
                .unwrap();
        for (c, e) in r
            .conc()
            .iter()
            .zip([2.0165022370, 8.0084039847, 4.6858678966, 2.0165022370])
        {
            printed(*c, e);
        }
        // The true root (T-047 question 4): 0.9158084346, not the printed 0.9158084340.
        printed(r.get("tmax_ss").unwrap(), 0.9158084346);
        printed(r.get("cmax_ss").unwrap(), 8.0267804471);
    }

    #[test]
    fn regimen_errors_name_what_to_fix() {
        let ss = Regimen::SteadyState { tau: 1.5 };
        let e = run_model(
            &input(
                ModelId::IvInfusion,
                &[("v", 10.0), ("cl", 2.0), ("dur", 2.0)],
                &[1.0],
            )
            .with_regimen(&ss),
        )
        .unwrap_err()
        .to_string();
        assert!(e.contains("`tau` = 1.5") && e.contains("schedule"), "{e}");
        let e = run_model(
            &input(ModelId::IvBolus, &[("v", 10.0), ("cl", 2.0)], &[1.0, 2.0]).with_regimen(&ss),
        )
        .unwrap_err()
        .to_string();
        assert!(e.contains("time 2 = 2") && e.contains("tau = 1.5"), "{e}");
        let with_dur = Regimen::Schedule {
            doses: vec![DoseEvent {
                time: 0.0,
                amount: 1.0,
                dur: Some(1.0),
            }],
        };
        let e = run_model(
            &input(ModelId::IvBolus, &[("v", 10.0), ("cl", 2.0)], &[1.0]).with_regimen(&with_dur),
        )
        .unwrap_err()
        .to_string();
        assert!(e.contains("`dose_dur[0]`"), "{e}");
        let no_dur = Regimen::Schedule {
            doses: vec![DoseEvent {
                time: 0.0,
                amount: 1.0,
                dur: None,
            }],
        };
        let e = run_model(
            &input(ModelId::Oral0, &[("v", 10.0), ("cl", 2.0)], &[1.0]).with_regimen(&no_dur),
        )
        .unwrap_err()
        .to_string();
        assert!(e.contains("`dose_dur[0]`") && e.contains("`dur`"), "{e}");
    }

    /// A zero dose at steady state gives exact zeros, no peak time and no accumulation ratio.
    #[test]
    fn a_zero_dose_at_steady_state_is_flat() {
        let mut zero = input(
            ModelId::Pk2Oral1,
            &[
                ("cl", 2.0),
                ("vc", 10.0),
                ("q", 4.0),
                ("vp", 8.0),
                ("ka", 2.0),
            ],
            &[0.0, 3.0],
        )
        .with_regimen(&Regimen::SteadyState { tau: 12.0 });
        zero.dose = 0.0;
        let r = run_model(&zero).unwrap();
        assert_eq!(r.conc(), [0.0, 0.0]);
        assert_eq!(r.auc(), [0.0, 0.0]);
        assert_eq!(r.get("cmax_ss"), Some(0.0));
        for name in ["tmax_ss", "accum_cmax", "accum_auc", "accum_c[1]"] {
            assert_eq!(r.get(name), None, "{name}");
        }
    }

    /// A schedule of an input with a duration may take the model's `dur` for every dose.
    #[test]
    fn a_schedule_takes_the_model_duration_as_default() {
        let doses = |dur: Option<f64>| Regimen::Schedule {
            doses: vec![
                DoseEvent {
                    time: 0.0,
                    amount: 100.0,
                    dur,
                },
                DoseEvent {
                    time: 3.0,
                    amount: 50.0,
                    dur,
                },
            ],
        };
        let times = [1.0, 4.0, 9.0];
        let own = run_model(
            &input(ModelId::Oral0, &[("v", 10.0), ("cl", 2.0)], &times)
                .with_regimen(&doses(Some(2.0))),
        )
        .unwrap();
        let default = run_model(
            &input(
                ModelId::Oral0,
                &[("v", 10.0), ("cl", 2.0), ("dur", 2.0)],
                &times,
            )
            .with_regimen(&doses(None)),
        )
        .unwrap();
        assert_eq!(own.conc(), default.conc());
        assert_eq!(own.get("dur"), None);
        assert_eq!(default.get("dur"), Some(2.0));
    }
}

#[cfg(test)]
mod derivative_tests {
    use crate::{Derivatives, ModelError, ModelId, ModelInput, Regimen, jacobian};

    /// MOD-MD (card T-049): no closed-form derivative is derived for a regimen; `Analytic` is a
    /// readable refusal (never a silent fallback), forward differences give one column per model
    /// parameter and none for the regimen.
    #[test]
    fn analytic_derivatives_of_a_regimen_are_refused_readably() {
        let cases = [
            (ModelId::IvBolus, vec![("v", 10.0), ("cl", 2.0)]),
            (
                ModelId::Pk2Oral1,
                vec![
                    ("cl", 2.0),
                    ("vc", 10.0),
                    ("q", 4.0),
                    ("vp", 8.0),
                    ("ka", 2.0),
                ],
            ),
        ];
        for (model, p) in cases {
            let input = ModelInput {
                model,
                dose: 100.0,
                params: p.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
                times: vec![0.5, 3.0],
            }
            .with_regimen(&Regimen::SteadyState { tau: 6.0 });
            let e = jacobian(&input, Derivatives::Analytic).unwrap_err();
            assert!(
                matches!(&e, ModelError::DerivativesUnavailable { model: m, .. } if m == model.id()),
                "{e:?}"
            );
            let message = e.to_string();
            assert!(
                message.contains("unavailable")
                    && message.contains("regimen")
                    && message.contains("forward differences"),
                "{message}"
            );
            let j = jacobian(
                &input,
                Derivatives::ForwardDifference {
                    increment: crate::DEFAULT_INCREMENT,
                },
            )
            .unwrap();
            let names: Vec<&str> = p.iter().map(|(n, _)| *n).collect();
            let mut sorted = names.clone();
            sorted.sort_unstable();
            assert_eq!(j.parameters, sorted, "{model:?}");
            assert!(j.columns.iter().flatten().all(|x| x.is_finite()));
        }
    }
}
