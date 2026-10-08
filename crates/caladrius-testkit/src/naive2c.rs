//! A second, deliberately naive implementation of the two-compartment models of `specs/models.md`
//! section 11, in double precision (task T-032; `AGENTS.md` section 5, source 3). It exists to test
//! the oracle files of `oracle/expected/models/pk2/` (which come from a 256-bit R script) by
//! something written in another language and another arithmetic, and to give the derivative grids a
//! finite-difference check. It shares no code with `caladrius-models`: the engine is never tested
//! against itself.
//!
//! Naive means the textbook forms with none of the safeguards of MOD-2C-03, 04 and 10: the
//! discriminant is `S^2 - 4 k10 k21`, the weights are `(alpha - k21)/(alpha - beta)`, the oral
//! profile is the three-exponential form of MOD-2C-09 (so it is only valid when `ka` is not close
//! to an exponent and the exponents are not close to each other; the oracle options say which cases
//! those are, `textbook_double_ok`). The areas are the integrals of the same sums of exponentials.

use std::collections::BTreeMap;

/// One term `c * u^p * exp(-l * u)` of the concentration on a segment of constant input.
#[derive(Debug, Clone, Copy)]
struct Term {
    c: f64,
    l: f64,
    p: i32,
}

/// A stretch of time `[s0, s0 + len]` (`len` `None`: to infinity) on which the concentration is a
/// sum of terms in `u = t - s0`.
#[derive(Debug, Clone)]
struct Segment {
    s0: f64,
    len: Option<f64>,
    terms: Vec<Term>,
}

fn term(c: f64, l: f64, p: i32) -> Term {
    Term { c, l, p }
}

fn factorial(n: i32) -> f64 {
    (1..=n).map(f64::from).product()
}

/// Integral over `[0, x]` (`x` `None`: infinity) of `u^p exp(-l u)`.
fn ipow(p: i32, l: f64, x: Option<f64>) -> f64 {
    if l == 0.0 {
        return x.map_or(f64::INFINITY, |x| x.powi(p + 1) / f64::from(p + 1));
    }
    let scale = factorial(p) / l.powi(p + 1);
    match x {
        None => scale,
        Some(x) => {
            let y = l * x;
            let partial: f64 = (0..=p).map(|j| y.powi(j) / factorial(j)).sum();
            scale * (1.0 - (-y).exp() * partial)
        }
    }
}

/// The exponents, the weights and the volume of a parameter set (`clearance`, `micro` or `macro`).
struct Disposition {
    vc: f64,
    alpha: f64,
    beta: f64,
    wa: f64,
    wb: f64,
}

fn disposition(set: &str, p: &BTreeMap<String, f64>, dose: f64) -> Option<Disposition> {
    let get = |n: &str| p.get(n).copied();
    if set == "macro" {
        let (a, b) = (get("a")?, get("b")?);
        return Some(Disposition {
            vc: dose / (a + b),
            alpha: get("alpha")?,
            beta: get("beta")?,
            wa: a / (a + b),
            wb: b / (a + b),
        });
    }
    let (vc, k10, k12, k21) = match set {
        "clearance" => {
            let vc = get("vc")?;
            (vc, get("cl")? / vc, get("q")? / vc, get("q")? / get("vp")?)
        }
        "micro" => (get("vc")?, get("k10")?, get("k12")?, get("k21")?),
        _ => return None,
    };
    let s = k10 + k12 + k21;
    let r = (s * s - 4.0 * k10 * k21).sqrt();
    let (alpha, beta) = ((s + r) / 2.0, (s - r) / 2.0);
    let wa = (alpha - k21) / (alpha - beta);
    Some(Disposition {
        vc,
        alpha,
        beta,
        wa,
        wb: 1.0 - wa,
    })
}

fn segments(
    model: &str,
    d: &Disposition,
    dose: f64,
    p: &BTreeMap<String, f64>,
) -> Option<Vec<Segment>> {
    let (al, be) = (d.alpha, d.beta);
    let (ca, cb) = (dose * d.wa / d.vc, dose * d.wb / d.vc);
    let tau = p.get("tlag").copied().unwrap_or(0.0);
    match model {
        "pk2.iv_bolus" => Some(vec![Segment {
            s0: 0.0,
            len: None,
            terms: vec![term(ca, al, 0), term(cb, be, 0)],
        }]),
        "pk2.iv_infusion" | "pk2.oral_0" | "pk2.oral_0_lag" => {
            let big_t = p.get("dur").copied()?;
            let (xa, xb) = (ca / (al * big_t), cb / (be * big_t));
            Some(vec![
                Segment {
                    s0: tau,
                    len: Some(big_t),
                    terms: vec![
                        term(xa, 0.0, 0),
                        term(-xa, al, 0),
                        term(xb, 0.0, 0),
                        term(-xb, be, 0),
                    ],
                },
                Segment {
                    s0: tau + big_t,
                    len: None,
                    terms: vec![
                        term(xa * (1.0 - (-al * big_t).exp()), al, 0),
                        term(xb * (1.0 - (-be * big_t).exp()), be, 0),
                    ],
                },
            ])
        }
        "pk2.oral_1" | "pk2.oral_1_lag" => {
            let ka = p.get("ka").copied()?;
            let (ao, bo) = (ka * ca / (ka - al), ka * cb / (ka - be));
            Some(vec![Segment {
                s0: tau,
                len: None,
                terms: vec![term(ao, al, 0), term(bo, be, 0), term(-(ao + bo), ka, 0)],
            }])
        }
        _ => None,
    }
}

