//! Tests of the project layer: import, worksheets, stale marking, save and load.

use caladrius_nca::{NcaInput, NcaOptions, Route};

use crate::*;

const SAMPLE_CSV: &str = "Subject,Time (h),Conc (ng/mL),Dose (mg)\n\
1,0,0,100\n1,1,8,100\n1,2,6,100\n1,4,3,100\n1,6,1.5,100\n\
2,0,0,100\n2,1,7,100\n2,2,5,100\n2,4,2.5,100\n2,6,1.2,100\n";

fn sample_project() -> (Project, WorksheetId) {
    let table = ImportedTable::from_csv(SAMPLE_CSV.as_bytes(), &CsvOptions::default()).unwrap();
    let mut project = Project::new("test");
    let id = project.add_worksheet("sample", table.columns).unwrap();
    (project, id)
}

fn nca_spec(worksheet: WorksheetId) -> AnalysisSpec {
    AnalysisSpec::Nca(NcaSpec {
        worksheet,
        subject: None,
        route: Some(Route::Extravascular),
        dose: None,
        options: NcaOptions::default(),
    })
}

fn some_nca_result() -> AnalysisResult {
    let result = caladrius_nca::run(&NcaInput {
        time: vec![0.0, 1.0, 2.0, 4.0, 6.0],
        conc: vec![0.0, 8.0, 6.0, 3.0, 1.5],
        dose: 100.0,
        route: Route::Extravascular,
        options: NcaOptions::default(),
    })
    .unwrap();
    AnalysisResult::Nca {
        subjects: vec![NcaSubjectResult {
            subject: "1".to_owned(),
            dose: Some(100.0),
            route: Some(Route::Extravascular),
            outcome: Outcome::Ok(result),
        }],
    }
}

// ---- CSV import ------------------------------------------------------------------------

#[test]
fn csv_import_reads_units_roles_and_types() {
    let t = ImportedTable::from_csv(SAMPLE_CSV.as_bytes(), &CsvOptions::default()).unwrap();
    assert_eq!(t.delimiter, ',');
    let roles: Vec<_> = t
        .columns
        .iter()
        .map(|c| (c.name.as_str(), c.role))
        .collect();
    assert_eq!(
        roles,
        [
            ("Subject", ColumnRole::Subject),
            ("Time", ColumnRole::Time),
            ("Conc", ColumnRole::Concentration),
            ("Dose", ColumnRole::Dose),
        ]
    );
    let units: Vec<_> = t.columns.iter().map(|c| c.unit.as_deref()).collect();
    assert_eq!(units, [None, Some("h"), Some("ng/mL"), Some("mg")]);
    // The subject column holds numbers, read as labels.
    assert!(matches!(t.columns[0].data, ColumnData::Number(_)));
    assert!(t.notes.iter().any(|n| n.contains("`ng/mL`")));
}

#[test]
fn csv_import_handles_semicolons_decimal_commas_quotes_and_bom() {
    let text = "\u{feff}\"Sujet\";Time;Conc\r\n\"A, B\";0;0,5\r\n\"A, B\";1;12,25\r\n\r\n";
    let t = ImportedTable::from_csv(text.as_bytes(), &CsvOptions::default()).unwrap();
    assert_eq!(t.delimiter, ';');
    assert_eq!(t.columns.len(), 3);
    assert_eq!(
        t.columns[2].data,
        ColumnData::Number(vec![Some(0.5), Some(12.25)])
    );
    assert_eq!(
        t.columns[0].data,
        ColumnData::Text(vec!["A, B".to_owned(), "A, B".to_owned()])
    );
}

