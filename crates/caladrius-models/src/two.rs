//! Two-compartment models (`specs/models.md` section 11): concentrations, areas, first moments,
//! derived quantities and closed-form partial derivatives, all as positive combinations of the
//! one-compartment kernels of `kernel` (MOD-2C-06): C = (1/Vc)·[wα·Φ(α) + wβ·Φ(β)].

use std::collections::BTreeMap;

use crate::kernel::{Kernel, Partials, Rate, first_order_peak};
use crate::model::Input;
use crate::params2::{self, Disposition, Set};
use crate::{Derivatives, Jacobian, ModelError, ModelInput, ModelOutput};

/// The checked parameters and the kernel of `input`; times are checked here too (MOD-GEN-04).
fn prepare(input: &ModelInput) -> Result<(Disposition, Kernel), ModelError> {
    let p = params2::resolve(input.model, &input.params, input.dose)?;
    if let Some(index) = input.times.iter().position(|t| !t.is_finite()) {
        return Err(ModelError::NonFiniteTime { index });
    }
    let kernel = Kernel {
        input: input.model.input(),
        dose: input.dose,
        ka: p.ka,
        dur: p.dur,
    };
    Ok((p, kernel))
}

/// Concentration, AUC(0, t) and AUMC(0, t) at time `t` since the dose (MOD-2C-06 to 14). The
/// moment is taken about the dose time: with a lag it is M₀(s) + tlag·AUC₀(s), s = t − tlag.
fn at(p: &Disposition, kernel: &Kernel, t: f64) -> (f64, f64, f64) {
    let s = t - p.tlag;
    let (c_alpha, auc_alpha) = kernel.conc_auc(p.alpha, s);
    let (c_beta, auc_beta) = kernel.conc_auc(p.beta, s);
    let combine = |x: f64, y: f64| (p.w_alpha * x + p.w_beta * y) / p.vc;
    let conc = combine(c_alpha, c_beta);
    let auc = combine(auc_alpha, auc_beta);
    let moment = combine(kernel.moment(p.alpha, s), kernel.moment(p.beta, s));
    let aumc = if s <= 0.0 { 0.0 } else { moment + p.tlag * auc };
    (conc, auc, aumc)
}

/// MOD-2C-15: the single maximum of first-order input lies between the one-compartment peak times
/// of (ka, α) and (ka, β); bisection on the sign of C′ until the bracket is two adjacent doubles.
fn first_order_peak_time(p: &Disposition, kernel: &Kernel) -> f64 {
    let (x_alpha, x_beta) = (
        first_order_peak(p.alpha, p.ka),
        first_order_peak(p.beta, p.ka),
    );
    let (mut lo, mut hi) = (x_alpha.min(x_beta), x_alpha.max(x_beta));
    let slope = |s: f64| p.w_alpha * kernel.slope(p.alpha, s) + p.w_beta * kernel.slope(p.beta, s);
    for _ in 0..2200 {
        let mid = lo + (hi - lo) / 2.0;
        if mid <= lo || mid >= hi {
            break;
        }
        let d = slope(mid);
        if d > 0.0 {
            lo = mid;
        } else if d < 0.0 {
            hi = mid;
        } else {
            return mid;
        }
    }
    lo + (hi - lo) / 2.0
}

