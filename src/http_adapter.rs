use crate::events::{EventEnvelope, EVENT_SCHEMA_VERSION};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::Value;
use std::env;
use std::fs::{self, DirBuilder, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub const HTTP_ENDPOINT_ENV: &str = "DUTIS_HTTP_ENDPOINT";
pub const HTTP_BEARER_TOKEN_ENV: &str = "DUTIS_HTTP_BEARER_TOKEN";
pub const HTTP_TIMEOUT_ENV: &str = "DUTIS_HTTP_TIMEOUT_SECONDS";
pub const HTTP_RETRIES_ENV: &str = "DUTIS_HTTP_RETRIES";
pub const HTTP_CURL_ENV: &str = "DUTIS_HTTP_CURL";
pub const MAX_EVENT_BYTES: u64 = 1024 * 1024;

const DEFAULT_TIMEOUT_SECONDS: u64 = 10;
const MAX_TIMEOUT_SECONDS: u64 = 300;
const DEFAULT_RETRIES: u32 = 2;
const MAX_RETRIES: u32 = 10;
const DEFAULT_CURL_PATH: &str = "/usr/bin/curl";

static REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
pub struct HttpAdapterConfig {
    endpoint: String,
    bearer_token: Option<String>,
    timeout_seconds: u64,
    retries: u32,
    curl: PathBuf,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct HttpAdapterStatus {
    pub schema_version: u32,
    pub transport: &'static str,
    pub authentication: &'static str,
    pub timeout_seconds: u64,
    pub retries: u32,
    pub curl_available: bool,
}

impl HttpAdapterConfig {
    pub fn from_environment() -> Result<Self> {
        let endpoint = required_environment_value(HTTP_ENDPOINT_ENV)?;
        validate_endpoint(&endpoint)?;
        let bearer_token = optional_environment_value(HTTP_BEARER_TOKEN_ENV);
        if let Some(token) = &bearer_token {
            validate_single_line_secret(HTTP_BEARER_TOKEN_ENV, token)?;
        }
        let timeout_seconds = numeric_environment_value(
            HTTP_TIMEOUT_ENV,
            DEFAULT_TIMEOUT_SECONDS,
            1,
            MAX_TIMEOUT_SECONDS,
        )?;
        let retries = numeric_environment_value(HTTP_RETRIES_ENV, DEFAULT_RETRIES, 0, MAX_RETRIES)?;
        let curl = env::var_os(HTTP_CURL_ENV)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_CURL_PATH));
        validate_curl(&curl)?;
        Ok(Self {
            endpoint,
            bearer_token,
            timeout_seconds,
            retries,
            curl,
        })
    }

    pub fn status(&self) -> HttpAdapterStatus {
        HttpAdapterStatus {
            schema_version: 1,
            transport: "https",
            authentication: if self.bearer_token.is_some() {
                "bearer"
            } else {
                "none"
            },
            timeout_seconds: self.timeout_seconds,
            retries: self.retries,
            curl_available: true,
        }
    }

    pub fn deliver<R: Read>(&self, reader: R) -> Result<()> {
        let (event, encoded) = read_event(reader)?;
        let request = PrivateRequest::create(&encoded)?;
        let curl_config = self.render_curl_config(&event, request.body())?;
        request.write_config(curl_config.as_bytes())?;

        let output = Command::new(&self.curl)
            .arg("--disable")
            .arg("--config")
            .arg(request.config())
            .env_remove(HTTP_ENDPOINT_ENV)
            .env_remove(HTTP_BEARER_TOKEN_ENV)
            .env_remove("DUTIS_APPROVAL_TOKEN")
            .env_remove("DUTIS_MCP_APPROVAL_TOKEN")
            .env_remove("DUTIS_WATCH_APPROVAL_TOKEN")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .context("failed to start the HTTP transport")?;
        if !output.status.success() {
            bail!(
                "HTTP event delivery failed with transport status {}",
                output.status
            );
        }
        Ok(())
    }

    fn render_curl_config(&self, event: &EventEnvelope, body: &Path) -> Result<String> {
        let event_type = serde_json::to_value(event.event_type)?;
        let event_type = event_type
            .as_str()
            .context("event type did not serialize as a string")?;
        let mut lines = vec![
            config_line("url", &self.endpoint),
            config_line("request", "POST"),
            config_line("header", "Content-Type: application/json"),
            config_line("header", "Accept: application/json"),
            config_line("header", &format!("X-Dutis-Event-Id: {}", event.id)),
            config_line("header", &format!("X-Dutis-Event-Type: {event_type}")),
            config_line("header", &format!("Idempotency-Key: {}", event.id)),
            config_line("data-binary", &format!("@{}", body.display())),
            config_line(
                "user-agent",
                &format!("dutis-event-http/{}", env!("CARGO_PKG_VERSION")),
            ),
            config_line("connect-timeout", &self.timeout_seconds.min(5).to_string()),
            config_line("max-time", &self.timeout_seconds.to_string()),
            config_line("retry", &self.retries.to_string()),
            config_line("retry-max-time", &self.timeout_seconds.to_string()),
            config_line("proto", "=https"),
            "fail".to_owned(),
            "silent".to_owned(),
            "show-error".to_owned(),
            config_line("output", "/dev/null"),
        ];
        if let Some(token) = &self.bearer_token {
            lines.insert(
                7,
                config_line("header", &format!("Authorization: Bearer {token}")),
            );
        }
        lines.push(String::new());
        Ok(lines.join("\n"))
    }
}