#[test]
fn csv_import_keeps_missing_values_and_explains_text_in_numbers() {
    let text = "time,conc\n0,0\n1,BLQ\n2,\n3,4\n";
    let t = ImportedTable::from_csv(text.as_bytes(), &CsvOptions::default()).unwrap();
    // `conc` mixes numbers and text: kept as text, not given the concentration role, explained.
    assert!(matches!(t.columns[1].data, ColumnData::Text(_)));
    assert_eq!(t.columns[1].role, ColumnRole::Other);
    assert!(t.notes.iter().any(|n| n.contains("BLQ")));
    assert!(
        t.notes
            .iter()
            .any(|n| n.contains("no concentration column"))
    );
    let text = "time,conc\n0,0\n1,NA\n2,\n3,4\n";
    let t = ImportedTable::from_csv(text.as_bytes(), &CsvOptions::default()).unwrap();
    assert_eq!(
        t.columns[1].data,
        ColumnData::Number(vec![Some(0.0), None, None, Some(4.0)])
    );
}

#[test]
fn csv_errors_say_what_to_fix() {
    let cases: [(&[u8], &str); 5] = [
        (b"", "empty"),
        (b"a,b\n1,2\n3\n", "line 3 has 1 fields"),
        (b"a,b\n\"1,2\n", "never closed"),
        (b"\xff\xfe", "UTF-8"),
        (b"a,a\n1,2\n", ""),
    ];
    for (bytes, expected) in cases {
        let parsed = ImportedTable::from_csv(bytes, &CsvOptions::default());
        match (parsed, expected) {
            // Duplicate names are refused when the worksheet is built.
            (Ok(t), "") => {
                let mut p = Project::new("x");
                let err = p.add_worksheet("w", t.columns).unwrap_err();
                assert_eq!(err.code, "duplicate_column");
            }
            (Err(e), text) => assert!(e.message.contains(text), "{}: {e}", text),
            (Ok(_), text) => panic!("expected an error containing {text:?}"),
        }
    }
}

// ---- worksheets ------------------------------------------------------------------------

#[test]
fn profile_selects_one_subject() {
    let (project, id) = sample_project();
    let ws = project.worksheet(id).unwrap();
    assert_eq!(ws.subjects(), ["1", "2"]);
    let p = ws.profile(Some("2")).unwrap();
    assert_eq!(p.time, [0.0, 1.0, 2.0, 4.0, 6.0]);
    assert_eq!(p.conc, [0.0, 7.0, 5.0, 2.5, 1.2]);
    assert_eq!(p.dose, Some(100.0));
    // Several subjects: one must be named, and it must exist.
    assert_eq!(ws.profile(None).unwrap_err().code, "subject_required");
    let err = ws.profile(Some("9")).unwrap_err();
    assert_eq!(err.code, "unknown_subject");
    assert!(err.message.contains("1, 2"));
}

#[test]
fn a_worksheet_without_subject_column_has_one_subject() {
    let t = ImportedTable::from_csv(b"time,conc\n0,1\n1,2\n", &CsvOptions::default()).unwrap();
    let mut project = Project::new("p");
    let id = project.add_worksheet("w", t.columns).unwrap();
    let ws = project.worksheet(id).unwrap();
    assert_eq!(ws.subjects(), [SINGLE_SUBJECT]);
    let p = ws.profile(None).unwrap();
    assert_eq!((p.subject.as_str(), p.dose), ("1", None));
}

#[test]
fn profile_reports_a_missing_time_and_a_varying_dose() {
    let t = ImportedTable::from_csv(b"time,conc,dose\n0,1,10\n,2,10\n", &CsvOptions::default())
        .unwrap();
    let mut project = Project::new("p");
    let id = project.add_worksheet("w", t.columns).unwrap();
    let err = project.worksheet(id).unwrap().profile(None).unwrap_err();
    assert_eq!(err.code, "missing_time");
    assert!(err.message.contains("row 2"), "{err}");
    project
        .edit_worksheet(id, |w| {
            w.set_number("time", 1, Some(1.0))?;
            w.set_number("dose", 1, Some(20.0))
        })
        .unwrap();
    let err = project.worksheet(id).unwrap().profile(None).unwrap_err();
    assert_eq!(err.code, "dose_varies");
}

