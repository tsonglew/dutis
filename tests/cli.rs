use serde_json::Value;
use std::process::Command;
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
