//! An independent, deliberately simple population objective in double precision (task T-152).
//!
//! It shares no code with `caladrius-fit` or with the R scripts: forward second-order jets (value, gradient,
//! Hessian) for the derivatives of the prediction, a Newton search for the conditional mode, small dense
//! Cholesky and Jacobi routines for the linear algebra. It computes, for a model, parameters and data given as
//! in the case files of [`crate::pop`]:
//!
//! - the five objective forms of `specs/population.md` POP-OBJ-* (linearised at the conditional modes, FO, and
//!   the three Laplace forms), with the variance following the prediction (interaction) or frozen (OP-14) at the
//!   population prediction (`Pred0`) or at the fixed point of the mode (`Mode`);
//! - the diagnostics of POP-DIA-*: PRED, IPRED, IWRES, E, the diagonal of C, CWRES in three forms (diagonal of C,
//!   Cholesky factor, symmetric inverse square root), shrinkage with both divisors, AIC and BIC with both counts;
//! - for one random effect, the exact marginal by the trapezoidal rule (the test of the Laplace forms).
//!
//! Its job is to reproduce the 256-bit values of `oracle/expected/pop/{exact,ofv}/` to the double-precision floor
//! ([`crate::AbsoluteTolerance::NAIVE_POP_OFV`]) so that a reader can check the oracle against a second
//! implementation that is a few hundred lines long, and so that the tests of the population fit have a reference
//! that does not depend on the code under test.

// Dense linear algebra on matrices of a few rows, written with indices on purpose: it is read against the
// formulas of the specification, element by element, and an iterator form hides the index it is checked against.
#![allow(clippy::needless_range_loop)]

use std::collections::BTreeMap;
use std::ops::{Add, Div, Mul, Neg, Sub};

use crate::oracle::OracleError;
use crate::pop::{POP_FORMS, PopModel, PopSubject};

type R<T> = Result<T, String>;

fn wrap<T>(r: R<T>) -> Result<T, OracleError> {
    r.map_err(|message| OracleError::Inconsistent {
        path: "naive_pop".to_owned(),
        message,
    })
}

const LN_2PI: f64 = 1.837_877_066_409_345_5;

// ---------------------------------------------------------------- second-order jets

#[derive(Clone, Debug)]
struct Jet {
    v: f64,
    g: Vec<f64>,
    h: Vec<f64>,
}

impl Jet {
    fn constant(v: f64, q: usize) -> Jet {
        Jet {
            v,
            g: vec![0.0; q],
            h: vec![0.0; q * q],
        }
    }
    fn var(i: usize, v: f64, q: usize) -> Jet {
        let mut j = Jet::constant(v, q);
        j.g[i] = 1.0;
        j
    }
    /// f(self) from the values f, f', f'' at self.v.
    fn chain(&self, f0: f64, f1: f64, f2: f64) -> Jet {
        let q = self.g.len();
        let g = self.g.iter().map(|x| f1 * x).collect();
        let mut h = vec![0.0; q * q];
        for i in 0..q {
            for j in 0..q {
                h[i * q + j] = f1 * self.h[i * q + j] + f2 * self.g[i] * self.g[j];
            }
        }
        Jet { v: f0, g, h }
    }
    fn exp(&self) -> Jet {
        let e = self.v.exp();
        self.chain(e, e, e)
    }
    fn ln(&self) -> Jet {
        let v = self.v;
        self.chain(v.ln(), 1.0 / v, -1.0 / (v * v))
    }
    fn sqrt(&self) -> Jet {
        let s = self.v.sqrt();
        self.chain(s, 0.5 / s, -0.25 / (s * s * s))
    }
    fn recip(&self) -> Jet {
        let v = self.v;
        self.chain(1.0 / v, -1.0 / (v * v), 2.0 / (v * v * v))
    }
    fn abs(&self) -> Jet {
        if self.v < 0.0 {
            -self.clone()
        } else {
            self.clone()
        }
    }
    fn lift(&self, x: f64) -> Jet {
        Jet::constant(x, self.g.len())
    }
}