#[test]
fn roles_are_unique_and_checked_against_the_data() {
    let t = ImportedTable::from_csv(b"a,b,note\n0,1,x\n1,2,y\n", &CsvOptions::default()).unwrap();
    let mut project = Project::new("p");
    let id = project.add_worksheet("w", t.columns).unwrap();
    project
        .edit_worksheet(id, |w| {
            w.set_role("a", ColumnRole::Time)?;
            w.set_role("b", ColumnRole::Concentration)
        })
        .unwrap();
    // Moving the time role to `b` demotes `a`... and `b` loses nothing else.
    project
        .edit_worksheet(id, |w| w.set_role("b", ColumnRole::Time))
        .unwrap();
    let ws = project.worksheet(id).unwrap();
    assert_eq!(ws.column("a").unwrap().role, ColumnRole::Other);
    assert_eq!(ws.column("b").unwrap().role, ColumnRole::Time);
    // A text column cannot be a numeric column.
    let err = project
        .edit_worksheet(id, |w| w.set_role("note", ColumnRole::Concentration))
        .unwrap_err();
    assert_eq!(err.code, "role_needs_numbers");
    let err = project
        .edit_worksheet(id, |w| w.set_role("nope", ColumnRole::Time))
        .unwrap_err();
    assert_eq!(err.code, "unknown_column");
    assert!(err.message.contains("a, b, note"));
}

#[test]
fn cells_are_checked() {
    let (mut project, id) = sample_project();
    let edit = |p: &mut Project, f: &dyn Fn(&mut Worksheet) -> Result<(), ProjectError>| {
        p.edit_worksheet(id, |w| f(w))
    };
    assert_eq!(
        edit(&mut project, &|w| w.set_number("Conc", 999, Some(1.0)))
            .unwrap_err()
            .code,
        "row_out_of_range"
    );
    assert_eq!(
        edit(&mut project, &|w| w.set_number("Conc", 0, Some(f64::NAN)))
            .unwrap_err()
            .code,
        "non_finite_value"
    );
    assert_eq!(
        edit(&mut project, &|w| w.set_number(
            "Conc",
            0,
            Some(f64::INFINITY)
        ))
        .unwrap_err()
        .code,
        "non_finite_value"
    );
    assert_eq!(
        edit(&mut project, &|w| w.set_text("Conc", 0, "x"))
            .unwrap_err()
            .code,
        "not_a_text_column"
    );
}

// ---- units -----------------------------------------------------------------------------

#[test]
fn units_are_checked_together() {
    let none = unit_warnings(Some("h"), Some("ng/mL"), Some("ng"));
    assert!(none.is_empty(), "{none:?}");
    let codes =
        |t, c, d| -> Vec<String> { unit_warnings(t, c, d).into_iter().map(|w| w.code).collect() };
    assert_eq!(
        codes(Some("h"), Some("ng/mL"), Some("mg")),
        ["mass_mismatch"]
    );
    assert_eq!(
        codes(Some("h"), Some("ng/mL"), Some("mg/kg")),
        ["dose_per_body_size"]
    );
    assert_eq!(codes(None, Some("ng/mL"), Some("ng")), ["missing_unit"]);
    assert_eq!(
        codes(Some("fortnight"), Some("ng/mL"), Some("ng")),
        ["unrecognised_unit"]
    );
    assert_eq!(
        codes(Some("h"), Some("\u{b5}g/L"), Some("ug")),
        Vec::<String>::new()
    );
    let d = derived_units(Some("h"), Some("ng/mL"), Some("ng"));
    assert_eq!(d["auc"], "h*ng/mL");
    assert_eq!(d["cl"], "mL/h");
    assert_eq!(d["v"], "mL");
    let d = derived_units(Some("h"), Some("ng/mL"), Some("mg"));
    assert_eq!(d["cl"], "mg/(h*ng/mL)");
    // Nothing is guessed for a missing unit.
    assert!(!derived_units(Some("h"), None, None).contains_key("auc"));
}

