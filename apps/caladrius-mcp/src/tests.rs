use serde_json::{Value, json};

use super::*;

fn send(server: &mut Server, message: Value) -> Value {
    let line = server
        .handle_line(&message.to_string())
        .expect("a request is answered");
    serde_json::from_str(&line).unwrap()
}

fn call(server: &mut Server, id: u64, tool: &str, arguments: Value) -> Value {
    send(
        server,
        json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call",
                "params": { "name": tool, "arguments": arguments } }),
    )
}

fn assert_jsonrpc(answer: &Value) {
    assert_eq!(answer["jsonrpc"], "2.0", "{answer}");
    assert!(answer.get("id").is_some(), "{answer}");
    assert!(
        answer.get("result").is_some() ^ answer.get("error").is_some(),
        "exactly one of result and error: {answer}"
    );
}

#[test]
fn initialize_negotiates_the_version_and_announces_tools() {
    let mut s = Server::new();
    let r = send(
        &mut s,
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": { "protocolVersion": "2024-11-05", "capabilities": {},
                            "clientInfo": { "name": "t", "version": "0" } } }),
    );
    assert_jsonrpc(&r);
    assert_eq!(r["id"], 1);
    assert_eq!(r["result"]["protocolVersion"], "2024-11-05");
    assert_eq!(r["result"]["serverInfo"]["name"], "caladrius-mcp");
    assert!(r["result"]["capabilities"]["tools"].is_object());
    assert!(
        r["result"]["instructions"]
            .as_str()
            .unwrap()
            .contains("nca_run")
    );
    // An unknown version gets the newest we speak; missing params are tolerated.
    let r = send(
        &mut s,
        json!({ "jsonrpc": "2.0", "id": "a", "method": "initialize",
                "params": { "protocolVersion": "1999-01-01" } }),
    );
    assert_eq!(r["result"]["protocolVersion"], PROTOCOL_VERSIONS[0]);
    assert_eq!(r["id"], "a");
    let r = send(
        &mut s,
        json!({ "jsonrpc": "2.0", "id": 2, "method": "initialize" }),
    );
    assert_eq!(r["result"]["protocolVersion"], PROTOCOL_VERSIONS[0]);
}

#[test]
fn notifications_get_no_answer_and_ping_gets_an_empty_result() {
    let mut s = Server::new();
    for method in [
        "notifications/initialized",
        "notifications/cancelled",
        "anything/else",
    ] {
        let line = json!({ "jsonrpc": "2.0", "method": method }).to_string();
        assert_eq!(s.handle_line(&line), None, "{method}");
    }
    assert_eq!(s.handle_line(""), None);
    assert_eq!(s.handle_line("   "), None);
    let r = send(
        &mut s,
        json!({ "jsonrpc": "2.0", "id": 7, "method": "ping" }),
    );
    assert_eq!(r["result"], json!({}));
    // A response from the client (we send no requests) is ignored.
    let line = json!({ "jsonrpc": "2.0", "id": 1, "result": {} }).to_string();
    assert_eq!(s.handle_line(&line), None);
}

#[test]
fn every_command_is_a_tool_with_the_registry_schemas() {
    let mut s = Server::new();
    let r = send(
        &mut s,
        json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }),
    );
    let tools = r["result"]["tools"].as_array().unwrap();
    let commands = caladrius_engine::describe();
    assert_eq!(tools.len(), commands.len());
    for (tool, command) in tools.iter().zip(&commands) {
        let name = tool["name"].as_str().unwrap();
        assert_eq!(name, tool_name(&command.id));
        assert!(
            name.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
            "{name}"
        );
        assert!(name.len() <= 64);
        assert_eq!(tool["inputSchema"], command.params_schema, "{name}");
        assert_eq!(tool["inputSchema"]["type"], "object");
        assert_eq!(tool["outputSchema"], command.result_schema, "{name}");
        assert!(tool["description"].as_str().unwrap().contains(&command.id));
        assert_eq!(tool["annotations"]["readOnlyHint"], !command.mutates);
    }
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    for expected in [
        "nca_run",
        "fit_run",
        "data_import",
        "model_simulate",
        "export_table",
    ] {
        assert!(names.contains(&expected), "{expected}");
    }
    // Same list outside a session.
    assert_eq!(super::tools().len(), tools.len());
}

