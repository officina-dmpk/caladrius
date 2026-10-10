//! MOD-MD-01: the superposition of the single-dose kernels over a schedule or a regular regimen,
//! with an early exit for the doses that no longer change the sum (card T-051).
//!
//! Every model is a positive combination C = Σ w·Φ(λ, s)/V of one-compartment kernels of volume 1
//! (`dosing::Terms`), s the time since the start of the dose's input. Per unit dose, each kernel
//! has an upper bound B(λ, s) and its tail ∫ₛ^∞ Φ an upper bound B_tail(λ, s), both from the rates
//! of the model and non-increasing in s (from s = 1/m for the first-order B):
//!
//! - bolus: Φ = e^(−λs) ≤ B = e^(−λs), B_tail = e^(−λs)/λ;
//! - zero-order input of duration T ≤ T_max (infusion): Φ ≤ B = e^(−λ(s − T_max)), since
//!   (1 − e^−x)/x ≤ 1 during the input and Φ(T)·e^(−λ(s − T)) after it; B_tail = B/λ;
//! - first-order input: Φ = ka·(e^(−λs) − e^(−ka·s))/(ka − λ) = ka·s·e^(−ξs) for some ξ between
//!   λ and ka (mean value theorem; ka·s·e^(−λs) at ka = λ), so Φ ≤ B = ka·s·e^(−ms) with
//!   m = min(λ, ka), which decreases for s ≥ 1/m; B_tail = ka·e^(−ms)·(s/m + 1/m²).
//!
//! At each time the doses are visited from the newest back. When the doses not yet visited (total
//! amount R, all given no later than the current one, whose elapsed time is s) satisfy
//! R·Σ|w|·B(λ, s)/V < 2⁻⁶² of the concentration already summed and R·Σ|w|·B_tail(λ, s)/V < 2⁻⁶²
//! of the area already summed, their concentrations are dropped and their areas replaced by the
//! area to infinity R·Σ w/(λ·V) (each dose's area is its area to infinity minus its tail). Both
//! changes are below 2⁻⁶² of the result, a thousandth of its last bit (2⁻⁵²): the bound is
//! conservative, never an approximation above rounding. The doses visited are then added oldest
//! first, starting from that area, which is the order of the plain sum: with nothing dropped, the
//! result is the plain sum bit for bit. When the remaining amount is 0, or both bounds are exactly
//! 0 (their exponential has underflowed, and so has the kernel's), the remaining concentrations are
//! exactly 0 and nothing is compared.
//!
//! The test is made at the first dose visited and then every `CHECK_EVERY` = 8 doses, not at each:
//! it costs about as much as one dose (one exponential per mode). Any spacing keeps the bound
//! conservative: a drop is still made only where the criterion above holds, at the dose where it
//! is tested, with the amount and elapsed time of that dose; testing less often only delays a drop
//! by at most 7 doses, which are then summed exactly.
//!
//! Golden rule 6 (never hang): the evaluations of one dose at one time are counted, and the
//! superposition is refused readably as soon as the count exceeds [`MAX_DOSE_EVALUATIONS`] = 10⁷.
//! The rule is exact: a regimen is refused if and only if, after the early exit, it needs more than
//! 10⁷ evaluations; since each (dose, time) pair is evaluated at most once, a regimen with
//! doses × times ≤ 10⁷ is never refused, and beyond it the early exit usually keeps the count far
//! below the limit. The count is checked at each evaluation, so the refusal comes after at most
//! 10⁷ evaluations, about 2 s in a release build (card T-052b).

use crate::ModelError;
use crate::dosing::Terms;
use crate::model::Input;
use crate::regimen::DoseEvent;

/// Largest number of evaluations of one dose at one time in one superposition (the doses that
/// still contribute, summed over the times); every regimen with doses × times at most this limit
/// is evaluated. 10⁷ rather than the 10⁸ of card T-051, where a regimen whose doses never decay
/// within the times waited 22 s in a release build before the refusal (two-compartment first-order
/// input): at 10⁷ the refusal comes within about 2 s, and 10,000 doses × 1,000 times still pass
/// whatever the rates (card T-052b).
pub const MAX_DOSE_EVALUATIONS: u64 = 10_000_000;