#[test]
fn a_worksheet_reports_its_unit_warnings() {
    let (project, id) = sample_project();
    let w = project.worksheet(id).unwrap().unit_warnings();
    assert_eq!(w.len(), 1);
    assert_eq!(w[0].code, "mass_mismatch");
}

// ---- analyses and stale marking --------------------------------------------------------

#[test]
fn stale_marking_follows_data_and_options() {
    let (mut project, ws) = sample_project();
    let a = project.add_analysis(nca_spec(ws), None).unwrap();
    assert_eq!(project.status(a).unwrap(), AnalysisStatus::NoResult);

    project.set_result(a, some_nca_result()).unwrap();
    assert_eq!(project.status(a).unwrap(), AnalysisStatus::Fresh);

    // An input change makes it stale, and says so.
    project
        .edit_worksheet(ws, |w| w.set_number("Conc", 1, Some(9.0)))
        .unwrap();
    let AnalysisStatus::Stale { reasons } = project.status(a).unwrap() else {
        panic!("expected stale");
    };
    assert_eq!(
        reasons,
        [StaleReason::InputChanged {
            ran_on_revision: 0,
            current_revision: 1
        }]
    );
    // Setting the same value again changes nothing.
    project
        .edit_worksheet(ws, |w| w.set_number("Conc", 1, Some(9.0)))
        .unwrap();
    assert_eq!(project.worksheet(ws).unwrap().revision(), 1);

    // A run makes it fresh again; a change of options makes it stale again.
    project.set_result(a, some_nca_result()).unwrap();
    assert_eq!(project.status(a).unwrap(), AnalysisStatus::Fresh);
    let mut spec = project.analysis(a).unwrap().spec().clone();
    if let AnalysisSpec::Nca(s) = &mut spec {
        s.options.lambda_z.min_points = 4;
    }
    project.update_spec(a, spec.clone()).unwrap();
    assert_eq!(
        project.status(a).unwrap(),
        AnalysisStatus::Stale {
            reasons: vec![StaleReason::OptionsChanged]
        }
    );
    // The previous result is still there to look at.
    assert!(project.analysis(a).unwrap().result().is_some());
    // The same spec again does not add staleness.
    project.set_result(a, some_nca_result()).unwrap();
    project.update_spec(a, spec).unwrap();
    assert_eq!(project.status(a).unwrap(), AnalysisStatus::Fresh);
    // Changing a unit or a role is an input change too; renaming is not.
    project
        .edit_worksheet(ws, |w| w.set_unit("Conc", Some("ug/L")))
        .unwrap();
    assert!(matches!(
        project.status(a).unwrap(),
        AnalysisStatus::Stale { .. }
    ));
    project.set_result(a, some_nca_result()).unwrap();
    project
        .edit_worksheet(ws, |w| {
            w.rename("renamed");
            Ok(())
        })
        .unwrap();
    assert_eq!(project.status(a).unwrap(), AnalysisStatus::Fresh);
}

#[test]
fn analyses_are_named_from_their_content() {
    let (mut project, ws) = sample_project();
    let a = project.add_analysis(nca_spec(ws), None).unwrap();
    assert_eq!(project.label_of(a).unwrap(), "NCA of sample, all subjects");
    project.rename_analysis(a, Some("Phase 1 NCA")).unwrap();
    assert_eq!(project.label_of(a).unwrap(), "Phase 1 NCA");
    project.rename_analysis(a, None).unwrap();
    assert_eq!(project.label_of(a).unwrap(), "NCA of sample, all subjects");
}

#[test]
fn analyses_and_worksheets_must_exist() {
    let (mut project, ws) = sample_project();
    let err = project
        .add_analysis(nca_spec(WorksheetId(99)), None)
        .unwrap_err();
    assert_eq!(err.code, "unknown_worksheet");
    assert!(err.message.contains("sample"));
    assert_eq!(
        project.analysis(AnalysisId(99)).unwrap_err().code,
        "unknown_analysis"
    );
    let a = project.add_analysis(nca_spec(ws), None).unwrap();
    assert_eq!(
        project.remove_worksheet(ws).unwrap_err().code,
        "worksheet_in_use"
    );
    project.remove_analysis(a).unwrap();
    project.remove_worksheet(ws).unwrap();
    assert!(project.worksheets().is_empty());
}

