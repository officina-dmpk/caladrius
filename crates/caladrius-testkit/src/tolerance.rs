//! Tolerances, defined once (AGENTS.md section 5). A test never builds its own: it names one of the
//! constants below. Loosening a tolerance is a change to this file, reviewed as such.

use std::fmt;

/// How close an actual value must be to an expected value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tolerance {
    /// Passes when `|actual - expected| <= rel * |expected|`; when `expected` is exactly zero,
    /// when `|actual| <= abs`.
    ///
    /// The absolute part is not a floor added to every comparison: it only gives an expected value
    /// of exactly zero a defined meaning. Added to a non-zero expected value it would make the
    /// tolerance silently relative-plus-1e-12, which for a quantity of order 1e-6 (a dose-normalised
    /// value in large units, a tiny rate constant) is a relative error of 1e-6 hidden inside a
    /// 1e-6 tolerance (task T-009, review of the reviewer's note on T-004c).
    Relative { rel: f64, abs: f64 },
    /// Equality at the precision shown in an export: passes when the actual value, rounded to
    /// `decimals` decimal places, equals the expected value, i.e. `|actual - expected| <= 0.5 * 10^-decimals`.
    DisplayedDecimals { decimals: u32 },
}

impl Tolerance {
    /// NCA parameters against PKNCA: relative error at most 1e-6.
    pub const NCA_VS_PKNCA: Tolerance = Tolerance::Relative {
        rel: 1e-6,
        abs: 1e-12,
    };
    /// Fitted parameters: relative error at most 1e-4.
    pub const FIT_PARAMETERS: Tolerance = Tolerance::Relative {
        rel: 1e-4,
        abs: 1e-12,
    };
    /// Fitted parameters and statistics on the five-point example (`specs/fit.md` section 11, F2,
    /// small enough to be worked on paper): relative error at most 1e-6, the NCA tolerance. It
    /// shows that `FIT_PARAMETERS` is a margin of caution (task T-029); it does not replace it.
    pub const FIT_PARAMETERS_SMALL_CASE: Tolerance = Tolerance::Relative {
        rel: 1e-6,
        abs: 1e-12,
    };
    /// Weighted sum of squares of a fit: relative error at most 1e-6.
    pub const FIT_WEIGHTED_SS: Tolerance = Tolerance::Relative {
        rel: 1e-6,
        abs: 1e-12,
    };

    /// Closed-form model values (concentration, AUC, secondary parameters) against the exact values
    /// of `oracle/expected/models/`: relative error at most 1e-12. An expected zero (before the dose
    /// or the lag, dose 0) must be returned as exactly zero.
    pub const MODEL_VALUES: Tolerance = Tolerance::Relative {
        rel: 1e-12,
        abs: 0.0,
    };

    /// Equality at the precision displayed in a reference export, with `decimals` decimal places.
    pub const fn displayed(decimals: u32) -> Tolerance {
        Tolerance::DisplayedDecimals { decimals }
    }

    /// Largest absolute difference accepted for this expected value.
    pub fn allowed_difference(&self, expected: f64) -> f64 {
        match *self {
            Tolerance::Relative { rel, abs } => {
                if expected == 0.0 {
                    abs
                } else {
                    rel * expected.abs()
                }
            }
            Tolerance::DisplayedDecimals { decimals } => {
                // Half a unit in the last displayed place, plus a margin for binary rounding of the
                // decimal text (relative 1e-9 of that half unit).
                let half_unit = 0.5 * 10f64.powi(-i32::try_from(decimals).unwrap_or(i32::MAX));
                half_unit * (1.0 + 1e-9)
            }
        }
    }

    /// True when `actual` is within tolerance of `expected`. Non-finite values are never within
    /// tolerance (a NaN or infinite result is reported, not silently accepted).
    pub fn accepts(&self, actual: f64, expected: f64) -> bool {
        if !actual.is_finite() || !expected.is_finite() {
            return false;
        }
        (actual - expected).abs() <= self.allowed_difference(expected)
    }

    /// Error measure shown in reports: relative error `|a - e| / |e|` for a relative tolerance
    /// (absolute difference when `e` is zero), absolute difference for a displayed-precision tolerance.
    pub fn error(&self, actual: f64, expected: f64) -> f64 {
        let diff = (actual - expected).abs();
        match self {
            Tolerance::Relative { .. } if expected != 0.0 => diff / expected.abs(),
            _ => diff,
        }
    }
}

impl fmt::Display for Tolerance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Tolerance::Relative { rel, abs } => {
                write!(
                    f,
                    "relative error <= {rel:e} (absolute {abs:e} when the expected value is 0)"
                )
            }
            Tolerance::DisplayedDecimals { decimals } => {
                write!(f, "equality at {decimals} displayed decimal place(s)")
            }
        }
    }
}

