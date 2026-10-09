//! Two-compartment parameter sets: names, completeness, domains and the conversions between the
//! three parameterisations (`specs/models.md` MOD-2C-02 to 05).
//!
//! Every quantity is computed as a sum of positive numbers, a product or a quotient (MOD-2C-03,
//! 2C-04, 2C-21): no digit is lost when the exponents are close or when k12 is small.

use std::collections::BTreeMap;

use crate::ModelError;
use crate::model::ModelId;

/// The parameterisation given by the user (MOD-2C-02).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Set {
    /// `cl`, `vc`, `q`, `vp` (the default).
    Clearance,
    /// `k10`, `k12`, `k21`, `vc`.
    Micro,
    /// `a`, `b`, `alpha`, `beta` (the intravenous coefficients; the dose is required).
    Macro,
}

const CLEARANCE: [&str; 4] = ["cl", "vc", "q", "vp"];
const MICRO: [&str; 4] = ["k10", "k12", "k21", "vc"];
const MACRO: [&str; 4] = ["a", "b", "alpha", "beta"];
/// Names whose presence decides the set (`vc` belongs to two sets).
const MARKERS: [(Set, &[&str]); 3] = [
    (Set::Clearance, &["cl", "q", "vp", "vc"]),
    (Set::Micro, &["k10", "k12", "k21"]),
    (Set::Macro, &MACRO),
];
/// Names of other models that a two-compartment model never takes.
const OTHER_MODELS: [&str; 2] = ["v", "k"];

/// A checked two-compartment parameter set with every derived quantity of MOD-2C-03/04/13.
/// Absent inputs (`ka`, `dur`, `tlag`) are 0 and never read by a model without them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Disposition {
    pub set: Set,
    pub vc: f64,
    pub cl: f64,
    pub q: f64,
    pub vp: f64,
    pub k10: f64,
    pub k12: f64,
    pub k21: f64,
    pub alpha: f64,
    pub beta: f64,
    /// alpha − beta, > 0.
    pub d: f64,
    pub w_alpha: f64,
    pub w_beta: f64,
    /// alpha − k10 and k10 − beta, both > 0 (MOD-2C-19).
    pub m: f64,
    pub n: f64,
    /// The intravenous coefficients D·wα/vc and D·wβ/vc (given for the macro set).
    pub a: f64,
    pub b: f64,
    pub ka: f64,
    pub dur: f64,
    pub tlag: f64,
}

fn out_of_domain(name: &str, value: f64, domain: &str) -> ModelError {
    ModelError::ParameterOutOfDomain {
        name: name.to_string(),
        value,
        domain: domain.to_string(),
    }
}

/// A finite number > 0 (or >= 0 when `zero_allowed`).
fn check(name: &str, value: f64, zero_allowed: bool) -> Result<f64, ModelError> {
    let ok = value.is_finite() && (value > 0.0 || (zero_allowed && value == 0.0));
    if ok {
        Ok(value)
    } else if zero_allowed {
        Err(out_of_domain(name, value, "a finite number >= 0"))
    } else {
        Err(out_of_domain(name, value, "a finite number > 0"))
    }
}

/// A derived quantity must be a finite number > 0 (MOD-2C-05 item 3).
fn derived(name: &str, from: &str, value: f64) -> Result<f64, ModelError> {
    if value.is_finite() && value > 0.0 {
        Ok(value)
    } else {
        Err(ModelError::DerivedOutOfRange {
            name: name.to_string(),
            from: from.to_string(),
            value,
        })
    }
}

/// Which set the given names select; an error for none or for two sets.
fn select(model: &str, given: &BTreeMap<String, f64>) -> Result<Set, ModelError> {
    let mut found: Option<(Set, &str)> = None;
    for (set, names) in MARKERS {
        let Some(name) = names.iter().copied().find(|n| given.contains_key(*n)) else {
            continue;
        };
        // `vc` alone with the micro markers is the micro set, not a mixture.
        if set == Set::Clearance && name == "vc" && MICRO.iter().any(|n| given.contains_key(*n)) {
            continue;
        }
        match found {
            None => found = Some((set, name)),
            Some((_, first)) => {
                return Err(ModelError::MixedParameterSets {
                    first: first.to_string(),
                    second: name.to_string(),
                    model: model.to_string(),
                });
            }
        }
    }
    found.map(|(set, _)| set).ok_or(ModelError::NoParameterSet {
        model: model.to_string(),
    })
}