#[test]
fn the_kind_of_an_analysis_cannot_change() {
    let (mut project, ws) = sample_project();
    let a = project.add_analysis(nca_spec(ws), None).unwrap();
    let fit = AnalysisSpec::Fit(FitSpec {
        worksheet: ws,
        subject: Some("1".to_owned()),
        model: caladrius_models::ModelId::Oral1,
        dose: None,
        weighting: Default::default(),
        initial: Default::default(),
        options: Default::default(),
    });
    assert_eq!(
        project.update_spec(a, fit).unwrap_err().code,
        "analysis_kind_changed"
    );
}

// ---- serialization ---------------------------------------------------------------------

#[test]
fn specs_round_trip_through_json() {
    let (_, ws) = sample_project();
    let specs = [
        nca_spec(ws),
        AnalysisSpec::Fit(FitSpec {
            worksheet: ws,
            subject: Some("1".to_owned()),
            model: caladrius_models::ModelId::Oral1Lag,
            dose: Some(100.0),
            weighting: caladrius_fit::Weighting::InvY2,
            initial: [("v".to_owned(), 10.0)].into(),
            options: Default::default(),
        }),
        AnalysisSpec::Simulation(SimulationSpec {
            input: caladrius_models::ModelInput {
                model: caladrius_models::ModelId::IvBolus,
                dose: 10.0,
                params: [("v".to_owned(), 5.0), ("k".to_owned(), 0.1)].into(),
                times: vec![0.0, 1.0],
            },
        }),
    ];
    for spec in specs {
        let text = serde_json::to_string(&spec).unwrap();
        let back: AnalysisSpec = serde_json::from_str(&text).unwrap();
        assert_eq!(back, spec, "{text}");
    }
}

#[test]
fn save_and_load_round_trip_including_results_and_staleness() {
    let (mut project, ws) = sample_project();
    let a = project.add_analysis(nca_spec(ws), Some("kept")).unwrap();
    project.set_result(a, some_nca_result()).unwrap();
    let b = project.add_analysis(nca_spec(ws), None).unwrap();
    project.set_result(b, some_nca_result()).unwrap();
    project
        .edit_worksheet(ws, |w| w.set_number("Time", 3, Some(3.0)))
        .unwrap();
    let bytes = project.to_bytes().unwrap();
    let loaded = Project::from_bytes(&bytes).unwrap();
    assert_eq!(loaded, project);
    assert!(matches!(
        loaded.status(a).unwrap(),
        AnalysisStatus::Stale { .. }
    ));
    // Ids keep counting after a load.
    let mut loaded = loaded;
    let c = loaded.add_analysis(nca_spec(ws), None).unwrap();
    assert!(c > b);
    // Saving twice gives the same bytes.
    assert_eq!(project.to_bytes().unwrap(), bytes);
}

#[test]
fn loading_a_bad_file_says_why() {
    let (project, ws) = sample_project();
    let good = String::from_utf8(project.to_bytes().unwrap()).unwrap();
    assert_eq!(Project::from_bytes(b"{").unwrap_err().code, "load_failed");
    assert_eq!(Project::from_bytes(b"[]").unwrap_err().code, "load_failed");
    let wrong = good.replace("\"caladrius-project\"", "\"other\"");
    assert_eq!(
        Project::from_bytes(wrong.as_bytes()).unwrap_err().code,
        "load_wrong_format"
    );
    let newer = good.replace("\"format_version\": 1", "\"format_version\": 99");
    assert_eq!(
        Project::from_bytes(newer.as_bytes()).unwrap_err().code,
        "load_newer_version"
    );
    // A column with one more row than the others is refused, not loaded.
    let mut value: serde_json::Value = serde_json::from_str(&good).unwrap();
    let column = value
        .pointer_mut("/worksheets/0/columns/1/data/number")
        .and_then(|v| v.as_array_mut())
        .unwrap();
    column.push(serde_json::Value::Null);
    let ragged = serde_json::to_vec(&value).unwrap();
    assert_eq!(
        Project::from_bytes(&ragged).unwrap_err().code,
        "ragged_columns"
    );
    // An analysis that reads a missing worksheet.
    let mut with_analysis = project.clone();
    with_analysis.add_analysis(nca_spec(ws), None).unwrap();
    let text = String::from_utf8(with_analysis.to_bytes().unwrap()).unwrap();
    let broken = text.replace(&format!("\"worksheet\": {}", ws.0), "\"worksheet\": 77");
    assert_eq!(
        Project::from_bytes(broken.as_bytes()).unwrap_err().code,
        "unknown_worksheet"
    );
}

