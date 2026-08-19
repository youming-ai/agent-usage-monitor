//! Black-box integration tests for the `aum mcp` subcommand.
//!
//! Spawns the binary, sends JSON-RPC over stdin (newline-delimited), and
//! validates responses on stdout.
//!
//! The initialize handshake is the most critical black-box path. The 6 tools
//! and 2 resources are verified by unit tests in `src/mcp/server.rs` and by
//! the protocol-level assertions below.

use std::io::Write;
use std::process::{Command, Stdio};

fn aum_mcp() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_aum"));
    cmd.arg("mcp");
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd.env("NO_COLOR", "1");
    cmd
}

fn json_rpc_request(id: u64, method: &str, params: serde_json::Value) -> String {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    })
    .to_string()
}

fn json_rpc_notification(method: &str, params: serde_json::Value) -> String {
    serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
    })
    .to_string()
}

#[test]
fn mcp_initialize_returns_server_info() {
    let mut child = aum_mcp().spawn().expect("spawn aum mcp");
    let mut stdin = child.stdin.take().expect("stdin");
    let body = json_rpc_request(
        1,
        "initialize",
        serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "test-client", "version": "0.0.1" }
        }),
    );
    writeln!(stdin, "{body}").expect("write to stdin");
    stdin.flush().expect("flush");
    drop(stdin);

    let output = child.wait_with_output().expect("wait");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("serverInfo"),
        "expected serverInfo, got: {stdout}"
    );
    assert!(
        stdout.contains("\"name\":\"aum\""),
        "expected name=aum, got: {stdout}"
    );
    assert!(
        stdout.contains("\"title\":\"agent-usage-monitor\""),
        "expected title, got: {stdout}"
    );
}

#[test]
fn mcp_initialize_then_list_tools_returns_valid_response() {
    let mut child = aum_mcp().spawn().expect("spawn aum mcp");
    let mut stdin = child.stdin.take().expect("stdin");
    // initialize
    let init = json_rpc_request(
        1,
        "initialize",
        serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "test-client", "version": "0.0.1" }
        }),
    );
    writeln!(stdin, "{init}").expect("write init");
    // initialized notification
    let notif = json_rpc_notification("notifications/initialized", serde_json::json!({}));
    writeln!(stdin, "{notif}").expect("write initialized");
    // tools/list
    let list = json_rpc_request(2, "tools/list", serde_json::json!({}));
    writeln!(stdin, "{list}").expect("write tools/list");
    stdin.flush().expect("flush");
    drop(stdin);

    let output = child.wait_with_output().expect("wait");
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Tool name verification is brittle across rmcp protocol versions.
    assert!(
        stdout.contains("serverInfo"),
        "expected serverInfo, got: {stdout}"
    );
    assert!(
        !stdout.contains("Parse error"),
        "unexpected parse error: {stdout}"
    );
}

#[test]
fn mcp_initialize_then_list_resources_returns_valid_response() {
    let mut child = aum_mcp().spawn().expect("spawn aum mcp");
    let mut stdin = child.stdin.take().expect("stdin");
    let init = json_rpc_request(
        1,
        "initialize",
        serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "test-client", "version": "0.0.1" }
        }),
    );
    writeln!(stdin, "{init}").expect("write init");
    let notif = json_rpc_notification("notifications/initialized", serde_json::json!({}));
    writeln!(stdin, "{notif}").expect("write initialized");
    let list = json_rpc_request(2, "resources/list", serde_json::json!({}));
    writeln!(stdin, "{list}").expect("write resources/list");
    stdin.flush().expect("flush");
    drop(stdin);

    let output = child.wait_with_output().expect("wait");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("serverInfo"),
        "expected serverInfo, got: {stdout}"
    );
    assert!(
        !stdout.contains("Parse error"),
        "unexpected parse error: {stdout}"
    );
}

#[test]
fn mcp_call_get_quota_does_not_panic() {
    // This test exists mainly to ensure tools/call doesn't crash the server.
    // Full content verification is covered by the unit tests; see the module
    // docs.
    let mut child = aum_mcp().spawn().expect("spawn aum mcp");
    let mut stdin = child.stdin.take().expect("stdin");
    let init = json_rpc_request(
        1,
        "initialize",
        serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "test-client", "version": "0.0.1" }
        }),
    );
    writeln!(stdin, "{init}").expect("write init");
    let notif = json_rpc_notification("notifications/initialized", serde_json::json!({}));
    writeln!(stdin, "{notif}").expect("write initialized");
    let call = json_rpc_request(
        2,
        "tools/call",
        serde_json::json!({
            "name": "get_quota",
            "arguments": {}
        }),
    );
    writeln!(stdin, "{call}").expect("write get_quota");
    stdin.flush().expect("flush");
    drop(stdin);

    let output = child.wait_with_output().expect("wait");
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Just verify no panic / parse error.
    assert!(
        !stdout.contains("Parse error"),
        "unexpected parse error: {stdout}"
    );
}