/// Checks the names against the model, selects the set, checks every domain (MOD-2C-05,
/// MOD-GEN-04) and converts (MOD-2C-03, 2C-04). `dose` is checked here too: the macro set needs it.
pub(crate) fn resolve(
    model: ModelId,
    given: &BTreeMap<String, f64>,
    dose: f64,
) -> Result<Disposition, ModelError> {
    let id = model.id();
    let inputs = model.input_parameters();
    for name in given.keys() {
        let known_here = CLEARANCE.contains(&name.as_str())
            || MICRO.contains(&name.as_str())
            || MACRO.contains(&name.as_str());
        if known_here || inputs.contains(&name.as_str()) {
            continue;
        }
        let known_elsewhere =
            OTHER_MODELS.contains(&name.as_str()) || ["ka", "dur", "tlag"].contains(&name.as_str());
        return Err(if known_elsewhere {
            ModelError::ParameterNotInModel {
                name: name.clone(),
                model: id.to_string(),
            }
        } else {
            ModelError::UnknownParameter {
                name: name.clone(),
                model: id.to_string(),
            }
        });
    }
    // Mixed sets come before missing names: a mixture is the more useful message.
    let set = select(id, given)?;
    let names = match set {
        Set::Clearance => CLEARANCE,
        Set::Micro => MICRO,
        Set::Macro => MACRO,
    };
    let required = |name: &str| {
        given
            .get(name)
            .copied()
            .ok_or_else(|| ModelError::MissingParameter {
                name: name.to_string(),
                model: id.to_string(),
            })
    };
    let mut values = [0.0; 4];
    for (slot, name) in values.iter_mut().zip(names) {
        *slot = required(name)?;
    }
    for name in inputs {
        required(name)?;
    }
    // Domains, in the order of the set (MOD-2C-05 items 1 and 2).
    for (value, name) in values.iter().zip(names) {
        if *value == 0.0 && matches!(name, "q" | "vp" | "k12") {
            return Err(ModelError::OneCompartment {
                name: name.to_string(),
                model: id.to_string(),
            });
        }
        check(name, *value, false)?;
    }
    let input = |name: &str, zero_allowed: bool| -> Result<f64, ModelError> {
        if inputs.contains(&name) {
            check(name, required(name)?, zero_allowed)
        } else {
            Ok(0.0)
        }
    };
    let ka = input("ka", false)?;
    let dur = input("dur", false)?;
    let tlag = input("tlag", true)?;
    if !(dose.is_finite() && dose >= 0.0) {
        return Err(ModelError::InvalidDose { value: dose });
    }
    let [p0, p1, p2, p3] = values;
    let mut out = match set {
        Set::Clearance => from_clearance(id, dose, p0, p1, p2, p3)?,
        Set::Micro => from_micro(id, dose, p0, p1, p2, p3)?,
        Set::Macro => from_macro(dose, p0, p1, p2, p3)?,
    };
    out.ka = ka;
    out.dur = dur;
    out.tlag = tlag;
    Ok(out)
}

/// Clearance set: k10 = cl/vc, k12 = q/vc, k21 = q/vp, then MOD-2C-03.
fn from_clearance(
    id: &str,
    dose: f64,
    cl: f64,
    vc: f64,
    q: f64,
    vp: f64,
) -> Result<Disposition, ModelError> {
    let k10 = derived("k10", "cl / vc", cl / vc)?;
    let k12 = derived("k12", "q / vc", q / vc)?;
    let k21 = derived("k21", "q / vp", q / vp)?;
    let mut out = exponents(id, Set::Clearance, dose, vc, k10, k12, k21)?;
    out.cl = cl;
    out.q = q;
    out.vp = vp;
    Ok(out)
}

/// Micro set: cl = k10·vc, q = k12·vc, vp = vc·k12/k21, then MOD-2C-03.
fn from_micro(
    id: &str,
    dose: f64,
    k10: f64,
    k12: f64,
    k21: f64,
    vc: f64,
) -> Result<Disposition, ModelError> {
    let mut out = exponents(id, Set::Micro, dose, vc, k10, k12, k21)?;
    out.cl = derived("cl", "k10 x vc", k10 * vc)?;
    out.q = derived("q", "k12 x vc", k12 * vc)?;
    out.vp = derived("vp", "q / k21", out.q / k21)?;
    Ok(out)
}