fn add_jets(a: &Jet, b: &Jet) -> Jet {
    Jet {
        v: a.v + b.v,
        g: a.g.iter().zip(&b.g).map(|(x, y)| x + y).collect(),
        h: a.h.iter().zip(&b.h).map(|(x, y)| x + y).collect(),
    }
}
fn sub_jets(a: &Jet, b: &Jet) -> Jet {
    Jet {
        v: a.v - b.v,
        g: a.g.iter().zip(&b.g).map(|(x, y)| x - y).collect(),
        h: a.h.iter().zip(&b.h).map(|(x, y)| x - y).collect(),
    }
}
fn mul_jets(a: &Jet, b: &Jet) -> Jet {
    let q = a.g.len();
    let mut h = vec![0.0; q * q];
    for i in 0..q {
        for j in 0..q {
            h[i * q + j] =
                a.h[i * q + j] * b.v + b.h[i * q + j] * a.v + a.g[i] * b.g[j] + a.g[j] * b.g[i];
        }
    }
    Jet {
        v: a.v * b.v,
        g: (0..q).map(|i| a.g[i] * b.v + a.v * b.g[i]).collect(),
        h,
    }
}

macro_rules! jet_op {
    ($tr:ident, $m:ident, $f:ident) => {
        impl $tr<Jet> for Jet {
            type Output = Jet;
            fn $m(self, o: Jet) -> Jet {
                $f(&self, &o)
            }
        }
        impl $tr<&Jet> for Jet {
            type Output = Jet;
            fn $m(self, o: &Jet) -> Jet {
                $f(&self, o)
            }
        }
        impl $tr<Jet> for &Jet {
            type Output = Jet;
            fn $m(self, o: Jet) -> Jet {
                $f(self, &o)
            }
        }
        impl $tr<&Jet> for &Jet {
            type Output = Jet;
            fn $m(self, o: &Jet) -> Jet {
                $f(self, o)
            }
        }
    };
}
jet_op!(Add, add, add_jets);
jet_op!(Sub, sub, sub_jets);
jet_op!(Mul, mul, mul_jets);

fn div_jets(a: &Jet, b: &Jet) -> Jet {
    mul_jets(a, &b.recip())
}
jet_op!(Div, div, div_jets);

impl Neg for Jet {
    type Output = Jet;
    fn neg(self) -> Jet {
        Jet {
            v: -self.v,
            g: self.g.iter().map(|x| -x).collect(),
            h: self.h.iter().map(|x| -x).collect(),
        }
    }
}

// ---------------------------------------------------------------- small dense linear algebra

type Mat = Vec<Vec<f64>>;

fn chol(a: &Mat) -> R<Mat> {
    let n = a.len();
    let mut l = vec![vec![0.0; n]; n];
    for i in 0..n {
        for j in 0..=i {
            let mut s = a[i][j];
            for k in 0..j {
                s -= l[i][k] * l[j][k];
            }
            if i == j {
                if s <= 0.0 {
                    return Err("matrix is not positive definite".to_owned());
                }
                l[i][i] = s.sqrt();
            } else {
                l[i][j] = s / l[j][j];
            }
        }
    }
    Ok(l)
}
fn forward(l: &Mat, b: &[f64]) -> Vec<f64> {
    let n = l.len();
    let mut x = vec![0.0; n];
    for i in 0..n {
        let mut s = b[i];
        for k in 0..i {
            s -= l[i][k] * x[k];
        }
        x[i] = s / l[i][i];
    }
    x
}
fn backward(l: &Mat, b: &[f64]) -> Vec<f64> {
    let n = l.len();
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let mut s = b[i];
        for k in i + 1..n {
            s -= l[k][i] * x[k];
        }
        x[i] = s / l[i][i];
    }
    x
}
fn logdet_from_chol(l: &Mat) -> f64 {
    (0..l.len()).map(|i| 2.0 * l[i][i].ln()).sum()
}
fn inverse(a: &Mat) -> R<Mat> {
    let l = chol(a)?;
    let n = a.len();
    let mut inv = vec![vec![0.0; n]; n];
    for j in 0..n {
        let mut e = vec![0.0; n];
        e[j] = 1.0;
        let col = backward(&l, &forward(&l, &e));
        for i in 0..n {
            inv[i][j] = col[i];
        }
    }
    Ok(inv)
}
/// Eigenvalues and eigenvectors (columns) of a symmetric matrix by cyclic Jacobi rotations.
fn jacobi(a: &Mat) -> (Vec<f64>, Mat) {
    let n = a.len();
    let mut a = a.clone();
    let mut v: Mat = (0..n)
        .map(|i| (0..n).map(|j| if i == j { 1.0 } else { 0.0 }).collect())
        .collect();
    for _ in 0..100 {
        let mut off = 0.0;
        for i in 0..n {
            for j in i + 1..n {
                off += a[i][j] * a[i][j];
            }
        }
        if off.sqrt() < 1e-300 {
            break;
        }
        for p in 0..n {
            for q in p + 1..n {
                if a[p][q].abs() < 1e-300 {
                    continue;
                }
                let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
                let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                for k in 0..n {
                    let (akp, akq) = (a[k][p], a[k][q]);
                    a[k][p] = c * akp - s * akq;
                    a[k][q] = s * akp + c * akq;
                }
                for k in 0..n {
                    let (apk, aqk) = (a[p][k], a[q][k]);
                    a[p][k] = c * apk - s * aqk;
                    a[q][k] = s * apk + c * aqk;
                }
                for k in 0..n {
                    let (vkp, vkq) = (v[k][p], v[k][q]);
                    v[k][p] = c * vkp - s * vkq;
                    v[k][q] = s * vkp + c * vkq;
                }
            }
        }
    }
    ((0..n).map(|i| a[i][i]).collect(), v)
}