#[test]
fn a_session_keeps_its_worksheets_between_calls() {
    let mut s = Server::new();
    let r = call(
        &mut s,
        1,
        "data_import",
        json!({ "name": "w", "csv": "time,conc,dose\n0,0,10\n1,5,10\n2,3,10\n4,1,10\n8,0.2,10\n" }),
    );
    assert_jsonrpc(&r);
    assert_eq!(r["result"]["isError"], false);
    let id = r["result"]["structuredContent"]["worksheet"]["id"].clone();
    // The text content is the same JSON.
    let text: Value =
        serde_json::from_str(r["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(text, r["result"]["structuredContent"]);
    // The dotted command id works too.
    let r = call(&mut s, 2, "data.describe", json!({ "worksheet": id }));
    assert_eq!(r["result"]["isError"], false);
    assert_eq!(r["result"]["structuredContent"]["worksheet"]["rows"], 5);
    let r = call(
        &mut s,
        3,
        "nca_run",
        json!({ "worksheet": id, "route": "extravascular" }),
    );
    assert_eq!(
        r["result"]["structuredContent"]["status"],
        json!({ "state": "fresh" })
    );
    assert_eq!(s.engine().project().analyses().len(), 1);
}

#[test]
fn a_failing_command_is_a_tool_error_with_the_readable_message() {
    let mut s = Server::new();
    let r = call(&mut s, 1, "data_describe", json!({ "worksheet": 3 }));
    assert_jsonrpc(&r);
    assert_eq!(r["result"]["isError"], true);
    let text = r["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.starts_with("unknown_worksheet: "), "{text}");
    assert!(r["result"].get("structuredContent").is_none());
    // Bad arguments are tool errors too, whatever their shape.
    for arguments in [
        json!({ "bogus": 1 }),
        json!([1, 2]),
        json!("text"),
        json!(null),
        json!(12),
    ] {
        let r = call(&mut s, 2, "nca_run", arguments);
        assert_eq!(r["result"]["isError"], true, "{r}");
        assert!(
            r["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .starts_with("invalid_parameters: ")
        );
    }
    // No arguments at all.
    let r = send(
        &mut s,
        json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "project_describe" } }),
    );
    assert_eq!(r["result"]["isError"], false);
}

#[test]
fn protocol_errors_use_the_json_rpc_codes() {
    let mut s = Server::new();
    let cases: [(Value, i64); 9] = [
        (
            json!({ "jsonrpc": "2.0", "id": 1, "method": "resources/list" }),
            METHOD_NOT_FOUND,
        ),
        (
            json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call" }),
            INVALID_PARAMS,
        ),
        (
            json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": [] }),
            INVALID_PARAMS,
        ),
        (
            json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": 5 } }),
            INVALID_PARAMS,
        ),
        (
            json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": "nope" } }),
            INVALID_PARAMS,
        ),
        (json!({ "jsonrpc": "2.0", "id": 1 }), INVALID_REQUEST),
        (
            json!({ "jsonrpc": "1.0", "id": 1, "method": "ping" }),
            INVALID_REQUEST,
        ),
        (json!({ "id": 1, "method": "ping" }), INVALID_REQUEST),
        (
            json!({ "jsonrpc": "2.0", "id": [1], "method": "ping" }),
            INVALID_REQUEST,
        ),
    ];
    for (message, code) in cases {
        let r = send(&mut s, message.clone());
        assert_jsonrpc(&r);
        assert_eq!(r["error"]["code"], code, "{message} -> {r}");
        assert!(!r["error"]["message"].as_str().unwrap().is_empty());
    }
    let r = send(
        &mut s,
        json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": "nope" } }),
    );
    assert!(r["error"]["message"].as_str().unwrap().contains("nca_run"));
    // The id is echoed as it came.
    let r = send(
        &mut s,
        json!({ "jsonrpc": "2.0", "id": "req-9", "method": "nope" }),
    );
    assert_eq!(r["id"], "req-9");
    let r = send(
        &mut s,
        json!({ "jsonrpc": "2.0", "id": null, "method": "nope" }),
    );
    assert_eq!(r["id"], Value::Null);
}

#[test]
fn malformed_text_gets_a_parse_error_and_the_server_carries_on() {
    let mut s = Server::new();
    for line in [
        "{",
        "}",
        "nonsense",
        "[",
        "{\"jsonrpc\":",
        "\"just a string\"",
        "42",
        "null",
        "[]",
        "[1,2]",
        "true",
    ] {
        let answer = s.handle_line(line).expect(line);
        let v: Value = serde_json::from_str(&answer).unwrap();
        // A batch is answered by an array with one error per bad item.
        let items = if let Value::Array(items) = &v {
            items.clone()
        } else {
            vec![v]
        };
        assert!(!items.is_empty(), "{line}");
        for item in items {
            assert_eq!(item["jsonrpc"], "2.0", "{line}");
            assert!(item["error"]["code"].is_i64(), "{line}: {item}");
        }
    }
    let r = send(
        &mut s,
        json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" }),
    );
    assert_eq!(r["result"], json!({}));
}

#[test]
fn a_batch_is_answered_in_order_without_the_notifications() {
    let mut s = Server::new();
    let line = json!([
        { "jsonrpc": "2.0", "id": 1, "method": "ping" },
        { "jsonrpc": "2.0", "method": "notifications/initialized" },
        { "jsonrpc": "2.0", "id": 2, "method": "nope" },
    ])
    .to_string();
    let answer: Value = serde_json::from_str(&s.handle_line(&line).unwrap()).unwrap();
    let list = answer.as_array().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0]["id"], 1);
    assert_eq!(list[1]["error"]["code"], METHOD_NOT_FOUND);
    // Only notifications: nothing to say.
    let line = json!([{ "jsonrpc": "2.0", "method": "x" }]).to_string();
    assert_eq!(s.handle_line(&line), None);
}

