use serde_json::Value;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn dutis() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dutis"))
}

#[test]
fn reports_package_version() {
    let output = dutis().arg("--version").output().unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        format!("dutis {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn json_usage_errors_have_a_stable_envelope_and_exit_code() {
    let output = dutis()
        .args(["query", "../../etc/passwd", "--json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stderr.is_empty());

    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["api_version"], "1");
    assert_eq!(response["command"], "query");
    assert_eq!(response["error"]["code"], 2);
    assert_eq!(response["error"]["kind"], "usage");
}

#[test]
fn set_requires_confirmation_before_scanning_or_mutating() {
    let output = dutis()
        .args(["set", "md", "com.example.Editor", "--json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));

    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["command"], "set");
    assert_eq!(response["error"]["kind"], "usage");
}

#[test]
fn apply_requires_a_reviewed_digest_before_reading_configuration() {
    let output = dutis()
        .args(["apply", "missing.toml", "--yes", "--json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stderr.is_empty());

    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["command"], "apply");
    assert_eq!(response["error"]["kind"], "usage");
    assert!(response["error"]["message"]
        .as_str()
        .unwrap()
        .contains("--plan-digest"));
}

#[test]
fn rollback_requires_confirmation_before_reading_snapshot_storage() {
    let output = dutis()
        .args(["rollback", "missing-snapshot", "--json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stderr.is_empty());

    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["command"], "rollback");
    assert_eq!(response["error"]["kind"], "usage");
}

#[test]
fn history_is_empty_for_an_isolated_state_directory() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let state =
        std::env::temp_dir().join(format!("dutis-cli-history-{}-{unique}", std::process::id()));
    let output = dutis()
        .env("DUTIS_STATE_DIR", state)
        .args(["history", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["command"], "history");
    assert_eq!(response["data"].as_array().unwrap().len(), 0);
}

#[test]
fn mcp_stdio_initializes_and_advertises_read_only_tools() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let state = std::env::temp_dir().join(format!(
        "dutis-mcp-integration-{}-{unique}",
        std::process::id()
    ));
    let mut child = dutis()
        .arg("mcp")
        .env("DUTIS_STATE_DIR", state)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let input = concat!(
        "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2024-11-05\"}}\n",
        "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n",
        "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\",\"params\":{}}\n",
        "{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"tools/call\",\"params\":{\"name\":\"dutis_history\",\"arguments\":{}}}\n"
    );
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());

    let responses = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(responses.len(), 3);
    assert_eq!(responses[0]["result"]["protocolVersion"], "2024-11-05");
    let tools = responses[1]["result"]["tools"].as_array().unwrap();
    assert!(tools.iter().any(|tool| tool["name"] == "dutis_diff"));
    assert!(!tools.iter().any(|tool| tool["name"] == "dutis_apply"));

    let audit: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(audit["schema_version"], 1);
    assert_eq!(audit["tool"], "dutis_history");
    assert_eq!(audit["access"], "read");
}

#[test]
fn mcp_write_mode_requires_a_server_side_approval_token() {
    let output = dutis()
        .env_remove("DUTIS_MCP_APPROVAL_TOKEN")
        .args(["mcp", "--allow-writes"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8(output.stderr)
        .unwrap()
        .contains("DUTIS_MCP_APPROVAL_TOKEN"));
}