// ---------------------------------------------------------------- model and data

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    IvBolus,
    Oral1,
    Pk2IvBolus,
    IdentityLinear,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ErrKind {
    Additive,
    Proportional,
    Combined,
    CombinedSd,
}

struct Spec {
    kind: Kind,
    theta: BTreeMap<String, f64>,
    re: Vec<String>,
    q: usize,
    omega: Mat,
    omega_inv: Mat,
    log_det_omega: f64,
    err: ErrKind,
    a: f64,
    b: f64,
    z_powers: Vec<u32>,
    n_theta: usize,
    n_omega: usize,
    n_xi: usize,
}

struct Subj {
    id: String,
    dose: f64,
    t: Vec<f64>,
    y: Vec<f64>,
}

fn spec_of(model: &PopModel) -> R<Spec> {
    let kind = match model.name.as_str() {
        "pk1.iv_bolus" => Kind::IvBolus,
        "pk1.oral_1" => Kind::Oral1,
        "pk2.iv_bolus" => Kind::Pk2IvBolus,
        "identity_linear" => Kind::IdentityLinear,
        other => return Err(format!("naive_pop does not know the model {other}")),
    };
    let err = match model.error_model.as_str() {
        "additive" => ErrKind::Additive,
        "proportional" => ErrKind::Proportional,
        "combined" => ErrKind::Combined,
        "combined_sd" => ErrKind::CombinedSd,
        other => return Err(format!("unknown error model {other}")),
    };
    let mut theta = BTreeMap::new();
    for name in model.theta.keys() {
        theta.insert(
            name.clone(),
            model.theta_f64(name).map_err(|e| e.to_string())?,
        );
    }
    let omega = model.omega_f64().map_err(|e| e.to_string())?;
    let q = model.random_effects.len();
    let l = chol(&omega)?;
    let z_powers = model.z_powers.clone().unwrap_or_default();
    if kind == Kind::IdentityLinear && z_powers.len() != q {
        return Err("identity_linear needs one z power per random effect".to_owned());
    }
    Ok(Spec {
        kind,
        theta,
        re: model.random_effects.clone(),
        q,
        omega_inv: inverse(&omega)?,
        log_det_omega: logdet_from_chol(&l),
        omega,
        err,
        a: model.xi_f64("a").map_err(|e| e.to_string())?,
        b: model.xi_f64("b").map_err(|e| e.to_string())?,
        z_powers,
        n_theta: model.theta.len(),
        n_omega: if model.omega_is_block() {
            q * (q + 1) / 2
        } else {
            q
        },
        n_xi: model.xi.len(),
    })
}

fn subj_of(s: &PopSubject) -> R<Subj> {
    Ok(Subj {
        id: s.id.clone(),
        dose: s.dose_f64().map_err(|e| e.to_string())?,
        t: s.times_f64().map_err(|e| e.to_string())?,
        y: s.y_f64().map_err(|e| e.to_string())?,
    })
}

impl Spec {
    fn par(&self, name: &str, eta: &[Jet]) -> R<Jet> {
        let th = self
            .theta
            .get(name)
            .ok_or_else(|| format!("no typical value named {name}"))?;
        let base = Jet::constant(*th, self.q);
        Ok(match self.re.iter().position(|r| r == name) {
            Some(i) => base * eta[i].exp(),
            None => base,
        })
    }