// ---- counters --------------------------------------------------------------------------

/// The saved project with the JSON value at `pointer` replaced by `value`.
fn with_counter(project: &Project, pointer: &str, value: u64) -> Vec<u8> {
    let mut doc: Value = serde_json::from_slice(&project.to_bytes().unwrap()).unwrap();
    *doc.pointer_mut(pointer).unwrap() = serde_json::json!(value);
    serde_json::to_vec(&doc).unwrap()
}

use serde_json::Value;

#[test]
fn a_loaded_counter_at_the_limit_is_refused_and_one_below_it_errors_instead_of_wrapping() {
    let (mut project, ws) = sample_project();
    let a = project.add_analysis(nca_spec(ws), None).unwrap();
    project.set_result(a, some_nca_result()).unwrap();
    for pointer in [
        "/next_id",
        "/worksheets/0/revision",
        "/analyses/0/spec_version",
    ] {
        let bytes = with_counter(&project, pointer, u64::MAX);
        let err = Project::from_bytes(&bytes).unwrap_err();
        assert_eq!(err.code, "load_bad_counter", "{pointer}");
    }

    // next_id one below the limit: the next identifier would be the last, so it is refused.
    let mut loaded =
        Project::from_bytes(&with_counter(&project, "/next_id", u64::MAX - 1)).unwrap();
    let before = loaded.clone();
    let table = ImportedTable::from_csv(b"time,conc\n0,1\n", &CsvOptions::default()).unwrap();
    assert_eq!(
        loaded
            .add_worksheet("more", table.columns)
            .unwrap_err()
            .code,
        "counter_overflow"
    );
    assert_eq!(
        loaded.add_analysis(nca_spec(ws), None).unwrap_err().code,
        "counter_overflow"
    );
    assert_eq!(
        loaded, before,
        "a refused change leaves the project as it was"
    );

    // revision one below the limit: every kind of edit is refused, none wraps.
    let mut loaded = Project::from_bytes(&with_counter(
        &project,
        "/worksheets/0/revision",
        u64::MAX - 1,
    ))
    .unwrap();
    let before = loaded.clone();
    for edit in [
        &(|w: &mut Worksheet| w.set_number("Conc", 1, Some(1.5)))
            as &dyn Fn(&mut Worksheet) -> Result<(), ProjectError>,
        &|w: &mut Worksheet| w.set_unit("Conc", Some("ug/L")),
        &|w: &mut Worksheet| w.set_role("Dose", ColumnRole::Other),
        &|w: &mut Worksheet| w.set_text("Subject", 0, "Z"),
    ] {
        let result = loaded.edit_worksheet(ws, |w| edit(w));
        assert_eq!(result.unwrap_err().code, "counter_overflow");
        assert_eq!(loaded, before);
    }

    // spec_version one below the limit: a changed spec is refused, the same spec is not a change.
    let mut loaded = Project::from_bytes(&with_counter(
        &project,
        "/analyses/0/spec_version",
        u64::MAX - 1,
    ))
    .unwrap();
    let mut spec = loaded.analysis(a).unwrap().spec().clone();
    loaded.update_spec(a, spec.clone()).unwrap();
    if let AnalysisSpec::Nca(s) = &mut spec {
        s.options.lambda_z.min_points = 5;
    }
    assert_eq!(
        loaded.update_spec(a, spec).unwrap_err().code,
        "counter_overflow"
    );

    // A counter just below the one-below-limit still works normally.
    let mut loaded =
        Project::from_bytes(&with_counter(&project, "/next_id", u64::MAX - 2)).unwrap();
    let table = ImportedTable::from_csv(b"time,conc\n0,1\n", &CsvOptions::default()).unwrap();
    loaded.add_worksheet("last", table.columns).unwrap();
}

