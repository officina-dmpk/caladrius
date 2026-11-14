#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: helpers panic on purpose

//! Cross-checks the population-fit oracle (task T-152, `specs/population.md` POP-ORA-*) against independent
//! arithmetic (`AGENTS.md` section 5, source 3), so that the expected files are tested by something other than
//! the R scripts that wrote them:
//!
//! - the files load, are complete and have the expected shape;
//! - `naive_pop` (double precision, second-order jets, written without the R scripts) reproduces every exact case
//!   (five forms, per subject and in total, conditional modes) and every `ofv` case (the same, and the
//!   diagnostics PRED, IPRED, IWRES, E, C, three CWRES, shrinkage with both divisors, AIC and BIC);
//! - the identities of POP-OBJ-04 hold in the 256-bit values themselves (read as doubles);
//! - the worked examples PX1 and PX2 of the spec, computed by the reader, are reproduced at the digits printed
//!   there, and so are the PX3 diagnostics;
//! - the sealed data: every file's SHA-256 and the manifest are recomputed (and the SHA-256 code is checked
//!   against the FIPS 180-4 vectors);
//! - `errors.json` is well formed and every row names its rule, its code and its words.
//!
//! This tests the oracle and the testkit, not the engine. The largest differences found are printed
//! (`cargo test -p caladrius-testkit --test pop_consistency -- --nocapture`) and are the measurements behind the
//! tolerances proposed in the report of T-152.

use caladrius_testkit::{
    AbsoluteTolerance, NaiveFrozen, POP_FORMS, POP_SEALED_DESIGNS, PopExactCase,
    list_pop_exact_cases, list_pop_ofv_cases, load_pop_errors, load_pop_exact_case,
    load_pop_ofv_case, load_pop_sealed, load_pop_sealed_replicate, naive_pop, sha256_hex,
    verify_pop_sealed,
};

/// The naive code against the 256-bit eta: the conditional mode is found by Newton to a gradient of 1e-12.
/// The measured gap is 8.5e-15 (exact cases), so the margin is more than two orders of magnitude.
const NAIVE_ETA: f64 = 1e-12;
/// The naive diagnostics against the doubles of the file (values of order 1 to 10).
const NAIVE_DIAG: f64 = 1e-8;

fn parse(text: &str) -> f64 {
    text.parse::<f64>().unwrap()
}

fn frozen_of(c: &PopExactCase) -> NaiveFrozen {
    NaiveFrozen::parse(&c.frozen_sigma).unwrap()
}

fn has_marginal(c: &PopExactCase) -> bool {
    c.expected.total_full.contains_key("exact_marginal")
}

/// Largest absolute difference, tracked by name.
#[derive(Default)]
struct Worst {
    value: f64,
    at: String,
}
impl Worst {
    fn see(&mut self, what: &str, actual: f64, expected: f64) {
        let d = (actual - expected).abs();
        if d > self.value || d.is_nan() {
            self.value = if d.is_nan() { f64::INFINITY } else { d };
            self.at = what.to_owned();
        }
    }
    fn within(&self, what: &str, limit: f64) {
        eprintln!("{what}: largest difference {:e} ({})", self.value, self.at);
        assert!(
            self.value <= limit,
            "{what}: {:e} at {} exceeds {limit:e}",
            self.value,
            self.at
        );
    }
}

// ---------------------------------------------------------------- the SHA-256 code