    /// The prediction at the time `t` as a jet over the random effects.
    fn predict(&self, dose: f64, t: f64, eta: &[Jet]) -> R<Jet> {
        let q = self.q;
        let k = |x: f64| Jet::constant(x, q);
        let d = k(dose);
        let tt = k(t);
        match self.kind {
            Kind::IvBolus => {
                let (v, cl) = (self.par("v", eta)?, self.par("cl", eta)?);
                let kel = &cl / &v;
                Ok((&d / &v) * (-(kel * tt)).exp())
            }
            Kind::Oral1 => {
                let (v, cl, ka) = (
                    self.par("v", eta)?,
                    self.par("cl", eta)?,
                    self.par("ka", eta)?,
                );
                let kel = &cl / &v;
                let diff = &ka - &kel;
                let front = (&d * &ka) / (&v * &diff);
                Ok(front * ((-(&kel * &tt)).exp() - (-(&ka * &tt)).exp()))
            }
            Kind::Pk2IvBolus => {
                let (cl, vc, qq, vp) = (
                    self.par("cl", eta)?,
                    self.par("vc", eta)?,
                    self.par("q", eta)?,
                    self.par("vp", eta)?,
                );
                let k10 = &cl / &vc;
                let k12 = &qq / &vc;
                let k21 = &qq / &vp;
                let s = &k10 + &k12 + &k21;
                let dk = &k10 - &k21;
                let disc = &dk * &dk + &k12 * (&k12 + k(2.0) * &k10 + k(2.0) * &k21);
                let r = disc.sqrt();
                let al = (&s + &r) / k(2.0);
                let be = (&k10 * &k21) / &al;
                let wa = (&al - &k21) / (&al - &be);
                let wb = (&k21 - &be) / (&al - &be);
                Ok((&d / &vc) * (wa * (-(&al * &tt)).exp() + wb * (-(&be * &tt)).exp()))
            }
            Kind::IdentityLinear => {
                let t1 = self.theta.get("theta1").copied().unwrap_or(0.0);
                let t2 = self.theta.get("theta2").copied().unwrap_or(0.0);
                let mut a = k(t1 + t2 * t);
                for (j, p) in self.z_powers.iter().enumerate() {
                    let z = if *p == 0 { 1.0 } else { t.powi(*p as i32) };
                    a = a + k(z) * &eta[j];
                }
                Ok(a)
            }
        }
    }

    fn variance(&self, yhat: &Jet) -> Jet {
        let a2 = yhat.lift(self.a * self.a);
        let b2 = yhat.lift(self.b * self.b);
        match self.err {
            ErrKind::Additive => a2,
            ErrKind::Proportional => b2 * yhat * yhat,
            ErrKind::Combined => a2 + b2 * yhat * yhat,
            ErrKind::CombinedSd => {
                let s = yhat.lift(self.a) + yhat.lift(self.b) * yhat.abs();
                &s * &s
            }
        }
    }
}

// ---------------------------------------------------------------- one subject

struct Parts {
    yhat: Vec<Jet>,
    s2: Vec<Jet>,
    ell: Jet,
    pen: Jet,
    h: Jet,
}

fn parts(spec: &Spec, s: &Subj, eta: &[f64], sig2_fixed: Option<&[f64]>) -> R<Parts> {
    let q = spec.q;
    let ej: Vec<Jet> = (0..q).map(|i| Jet::var(i, eta[i], q)).collect();
    let mut yhat = Vec::new();
    let mut s2 = Vec::new();
    let mut ell = Jet::constant(0.0, q);
    for (j, t) in s.t.iter().enumerate() {
        let yh = spec.predict(s.dose, *t, &ej)?;
        let var = match sig2_fixed {
            Some(f) => Jet::constant(f[j], q),
            None => spec.variance(&yh),
        };
        if var.v <= 0.0 {
            return Err("non-positive variance".to_owned());
        }
        let r = Jet::constant(s.y[j], q) - &yh;
        ell = ell + var.ln() + &r * &r / &var;
        yhat.push(yh);
        s2.push(var);
    }
    let mut pen = Jet::constant(0.0, q);
    for k in 0..q {
        for l in 0..q {
            pen = pen + Jet::constant(spec.omega_inv[k][l], q) * &ej[k] * &ej[l];
        }
    }
    let h = &ell + &pen;
    Ok(Parts {
        yhat,
        s2,
        ell,
        pen,
        h,
    })
}

