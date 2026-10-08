#![cfg(feature = "private-oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: helpers panic on purpose
//! Private oracle (task T-019, `AGENTS.md` section 5.2): `caladrius-nca` against the tables
//! exported by the reference software for the coursework exercises, compared at the precision
//! displayed in the export (worksheets, which store full double precision, at a relative 1e-9).
//! Compiled only with `--features private-oracle`.
//!
//! The coursework data are read from `private/coursework/` and the exports from
//! `private/exports/<exercise>/` at run time. When either is missing a test prints why and returns
//! (it is skipped, not failed). Nothing from `private/` is in this file, and a failure message
//! names parameters and the size of the difference (in displayed units): no value, no file name.
//!
//! The engine runs with its defaults except for the AUC method of the export being compared; the
//! reference software's defaults (uniform weighting of the terminal regression, automatic best-fit
//! choice of the points, no acceptance thresholds, `board/QUESTIONS.md` Q-008 and the settings
//! files of the exports) are those of the engine's default options.

use std::collections::BTreeMap;

use caladrius_nca::{AucMethod, NcaInput, NcaOptions, Route, run};
use caladrius_testkit::private::{
    CourseworkProfile, ExportKind, MethodHint, PRIVATE_CASES, PrivateCase, PrivateCounts,
    ProfileEngine, discover_exports, evaluate, evaluate_profile, load_coursework_profile,
    read_export, read_profile_rows, read_settings,
};

fn route_of(case: &PrivateCase) -> Route {
    match case.route {
        "extravascular" => Route::Extravascular,
        "iv_bolus" => Route::IvBolus,
        other => panic!("private case {}: unsupported route {other}", case.id),
    }
}

fn method_of(hint: MethodHint) -> AucMethod {
    match hint {
        MethodHint::Linear => AucMethod::Linear,
        MethodHint::LinUpLogDown => AucMethod::LinUpLogDown,
    }
}

fn input_of(case: &PrivateCase, time: &[f64], conc: &[f64], method: MethodHint) -> NcaInput {
    NcaInput {
        time: time.to_vec(),
        conc: conc.to_vec(),
        dose: case.dose,
        route: route_of(case),
        options: NcaOptions {
            auc_method: method_of(method),
            ..NcaOptions::default()
        },
    }
}

/// The engine's parameters by name, plus the few quantities the export lists that are not named
/// parameters of the engine: the intercept of the terminal regression, the correlation of time and
/// ln C (minus the square root of R squared for a decay), the number of samples and the dose.
fn engine_values(
    case: &PrivateCase,
    profile: &CourseworkProfile,
    method: MethodHint,
) -> BTreeMap<String, Option<f64>> {
    let result = run(&input_of(case, &profile.time, &profile.conc, method))
        .expect("the engine accepts the coursework profile");
    let mut values: BTreeMap<String, Option<f64>> = result
        .parameters()
        .iter()
        .map(|p| (p.name.clone(), p.value.value()))
        .collect();
    let selected = result.lambda_z_candidates().iter().find(|c| c.selected);
    values.insert("lambda.z.intercept".into(), selected.map(|c| c.intercept));
    values.insert(
        "corr.xy".into(),
        selected.and_then(|c| c.r_squared).map(|r2| -r2.sqrt()),
    );
    values.insert("n.samples".into(), Some(profile.time.len() as f64));
    values.insert("dose".into(), Some(case.dose));
    values
}

/// Per sample: the cumulative areas (the areas of the profile cut at that sample), the prediction
/// of the terminal regression and whether the sample is in its window.
fn engine_profile(
    case: &PrivateCase,
    profile: &CourseworkProfile,
    method: MethodHint,
) -> Vec<ProfileEngine> {
    let full = run(&input_of(case, &profile.time, &profile.conc, method)).unwrap();
    let selected = full
        .lambda_z_candidates()
        .iter()
        .find(|c| c.selected)
        .copied();
    (0..profile.time.len())
        .map(|i| {
            let t = profile.time[i];
            let (auc, aumc) = if i == 0 {
                (Some(0.0), Some(0.0))
            } else {
                let cut = run(&input_of(
                    case,
                    &profile.time[..=i],
                    &profile.conc[..=i],
                    method,
                ))
                .unwrap();
                (cut.get("auclast"), cut.get("aumclast"))
            };
            let included = selected.is_some_and(|c| t >= c.time_first && t <= c.time_last);
            let predicted = selected
                .filter(|_| included)
                .map(|c| (c.intercept - c.lambda_z * t).exp());
            ProfileEngine {
                auc,
                aumc,
                predicted,
                included,
            }
        })
        .collect()
}

