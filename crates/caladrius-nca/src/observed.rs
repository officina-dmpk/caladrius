//! Observed parameters (`specs/nca.md` section 4) and C0 (NCA-IV-01). They are read from the
//! cleaned samples only, never from a point inserted at the dose time.

use crate::clean::{DOSE_TIME, ProfilePoint};
use crate::result::{NcReason, ParamValue};
use crate::{Route, TmaxTie};

/// Cmax, Tmax, Tfirst, Tlast and Clast of a cleaned profile.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Observed {
    pub cmax: ParamValue,
    pub tmax: ParamValue,
    pub tfirst: ParamValue,
    pub tlast: ParamValue,
    pub clast: ParamValue,
}

/// NCA-OBS-01, 02, 03, 06 and NCA-DAT-09 on the cleaned samples.
pub(crate) fn observed(points: &[ProfilePoint], tie: TmaxTie) -> Observed {
    let samples = || points.iter().filter(|p| p.is_sample());
    let Some(cmax) = samples().map(|p| p.conc).reduce(f64::max) else {
        let nc = ParamValue::nc(NcReason::NoDataAfterCleaning);
        return Observed {
            cmax: nc,
            tmax: nc,
            tfirst: nc,
            tlast: nc,
            clast: nc,
        };
    };
    let no_positive = ParamValue::nc(NcReason::NoPositiveConcentration);
    let at_max = |p: &&ProfilePoint| p.conc == cmax;
    let tmax = if cmax > 0.0 {
        let point = match tie {
            TmaxTie::First => samples().find(at_max),
            TmaxTie::Last => samples().rev().find(at_max),
        };
        point.map_or(no_positive, |p| ParamValue::of(p.time))
    } else {
        no_positive
    };
    let first_q = samples().find(|p| p.is_quantifiable());
    let last_q = samples().rev().find(|p| p.is_quantifiable());
    Observed {
        cmax: ParamValue::of(cmax),
        tmax,
        tfirst: first_q.map_or(no_positive, |p| ParamValue::of(p.time)),
        tlast: last_q.map_or(no_positive, |p| ParamValue::of(p.time)),
        // NCA-DAT-09: Clast is 0 when no concentration is quantifiable.
        clast: ParamValue::of(last_q.map_or(0.0, |p| p.conc)),
    }
}

/// C0 of the route (NCA-IV-01): for an IV bolus the first method that gives a value among
/// (1) the sample at the dose time if > 0, (2) log-linear back-extrapolation through the first two
/// post-dose samples when 0 < C2 < C1, (3) the first post-dose sample; 0 for the other routes.
pub(crate) fn c0(route: Route, points: &[ProfilePoint]) -> ParamValue {
    match route {
        Route::IvBolus => {}
        Route::Extravascular | Route::IvInfusion { .. } => return ParamValue::of(0.0),
    }
    let samples = || points.iter().filter(|p| p.is_sample());
    if let Some(at_dose) = samples().find(|p| p.time == DOSE_TIME && p.conc > 0.0) {
        return ParamValue::of(at_dose.conc);
    }
    let mut post_dose = samples().filter(|p| p.time > DOSE_TIME);
    let Some(first) = post_dose.next() else {
        return ParamValue::nc(NcReason::NoDataAfterCleaning);
    };
    if let Some(second) = post_dose.next() {
        if second.conc > 0.0 && second.conc < first.conc {
            let k = (first.conc / second.conc).ln() / (second.time - first.time);
            return ParamValue::of(first.conc * (k * (first.time - DOSE_TIME)).exp());
        }
    }
    ParamValue::of(first.conc)
}

/// Tlag (NCA-OBS-04, extravascular only): the time of the sample just before the first sample whose
/// concentration exceeds the one before it. Read on the samples before the BLQ policy, so a dropped
/// leading zero still marks the lag (T-012: PKNCA does the same).
pub(crate) fn tlag(route: Route, before_blq: &[ProfilePoint]) -> ParamValue {
    match route {
        Route::Extravascular => {}
        Route::IvBolus | Route::IvInfusion { .. } => {
            return ParamValue::nc(NcReason::NotApplicableToRoute);
        }
    }
    match before_blq.len() {
        0 => return ParamValue::nc(NcReason::NoDataAfterCleaning),
        1 => return ParamValue::nc(NcReason::SinglePoint),
        _ => {}
    }
    before_blq
        .windows(2)
        .find_map(|pair| match pair {
            [a, b] if b.conc > a.conc => Some(a.time),
            _ => None,
        })
        .map_or(ParamValue::nc(NcReason::NoRise), ParamValue::of)
}
