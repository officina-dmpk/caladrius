#![cfg(feature = "private-oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: helpers panic on purpose
//! Private oracle (task T-019, `AGENTS.md` section 5.2): `caladrius-nca` against the tables
//! exported by the reference software for the coursework exercises, compared at the precision
//! displayed in the export. Compiled only with `--features private-oracle`.
//!
//! The coursework data are read from `private/coursework/` and the exports from
//! `private/exports/<exercise>/` at run time. When either is missing a test prints why and returns
//! (it is skipped, not failed). Nothing from `private/` is in this file, and a failure message
//! names parameters and the number of displayed decimals only: no value, no file name.
//!
//! The engine runs with its defaults except for the AUC method of the export being compared; the
//! reference software's defaults (uniform weighting of the terminal regression, no acceptance
//! thresholds, `board/QUESTIONS.md` Q-008) are those of the engine's default options.

use std::collections::BTreeMap;

use caladrius_nca::{AucMethod, NcaInput, NcaOptions, Route, run};
use caladrius_testkit::private::{
    CourseworkProfile, PRIVATE_CASES, PrivateCase, PrivateCounts, discover_exports, evaluate,
    load_coursework_profile, read_export,
};

fn route_of(case: &PrivateCase) -> Route {
    match case.route {
        "extravascular" => Route::Extravascular,
        "iv_bolus" => Route::IvBolus,
        other => panic!("private case {}: unsupported route {other}", case.id),
    }
}

fn engine_values(
    case: &PrivateCase,
    profile: &CourseworkProfile,
    method: AucMethod,
) -> BTreeMap<String, Option<f64>> {
    let input = NcaInput {
        time: profile.time.clone(),
        conc: profile.conc.clone(),
        dose: case.dose,
        route: route_of(case),
        options: NcaOptions {
            auc_method: method,
            ..NcaOptions::default()
        },
    };
    let result = run(&input).expect("the engine accepts the coursework profile");
    result
        .parameters()
        .iter()
        .map(|p| (p.name.clone(), p.value.value()))
        .collect()
}

fn method_of(hint: caladrius_testkit::private::MethodHint) -> AucMethod {
    match hint {
        caladrius_testkit::private::MethodHint::Linear => AucMethod::Linear,
        caladrius_testkit::private::MethodHint::LinUpLogDown => AucMethod::LinUpLogDown,
    }
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
                "SKIPPED private oracle {}: no export files in private/exports/{}/ (drop the reference software's Final Parameters and Summary tables there, as csv, txt or tsv; add a manifest.json to say which is which if the file names do not)",
                case.id, case.id
            );
            return None;
        }
        Err(e) => panic!("private oracle {}: {e}", case.id),
    };
    if discovery.unclassified > 0 {
        eprintln!(
            "private oracle {}: {} export file(s) could not be classified by name (need 'final' or 'summary' and a method) and are ignored; add private/exports/{}/manifest.json",
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
        let entries =
            read_export(file).unwrap_or_else(|e| panic!("private oracle {}: {e}", case.id));
        let counts = evaluate(
            &entries,
            &engine_values(case, &profile, method_of(file.method)),
        );
        eprintln!(
            "private oracle {} export {} ({}, {}): {} validated, {} different, {} not computed, {} unmapped, {} unreadable",
            case.id,
            file.index + 1,
            file.kind.label(),
            file.method.label(),
            counts.validated,
            counts.mismatched.len(),
            counts.engine_missing.len(),
            counts.unmapped,
            counts.unreadable
        );
        for (name, decimals) in &counts.mismatched {
            failures.push(format!(
                "export {} ({}, {}): {name} differs at the displayed precision ({decimals} decimals)",
                file.index + 1,
                file.kind.label(),
                file.method.label()
            ));
        }
        total.add(&counts);
    }
    assert!(
        failures.is_empty(),
        "private oracle {}: {} value(s) differ from the export:\n  {}",
        case.id,
        failures.len(),
        failures.join("\n  ")
    );
    assert!(
        total.validated > 0,
        "private oracle {}: export files were found but no value could be compared (names not recognised: {} unmapped, {} unreadable); extend `canonical_name` or add a manifest",
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
        for method in [AucMethod::Linear, AucMethod::LinUpLogDown] {
            let values = engine_values(case, &profile, method);
            assert!(values.get("cmax").copied().flatten().is_some());
            assert!(values.get("auclast").copied().flatten().is_some());
        }
    }
}