/// MOD-2C-03: exponents and weights from the micro-constants, with the discriminant as a sum of
/// non-negative terms, beta = k10·k21/alpha, and the weights through p = alpha − k21 and
/// q = k21 − beta with p·q = k12·k21.
fn exponents(
    id: &str,
    set: Set,
    dose: f64,
    vc: f64,
    k10: f64,
    k12: f64,
    k21: f64,
) -> Result<Disposition, ModelError> {
    let degenerate = || ModelError::DegenerateExponents {
        model: id.to_string(),
    };
    let sum = k10 + k12 + k21;
    let diff = k10 - k21;
    let discriminant = diff * diff + k12 * (k12 + 2.0 * k10 + 2.0 * k21);
    let r = discriminant.sqrt();
    let alpha = (sum + r) / 2.0;
    let beta = k10 * k21 / alpha;
    let u = diff + k12;
    let (p, q) = if u >= 0.0 {
        let p = (u + r) / 2.0;
        (p, k12 * k21 / p)
    } else {
        let q = (r - u) / 2.0;
        (k12 * k21 / q, q)
    };
    let usable = |x: f64| x.is_finite() && x > 0.0;
    if !(usable(r) && usable(p) && usable(q) && usable(alpha) && usable(beta)) {
        return Err(degenerate());
    }
    let w_alpha = p / (p + q);
    let w_beta = q / (p + q);
    let a = dose * w_alpha / vc;
    let b = dose * w_beta / vc;
    Ok(Disposition {
        set,
        vc,
        cl: 0.0,
        q: 0.0,
        vp: 0.0,
        k10,
        k12,
        k21,
        alpha,
        beta,
        d: r,
        w_alpha,
        w_beta,
        // alpha − k10 = alpha·(k21 − beta)/k21 and k10 − beta = beta·(alpha − k21)/k21.
        m: alpha * q / k21,
        n: beta * p / k21,
        a,
        b,
        ka: 0.0,
        dur: 0.0,
        tlag: 0.0,
    })
}

