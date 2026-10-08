//! Extrapolation to infinity (`specs/nca.md` NCA-EXT-01 to 03, LZ-11).

use crate::result::{NcReason, ParamValue};

/// AUC and AUMC to infinity and the extrapolated percentages, observed and predicted.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Extrapolated {
    pub aucinf_obs: ParamValue,
    pub aucinf_pred: ParamValue,
    pub aumcinf_obs: ParamValue,
    pub aumcinf_pred: ParamValue,
    pub aucpext_obs: ParamValue,
    pub aucpext_pred: ParamValue,
    pub aumcpext_obs: ParamValue,
    pub aumcpext_pred: ParamValue,
}

/// AUC from Tlast to infinity: C/λz (NCA-EXT-01).
fn auc_tail(c: f64, lambda_z: f64) -> f64 {
    c / lambda_z
}

/// AUMC from Tlast to infinity: C·Tlast/λz + C/λz² (NCA-EXT-03).
fn aumc_tail(c: f64, tlast: f64, lambda_z: f64) -> f64 {
    c * tlast / lambda_z + c / (lambda_z * lambda_z)
}

/// 100·(1 − part/total), not calculated unless both areas are > 0 (NCA-EXT-02).
fn percent_extrapolated(part: ParamValue, total: ParamValue) -> ParamValue {
    total.zip(part, |total, part| {
        if part > 0.0 && total > 0.0 {
            ParamValue::of(100.0 * (1.0 - part / total))
        } else {
            ParamValue::nc(NcReason::NonPositiveArea)
        }
    })
}

/// `lambda_z` first so that its failure reason is the one propagated (NCA-LZ-11).
pub(crate) fn extrapolate(
    lambda_z: ParamValue,
    auclast: ParamValue,
    aumclast: ParamValue,
    clast_obs: ParamValue,
    clast_pred: ParamValue,
    // Start of the AUMC tail: where AUClast ends (Tlast unless a replaced value follows it).
    tlast: ParamValue,
) -> Extrapolated {
    let aucinf = |clast: ParamValue| {
        lambda_z.zip(auclast, |lz, area| {
            clast.zip(lambda_z, |c, _| ParamValue::of(area + auc_tail(c, lz)))
        })
    };
    let aumcinf = |clast: ParamValue| {
        lambda_z.zip(aumclast, |lz, area| {
            clast.zip(tlast, |c, t| ParamValue::of(area + aumc_tail(c, t, lz)))
        })
    };
    let (aucinf_obs, aucinf_pred) = (aucinf(clast_obs), aucinf(clast_pred));
    let (aumcinf_obs, aumcinf_pred) = (aumcinf(clast_obs), aumcinf(clast_pred));
    Extrapolated {
        aucinf_obs,
        aucinf_pred,
        aumcinf_obs,
        aumcinf_pred,
        aucpext_obs: percent_extrapolated(auclast, aucinf_obs),
        aucpext_pred: percent_extrapolated(auclast, aucinf_pred),
        aumcpext_obs: percent_extrapolated(aumclast, aumcinf_obs),
        aumcpext_pred: percent_extrapolated(aumclast, aumcinf_pred),
    }
}