/// Evaluates a two-compartment model (MOD-2C-01 to 22).
pub(crate) fn run(input: &ModelInput) -> Result<ModelOutput, ModelError> {
    let (p, kernel) = prepare(input)?;
    let dose = input.dose;
    let mut conc = Vec::with_capacity(input.times.len());
    let mut auc = Vec::with_capacity(input.times.len());
    let mut aumc = Vec::with_capacity(input.times.len());
    for &t in &input.times {
        let (c, a, m) = at(&p, &kernel, t);
        conc.push(c);
        auc.push(a);
        aumc.push(m);
    }

    let mut secondary = BTreeMap::new();
    let mut put = |name: &str, value: f64| {
        secondary.insert(name.to_string(), value);
    };
    for (name, value) in [
        ("cl", p.cl),
        ("vc", p.vc),
        ("q", p.q),
        ("vp", p.vp),
        ("k10", p.k10),
        ("k12", p.k12),
        ("k21", p.k21),
        ("alpha", p.alpha),
        ("beta", p.beta),
        ("a", p.a),
        ("b", p.b),
        ("w_alpha", p.w_alpha),
        ("w_beta", p.w_beta),
    ] {
        put(name, value);
    }
    let ln2 = std::f64::consts::LN_2;
    let vss = p.vc + p.vp;
    let mrt_system = vss / p.cl;
    let auc_inf = dose / p.cl;
    put("half_life", ln2 / p.beta);
    put("half_life_alpha", ln2 / p.alpha);
    put("vss", vss);
    put("vz", p.cl / p.beta);
    // D/B = vc/wβ, also defined at dose 0 (decision of card T-032).
    put("v_extrap", p.vc / p.w_beta);
    put("auc_inf", auc_inf);
    put("mrt_system", mrt_system);
    // The mean input time m of MOD-2C-14.
    let input_time = match kernel.input {
        Input::Bolus => 0.0,
        Input::ZeroOrder => p.dur / 2.0 + p.tlag,
        Input::FirstOrder => 1.0 / p.ka + p.tlag,
    };
    let mrt = mrt_system + input_time;
    put("mrt", mrt);
    put("aumc_inf", auc_inf * mrt);
    if input.model.has_lag() {
        put("tlag", p.tlag);
    }
    match kernel.input {
        Input::Bolus => put("c0", dose / p.vc),
        Input::ZeroOrder => {
            put("dur", p.dur);
            put("rate", dose / p.dur);
            put("tmax_pred", p.dur + p.tlag);
            put("cmax_pred", at(&p, &kernel, p.dur + p.tlag).0);
        }
        Input::FirstOrder => {
            put("ka", p.ka);
            // Not defined at dose 0: the curve is flat (decision of card T-032).
            if dose > 0.0 {
                let peak = first_order_peak_time(&p, &kernel);
                put("tmax_pred", peak + p.tlag);
                put("cmax_pred", at(&p, &kernel, peak + p.tlag).0);
            }
            // MOD-2C-09: the oral-profile coefficients, only where ka is not close to an exponent
            // (they grow without bound there and lose their digits).
            let separated = |x: f64| (p.ka - x).abs() >= 1e-2 * p.ka;
            if separated(p.alpha) && separated(p.beta) {
                put("a_oral", p.ka * p.a / (p.ka - p.alpha));
                put("b_oral", p.ka * p.b / (p.ka - p.beta));
            }
        }
    }
    let overflow = |what: String| Err(ModelError::Overflow { what });
    for (i, t) in input.times.iter().enumerate() {
        let finite = |v: &Vec<f64>| v.get(i).is_some_and(|x| x.is_finite());
        if !finite(&conc) {
            return overflow(format!("the concentration at time {t}"));
        }
        if !finite(&auc) {
            return overflow(format!("the AUC at time {t}"));
        }
        if !finite(&aumc) {
            return overflow(format!("the AUMC at time {t}"));
        }
    }
    if let Some((name, _)) = secondary.iter().find(|(_, v)| !v.is_finite()) {
        return overflow(format!("the secondary parameter `{name}`"));
    }
    Ok(ModelOutput {
        conc,
        auc,
        aumc,
        secondary,
    })
}

// ---------------------------------------------------------------- partial derivatives

/// 8-point Gauss–Legendre nodes (positive half) and weights on [−1, 1].
const GL_NODES: [f64; 4] = [
    0.183_434_642_495_649_8,
    0.525_532_409_916_329,
    0.796_666_477_413_626_7,
    0.960_289_856_497_536_3,
];
const GL_WEIGHTS: [f64; 4] = [
    0.362_683_783_378_362,
    0.313_706_645_877_887_3,
    0.222_381_034_453_374_5,
    0.101_228_536_290_376_3,
];

/// Φ(α, s) − Φ(β, s). When the exponents are close on the time scale of s (d·s <= 1) the
/// difference would cancel, so it is the integral of ∂Φ/∂λ over [β, α] by Gauss–Legendre (exact to
/// rounding there: Φ is an entire function of λ varying on the scale 1/s), with every node written
/// about the mean of α and β like the two kernels themselves; otherwise the difference, which then
/// keeps all but a few bits.
fn kernel_difference(
    p: &Disposition,
    kernel: &Kernel,
    s: f64,
    at_alpha: &Partials,
    at_beta: &Partials,
) -> f64 {
    if !close_on_scale(p, s) {
        return at_alpha.value - at_beta.value;
    }
    let (center, half) = ((p.alpha + p.beta) / 2.0, p.d / 2.0);
    let node = |offset: f64| kernel.partials(Rate { center, offset }, s).rate;
    let mut sum = 0.0;
    for (x, w) in GL_NODES.iter().zip(GL_WEIGHTS) {
        sum += w * (node(half * x) + node(-half * x));
    }
    half * sum
}

/// Whether the two exponents are close on the time scale of s (d·s <= 1).
fn close_on_scale(p: &Disposition, s: f64) -> bool {
    p.d * s <= 1.0
}

