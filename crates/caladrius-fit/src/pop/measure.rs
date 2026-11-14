//! Measurements behind the constants of OP-11 (T-154), ignored by default: they print, they assert
//! nothing. Run with `cargo test -p caladrius-fit --lib pop::measure -- --ignored --nocapture`.

use caladrius_testkit::{
    AbsoluteTolerance, PopFitCase, PopModel, PopSealed, PopSubject, list_pop_fit_cases,
    load_pop_fit_case, load_pop_sealed, load_pop_sealed_replicate,
};

use super::*;

fn num(text: &str) -> f64 {
    text.trim().parse().unwrap()
}

fn request_of_shape(shape: &str, model: &PopModel, subjects: &[PopSubject]) -> PopRequest {
    let error_init = match model.error_model.as_str() {
        "additive" => vec![num(&model.xi["a"])],
        "proportional" => vec![num(&model.xi["b"])],
        _ => vec![num(&model.xi["a"]), num(&model.xi["b"])],
    };
    PopRequest {
        model: Some(model.name.clone()),
        initial: model
            .theta
            .iter()
            .map(|(k, v)| (k.clone(), num(v)))
            .collect(),
        random_effects: Some(model.random_effects.clone()),
        omega: Some(
            model
                .omega
                .iter()
                .map(|r| r.iter().map(|t| num(t)).collect())
                .collect(),
        ),
        omega_shape: Some(shape.to_string()),
        error_model: Some(model.error_model.clone()),
        error_init: Some(error_init),
        model_options: ModelOptions {
            z_powers: model.z_powers.clone(),
        },
        options: PopOptions {
            method: Some("foce_i".to_string()),
            ofv_convention: Some("full".to_string()),
            ..PopOptions::default()
        },
        subjects: Some(
            subjects
                .iter()
                .map(|s| SubjectInput {
                    id: s.id.clone(),
                    dose: num(&s.dose),
                    time: s.time.iter().map(|t| num(t)).collect(),
                    y: s.y.iter().map(|t| Some(num(t))).collect(),
                    duration: None,
                })
                .collect(),
        ),
        ..PopRequest::default()
    }
}

/// The request of `oracle_pop.rs::fit_request`, built in Rust.
fn request_of(case: &PopFitCase, model: &PopModel, subjects: &[PopSubject]) -> PopRequest {
    request_of_shape(&case.design.omega_shape, model, subjects)
}

/// The request of `oracle_pop.rs::check_sealed_recovery` for one replicate (the start of
/// `sealed_start`: θ × 1.25, Ω × 1.5, ξ × 1.25).
fn sealed_request(sealed: &PopSealed, rep: usize) -> PopRequest {
    let mut m = sealed.model.clone();
    let t = &sealed.truth;
    m.theta = t
        .typical_values
        .iter()
        .map(|(k, v)| (k.clone(), format!("{:e}", num(v) * 1.25)))
        .collect();
    m.omega = t
        .omega_full
        .iter()
        .map(|r| r.iter().map(|x| format!("{:e}", num(x) * 1.5)).collect())
        .collect();
    m.xi =
        t.xi.iter()
            .map(|(k, v)| (k.clone(), format!("{:e}", num(v) * 1.25)))
            .collect();
    let subjects = load_pop_sealed_replicate(sealed, rep).unwrap();
    let shape = if m.omega_is_block() {
        "block"
    } else {
        "diagonal"
    };
    request_of_shape(shape, &m, &subjects)
}

fn labelled(estimate: &Estimate, label: &str) -> f64 {
    match label {
        "theta1" | "theta2" => estimate.theta[label],
        "xi.a" => estimate.xi["a"],
        other => {
            let idx: Vec<usize> = other
                .trim_start_matches("omega_full")
                .split(['[', ']'])
                .filter(|t| !t.is_empty())
                .map(|t| t.parse().unwrap())
                .collect();
            estimate.omega_full[idx[0]][idx[1]]
        }
    }
}