/// Conditional mode: Newton on `h` with the exact Hessian and a backtracking search.
fn find_mode(spec: &Spec, s: &Subj, sig2_fixed: Option<&[f64]>) -> R<Vec<f64>> {
    let q = spec.q;
    let mut eta = vec![0.0; q];
    let mut p = parts(spec, s, &eta, sig2_fixed)?;
    for _ in 0..200 {
        let gmax = p.h.g.iter().fold(0.0_f64, |m, x| m.max(x.abs()));
        if gmax < 1e-12 {
            return Ok(eta);
        }
        let hess: Mat = (0..q)
            .map(|i| (0..q).map(|j| p.h.h[i * q + j]).collect())
            .collect();
        // a Newton step if the Hessian is positive definite, else the steepest descent direction
        let step = match chol(&hess) {
            Ok(l) => backward(&l, &forward(&l, &p.h.g)),
            Err(_) => p.h.g.clone(),
        };
        let mut lam = 1.0;
        let mut accepted = false;
        for _ in 0..60 {
            let trial: Vec<f64> = (0..q).map(|i| eta[i] - lam * step[i]).collect();
            if let Ok(pn) = parts(spec, s, &trial, sig2_fixed)
                && pn.h.v <= p.h.v + 1e-13 * (1.0 + p.h.v.abs())
            {
                eta = trial;
                p = pn;
                accepted = true;
                break;
            }
            lam /= 2.0;
        }
        if !accepted {
            if gmax < 1e-7 {
                return Ok(eta);
            }
            return Err(format!("subject {}: the search for the mode failed", s.id));
        }
        let smax = step.iter().fold(0.0_f64, |m, x| m.max((lam * x).abs()));
        if smax < 1e-15 {
            return Ok(eta);
        }
    }
    Err(format!("subject {}: the mode did not converge", s.id))
}

fn values(js: &[Jet]) -> Vec<f64> {
    js.iter().map(|j| j.v).collect()
}

/// The pieces of the forms at one eta.
struct Pieces {
    g: Vec<Vec<f64>>,
    yhat: Vec<f64>,
    s2: Vec<f64>,
    ds2: Vec<Vec<f64>>,
    y: Vec<f64>,
    eta: Vec<f64>,
    parts: Parts,
}

fn pieces(spec: &Spec, s: &Subj, eta: &[f64], sig2_fixed: Option<&[f64]>) -> R<Pieces> {
    let p = parts(spec, s, eta, sig2_fixed)?;
    Ok(Pieces {
        g: p.yhat.iter().map(|j| j.g.clone()).collect(),
        yhat: values(&p.yhat),
        s2: values(&p.s2),
        ds2: p.s2.iter().map(|j| j.g.clone()).collect(),
        y: s.y.clone(),
        eta: eta.to_vec(),
        parts: p,
    })
}

struct Lin {
    ofv: f64,
    e: Vec<f64>,
    c: Mat,
    rho: Vec<f64>,
    w: Vec<f64>,
    logdet: f64,
    dist: f64,
}

/// POP-OBJ-02 and POP-OBJ-03: E = yhat - G eta, C = G Omega G' + diag(s2).
fn lin_form(spec: &Spec, pc: &Pieces) -> R<Lin> {
    let n = pc.yhat.len();
    let q = spec.q;
    let e: Vec<f64> = (0..n)
        .map(|j| pc.yhat[j] - (0..q).map(|k| pc.g[j][k] * pc.eta[k]).sum::<f64>())
        .collect();
    let mut c = vec![vec![0.0; n]; n];
    for i in 0..n {
        for j in 0..n {
            let mut s = 0.0;
            for k in 0..q {
                for l in 0..q {
                    s += pc.g[i][k] * spec.omega[k][l] * pc.g[j][l];
                }
            }
            c[i][j] = s + if i == j { pc.s2[i] } else { 0.0 };
        }
    }
    let l = chol(&c)?;
    let rho: Vec<f64> = (0..n).map(|j| pc.y[j] - e[j]).collect();
    let w = forward(&l, &rho);
    let dist: f64 = w.iter().map(|x| x * x).sum();
    let logdet = logdet_from_chol(&l);
    Ok(Lin {
        ofv: n as f64 * LN_2PI + logdet + dist,
        e,
        c,
        rho,
        w,
        logdet,
        dist,
    })
}