fn required_environment_value(name: &str) -> Result<String> {
    optional_environment_value(name).with_context(|| format!("{name} is required"))
}

fn optional_environment_value(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.trim().is_empty())
}

fn numeric_environment_value<T>(name: &str, default: T, minimum: T, maximum: T) -> Result<T>
where
    T: Copy + Ord + std::str::FromStr + std::fmt::Display,
{
    let Some(value) = optional_environment_value(name) else {
        return Ok(default);
    };
    let parsed = value
        .parse::<T>()
        .map_err(|_| anyhow::anyhow!("{name} must be an integer"))?;
    if parsed < minimum || parsed > maximum {
        bail!("{name} must be between {minimum} and {maximum}");
    }
    Ok(parsed)
}

fn validate_endpoint(endpoint: &str) -> Result<()> {
    validate_single_line_secret(HTTP_ENDPOINT_ENV, endpoint)?;
    let authority = endpoint
        .strip_prefix("https://")
        .context("DUTIS_HTTP_ENDPOINT must be an absolute HTTPS URL")?;
    if authority.is_empty()
        || authority.starts_with('/')
        || authority.chars().any(char::is_whitespace)
    {
        bail!("DUTIS_HTTP_ENDPOINT must be an absolute HTTPS URL");
    }
    Ok(())
}

fn validate_single_line_secret(name: &str, value: &str) -> Result<()> {
    if value
        .chars()
        .any(|character| matches!(character, '\r' | '\n' | '\0'))
    {
        bail!("{name} must contain a single non-NUL line");
    }
    Ok(())
}

fn validate_curl(path: &Path) -> Result<()> {
    if !path.is_absolute() || !path.is_file() {
        bail!("DUTIS_HTTP_CURL must be an existing absolute executable");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if fs::metadata(path)?.permissions().mode() & 0o111 == 0 {
            bail!("DUTIS_HTTP_CURL must be an existing absolute executable");
        }
    }
    Ok(())
}

fn read_event<R: Read>(reader: R) -> Result<(EventEnvelope, Vec<u8>)> {
    let mut encoded = Vec::new();
    reader
        .take(MAX_EVENT_BYTES + 1)
        .read_to_end(&mut encoded)
        .context("failed to read the event from stdin")?;
    if encoded.len() as u64 > MAX_EVENT_BYTES {
        bail!("event exceeds the {MAX_EVENT_BYTES}-byte input limit");
    }
    let value: Value =
        serde_json::from_slice(&encoded).context("stdin must contain one JSON event")?;
    let event: EventEnvelope =
        serde_json::from_value(value.clone()).context("stdin is not a valid Dutis event")?;
    if event.schema_version != EVENT_SCHEMA_VERSION {
        bail!(
            "unsupported event schema version {}; expected {}",
            event.schema_version,
            EVENT_SCHEMA_VERSION
        );
    }
    validate_event_id(&event.id)?;
    let mut canonical = serde_json::to_vec(&value).context("failed to encode the event")?;
    canonical.push(b'\n');
    Ok((event, canonical))
}

