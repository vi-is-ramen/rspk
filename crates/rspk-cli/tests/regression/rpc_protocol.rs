//! Regression: JSON-RPC 2.0 protocol contract.
//!
//! These tests spawn the `pk rpc` binary and verify that the wire
//! protocol remains stable. Any change to response shape, error
//! codes, or batch handling will break these tests.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

/// Sends a single JSON-RPC request to `pk rpc` and returns the
/// parsed response.
fn rpc_call(request: &Value) -> Value
{
    let bin = env!("CARGO_BIN_EXE_pk");
    let mut child = Command::new(bin)
        .arg("rpc")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn pk rpc");

    {
        let stdin = child.stdin.as_mut().unwrap();
        writeln!(stdin, "{}", request).unwrap();
    }

    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();

    // Kill the child so it doesn't hang
    let _ = child.kill();
    let _ = child.wait();

    serde_json::from_str(&line).expect("response must be valid JSON")
}

/// Sends a batch (array) request and returns the parsed array.
fn rpc_batch(requests: &[Value]) -> Vec<Value>
{
    let bin = env!("CARGO_BIN_EXE_pk");
    let mut child = Command::new(bin)
        .arg("rpc")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn pk rpc");

    {
        let stdin = child.stdin.as_mut().unwrap();
        let batch = serde_json::to_string(requests).unwrap();
        writeln!(stdin, "{batch}").unwrap();
    }

    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();

    let _ = child.kill();
    let _ = child.wait();

    serde_json::from_str(&line).expect("batch response must be valid JSON")
}

// ── system.listMethods ──────────────────────────────────────────

#[test]
fn list_methods_returns_known_set()
{
    let resp = rpc_call(&json!({
        "jsonrpc": "2.0",
        "method": "system.listMethods",
        "id": 1
    }));

    assert_eq!(resp["jsonrpc"], "2.0");
    assert_eq!(resp["id"], 1);
    assert!(resp["result"]["methods"].is_array());

    let methods: Vec<&str> = resp["result"]["methods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();

    // These methods must always exist
    for expected in &[
        "inventory", "installed", "outdated", "search",
        "resolve", "install", "upgrade", "uninstall",
        "sync", "cleanup", "satisfy", "sbom",
        "system.listMethods", "system.describe",
    ] {
        assert!(
            methods.contains(expected),
            "method '{expected}' missing from {methods:?}"
        );
    }
}

// ── Batch request contract ──────────────────────────────────────

#[test]
fn batch_returns_array_with_matching_ids()
{
    let responses = rpc_batch(&[
        json!({"jsonrpc": "2.0", "method": "system.listMethods", "id": 1}),
        json!({"jsonrpc": "2.0", "method": "inventory", "id": 2}),
    ]);

    assert_eq!(responses.len(), 2, "batch must return exactly 2 responses");
    assert_eq!(responses[0]["id"], 1);
    assert_eq!(responses[1]["id"], 2);
    assert!(responses[0]["result"].is_object());
    assert!(responses[1]["result"].is_object());
}

// ── Error codes ─────────────────────────────────────────────────

#[test]
fn unknown_method_returns_32601()
{
    let resp = rpc_call(&json!({
        "jsonrpc": "2.0",
        "method": "nonexistent_method",
        "id": 99
    }));

    assert_eq!(resp["error"]["code"], -32601);
    assert_eq!(resp["id"], 99);
    assert!(resp["result"].is_null() || resp.get("result").is_none());
}

#[test]
fn invalid_json_returns_32700()
{
    let bin = env!("CARGO_BIN_EXE_pk");
    let mut child = Command::new(bin)
        .arg("rpc")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    {
        let stdin = child.stdin.as_mut().unwrap();
        writeln!(stdin, "this is not json at all").unwrap();
    }

    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();

    let _ = child.kill();
    let _ = child.wait();

    let resp: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(resp["error"]["code"], -32700);
}

#[test]
fn invalid_request_returns_32600()
{
    // Missing "method" field
    let resp = rpc_call(&json!({
        "jsonrpc": "2.0",
        "id": 42
    }));

    assert_eq!(resp["error"]["code"], -32600);
}

// ── Notification (no id) produces no response ───────────────────

#[test]
fn notification_produces_no_output()
{
    let bin = env!("CARGO_BIN_EXE_pk");
    let mut child = Command::new(bin)
        .arg("rpc")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    {
        let stdin = child.stdin.as_mut().unwrap();
        // Notification: no "id" field
        writeln!(stdin, "{{\"jsonrpc\":\"2.0\",\"method\":\"inventory\"}}").unwrap();
        // Follow with a real request so we can read something
        writeln!(stdin, "{{\"jsonrpc\":\"2.0\",\"method\":\"system.listMethods\",\"id\":1}}").unwrap();
    }

    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();

    let _ = child.kill();
    let _ = child.wait();

    // The first line we read must be the response to id=1,
    // NOT a response to the notification.
    let resp: Value = serde_json::from_str(&(line+"\n")).unwrap();
    assert_eq!(resp["id"], 1, "notification must not produce a response");
}

// ── system.describe contract ────────────────────────────────────

#[test]
fn describe_returns_schema_for_known_method()
{
    let resp = rpc_call(&json!({
        "jsonrpc": "2.0",
        "method": "system.describe",
        "params": {"method": "install"},
        "id": 5
    }));

    assert_eq!(resp["id"], 5);
    assert_eq!(resp["result"]["method"], "install");
    assert!(resp["result"]["params_schema"].is_string());
    assert!(resp["result"]["result_schema"].is_string());
}

#[test]
fn describe_unknown_method_returns_error()
{
    let resp = rpc_call(&json!({
        "jsonrpc": "2.0",
        "method": "system.describe",
        "params": {"method": "does_not_exist"},
        "id": 6
    }));

    assert!(resp["error"].is_object());
    assert_eq!(resp["error"]["code"], -32601);
}