/// A tiny deterministic generator, so the fuzz run is the same every time.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

#[test]
fn mutated_messages_never_break_the_server() {
    let valid = [
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18" } }),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
        json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "data_import",
                "arguments": { "name": "w", "csv": "time,conc\n0,0\n1,5\n2,3\n" } } }),
        json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": { "name": "nca_run",
                "arguments": { "worksheet": 1, "route": "extravascular" } } }),
        json!({ "jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": { "name": "model_simulate",
                "arguments": { "model": "pk1.oral_1", "dose": 1, "params": { "v": 1, "k": 1, "ka": 2 },
                               "grid": { "start": 0, "end": 5, "points": 4 } } } }),
    ];
    let mut rng = Lcg(0x5eed);
    let mut s = Server::new();
    let mut answered = 0;
    for round in 0..4000 {
        let base = valid[round % valid.len()].to_string();
        let mut bytes = base.clone().into_bytes();
        for _ in 0..=rng.below(4) {
            match rng.below(5) {
                0 if !bytes.is_empty() => {
                    let at = rng.below(bytes.len());
                    bytes.truncate(at);
                }
                1 if !bytes.is_empty() => {
                    let at = rng.below(bytes.len());
                    if let Some(b) = bytes.get_mut(at) {
                        *b = (rng.next() & 0xff) as u8;
                    }
                }
                2 if !bytes.is_empty() => {
                    let at = rng.below(bytes.len());
                    bytes.remove(at);
                }
                3 => {
                    let at = rng.below(bytes.len() + 1);
                    let junk = b"{}[]\",:-9e\\";
                    bytes.insert(at, junk.get(rng.below(junk.len())).copied().unwrap_or(b'{'));
                }
                _ => {
                    let copy = bytes.clone();
                    bytes.extend_from_slice(&copy);
                }
            }
        }
        // Replaced bytes may not be UTF-8: the line reader handles that; here, the lossy form.
        let line = String::from_utf8_lossy(&bytes).replace('\n', " ");
        if let Some(answer) = s.handle_line(&line) {
            answered += 1;
            let v: Value = serde_json::from_str(&answer).expect("an answer is JSON");
            let all = if let Value::Array(items) = &v {
                items.clone()
            } else {
                vec![v]
            };
            for item in &all {
                assert_eq!(item["jsonrpc"], "2.0", "{line} -> {answer}");
                assert!(
                    item.get("result").is_some() ^ item.get("error").is_some(),
                    "{answer}"
                );
            }
        }
    }
    assert!(
        answered > 1000,
        "the generator produced mostly unanswerable input"
    );
    // Still alive and well.
    let r = send(
        &mut s,
        json!({ "jsonrpc": "2.0", "id": 99, "method": "ping" }),
    );
    assert_eq!(r["result"], json!({}));
}