/// Concentration, AUC(0, t) and AUMC(0, t) of a two-compartment model at `t`, by the naive textbook
/// forms in double precision. `set` is `clearance`, `micro` or `macro`; `params` holds the names of
/// that set plus `ka`, `dur`, `tlag` as the model needs them. `None` for an unknown model or set or
/// a missing parameter.
pub fn conc_auc_aumc(
    model: &str,
    set: &str,
    params: &BTreeMap<String, f64>,
    dose: f64,
    t: f64,
) -> Option<[f64; 3]> {
    let d = disposition(set, params, dose)?;
    let segs = segments(model, &d, dose, params)?;
    let bolus = model == "pk2.iv_bolus";
    let mut conc = 0.0;
    for sg in &segs {
        let inside = t >= sg.s0 && sg.len.is_none_or(|len| t <= sg.s0 + len);
        if inside {
            let u = t - sg.s0;
            conc = if !bolus && u == 0.0 {
                0.0
            } else {
                sg.terms
                    .iter()
                    .map(|m| m.c * u.powi(m.p) * (-m.l * u).exp())
                    .sum()
            };
            break;
        }
    }
    if bolus && t < 0.0 {
        conc = 0.0;
    }
    let (mut auc, mut aumc) = (0.0, 0.0);
    for sg in &segs {
        if t <= sg.s0 {
            continue;
        }
        let upper = sg.len.map_or(t - sg.s0, |len| len.min(t - sg.s0));
        for m in &sg.terms {
            auc += m.c * ipow(m.p, m.l, Some(upper));
            aumc += m.c * (sg.s0 * ipow(m.p, m.l, Some(upper)) + ipow(m.p + 1, m.l, Some(upper)));
        }
    }
    Some([conc, auc, aumc])
}

/// Exponents `(alpha, beta)` of a clearance or micro set by the textbook quadratic.
pub fn exponents(set: &str, params: &BTreeMap<String, f64>, dose: f64) -> Option<(f64, f64)> {
    disposition(set, params, dose).map(|d| (d.alpha, d.beta))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> BTreeMap<String, f64> {
        [("cl", 2.0), ("vc", 10.0), ("q", 4.0), ("vp", 8.0)]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect()
    }

    /// Worked example N1 of `specs/models.md` section 11.6 (digits as printed there).
    #[test]
    fn the_bolus_reproduces_worked_example_n1() {
        let p = base();
        let at = |t: f64| conc_auc_aumc("pk2.iv_bolus", "clearance", &p, 100.0, t);
        let [c, auc, _] = at(4.0).unwrap_or([f64::NAN; 3]);
        assert!((c - 3.0809537540).abs() < 5e-11, "{c}");
        assert!((auc - 20.1062444046).abs() < 5e-10, "{auc}");
        let [c24, auc24, _] = at(24.0).unwrap_or([f64::NAN; 3]);
        assert!((c24 - 0.4031909037).abs() < 5e-11, "{c24}");
        assert!((auc24 - 45.9680909647).abs() < 5e-10, "{auc24}");
    }

    /// N2: the infusion over 2 h, and N3: first-order absorption with ka = 2.
    #[test]
    fn the_infusion_and_the_oral_model_reproduce_worked_examples_n2_and_n3() {
        let mut p = base();
        p.insert("dur".into(), 2.0);
        let [c, auc, aumc] =
            conc_auc_aumc("pk2.iv_infusion", "clearance", &p, 100.0, 24.0).unwrap_or([f64::NAN; 3]);
        assert!((c - 0.4463378912).abs() < 5e-11, "{c}");
        assert!((auc - 45.5366210942).abs() < 5e-10, "{auc}");
        assert!(aumc > 0.0 && aumc < 500.0);
        let mut o = base();
        o.insert("ka".into(), 2.0);
        let [c4, auc12, _] =
            conc_auc_aumc("pk2.oral_1", "clearance", &o, 100.0, 4.0).unwrap_or([f64::NAN; 3]);
        assert!((c4 - 3.3342105358).abs() < 5e-11, "{c4}");
        let [_, a12, _] =
            conc_auc_aumc("pk2.oral_1", "clearance", &o, 100.0, 12.0).unwrap_or([f64::NAN; 3]);
        assert!((a12 - 35.9089744488).abs() < 5e-10, "{a12} {auc12}");
    }

    #[test]
    fn unknown_models_sets_and_missing_parameters_are_none() {
        let p = base();
        assert!(conc_auc_aumc("pk2.nothing", "clearance", &p, 100.0, 1.0).is_none());
        assert!(conc_auc_aumc("pk2.iv_bolus", "weird", &p, 100.0, 1.0).is_none());
        assert!(conc_auc_aumc("pk2.oral_1", "clearance", &p, 100.0, 1.0).is_none());
    }
}
