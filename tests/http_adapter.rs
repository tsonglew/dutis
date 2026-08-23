#![cfg(unix)]

use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

const ENDPOINT: &str = "https://hooks.example.invalid/private/path?key=endpoint-secret";
const TOKEN: &str = "adapter-bearer-secret";

fn adapter() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dutis-event-http"))
}

fn dutis() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dutis"))
}

fn temp_root(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "dutis-http-adapter-{label}-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir(&root).unwrap();
    root
}

fn write_executable(path: &Path, contents: &str) {
    fs::write(path, contents).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

fn event() -> Value {
    json!({
        "schema_version": 1,
        "id": "event-123",
        "emitted_at": "2026-08-23T00:00:00Z",
        "event_type": "mutation.completed",
        "source": "governance",
        "payload": {"audit_id": "audit-123"}
    })
}

#[test]
fn forwards_event_without_exposing_transport_secrets_in_arguments_or_environment() {
    let root = temp_root("success");
    let fake_curl = root.join("curl");
    let captured_arguments = root.join("arguments");
    let captured_config = root.join("config");
    let captured_body = root.join("body");
    let captured_environment = root.join("environment");
    write_executable(
        &fake_curl,
        concat!(
            "#!/bin/sh\n",
            "printf '%s\\n' \"$@\" > \"$DUTIS_CAPTURE_ARGUMENTS\"\n",
            "/bin/cp \"$3\" \"$DUTIS_CAPTURE_CONFIG\"\n",
            "body_path=$(/usr/bin/sed -n 's/^data-binary = \"@\\(.*\\)\"$/\\1/p' \"$3\")\n",
            "/bin/cp \"$body_path\" \"$DUTIS_CAPTURE_BODY\"\n",
            "printf '%s|%s' \"${DUTIS_HTTP_ENDPOINT-unset}\" \"${DUTIS_HTTP_BEARER_TOKEN-unset}\" > \"$DUTIS_CAPTURE_ENVIRONMENT\"\n",
        ),
    );
    let input = serde_json::to_vec(&event()).unwrap();
    let mut child = adapter()
        .env("DUTIS_HTTP_ENDPOINT", ENDPOINT)
        .env("DUTIS_HTTP_BEARER_TOKEN", TOKEN)
        .env("DUTIS_HTTP_CURL", &fake_curl)
        .env("DUTIS_CAPTURE_ARGUMENTS", &captured_arguments)
        .env("DUTIS_CAPTURE_CONFIG", &captured_config)
        .env("DUTIS_CAPTURE_BODY", &captured_body)
        .env("DUTIS_CAPTURE_ENVIRONMENT", &captured_environment)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&input).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());

    let arguments = fs::read_to_string(&captured_arguments).unwrap();
    assert!(arguments.starts_with("--disable\n--config\n"));
    assert!(!arguments.contains(ENDPOINT));
    assert!(!arguments.contains(TOKEN));
    let config_path = PathBuf::from(arguments.lines().nth(2).unwrap());
    assert!(!config_path.exists());

    let config = fs::read_to_string(&captured_config).unwrap();
    assert!(config.contains(&format!("url = \"{ENDPOINT}\"")));
    assert!(config.contains(&format!("Authorization: Bearer {TOKEN}")));
    assert!(config.contains("X-Dutis-Event-Id: event-123"));
    assert!(config.contains("Idempotency-Key: event-123"));
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(&captured_body).unwrap()).unwrap(),
        event()
    );
    assert_eq!(
        fs::read_to_string(&captured_environment).unwrap(),
        "unset|unset"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn check_is_sanitized_and_delivery_failure_does_not_echo_secrets() {
    let root = temp_root("sanitized");
    let fake_curl = root.join("curl");
    write_executable(
        &fake_curl,
        concat!(
            "#!/bin/sh\n",
            "printf 'https://leaked.example/private adapter-bearer-secret\\n' >&2\n",
            "exit 22\n",
        ),
    );
    let check = adapter()
        .env("DUTIS_HTTP_ENDPOINT", ENDPOINT)
        .env("DUTIS_HTTP_BEARER_TOKEN", TOKEN)
        .env("DUTIS_HTTP_CURL", &fake_curl)
        .args(["--check", "--json"])
        .output()
        .unwrap();
    assert!(check.status.success());
    let status: Value = serde_json::from_slice(&check.stdout).unwrap();
    assert_eq!(status["data"]["transport"], "https");
    assert_eq!(status["data"]["authentication"], "bearer");
    let rendered = String::from_utf8(check.stdout).unwrap();
    assert!(!rendered.contains(ENDPOINT));
    assert!(!rendered.contains(TOKEN));

    let mut child = adapter()
        .env("DUTIS_HTTP_ENDPOINT", ENDPOINT)
        .env("DUTIS_HTTP_BEARER_TOKEN", TOKEN)
        .env("DUTIS_HTTP_CURL", &fake_curl)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&event()).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("transport status"));
    assert!(!stderr.contains(ENDPOINT));
    assert!(!stderr.contains(TOKEN));
    assert!(!stderr.contains("leaked.example"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rejects_plain_http_before_starting_transport() {
    let root = temp_root("https-only");
    let fake_curl = root.join("curl");
    let marker = root.join("called");
    write_executable(&fake_curl, "#!/bin/sh\ntouch \"$DUTIS_CALLED\"\n");
    let output = adapter()
        .env("DUTIS_HTTP_ENDPOINT", "http://example.invalid/hook")
        .env("DUTIS_HTTP_CURL", &fake_curl)
        .env("DUTIS_CALLED", &marker)
        .args(["--check"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!marker.exists());
    assert!(String::from_utf8(output.stderr)
        .unwrap()
        .contains("absolute HTTPS URL"));
    fs::remove_dir_all(root).unwrap();
}

#[cfg(target_os = "macos")]
#[test]
fn watcher_event_flows_through_external_adapter_to_transport() {
    let root = temp_root("full-chain");
    let bin = root.join("bin");
    fs::create_dir(&bin).unwrap();
    let fake_duti = bin.join("duti");
    write_executable(
        &fake_duti,
        concat!(
            "#!/bin/sh\n",
            "if [ \"$1\" = \"-V\" ]; then printf 'test-duti\\n'; exit 0; fi\n",
            "if [ \"$1\" = \"-x\" ]; then printf 'Other\\n/Applications/Other.app\\ncom.example.Other\\n'; exit 0; fi\n",
            "exit 99\n",
        ),
    );
    let fake_curl = root.join("curl");
    write_executable(
        &fake_curl,
        concat!(
            "#!/bin/sh\n",
            "body_path=$(/usr/bin/sed -n 's/^data-binary = \"@\\(.*\\)\"$/\\1/p' \"$3\")\n",
            "/bin/cp \"$body_path\" \"$DUTIS_CAPTURE_BODY\"\n",
        ),
    );
    let config = root.join("dutis.toml");
    fs::write(
        &config,
        "version = 1\n[associations]\nmd = 'com.apple.TextEdit'\n",
    )
    .unwrap();
    let captured_body = root.join("body");

    let output = dutis()
        .env("PATH", &bin)
        .env("DUTIS_STATE_DIR", root.join("state"))
        .env("DUTIS_HTTP_ENDPOINT", ENDPOINT)
        .env("DUTIS_HTTP_BEARER_TOKEN", TOKEN)
        .env("DUTIS_HTTP_CURL", &fake_curl)
        .env("DUTIS_CAPTURE_BODY", &captured_body)
        .args([
            "--event-command",
            env!("CARGO_BIN_EXE_dutis-event-http"),
            "watch",
            config.to_str().unwrap(),
            "--once",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let event: Value = serde_json::from_slice(&fs::read(captured_body).unwrap()).unwrap();
    assert_eq!(event["event_type"], "drift.checked");
    assert_eq!(event["source"], "watcher");
    assert_eq!(event["payload"]["state"], "drift_detected");
    fs::remove_dir_all(root).unwrap();
}

#[cfg(target_os = "macos")]
#[test]
fn failed_http_delivery_is_durably_queued_and_replayed() {
    let root = temp_root("outbox-replay");
    let bin = root.join("bin");
    fs::create_dir(&bin).unwrap();
    let fake_duti = bin.join("duti");
    write_executable(
        &fake_duti,
        concat!(
            "#!/bin/sh\n",
            "if [ \"$1\" = \"-V\" ]; then printf 'test-duti\\n'; exit 0; fi\n",
            "if [ \"$1\" = \"-x\" ]; then printf 'Other\\n/Applications/Other.app\\ncom.example.Other\\n'; exit 0; fi\n",
            "exit 99\n",
        ),
    );
    let fake_curl = root.join("curl");
    write_executable(
        &fake_curl,
        concat!(
            "#!/bin/sh\n",
            "if [ -e \"$DUTIS_FAIL_MARKER\" ]; then exit 22; fi\n",
            "body_path=$(/usr/bin/sed -n 's/^data-binary = \"@\\(.*\\)\"$/\\1/p' \"$3\")\n",
            "/bin/cp \"$body_path\" \"$DUTIS_CAPTURE_BODY\"\n",
        ),
    );
    let config = root.join("dutis.toml");
    fs::write(
        &config,
        "version = 1\n[associations]\nmd = 'com.apple.TextEdit'\n",
    )
    .unwrap();
    let state = root.join("state");
    let fail_marker = root.join("fail");
    fs::write(&fail_marker, b"").unwrap();
    let captured_body = root.join("body");

    let watch = dutis()
        .env("PATH", &bin)
        .env("DUTIS_STATE_DIR", &state)
        .env("DUTIS_HTTP_ENDPOINT", ENDPOINT)
        .env("DUTIS_HTTP_BEARER_TOKEN", TOKEN)
        .env("DUTIS_HTTP_CURL", &fake_curl)
        .env("DUTIS_FAIL_MARKER", &fail_marker)
        .env("DUTIS_CAPTURE_BODY", &captured_body)
        .args([
            "--event-command",
            env!("CARGO_BIN_EXE_dutis-event-http"),
            "watch",
            config.to_str().unwrap(),
            "--once",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(watch.status.success());
    assert!(String::from_utf8_lossy(&watch.stderr).contains("queued for replay"));

    let pending = dutis()
        .env("DUTIS_STATE_DIR", &state)
        .args(["events", "pending", "--json"])
        .output()
        .unwrap();
    assert!(pending.status.success());
    let response: Value = serde_json::from_slice(&pending.stdout).unwrap();
    assert_eq!(response["data"].as_array().unwrap().len(), 1);
    let event_id = response["data"][0]["id"].as_str().unwrap().to_owned();

    fs::remove_file(&fail_marker).unwrap();
    let replay = dutis()
        .env("DUTIS_STATE_DIR", &state)
        .env("DUTIS_HTTP_ENDPOINT", ENDPOINT)
        .env("DUTIS_HTTP_BEARER_TOKEN", TOKEN)
        .env("DUTIS_HTTP_CURL", &fake_curl)
        .env("DUTIS_FAIL_MARKER", &fail_marker)
        .env("DUTIS_CAPTURE_BODY", &captured_body)
        .args([
            "--event-command",
            env!("CARGO_BIN_EXE_dutis-event-http"),
            "events",
            "replay",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        replay.status.success(),
        "{}",
        String::from_utf8_lossy(&replay.stderr)
    );
    let response: Value = serde_json::from_slice(&replay.stdout).unwrap();
    assert_eq!(response["data"]["delivered"], 1);
    assert_eq!(response["data"]["remaining"], 0);
    let delivered: Value = serde_json::from_slice(&fs::read(&captured_body).unwrap()).unwrap();
    assert_eq!(delivered["id"], event_id);
    fs::remove_dir_all(root).unwrap();
}