// ---- readings of a CSV -----------------------------------------------------------------

fn first_reading(text: &str) -> Reading {
    readings(text.as_bytes(), &CsvOptions::default())
        .unwrap()
        .remove(0)
}

#[test]
fn the_same_profile_in_two_conventions_gives_the_same_table() {
    let point = first_reading("time,conc\n0,0\n0.25,3.4\n1.5,7.25\n");
    let comma = first_reading("time;conc\n0;0\n0,25;3,4\n1,5;7,25\n");
    assert_eq!(point.table.columns, comma.table.columns);
    assert!(point.checks.is_empty() && comma.table.decimal_comma);
    assert_eq!(point.table.delimiter, ',');
    assert_eq!(comma.table.delimiter, ';');
    let tab = first_reading("time\tconc\n0\t0\n0.25\t3.4\n1.5\t7.25\n");
    assert_eq!(tab.table.columns, point.table.columns);
}

#[test]
fn a_comma_that_is_a_decimal_mark_is_not_a_separator() {
    // The reported failure: the quarter must not become two cells.
    let r = readings(b"time;conc\n0,25;3,4\n", &CsvOptions::default()).unwrap();
    assert_eq!(r.len(), 1);
    assert!(r[0].table.decimal_comma);
    assert_eq!(
        r[0].table.columns[0].data,
        ColumnData::Number(vec![Some(0.25)])
    );
    assert_eq!(
        r[0].table.columns[1].data,
        ColumnData::Number(vec![Some(3.4)])
    );
}

#[test]
fn a_reading_that_gives_repeated_times_is_flagged() {
    // `0,25` read with a comma separator: a quarter becomes time 0 and concentration 25.
    let r = readings(b"time,conc\n0,25\n0,5\n1,2\n", &CsvOptions::default()).unwrap();
    assert_eq!(r.len(), 1);
    let codes: Vec<&str> = r[0].checks.iter().map(|c| c.code.as_str()).collect();
    assert_eq!(codes, ["duplicate_times"]);
    assert!(
        r[0].checks[0].message.contains("row 2"),
        "{:?}",
        r[0].checks
    );
    // Going back in time, per subject.
    let r = readings(
        b"id,time,conc\nA,0,1\nA,2,3\nA,1,2\nB,0,1\nB,1,1\n",
        &CsvOptions::default(),
    )
    .unwrap();
    assert_eq!(r[0].checks.len(), 1);
    assert_eq!(r[0].checks[0].code, "time_not_increasing");
    assert!(r[0].checks[0].message.contains("subject A"));
}

#[test]
fn fixed_options_restrict_the_readings_and_nothing_usable_is_an_error() {
    let only = CsvOptions {
        delimiter: Some(';'),
        decimal_comma: Some(false),
    };
    // With a point as decimal mark the comma numbers are text: no usable reading.
    assert!(readings(b"time;conc\n0,25;3,4\n", &only).is_err());
    let err = readings(b"a,b\n1,2\n", &CsvOptions::default()).unwrap_err();
    assert!(
        err.message.contains("no reading") || err.code.starts_with("csv"),
        "{err}"
    );
}