/// The early exit is tested at the first dose visited and then every `CHECK_EVERY` doses (module
/// documentation): the bound is computed on one dose in eight, for at most seven extra
/// evaluations per time once the remaining doses could be dropped.
const CHECK_EVERY: usize = 8;

/// The share of the running sums below which the doses not yet visited are dropped: 2⁻⁶², a
/// thousandth of the last bit of a double (2⁻⁵²).
const NEGLIGIBLE: f64 = 1.0 / 4_611_686_018_427_387_904.0;

/// Upper bounds, per unit dose, of the concentration of one dose and of its area after s.
struct Bound {
    input: Input,
    ka: f64,
    /// The longest input duration among the doses (zero-order input only).
    dur_max: f64,
    /// (|weight|/volume, rate) of each mode.
    modes: Vec<(f64, f64)>,
    /// The elapsed time from which the concentration bound does not increase.
    settle: f64,
}

impl Bound {
    fn new(terms: &Terms, doses: &[DoseEvent]) -> Bound {
        let modes: Vec<(f64, f64)> = terms
            .modes
            .iter()
            .map(|&(w, rate)| (w.abs() / terms.volume, rate))
            .collect();
        let settle = match terms.input {
            Input::FirstOrder => modes
                .iter()
                .map(|&(_, rate)| 1.0 / rate.min(terms.ka))
                .fold(0.0, f64::max),
            Input::Bolus | Input::ZeroOrder => 0.0,
        };
        let dur_max = doses
            .iter()
            .map(|d| d.dur.unwrap_or(terms.dur))
            .fold(terms.dur, f64::max);
        Bound {
            input: terms.input,
            ka: terms.ka,
            dur_max,
            modes,
            settle,
        }
    }

    /// (B, B_tail) of the module documentation, summed over the modes, at elapsed time `s`.
    fn at(&self, s: f64) -> (f64, f64) {
        self.modes
            .iter()
            .fold((0.0, 0.0), |(c, a), &(scale, rate)| {
                let (b, tail) = match self.input {
                    Input::Bolus => {
                        let b = (-rate * s).exp();
                        (b, b / rate)
                    }
                    Input::ZeroOrder => {
                        let b = (-rate * (s - self.dur_max)).exp();
                        (b, b / rate)
                    }
                    Input::FirstOrder => {
                        let m = rate.min(self.ka);
                        let decay = (-m * s).exp();
                        if decay == 0.0 {
                            (0.0, 0.0)
                        } else {
                            (
                                self.ka * s * decay,
                                self.ka * decay * (s / m + 1.0 / (m * m)),
                            )
                        }
                    }
                };
                (c + scale * b, a + scale * tail)
            })
    }

    /// Whether the doses not yet visited, of total amount `remaining`, the newest of them at
    /// elapsed time `s`, can be dropped from the sums `conc` and `area` of the doses visited.
    fn exhausted(&self, s: f64, remaining: f64, conc: f64, area: f64) -> bool {
        if remaining == 0.0 {
            return true;
        }
        if s < self.settle {
            return false;
        }
        let (b, tail) = self.at(s);
        if b == 0.0 && tail == 0.0 {
            return true;
        }
        remaining * b < NEGLIGIBLE * conc && remaining * tail < NEGLIGIBLE * area
    }
}

/// MOD-MD-01: at each time, the sum over the doses given by then of the single-dose
/// concentrations and areas (each area from its own dose time), with the early exit of the module
/// documentation.
pub(crate) fn superpose(
    terms: &Terms,
    doses: &[DoseEvent],
    times: &[f64],
) -> Result<(Vec<f64>, Vec<f64>), ModelError> {
    superpose_counted(terms, doses, times, MAX_DOSE_EVALUATIONS).map(|(conc, auc, _)| (conc, auc))
}

type Counted = (Vec<f64>, Vec<f64>, u64);