/// The worst error of each exact fit case against its tolerance, from each start, and the ratio
/// ω̂/ω⁰ of the flag `omega_collapse`.
#[test]
#[ignore = "a measurement for the report of T-154"]
fn measure_the_exact_fits() {
    for name in list_pop_fit_cases().unwrap() {
        let case = load_pop_fit_case(&name).unwrap();
        for start in &case.starts {
            let request = request_of(&case, &start.model, &case.subjects);
            let start_sd: Vec<f64> = request
                .omega
                .as_ref()
                .unwrap()
                .iter()
                .enumerate()
                .map(|(i, r)| r[i].sqrt())
                .collect();
            let answer = run(&request).unwrap();
            let estimate = answer.estimate.clone().unwrap();
            let mut worst = (String::new(), 0.0_f64, 0.0_f64, 0.0_f64);
            for label in case.labels() {
                let expected = case.expected_value(&label).unwrap();
                let tol = match case.bound_gradient(&label) {
                    Some(g) => AbsoluteTolerance::POP_OFV.abs / g,
                    None => case
                        .curvature_bound(&label, AbsoluteTolerance::POP_OFV)
                        .unwrap(),
                };
                let err = (labelled(&estimate, &label) - expected).abs();
                if err / tol > worst.1 {
                    worst = (label.clone(), err / tol, err, tol);
                }
            }
            let ratios: Vec<String> = answer
                .random_effects
                .iter()
                .zip(&start_sd)
                .map(|(re, s)| format!("{:.3e}", estimate.omega_sd[re] / s))
                .collect();
            let ratios = ratios.join(", ");
            let ofv_err = (answer.ofv - num(&case.expected.ofv_full)).abs();
            let flags = answer
                .covariance
                .as_ref()
                .map(|c| c.flags.clone())
                .unwrap_or_default();
            println!(
                "{name} from {}: status {} iterations {:?} worst {} err {:.3e} tol {:.3e} ratio {:.3e}; ofv err {:.3e} (tol 1e-7); omega ratio {ratios}; flags {flags:?}",
                start.name,
                answer.status,
                answer.iterations,
                worst.0,
                worst.2,
                worst.3,
                worst.1,
                ofv_err
            );
        }
        let gradients: Vec<String> = case
            .labels()
            .iter()
            .filter_map(|l| case.bound_gradient(l).map(|g| format!("{l}: {g:.4e}")))
            .collect();
        if !gradients.is_empty() {
            println!("{name}: gradients at the bound {gradients:?}");
        }
    }
}