/// POP-OBJ-04: the three Laplace forms at the mode (full convention).
fn laplace_forms(spec: &Spec, pc: &Pieces) -> R<[f64; 3]> {
    let q = spec.q;
    let n = pc.yhat.len();
    let mut a_nv = vec![vec![0.0; q]; q];
    let mut a_ex = vec![vec![0.0; q]; q];
    for j in 0..n {
        for k in 0..q {
            for l in 0..q {
                let gg = pc.g[j][k] * pc.g[j][l] / pc.s2[j];
                a_nv[k][l] += gg;
                a_ex[k][l] += gg + pc.ds2[j][k] * pc.ds2[j][l] / (2.0 * pc.s2[j] * pc.s2[j]);
            }
        }
    }
    let a_full: Mat = (0..q)
        .map(|k| (0..q).map(|l| 0.5 * pc.parts.ell.h[k * q + l]).collect())
        .collect();
    let one = |a: &Mat| -> R<f64> {
        let m: Mat = (0..q)
            .map(|k| (0..q).map(|l| a[k][l] + spec.omega_inv[k][l]).collect())
            .collect();
        let l = chol(&m)?;
        Ok(n as f64 * LN_2PI
            + pc.parts.ell.v
            + pc.parts.pen.v
            + spec.log_det_omega
            + logdet_from_chol(&l))
    };
    Ok([one(&a_full)?, one(&a_ex)?, one(&a_nv)?])
}

/// Where the variance is held (OP-14), or not held at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NaiveFrozen {
    /// The variance follows the prediction at eta (interaction).
    None,
    /// Held at the variance of the population prediction yhat(0).
    Pred0,
    /// Held at the variance of yhat(eta_hat), at the fixed point.
    Mode,
}

impl NaiveFrozen {
    /// The `frozen_sigma` of a case file.
    pub fn parse(text: &str) -> Option<NaiveFrozen> {
        match text {
            "none" => Some(NaiveFrozen::None),
            "pred0" => Some(NaiveFrozen::Pred0),
            "mode" => Some(NaiveFrozen::Mode),
            _ => None,
        }
    }
}

/// The diagnostics of one subject (POP-DIA-*), linearised form, at the conditional mode.
#[derive(Debug, Clone)]
pub struct NaiveDiagnostics {
    pub pred: Vec<f64>,
    pub ipred: Vec<f64>,
    pub iwres: Vec<f64>,
    pub sigma_hat: Vec<f64>,
    pub e: Vec<f64>,
    pub c_diag: Vec<f64>,
    pub cwres_diagonal: Vec<f64>,
    pub cwres_cholesky: Vec<f64>,
    pub cwres_symmetric: Vec<f64>,
    pub sum_w2: f64,
    pub logdet_c: f64,
    /// Jacobian of the prediction in the random effects at the mode, one row per observation (the matrix G).
    pub d_yhat_d_eta: Vec<Vec<f64>>,
    /// Derivative of the residual variance in the random effects at the mode, one row per observation.
    pub d_s2_d_eta: Vec<Vec<f64>>,
}

/// One subject of the result.
#[derive(Debug, Clone)]
pub struct NaiveSubject {
    pub id: String,
    pub eta_hat: Vec<f64>,
    /// The five forms (`POP_FORMS`) in the full convention, plus `exact_marginal` when asked for (q = 1).
    pub forms: BTreeMap<String, f64>,
    pub diagnostics: NaiveDiagnostics,
}

/// Shrinkage and information criteria of the population.
#[derive(Debug, Clone)]
pub struct NaivePopulation {
    pub eta_shrinkage_m_minus_1: Vec<f64>,
    pub eta_shrinkage_m: Vec<f64>,
    pub sd_eta_hat_m_minus_1: Vec<f64>,
    pub sd_eta_hat_m: Vec<f64>,
    pub omega_sd: Vec<f64>,
    pub sd_iwres_n_minus_1: f64,
    pub sd_iwres_n: f64,
    pub eps_shrinkage_n_minus_1: f64,
    pub eps_shrinkage_n: f64,
    pub k: usize,
    /// By form: AIC = OFV + 2k.
    pub aic: BTreeMap<String, f64>,
    /// By form: BIC = OFV + k ln N (N the number of observations).
    pub bic_n_observations: BTreeMap<String, f64>,
    /// By form: BIC = OFV + k ln m (m the number of subjects).
    pub bic_n_subjects: BTreeMap<String, f64>,
}

/// The result of [`naive_pop`].
#[derive(Debug, Clone)]
pub struct NaivePop {
    pub subjects: Vec<NaiveSubject>,
    pub n_obs: usize,
    /// Sum over subjects, by form, full convention.
    pub totals: BTreeMap<String, f64>,
    pub population: NaivePopulation,
}