/// [`superpose`] with a given limit, and the number of evaluations of one dose at one time.
fn superpose_counted(
    terms: &Terms,
    doses: &[DoseEvent],
    times: &[f64],
    limit: u64,
) -> Result<Counted, ModelError> {
    let mut sorted = doses.to_vec();
    // Stable: doses at the same time keep their order (that of the plain sum).
    sorted.sort_by(|a, b| a.time.total_cmp(&b.time));
    // `before[j]`: the amount of the doses before dose j (j = 0..=n).
    let before: Vec<f64> = std::iter::once(0.0)
        .chain(sorted.iter().scan(0.0, |sum, d| {
            *sum += d.amount;
            Some(*sum)
        }))
        .collect();
    let bound = Bound::new(terms, &sorted);
    let mut conc = Vec::with_capacity(times.len());
    let mut auc = Vec::with_capacity(times.len());
    let mut visited: Vec<(f64, f64)> = Vec::new();
    let mut evaluations: u64 = 0;
    for &t in times {
        let given = sorted.partition_point(|d| d.time <= t);
        visited.clear();
        let (mut c_sum, mut a_sum) = (0.0, 0.0);
        let mut dropped = 0.0;
        for (j, dose) in sorted.iter().take(given).enumerate().rev() {
            let s = (t - dose.time) - terms.tlag;
            if visited.len() % CHECK_EVERY == 0 {
                let remaining = before.get(j + 1).copied().unwrap_or(f64::INFINITY);
                if bound.exhausted(s, remaining, c_sum, a_sum) {
                    dropped = remaining;
                    break;
                }
            }
            evaluations += 1;
            if evaluations > limit {
                return Err(ModelError::SuperpositionTooLarge {
                    doses: doses.len(),
                    times: times.len(),
                    limit,
                });
            }
            let (c, a) = terms.dose_at(dose, t);
            c_sum += c;
            a_sum += a;
            visited.push((c, a));
        }
        let start = if dropped > 0.0 {
            terms.combine(|rate| dropped / rate)
        } else {
            0.0
        };
        let (c, a) = visited
            .iter()
            .rev()
            .fold((0.0, start), |(c, a), &(dc, da)| (c + dc, a + da));
        conc.push(c);
        auc.push(a);
    }
    Ok((conc, auc, evaluations))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::Instant;

    use caladrius_testkit::Tolerance;

    use super::*;
    use crate::{ModelId, ModelInput, Regimen, run};

    fn params(p: &[(&str, f64)]) -> BTreeMap<String, f64> {
        p.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    fn terms(model: ModelId, p: &[(&str, f64)]) -> Terms {
        Terms::new(model, &params(p), 100.0).unwrap()
    }

    /// The plain sum of T-049, kept here as the reference: every dose given by `t`, in record
    /// order.
    fn plain(terms: &Terms, doses: &[DoseEvent], times: &[f64]) -> (Vec<f64>, Vec<f64>) {
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

    fn regular(n: usize, tau: f64, amount: f64) -> Vec<DoseEvent> {
        (0..n)
            .map(|j| DoseEvent {
                time: tau * j as f64,
                amount,
                dur: None,
            })
            .collect()
    }

    fn grid(n: usize, end: f64) -> Vec<f64> {
        (0..=n).map(|i| end * i as f64 / n as f64).collect()
    }

    /// Every model of the crate, with fast and slow rates, ka near the elimination rate, a lag, an
    /// interval shorter than the input, unsorted records with zero amounts: the early exit gives
    /// the plain sum at `MODEL_VALUES`, exact zeros where the plain sum is 0, and skips doses for
    /// each of the 8 models (card T-052b: one model losing its exit fails the test).
    #[test]
    fn the_early_exit_gives_the_plain_sum() {
        let pk2 = [("cl", 2.0), ("vc", 10.0), ("q", 4.0), ("vp", 8.0)];
        let with = |extra: &[(&'static str, f64)]| {
            let mut p = pk2.to_vec();
            p.extend_from_slice(extra);
            p
        };
        let cases: Vec<(ModelId, Vec<(&str, f64)>)> = vec![
            (ModelId::IvBolus, vec![("v", 10.0), ("cl", 2.0)]),
            (ModelId::IvBolus, vec![("v", 10.0), ("cl", 20.0)]),
            (
                ModelId::IvInfusion,
                vec![("v", 10.0), ("cl", 2.0), ("dur", 30.0)],
            ),
            (ModelId::Oral1, vec![("v", 10.0), ("cl", 2.0), ("ka", 1.0)]),
            (ModelId::Oral1, vec![("v", 10.0), ("cl", 2.0), ("ka", 0.2)]),
            (ModelId::Oral1, vec![("v", 10.0), ("cl", 2.0), ("ka", 0.05)]),
            (
                ModelId::Oral1Lag,
                vec![("v", 10.0), ("cl", 2.0), ("ka", 1.0), ("tlag", 7.5)],
            ),
            (
                ModelId::Oral0Lag,
                vec![("v", 10.0), ("cl", 2.0), ("dur", 2.0), ("tlag", 0.5)],
            ),
            (ModelId::Pk2IvBolus, with(&[])),
            (ModelId::Pk2IvInfusion, with(&[("dur", 1.5)])),
            (ModelId::Pk2Oral1, with(&[("ka", 2.0)])),
            (ModelId::Pk2Oral1, with(&[("ka", 0.1)])),
        ];
        let mut schedule = regular(60, 6.0, 100.0);
        schedule.reverse();
        if let Some(d) = schedule.get_mut(7) {
            d.amount = 0.0;
        }
        schedule.push(DoseEvent {
            time: 500.0,
            amount: 50.0,
            dur: None,
        });
        let times = grid(1000, 700.0);
        let mut skipped: Vec<ModelId> = Vec::new();
        for (model, p) in &cases {
            let terms = terms(*model, p);
            for doses in [regular(100, 4.0, 100.0), schedule.clone()] {
                let (c, a, n) = superpose_counted(&terms, &doses, &times, u64::MAX).unwrap();
                let (pc, pa) = plain(&terms, &doses, &times);
                // The pairs (dose, time) of the plain sum: fewer evaluations means a drop.
                let pairs: usize = times
                    .iter()
                    .map(|&t| doses.iter().filter(|d| d.time <= t).count())
                    .sum();
                if n < pairs as u64 && !skipped.contains(model) {
                    skipped.push(*model);
                }
                for (i, (x, y)) in c.iter().zip(&pc).chain(a.iter().zip(&pa)).enumerate() {
                    assert!(
                        Tolerance::MODEL_VALUES.accepts(*x, *y),
                        "{model:?} {i}: {x} against the plain sum {y}"
                    );
                }
            }
        }
        let mut models: Vec<ModelId> = Vec::new();
        for (model, _) in &cases {
            if !models.contains(model) {
                models.push(*model);
            }
        }
        assert_eq!(models.len(), 8);
        for model in &models {
            assert!(
                skipped.contains(model),
                "{model:?}: the early exit never fired"
            );
        }
    }

    /// Long after the last dose everything has underflowed: exact zeros, the area to infinity,
    /// and one bound evaluation per time instead of one per dose.
    #[test]
    fn exhausted_doses_give_zero_and_their_whole_area() {
        let terms = terms(ModelId::IvBolus, &[("v", 10.0), ("cl", 2.0)]);
        let doses = regular(1000, 1.0, 100.0);
        let (c, a, n) = superpose_counted(&terms, &doses, &[1e6], u64::MAX).unwrap();
        assert_eq!(c, [0.0]);
        assert!(Tolerance::MODEL_VALUES.accepts(a[0], 1000.0 * 100.0 / 2.0));
        assert_eq!(n, 0);
    }

    /// Golden rule 6: more evaluations than the limit is a readable refusal, not a long run.
    #[test]
    fn too_many_evaluations_are_refused_readably() {
        let terms = terms(ModelId::IvBolus, &[("v", 10.0), ("cl", 2e-6)]);
        let doses = regular(100, 1.0, 100.0);
        let times = grid(100, 200.0);
        assert!(superpose_counted(&terms, &doses, &times, 10_000_000).is_ok());
        let e = superpose_counted(&terms, &doses, &times, 1000)
            .unwrap_err()
            .to_string();
        assert!(
            e.contains("100 doses at 101 times")
                && e.contains("more than 1000")
                && e.contains("fewer times"),
            "{e}"
        );
        // Through `run`, with the crate's limit: within doses x times, never refused.
        let input = ModelInput {
            model: ModelId::IvBolus,
            dose: 100.0,
            params: params(&[("v", 10.0), ("cl", 2e-6)]),
            times,
        }
        .with_regimen(&Regimen::Regular {
            tau: 1.0,
            n_doses: 100,
        });
        assert!(run(&input).is_ok());
    }

    /// Card T-052b: with doses that never decay within the times, the refusal at the crate's limit
    /// comes after `MAX_DOSE_EVALUATIONS` evaluations. Ignored in the default suite (seconds in a
    /// debug build); release timing:
    /// `cargo test -p caladrius-models --release --lib -- --ignored refusal_at --nocapture`.
    #[test]
    #[ignore = "timing figure, run in release"]
    fn refusal_at_the_crate_limit_is_prompt() {
        let n: u32 = 10_000;
        let tau = 12.0;
        let last = tau * f64::from(n);
        let count = MAX_DOSE_EVALUATIONS / u64::from(n) + 1;
        let times: Vec<f64> = (0..count).map(|i| last + i as f64).collect();
        let input = ModelInput {
            model: ModelId::Pk2Oral1,
            dose: 100.0,
            params: params(&[
                ("cl", 2e-5),
                ("vc", 10.0),
                ("q", 4.0),
                ("vp", 8.0),
                ("ka", 2.0),
            ]),
            times,
        }
        .with_regimen(&Regimen::Regular { tau, n_doses: n });
        let start = Instant::now();
        let e = run(&input).unwrap_err();
        eprintln!("refused after {:?}: {e}", start.elapsed());
        assert!(matches!(e, ModelError::SuperpositionTooLarge { .. }));
    }

    /// Card T-051: 10,000 doses at 10,000 times, 26.6 s (pk2 regular) and 13.1 s (pk1 schedule)
    /// with the plain sum in a release build. Card T-052b: the gate is the evaluation count, which
    /// is deterministic, not the wall clock of a possibly loaded runner: fewer than a hundredth of
    /// the 10⁸ (dose, time) pairs. The time is printed in a release build only:
    /// `cargo test -p caladrius-models --release --lib -- superposition_of --nocapture`.
    #[test]
    fn superposition_of_10000_doses_at_10000_times_evaluates_few_doses() {
        let n = 10_000;
        let tau = 12.0;
        let times: Vec<f64> = (0..n).map(|i| tau * i as f64).collect();
        let doses = regular(n, tau, 100.0);
        let pk2 = terms(
            ModelId::Pk2Oral1,
            &[
                ("cl", 2.0),
                ("vc", 10.0),
                ("q", 4.0),
                ("vp", 8.0),
                ("ka", 2.0),
            ],
        );
        let pk1 = terms(ModelId::Oral1, &[("v", 10.0), ("cl", 2.0), ("ka", 1.0)]);
        let pairs = (n * n) as u64;
        for (name, terms) in [("pk2.oral_1", pk2), ("pk1.oral_1", pk1)] {
            let start = Instant::now();
            let (c, _, count) = superpose_counted(&terms, &doses, &times, u64::MAX).unwrap();
            let elapsed = start.elapsed();
            assert_eq!(c.len(), n);
            assert!(count < pairs / 100, "{name}: {count} evaluations");
            if cfg!(not(debug_assertions)) {
                eprintln!("{name}: {count} evaluations, {elapsed:?}");
            }
        }
        // Through `run`, with the crate's limit.
        let input = ModelInput {
            model: ModelId::Pk2Oral1,
            dose: 100.0,
            params: params(&[
                ("cl", 2.0),
                ("vc", 10.0),
                ("q", 4.0),
                ("vp", 8.0),
                ("ka", 2.0),
            ]),
            times,
        }
        .with_regimen(&Regimen::Regular {
            tau,
            n_doses: 10_000,
        });
        assert_eq!(run(&input).unwrap().conc().len(), n);
    }
}