/// Truncation and noise of the central differences of the gradient and of the second differences
/// of the Hessian, at the fitted point of two problems.
#[test]
#[ignore = "a measurement for the report of T-154"]
fn measure_the_difference_steps() {
    let case = load_pop_fit_case("fit_q1_boundary").unwrap();
    let sealed = load_pop_sealed("accurate").unwrap();
    let problems = [
        (
            "fit_q1_boundary (generating values)",
            request_of(&case, &case.starts[0].model, &case.subjects),
        ),
        ("sealed accurate replicate 1", sealed_request(&sealed, 1)),
    ];
    let mut problems = problems.to_vec();
    // T154_EXTRA = "design:replicate", one more sealed replicate.
    if let Some((design, rep)) = std::env::var("T154_EXTRA")
        .ok()
        .and_then(|v| {
            v.split_once(':')
                .map(|(d, r)| (d.to_string(), r.parse::<usize>().ok()))
        })
        .and_then(|(d, r)| r.map(|r| (d, r)))
    {
        let other = load_pop_sealed(&design).unwrap();
        problems.clear();
        problems.push(("the extra sealed replicate", sealed_request(&other, rep)));
    }
    for (label, request) in problems {
        let setup = Setup::new(&request).unwrap();
        let layout = setup.layout();
        let q = setup.random_effects.len();
        let phi0 = layout.phi(&setup.start);
        let first = setup
            .evaluate_params(
                &setup.start,
                &[],
                Settings {
                    eps: inner::EPS_IN,
                    keep: false,
                    fail_inner: &|_| false,
                },
            )
            .unwrap();
        let problem = outer::Problem {
            setup: &setup,
            layout: &layout,
        };
        let outcome = problem.minimise(phi0, first);
        let phi = outcome.phi.clone();
        let warm = outcome.evaluation.warm(q);
        let f = |x: &[f64]| problem.eval(x, &warm).unwrap().core;
        let f0 = f(&phi);
        // The noise of one evaluation: the same point from a cold start (η = 0).
        let cold = problem.eval(&phi, &[]).unwrap().core;
        if let Some(g) = outcome.gradient.as_ref() {
            let all: Vec<usize> = (0..phi.len()).collect();
            let r = problem.hessian(&phi, &all, &warm).unwrap();
            let eig = crate::linalg::symmetric_eigenvalues(&r);
            let delta = outer::spd_inverse(&r).map(|ri| {
                let mut d = 0.0;
                for a in 0..phi.len() {
                    for b in 0..phi.len() {
                        d += g.total[a] * ri.at(a, b) * g.total[b];
                    }
                }
                0.5 * d
            });
            println!(
                "  gradient {:?}
  R eigenvalues {eig:?}
  Newton decrement {delta:?}",
                g.total
            );
        }
        let rounding = f0.abs() * f64::EPSILON;
        let noise = (f0 - cold).abs().max(rounding);
        println!(
            "{label}: status {:?} after {} iterations, OFV core {f0:.12e}; |warm - cold| {:.2e}; rounding |f|*u {:.2e}; noise used {noise:.2e}",
            outcome.status,
            outcome.iterations,
            (f0 - cold).abs(),
            rounding
        );
        for j in 0..phi.len() {
            // The scale of the engine's steps: |ξ| for an error parameter, max(|φ_j|, 1) otherwise.
            let scale = match layout.coords[j].part() {
                phi::Part::Xi => phi[j].abs(),
                _ => phi[j].abs().max(1.0),
            };
            let lo = layout.lower[j];
            let shifted = |by: f64| {
                let mut x = phi.clone();
                x[j] += by;
                f(&x)
            };
            let central = |frac: f64| {
                let h = frac * scale;
                if phi[j] - h < lo {
                    return f64::NAN;
                }
                (shifted(h) - shifted(-h)) / (2.0 * h)
            };
            let second = |frac: f64| {
                let h = frac * scale;
                if phi[j] - h < lo {
                    return f64::NAN;
                }
                (shifted(h) - 2.0 * f0 + shifted(-h)) / (h * h)
            };
            // Third and fourth derivatives from two large steps (error ∝ h²).
            let (a, b) = (1e-2 * scale, 1e-3 * scale);
            let d3 = 6.0 * (central(1e-2) - central(1e-3)) / (a * a - b * b);
            let d4 = 12.0 * (second(1e-2) - second(1e-3)) / (a * a - b * b);
            println!(
                "  phi[{j}] {:?} = {:.4e}: |f'''| {:.2e} |f''''| {:.2e}",
                layout.coords[j],
                phi[j],
                d3.abs(),
                d4.abs()
            );
            for frac in [1e-3, 1e-4, 1e-5, 1e-6, 1e-7] {
                let h = frac * scale;
                println!(
                    "    h {frac:.0e}: gradient {:.8e} truncation {:.1e} noise {:.1e} | R {:.8e} truncation {:.1e} noise {:.1e}",
                    central(frac),
                    d3.abs() * h * h / 6.0,
                    noise / h,
                    second(frac),
                    d4.abs() * h * h / 12.0,
                    4.0 * noise / (h * h)
                );
            }
            let h_star = libm::cbrt(3.0 * noise / d3.abs()) / scale;
            let k_star = libm::pow(48.0 * noise / d4.abs(), 0.25) / scale;
            println!("    optimum fraction: gradient {h_star:.1e}, Hessian {k_star:.1e}");
        }
    }
}