/// Runs one private case; `None` (with a message) when the data are not there.
fn run_case(case: &PrivateCase) -> Option<PrivateCounts> {
    let profile = match load_coursework_profile(case) {
        Ok(Some(p)) => p,
        Ok(None) => {
            eprintln!(
                "SKIPPED private oracle {}: no coursework data in private/coursework/ (a CSV whose name starts with {:?}, header then time and concentration)",
                case.id, case.id
            );
            return None;
        }
        Err(e) => panic!("private oracle {}: {e}", case.id),
    };
    let discovery = match discover_exports(case.id) {
        Ok(Some(d)) => d,
        Ok(None) => {
            eprintln!(
                "SKIPPED private oracle {}: no export files in private/exports/{}/ (drop the reference software's tables there; see Q-011 in board/QUESTIONS.md)",
                case.id, case.id
            );
            return None;
        }
        Err(e) => panic!("private oracle {}: {e}", case.id),
    };
    if discovery.unclassified > 0 {
        eprintln!(
            "private oracle {}: {} export file(s) could not be classified by name and are ignored; add private/exports/{}/manifest.json",
            case.id, discovery.unclassified, case.id
        );
    }
    if discovery.files.is_empty() {
        eprintln!(
            "SKIPPED private oracle {}: no export file could be classified",
            case.id
        );
        return None;
    }
    let mut total = PrivateCounts::default();
    let mut failures = Vec::new();
    for file in &discovery.files {
        let label = format!(
            "export {} ({}, {})",
            file.index + 1,
            file.kind.label(),
            file.method.label()
        );
        let counts = match file.kind {
            ExportKind::Settings => {
                // The conventions the settings record must be those the engine's defaults implement.
                let facts = read_settings(file).unwrap_or_else(|e| panic!("{label}: {e}"));
                let method_words = match file.method {
                    MethodHint::Linear => {
                        facts.calculation_method.contains("linear")
                            && !facts.calculation_method.contains("log")
                    }
                    MethodHint::LinUpLogDown => facts.calculation_method.contains("uplogdown"),
                };
                if !method_words {
                    failures.push(format!(
                        "{label}: the settings name another AUC method than the file name says"
                    ));
                }
                if !facts.weighting.contains("uniform") {
                    failures.push(format!(
                        "{label}: the terminal regression is not uniformly weighted"
                    ));
                }
                if !facts.slope_selection.contains("bestfit") {
                    failures.push(format!(
                        "{label}: the terminal phase is not an automatic best fit"
                    ));
                }
                eprintln!(
                    "private oracle {label}: settings read (uniform weighting, automatic best fit)"
                );
                continue;
            }
            ExportKind::Summary => {
                let rows = read_profile_rows(file).unwrap_or_else(|e| panic!("{label}: {e}"));
                let other = match file.method {
                    MethodHint::Linear => MethodHint::LinUpLogDown,
                    MethodHint::LinUpLogDown => MethodHint::Linear,
                };
                let wrong = evaluate_profile(&rows, &engine_profile(case, &profile, other));
                assert!(
                    !wrong.mismatched.is_empty(),
                    "{label}: the worksheet agrees with the other AUC method too: the comparison tests nothing"
                );
                evaluate_profile(&rows, &engine_profile(case, &profile, file.method))
            }
            ExportKind::FinalParameters | ExportKind::CoreOutput => {
                let entries = read_export(file).unwrap_or_else(|e| panic!("{label}: {e}"));
                // Guard against a vacuous comparison: the same table must NOT agree with the
                // engine run under the other AUC method.
                let other = match file.method {
                    MethodHint::Linear => MethodHint::LinUpLogDown,
                    MethodHint::LinUpLogDown => MethodHint::Linear,
                };
                let wrong = evaluate(&entries, &engine_values(case, &profile, other));
                assert!(
                    !wrong.mismatched.is_empty(),
                    "{label}: the export agrees with the other AUC method too: the comparison tests nothing"
                );
                evaluate(&entries, &engine_values(case, &profile, file.method))
            }
        };
        eprintln!(
            "private oracle {label}: {} agree, {} differ, {} not computed, {} not mapped, {} unreadable",
            counts.validated,
            counts.mismatched.len(),
            counts.engine_missing.len(),
            counts.unmapped,
            counts.unreadable
        );
        for d in &counts.mismatched {
            failures.push(format!("{label}: {} differs: {}", d.name, d.describe()));
        }
        for name in &counts.engine_missing {
            eprintln!("private oracle {label}: {name} is exported but not computed by the engine");
        }
        total.add(&counts);
    }
    assert!(
        failures.is_empty(),
        "private oracle {}: {} problem(s):\n  {}",
        case.id,
        failures.len(),
        failures.join("\n  ")
    );
    assert!(
        total.validated > 0,
        "private oracle {}: export files were found but no value could be compared ({} unmapped, {} unreadable); extend `canonical_name` or add a manifest",
        case.id,
        total.unmapped,
        total.unreadable
    );
    Some(total)
}

#[test]
fn every_private_case_matches_its_export_at_the_displayed_precision() {
    for case in PRIVATE_CASES {
        let _ = run_case(case);
    }
}

/// The coursework profile, if present, is accepted by the engine under both AUC methods.
#[test]
fn the_coursework_profile_runs_under_both_auc_methods() {
    for case in PRIVATE_CASES {
        let Some(profile) = load_coursework_profile(case).unwrap() else {
            eprintln!("SKIPPED private oracle {}: no coursework data", case.id);
            continue;
        };
        for method in [MethodHint::Linear, MethodHint::LinUpLogDown] {
            let values = engine_values(case, &profile, method);
            assert!(values.get("cmax").copied().flatten().is_some());
            assert!(values.get("auclast").copied().flatten().is_some());
        }
    }
}
