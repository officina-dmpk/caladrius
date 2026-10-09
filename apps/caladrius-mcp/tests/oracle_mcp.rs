#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: helpers panic on purpose
//! The MCP server as an agent host sees it: the binary is spawned and spoken to over stdio
//! (initialize, tools/list, tools/call data_import then nca_run on the Theoph dataset), and the
//! NCA parameters it returns are compared, through the test kit, with the PKNCA values of
//! `oracle/expected/theoph`.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use caladrius_testkit::oracle::oracle_dir;
use caladrius_testkit::{Table, Tolerance, compare_tables, load_case};
use serde_json::{Value, json};

struct Session {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl Session {
    fn start() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_caladrius-mcp"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the server starts");
        let stdin = child.stdin.take();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Session {
            child,
            stdin,
            stdout,
            next_id: 1,
        }
    }

    fn write_raw(&mut self, bytes: &[u8]) {
        let stdin = self.stdin.as_mut().unwrap();
        stdin.write_all(bytes).unwrap();
        stdin.write_all(b"\n").unwrap();
        stdin.flush().unwrap();
    }

    fn read_line(&mut self) -> Value {
        let mut line = String::new();
        let n = self.stdout.read_line(&mut line).unwrap();
        assert!(n > 0, "the server closed its output");
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("not JSON ({e}): {line}"))
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let message = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        self.write_raw(message.to_string().as_bytes());
        let answer = self.read_line();
        assert_eq!(answer["id"], id, "{answer}");
        answer
    }

    fn notify(&mut self, method: &str) {
        let message = json!({ "jsonrpc": "2.0", "method": method });
        self.write_raw(message.to_string().as_bytes());
    }

    fn tool(&mut self, name: &str, arguments: Value) -> Value {
        self.request(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        )
    }

    fn initialize(&mut self) -> Value {
        let answer = self.request(
            "initialize",
            json!({ "protocolVersion": "2025-06-18", "capabilities": {},
                    "clientInfo": { "name": "oracle-test", "version": "0" } }),
        );
        self.notify("notifications/initialized");
        answer
    }

    /// Closes the input and waits: the server ends cleanly when its input ends.
    fn finish(mut self) {
        drop(self.stdin.take());
        let status = self.child.wait().unwrap();
        assert!(status.success(), "{status}");
    }
}