#[test]
fn deeply_nested_input_is_a_parse_error_not_a_crash() {
    let mut s = Server::new();
    let deep = format!("{}{}", "[".repeat(100_000), "]".repeat(100_000));
    let answer = s.handle_line(&deep).unwrap();
    assert!(answer.contains("-32700"), "{answer}");
    let deep_object = format!("{}1{}", "{\"a\":".repeat(5000), "}".repeat(5000));
    assert!(s.handle_line(&deep_object).is_some());
    // Huge numbers and odd tokens in the parameters are the engine's to refuse.
    let r = call(
        &mut s,
        1,
        "model_simulate",
        json!({ "model": "pk1.iv_bolus", "dose": 1e308,
        "params": { "v": 1e-308, "k": 1 }, "times": [1e308] }),
    );
    assert_jsonrpc(&r);
}

#[test]
fn serve_reads_lines_and_survives_bytes_that_are_not_text() {
    let mut s = Server::new();
    let mut input: Vec<u8> = Vec::new();
    input.extend_from_slice(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n");
    input.extend_from_slice(&[0xff, 0xfe, 0xfd, b'\n']);
    input.extend_from_slice(b"\n\n");
    input.extend_from_slice(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n");
    // The last line has no newline.
    input.extend_from_slice(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"ping\"}");
    let mut output = Vec::new();
    s.serve(&input[..], &mut output).unwrap();
    let text = String::from_utf8(output).unwrap();
    let answers: Vec<Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(answers.len(), 3);
    assert_eq!(answers[0]["id"], 1);
    assert_eq!(answers[1]["error"]["code"], PARSE_ERROR);
    assert_eq!(answers[2]["id"], 2);
}

#[test]
fn a_message_without_an_id_is_a_notification_and_is_never_answered() {
    let mut s = Server::new();
    for message in [
        json!({}),
        json!({ "foo": 1 }),
        json!({ "method": "ping" }),
        json!({ "jsonrpc": "1.0", "method": "ping" }),
        json!({ "jsonrpc": "2.0" }),
        json!({ "jsonrpc": "2.0", "method": 5 }),
        json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "nope" } }),
        json!({ "jsonrpc": "2.0", "method": "initialize" }),
    ] {
        assert_eq!(s.handle_line(&message.to_string()), None, "{message}");
    }
    // The same inside a batch: only the requests are answered.
    let line = json!([
        { "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "nope" } },
        { "method": "ping" },
        { "jsonrpc": "2.0", "id": 1, "method": "ping" },
    ])
    .to_string();
    let answer: Value = serde_json::from_str(&s.handle_line(&line).unwrap()).unwrap();
    assert_eq!(answer.as_array().unwrap().len(), 1);
    // A present id (even null) makes it a request, answered even when invalid.
    let r = send(&mut s, json!({ "id": null, "method": "ping" }));
    assert_eq!(r["error"]["code"], INVALID_REQUEST);
    // A top-level value that is not an object cannot be a notification.
    assert!(s.handle_line("5").is_some());
}

#[test]
fn a_batch_is_limited() {
    let mut s = Server::new();
    let ping = |i: usize| json!({ "jsonrpc": "2.0", "id": i, "method": "ping" });
    let at_limit: Vec<Value> = (0..MAX_BATCH).map(ping).collect();
    let answer: Value =
        serde_json::from_str(&s.handle_line(&json!(at_limit).to_string()).unwrap()).unwrap();
    assert_eq!(answer.as_array().unwrap().len(), MAX_BATCH);
    let over: Vec<Value> = (0..=MAX_BATCH).map(ping).collect();
    let answer: Value =
        serde_json::from_str(&s.handle_line(&json!(over).to_string()).unwrap()).unwrap();
    assert_eq!(answer["error"]["code"], INVALID_REQUEST);
    assert!(
        answer["error"]["message"]
            .as_str()
            .unwrap()
            .contains("too many")
    );
}