/// Number of decimal places written in a numeric text such as `"12.350"` (3) or `"7"` (0).
/// Returns `None` when the text is not a plain decimal number.
pub fn displayed_decimals(text: &str) -> Option<u32> {
    let trimmed = text.trim();
    trimmed.parse::<f64>().ok()?;
    if trimmed.contains(['e', 'E']) {
        return None;
    }
    match trimmed.split_once('.') {
        Some((_, frac)) => u32::try_from(frac.len()).ok(),
        None => Some(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nca_tolerance_is_one_in_a_million() {
        assert_eq!(
            Tolerance::NCA_VS_PKNCA,
            Tolerance::Relative {
                rel: 1e-6,
                abs: 1e-12
            }
        );
        assert!(Tolerance::NCA_VS_PKNCA.accepts(100.000099, 100.0));
        assert!(!Tolerance::NCA_VS_PKNCA.accepts(100.00011, 100.0));
        assert!(!Tolerance::NCA_VS_PKNCA.accepts(99.99989, 100.0));
    }

    #[test]
    fn fit_tolerances_match_the_contract() {
        assert_eq!(
            Tolerance::FIT_PARAMETERS,
            Tolerance::Relative {
                rel: 1e-4,
                abs: 1e-12
            }
        );
        assert_eq!(
            Tolerance::FIT_WEIGHTED_SS,
            Tolerance::Relative {
                rel: 1e-6,
                abs: 1e-12
            }
        );
        // The small-case tolerance is the stricter one, never looser than the contract.
        assert_eq!(
            Tolerance::FIT_PARAMETERS_SMALL_CASE,
            Tolerance::NCA_VS_PKNCA
        );
        assert!(!Tolerance::FIT_PARAMETERS_SMALL_CASE.accepts(1.0 + 2e-6, 1.0));
        assert!(Tolerance::FIT_PARAMETERS.accepts(1.0 + 2e-6, 1.0));
    }

    #[test]
    fn expected_zero_requires_essentially_zero() {
        assert!(Tolerance::NCA_VS_PKNCA.accepts(0.0, 0.0));
        assert!(Tolerance::NCA_VS_PKNCA.accepts(1e-13, 0.0));
        assert!(!Tolerance::NCA_VS_PKNCA.accepts(1e-9, 0.0));
    }

    #[test]
    fn the_absolute_part_only_applies_to_an_expected_zero() {
        // A value of order 1e-7 is compared relatively: an absolute floor of 1e-12 added to every
        // comparison would let a relative error of 1e-5 through (1e-12 / 1e-7).
        assert!(!Tolerance::NCA_VS_PKNCA.accepts(1.00001e-7, 1e-7));
        assert!(Tolerance::NCA_VS_PKNCA.accepts(1.0000009e-7, 1e-7));
        // Smaller still (1e-14): the floor would have accepted anything below 1e-12.
        assert!(!Tolerance::NCA_VS_PKNCA.accepts(2e-14, 1e-14));
        assert!(Tolerance::NCA_VS_PKNCA.accepts(1e-14 * (1.0 + 5e-7), 1e-14));
    }

    #[test]
    fn model_values_are_relative_to_1e_12_and_zero_is_exact() {
        let t = Tolerance::MODEL_VALUES;
        assert!(t.accepts(5.0 * (1.0 + 9e-13), 5.0));
        assert!(!t.accepts(5.0 * (1.0 + 2e-12), 5.0));
        assert!(t.accepts(0.0, 0.0));
        assert!(!t.accepts(1e-300, 0.0));
        assert!(t.to_string().contains("1e-12"));
    }

    #[test]
    fn non_finite_values_are_never_accepted() {
        assert!(!Tolerance::NCA_VS_PKNCA.accepts(f64::NAN, 1.0));
        assert!(!Tolerance::NCA_VS_PKNCA.accepts(f64::INFINITY, f64::INFINITY));
        assert!(!Tolerance::NCA_VS_PKNCA.accepts(1.0, f64::NAN));
        assert!(!Tolerance::displayed(2).accepts(f64::NAN, 1.0));
    }

    #[test]
    fn displayed_precision_is_half_a_unit_in_the_last_place() {
        let t = Tolerance::displayed(2);
        assert!(t.accepts(12.346, 12.35));
        assert!(t.accepts(12.3549, 12.35));
        assert!(!t.accepts(12.3551, 12.35));
        assert!(t.accepts(12.345, 12.35)); // exactly half a unit away: still the same rounded value
        assert!(Tolerance::displayed(0).accepts(7.4, 7.0));
        assert!(!Tolerance::displayed(0).accepts(7.6, 7.0));
    }

    #[test]
    fn error_is_relative_for_relative_tolerances() {
        let e = Tolerance::NCA_VS_PKNCA.error(101.0, 100.0);
        assert!((e - 0.01).abs() < 1e-15);
        assert!((Tolerance::NCA_VS_PKNCA.error(0.5, 0.0) - 0.5).abs() < 1e-15);
        assert!((Tolerance::displayed(1).error(3.2, 3.0) - 0.2).abs() < 1e-12);
    }

    #[test]
    fn decimals_of_a_text() {
        assert_eq!(displayed_decimals("12.350"), Some(3));
        assert_eq!(displayed_decimals(" 7 "), Some(0));
        assert_eq!(displayed_decimals("0.5"), Some(1));
        assert_eq!(displayed_decimals("1e-3"), None);
        assert_eq!(displayed_decimals("abc"), None);
        assert_eq!(displayed_decimals(""), None);
    }

    #[test]
    fn display_names_the_rule() {
        assert!(Tolerance::NCA_VS_PKNCA.to_string().contains("1e-6"));
        assert!(Tolerance::displayed(3).to_string().contains("3 displayed"));
    }
}
