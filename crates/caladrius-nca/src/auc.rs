//! AUC and AUMC over the observed range (`specs/nca.md` section 5): AUClast, AUCall, AUMClast,
//! AUMCall, with the segment rule of the chosen method.

use crate::AucMethod;
use crate::clean::ProfilePoint;
use crate::result::{NcReason, ParamValue};

/// Areas from the start of the profile to Tlast, and to the first point after Tlast.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Areas {
    pub auclast: ParamValue,
    pub aucall: ParamValue,
    pub aumclast: ParamValue,
    pub aumcall: ParamValue,
}

impl Areas {
    fn all(value: ParamValue) -> Self {
        Self {
            auclast: value,
            aucall: value,
            aumclast: value,
            aumcall: value,
        }
    }
}

/// True when the segment from `(t1, c1)` to `(t2, c2)` uses the log formula under `method`
/// (NCA-AUC-04 to 07). `tmax` is the observed Tmax, used by `LinLog` only.
fn uses_log(method: AucMethod, tmax: Option<f64>, t2: f64, c1: f64, c2: f64) -> bool {
    let positive_and_different = c1 > 0.0 && c2 > 0.0 && c1 != c2;
    match method {
        AucMethod::Linear => false,
        AucMethod::LinUpLogDown => positive_and_different && c2 < c1,
        AucMethod::LinLog => positive_and_different && tmax.is_some_and(|tm| t2 > tm),
    }
}

/// Area under C and under t·C of one segment, by the linear (NCA-AUC-02, AUC-08) or log
/// (NCA-AUC-03, AUC-08) formula.
fn segment(log: bool, a: &ProfilePoint, b: &ProfilePoint) -> (f64, f64) {
    let (t1, c1, t2, c2) = (a.time, a.conc, b.time, b.conc);
    let dt = t2 - t1;
    // ln C1 - ln C2 rather than ln(C1/C2): the ratio itself may overflow. A difference that
    // rounds to 0 (C1, C2 adjacent floats) falls back to the linear limit.
    let ln_ratio = if log { c1.ln() - c2.ln() } else { 0.0 };
    if ln_ratio != 0.0 && ln_ratio.is_finite() {
        let k = ln_ratio / dt;
        (
            dt * (c1 - c2) / ln_ratio,
            (t1 * c1 - t2 * c2) / k + (c1 - c2) / (k * k),
        )
    } else {
        (dt * (c1 + c2) / 2.0, dt * (t1 * c1 + t2 * c2) / 2.0)
    }
}

/// Integrates `points` (cleaned, start point included, times strictly increasing).
///
/// `start_missing` is true when the profile has no concentration at the dose time (NCA-DAT-08);
/// `tlast` and `tmax` are the observed values, `None` when not calculated.
pub(crate) fn areas(
    points: &[ProfilePoint],
    method: AucMethod,
    start_missing: bool,
    tmax: Option<f64>,
    tlast: Option<f64>,
) -> Areas {
    if points.is_empty() {
        return Areas::all(ParamValue::nc(NcReason::NoDataAfterCleaning));
    }
    if start_missing {
        return Areas::all(ParamValue::nc(NcReason::NoStartConcentration));
    }
    // Count samples only: a point inserted at the dose time does not make a second observation.
    if points.iter().filter(|p| p.is_sample()).count() < 2 {
        return Areas::all(ParamValue::nc(NcReason::SinglePoint));
    }
    // No quantifiable concentration: zero area when every value is zero (NCA-DAT-09).
    let Some(end) = tlast.and_then(|t| points.iter().position(|p| p.is_sample() && p.time == t))
    else {
        return if points.iter().all(|p| p.conc == 0.0) {
            Areas::all(ParamValue::of(0.0))
        } else {
            Areas::all(ParamValue::nc(NcReason::NoPositiveConcentration))
        };
    };
    let (mut auc, mut aumc) = (0.0, 0.0);
    for pair in points.get(..=end).unwrap_or(&[]).windows(2) {
        let [a, b] = pair else {
            continue;
        };
        let (s_auc, s_aumc) = segment(uses_log(method, tmax, b.time, a.conc, b.conc), a, b);
        auc += s_auc;
        aumc += s_aumc;
    }
    // NCA-AUC-09: one more linear segment, to the first point after Tlast, if any.
    let (extra_auc, extra_aumc) = match (points.get(end), points.get(end + 1)) {
        (Some(a), Some(b)) => segment(false, a, b),
        _ => (0.0, 0.0),
    };
    Areas {
        auclast: ParamValue::of(auc),
        aucall: ParamValue::of(auc + extra_auc),
        aumclast: ParamValue::of(aumc),
        aumcall: ParamValue::of(aumc + extra_aumc),
    }
}
