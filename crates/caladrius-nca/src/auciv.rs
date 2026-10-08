//! PKNCA's IV bolus areas with a back-extrapolated start and the percent back-extrapolated
//! (`specs/nca.md` NCA-IV-02, IV-03; oracle cases `edge_iv*`).

use crate::auc::{segment, uses_log};
use crate::clean::{DOSE_TIME, PointOrigin, ProfilePoint};
use crate::result::{NcReason, ParamValue};
use crate::{AucMethod, Route};

/// The ordinary areas the IV areas are built on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Areas {
    pub auclast: ParamValue,
    pub aucall: ParamValue,
    pub aucinf_obs: ParamValue,
    pub aucinf_pred: ParamValue,
}

/// The extra area when the record at the dose time is replaced by C0 in the first segment
/// (NCA-IV-02): segment from (t_d, C0) minus segment from the record, both by the lin-up/log-down rule.
fn first_segment_change(profile: &[ProfilePoint], c0: ParamValue) -> ParamValue {
    // A record at the dose time is required; a point inserted by the start policy is not one.
    let Some(record) = profile
        .first()
        .filter(|p| p.is_sample() && p.time == DOSE_TIME)
    else {
        return ParamValue::nc(NcReason::NoStartConcentration);
    };
    let Some(next) = profile.get(1) else {
        return ParamValue::nc(NcReason::SinglePoint);
    };
    let Some(c0) = c0.value() else {
        return c0;
    };
    let start = ProfilePoint {
        index: None,
        time: DOSE_TIME,
        conc: c0,
        origin: PointOrigin::InsertedStart,
    };
    // Both first segments use the lin-up/log-down rule whatever the AUC method (T-013: PKNCA does
    // so, `edge_iv_linear`; with an observed C0 the change is then exactly 0).
    let first = |a: &ProfilePoint| {
        segment(
            uses_log(AucMethod::LinUpLogDown, None, next.time, a.conc, next.conc),
            a,
            next,
        )
        .0
    };
    let (from_c0, from_record) = (first(&start), first(record));
    ParamValue::of(from_c0 - from_record)
}

/// `aucivlast`, `aucivall`, `aucivinf.obs/pred` and the matching `aucivpbext*` percentages
/// 100·(1 − AUC/AUC_iv) (NCA-IV-03). IV bolus only.
pub(crate) fn iv_areas(
    route: Route,
    profile: &[ProfilePoint],
    c0: ParamValue,
    areas: &Areas,
) -> Vec<(&'static str, ParamValue)> {
    let names = [
        ("aucivlast", "aucivpbextlast", areas.auclast),
        ("aucivall", "aucivpbextall", areas.aucall),
        ("aucivinf.obs", "aucivpbextinf.obs", areas.aucinf_obs),
        ("aucivinf.pred", "aucivpbextinf.pred", areas.aucinf_pred),
    ];
    let change = match route {
        Route::IvBolus => first_segment_change(profile, c0),
        Route::Extravascular | Route::IvInfusion { .. } => {
            ParamValue::nc(NcReason::NotApplicableToRoute)
        }
    };
    let mut out = Vec::with_capacity(2 * names.len());
    for (iv_name, _, area) in names {
        out.push((iv_name, change.zip(area, |d, a| ParamValue::of(a + d))));
    }
    for (_, pct_name, area) in names {
        let iv = change.zip(area, |d, a| ParamValue::of(a + d));
        let pct = iv.zip(area, |iv, a| {
            if iv > 0.0 {
                ParamValue::of(100.0 * (1.0 - a / iv))
            } else {
                ParamValue::nc(NcReason::NonPositiveArea)
            }
        });
        out.push((pct_name, pct));
    }
    out
}
