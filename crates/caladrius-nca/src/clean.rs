//! Data cleaning (`specs/nca.md` section 3): points before the dose, missing values, negative
//! values, the BLQ policy, then the concentration at the dose time. The result is the profile that
//! is integrated, with the reason for every point removed or changed.

use serde::{Deserialize, Serialize};

use crate::{BlqAction, BlqPolicy, MissingPolicy, NcaInput, NegativePolicy};

/// Dose time. All times of the input and of the results are relative to it (NCA-DAT-01).
pub(crate) const DOSE_TIME: f64 = 0.0;

/// Where a point of the cleaned profile comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PointOrigin {
    /// The observed value, unchanged.
    Observed,
    /// A missing value replaced under [`MissingPolicy::Replace`].
    MissingReplaced,
    /// A negative value replaced by 0 under [`NegativePolicy::SetZero`] and kept by the BLQ policy.
    NegativeSetZero,
    /// A BLQ value replaced under [`BlqAction::Set`]. Never quantifiable: never Tfirst, Tlast or
    /// Clast, never a λz point (NCA-LZ-02b).
    BlqSet,
    /// Added at the dose time by the start policy (NCA-DAT-08); not an observation.
    InsertedStart,
}

/// One point of the profile actually integrated.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ProfilePoint {
    /// Position in the input, `None` for an inserted point.
    pub index: Option<usize>,
    /// Time.
    pub time: f64,
    /// Concentration used.
    pub conc: f64,
    /// Where the value comes from.
    pub origin: PointOrigin,
}

impl ProfilePoint {
    /// True for a sample of the input (as opposed to a point added by the start policy).
    pub fn is_sample(&self) -> bool {
        self.origin != PointOrigin::InsertedStart
    }

    /// True when the point is a quantified concentration: a sample with C > 0 that was not BLQ.
    pub fn is_quantifiable(&self) -> bool {
        self.conc > 0.0
            && !matches!(
                self.origin,
                PointOrigin::BlqSet | PointOrigin::InsertedStart
            )
    }
}

/// Why an input point is not in the integrated profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemovalReason {
    /// Its time is before the dose time.
    BeforeDose,
    /// Its concentration is missing (NaN) under [`MissingPolicy::Drop`].
    Missing,
    /// It is BLQ and its class has the action [`BlqAction::Drop`].
    Blq,
}

/// An input point left out of the integrated profile.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RemovedPoint {
    /// Position in the input.
    pub index: usize,
    /// Its time.
    pub time: f64,
    /// Why it was removed.
    pub reason: RemovalReason,
}

/// The cleaned samples, before the start policy.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Cleaned {
    pub points: Vec<ProfilePoint>,
    pub removed: Vec<RemovedPoint>,
}

/// Applies NCA-DAT-01, 03, 04, 06 in that order. The input must have passed validation.
pub(crate) fn clean(input: &NcaInput) -> Cleaned {
    let options = &input.options;
    let mut out = Cleaned::default();
    for (index, (&time, &conc)) in input.time.iter().zip(&input.conc).enumerate() {
        let removed = |reason| RemovedPoint {
            index,
            time,
            reason,
        };
        if time < DOSE_TIME {
            out.removed.push(removed(RemovalReason::BeforeDose));
            continue;
        }
        let (conc, origin) = if conc.is_nan() {
            match options.missing {
                MissingPolicy::Drop => {
                    out.removed.push(removed(RemovalReason::Missing));
                    continue;
                }
                MissingPolicy::Replace(value) => (value, PointOrigin::MissingReplaced),
            }
        } else if conc < 0.0 && options.negative == NegativePolicy::SetZero {
            (0.0, PointOrigin::NegativeSetZero)
        } else {
            // A negative value here is allowed by the policy (validation refused it otherwise).
            (conc, PointOrigin::Observed)
        };
        out.points.push(ProfilePoint {
            index: Some(index),
            time,
            conc,
            origin,
        });
    }
    apply_blq(options.blq, out)
}

/// Classifies the BLQ (zero) points and applies the action of their class (NCA-DAT-06).
fn apply_blq(policy: BlqPolicy, cleaned: Cleaned) -> Cleaned {
    let Cleaned {
        points,
        mut removed,
    } = cleaned;
    let first_q = points.iter().find(|p| p.conc > 0.0).map(|p| p.time);
    let last_q = points.iter().rev().find(|p| p.conc > 0.0).map(|p| p.time);
    let cmax = points
        .iter()
        .map(|p| p.conc)
        .fold(f64::NEG_INFINITY, f64::max);
    // Time of the first maximum, when the maximum is positive.
    let tmax = points
        .iter()
        .find(|p| cmax > 0.0 && p.conc == cmax)
        .map(|p| p.time);
    let action_for = |time: f64| -> BlqAction {
        match policy {
            BlqPolicy::Position {
                first,
                middle,
                last,
            } => match (first_q, last_q) {
                (Some(f), Some(l)) if time > f && time < l => middle,
                (Some(_), Some(l)) if time > l => last,
                _ => first,
            },
            BlqPolicy::Tmax { before, after } => match tmax {
                Some(t) if time > t => after,
                _ => before,
            },
        }
    };
    let mut kept = Vec::with_capacity(points.len());
    for point in points {
        if point.conc != 0.0 {
            kept.push(point);
            continue;
        }
        match action_for(point.time) {
            BlqAction::Keep => kept.push(point),
            BlqAction::Drop => {
                // Every point here is a sample; one without a position is kept rather than
                // reported under a wrong index.
                let Some(index) = point.index else {
                    kept.push(point);
                    continue;
                };
                removed.push(RemovedPoint {
                    index,
                    time: point.time,
                    reason: RemovalReason::Blq,
                });
            }
            BlqAction::Set(value) => kept.push(ProfilePoint {
                conc: value,
                origin: PointOrigin::BlqSet,
                ..point
            }),
        }
    }
    removed.sort_by_key(|r| r.index);
    Cleaned {
        points: kept,
        removed,
    }
}
