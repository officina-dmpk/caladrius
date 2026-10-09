use std::fs;
use std::path::PathBuf;

use serde_json::{Value, json};

use super::*;

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

fn run_args(list: &[&str]) -> Result<Output, CliError> {
    run(&args(list), &mut std::io::empty())
}

fn failure(list: &[&str]) -> String {
    match run_args(list) {
        Ok(o) => panic!("should have failed, printed {}", o.stdout),
        Err(e) => e.0,
    }
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "caladrius-cli-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

const CSV: &str = "Subject,Time (h),Conc (mg/L),Dose (mg)\n\
A,0,0,100\nA,0.5,2.195,100\nA,1,3.293,100\nA,2,3.971,100\nA,4,3.611,100\nA,6,2.989,100\nA,8,2.451,100\nA,12,1.643,100\nA,24,0.495,100\n";

fn csv_file(dir: &std::path::Path) -> String {
    let path = dir.join("study.csv");
    fs::write(&path, CSV).unwrap();
    path.to_str().unwrap().to_owned()
}

fn json_out(o: &Output) -> Value {
    serde_json::from_str(&o.stdout).unwrap()
}

#[test]
fn arguments_are_parsed_in_both_flag_styles() {
    assert_eq!(parse_args(&args(&["help"])).unwrap(), Action::Help);
    assert_eq!(parse_args(&args(&["--help"])).unwrap(), Action::Help);
    assert_eq!(
        parse_args(&args(&["commands", "--format=json"])).unwrap(),
        Action::Commands(Some(Format::Json))
    );
    let Action::Run(r) = parse_args(&args(&[
        "nca.run",
        "--param",
        "route=extravascular",
        "--csv=a.csv",
        "--format",
        "csv",
        "--param=dose=100",
    ]))
    .unwrap() else {
        panic!("expected a run");
    };
    assert_eq!(r.command, "nca.run");
    assert_eq!(r.params, ["route=extravascular", "dose=100"]);
    assert_eq!(r.csv, Some(PathBuf::from("a.csv")));
    assert_eq!(r.format, Some(Format::Csv));
}

#[test]
fn bad_arguments_say_what_to_fix() {
    for (list, text) in [
        (vec![], "missing command"),
        (vec!["nca.run", "--bogus"], "unknown option `--bogus`"),
        (vec!["nca.run", "--param"], "needs a value"),
        (vec!["nca.run", "extra"], "unexpected argument"),
        (vec!["nca.run", "--format", "xml"], "unknown format"),
        (vec!["commands", "--csv", "x"], "unknown option"),
        (vec!["nca.run", "--param", "novalue"], "KEY=VALUE"),
        (vec!["nca.run", "--param", "=1"], "empty parameter name"),
        (vec!["nca.run", "--param", "a..b=1"], "empty parameter name"),
    ] {
        assert!(failure(&list).contains(text), "{list:?}");
    }
}

#[test]
fn parameters_are_json_when_they_parse_and_nest_by_dots() {
    let mut map = Map::new();
    for p in [
        "worksheet=1",
        "route=extravascular",
        "options.auc_method=linear",
        "options.lambda_z.min_points=4",
        "flag=true",
        "list=[1,2]",
        "quoted=\"7\"",
    ] {
        let (path, value) = parse_param(p, &read_text).unwrap();
        set_path(&mut map, &path, value).unwrap();
    }
    assert_eq!(
        Value::Object(map.clone()),
        json!({ "worksheet": 1, "route": "extravascular", "flag": true, "list": [1, 2],
                "quoted": "7",
                "options": { "auc_method": "linear", "lambda_z": { "min_points": 4 } } })
    );
    // A scalar cannot also be an object.
    let (path, value) = parse_param("route.x=1", &read_text).unwrap();
    assert!(set_path(&mut map, &path, value).is_err());
}

#[test]
fn a_parameter_can_come_from_a_file() {
    let dir = scratch("file-param");
    let path = dir.join("data.csv");
    fs::write(&path, "time,conc\n0,1\n").unwrap();
    let (_, value) = parse_param(&format!("csv=@{}", path.display()), &read_text).unwrap();
    assert_eq!(value, json!("time,conc\n0,1\n"));
    assert!(parse_param("csv=@/no/such/file.csv", &read_text).is_err());
}

#[test]
fn commands_are_listed_as_text_and_as_json_with_schemas() {
    let text = run_args(&["commands"]).unwrap().stdout;
    assert_eq!(text.lines().count(), commands().len());
    assert!(text.lines().any(|l| l.starts_with("nca.run ")));
    let full = json_out(&run_args(&["commands", "--format", "json"]).unwrap());
    let list = full.as_array().unwrap();
    assert_eq!(list.len(), commands().len());
    for c in list {
        assert!(c["params_schema"]["$schema"].is_string());
        assert!(c["result_schema"]["$schema"].is_string());
    }
    assert!(run_args(&["commands", "--format", "csv"]).is_err());
}

#[test]
fn an_unknown_command_lists_the_known_ones() {
    let m = failure(&["nca.runn"]);
    assert!(
        m.contains("unknown_command") && m.contains("nca.run"),
        "{m}"
    );
}

#[test]
fn nca_from_a_csv_prints_json_or_a_table() {
    let dir = scratch("nca");
    let file = csv_file(&dir);
    let out = run_args(&["nca.run", "--csv", &file, "--param", "route=extravascular"]).unwrap();
    let v = json_out(&out);
    assert_eq!(v["kind"], "nca");
    assert_eq!(v["status"], json!({ "state": "fresh" }));
    // The import notes go to the notes, not to the output.
    assert!(
        out.notes.iter().any(|n| n.contains("unit `mg/L`")),
        "{:?}",
        out.notes
    );

    let csv = run_args(&[
        "nca.run",
        "--csv",
        &file,
        "--param",
        "route=extravascular",
        "--format",
        "csv",
    ])
    .unwrap()
    .stdout;
    assert!(csv.starts_with("subject,parameter,value,not_calculated_reason\nA,"));
    assert!(csv.lines().any(|l| l.starts_with("A,cmax,3.971,")), "{csv}");

    let table = run_args(&[
        "nca.run",
        "--csv",
        &file,
        "--param",
        "route=extravascular",
        "--format",
        "csv",
        "--table",
        "nca.parameters",
    ])
    .unwrap()
    .stdout;
    assert_eq!(table, csv);
    assert!(
        failure(&[
            "nca.run",
            "--csv",
            &file,
            "--param",
            "route=extravascular",
            "--table",
            "nca.parameters"
        ])
        .contains("--table needs --format csv")
    );
}

#[test]
fn fit_and_simulation_have_csv_forms() {
    let dir = scratch("fit");
    let file = csv_file(&dir);
    let fit = run_args(&[
        "fit.run",
        "--csv",
        &file,
        "--param",
        "model=pk1.oral_1",
        "--format",
        "csv",
    ])
    .unwrap()
    .stdout;
    assert!(
        fit.starts_with("parameter,estimate,se,cv_percent,ci_lo,ci_hi\n"),
        "{fit}"
    );
    assert_eq!(fit.lines().count(), 4);
    let curve = run_args(&[
        "fit.run",
        "--csv",
        &file,
        "--param",
        "model=pk1.oral_1",
        "--format",
        "csv",
        "--table",
        "fit.curve",
    ])
    .unwrap()
    .stdout;
    assert_eq!(curve.lines().next(), Some("time,conc"));
    let sim = run_args(&[
        "model.simulate",
        "--param",
        "model=pk1.iv_bolus",
        "--param",
        "dose=10",
        "--param",
        "params.v=5",
        "--param",
        "params.k=0.2",
        "--param",
        "times=[0,1,2]",
        "--format",
        "csv",
    ])
    .unwrap()
    .stdout;
    assert_eq!(sim.lines().next(), Some("time,conc,auc"));
    assert_eq!(sim.lines().count(), 4);
    assert!(sim.contains("\n0,2,0\n"), "{sim}");
}

#[test]
fn json_parameters_come_from_a_file_or_standard_input_and_params_override_them() {
    let dir = scratch("json");
    let params = dir.join("p.json");
    fs::write(
        &params,
        json!({ "model": "pk1.iv_bolus", "dose": 10, "params": { "v": 5, "k": 0.2 }, "times": [1] })
            .to_string(),
    )
    .unwrap();
    let path = params.to_str().unwrap();
    let a = json_out(&run_args(&["model.simulate", "--json", path]).unwrap());
    let b = json_out(&run_args(&["model.simulate", "--json", path, "--param", "dose=20"]).unwrap());
    let (ca, cb) = (
        a["conc"][0].as_f64().unwrap(),
        b["conc"][0].as_f64().unwrap(),
    );
    assert!((cb - 2.0 * ca).abs() < 1e-12);
    let mut input: &[u8] =
        br#"{"model":"pk1.iv_bolus","dose":10,"params":{"v":5,"k":0.2},"times":[1]}"#;
    let c = run(&args(&["model.simulate", "--json", "-"]), &mut input).unwrap();
    assert_eq!(json_out(&c)["conc"], a["conc"]);

    fs::write(&params, "[1]").unwrap();
    assert!(failure(&["model.simulate", "--json", path]).contains("JSON object"));
    fs::write(&params, "{").unwrap();
    assert!(failure(&["model.simulate", "--json", path]).contains("not valid JSON"));
    assert!(failure(&["model.simulate", "--json", "/no/such/p.json"]).contains("cannot read"));
}

#[test]
fn engine_errors_are_one_line_with_a_code() {
    let m = failure(&["model.simulate", "--param", "model=pk1.iv_bolus"]);
    assert!(m.starts_with("invalid_parameters: "), "{m}");
    let dir = scratch("err");
    let file = csv_file(&dir);
    let m = failure(&["nca.run", "--csv", &file, "--param", "route=sideways"]);
    assert!(m.starts_with("invalid_parameters: "), "{m}");
    let m = failure(&["nca.run", "--csv", "/no/such.csv"]);
    assert!(m.contains("cannot read"), "{m}");
    let m = failure(&["data.describe", "--param", "worksheet=4"]);
    assert!(m.starts_with("unknown_worksheet: "), "{m}");
    assert!(!m.contains('\n'));
}

#[test]
fn a_project_file_carries_state_between_invocations() {
    let dir = scratch("project");
    let file = csv_file(&dir);
    let project = dir.join("study.caladrius.json");
    let project = project.to_str().unwrap();
    // First call: import and run; the project is written.
    run_args(&[
        "nca.run",
        "--csv",
        &file,
        "--project",
        project,
        "--param",
        "route=extravascular",
    ])
    .unwrap();
    assert!(fs::metadata(project).unwrap().len() > 100);
    // Second call: the worksheet and the analysis are there.
    let v = json_out(&run_args(&["project.describe", "--project", project]).unwrap());
    assert_eq!(v["worksheets"].as_array().unwrap().len(), 1);
    assert_eq!(v["analyses"][0]["status"], json!({ "state": "fresh" }));
    // An edit is written back and makes the analysis stale for the next call.
    let before = fs::read(project).unwrap();
    run_args(&["project.describe", "--project", project]).unwrap();
    assert_eq!(
        fs::read(project).unwrap(),
        before,
        "a read-only command leaves the file alone"
    );
    run_args(&[
        "data.set_cell",
        "--project",
        project,
        "--param",
        "column=Conc",
        "--param",
        "row=3",
        "--param",
        "value=4.5",
    ])
    .unwrap();
    let a = json_out(
        &run_args(&[
            "analysis.get",
            "--project",
            project,
            "--param",
            "analysis=2",
        ])
        .unwrap(),
    );
    assert_eq!(a["status"]["state"], "stale");
    // Run again, export the table, the history shows the earlier invocations.
    let table = run_args(&[
        "analysis.run",
        "--project",
        project,
        "--param",
        "analysis=2",
        "--format",
        "csv",
    ])
    .unwrap()
    .stdout;
    assert!(table.contains("A,cmax,4.5,"), "{table}");
    let h = json_out(&run_args(&["history.list", "--project", project]).unwrap());
    let commands: Vec<&str> = h["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["command"].as_str().unwrap())
        .collect();
    assert!(
        commands.starts_with(&["data.import", "nca.run", "project.save"]),
        "{commands:?}"
    );
    // A damaged project is refused without being overwritten.
    fs::write(project, "{}").unwrap();
    assert!(failure(&["project.describe", "--project", project]).contains("load_failed"));
    assert_eq!(fs::read_to_string(project).unwrap(), "{}");
}

#[test]
fn an_argument_that_is_not_text_is_refused_with_its_position() {
    use std::ffi::OsString;
    #[cfg(windows)]
    let bad = {
        use std::os::windows::ffi::OsStringExt;
        OsString::from_wide(&[0x61, 0xD800])
    };
    #[cfg(unix)]
    let bad = {
        use std::os::unix::ffi::OsStringExt;
        OsString::from_vec(vec![0x61, 0xff])
    };
    let e = text_args([OsString::from("nca.run"), bad]).unwrap_err();
    assert_eq!(e.0, "argument 2 is not valid text");
    assert_eq!(
        text_args([OsString::from("a"), OsString::from("b")]).unwrap(),
        ["a", "b"]
    );
}

#[test]
fn data_import_with_a_csv_file_is_the_import_itself() {
    let dir = scratch("import");
    let file = csv_file(&dir);
    let project = dir.join("p.caladrius.json");
    let project = project.to_str().unwrap();
    let out = run_args(&["data.import", "--csv", &file, "--project", project]).unwrap();
    let v = json_out(&out);
    assert_eq!(v["worksheet"]["name"], "study");
    assert_eq!(v["worksheet"]["rows"], 9);
    assert!(
        out.notes.iter().any(|n| n.contains("unit `mg/L`")),
        "{:?}",
        out.notes
    );
    // The project was written: the worksheet is there for the next call.
    let d = json_out(&run_args(&["project.describe", "--project", project]).unwrap());
    assert_eq!(d["worksheets"][0]["name"], "study");
    // A name can be given, and the CSV cannot be given twice.
    let v = json_out(&run_args(&["data.import", "--csv", &file, "--param", "name=mine"]).unwrap());
    assert_eq!(v["worksheet"]["name"], "mine");
    assert!(failure(&["data.import", "--csv", &file, "--param", "csv=a,b"]).contains("once"));
}

#[test]
fn a_repeated_parameter_is_refused_naming_the_key() {
    for list in [
        vec![
            "data.describe",
            "--param",
            "worksheet=1",
            "--param",
            "worksheet=2",
        ],
        vec![
            "nca.run",
            "--param",
            "options.auc_method=linear",
            "--param",
            "options=1",
        ],
        vec![
            "nca.run",
            "--param",
            "options=1",
            "--param",
            "options.auc_method=linear",
        ],
    ] {
        let m = failure(&list);
        assert!(
            m.contains("--param") && m.contains("conflicts"),
            "{list:?}: {m}"
        );
    }
    let m = failure(&[
        "data.describe",
        "--param",
        "worksheet=1",
        "--param",
        "worksheet=2",
    ]);
    assert!(m.contains("`--param worksheet`"), "{m}");
    // Siblings are not a conflict.
    assert!(check_duplicate_params(&args(&["options.a=1", "options.b=2", "route=x"])).is_ok());
}

#[test]
fn bad_flag_combinations_are_refused_before_anything_runs() {
    let dir = scratch("flags");
    let file = csv_file(&dir);
    let project = dir.join("p.caladrius.json");
    let project = project.to_str().unwrap();
    for (list, text) in [
        (
            vec!["nca.run", "--format", "csv", "--table", "nope"],
            "no table `nope`",
        ),
        (
            vec!["nca.run", "--table", "nca.parameters"],
            "--table needs --format csv",
        ),
        (vec!["nca.run", "--format", "text"], "only for `commands`"),
        (
            vec!["data.describe", "--format", "csv"],
            "no table to print",
        ),
        (
            vec!["project.describe", "--format", "csv"],
            "no table to print",
        ),
    ] {
        let mut full = list.clone();
        full.extend([
            "--csv",
            &file,
            "--project",
            project,
            "--param",
            "route=extravascular",
        ]);
        let m = failure(&full);
        assert!(m.contains(text), "{list:?}: {m}");
        // Nothing ran: no project was written.
        assert!(!std::path::Path::new(project).exists(), "{list:?}");
    }
}

#[test]
fn a_numeric_subject_may_be_spelled_one_point_zero() {
    let dir = scratch("subject");
    let file = dir.join("n.csv");
    fs::write(
        &file,
        "id,time,conc,dose\n1,0,0,10\n1,1,5,10\n1,2,3,10\n1,4,1,10\n",
    )
    .unwrap();
    let file = file.to_str().unwrap();
    for subject in ["subject=1", "subject=1.0", "subject=\"1\""] {
        let v = json_out(
            &run_args(&[
                "nca.run",
                "--csv",
                file,
                "--param",
                "route=extravascular",
                "--param",
                subject,
            ])
            .unwrap(),
        );
        assert_eq!(v["result"]["subjects"][0]["subject"], "1", "{subject}");
    }
}

/// A pk2 profile simulated by `model.simulate` (public oracle parameters cl 2, vc 10, q 4, vp 8,
/// dose 100) with a 1 % alternating error, written as a CSV file.
fn pk2_csv_file(dir: &std::path::Path) -> String {
    let sim = run_args(&[
        "model.simulate",
        "--param",
        "model=pk2.iv_bolus",
        "--param",
        "dose=100",
        "--param",
        "params.cl=2",
        "--param",
        "params.vc=10",
        "--param",
        "params.q=4",
        "--param",
        "params.vp=8",
        "--param",
        "times=[0.1,0.25,0.5,1,2,3,4,6,8,12,16,24,36,48]",
    ])
    .unwrap();
    let sim = json_out(&sim);
    let times = sim["times"].as_array().unwrap();
    let conc = sim["conc"].as_array().unwrap();
    let mut csv = String::from("Subject,Time (h),Conc (mg/L),Dose (mg)\n");
    for (i, (t, c)) in times.iter().zip(conc).enumerate() {
        let noise = if i % 2 == 0 { 1.01 } else { 0.99 };
        csv.push_str(&format!(
            "A,{},{},100\n",
            t.as_f64().unwrap(),
            c.as_f64().unwrap() * noise
        ));
    }
    let path = dir.join("pk2.csv");
    fs::write(&path, csv).unwrap();
    path.to_str().unwrap().to_owned()
}

#[test]
fn a_pk2_fit_runs_from_the_command_line() {
    let dir = scratch("pk2");
    let file = pk2_csv_file(&dir);
    let fit = run_args(&[
        "fit.run",
        "--csv",
        &file,
        "--param",
        "model=pk2.iv_bolus",
        "--param",
        "initial.cl=1.6",
        "--param",
        "initial.vc=11",
        "--param",
        "initial.q=3.2",
        "--param",
        "initial.vp=9",
        "--format",
        "csv",
    ])
    .unwrap()
    .stdout;
    assert!(
        fit.starts_with("parameter,estimate,se,cv_percent,ci_lo,ci_hi\n"),
        "{fit}"
    );
    assert_eq!(fit.lines().count(), 5, "{fit}");
    for name in ["cl", "vc", "q", "vp"] {
        assert!(fit.contains(&format!("\n{name},")), "{name}: {fit}");
    }
    // Initial estimates are not generated for pk2: the refusal is one readable line.
    let refused = failure(&[
        "fit.initial_estimates",
        "--csv",
        &file,
        "--param",
        "model=pk2.iv_bolus",
    ]);
    assert!(refused.contains("fit_error"), "{refused}");
    assert!(refused.contains("enter initial estimates"), "{refused}");
}

#[test]
fn compare_is_analysis_compare_with_the_two_ids_as_arguments() {
    let Action::Run(r) = parse_args(&args(&["compare", "2", "3", "--format", "csv"])).unwrap()
    else {
        panic!("a run");
    };
    assert_eq!(r.command, "analysis.compare");
    assert_eq!(r.params, ["a=2", "b=3"]);
    assert_eq!(r.format, Some(Format::Csv));
    // The long spelling takes the ids as parameters.
    let Action::Run(r) = parse_args(&args(&["analysis.compare", "--param", "a=2"])).unwrap() else {
        panic!("a run");
    };
    assert_eq!(r.command, "analysis.compare");
    // Anything but a number is refused with the way out; a third id is not an option.
    assert!(failure(&["compare", "x", "3"]).contains("not an analysis id"));
    assert!(failure(&["compare", "2", "3", "4"]).contains("unexpected argument `4`"));
    // Other commands still take no bare argument.
    assert!(failure(&["nca.run", "2"]).contains("unexpected argument `2`"));
}

#[test]
fn compare_runs_on_a_project_and_prints_json_or_a_table() {
    let dir = scratch("compare");
    let file = csv_file(&dir);
    let project = dir.join("study.caladrius.json");
    let project = project.to_str().unwrap();
    let units = r#"options.units={"time":"h","concentration":"mg/L","dose":"mg"}"#;
    // Analysis 2 imports the worksheet; analysis 3 uses it again.
    for (method, with_csv) in [("linear", true), ("lin_up_log_down", false)] {
        let method = format!("options.auc_method={method}");
        let mut list = vec!["nca.run", "--project", project];
        if with_csv {
            list.extend(["--csv", &file]);
        }
        list.extend([
            "--param",
            "route=extravascular",
            "--param",
            units,
            "--param",
            &method,
        ]);
        run_args(&list).unwrap();
    }
    let before = fs::read(project).unwrap();
    let json = json_out(
        &run_args(&[
            "compare",
            "2",
            "3",
            "--project",
            project,
            "--param",
            r#"parameters=["auclast","tlag"]"#,
        ])
        .unwrap(),
    );
    assert_eq!(json["a"]["analysis"], 2);
    assert_eq!(json["b"]["subject"], "A");
    let rows = json["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    let (a, b) = (
        rows[0]["a"].as_f64().unwrap(),
        rows[0]["b"].as_f64().unwrap(),
    );
    assert_eq!(rows[0]["difference"], b - a);
    assert_eq!(rows[0]["ratio"], b / a);
    // tlag is 0 in a: the difference is kept, the percentage and the ratio are not.
    assert_eq!(rows[1]["a"], 0.0);
    assert_eq!(rows[1]["relative_percent"], Value::Null);
    // The table: the header, a line per parameter, an empty cell for what was not computed.
    let table = run_args(&[
        "compare",
        "2",
        "3",
        "--project",
        project,
        "--param",
        r#"parameters=["auclast","tlag"]"#,
        "--format",
        "csv",
    ])
    .unwrap()
    .stdout;
    let lines: Vec<&str> = table.lines().collect();
    assert_eq!(
        lines[0],
        "parameter,a,b,unit_a,unit_b,difference,difference_unit,relative_percent,ratio,not_comparable"
    );
    assert_eq!(lines.len(), 3, "{table}");
    let auc: Vec<&str> = lines[1].split(',').collect();
    assert_eq!(auc[0], "auclast");
    assert_eq!(auc[1].parse::<f64>().unwrap(), a);
    assert_eq!(auc[5].parse::<f64>().unwrap(), b - a);
    assert_eq!(auc[9], "");
    // tlag: "a is zero: ..." is the reason, the percentage and the ratio are empty.
    assert!(
        lines[2].starts_with("tlag,0,0,h,h,0,h,,,a is zero"),
        "{}",
        lines[2]
    );
    // Comparing changes nothing in the project file.
    assert_eq!(fs::read(project).unwrap(), before);
    // The table is a --format csv of this command only.
    assert!(
        failure(&[
            "compare",
            "2",
            "3",
            "--project",
            project,
            "--table",
            "nca.parameters"
        ])
        .contains("--table needs --format csv")
    );
    // A bad id is one line with its code.
    assert!(
        failure(&["compare", "2", "99", "--project", project]).starts_with("unknown_analysis: ")
    );
}

#[test]
fn a_reason_with_a_comma_is_quoted_in_the_table() {
    let result = json!({
        "a": { "analysis": 1 }, "b": { "analysis": 2 },
        "rows": [{ "parameter": "x", "a": 1.5, "b": null, "unit_a": "h", "unit_b": null,
                   "difference": null, "difference_unit": null, "relative_percent": null,
                   "ratio": null, "not_comparable": "a, \"b\"" }]
    });
    assert_eq!(
        compare_csv(&result),
        "parameter,a,b,unit_a,unit_b,difference,difference_unit,relative_percent,ratio,not_comparable\nx,1.5,,h,,,,,,\"a, \"\"b\"\"\"\n"
    );
}