fn eval_subject(spec: &Spec, s: &Subj, frozen: NaiveFrozen, marginal: bool) -> R<NaiveSubject> {
    let q = spec.q;
    let zeros = vec![0.0; q];
    let (eta, fixed): (Vec<f64>, Option<Vec<f64>>) = match frozen {
        NaiveFrozen::None => (find_mode(spec, s, None)?, None),
        NaiveFrozen::Pred0 => {
            let s2 = values(&parts(spec, s, &zeros, None)?.s2);
            (find_mode(spec, s, Some(&s2))?, Some(s2))
        }
        NaiveFrozen::Mode => {
            // eta = argmin h(eta; s2 held at s2(yhat(eta))): iterate the mode with the variance of the last mode.
            let mut s2 = values(&parts(spec, s, &zeros, None)?.s2);
            let mut eta = find_mode(spec, s, Some(&s2))?;
            for _ in 0..2000 {
                let next = values(&parts(spec, s, &eta, None)?.s2);
                let change = next
                    .iter()
                    .zip(&s2)
                    .fold(0.0_f64, |m, (a, b)| m.max(((a - b) / b).abs()));
                s2 = next;
                eta = find_mode(spec, s, Some(&s2))?;
                if change < 1e-15 {
                    break;
                }
            }
            (eta, Some(s2))
        }
    };
    let pc = pieces(spec, s, &eta, fixed.as_deref())?;
    let lf = lin_form(spec, &pc)?;
    let lap = laplace_forms(spec, &pc)?;
    let pc0 = pieces(spec, s, &zeros, None)?;
    let fo = lin_form(spec, &pc0)?;
    let mut forms = BTreeMap::new();
    forms.insert("linearised".to_owned(), lf.ofv);
    forms.insert("fo".to_owned(), fo.ofv);
    forms.insert("laplace_full".to_owned(), lap[0]);
    forms.insert("laplace_expected".to_owned(), lap[1]);
    forms.insert("laplace_no_variance".to_owned(), lap[2]);
    if marginal {
        forms.insert(
            "exact_marginal".to_owned(),
            exact_marginal_q1(spec, s, &pc)?,
        );
    }
    // diagnostics
    let n = s.t.len();
    let sigma_hat: Vec<f64> = pc.s2.iter().map(|v| v.sqrt()).collect();
    let iwres: Vec<f64> = (0..n)
        .map(|j| (s.y[j] - pc.yhat[j]) / sigma_hat[j])
        .collect();
    let c_diag: Vec<f64> = (0..n).map(|j| lf.c[j][j]).collect();
    let cwres_diagonal: Vec<f64> = (0..n).map(|j| lf.rho[j] / c_diag[j].sqrt()).collect();
    let csym: Mat = (0..n)
        .map(|i| (0..n).map(|j| 0.5 * (lf.c[i][j] + lf.c[j][i])).collect())
        .collect();
    let (vals, vecs) = jacobi(&csym);
    let mut cwres_symmetric = vec![0.0; n];
    for i in 0..n {
        for j in 0..n {
            let wij: f64 = (0..n)
                .map(|k| vecs[i][k] * vecs[j][k] / vals[k].sqrt())
                .sum();
            cwres_symmetric[i] += wij * lf.rho[j];
        }
    }
    let diagnostics = NaiveDiagnostics {
        pred: pc0.yhat.clone(),
        ipred: pc.yhat.clone(),
        iwres,
        sigma_hat,
        e: lf.e.clone(),
        c_diag,
        cwres_diagonal,
        cwres_cholesky: lf.w.clone(),
        cwres_symmetric,
        sum_w2: lf.dist,
        logdet_c: lf.logdet,
        d_yhat_d_eta: pc.g.clone(),
        d_s2_d_eta: pc.ds2.clone(),
    };
    Ok(NaiveSubject {
        id: s.id.clone(),
        eta_hat: eta,
        forms,
        diagnostics,
    })
}

/// -2 ln of the marginal likelihood by the trapezoidal rule on [-13 sd, 13 sd] of the prior (q = 1 only).
fn exact_marginal_q1(spec: &Spec, s: &Subj, mode: &Pieces) -> R<f64> {
    if spec.q != 1 {
        return Err("the exact marginal is computed for one random effect only".to_owned());
    }
    let omega = spec.omega[0][0];
    let sd = omega.sqrt();
    let hess = mode.parts.h.h[0];
    let post = (2.0 / hess).sqrt();
    let step = (0.6 * post).min(sd / 2.0);
    let nodes = (26.0 * sd / step).ceil() as usize + 1;
    let step = 26.0 * sd / (nodes - 1) as f64;
    let n = s.t.len() as f64;
    let mut logs = Vec::with_capacity(nodes);
    for i in 0..nodes {
        let eta = -13.0 * sd + i as f64 * step;
        // far in the tail the prediction can underflow to 0 and the variance with it: the likelihood of a
        // positive observation is then 0 (r^2 / s2 grows without bound), the node contributes nothing
        let g = match parts(spec, s, &[eta], None) {
            Ok(p) => {
                -0.5 * (p.ell.v + n * LN_2PI) - 0.5 * (eta * eta / omega + (LN_2PI + omega.ln()))
            }
            Err(_) => f64::NEG_INFINITY,
        };
        logs.push(g);
    }
    let top = logs.iter().fold(f64::NEG_INFINITY, |m, x| m.max(*x));
    let sum: f64 = logs.iter().map(|g| (g - top).exp()).sum();
    Ok(-2.0 * (top + (step * sum).ln()))
}

