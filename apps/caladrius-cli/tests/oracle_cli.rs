#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: helpers panic on purpose
//! The command-line application against the public oracle: the binary is run on the dataset of a
//! case (`caladrius-cli nca.run --csv <dataset> ... --format csv`), and the table it prints is
//! compared, through the test kit, with the PKNCA values of `oracle/expected/` at the NCA
//! tolerance. Also: exit codes and messages of the binary.

use std::path::PathBuf;
use std::process::{Command, Output};

use caladrius_testkit::oracle::oracle_dir;
use caladrius_testkit::{Table, Tolerance, compare_tables, list_cases, load_case};

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_caladrius-cli"))
        .args(args)
        .output()
        .expect("the binary runs")
}

fn stdout(o: &Output) -> String {
    String::from_utf8(o.stdout.clone()).unwrap()
}

fn stderr(o: &Output) -> String {
    String::from_utf8(o.stderr.clone()).unwrap()
}

/// Runs `nca.run` on the dataset of `case` with the options that case was computed with, and
/// reads the table the binary prints into (subject, parameter) -> value.
fn nca_table(case: &str, extra: &[&str]) -> Table {
    let case = load_case(case).unwrap();
    let data: PathBuf = oracle_dir().join(&case.options.data_file);
    let data = data.to_str().unwrap();
    let route = format!("route={}", case.options.route);
    let mut args = vec![
        "nca.run", "--csv", data, "--param", &route, "--format", "csv",
    ];
    args.extend_from_slice(extra);
    let out = cli(&args);
    assert!(out.status.success(), "{case:?}: {}", stderr(&out));
    let text = stdout(&out);
    let mut lines = text.lines();
    assert_eq!(
        lines.next(),
        Some("subject,parameter,value,not_calculated_reason")
    );
    let mut table = Table::new();
    for line in lines {
        let mut f = line.splitn(4, ',');
        let (subject, parameter, value, reason) = (
            f.next().unwrap(),
            f.next().unwrap(),
            f.next().unwrap(),
            f.next().unwrap_or(""),
        );
        let value = if value.is_empty() {
            assert!(!reason.is_empty(), "{line}: no value and no reason");
            None
        } else {
            Some(value.parse::<f64>().unwrap())
        };
        table.insert(subject, parameter, value);
    }
    table
}

fn check_case(name: &str, extra: &[&str]) {
    let case = load_case(name).unwrap();
    let printed = nca_table(name, extra);
    // The oracle lists the parameters of the case; the binary prints more. Compare those.
    let mut actual = Table::new();
    for (subject, parameter, _) in case.expected.iter() {
        actual.insert(
            subject,
            parameter,
            printed.get(subject, parameter).flatten(),
        );
    }
    let report = compare_tables(&case.expected, &actual, Tolerance::NCA_VS_PKNCA);
    assert!(report.is_ok(), "{name}: {report}");
    assert!(case.expected.len() > 100, "{name}: a real comparison");
}

#[test]
fn theoph_through_the_binary_reproduces_pknca() {
    check_case("theoph", &["--param", "options.auc_method=lin_up_log_down"]);
}

#[test]
fn theoph_with_linear_trapezoids_reproduces_pknca() {
    check_case("theoph_linear", &["--param", "options.auc_method=linear"]);
}

#[test]
fn indometh_iv_bolus_reproduces_pknca() {
    check_case("indometh", &[]);
    check_case("indometh_linear", &["--param", "options.auc_method=linear"]);
}

#[test]
fn the_cases_run_here_exist_in_the_oracle() {
    let names = list_cases().unwrap();
    for n in ["theoph", "theoph_linear", "indometh", "indometh_linear"] {
        assert!(names.iter().any(|x| x == n), "{n}");
    }
}

