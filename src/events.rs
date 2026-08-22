use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

pub const EVENT_SCHEMA_VERSION: u32 = 1;
pub const EVENT_LOG_ENV: &str = "DUTIS_EVENT_LOG";
pub const EVENT_COMMAND_ENV: &str = "DUTIS_EVENT_COMMAND";

static EVENT_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub enum EventType {
    #[serde(rename = "drift.checked")]
    DriftChecked,
    #[serde(rename = "mutation.pending")]
    MutationPending,
    #[serde(rename = "mutation.denied")]
    MutationDenied,
    #[serde(rename = "mutation.failed")]
    MutationFailed,
    #[serde(rename = "mutation.completed")]
    MutationCompleted,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventSource {
    Watcher,
    Mcp,
    Governance,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub schema_version: u32,
    pub id: String,
    pub emitted_at: String,
    pub event_type: EventType,
    pub source: EventSource,
    pub payload: Value,
}

impl EventEnvelope {
    pub fn new<T: Serialize>(
        event_type: EventType,
        source: EventSource,
        payload: &T,
    ) -> Result<Self> {
        let now = OffsetDateTime::now_utc();
        let sequence = EVENT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        Ok(Self {
            schema_version: EVENT_SCHEMA_VERSION,
            id: format!(
                "{}-{}-{sequence}",
                now.unix_timestamp_nanos(),
                std::process::id()
            ),
            emitted_at: now
                .format(&Rfc3339)
                .context("failed to format event timestamp")?,
            event_type,
            source,
            payload: serde_json::to_value(payload).context("failed to serialize event payload")?,
        })
    }
}

#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct EventDispatcher {
    log: Option<PathBuf>,
    command: Option<PathBuf>,
}

impl EventDispatcher {
    pub fn from_environment() -> Result<Self> {
        let log = std::env::var_os(EVENT_LOG_ENV)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);
        let command = std::env::var_os(EVENT_COMMAND_ENV)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);
        Self::new(log, command)
    }

    pub fn new(log: Option<PathBuf>, command: Option<PathBuf>) -> Result<Self> {
        let log = log.map(absolute_path).transpose()?;
        let command = command.map(absolute_path).transpose()?;
        if let Some(command) = &command {
            validate_command(command)?;
        }
        Ok(Self { log, command })
    }

    pub fn is_enabled(&self) -> bool {
        self.log.is_some() || self.command.is_some()
    }

    pub fn log(&self) -> Option<&Path> {
        self.log.as_deref()
    }

    pub fn command(&self) -> Option<&Path> {
        self.command.as_deref()
    }

    pub fn emit<T: Serialize>(
        &self,
        event_type: EventType,
        source: EventSource,
        payload: &T,
    ) -> Result<Option<EventEnvelope>> {
        if !self.is_enabled() {
            return Ok(None);
        }
        let event = EventEnvelope::new(event_type, source, payload)?;
        let mut encoded = serde_json::to_vec(&event).context("failed to encode event")?;
        encoded.push(b'\n');
        let mut failures = Vec::new();
        if let Some(path) = &self.log {
            if let Err(error) = append_json_line(path, &encoded) {
                failures.push(format!("JSONL sink {}: {error:#}", path.display()));
            }
        }
        if let Some(command) = &self.command {
            if let Err(error) = run_event_command(command, &event, &encoded) {
                failures.push(format!("command sink {}: {error:#}", command.display()));
            }
        }
        if failures.is_empty() {
            Ok(Some(event))
        } else {
            bail!(failures.join("; "))
        }
    }
}

pub fn configure_process_sinks(
    log_override: Option<&Path>,
    command_override: Option<&Path>,
) -> Result<EventDispatcher> {
    if let Some(path) = log_override {
        let path = absolute_path(path.to_path_buf())?;
        std::env::set_var(EVENT_LOG_ENV, &path);
    }
    if let Some(path) = command_override {
        let path = absolute_path(path.to_path_buf())?;
        validate_command(&path)?;
        std::env::set_var(EVENT_COMMAND_ENV, &path);
    }
    let dispatcher = EventDispatcher::from_environment()?;
    if let Some(path) = dispatcher.log() {
        std::env::set_var(EVENT_LOG_ENV, path);
    }
    if let Some(path) = dispatcher.command() {
        std::env::set_var(EVENT_COMMAND_ENV, path);
    }
    Ok(dispatcher)
}

pub fn emit_best_effort<T: Serialize>(event_type: EventType, source: EventSource, payload: &T) {
    let result = EventDispatcher::from_environment()
        .and_then(|dispatcher| dispatcher.emit(event_type, source, payload).map(|_| ()));
    if let Err(error) = result {
        eprintln!("Warning: failed to deliver Dutis event: {error:#}");
    }
}

fn absolute_path(path: PathBuf) -> Result<PathBuf> {
    if path.as_os_str().is_empty() {
        bail!("event sink path cannot be empty");
    }
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(std::env::current_dir()
            .context("failed to resolve the current directory")?
            .join(path))
    }
}

fn validate_command(path: &Path) -> Result<()> {
    if !path.is_absolute() || !path.is_file() {
        bail!(
            "event command must be an existing absolute file: {}",
            path.display()
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if fs::metadata(path)?.permissions().mode() & 0o111 == 0 {
            bail!("event command is not executable: {}", path.display());
        }
    }
    Ok(())
}

fn append_json_line(path: &Path, encoded: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        create_private_directories(parent)?;
    }
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    file.write_all(encoded)
        .with_context(|| format!("failed to append {}", path.display()))?;
    file.flush()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn create_private_directories(path: &Path) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    let mut missing = Vec::new();
    let mut current = Some(path);
    while let Some(directory) = current {
        if directory.exists() {
            break;
        }
        missing.push(directory);
        current = directory.parent();
    }
    fs::create_dir_all(path).with_context(|| format!("failed to create {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for directory in missing.into_iter().rev() {
            fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
        }
    }
    Ok(())
}