fn sd_div(x: &[f64], div: f64) -> f64 {
    let m = x.iter().sum::<f64>() / x.len() as f64;
    (x.iter().map(|v| (v - m) * (v - m)).sum::<f64>() / div).sqrt()
}

/// The population objective of a case in double precision, independent of every other implementation.
///
/// `frozen` follows `frozen_sigma` of the exact cases; `marginal` adds the exact marginal to the forms (q = 1).
pub fn naive_pop(
    model: &PopModel,
    subjects: &[PopSubject],
    frozen: NaiveFrozen,
    marginal: bool,
) -> Result<NaivePop, OracleError> {
    wrap(naive_pop_inner(model, subjects, frozen, marginal))
}

fn naive_pop_inner(
    model: &PopModel,
    subjects: &[PopSubject],
    frozen: NaiveFrozen,
    marginal: bool,
) -> R<NaivePop> {
    let spec = spec_of(model)?;
    let mut out = Vec::new();
    let mut n_obs = 0usize;
    for s in subjects {
        let sj = subj_of(s)?;
        n_obs += sj.t.len();
        out.push(eval_subject(&spec, &sj, frozen, marginal)?);
    }
    let mut totals = BTreeMap::new();
    let names: Vec<String> = out
        .first()
        .map(|s| s.forms.keys().cloned().collect())
        .unwrap_or_default();
    for name in &names {
        totals.insert(name.clone(), out.iter().map(|s| s.forms[name]).sum::<f64>());
    }
    // population diagnostics
    let m = out.len() as f64;
    let q = spec.q;
    let mut sd1 = Vec::new();
    let mut sd0 = Vec::new();
    let mut om_sd = Vec::new();
    for k in 0..q {
        let col: Vec<f64> = out.iter().map(|s| s.eta_hat[k]).collect();
        sd1.push(sd_div(&col, m - 1.0));
        sd0.push(sd_div(&col, m));
        om_sd.push(spec.omega[k][k].sqrt());
    }
    let iw: Vec<f64> = out
        .iter()
        .flat_map(|s| s.diagnostics.iwres.iter().copied())
        .collect();
    let nn = iw.len() as f64;
    let k_total = spec.n_theta + spec.n_omega + spec.n_xi;
    let mut aic = BTreeMap::new();
    let mut bic_n = BTreeMap::new();
    let mut bic_m = BTreeMap::new();
    for name in POP_FORMS {
        let total = totals.get(name).copied().unwrap_or(f64::NAN);
        aic.insert(name.to_owned(), total + 2.0 * k_total as f64);
        bic_n.insert(name.to_owned(), total + k_total as f64 * nn.ln());
        bic_m.insert(name.to_owned(), total + k_total as f64 * m.ln());
    }
    let population = NaivePopulation {
        eta_shrinkage_m_minus_1: (0..q).map(|k| 1.0 - sd1[k] / om_sd[k]).collect(),
        eta_shrinkage_m: (0..q).map(|k| 1.0 - sd0[k] / om_sd[k]).collect(),
        sd_eta_hat_m_minus_1: sd1,
        sd_eta_hat_m: sd0,
        omega_sd: om_sd,
        sd_iwres_n_minus_1: sd_div(&iw, nn - 1.0),
        sd_iwres_n: sd_div(&iw, nn),
        eps_shrinkage_n_minus_1: 1.0 - sd_div(&iw, nn - 1.0),
        eps_shrinkage_n: 1.0 - sd_div(&iw, nn),
        k: k_total,
        aic,
        bic_n_observations: bic_n,
        bic_n_subjects: bic_m,
    };
    Ok(NaivePop {
        subjects: out,
        n_obs,
        totals,
        population,
    })
}