#[test]
fn a_line_that_is_too_long_is_skipped_and_the_server_goes_on() {
    let mut s = Server::new();
    let ping = br#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
    let mut input: Vec<u8> = Vec::new();
    input.extend_from_slice(ping);
    input.push(b'\n');
    input.extend(std::iter::repeat_n(b'x', 1000));
    input.push(b'\n');
    input.extend_from_slice(ping);
    input.push(b'\n');
    // A last line that is too long and has no newline at all.
    input.extend(std::iter::repeat_n(b'y', 500));
    let mut output = Vec::new();
    s.serve_with_limit(&input[..], &mut output, 200).unwrap();
    let text = String::from_utf8(output).unwrap();
    let answers: Vec<Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(answers.len(), 4, "{text}");
    assert_eq!(answers[0]["id"], 1);
    assert_eq!(answers[1]["error"]["code"], INVALID_REQUEST);
    assert!(
        answers[1]["error"]["message"]
            .as_str()
            .unwrap()
            .contains("too long")
    );
    assert_eq!(answers[2]["id"], 1);
    assert_eq!(answers[3]["error"]["code"], INVALID_REQUEST);

    // A line exactly at the limit is read.
    let exact = format!(
        "{}{}",
        r#"{"jsonrpc":"2.0","id":3,"method":"ping"}"#,
        " ".repeat(60)
    );
    let mut output = Vec::new();
    let mut input = exact.clone().into_bytes();
    input.push(b'\n');
    s.serve_with_limit(&input[..], &mut output, exact.len())
        .unwrap();
    assert!(String::from_utf8(output).unwrap().contains("\"id\":3"));
}

#[test]
fn endless_input_without_a_newline_does_not_grow_the_memory() {
    // 40 MiB of one line, limit 1 MiB: the server answers once and reaches the end of the input.
    let mut s = Server::new();
    let endless = std::io::BufReader::new(Read::take(std::io::repeat(b'z'), 40 * 1024 * 1024));
    let mut output = Vec::new();
    s.serve_with_limit(endless, &mut output, 1024 * 1024)
        .unwrap();
    let text = String::from_utf8(output).unwrap();
    assert_eq!(text.lines().count(), 1, "{text}");
    assert!(text.contains("too long"));
}

#[test]
fn a_byte_order_mark_on_the_first_line_is_ignored() {
    let mut s = Server::new();
    let mut input: Vec<u8> = vec![0xef, 0xbb, 0xbf];
    input.extend_from_slice(br#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#);
    input.push(b'\n');
    input.extend_from_slice(&[0xef, 0xbb, 0xbf]);
    input.extend_from_slice(br#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#);
    input.push(b'\n');
    let mut output = Vec::new();
    s.serve(&input[..], &mut output).unwrap();
    let answers: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(answers.len(), 2);
    assert_eq!(answers[0]["result"], json!({}));
    // Only the first line may carry one: a mark later is part of the text, hence a parse error.
    assert_eq!(answers[1]["error"]["code"], PARSE_ERROR);
}

#[test]
fn a_pk2_fit_runs_over_mcp_and_its_initial_estimates_are_refused_readably() {
    let mut s = Server::new();
    // The tool schema lists the two-compartment ids.
    let list = send(
        &mut s,
        json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }),
    );
    let fit_run = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "fit_run")
        .unwrap();
    assert!(
        fit_run["inputSchema"]
            .to_string()
            .contains("pk2.oral_1_lag")
    );
    // A pk2 profile from model_simulate (public oracle parameters), 1 % alternating error.
    let times = [
        0.1, 0.25, 0.5, 1.0, 2.0, 3.0, 4.0, 6.0, 8.0, 12.0, 16.0, 24.0, 36.0, 48.0,
    ];
    let sim = call(
        &mut s,
        2,
        "model_simulate",
        json!({ "model": "pk2.iv_bolus", "dose": 100,
                "params": { "cl": 2, "vc": 10, "q": 4, "vp": 8 }, "times": times }),
    );
    assert_eq!(sim["result"]["isError"], false, "{sim}");
    let conc = sim["result"]["structuredContent"]["conc"]
        .as_array()
        .unwrap();
    let mut csv = String::from("time,conc,dose\n");
    for (i, (t, c)) in times.iter().zip(conc).enumerate() {
        let noise = if i % 2 == 0 { 1.01 } else { 0.99 };
        csv.push_str(&format!("{t},{},100\n", c.as_f64().unwrap() * noise));
    }
    let r = call(
        &mut s,
        3,
        "data_import",
        json!({ "name": "pk2", "csv": csv }),
    );
    let id = r["result"]["structuredContent"]["worksheet"]["id"].clone();
    let fit = call(
        &mut s,
        4,
        "fit_run",
        json!({ "worksheet": id, "model": "pk2.iv_bolus",
                "initial": { "cl": 1.6, "vc": 11, "q": 3.2, "vp": 9 } }),
    );
    assert_jsonrpc(&fit);
    assert_eq!(fit["result"]["isError"], false, "{fit}");
    let ok = &fit["result"]["structuredContent"]["result"]["outcome"]["ok"];
    assert_eq!(ok["status"], "converged", "{ok}");
    assert!(ok["values"]["estimate.cl"].as_f64().is_some());
    // No automatic initial estimates for pk2: a tool error that says what to do.
    let r = call(
        &mut s,
        5,
        "fit_initial_estimates",
        json!({ "worksheet": id, "model": "pk2.iv_bolus" }),
    );
    assert_eq!(r["result"]["isError"], true);
    let text = r["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.starts_with("fit_error: "), "{text}");
    assert!(text.contains("enter initial estimates"), "{text}");
}

