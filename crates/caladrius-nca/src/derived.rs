//! Parameters derived from the dose and the areas (`specs/nca.md` NCA-EXT-04 to 08, NCA-OBS-05,
//! NCA-DAT-10): MRT, CL, Vz, Vss and dose-normalised values.

use crate::Route;
use crate::result::{NcReason, ParamValue};

/// The dose as a parameter: a value when finite and > 0, otherwise the reason (NCA-DAT-10).
pub(crate) fn dose_value(dose: f64) -> ParamValue {
    if dose.is_nan() {
        ParamValue::nc(NcReason::DoseMissing)
    } else if dose.is_finite() && dose > 0.0 {
        ParamValue::Value(dose)
    } else {
        ParamValue::nc(NcReason::InvalidDose)
    }
}

/// `num / den` when `den` is an area > 0, never an infinity (NCA-EXT-08). The reason of `num`
/// comes first, then that of `den`.
fn over_area(num: ParamValue, den: ParamValue) -> ParamValue {
    num.zip(den, |n, d| {
        if d > 0.0 {
            ParamValue::of(n / d)
        } else {
            ParamValue::nc(NcReason::NonPositiveArea)
        }
    })
}

/// Areas and terminal-phase values the derived parameters are computed from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Inputs {
    pub dose: ParamValue,
    pub lambda_z: ParamValue,
    pub cmax: ParamValue,
    pub clast: ParamValue,
    pub auclast: ParamValue,
    pub aucall: ParamValue,
    pub aumclast: ParamValue,
    pub aucinf_obs: ParamValue,
    pub aucinf_pred: ParamValue,
    pub aumcinf_obs: ParamValue,
    pub aumcinf_pred: ParamValue,
}

/// Named derived parameters, in a stable order. Route-specific names that do not apply to `route`
/// are not calculated with [`NcReason::NotApplicableToRoute`].
pub(crate) fn derived(route: Route, x: &Inputs) -> Vec<(&'static str, ParamValue)> {
    let not_applicable = ParamValue::nc(NcReason::NotApplicableToRoute);
    // NCA-EXT-04: MRT = AUMC/AUC, minus T_inf/2 for intravascular data.
    let infusion_half = match route {
        Route::IvInfusion { duration } => Some(duration / 2.0),
        Route::IvBolus => Some(0.0),
        Route::Extravascular => None,
    };
    let mrt = |aumc, auc| over_area(aumc, auc);
    let mrt_iv = |aumc, auc| match infusion_half {
        Some(half) => {
            let ratio: ParamValue = over_area(aumc, auc);
            ratio.zip(ParamValue::Value(half), |m, h| ParamValue::of(m - h))
        }
        None => not_applicable,
    };
    // NCA-EXT-05, 06: CL = D/AUCinf, Vz = CL/λz (apparent CL/F, Vz/F for extravascular data).
    let cl = |aucinf| over_area(x.dose, aucinf);
    let vz = |cl: ParamValue| cl.zip(x.lambda_z, |c, lz| ParamValue::of(c / lz));
    let (cl_obs, cl_pred) = (cl(x.aucinf_obs), cl(x.aucinf_pred));
    let (mrt_iv_obs, mrt_iv_pred) = (
        mrt_iv(x.aumcinf_obs, x.aucinf_obs),
        mrt_iv(x.aumcinf_pred, x.aucinf_pred),
    );
    // NCA-EXT-07: Vss = CL·MRT_iv, intravascular only.
    let vss = |cl: ParamValue, mrt: ParamValue| cl.zip(mrt, |c, m| ParamValue::of(c * m));
    let extravascular = infusion_half.is_none();
    let ev_only = |value: ParamValue| if extravascular { value } else { not_applicable };
    let iv_only = |value: ParamValue| if extravascular { not_applicable } else { value };
    // NCA-OBS-05: value per unit of dose.
    let dn = |value: ParamValue| value.zip(x.dose, |v, d| ParamValue::of(v / d));
    vec![
        ("mrt.last", ev_only(mrt(x.aumclast, x.auclast))),
        ("mrt.obs", ev_only(mrt(x.aumcinf_obs, x.aucinf_obs))),
        ("mrt.pred", ev_only(mrt(x.aumcinf_pred, x.aucinf_pred))),
        ("mrt.iv.last", mrt_iv(x.aumclast, x.auclast)),
        ("mrt.iv.obs", mrt_iv_obs),
        ("mrt.iv.pred", mrt_iv_pred),
        ("cl.obs", cl_obs),
        ("cl.pred", cl_pred),
        ("vz.obs", vz(cl_obs)),
        ("vz.pred", vz(cl_pred)),
        ("vss.iv.obs", iv_only(vss(cl_obs, mrt_iv_obs))),
        ("vss.iv.pred", iv_only(vss(cl_pred, mrt_iv_pred))),
        ("cmax.dn", dn(x.cmax)),
        ("clast.obs.dn", dn(x.clast)),
        ("auclast.dn", dn(x.auclast)),
        ("aucall.dn", dn(x.aucall)),
        ("aucinf.obs.dn", dn(x.aucinf_obs)),
        ("aucinf.pred.dn", dn(x.aucinf_pred)),
        ("aumclast.dn", dn(x.aumclast)),
        ("aumcinf.obs.dn", dn(x.aumcinf_obs)),
        ("aumcinf.pred.dn", dn(x.aumcinf_pred)),
    ]
}