#[test]
fn sha256_matches_the_fips_180_4_vectors() {
    assert_eq!(
        sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
    // one million 'a' (the third FIPS vector) crosses many blocks
    let million = vec![b'a'; 1_000_000];
    assert_eq!(
        sha256_hex(&million),
        "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
    );
}

// ---------------------------------------------------------------- the exact cases

#[test]
fn the_exact_cases_load_and_are_complete() {
    let ids = list_pop_exact_cases().unwrap();
    assert!(ids.len() >= 29, "{} exact cases", ids.len());
    for id in &ids {
        let c = load_pop_exact_case(id).unwrap();
        assert_eq!(&c.id, id);
        assert_eq!(c.precision_bits, 256);
        assert!(c.digits >= 50);
        assert_eq!(c.expected.per_subject.len(), c.subjects.len(), "{id}");
        let n: usize = c.subjects.iter().map(|s| s.time.len()).sum();
        assert_eq!(c.expected.n_obs, n, "{id}");
        for s in &c.subjects {
            assert_eq!(s.time.len(), s.y.len(), "{id}/{}", s.id);
        }
        for form in POP_FORMS {
            assert!(c.expected.total_full.contains_key(form), "{id}: {form}");
            assert!(c.expected.total_no_2pi.contains_key(form), "{id}: {form}");
            for s in &c.expected.per_subject {
                assert!(s.ofv_full.contains_key(form), "{id}/{}: {form}", s.id);
                assert_eq!(s.eta_hat.len(), c.model.random_effects.len());
            }
        }
        // the convention: no_2pi = full - N ln 2pi
        let n2pi = parse(&c.expected.n_ln_2pi);
        for form in POP_FORMS {
            let full = parse(&c.expected.total_full[form]);
            let no = parse(&c.expected.total_no_2pi[form]);
            assert!(
                (full - n2pi - no).abs() <= 1e-12 * full.abs().max(1.0),
                "{id}: {form} conventions disagree"
            );
        }
    }
    let families: std::collections::BTreeSet<String> = ids
        .iter()
        .map(|id| load_pop_exact_case(id).unwrap().family)
        .collect();
    assert_eq!(
        families.into_iter().collect::<Vec<_>>(),
        ["a_linear", "b_nonlinear", "c_frozen", "d_interaction"]
    );
}

#[test]
fn the_256_bit_values_satisfy_the_identities_of_pop_obj_04() {
    let mut worst_lin = Worst::default();
    let mut worst_frozen = Worst::default();
    let mut worst_additive = Worst::default();
    let mut separated = 0usize;
    for id in list_pop_exact_cases().unwrap() {
        let c = load_pop_exact_case(&id).unwrap();
        for s in &c.expected.per_subject {
            let f = |k: &str| parse(&s.ofv_full[k]);
            let (lin, exp, nv, full, fo) = (
                f("linearised"),
                f("laplace_expected"),
                f("laplace_no_variance"),
                f("laplace_full"),
                f("fo"),
            );
            let what = format!("{id}/{}", s.id);
            match c.family.as_str() {
                // linear model, additive error: every form is the exact marginal
                "a_linear" => {
                    for (k, v) in [("exp", exp), ("nv", nv), ("full", full), ("fo", fo)] {
                        worst_lin.see(&format!("{what} {k}"), v, lin);
                    }
                }
                // sigma independent of eta: the three equal forms of POP-OBJ-04
                "b_nonlinear" => {
                    worst_additive.see(&format!("{what} exp"), exp, lin);
                    worst_additive.see(&format!("{what} nv"), nv, lin);
                    if (full - lin).abs() > 1e-6 {
                        separated += 1;
                    }
                }
                "c_frozen" => {
                    worst_frozen.see(&format!("{what} exp"), exp, lin);
                    worst_frozen.see(&format!("{what} nv"), nv, lin);
                }
                _ => {
                    // interaction: the forms split
                    assert!(
                        (exp - lin).abs() > 1e-4 || (nv - lin).abs() > 1e-4,
                        "{what}: the interaction case does not separate the forms"
                    );
                }
            }
        }
    }
    worst_lin.within("a_linear, all forms against linearised", 1e-13);
    worst_additive.within(
        "b_nonlinear, expected and no_variance against linearised",
        1e-13,
    );
    worst_frozen.within(
        "c_frozen, expected and no_variance against linearised",
        1e-13,
    );
    assert!(
        separated > 0,
        "laplace_full never separates in the nonlinear family"
    );
}

#[test]
fn naive_pop_reproduces_every_exact_case() {
    let mut obj = Worst::default();
    let mut eta = Worst::default();
    let mut marg = Worst::default();
    let mut total = Worst::default();
    for id in list_pop_exact_cases().unwrap() {
        let c = load_pop_exact_case(&id).unwrap();
        let with_marginal = has_marginal(&c);
        let r = naive_pop(&c.model, &c.subjects, frozen_of(&c), with_marginal)
            .unwrap_or_else(|e| panic!("{id}: {e}"));
        assert_eq!(r.n_obs, c.expected.n_obs);
        for (got, want) in r.subjects.iter().zip(&c.expected.per_subject) {
            assert_eq!(got.id, want.id);
            for (k, (g, w)) in got.eta_hat.iter().zip(&want.eta_hat).enumerate() {
                eta.see(&format!("{id}/{} eta[{k}]", got.id), *g, parse(w));
            }
            for form in POP_FORMS {
                obj.see(
                    &format!("{id}/{} {form}", got.id),
                    got.forms[form],
                    parse(&want.ofv_full[form]),
                );
            }
            if with_marginal {
                // the file states an error bound for its own quadrature (one is 1.9e-9 on the total): the naive
                // trapezoid must agree to the double-precision floor plus that bound
                let i = r.subjects.iter().position(|x| x.id == got.id).unwrap();
                let bound = parse(
                    c.checks["exact_marginal"]["per_subject"][i]["ofv_error_bound"]
                        .as_str()
                        .unwrap(),
                );
                let excess =
                    ((got.forms["exact_marginal"] - parse(&want.ofv_full["exact_marginal"])).abs()
                        - bound)
                        .max(0.0);
                marg.see(
                    &format!("{id}/{} exact_marginal beyond its estimated bound", got.id),
                    excess,
                    0.0,
                );
            }
        }
        for form in POP_FORMS {
            total.see(
                &format!("{id} {form}"),
                r.totals[form],
                parse(&c.expected.total_full[form]),
            );
        }
    }
    eta.within("exact cases, conditional modes", NAIVE_ETA);
    obj.within(
        "exact cases, per-subject objectives",
        AbsoluteTolerance::NAIVE_POP_OFV.abs,
    );
    total.within(
        "exact cases, total objectives",
        AbsoluteTolerance::NAIVE_POP_OFV.abs,
    );
    marg.within(
        "exact cases, exact marginal (trapezoid in double precision)",
        AbsoluteTolerance::NAIVE_POP_OFV.abs,
    );
}

#[test]
fn the_exact_marginal_of_the_file_carries_an_estimated_error_bound() {
    let mut seen = 0usize;
    for id in list_pop_exact_cases().unwrap() {
        let c = load_pop_exact_case(&id).unwrap();
        if !has_marginal(&c) {
            continue;
        }
        seen += 1;
        let bound = parse(
            c.checks["exact_marginal"]["ofv_total_error_bound"]
                .as_str()
                .unwrap(),
        );
        assert!(
            bound < 1e-8,
            "{id}: the quadrature bound {bound:e} is not tight"
        );
        eprintln!("{id}: estimated bound of the total exact marginal {bound:e}");
        let exact = parse(&c.expected.total_full["exact_marginal"]);
        let lin = parse(&c.expected.total_full["linearised"]);
        assert!(
            (exact - lin).abs() > 1e-6,
            "{id}: the marginal never differs from the approximation"
        );
    }
    assert!(seen >= 4, "{seen} cases with an exact marginal");
}

// ---------------------------------------------------------------- the ofv cases

#[test]
fn the_ofv_cases_load_and_are_complete() {
    let ids = list_pop_ofv_cases().unwrap();
    assert_eq!(ids.len(), 20, "12 synthetic, 4 Theoph, 4 Indometh");
    for id in &ids {
        let c = load_pop_ofv_case(id).unwrap();
        assert_eq!(&c.id, id);
        assert_eq!(c.expected.n_subjects, c.subjects.len());
        let n: usize = c.subjects.iter().map(|s| s.time.len()).sum();
        assert_eq!(c.expected.n_obs, n);
        for form in POP_FORMS {
            let sum: f64 = c
                .expected
                .per_subject
                .iter()
                .map(|s| s.ofv_full[form])
                .sum();
            assert!(
                (sum - c.expected.total_full[form]).abs() <= 1e-10,
                "{id}: {form} total"
            );
            assert!(
                (c.expected.total_full[form] - c.expected.n_ln_2pi - c.expected.total_no_2pi[form])
                    .abs()
                    <= 1e-10,
                "{id}: {form} conventions"
            );
            assert!(
                c.diagnostics_population
                    .information_criteria_full
                    .contains_key(form)
            );
        }
    }
    let kinds = |p: &str| ids.iter().filter(|i| i.starts_with(p)).count();
    assert_eq!(
        (kinds("syn_"), kinds("theoph_"), kinds("indometh_")),
        (12, 4, 4)
    );
}

#[test]
fn naive_pop_reproduces_every_ofv_case_and_its_diagnostics() {
    let mut obj = Worst::default();
    let mut eta = Worst::default();
    let mut diag = Worst::default();
    let mut popd = Worst::default();
    let mut cwres_sum = Worst::default();
    for id in list_pop_ofv_cases().unwrap() {
        let c = load_pop_ofv_case(&id).unwrap();
        let r = naive_pop(&c.model, &c.subjects, NaiveFrozen::None, false).unwrap();
        for (got, want) in r.subjects.iter().zip(&c.expected.per_subject) {
            let sid = format!("{id}/{}", got.id);
            for (k, (g, w)) in got.eta_hat.iter().zip(&want.eta_hat).enumerate() {
                eta.see(&format!("{sid} eta[{k}]"), *g, *w);
            }
            for form in POP_FORMS {
                obj.see(
                    &format!("{sid} {form}"),
                    got.forms[form],
                    want.ofv_full[form],
                );
            }
            let (g, w) = (&got.diagnostics, &want.diagnostics);
            let pairs: [(&str, &Vec<f64>, &Vec<f64>); 9] = [
                ("pred", &g.pred, &w.pred),
                ("ipred", &g.ipred, &w.ipred),
                ("iwres", &g.iwres, &w.iwres),
                ("sigma_hat", &g.sigma_hat, &w.sigma_hat),
                ("e", &g.e, &w.e),
                ("c_diag", &g.c_diag, &w.c_diag),
                ("cwres_diagonal", &g.cwres_diagonal, &w.cwres_diagonal),
                ("cwres_cholesky", &g.cwres_cholesky, &w.cwres_cholesky),
                ("cwres_symmetric", &g.cwres_symmetric, &w.cwres_symmetric),
            ];
            for (name, a, b) in pairs {
                assert_eq!(a.len(), b.len(), "{sid} {name}");
                for (j, (x, y)) in a.iter().zip(b).enumerate() {
                    diag.see(&format!("{sid} {name}[{j}]"), *x, *y);
                }
            }
            diag.see(&format!("{sid} sum_w2"), g.sum_w2, w.sum_w2);
            diag.see(&format!("{sid} logdet_c"), g.logdet_c, w.logdet_c);
            // the decorrelated and the symmetric forms both square-sum to the distance term of the objective
            let sq = |v: &Vec<f64>| v.iter().map(|x| x * x).sum::<f64>();
            cwres_sum.see(
                &format!("{sid} cholesky sum"),
                sq(&w.cwres_cholesky),
                w.sum_w2,
            );
            cwres_sum.see(
                &format!("{sid} symmetric sum"),
                sq(&w.cwres_symmetric),
                w.sum_w2,
            );
        }
        let (g, w) = (&r.population, &c.diagnostics_population);
        for k in 0..w.omega_sd.len() {
            popd.see(
                &format!("{id} eta shrinkage m-1 [{k}]"),
                g.eta_shrinkage_m_minus_1[k],
                w.eta_shrinkage_divisor_m_minus_1[k],
            );
            popd.see(
                &format!("{id} eta shrinkage m [{k}]"),
                g.eta_shrinkage_m[k],
                w.eta_shrinkage_divisor_m[k],
            );
            popd.see(
                &format!("{id} sd eta m-1 [{k}]"),
                g.sd_eta_hat_m_minus_1[k],
                w.sd_eta_hat_divisor_m_minus_1[k],
            );
            popd.see(
                &format!("{id} sd eta m [{k}]"),
                g.sd_eta_hat_m[k],
                w.sd_eta_hat_divisor_m[k],
            );
            popd.see(
                &format!("{id} omega sd [{k}]"),
                g.omega_sd[k],
                w.omega_sd[k],
            );
        }
        popd.see(
            &format!("{id} sd iwres n-1"),
            g.sd_iwres_n_minus_1,
            w.sd_iwres_divisor_n_minus_1,
        );
        popd.see(
            &format!("{id} sd iwres n"),
            g.sd_iwres_n,
            w.sd_iwres_divisor_n,
        );
        popd.see(
            &format!("{id} eps shrinkage n-1"),
            g.eps_shrinkage_n_minus_1,
            w.eps_shrinkage_divisor_n_minus_1,
        );
        popd.see(
            &format!("{id} eps shrinkage n"),
            g.eps_shrinkage_n,
            w.eps_shrinkage_divisor_n,
        );
        assert_eq!(g.k, w.k.total, "{id}: k");
        assert_eq!(w.k.total, w.k.theta + w.k.omega + w.k.xi, "{id}: k parts");
        for form in POP_FORMS {
            popd.see(
                &format!("{id} aic {form}"),
                g.aic[form],
                w.information_criteria_full[form].aic,
            );
            popd.see(
                &format!("{id} bic N {form}"),
                g.bic_n_observations[form],
                w.information_criteria_full[form].bic_n_observations,
            );
            popd.see(
                &format!("{id} bic m {form}"),
                g.bic_n_subjects[form],
                w.information_criteria_full[form].bic_n_subjects,
            );
            obj.see(
                &format!("{id} total {form}"),
                r.totals[form],
                c.expected.total_full[form],
            );
        }
    }
    eta.within("ofv cases, conditional modes", NAIVE_ETA);
    obj.within(
        "ofv cases, objectives",
        AbsoluteTolerance::NAIVE_POP_OFV.abs,
    );
    diag.within("ofv cases, per-observation diagnostics", NAIVE_DIAG);
    popd.within("ofv cases, shrinkage, AIC, BIC", NAIVE_DIAG);
    cwres_sum.within(
        "ofv cases, squared sums of the decorrelated CWRES against the distance term",
        1e-12,
    );
}

// ---------------------------------------------------------------- the worked examples of the spec

/// PX1 (spec section 10): the printed 20-digit modes and objectives and the totals.
#[test]
fn px1_is_reproduced_at_the_printed_digits() {
    let c = load_pop_exact_case("lin_px1").unwrap();
    let modes = [1.0 / 15.0, -37.0 / 45.0, 44.0 / 45.0];
    let ofv = [
        4.203_555_398_494_039, // the spec prints 20 digits; the double nearest to them
        6.914_666_509_605_15,
        6.714_666_509_605_15,
    ];
    for (i, s) in c.expected.per_subject.iter().enumerate() {
        assert!((parse(&s.eta_hat[0]) - modes[i]).abs() < 1e-15, "mode {i}");
        for form in POP_FORMS {
            assert!(
                (parse(&s.ofv_full[form]) - ofv[i]).abs() < 1e-14,
                "{form} subject {i}"
            );
        }
    }
    assert!(
        c.expected.total_full["linearised"]
            .starts_with("17.83288841770433874730803640249596217296870")
    );
    assert!(
        c.expected
            .n_ln_2pi
            .starts_with("22.05452479691214580272791367373482335667353")
    );
    assert!(
        c.expected.total_no_2pi["linearised"]
            .starts_with("-4.22163637920780705541987727123886118370483")
    );
}

/// PX2 (spec section 10): per-subject and total objectives at five decimals, from the reader's double-precision
/// script; the oracle's values are the 256-bit ones, so the comparison is at the printed digits.
#[test]
fn px2_is_reproduced_at_the_printed_digits() {
    let c = load_pop_exact_case("int_px2").unwrap();
    // columns: linearised, laplace_expected, laplace_no_variance, laplace_full, fo
    let rows: [[f64; 5]; 4] = [
        [-0.70398, -0.64952, -0.68705, -0.53679, -0.76677],
        [-4.36642, -4.32467, -4.36199, -4.53638, -4.36425],
        [-3.61581, -3.55578, -3.59289, -3.68442, -3.25241],
        [-5.45569, -5.39038, -5.42763, -5.35642, -5.71680],
    ];
    let names = [
        "linearised",
        "laplace_expected",
        "laplace_no_variance",
        "laplace_full",
        "fo",
    ];
    for (i, s) in c.expected.per_subject.iter().enumerate() {
        for (k, name) in names.iter().enumerate() {
            let got = parse(&s.ofv_full[*name]);
            assert!(
                (got - rows[i][k]).abs() <= 5.1e-6,
                "PX2 subject {} {name}: oracle {got}, spec {}",
                i + 1,
                rows[i][k]
            );
        }
    }
    let totals = [-14.14190, -13.92035, -14.06957, -14.11401, -14.10023];
    for (k, name) in names.iter().enumerate() {
        let got = parse(&c.expected.total_full[*name]);
        assert!(
            (got - totals[k]).abs() <= 5.1e-6,
            "PX2 total {name}: oracle {got}, spec {}",
            totals[k]
        );
    }
    let n2pi = parse(&c.expected.n_ln_2pi);
    assert!((n2pi - 44.10905).abs() <= 5.1e-6);
    let no2pi = parse(&c.expected.total_no_2pi["linearised"]);
    assert!((no2pi - -58.25095).abs() <= 5.1e-6);
    // the modes printed in the spec
    let modes = [
        [0.20380, -0.26150],
        [0.21388, 0.02097],
        [0.24425, 0.18824],
        [0.04647, -0.09498],
    ];
    for (i, s) in c.expected.per_subject.iter().enumerate() {
        for (k, printed) in modes[i].iter().enumerate() {
            assert!(
                (parse(&s.eta_hat[k]) - printed).abs() <= 2e-4,
                "PX2 mode {i},{k}"
            );
        }
    }
}

/// PX3 (spec section 10): the diagnostics of subject 1 of PX2, from the independent code, at five decimals.
#[test]
fn px3_is_reproduced_at_the_printed_digits() {
    let c = load_pop_exact_case("int_px2").unwrap();
    let r = naive_pop(&c.model, &c.subjects, NaiveFrozen::None, false).unwrap();
    let d = &r.subjects[0].diagnostics;
    let rows: [(&str, &Vec<f64>, [f64; 6]); 7] = [
        (
            "PRED",
            &d.pred,
            [1.90246, 1.80967, 1.63746, 1.34064, 0.89866, 0.60239],
        ),
        (
            "IPRED",
            &d.ipred,
            [2.39892, 2.21532, 1.88919, 1.37390, 0.72663, 0.38430],
        ),
        (
            "IWRES",
            &d.iwres,
            [0.17123, 0.78852, 0.48069, 0.04443, -1.74265, 0.82494],
        ),
        (
            "E",
            &d.e,
            [1.86049, 1.80017, 1.67514, 1.42184, 0.96735, 0.62551],
        ),
        (
            "diag C",
            &d.c_diag,
            [0.255827, 0.199039, 0.134579, 0.097757, 0.083990, 0.054918],
        ),
        (
            "CWRES diagonal",
            &d.cwres_diagonal,
            [1.14575, 1.32209, 0.83102, -0.13381, -1.26754, -0.89402],
        ),
        (
            "CWRES Cholesky",
            &d.cwres_cholesky,
            [1.14575, 0.69657, -0.21341, -0.86618, -1.28908, 1.58117],
        ),
    ];
    for (name, got, want) in rows {
        for (j, w) in want.iter().enumerate() {
            assert!(
                (got[j] - w).abs() <= 6e-6,
                "PX3 {name}[{j}]: {} against {w}",
                got[j]
            );
        }
    }
    let sym = [0.65038, 1.13244, 0.59827, -0.13426, -2.07139, 0.61932];
    for (j, w) in sym.iter().enumerate() {
        assert!(
            (d.cwres_symmetric[j] - w).abs() <= 6e-6,
            "PX3 symmetric[{j}]: {}",
            d.cwres_symmetric[j]
        );
    }
    assert!((d.sum_w2 - 6.755572).abs() <= 6e-7);
    assert!((d.logdet_c - -18.48681).abs() <= 6e-6);
    let p = &r.population;
    assert!(
        (p.sd_eta_hat_m_minus_1[0] - 0.088768).abs() <= 6e-7
            && (p.sd_eta_hat_m_minus_1[1] - 0.189610).abs() <= 6e-7
    );
    assert!(
        (p.eta_shrinkage_m_minus_1[0] - 0.70411).abs() <= 6e-6
            && (p.eta_shrinkage_m_minus_1[1] - 0.05195).abs() <= 6e-6
    );
    assert!(
        (p.sd_eta_hat_m[0] - 0.076875).abs() <= 6e-7
            && (p.sd_eta_hat_m[1] - 0.164207).abs() <= 6e-7
    );
    assert!(
        (p.eta_shrinkage_m[0] - 0.74375).abs() <= 6e-6
            && (p.eta_shrinkage_m[1] - 0.17896).abs() <= 6e-6
    );
    assert!((p.sd_iwres_n_minus_1 - 0.848793).abs() <= 6e-7);
    assert!((p.eps_shrinkage_n_minus_1 - 0.15121).abs() <= 6e-6);
}

// ---------------------------------------------------------------- the sealed data

#[test]
fn the_sealed_designs_verify_and_have_the_stated_shape() {
    for design in POP_SEALED_DESIGNS {
        let s = load_pop_sealed(design).unwrap();
        assert!(s.sealed);
        assert_eq!(s.name, design);
        verify_pop_sealed(&s).unwrap();
        assert_eq!(s.design.replicates, 200);
        assert_eq!(s.design.observations_per_subject, s.design.times.len());
        assert_eq!(
            s.design.observations_per_replicate,
            s.design.subjects * s.design.times.len()
        );
        assert_eq!(s.aggregate_checks.asserted, design == "accurate");
        for k in [1, 100, 200] {
            let subjects = load_pop_sealed_replicate(&s, k).unwrap();
            assert_eq!(subjects.len(), s.design.subjects, "{design} replicate {k}");
            for sub in &subjects {
                assert_eq!(sub.time.len(), s.design.times.len());
                assert!(
                    sub.y_f64().unwrap().iter().all(|y| *y > 0.0),
                    "{design}: non-positive concentration"
                );
            }
        }
        let names: Vec<&String> = s.model.theta.keys().collect();
        assert_eq!(names, s.truth.typical_values.keys().collect::<Vec<_>>());
    }
}

#[test]
fn a_changed_sealed_file_is_detected() {
    let s = load_pop_sealed("limits").unwrap();
    let mut bad = s.clone();
    bad.files[0].sha256 = "0".repeat(64);
    assert!(verify_pop_sealed(&bad).is_err());
    let mut bad = s;
    bad.manifest_sha256 = "f".repeat(64);
    assert!(verify_pop_sealed(&bad).is_err());
}

// ---------------------------------------------------------------- the refusals

#[test]
fn errors_json_is_well_formed() {
    let rows = load_pop_errors().unwrap();
    assert_eq!(rows.len(), 32);
    let mut names = std::collections::BTreeSet::new();
    let (mut refused, mut accepted, mut controls) = (0, 0, 0);
    for r in &rows {
        assert!(names.insert(r.case.clone()), "duplicate row {}", r.case);
        assert!(
            r.rule.starts_with("POP-INN-03") || r.rule.starts_with("POP-"),
            "{}: rule {}",
            r.case,
            r.rule
        );
        assert!(
            r.expect == "refused" || r.expect == "accepted",
            "{}",
            r.case
        );
        match r.expect.as_str() {
            "refused" => {
                refused += 1;
                let e = r
                    .error
                    .as_ref()
                    .unwrap_or_else(|| panic!("{}: no error", r.case));
                assert!(!e.words.is_empty(), "{}: no words", r.case);
                assert!(
                    [
                        "spec_sentence",
                        "reused_sentence",
                        "inferred",
                        "rule_names_option"
                    ]
                    .contains(&e.words_basis.as_str()),
                    "{}: words_basis {}",
                    r.case,
                    e.words_basis
                );
                assert!(e.code.is_some(), "{}: no code", r.case);
                assert!(r.expect_flags.is_empty(), "{}", r.case);
                if r.constructible {
                    assert!(r.request.is_some(), "{}: no request", r.case);
                    // a refusal that can be built has a control: the same request, repaired, is accepted
                    assert!(r.control.is_some(), "{}: no control", r.case);
                } else {
                    assert!(r.request.is_none());
                }
            }
            _ => {
                accepted += 1;
                assert!(r.request.is_some(), "{}", r.case);
                assert!(
                    r.expect_flags
                        .iter()
                        .all(|f| f == "few_observations" || f == "few_subjects"),
                    "{}: flags {:?}",
                    r.case,
                    r.expect_flags
                );
            }
        }
        if let Some(c) = &r.control {
            controls += 1;
            assert!(!c.change.is_empty());
            assert!(
                c.request.get("model").is_some(),
                "{}: control without model",
                r.case
            );
        }
    }
    assert_eq!((refused, accepted), (30, 2));
    assert_eq!(controls, 29);
    // every rule 1 to 6 is represented
    for n in 1..=7 {
        let tag = format!("POP-INN-03.{n}");
        assert!(
            rows.iter().any(|r| r.rule.contains(&tag)),
            "no row for {tag}"
        );
    }
    // the datasets the rows point at exist
    for r in &rows {
        if let Some(file) = r
            .request
            .as_ref()
            .and_then(|q| q.get("dataset_file"))
            .and_then(|f| f.as_str())
        {
            assert!(
                caladrius_testkit::oracle::oracle_dir().join(file).exists(),
                "{}: dataset {file} is missing",
                r.case
            );
        }
    }
}

/// The unbounded-likelihood rows name the subject that carries the zero, and not the others.
#[test]
fn the_unbounded_rows_are_constructed_as_they_say() {
    for r in load_pop_errors().unwrap() {
        if !r.case.starts_with("pr1_unbounded_") {
            continue;
        }
        let q = r.request.unwrap();
        let subjects = q["subjects"].as_array().unwrap();
        let zero_ids: Vec<&str> = subjects
            .iter()
            .filter(|s| s["time"][0].as_f64() == Some(0.0) && s["y"][0].as_f64() == Some(0.0))
            .map(|s| s["id"].as_str().unwrap())
            .collect();
        assert_eq!(
            zero_ids.len(),
            1,
            "{}: exactly one subject carries the zero",
            r.case
        );
        let e = r.error.unwrap();
        assert!(
            e.words.iter().any(|w| w.contains(zero_ids[0])),
            "{}: the words name {}",
            r.case,
            zero_ids[0]
        );
        assert!(!e.must_not_contain.iter().any(|w| w == zero_ids[0]));
        assert_eq!(e.code.as_deref(), Some("unbounded_likelihood"));
    }
}

/// Measures the factor quoted in the documentation of `AbsoluteTolerance::POP_DIAGNOSTIC`: an error `d` in an eta
/// changes a standardised residual by about `factor * d`, with `factor = |d yhat / d eta| / sigma` for the leading
/// term, and the exact derivative of IWRES (the interaction term included) as a check. Over the ofv cases and the
/// exact cases given to the engine (the variance not frozen), at the conditional modes.
#[test]
fn the_amplification_of_an_error_in_eta_is_below_the_documented_factor() {
    let mut lead = Worst::default();
    let mut iwres = Worst::default();
    let place = |w: &mut Worst, what: String, v: f64| {
        if v > w.value {
            w.value = v;
            w.at = what;
        }
    };
    let mut visit = |id: &str, r: &caladrius_testkit::NaivePop| {
        for s in &r.subjects {
            let d = &s.diagnostics;
            for j in 0..d.sigma_hat.len() {
                let sigma = d.sigma_hat[j];
                for k in 0..d.d_yhat_d_eta[j].len() {
                    let g = d.d_yhat_d_eta[j][k];
                    place(
                        &mut lead,
                        format!("{id}/{} obs {j} eta[{k}]", s.id),
                        g.abs() / sigma,
                    );
                    let full = (g + d.iwres[j] * d.d_s2_d_eta[j][k] / (2.0 * sigma)) / sigma;
                    place(
                        &mut iwres,
                        format!("{id}/{} obs {j} eta[{k}]", s.id),
                        full.abs(),
                    );
                }
            }
        }
    };
    for id in list_pop_ofv_cases().unwrap() {
        let c = load_pop_ofv_case(&id).unwrap();
        visit(
            &id,
            &naive_pop(&c.model, &c.subjects, NaiveFrozen::None, false).unwrap(),
        );
    }
    for id in list_pop_exact_cases().unwrap() {
        let c = load_pop_exact_case(&id).unwrap();
        if matches!(frozen_of(&c), NaiveFrozen::None) {
            visit(
                &id,
                &naive_pop(&c.model, &c.subjects, NaiveFrozen::None, false).unwrap(),
            );
        }
    }
    eprintln!(
        "max |d yhat / d eta| / sigma: {:e} ({})",
        lead.value, lead.at
    );
    eprintln!(
        "max |d IWRES / d eta| (interaction included): {:e} ({})",
        iwres.value, iwres.at
    );
    assert!(lead.value < 1e3, "the documented factor 1e3 is exceeded");
    assert!(iwres.value < 1e3, "the documented factor 1e3 is exceeded");
}

/// The row for several dose rows (POP-INN-03.7): the table has two dose rows for S2 and one for S1 and S3, the
/// words name S2 and not the others, and the control table is the same table without the second dose row of S2.
#[test]
fn the_several_dose_rows_table_is_constructed_as_it_says() {
    let rows = load_pop_errors().unwrap();
    let row = rows
        .iter()
        .find(|r| r.case == "pr6_several_dose_rows")
        .expect("the row exists");
    let dir = caladrius_testkit::oracle::oracle_dir();
    let table = |file: &str| -> Vec<String> {
        std::fs::read_to_string(dir.join(file))
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect()
    };
    let doses = |lines: &[String], id: &str| -> usize {
        lines
            .iter()
            .skip(1)
            .filter(|l| {
                let f: Vec<&str> = l.split(',').collect();
                f[0] == id && f[4] == "1"
            })
            .count()
    };
    let file = |q: &serde_json::Value| q["dataset_file"].as_str().unwrap().to_owned();
    let refused = table(&file(row.request.as_ref().unwrap()));
    let control = table(&file(&row.control.as_ref().unwrap().request));
    assert_eq!(
        [
            doses(&refused, "S1"),
            doses(&refused, "S2"),
            doses(&refused, "S3")
        ],
        [1, 2, 1]
    );
    assert_eq!(
        [
            doses(&control, "S1"),
            doses(&control, "S2"),
            doses(&control, "S3")
        ],
        [1, 1, 1]
    );
    assert_eq!(refused.len(), control.len() + 1);
    let e = row.error.as_ref().unwrap();
    assert_eq!(e.code.as_deref(), Some("unsupported_history"));
    assert!(e.words.iter().any(|w| w == "subject S2"));
    assert!(e.must_not_contain.iter().any(|w| w == "S1"));
    assert!(e.must_not_contain.iter().any(|w| w == "S3"));
}