/// MOD-2C-04: vc = D/(a + b), w = a/(a + b), k21 = alpha·wβ + beta·wα, k10 = alpha·beta/k21,
/// k12 = wα·wβ·(alpha − beta)²/k21.
fn from_macro(dose: f64, a: f64, b: f64, alpha: f64, beta: f64) -> Result<Disposition, ModelError> {
    if alpha <= beta {
        return Err(ModelError::AlphaNotAboveBeta { alpha, beta });
    }
    if dose == 0.0 {
        return Err(ModelError::MacroWithoutDose);
    }
    let total = derived("a + b", "a + b", a + b)?;
    let vc = derived("vc", "dose / (a + b)", dose / total)?;
    let w_alpha = a / total;
    let w_beta = b / total;
    let d = alpha - beta;
    let k21 = derived(
        "k21",
        "alpha x w_beta + beta x w_alpha",
        alpha * w_beta + beta * w_alpha,
    )?;
    let k10 = derived("k10", "alpha x beta / k21", alpha * beta / k21)?;
    let k12 = derived(
        "k12",
        "w_alpha x w_beta x (alpha - beta)^2 / k21",
        w_alpha * w_beta * d * d / k21,
    )?;
    let cl = derived("cl", "k10 x vc", k10 * vc)?;
    let q = derived("q", "k12 x vc", k12 * vc)?;
    let vp = derived("vp", "q / k21", q / k21)?;
    Ok(Disposition {
        set: Set::Macro,
        vc,
        cl,
        q,
        vp,
        k10,
        k12,
        k21,
        alpha,
        beta,
        d,
        w_alpha,
        w_beta,
        // alpha − k10 = alpha·wβ·d/k21 and k10 − beta = beta·wα·d/k21.
        m: alpha * w_beta * d / k21,
        n: beta * w_alpha * d / k21,
        a,
        b,
        ka: 0.0,
        dur: 0.0,
        tlag: 0.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn given(pairs: &[(&str, f64)]) -> BTreeMap<String, f64> {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-14 * b.abs()
    }

    /// Worked example N7: the three sets of one point give the same disposition.
    #[test]
    fn the_three_sets_of_worked_example_n7_agree() {
        let c = resolve(
            ModelId::Pk2IvBolus,
            &given(&[("cl", 2.0), ("vc", 10.0), ("q", 4.0), ("vp", 8.0)]),
            100.0,
        )
        .unwrap();
        assert_eq!(c.set, Set::Clearance);
        assert!(close(c.alpha, 1.0) && close(c.beta, 0.1) && close(c.d, 0.9));
        assert!(close(c.w_alpha, 5.0 / 9.0) && close(c.w_beta, 4.0 / 9.0));
        assert!(close(c.a, 50.0 / 9.0) && close(c.b, 40.0 / 9.0));
        assert!(close(c.m, 0.8) && close(c.n, 0.1));
        let m = resolve(
            ModelId::Pk2IvBolus,
            &given(&[("k10", 0.2), ("k12", 0.4), ("k21", 0.5), ("vc", 10.0)]),
            100.0,
        )
        .unwrap();
        assert_eq!(m.set, Set::Micro);
        assert!(close(m.cl, 2.0) && close(m.q, 4.0) && close(m.vp, 8.0));
        let x = resolve(
            ModelId::Pk2IvBolus,
            &given(&[
                ("a", 50.0 / 9.0),
                ("b", 40.0 / 9.0),
                ("alpha", 1.0),
                ("beta", 0.1),
            ]),
            100.0,
        )
        .unwrap();
        assert_eq!(x.set, Set::Macro);
        for (got, want) in [
            (x.vc, 10.0),
            (x.k10, 0.2),
            (x.k12, 0.4),
            (x.k21, 0.5),
            (x.cl, 2.0),
            (x.q, 4.0),
            (x.vp, 8.0),
            (x.m, 0.8),
            (x.n, 0.1),
        ] {
            assert!((got - want).abs() <= 1e-14 * want, "{got} against {want}");
        }
    }

    /// Worked example N6: the discriminant as a sum of non-negative terms keeps every digit.
    #[test]
    fn nearly_equal_exponents_keep_their_digits() {
        let c = resolve(
            ModelId::Pk2IvBolus,
            &given(&[("k10", 0.2), ("k12", 1e-12), ("k21", 0.2), ("vc", 10.0)]),
            100.0,
        )
        .unwrap();
        assert!((c.d - 8.944271910005e-7).abs() <= 1e-12 * 8.944271910005e-7 + 1e-19);
        // alpha and beta are rounded near 0.2: their difference is d to that rounding only.
        assert!((c.alpha - c.beta - c.d).abs() <= 1e-16);
    }

    #[test]
    fn refusals_name_what_to_fix() {
        let e = |pairs: &[(&str, f64)], dose: f64| {
            resolve(ModelId::Pk2Oral1, &given(pairs), dose)
                .unwrap_err()
                .to_string()
        };
        let base = [
            ("cl", 2.0),
            ("vc", 10.0),
            ("q", 4.0),
            ("vp", 8.0),
            ("ka", 1.0),
        ];
        let with = |name: &str, value: f64| -> Vec<(&str, f64)> {
            base.iter()
                .map(|&(n, v)| if n == name { (n, value) } else { (n, v) })
                .collect()
        };
        assert!(e(&with("q", 0.0), 100.0).contains("pk1"));
        assert!(e(&with("vp", -1.0), 100.0).contains("`vp`"));
        assert!(e(&with("ka", 0.0), 100.0).contains("`ka`"));
        assert!(e(&base, -1.0).contains("dose"));
        assert!(e(&[("vc", 10.0), ("q", 4.0), ("vp", 8.0), ("ka", 1.0)], 1.0).contains("`cl`"));
        assert!(e(&[], 1.0).contains("alpha"));
        let mixed = e(&[("cl", 2.0), ("k12", 0.4), ("ka", 1.0)], 1.0);
        assert!(mixed.contains("`cl`") && mixed.contains("`k12`"), "{mixed}");
        let macro_set = [
            ("a", 5.0),
            ("b", 4.0),
            ("alpha", 1.0),
            ("beta", 0.1),
            ("ka", 1.0),
        ];
        assert!(e(&macro_set, 0.0).contains("dose"));
        assert!(
            e(
                &[
                    ("a", 5.0),
                    ("b", 4.0),
                    ("alpha", 0.1),
                    ("beta", 1.0),
                    ("ka", 1.0)
                ],
                1.0
            )
            .contains("alpha")
        );
        let degenerate = resolve(
            ModelId::Pk2IvBolus,
            &given(&[("k10", 1.0), ("k12", 1e-200), ("k21", 1e-200), ("vc", 1.0)]),
            1.0,
        )
        .unwrap_err();
        assert!(matches!(degenerate, ModelError::DegenerateExponents { .. }));
        assert!(
            e(
                &[
                    ("cl", 2.0),
                    ("vc", 10.0),
                    ("q", 1e300),
                    ("vp", 1e-300),
                    ("ka", 1.0)
                ],
                1.0
            )
            .contains("k21")
        );
        assert!(
            e(
                &[
                    ("v", 2.0),
                    ("cl", 2.0),
                    ("vc", 10.0),
                    ("q", 4.0),
                    ("vp", 8.0),
                    ("ka", 1.0)
                ],
                1.0
            )
            .contains("`v`")
        );
    }
}