/// The rates α and β for the derivatives at s: about their mean when they are close on the time
/// scale of s (see [`Rate`]), as they are otherwise.
fn rates(p: &Disposition, s: f64) -> (Rate, Rate) {
    if close_on_scale(p, s) {
        let (center, half) = ((p.alpha + p.beta) / 2.0, p.d / 2.0);
        (
            Rate {
                center,
                offset: half,
            },
            Rate {
                center,
                offset: -half,
            },
        )
    } else {
        (Rate::plain(p.alpha), Rate::plain(p.beta))
    }
}

/// ∂(V, α, β, w)/∂θ of MOD-2C-19 for a parameter θ of the clearance or the micro set, in forms
/// with no difference of nearly equal numbers (with m = α − k10, n = k10 − β, d = α − β,
/// S = k10 + k12 + k21 and k12·k21/d³ = wα·wβ/d). `None` for a name of another set.
fn psi_gradient(p: &Disposition, name: &str) -> Option<[f64; 4]> {
    let (d, wa, wb) = (p.d, p.w_alpha, p.w_beta);
    let ww = wa * wb / d;
    let sum = p.k10 + p.k12 + p.k21;
    let g = match (p.set, name) {
        (Set::Clearance, "cl") => [0.0, wa / p.vc, wb / p.vc, 2.0 * ww / p.vc],
        (Set::Clearance, "vc") => [
            1.0,
            -p.alpha * wa / p.vc,
            -p.beta * wb / p.vc,
            -sum * ww / p.vc,
        ],
        (Set::Clearance, "q") => [
            0.0,
            p.alpha * p.m / (d * p.q),
            p.beta * p.n / (d * p.q),
            -2.0 * p.k10 * ww / p.q,
        ],
        (Set::Clearance, "vp") => [
            0.0,
            -p.k21 * p.m / (p.vp * d),
            -p.k21 * p.n / (p.vp * d),
            sum * ww / p.vp,
        ],
        (Set::Micro, "k10") => [0.0, wa, wb, 2.0 * ww],
        (Set::Micro, "k12") => {
            // dw/dk12 = k21·(k12 + k21 − k10)/d³; the bracket is formed as (k21 − k10) + k12.
            let u = (p.k21 - p.k10) + p.k12;
            [0.0, p.alpha / d, -p.beta / d, ww * u / p.k12]
        }
        (Set::Micro, "k21") => [0.0, p.m / d, p.n / d, -sum * ww / p.k21],
        (Set::Micro, "vc") => [1.0, 0.0, 0.0, 0.0],
        _ => return None,
    };
    Some(g)
}

/// Closed-form ∂C/∂θ for every parameter of `input` (MOD-2C-17 to 19). Columns in the order of
/// `input.params`; 0 before the dose and before the lag.
pub(crate) fn jacobian(input: &ModelInput) -> Result<Jacobian, ModelError> {
    let (p, kernel) = prepare(input)?;
    let names: Vec<String> = input.params.keys().cloned().collect();
    let mut columns: Vec<Vec<f64>> = vec![Vec::with_capacity(input.times.len()); names.len()];
    for &t in &input.times {
        let s = t - p.tlag;
        let (rate_alpha, rate_beta) = rates(&p, s);
        let at_alpha = kernel.partials(rate_alpha, s);
        let at_beta = kernel.partials(rate_beta, s);
        let (wa, wb, v) = (p.w_alpha, p.w_beta, p.vc);
        let conc = (wa * at_alpha.value + wb * at_beta.value) / v;
        // ∂C/∂(V, α, β, w), MOD-2C-18.
        let c_psi = [
            -conc / v,
            wa * at_alpha.rate / v,
            wb * at_beta.rate / v,
            kernel_difference(&p, &kernel, s, &at_alpha, &at_beta) / v,
        ];
        let combine = |x: f64, y: f64| (wa * x + wb * y) / v;
        for (name, column) in names.iter().zip(columns.iter_mut()) {
            let value = match name.as_str() {
                "ka" => combine(at_alpha.ka, at_beta.ka),
                "dur" => combine(at_alpha.dur, at_beta.dur),
                "tlag" => -combine(at_alpha.s, at_beta.s),
                // Macro set: C = (a·Φ(α) + b·Φ(β))/D, D > 0 (MOD-2C-18).
                "a" => at_alpha.value / input.dose,
                "b" => at_beta.value / input.dose,
                "alpha" => p.a / input.dose * at_alpha.rate,
                "beta" => p.b / input.dose * at_beta.rate,
                other => match psi_gradient(&p, other) {
                    Some(grad) => grad.iter().zip(c_psi).map(|(g, c)| g * c).sum(),
                    None => 0.0,
                },
            };
            column.push(value);
        }
    }
    Ok(Jacobian {
        parameters: names,
        columns,
        method: Derivatives::Analytic,
    })
}
