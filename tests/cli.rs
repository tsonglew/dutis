use serde_json::Value;
use std::fs;
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
fn profile_list_and_show_are_available_without_duti() {
    let list = dutis()
        .env("PATH", "")
        .args(["profile", "list", "--json"])
        .output()
        .unwrap();
    assert!(list.status.success());
    let response: Value = serde_json::from_slice(&list.stdout).unwrap();
    let names = response["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|profile| profile["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(names, ["developer", "designer", "media", "minimal"]);

    let show = dutis()
        .env("PATH", "")
        .args(["profile", "show", "developer", "--json"])
        .output()
        .unwrap();
    assert!(show.status.success());
    let response: Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(response["data"]["name"], "developer");
    assert!(response["data"]["associations"].as_array().unwrap().len() >= 5);
}

#[test]
fn unknown_profile_has_stable_json_error() {
    let output = dutis()
        .args(["profile", "show", "unknown", "--json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["command"], "profile");
    assert_eq!(response["error"]["kind"], "not_found");
}

#[test]
fn watch_remediation_requires_explicit_approval_before_reading_config() {
    let output = dutis()
        .args([
            "watch",
            "missing.toml",
            "--once",
            "--remediate",
            "--requester",
            "test-agent",
            "--json",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["command"], "watch");
    assert_eq!(response["error"]["kind"], "usage");
    assert!(response["error"]["message"]
        .as_str()
        .unwrap()
        .contains("--yes"));
}

#[cfg(target_os = "macos")]
#[test]
fn watch_once_reports_drift_without_invoking_duti_set() {
    use std::os::unix::fs::PermissionsExt;

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "dutis-watch-read-only-{}-{unique}",
        std::process::id()
    ));
    let bin = root.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let duti = bin.join("duti");
    fs::write(
        &duti,
        concat!(
            "#!/bin/sh\n",
            "printf '%s\\n' \"$*\" >> \"$DUTI_CALL_LOG\"\n",
            "if [ \"$1\" = \"-V\" ]; then printf 'test-duti\\n'; exit 0; fi\n",
            "if [ \"$1\" = \"-x\" ]; then printf 'Other\\n/Applications/Other.app\\ncom.example.Other\\n'; exit 0; fi\n",
            "exit 99\n",
        ),
    )
    .unwrap();
    fs::set_permissions(&duti, fs::Permissions::from_mode(0o700)).unwrap();
    let config = root.join("dutis.toml");
    fs::write(
        &config,
        "version = 1\n[associations]\nmd = 'com.apple.TextEdit'\n",
    )
    .unwrap();
    let calls = root.join("duti-calls.log");

    let output = dutis()
        .env("PATH", &bin)
        .env("DUTI_CALL_LOG", &calls)
        .env("DUTIS_STATE_DIR", root.join("state"))
        .args(["watch", config.to_str().unwrap(), "--once", "--json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["data"]["report"]["state"], "drift_detected");
    assert!(response["data"]["remediation"].is_null());
    let calls = fs::read_to_string(&calls).unwrap();
    assert!(calls.lines().any(|line| line == "-V"));
    assert!(calls.lines().any(|line| line == "-x md"));
    assert!(!calls.lines().any(|line| line.starts_with("-s ")));
    fs::remove_dir_all(root).unwrap();
}

#[cfg(target_os = "macos")]
#[test]
fn opted_in_watch_remediation_uses_snapshot_audit_and_verification() {
    use std::os::unix::fs::PermissionsExt;

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "dutis-watch-remediation-{}-{unique}",
        std::process::id()
    ));
    let bin = root.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let duti = bin.join("duti");
    fs::write(
        &duti,
        concat!(
            "#!/bin/sh\n",
            "printf '%s\\n' \"$*\" >> \"$DUTI_CALL_LOG\"\n",
            "if [ \"$1\" = \"-V\" ]; then printf 'test-duti\\n'; exit 0; fi\n",
            "if [ \"$1\" = \"-s\" ]; then : > \"$DUTI_FAKE_STATE\"; exit 0; fi\n",
            "if [ \"$1\" = \"-x\" ] && [ -f \"$DUTI_FAKE_STATE\" ]; then printf 'TextEdit\\n/System/Applications/TextEdit.app\\ncom.apple.TextEdit\\n'; exit 0; fi\n",
            "if [ \"$1\" = \"-x\" ]; then printf 'Other\\n/Applications/Other.app\\ncom.example.Other\\n'; exit 0; fi\n",
            "exit 99\n",
        ),
    )
    .unwrap();
    fs::set_permissions(&duti, fs::Permissions::from_mode(0o700)).unwrap();
    let config = root.join("dutis.toml");
    fs::write(
        &config,
        "version = 1\n[associations]\nmd = 'com.apple.TextEdit'\n",
    )
    .unwrap();
    let calls = root.join("duti-calls.log");
    let fake_state = root.join("applied");
    let state = root.join("state");

    let output = dutis()
        .env("PATH", &bin)
        .env("DUTI_CALL_LOG", &calls)
        .env("DUTI_FAKE_STATE", &fake_state)
        .env("DUTIS_STATE_DIR", &state)
        .args([
            "watch",
            config.to_str().unwrap(),
            "--once",
            "--remediate",
            "--yes",
            "--requester",
            "integration-test",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["data"]["remediation"]["status"], "succeeded");
    assert_eq!(response["data"]["remediation"]["mutation"]["applied"], 1);
    assert!(response["data"]["remediation"]["mutation"]["safety_snapshot_id"].is_string());
    assert!(response["data"]["remediation"]["mutation"]["audit_id"].is_string());
    let calls = fs::read_to_string(&calls).unwrap();
    assert!(calls
        .lines()
        .any(|line| line == "-s com.apple.TextEdit .md all"));
    assert_eq!(fs::read_dir(state.join("snapshots")).unwrap().count(), 1);
    assert_eq!(fs::read_dir(state.join("audit")).unwrap().count(), 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn launch_agent_status_is_read_only_for_an_isolated_directory() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "dutis-launch-agent-{}-{unique}",
        std::process::id()
    ));
    let output = dutis()
        .env("DUTIS_LAUNCH_AGENT_DIR", &directory)
        .args(["launch-agent", "status", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["command"], "launch-agent");
    assert_eq!(response["data"]["installed"], false);
    assert_eq!(response["data"]["loaded"], false);
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
        "{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"tools/call\",\"params\":{\"name\":\"dutis_history\",\"arguments\":{}}}\n",
        "{\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"tools/call\",\"params\":{\"name\":\"dutis_policy\",\"arguments\":{}}}\n",
        "{\"jsonrpc\":\"2.0\",\"id\":5,\"method\":\"tools/call\",\"params\":{\"name\":\"dutis_profile\",\"arguments\":{\"profile\":\"minimal\"}}}\n"
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
    assert_eq!(responses.len(), 5);
    assert_eq!(responses[0]["result"]["protocolVersion"], "2024-11-05");
    let tools = responses[1]["result"]["tools"].as_array().unwrap();
    assert!(tools.iter().any(|tool| tool["name"] == "dutis_diff"));
    assert!(tools.iter().any(|tool| tool["name"] == "dutis_policy"));
    assert!(tools.iter().any(|tool| tool["name"] == "dutis_profiles"));
    assert!(tools.iter().any(|tool| tool["name"] == "dutis_recommend"));
    assert!(tools.iter().any(|tool| tool["name"] == "dutis_drift"));
    assert!(!tools.iter().any(|tool| tool["name"] == "dutis_apply"));
    assert_eq!(
        responses[3]["result"]["structuredContent"]["data"]["approval_mode"],
        "explicit"
    );
    assert_eq!(
        responses[4]["result"]["structuredContent"]["data"]["name"],
        "minimal"
    );

    let audit = String::from_utf8(output.stderr)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(audit.len(), 3);
    assert_eq!(audit[0]["schema_version"], 1);
    assert_eq!(audit[0]["tool"], "dutis_history");
    assert_eq!(audit[1]["tool"], "dutis_policy");
    assert_eq!(audit[2]["tool"], "dutis_profile");
    assert!(audit.iter().all(|event| event["access"] == "read"));
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

#[test]
fn policy_show_redacts_token_hash_and_audit_starts_empty() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let state = std::env::temp_dir().join(format!(
        "dutis-policy-integration-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&state).unwrap();
    let policy_path = state.join("policy.toml");
    let secret_hash = "a".repeat(64);
    fs::write(
        &policy_path,
        format!("version = 1\napproval_mode = 'token'\napproval_token_sha256 = '{secret_hash}'\n"),
    )
    .unwrap();

    let policy_output = dutis()
        .env("DUTIS_STATE_DIR", &state)
        .args(["policy", "show", "--json"])
        .output()
        .unwrap();
    assert!(policy_output.status.success());
    let policy: Value = serde_json::from_slice(&policy_output.stdout).unwrap();
    assert_eq!(policy["data"]["approval_mode"], "token");
    assert_eq!(policy["data"]["approval_token_configured"], true);
    assert!(!String::from_utf8(policy_output.stdout)
        .unwrap()
        .contains(&secret_hash));

    let audit_output = dutis()
        .env("DUTIS_STATE_DIR", &state)
        .args(["audit", "--json"])
        .output()
        .unwrap();
    assert!(audit_output.status.success());
    let audit: Value = serde_json::from_slice(&audit_output.stdout).unwrap();
    assert_eq!(audit["data"].as_array().unwrap().len(), 0);
    fs::remove_dir_all(state).unwrap();
}