/// Iterations, statuses, the ratio ω̂/ω⁰, the η-shrinkage and the wall time on the first
/// replicates of a sealed design (`T154_DESIGN`, default `accurate`; `T154_FIRST`, default 1, to
/// `T154_REPLICATES`, default 20).
#[test]
#[ignore = "a measurement for the report of T-154"]
fn measure_the_sealed_design() {
    let design = std::env::var("T154_DESIGN").unwrap_or_else(|_| "accurate".to_string());
    let sealed = load_pop_sealed(&design).unwrap();
    let k: usize = std::env::var("T154_REPLICATES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);
    let mut max_iterations = 0;
    let mut min_ratio = f64::INFINITY;
    let mut max_shrinkage = 0.0_f64;
    let mut total = 0.0;
    let first: usize = std::env::var("T154_FIRST")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    // With `T154_CSV=<path>`, one row per replicate for an independent fit to compare against.
    let mut csv = std::env::var("T154_CSV").ok().map(|path| {
        use std::io::Write;
        let mut file = std::fs::File::create(path).unwrap();
        let mut names: Vec<String> = ["cl", "v", "ka"]
            .iter()
            .filter(|n| sealed.truth.typical_values.contains_key(**n))
            .map(|n| n.to_string())
            .collect();
        for n in sealed.truth.typical_values.keys() {
            if !names.contains(n) {
                names.push(n.clone());
            }
        }
        let res = sealed.model.random_effects.clone();
        writeln!(
            file,
            "# design {design}; engine T-154, release, FOCE-I, OFV in the `full` convention; omega_<re> \
             is the variance Omega_kk (as the request's omega); xi.a and xi.b are a and b on the \
             standard-deviation scale of the request and of error_init (sd = sqrt(a^2 + b^2 yhat^2)); \
             se.<theta> is the printed (sandwich) SE on the user's scale, NA where not computed; \
             at_bound is the flag at_bound of the covariance block; 17 significant digits"
        )
        .unwrap();
        let mut header = vec![
            "replicate".to_string(),
            "status".to_string(),
            "iterations".to_string(),
            "ofv".to_string(),
        ];
        header.extend(names.iter().map(|n| format!("estimate.{n}")));
        header.extend(res.iter().map(|r| format!("omega_{r}")));
        header.extend(["xi.a".to_string(), "xi.b".to_string()]);
        header.extend(names.iter().map(|n| format!("se.{n}")));
        header.push("at_bound".to_string());
        writeln!(file, "{}", header.join(",")).unwrap();
        (file, names, res)
    });
    let g17 = |x: f64| format!("{x:.16e}");
    for rep in first..=k {
        let request = sealed_request(&sealed, rep);
        let start_sd: Vec<f64> = request
            .omega
            .as_ref()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(i, r)| r[i].sqrt())
            .collect();
        let clock = std::time::Instant::now();
        let answer = match run(&request) {
            Ok(a) => a,
            Err(e) => {
                println!("replicate {rep}: refused: {e}");
                if let Some((file, _, _)) = csv.as_mut() {
                    use std::io::Write;
                    writeln!(file, "{rep},refused:{}", e.code.id()).unwrap();
                }
                continue;
            }
        };
        let seconds = clock.elapsed().as_secs_f64();
        if let Some((file, names, res)) = csv.as_mut() {
            use std::io::Write;
            let est = answer.estimate.as_ref().unwrap();
            let se = answer.se.as_ref().unwrap();
            let mut row = vec![
                rep.to_string(),
                answer.status.clone(),
                answer.iterations.unwrap().to_string(),
                g17(answer.ofv),
            ];
            row.extend(names.iter().map(|n| g17(est.theta[n])));
            row.extend(res.iter().map(|r| g17(est.omega_diagonal[r])));
            row.extend(
                ["a", "b"]
                    .iter()
                    .map(|key| est.xi.get(*key).map_or("NA".to_string(), |x| g17(*x))),
            );
            row.extend(names.iter().map(|n| {
                se.theta
                    .get(n)
                    .copied()
                    .flatten()
                    .map_or("NA".to_string(), g17)
            }));
            let at_bound = answer
                .covariance
                .as_ref()
                .is_some_and(|c| c.flags.iter().any(|f| f == "at_bound"));
            row.push(if at_bound { "TRUE" } else { "FALSE" }.to_string());
            writeln!(file, "{}", row.join(",")).unwrap();
        }
        // The typical values and their standard errors as the oracle reads them (`estimate.<name>`,
        // `se.<name>`), for the aggregates over every replicate.
        let json = serde_json::to_value(&answer).unwrap();
        for name in sealed.truth.typical_values.keys() {
            println!(
                "THETA {rep} {} {name} {} {}",
                answer.status,
                json["estimate"][name.as_str()],
                json["se"][name.as_str()]
            );
        }
        if std::env::var("T154_TRACE").is_ok() {
            for row in answer.trace.iter().flatten() {
                println!("TRACE {}", serde_json::to_string(row).unwrap());
            }
        }
        total += seconds;
        let estimate = answer.estimate.as_ref().unwrap();
        let ratios: Vec<f64> = answer
            .random_effects
            .iter()
            .zip(&start_sd)
            .map(|(re, s)| estimate.omega_sd[re] / s)
            .collect();
        let iterations = answer.iterations.unwrap();
        max_iterations = max_iterations.max(iterations);
        for r in &ratios {
            min_ratio = min_ratio.min(*r);
        }
        for s in answer.eta_shrinkage.iter().flatten() {
            max_shrinkage = max_shrinkage.max(*s);
        }
        println!(
            "replicate {rep}: {} in {iterations} iterations, {seconds:.2} s; omega ratio {ratios:.3?}; eta-shrinkage {:.3?}; flags {:?}",
            answer.status,
            answer.eta_shrinkage,
            answer.covariance.as_ref().map(|c| c.flags.clone())
        );
    }
    println!(
        "over {k} replicates: at most {max_iterations} iterations, smallest omega ratio {min_ratio:.3}, largest eta-shrinkage {max_shrinkage:.3}, {total:.1} s"
    );
}