fn validate_event_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > 256
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        bail!(
            "event ID must contain 1 to 256 ASCII letters, numbers, dots, underscores, or hyphens"
        );
    }
    Ok(())
}

fn config_line(name: &str, value: &str) -> String {
    format!("{name} = \"{}\"", escape_curl_config(value))
}

fn escape_curl_config(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

struct PrivateRequest {
    directory: PathBuf,
    body: PathBuf,
    config: PathBuf,
}

impl PrivateRequest {
    fn create(body: &[u8]) -> Result<Self> {
        let directory = create_private_directory()?;
        let request = Self {
            body: directory.join("event.json"),
            config: directory.join("curl.conf"),
            directory,
        };
        write_private_file(&request.body, body)?;
        Ok(request)
    }

    fn body(&self) -> &Path {
        &self.body
    }

    fn config(&self) -> &Path {
        &self.config
    }

    fn write_config(&self, contents: &[u8]) -> Result<()> {
        write_private_file(&self.config, contents)
    }
}

impl Drop for PrivateRequest {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.config);
        let _ = fs::remove_file(&self.body);
        let _ = fs::remove_dir(&self.directory);
    }
}

fn create_private_directory() -> Result<PathBuf> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before the Unix epoch")?
        .as_nanos();
    for _ in 0..128 {
        let sequence = REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = env::temp_dir().join(format!(
            "dutis-event-http-{}-{timestamp}-{sequence}",
            std::process::id()
        ));
        let mut builder = DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        match builder.create(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error).context("failed to create a private request directory");
            }
        }
    }
    bail!("failed to allocate a private request directory")
}

fn write_private_file(path: &Path, contents: &[u8]) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .with_context(|| "failed to create a private request file")?;
    file.write_all(contents)
        .context("failed to write a private request file")?;
    file.flush()
        .context("failed to flush a private request file")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event_json(schema_version: u32) -> String {
        format!(
            r#"{{"schema_version":{schema_version},"id":"event-1","emitted_at":"2026-08-23T00:00:00Z","event_type":"drift.checked","source":"watcher","payload":{{"state":"in_sync"}}}}"#
        )
    }

    #[test]
    fn reads_one_supported_event_and_preserves_unknown_fields() {
        let input = event_json(EVENT_SCHEMA_VERSION)
            .replace("\"payload\"", "\"future_field\":true,\"payload\"");
        let (event, encoded) = read_event(input.as_bytes()).unwrap();
        assert_eq!(event.id, "event-1");
        let value: Value = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(value["future_field"], true);
    }

    #[test]
    fn rejects_unsupported_schema_and_oversized_input() {
        assert!(read_event(event_json(99).as_bytes())
            .unwrap_err()
            .to_string()
            .contains("unsupported event schema"));
        let oversized = vec![b' '; MAX_EVENT_BYTES as usize + 1];
        assert!(read_event(oversized.as_slice())
            .unwrap_err()
            .to_string()
            .contains("input limit"));
    }

    #[test]
    fn rejects_event_id_header_injection() {
        let input =
            event_json(EVENT_SCHEMA_VERSION).replace("event-1", "event-1\\r\\nInjected: yes");
        assert!(read_event(input.as_bytes())
            .unwrap_err()
            .to_string()
            .contains("event ID"));
    }

    #[test]
    fn escapes_curl_config_values() {
        assert_eq!(escape_curl_config(r#"a\b"c"#), r#"a\\b\"c"#);
    }

    #[test]
    fn endpoint_requires_https_and_rejects_line_breaks() {
        assert!(validate_endpoint("http://example.com").is_err());
        assert!(validate_endpoint("https://example.com\nheader: value").is_err());
        assert!(validate_endpoint("https://example.com/hook").is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn request_files_are_private_and_removed_on_drop() {
        use std::os::unix::fs::PermissionsExt;

        let directory;
        let body;
        {
            let request = PrivateRequest::create(b"{}\n").unwrap();
            request.write_config(b"silent\n").unwrap();
            directory = request.directory.clone();
            body = request.body.clone();
            assert_eq!(
                fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(&body).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(
                fs::metadata(request.config()).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        assert!(!body.exists());
        assert!(!directory.exists());
    }
}
