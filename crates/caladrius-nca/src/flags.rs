//! Quality flags (`specs/nca.md` NCA-LZ-12b): a computed but doubtful result is reported with a
//! flag that says what to check. Flags never change or remove a number.

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::QualityThresholds;
use crate::lambda_z::LambdaZCandidate;
use crate::result::ParamValue;

/// One quality problem of a result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "code", deny_unknown_fields)]
pub enum QualityFlag {
    /// The adjusted R² of the terminal phase is below the threshold.
    LowAdjustedRSquared {
        /// Adjusted R² of the selected fit.
        #[serde(
            serialize_with = "crate::float::ser",
            deserialize_with = "crate::float::de"
        )]
        value: f64,
        /// Threshold.
        #[serde(
            serialize_with = "crate::float::ser",
            deserialize_with = "crate::float::de"
        )]
        threshold: f64,
    },
    /// The terminal phase spans fewer half-lives than the threshold.
    ShortSpan {
        /// Span ratio of the selected fit.
        #[serde(
            serialize_with = "crate::float::ser",
            deserialize_with = "crate::float::de"
        )]
        value: f64,
        /// Threshold.
        #[serde(
            serialize_with = "crate::float::ser",
            deserialize_with = "crate::float::de"
        )]
        threshold: f64,
    },
    /// The terminal phase has fewer points than the threshold.
    FewPoints {
        /// Number of points of the selected fit.
        value: usize,
        /// Threshold.
        threshold: usize,
    },
    /// The extrapolated part of an AUC to infinity is above the threshold.
    HighExtrapolation {
        /// Name of the percentage (`aucpext.obs` or `aucpext.pred`).
        parameter: String,
        /// The percentage.
        #[serde(
            serialize_with = "crate::float::ser",
            deserialize_with = "crate::float::de"
        )]
        value: f64,
        /// Threshold.
        #[serde(
            serialize_with = "crate::float::ser",
            deserialize_with = "crate::float::de"
        )]
        threshold: f64,
    },
    /// The terminal phase uses values that were missing or BLQ and replaced by a number.
    ReplacedPointsInLambdaZ {
        /// How many of its points are replaced values.
        count: usize,
    },
    /// AUClast ends on a replaced value after Tlast, so AUClast runs past Tlast while the
    /// extrapolation to infinity starts from Clast at Tlast: the stretch in between is counted
    /// twice in AUCinf (PKNCA behaviour, followed for conformance).
    AreaPastTlast {
        /// Where AUClast ends.
        #[serde(
            serialize_with = "crate::float::ser",
            deserialize_with = "crate::float::de"
        )]
        end: f64,
        /// Tlast (last measured positive concentration).
        #[serde(
            serialize_with = "crate::float::ser",
            deserialize_with = "crate::float::de"
        )]
        tlast: f64,
    },
}

impl fmt::Display for QualityFlag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LowAdjustedRSquared { value, threshold } => write!(
                f,
                "the terminal phase fits poorly (adjusted R² {value:.4} < {threshold}); check the selected points or choose them manually"
            ),
            Self::ShortSpan { value, threshold } => write!(
                f,
                "the terminal phase spans {value:.2} half-lives (< {threshold}); λz and the extrapolated values are uncertain, sample longer if possible"
            ),
            Self::FewPoints { value, threshold } => write!(
                f,
                "the terminal phase has {value} points (< {threshold}); λz is uncertain"
            ),
            Self::HighExtrapolation {
                parameter,
                value,
                threshold,
            } => write!(
                f,
                "{parameter} is {value:.1} % (> {threshold} %); the AUC to infinity relies mostly on extrapolation"
            ),
            Self::ReplacedPointsInLambdaZ { count } => write!(
                f,
                "{count} point(s) of the terminal phase are replaced missing or BLQ values; check the replacement value or exclude them (`exclude_replaced`)"
            ),
            Self::AreaPastTlast { end, tlast } => write!(
                f,
                "AUClast runs to a replaced value at {end}, past Tlast {tlast}; AUCinf counts that stretch twice, consider dropping or keeping the trailing BLQ values instead"
            ),
        }
    }
}

/// The values the flags look at.
pub(crate) struct Inputs<'a> {
    pub selected: Option<&'a LambdaZCandidate>,
    pub adj_r_squared: ParamValue,
    pub span_ratio: ParamValue,
    pub aucpext_obs: ParamValue,
    pub aucpext_pred: ParamValue,
    pub area_end: ParamValue,
    pub tlast: ParamValue,
}

/// Every flag raised under `thresholds`, in a stable order.
pub(crate) fn flags(thresholds: &QualityThresholds, x: &Inputs<'_>) -> Vec<QualityFlag> {
    let mut out = Vec::new();
    if let (Some(threshold), Some(value)) = (thresholds.min_adj_r_squared, x.adj_r_squared.value())
    {
        if value < threshold {
            out.push(QualityFlag::LowAdjustedRSquared { value, threshold });
        }
    }
    if let (Some(threshold), Some(value)) = (thresholds.min_span_ratio, x.span_ratio.value()) {
        if value < threshold {
            out.push(QualityFlag::ShortSpan { value, threshold });
        }
    }
    if let (Some(threshold), Some(fit)) = (thresholds.min_points, x.selected) {
        if fit.n_points < threshold {
            out.push(QualityFlag::FewPoints {
                value: fit.n_points,
                threshold,
            });
        }
    }
    if let Some(threshold) = thresholds.max_extrapolated_percent {
        for (parameter, pext) in [
            ("aucpext.obs", x.aucpext_obs),
            ("aucpext.pred", x.aucpext_pred),
        ] {
            if let Some(value) = pext.value().filter(|v| *v > threshold) {
                out.push(QualityFlag::HighExtrapolation {
                    parameter: parameter.to_string(),
                    value,
                    threshold,
                });
            }
        }
    }
    if let Some(fit) = x.selected.filter(|fit| fit.replaced_points > 0) {
        out.push(QualityFlag::ReplacedPointsInLambdaZ {
            count: fit.replaced_points,
        });
    }
    if let (Some(end), Some(tlast)) = (x.area_end.value(), x.tlast.value()) {
        if end > tlast {
            out.push(QualityFlag::AreaPastTlast { end, tlast });
        }
    }
    out
}