/// The noise of the OFV by subject at the end of the first replicate of `accurate`: the same
/// point solved from the warm start and from η = 0.
#[test]
#[ignore = "a measurement for the report of T-154"]
fn measure_the_inner_noise() {
    let sealed = load_pop_sealed("accurate").unwrap();
    let request = sealed_request(&sealed, 1);
    let setup = Setup::new(&request).unwrap();
    let layout = setup.layout();
    let q = setup.random_effects.len();
    let settings = Settings {
        eps: inner::EPS_IN,
        keep: false,
        fail_inner: &|_| false,
    };
    let first = setup.evaluate_params(&setup.start, &[], settings).unwrap();
    let problem = outer::Problem {
        setup: &setup,
        layout: &layout,
    };
    let outcome = problem.minimise(layout.phi(&setup.start), first);
    let warm = outcome.evaluation.warm(q);
    let a = problem.eval(&outcome.phi, &warm).unwrap();
    let b = problem.eval(&outcome.phi, &[]).unwrap();
    for (i, (x, y)) in a.subjects.iter().zip(&b.subjects).enumerate() {
        let d_eta: f64 = x
            .eta
            .as_ref()
            .zip(y.eta.as_ref())
            .map(|(u, v)| {
                u.iter()
                    .zip(v)
                    .map(|(p, r)| (p - r).abs())
                    .fold(0.0, f64::max)
            })
            .unwrap_or(0.0);
        println!(
            "subject {i}: core diff {:.2e}, eta diff {d_eta:.2e}, decrements {:?} {:?}",
            (x.core - y.core).abs(),
            x.decrement,
            y.decrement
        );
    }
}

/// The per-subject jump of OFV_i along one coordinate at the end of a fit: the subjects whose
/// second difference is above 1e-11 (`T154_EXTRA` = design:replicate, `T154_COORD` = j,
/// `T154_FRAC` = the fraction of the engine's scale).
#[test]
#[ignore = "a measurement for the report of T-154"]
fn measure_the_subject_jumps() {
    let (design, rep) = std::env::var("T154_EXTRA")
        .unwrap()
        .split_once(':')
        .map(|(d, r)| (d.to_string(), r.parse::<usize>().unwrap()))
        .unwrap();
    let j: usize = std::env::var("T154_COORD").unwrap().parse().unwrap();
    let frac: f64 = std::env::var("T154_FRAC").unwrap().parse().unwrap();
    let sealed = load_pop_sealed(&design).unwrap();
    let request = sealed_request(&sealed, rep);
    let setup = Setup::new(&request).unwrap();
    let layout = setup.layout();
    let q = setup.random_effects.len();
    let settings = Settings {
        eps: inner::EPS_IN,
        keep: false,
        fail_inner: &|_| false,
    };
    let first = setup.evaluate_params(&setup.start, &[], settings).unwrap();
    let problem = outer::Problem {
        setup: &setup,
        layout: &layout,
    };
    let outcome = problem.minimise(layout.phi(&setup.start), first);
    let warm = outcome.evaluation.warm(q);
    let phi = outcome.phi.clone();
    let scale = match layout.coords[j].part() {
        phi::Part::Xi => phi[j].abs(),
        _ => phi[j].abs().max(1.0),
    };
    let h = frac * scale;
    let at_shift = |by: f64| {
        let mut x = phi.clone();
        x[j] += by;
        problem.eval(&x, &warm).unwrap()
    };
    let (m, z, pl) = (at_shift(-h), at_shift(0.0), at_shift(h));
    for i in 0..z.subjects.len() {
        let (a, b, c) = (&m.subjects[i], &z.subjects[i], &pl.subjects[i]);
        let second = c.core - 2.0 * b.core + a.core;
        if second.abs() > 1e-11 {
            println!(
                "subject {i}: second diff {second:.3e}, first {:.3e}; decrements {:?} {:?} {:?}; eta {:?} {:?} {:?}",
                c.core - a.core,
                a.decrement,
                b.decrement,
                c.decrement,
                a.eta,
                b.eta,
                c.eta
            );
        }
    }
}