#[test]
fn an_agent_runs_the_nca_of_theoph_and_gets_the_pknca_values() {
    let case = load_case("theoph").unwrap();
    let csv = fs::read_to_string(oracle_dir().join(&case.options.data_file)).unwrap();
    let mut s = Session::start();

    let init = s.initialize();
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(init["result"]["serverInfo"]["name"], "caladrius-mcp");

    let listed = s.request("tools/list", json!({}));
    let tools = listed["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    for expected in [
        "data_import",
        "nca_run",
        "fit_run",
        "model_simulate",
        "export_table",
    ] {
        assert!(names.contains(&expected), "{expected} in {names:?}");
    }
    let nca = tools.iter().find(|t| t["name"] == "nca_run").unwrap();
    assert!(nca["inputSchema"]["properties"]["worksheet"].is_object());

    let imported = s.tool("data_import", json!({ "name": "theoph", "csv": csv }));
    assert_eq!(imported["result"]["isError"], false, "{imported}");
    let worksheet = imported["result"]["structuredContent"]["worksheet"]["id"].clone();

    let run = s.tool(
        "nca_run",
        json!({ "worksheet": worksheet, "route": case.options.route }),
    );
    assert_eq!(run["result"]["isError"], false, "{run}");
    let result = &run["result"]["structuredContent"]["result"];
    let mut printed = Table::new();
    for subject in result["subjects"].as_array().unwrap() {
        let label = subject["subject"].as_str().unwrap();
        for p in subject["outcome"]["ok"]["parameters"].as_array().unwrap() {
            let value = p["value"]["value"].as_f64();
            printed.insert(label, p["name"].as_str().unwrap(), value);
        }
    }
    let mut actual = Table::new();
    for (subject, parameter, _) in case.expected.iter() {
        actual.insert(
            subject,
            parameter,
            printed.get(subject, parameter).flatten(),
        );
    }
    let report = compare_tables(&case.expected, &actual, Tolerance::NCA_VS_PKNCA);
    assert!(report.is_ok(), "{report}");
    assert!(case.expected.len() > 100);

    // The text content carries the same result as JSON.
    let text: Value =
        serde_json::from_str(run["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(&text, &run["result"]["structuredContent"]);

    // The session remembers the work: the analysis is there, fresh, and exports as CSV.
    let table = s.tool(
        "export_table",
        json!({ "table": "nca.parameters", "analysis": run["result"]["structuredContent"]["id"] }),
    );
    assert_eq!(table["result"]["structuredContent"]["status"], "fresh");
    assert!(
        table["result"]["structuredContent"]["csv"]
            .as_str()
            .unwrap()
            .starts_with("subject,parameter,value,not_calculated_reason\n")
    );
    s.finish();
}

#[test]
fn malformed_input_does_not_stop_the_server() {
    let mut s = Session::start();
    s.write_raw(b"{");
    assert_eq!(s.read_line()["error"]["code"], -32700);
    s.write_raw(&[0xff, 0xfe, 0xfd]);
    assert_eq!(s.read_line()["error"]["code"], -32700);
    s.write_raw(br#"{"jsonrpc":"2.0","id":1,"method":"nope"}"#);
    let answer = s.read_line();
    assert_eq!(answer["error"]["code"], -32601);
    assert_eq!(answer["id"], 1);
    s.write_raw(br#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"nca_run","arguments":[1]}}"#);
    let answer = s.read_line();
    assert_eq!(answer["result"]["isError"], true, "{answer}");
    // Still serving.
    let answer = s.request("ping", json!({}));
    assert_eq!(answer["result"], json!({}));
    s.finish();
}

#[test]
fn a_tool_error_is_reported_as_such_with_the_readable_message() {
    let mut s = Server::start_initialized();
    let answer = s.tool("data_describe", json!({ "worksheet": 12 }));
    assert_eq!(answer["result"]["isError"], true);
    assert!(
        answer["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("unknown_worksheet: ")
    );
    s.finish();
}

struct Server;

impl Server {
    fn start_initialized() -> Session {
        let mut s = Session::start();
        s.initialize();
        s
    }
}

#[test]
fn an_agent_compares_two_auc_methods_of_theoph_and_the_engine_does_the_arithmetic() {
    let csv = fs::read_to_string(oracle_dir().join("data").join("theoph.csv")).unwrap();
    let linear = load_case("theoph_linear")
        .unwrap()
        .expected
        .get("1", "auclast")
        .flatten()
        .unwrap();
    let mixed = load_case("theoph")
        .unwrap()
        .expected
        .get("1", "auclast")
        .flatten()
        .unwrap();
    let mut s = Session::start();
    s.initialize();
    let imported = s.tool("data_import", json!({ "name": "theoph", "csv": csv }));
    let worksheet = imported["result"]["structuredContent"]["worksheet"]["id"].clone();
    let mut ids = Vec::new();
    for method in ["linear", "lin_up_log_down"] {
        let run = s.tool(
            "nca_run",
            json!({ "worksheet": worksheet, "route": "extravascular", "subject": 1,
                    "options": { "auc_method": method } }),
        );
        assert_eq!(run["result"]["isError"], false, "{run}");
        ids.push(run["result"]["structuredContent"]["id"].clone());
    }
    let answer = s.tool(
        "analysis_compare",
        json!({ "a": ids[0], "b": ids[1], "parameters": ["auclast"] }),
    );
    assert_eq!(answer["result"]["isError"], false, "{answer}");
    let row = &answer["result"]["structuredContent"]["rows"][0];
    let (a, b) = (row["a"].as_f64().unwrap(), row["b"].as_f64().unwrap());
    // The values are PKNCA's (1e-6), and the three operations are the engine's own.
    assert!((a - linear).abs() <= 1e-6 * linear, "{a} {linear}");
    assert!((b - mixed).abs() <= 1e-6 * mixed, "{b} {mixed}");
    assert_eq!(row["difference"].as_f64().unwrap(), b - a);
    assert_eq!(
        row["relative_percent"].as_f64().unwrap(),
        (b - a) / a * 100.0
    );
    assert_eq!(row["ratio"].as_f64().unwrap(), b / a);
    // Theoph has no units: still compared, with no unit to quote.
    assert_eq!(row["unit_a"], Value::Null);
    assert_eq!(row["not_comparable"], Value::Null);
    // The whole subject list of the worksheet is refused with the way out.
    let all = s.tool(
        "nca_run",
        json!({ "worksheet": worksheet, "route": "extravascular" }),
    );
    let all_id = all["result"]["structuredContent"]["id"].clone();
    let refused = s.tool("analysis_compare", json!({ "a": ids[0], "b": all_id }));
    assert_eq!(refused["result"]["isError"], true);
    let text = refused["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.starts_with("ambiguous_subject: "), "{text}");
    s.finish();
}