#[test]
fn oracle_data_survives_three_conventions_exactly() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../oracle/data/theoph.csv");
    let original = std::fs::read(path).unwrap();
    let table = ImportedTable::from_csv(&original, &CsvOptions::default()).unwrap();
    let write = |sep: &str, comma: bool| {
        let mut out = table
            .columns
            .iter()
            .map(|c| c.name.clone())
            .collect::<Vec<_>>()
            .join(sep);
        out.push('\n');
        for row in 0..table.columns[0].data.len() {
            let cells: Vec<String> = table
                .columns
                .iter()
                .map(|c| {
                    let t = c.data.text_at(row);
                    if comma { t.replace('.', ",") } else { t }
                })
                .collect();
            out.push_str(&cells.join(sep));
            out.push('\n');
        }
        out
    };
    for (sep, comma) in [(",", false), (";", true), ("\t", false)] {
        let text = write(sep, comma);
        let r = readings(text.as_bytes(), &CsvOptions::default()).unwrap();
        assert!(r[0].checks.is_empty(), "{sep:?}: {:?}", r[0].checks);
        assert_eq!(r[0].table.columns, table.columns, "{sep:?}");
    }
}

#[test]
fn french_headers_are_recognised() {
    let t = ImportedTable::from_csv(
        "Sujet;Temps (h);Concentration (mg/L);Dose (mg)\n1;0;0;100\n1;1;5;100\n".as_bytes(),
        &CsvOptions::default(),
    )
    .unwrap();
    let roles: Vec<ColumnRole> = t.columns.iter().map(|c| c.role).collect();
    assert_eq!(
        roles,
        [
            ColumnRole::Subject,
            ColumnRole::Time,
            ColumnRole::Concentration,
            ColumnRole::Dose
        ]
    );
}

#[test]
fn many_subjects_are_grouped_without_a_quadratic_search() {
    // 20 000 subjects of three rows each: grouping must be a hash lookup, not a scan of the groups.
    let mut text = String::from("id,time,conc\n");
    for subject in 0..20_000 {
        for (t, c) in [(0, 0), (1, 5), (2, 3)] {
            text.push_str(&format!("S{subject},{t},{c}\n"));
        }
    }
    let table = ImportedTable::from_csv(text.as_bytes(), &CsvOptions::default()).unwrap();
    let start = std::time::Instant::now();
    let checks = crate::csv::check_table(&table);
    let checked = start.elapsed();
    assert!(checks.is_empty());
    assert!(checked.as_millis() < 100, "check_table took {checked:?}");

    let mut project = Project::new("big");
    let id = project.add_worksheet("big", table.columns).unwrap();
    let start = std::time::Instant::now();
    let subjects = project.worksheet(id).unwrap().subjects();
    let listed = start.elapsed();
    assert_eq!(subjects.len(), 20_000);
    assert_eq!(
        subjects.first().map(String::as_str),
        Some("S0"),
        "order of first appearance"
    );
    assert_eq!(subjects.last().map(String::as_str), Some("S19999"));
    assert!(listed.as_millis() < 100, "subjects took {listed:?}");
}

#[test]
fn a_comma_that_may_be_a_thousands_separator_is_noted_under_a_decimal_comma() {
    let t = ImportedTable::from_csv(b"time;conc\n0;0\n1;1,500\n2;3,25\n", &CsvOptions::default())
        .unwrap();
    assert!(t.decimal_comma);
    assert!(
        t.notes
            .iter()
            .any(|n| n.contains("`1,500`") && n.contains("thousands separator")),
        "{:?}",
        t.notes
    );
    // No such value, no note.
    let t = ImportedTable::from_csv(b"time;conc\n0;0\n1;1,5\n2;3,25\n", &CsvOptions::default())
        .unwrap();
    assert!(
        !t.notes.iter().any(|n| n.contains("thousands")),
        "{:?}",
        t.notes
    );
    // A zero before the mark is a decimal (0,500), not a thousands group.
    let t =
        ImportedTable::from_csv(b"time;conc\n0;0,500\n1;1,5\n", &CsvOptions::default()).unwrap();
    assert!(
        !t.notes.iter().any(|n| n.contains("thousands")),
        "{:?}",
        t.notes
    );
}