#[test]
fn a_numeric_subject_label_names_a_subject() {
    let data = oracle_dir().join("data").join("theoph.csv");
    let out = cli(&[
        "nca.run",
        "--csv",
        data.to_str().unwrap(),
        "--param",
        "route=extravascular",
        "--param",
        "subject=3",
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    let v: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(v["result"]["subjects"].as_array().unwrap().len(), 1);
    assert_eq!(v["result"]["subjects"][0]["subject"], "3");
    assert_eq!(v["label"], "NCA of theoph, subject 3");
}

#[test]
fn json_output_is_the_command_result() {
    let data = oracle_dir().join("data").join("theoph.csv");
    let out = cli(&[
        "nca.run",
        "--csv",
        data.to_str().unwrap(),
        "--param",
        "route=extravascular",
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    let v: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(v["kind"], "nca");
    assert_eq!(v["result"]["subjects"].as_array().unwrap().len(), 12);
    assert_eq!(v["status"]["state"], "fresh");
}

#[test]
fn a_failure_is_one_readable_line_on_stderr_and_exit_code_1() {
    for args in [
        vec![],
        vec!["no.such.command"],
        vec!["nca.run", "--csv", "/no/such/file.csv"],
        vec!["nca.run", "--bogus"],
        vec!["model.simulate"],
    ] {
        let out = cli(&args);
        assert_eq!(out.status.code(), Some(1), "{args:?}");
        assert!(stdout(&out).is_empty(), "{args:?}");
        let err = stderr(&out);
        assert!(err.starts_with("error: "), "{args:?}: {err}");
        assert_eq!(err.trim_end().lines().count(), 1, "{args:?}: {err}");
    }
}

#[test]
fn the_command_list_is_the_describe_of_the_engine() {
    let out = cli(&["commands", "--format", "json"]);
    assert!(out.status.success());
    let printed: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    let ids: Vec<&str> = printed
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, caladrius_cli::commands());
    let help = cli(&["help"]);
    assert!(help.status.success());
    assert!(stdout(&help).contains("--param KEY=VALUE"));
}

#[test]
fn standard_input_can_carry_the_parameters() {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = Command::new(env!("CARGO_BIN_EXE_caladrius-cli"))
        .args(["model.simulate", "--json", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(br#"{"model":"pk1.iv_bolus","dose":10,"params":{"v":5,"k":0.2},"times":[0]}"#)
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    let v: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(v["conc"][0], 2.0);
}

/// The `auclast` of subject 1 in a case of the oracle.
fn expected_auclast(case: &str) -> f64 {
    load_case(case)
        .unwrap()
        .expected
        .get("1", "auclast")
        .flatten()
        .unwrap()
}

#[test]
fn compare_prints_the_difference_of_two_auc_methods_of_theoph_against_pknca() {
    let dir = std::env::temp_dir().join(format!("caladrius-cli-compare-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let project = dir.join("theoph.caladrius.json");
    let project = project.to_str().unwrap();
    let data = oracle_dir().join("data").join("theoph.csv");
    let data = data.to_str().unwrap();
    // The first call imports the file (worksheet 1, analysis 2); the second, on the project,
    // uses that worksheet (analysis 3).
    for (method, csv) in [("linear", true), ("lin_up_log_down", false)] {
        let option = format!("options.auc_method={method}");
        let mut args = vec!["nca.run", "--project", project];
        if csv {
            args.extend(["--csv", data]);
        }
        args.extend(["--param", "route=extravascular", "--param", "subject=1"]);
        args.extend(["--param", &option]);
        let out = cli(&args);
        assert!(out.status.success(), "{}", stderr(&out));
    }
    let out = cli(&[
        "compare",
        "2",
        "3",
        "--project",
        project,
        "--param",
        r#"parameters=["auclast","cmax","half.life"]"#,
        "--format",
        "csv",
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    let mut lines = text.lines();
    assert_eq!(
        lines.next(),
        Some(
            "parameter,a,b,unit_a,unit_b,difference,difference_unit,relative_percent,ratio,not_comparable"
        )
    );
    let rows: Vec<Vec<&str>> = lines.map(|l| l.split(',').collect()).collect();
    assert_eq!(rows.len(), 3, "{text}");
    let auc = &rows[0];
    assert_eq!(auc[0], "auclast");
    let (linear, mixed) = (
        expected_auclast("theoph_linear"),
        expected_auclast("theoph"),
    );
    let (a, b): (f64, f64) = (auc[1].parse().unwrap(), auc[2].parse().unwrap());
    assert!((a - linear).abs() <= 1e-6 * linear, "{a} {linear}");
    assert!((b - mixed).abs() <= 1e-6 * mixed, "{b} {mixed}");
    // The engine's own arithmetic on its own numbers, printed in full.
    let difference: f64 = auc[5].parse().unwrap();
    assert_eq!(difference, b - a);
    assert_eq!(auc[8].parse::<f64>().unwrap(), b / a);
    assert!(difference < 0.0);
    // No units were given to nca.run: no unit, and still a number.
    assert_eq!((auc[3], auc[6], auc[9]), ("", "", ""));
    // A bad id is one readable line.
    let bad = cli(&["compare", "2", "x", "--project", project]);
    assert_eq!(bad.status.code(), Some(1));
    assert!(stderr(&bad).starts_with("error: "), "{}", stderr(&bad));
    let missing = cli(&["compare", "2", "99", "--project", project]);
    assert_eq!(missing.status.code(), Some(1));
    assert!(
        stderr(&missing).contains("unknown_analysis"),
        "{}",
        stderr(&missing)
    );
    let _ = std::fs::remove_dir_all(&dir);
}