/// The `tools/list` entry of `analysis_compare` may add at most this many bytes to the list
/// (T-042: a small tool; every tool is read by the model at the start of every conversation).
const COMPARE_TOOL_BUDGET: usize = 600;

#[test]
fn the_compare_tool_is_small_and_read_only() {
    let mut s = Server::new();
    let r = send(
        &mut s,
        json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }),
    );
    let tools = r["result"]["tools"].as_array().unwrap();
    let tool = tools
        .iter()
        .find(|t| t["name"] == "analysis_compare")
        .expect("analysis_compare is a tool");
    assert_eq!(tool["annotations"]["readOnlyHint"], true);
    assert_eq!(tool["inputSchema"]["required"], json!(["a", "b"]));
    // The entry plus the comma that separates it from the next one.
    let growth = tool.to_string().len() + 1;
    assert!(
        growth < COMPARE_TOOL_BUDGET,
        "analysis_compare adds {growth} bytes to tools/list; the budget is {COMPARE_TOOL_BUDGET}"
    );
    // One line of description.
    let description = tool["description"].as_str().unwrap();
    assert!(!description.contains('\n'), "{description}");
}

#[test]
fn two_analyses_are_compared_over_the_session() {
    let mut s = Server::new();
    let r = call(
        &mut s,
        1,
        "data_import",
        json!({ "name": "w", "csv": "time (h),conc (mg/L),dose (mg)\n0,0,10\n0.5,4,10\n1,5,10\n2,3,10\n4,1,10\n8,0.2,10\n" }),
    );
    let ws = r["result"]["structuredContent"]["worksheet"]["id"].clone();
    let mut ids = Vec::new();
    for (i, method) in ["linear", "lin_up_log_down"].iter().enumerate() {
        let r = call(
            &mut s,
            2 + i as u64,
            "nca_run",
            json!({ "worksheet": ws, "route": "extravascular",
                    "options": { "auc_method": method,
                                 "units": { "time": "h", "concentration": "mg/L", "dose": "mg" } } }),
        );
        assert_eq!(r["result"]["isError"], false, "{r}");
        ids.push(r["result"]["structuredContent"]["id"].clone());
    }
    let r = call(
        &mut s,
        9,
        "analysis_compare",
        json!({ "a": ids[0], "b": ids[1], "parameters": ["auclast", "cmax"] }),
    );
    assert_jsonrpc(&r);
    assert_eq!(r["result"]["isError"], false, "{r}");
    let out = &r["result"]["structuredContent"];
    let rows = out["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    let auc = &rows[0];
    let (a, b) = (auc["a"].as_f64().unwrap(), auc["b"].as_f64().unwrap());
    assert!(b < a, "{auc}");
    assert_eq!(auc["difference"], b - a);
    assert_eq!(auc["relative_percent"], (b - a) / a * 100.0);
    assert_eq!(auc["ratio"], b / a);
    assert_eq!(auc["difference_unit"], "h·mg/L");
    assert_eq!(rows[1]["difference"], 0.0);
    // The text content is the same JSON, and the dotted id works too.
    let text: Value =
        serde_json::from_str(r["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(&text, out);
    let r = call(
        &mut s,
        10,
        "analysis.compare",
        json!({ "a": ids[0], "b": 99 }),
    );
    assert_eq!(r["result"]["isError"], true);
    let text = r["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.starts_with("unknown_analysis: "), "{text}");
    // Nothing was stored by comparing.
    assert_eq!(s.engine().project().analyses().len(), 2);
}