fn run_event_command(command: &Path, event: &EventEnvelope, encoded: &[u8]) -> Result<()> {
    let event_type = serde_json::to_value(event.event_type)?;
    let event_type = event_type
        .as_str()
        .ok_or_else(|| anyhow!("event type did not serialize as a string"))?;
    let mut process = Command::new(command);
    process
        .env("DUTIS_EVENT_ID", &event.id)
        .env("DUTIS_EVENT_TYPE", event_type)
        .env_remove("DUTIS_APPROVAL_TOKEN")
        .env_remove("DUTIS_MCP_APPROVAL_TOKEN")
        .env_remove("DUTIS_WATCH_APPROVAL_TOKEN")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let mut child = process
        .spawn()
        .with_context(|| format!("failed to start {}", command.display()))?;
    child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("event command stdin is unavailable"))?
        .write_all(encoded)?;
    let output = child.wait_with_output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim().chars().take(1024).collect::<String>();
        bail!(
            "event command exited with {}{}",
            output.status,
            if stderr.is_empty() {
                String::new()
            } else {
                format!(": {stderr}")
            }
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "dutis-events-{label}-{}-{unique}",
            std::process::id()
        ))
    }

    #[test]
    fn disabled_dispatcher_is_a_noop() {
        let dispatcher = EventDispatcher::new(None, None).unwrap();
        assert!(!dispatcher.is_enabled());
        assert!(dispatcher
            .emit(EventType::DriftChecked, EventSource::Watcher, &json!({}))
            .unwrap()
            .is_none());
    }

    #[test]
    fn jsonl_sink_appends_versioned_events_with_private_permissions() {
        let root = temp_root("jsonl");
        let log = root.join("nested/events.jsonl");
        let dispatcher = EventDispatcher::new(Some(log.clone()), None).unwrap();
        dispatcher
            .emit(
                EventType::DriftChecked,
                EventSource::Watcher,
                &json!({"state": "in_sync"}),
            )
            .unwrap();
        dispatcher
            .emit(
                EventType::MutationCompleted,
                EventSource::Governance,
                &json!({"audit_id": "audit-1"}),
            )
            .unwrap();
        let events = fs::read_to_string(&log)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<EventEnvelope>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].schema_version, EVENT_SCHEMA_VERSION);
        assert_eq!(events[0].event_type, EventType::DriftChecked);
        assert_eq!(events[1].payload["audit_id"], "audit-1");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&log).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(
                fs::metadata(&root).unwrap().permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(root.join("nested"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn command_sink_receives_json_on_stdin_and_event_metadata_in_environment() {
        use std::os::unix::fs::PermissionsExt;

        let root = temp_root("command");
        fs::create_dir_all(&root).unwrap();
        let command = root.join("sink.sh");
        let output = root.join("received.jsonl");
        fs::write(
            &command,
            "#!/bin/sh\nprintf '%s|%s\\n' \"$DUTIS_EVENT_TYPE\" \"$DUTIS_EVENT_ID\" >> \"$DUTIS_TEST_METADATA\"\ncat >> \"$DUTIS_TEST_OUTPUT\"\n",
        )
        .unwrap();
        fs::set_permissions(&command, fs::Permissions::from_mode(0o700)).unwrap();
        std::env::set_var("DUTIS_TEST_OUTPUT", &output);
        std::env::set_var("DUTIS_TEST_METADATA", root.join("metadata"));
        let dispatcher = EventDispatcher::new(None, Some(command)).unwrap();
        let event = dispatcher
            .emit(
                EventType::MutationPending,
                EventSource::Governance,
                &json!({"audit_id": "audit-1"}),
            )
            .unwrap()
            .unwrap();
        let received: EventEnvelope =
            serde_json::from_str(fs::read_to_string(output).unwrap().trim()).unwrap();
        assert_eq!(received, event);
        let metadata = fs::read_to_string(root.join("metadata")).unwrap();
        assert_eq!(metadata.trim(), format!("mutation.pending|{}", event.id));
        std::env::remove_var("DUTIS_TEST_OUTPUT");
        std::env::remove_var("DUTIS_TEST_METADATA");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_missing_or_non_executable_commands() {
        let root = temp_root("invalid-command");
        let missing = root.join("missing");
        assert!(EventDispatcher::new(None, Some(missing)).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn attempts_jsonl_sink_even_when_command_sink_fails() {
        use std::os::unix::fs::PermissionsExt;

        let root = temp_root("partial-delivery");
        fs::create_dir_all(&root).unwrap();
        let command = root.join("fail.sh");
        fs::write(&command, "#!/bin/sh\necho rejected >&2\nexit 7\n").unwrap();
        fs::set_permissions(&command, fs::Permissions::from_mode(0o700)).unwrap();
        let log = root.join("events.jsonl");
        let dispatcher = EventDispatcher::new(Some(log.clone()), Some(command)).unwrap();
        let error = dispatcher
            .emit(
                EventType::MutationDenied,
                EventSource::Governance,
                &json!({"audit_id": "audit-1"}),
            )
            .unwrap_err();
        assert!(error.to_string().contains("rejected"));
        assert_eq!(fs::read_to_string(log).unwrap().lines().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }
}
