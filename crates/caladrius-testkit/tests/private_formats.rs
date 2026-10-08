#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: helpers panic on purpose

//! The formats of the reference software's exports (task T-019), on made-up text: an RTF report
//! with a final-parameters block, a worksheet of the profile, a settings report, and the comparison
//! of full-precision cells. The real files are in `private/` and are read only by the tests behind
//! the `private-oracle` feature of `caladrius-nca`.

use std::collections::BTreeMap;

use caladrius_testkit::private::{
    ExportEntry, ProfileEngine, evaluate, evaluate_profile, extract_profile_table,
    report_final_parameters, rtf_to_text, settings_facts, split_table,
};

#[test]
fn rtf_reports_are_stripped_and_their_final_parameters_read() {
    let rtf = concat!(
        r"{\rtf1\ansi{\fonttbl{\f0 Some Font;}}\f0 Settings\par\par Final Parameters\par ",
        r"---------------\par\par Cmax   12.50\par\par AUC_%Extrap_obs    8.97\par\par ",
        r"Tlag   0.0000\par\par\par *) A note\par Mean \'e9 {\*\generator hidden;}x}"
    );
    let text = rtf_to_text(rtf);
    assert!(
        text.contains("Final Parameters") && text.contains("Cmax"),
        "{text:?}"
    );
    assert!(
        !text.contains("hidden") && !text.contains("Some Font"),
        "{text:?}"
    );
    assert!(text.contains('\u{e9}'), "{text:?}");
    let entries = report_final_parameters(&text);
    let seen: Vec<_> = entries
        .iter()
        .map(|e| (e.canonical, e.text.as_str(), e.exact))
        .collect();
    assert_eq!(
        seen,
        vec![
            (Some("cmax"), "12.50", false),
            (Some("aucpext.obs"), "8.97", false),
            (Some("tlag"), "0.0000", false)
        ]
    );
    assert!(report_final_parameters("no such block").is_empty());
}

#[test]
fn full_precision_cells_are_compared_with_a_relative_tolerance() {
    let entry = |text: &str, exact: bool| ExportEntry {
        raw_name: "Cmax".to_string(),
        canonical: Some("cmax"),
        text: text.to_string(),
        exact,
    };
    let engine =
        |x: f64| -> BTreeMap<String, Option<f64>> { [("cmax".to_string(), Some(x))].into() };
    // A cell stored as 262.1: 262.1 + 1e-10 is the same number, 262.2 is not.
    let c = evaluate(&[entry("262.1", true)], &engine(262.1 + 1e-10));
    assert_eq!((c.validated, c.mismatched.len()), (1, 0));
    let c = evaluate(&[entry("262.1", true)], &engine(262.2));
    assert_eq!(c.mismatched.len(), 1);
    assert!(c.mismatched[0].relative && c.mismatched[0].size > 1e-4);
    // The same text as a displayed value (one decimal) accepts 262.14 and measures a difference in
    // units of the last displayed place.
    let c = evaluate(&[entry("262.1", false)], &engine(262.14));
    assert_eq!(c.validated, 1);
    let c = evaluate(&[entry("262.1", false)], &engine(262.3));
    assert!(!c.mismatched[0].relative && (c.mismatched[0].size - 2.0).abs() < 1e-9);
    assert!(
        c.mismatched[0]
            .describe()
            .contains("units of the last displayed place")
    );
}

#[test]
fn a_profile_worksheet_is_read_and_compared_row_by_row() {
    let table = split_table(
        "Time,Incl,Conc,Predicted,Residual,AUC,AUMC,Weighting\nhr,,mg,mg,mg,x,y,\n0,,0,,,0,0,0\n1,*,5,4.5,0.5,2.5,1.25,1\n2,*,4,4.1,-0.1,7,9,1\n",
        "t",
    )
    .unwrap();
    let rows = extract_profile_table(&table).unwrap();
    assert_eq!(rows.len(), 3);
    assert!(!rows[0].included && rows[1].included);
    assert_eq!(rows[2].predicted.as_deref(), Some("4.1"));
    let engine = |auc: f64, aumc: f64, predicted: Option<f64>, included: bool| ProfileEngine {
        auc: Some(auc),
        aumc: Some(aumc),
        predicted,
        included,
    };
    let good = [
        engine(0.0, 0.0, None, false),
        engine(2.5, 1.25, Some(4.5), true),
        engine(7.0, 9.0, Some(4.1), true),
    ];
    let c = evaluate_profile(&rows, &good);
    assert!(c.mismatched.is_empty(), "{c:?}");
    // 3 rows: AUC x3, AUMC x3, weight x3, flag x3, predicted x2, residual x2.
    assert_eq!(c.validated, 16);
    let mut bad = good;
    bad[2].auc = Some(8.5);
    bad[1].included = false;
    let c = evaluate_profile(&rows, &bad);
    let names: Vec<_> = c.mismatched.iter().map(|d| d.name).collect();
    for expected in ["auc.cumulative", "lambda.z.included", "weight"] {
        assert!(names.contains(&expected), "{names:?}");
    }
    // A different number of rows is unreadable, not agreement.
    assert_eq!(evaluate_profile(&rows, &good[..2]).validated, 0);
}

#[test]
fn settings_facts_keep_words_only() {
    let facts = settings_facts(
        "Calculation Method = Linear Up Log Down\nWeighting = Uniform Weighting; 0\n\tSlope Settings: Start Time=4, End Time=48, Fit Method=BestFit, Selection=System\n",
    );
    assert!(facts.calculation_method.contains("uplogdown"));
    assert!(facts.weighting.contains("uniform"));
    assert!(facts.slope_selection.contains("bestfit") && facts.slope_selection.contains("system"));
}
