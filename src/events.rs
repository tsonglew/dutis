use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

pub const EVENT_SCHEMA_VERSION: u32 = 1;
pub const EVENT_LOG_ENV: &str = "DUTIS_EVENT_LOG";
pub const EVENT_COMMAND_ENV: &str = "DUTIS_EVENT_COMMAND";
pub const EVENT_OUTBOX_ENV: &str = "DUTIS_EVENT_OUTBOX";
pub const PENDING_EVENT_SCHEMA_VERSION: u32 = 1;
pub const DEAD_LETTER_SCHEMA_VERSION: u32 = 1;
pub const EVENT_HEALTH_SCHEMA_VERSION: u32 = 1;

const MAX_EVENT_RECORD_BYTES: u64 = 4 * 1024 * 1024;

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

impl EventType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DriftChecked => "drift.checked",
            Self::MutationPending => "mutation.pending",
            Self::MutationDenied => "mutation.denied",
            Self::MutationFailed => "mutation.failed",
            Self::MutationCompleted => "mutation.completed",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventSource {
    Watcher,
    Mcp,
    Governance,
}

impl EventSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Watcher => "watcher",
            Self::Mcp => "mcp",
            Self::Governance => "governance",
        }
    }
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

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingEvent {
    pub schema_version: u32,
    pub queued_at: String,
    pub last_attempted_at: String,
    pub attempts: u64,
    pub event: EventEnvelope,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct PendingEventSummary {
    pub id: String,
    pub event_type: EventType,
    pub source: EventSource,
    pub emitted_at: String,
    pub queued_at: String,
    pub last_attempted_at: String,
    pub attempts: u64,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct ReplayResult {
    pub id: String,
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct ReplayReport {
    pub attempted: usize,
    pub delivered: usize,
    pub failed: usize,
    pub remaining: usize,
    pub results: Vec<ReplayResult>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeadLetterEvent {
    pub schema_version: u32,
    pub dead_lettered_at: String,
    pub reasons: Vec<String>,
    pub pending: PendingEvent,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct DeadLetterSummary {
    pub id: String,
    pub event_type: EventType,
    pub source: EventSource,
    pub emitted_at: String,
    pub queued_at: String,
    pub dead_lettered_at: String,
    pub attempts: u64,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct OutboxMaintenanceItem {
    pub id: String,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct OutboxMaintenanceReport {
    pub operation: &'static str,
    pub applied: bool,
    pub matched: usize,
    pub pending: usize,
    pub dead_letters: usize,
    pub events: Vec<OutboxMaintenanceItem>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventDeliveryHealthStatus {
    Healthy,
    Degraded,
    AttentionRequired,
}

impl EventDeliveryHealthStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Degraded => "degraded",
            Self::AttentionRequired => "attention_required",
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct PendingDeliveryMetrics {
    pub count: usize,
    pub total_attempts: u64,
    pub max_attempts: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oldest_queued_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_attempted_at: Option<String>,
    pub by_event_type: BTreeMap<String, usize>,
    pub by_source: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct DeadLetterMetrics {
    pub count: usize,
    pub total_attempts: u64,
    pub max_attempts: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oldest_dead_lettered_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_dead_lettered_at: Option<String>,
    pub by_event_type: BTreeMap<String, usize>,
    pub by_source: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct EventDeliveryHealth {
    pub schema_version: u32,
    pub generated_at: String,
    pub status: EventDeliveryHealthStatus,
    pub pending: PendingDeliveryMetrics,
    pub dead_letters: DeadLetterMetrics,
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
    outbox: Option<EventOutbox>,
}

impl EventDispatcher {
    pub fn from_environment() -> Result<Self> {
        let log = std::env::var_os(EVENT_LOG_ENV)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);
        let command = std::env::var_os(EVENT_COMMAND_ENV)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);
        let outbox = if std::env::var_os(EVENT_OUTBOX_ENV).is_some() || command.is_some() {
            Some(EventOutbox::from_environment()?)
        } else {
            None
        };
        Self::with_outbox(log, command, outbox)
    }

    pub fn new(log: Option<PathBuf>, command: Option<PathBuf>) -> Result<Self> {
        Self::with_outbox(log, command, None)
    }

    pub fn with_outbox(
        log: Option<PathBuf>,
        command: Option<PathBuf>,
        outbox: Option<EventOutbox>,
    ) -> Result<Self> {
        let log = log.map(absolute_path).transpose()?;
        let command = command.map(absolute_path).transpose()?;
        if let Some(command) = &command {
            validate_command(command)?;
        }
        Ok(Self {
            log,
            command,
            outbox,
        })
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

    pub fn outbox(&self) -> Option<&EventOutbox> {
        self.outbox.as_ref()
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
                let mut failure = format!("command sink {}: {error:#}", command.display());
                if let Some(outbox) = &self.outbox {
                    match outbox.enqueue(&event) {
                        Ok(_) => failure.push_str(&format!(
                            "; event queued for replay in {}",
                            outbox.directory().display()
                        )),
                        Err(queue_error) => failure.push_str(&format!(
                            "; failed to queue event for replay: {queue_error:#}"
                        )),
                    }
                }
                failures.push(failure);
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
    configure_process_sinks_with_outbox(log_override, command_override, None)
}

pub fn configure_process_sinks_with_outbox(
    log_override: Option<&Path>,
    command_override: Option<&Path>,
    outbox_override: Option<&Path>,
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
    if let Some(path) = outbox_override {
        let path = absolute_path(path.to_path_buf())?;
        std::env::set_var(EVENT_OUTBOX_ENV, &path);
    }
    let dispatcher = EventDispatcher::from_environment()?;
    if let Some(path) = dispatcher.log() {
        std::env::set_var(EVENT_LOG_ENV, path);
    }
    if let Some(path) = dispatcher.command() {
        std::env::set_var(EVENT_COMMAND_ENV, path);
    }
    if let Some(outbox) = dispatcher.outbox() {
        std::env::set_var(EVENT_OUTBOX_ENV, outbox.directory());
    }
    Ok(dispatcher)
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EventOutbox {
    directory: PathBuf,
}

impl EventOutbox {
    pub fn from_environment() -> Result<Self> {
        let directory = if let Some(path) =
            std::env::var_os(EVENT_OUTBOX_ENV).filter(|value| !value.is_empty())
        {
            PathBuf::from(path)
        } else if let Some(path) =
            std::env::var_os("DUTIS_STATE_DIR").filter(|value| !value.is_empty())
        {
            PathBuf::from(path).join("event-outbox")
        } else {
            let home = std::env::var_os("HOME").ok_or_else(|| {
                anyhow!("HOME is not set; set DUTIS_EVENT_OUTBOX or DUTIS_STATE_DIR explicitly")
            })?;
            PathBuf::from(home).join("Library/Application Support/dutis/event-outbox")
        };
        Ok(Self::new(absolute_path(directory)?))
    }

    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn enqueue(&self, event: &EventEnvelope) -> Result<PendingEvent> {
        validate_event(event)?;
        let now = current_timestamp()?;
        let path = self.record_path(&event.id)?;
        let existing = if path.is_file() {
            Some(self.load_path(&path)?)
        } else {
            None
        };
        let pending = PendingEvent {
            schema_version: PENDING_EVENT_SCHEMA_VERSION,
            queued_at: existing
                .as_ref()
                .map_or_else(|| now.clone(), |record| record.queued_at.clone()),
            last_attempted_at: now,
            attempts: existing.map_or(1, |record| record.attempts.saturating_add(1)),
            event: event.clone(),
        };
        self.store(&pending)?;
        Ok(pending)
    }

    pub fn pending(&self) -> Result<Vec<PendingEventSummary>> {
        Ok(self
            .load_all()?
            .into_iter()
            .map(|pending| PendingEventSummary {
                id: pending.event.id,
                event_type: pending.event.event_type,
                source: pending.event.source,
                emitted_at: pending.event.emitted_at,
                queued_at: pending.queued_at,
                last_attempted_at: pending.last_attempted_at,
                attempts: pending.attempts,
            })
            .collect())
    }

    pub fn dead_letters(&self) -> Result<Vec<DeadLetterSummary>> {
        Ok(self
            .load_all_dead_letters()?
            .into_iter()
            .map(|record| DeadLetterSummary {
                id: record.pending.event.id,
                event_type: record.pending.event.event_type,
                source: record.pending.event.source,
                emitted_at: record.pending.event.emitted_at,
                queued_at: record.pending.queued_at,
                dead_lettered_at: record.dead_lettered_at,
                attempts: record.pending.attempts,
                reasons: record.reasons,
            })
            .collect())
    }

    pub fn health(&self) -> Result<EventDeliveryHealth> {
        let pending = self.load_all()?;
        let dead_letters = self.load_all_dead_letters()?;
        let (oldest_queued_at, _) = timestamp_range(
            pending.iter().map(|record| record.queued_at.as_str()),
            "queued_at",
        )?;
        let (_, latest_attempted_at) = timestamp_range(
            pending
                .iter()
                .map(|record| record.last_attempted_at.as_str()),
            "last_attempted_at",
        )?;
        let (oldest_dead_lettered_at, latest_dead_lettered_at) = timestamp_range(
            dead_letters
                .iter()
                .map(|record| record.dead_lettered_at.as_str()),
            "dead_lettered_at",
        )?;

        let mut pending_by_event_type = empty_event_type_counts();
        let mut pending_by_source = empty_event_source_counts();
        let mut pending_attempts = 0_u64;
        let mut pending_max_attempts = 0_u64;
        for record in &pending {
            increment(&mut pending_by_event_type, record.event.event_type.as_str());
            increment(&mut pending_by_source, record.event.source.as_str());
            pending_attempts = pending_attempts.saturating_add(record.attempts);
            pending_max_attempts = pending_max_attempts.max(record.attempts);
        }

        let mut dead_letter_by_event_type = empty_event_type_counts();
        let mut dead_letter_by_source = empty_event_source_counts();
        let mut dead_letter_attempts = 0_u64;
        let mut dead_letter_max_attempts = 0_u64;
        for record in &dead_letters {
            increment(
                &mut dead_letter_by_event_type,
                record.pending.event.event_type.as_str(),
            );
            increment(
                &mut dead_letter_by_source,
                record.pending.event.source.as_str(),
            );
            dead_letter_attempts = dead_letter_attempts.saturating_add(record.pending.attempts);
            dead_letter_max_attempts = dead_letter_max_attempts.max(record.pending.attempts);
        }

        let status = if !dead_letters.is_empty() {
            EventDeliveryHealthStatus::AttentionRequired
        } else if !pending.is_empty() {
            EventDeliveryHealthStatus::Degraded
        } else {
            EventDeliveryHealthStatus::Healthy
        };
        Ok(EventDeliveryHealth {
            schema_version: EVENT_HEALTH_SCHEMA_VERSION,
            generated_at: current_timestamp()?,
            status,
            pending: PendingDeliveryMetrics {
                count: pending.len(),
                total_attempts: pending_attempts,
                max_attempts: pending_max_attempts,
                oldest_queued_at,
                latest_attempted_at,
                by_event_type: pending_by_event_type,
                by_source: pending_by_source,
            },
            dead_letters: DeadLetterMetrics {
                count: dead_letters.len(),
                total_attempts: dead_letter_attempts,
                max_attempts: dead_letter_max_attempts,
                oldest_dead_lettered_at,
                latest_dead_lettered_at,
                by_event_type: dead_letter_by_event_type,
                by_source: dead_letter_by_source,
            },
        })
    }

    pub fn archive(
        &self,
        max_attempts: Option<u64>,
        older_than_days: Option<u64>,
        apply: bool,
    ) -> Result<OutboxMaintenanceReport> {
        if max_attempts.is_none() && older_than_days.is_none() {
            bail!("archive requires --max-attempts or --older-than-days");
        }
        if max_attempts == Some(0) {
            bail!("maximum attempts must be at least 1");
        }
        let queued_before = older_than_days.map(cutoff_from_days).transpose()?;
        let pending = self.load_all()?;
        let mut matched = Vec::new();
        for record in pending {
            let mut reasons = Vec::new();
            if let Some(maximum) = max_attempts {
                if record.attempts >= maximum {
                    reasons.push(format!("attempts_gte_{maximum}"));
                }
            }
            if let Some(cutoff) = queued_before {
                let queued_at = parse_timestamp(&record.queued_at, "queued_at")?;
                if queued_at <= cutoff {
                    reasons.push(format!(
                        "queued_at_least_{}_days",
                        older_than_days.expect("cutoff has a retention age")
                    ));
                }
            }
            if reasons.is_empty() {
                continue;
            }
            if apply {
                let dead_letter = DeadLetterEvent {
                    schema_version: DEAD_LETTER_SCHEMA_VERSION,
                    dead_lettered_at: current_timestamp()?,
                    reasons: reasons.clone(),
                    pending: record.clone(),
                };
                self.store_dead_letter(&dead_letter)?;
                fs::remove_file(self.record_path(&record.event.id)?).with_context(|| {
                    format!("failed to remove archived event {}", record.event.id)
                })?;
            }
            matched.push(OutboxMaintenanceItem {
                id: record.event.id,
                reasons,
            });
        }
        Ok(OutboxMaintenanceReport {
            operation: "archive",
            applied: apply,
            matched: matched.len(),
            pending: self.load_all()?.len(),
            dead_letters: self.load_all_dead_letters()?.len(),
            events: matched,
        })
    }

    pub fn purge(&self, older_than_days: u64, apply: bool) -> Result<OutboxMaintenanceReport> {
        let cutoff = cutoff_from_days(older_than_days)?;
        let dead_letters = self.load_all_dead_letters()?;
        let mut matched = Vec::new();
        for record in dead_letters {
            let dead_lettered_at = parse_timestamp(&record.dead_lettered_at, "dead_lettered_at")?;
            if dead_lettered_at > cutoff {
                continue;
            }
            let reasons = vec![format!("dead_lettered_at_least_{older_than_days}_days")];
            if apply {
                fs::remove_file(self.dead_letter_path(&record.pending.event.id)?).with_context(
                    || {
                        format!(
                            "failed to purge dead-letter event {}",
                            record.pending.event.id
                        )
                    },
                )?;
            }
            matched.push(OutboxMaintenanceItem {
                id: record.pending.event.id,
                reasons,
            });
        }
        Ok(OutboxMaintenanceReport {
            operation: "purge",
            applied: apply,
            matched: matched.len(),
            pending: self.load_all()?.len(),
            dead_letters: self.load_all_dead_letters()?.len(),
            events: matched,
        })
    }

    pub fn replay(&self, command: &Path, limit: usize) -> Result<ReplayReport> {
        validate_command(command)?;
        if limit == 0 {
            bail!("replay limit must be at least 1");
        }
        let pending = self.load_all()?;
        let mut results = Vec::new();
        let mut delivered = 0;
        let mut failed = 0;
        for mut record in pending.into_iter().take(limit) {
            let mut encoded = serde_json::to_vec(&record.event)?;
            encoded.push(b'\n');
            match run_event_command(command, &record.event, &encoded) {
                Ok(()) => {
                    fs::remove_file(self.record_path(&record.event.id)?).with_context(|| {
                        format!("failed to remove delivered event {}", record.event.id)
                    })?;
                    delivered += 1;
                    results.push(ReplayResult {
                        id: record.event.id,
                        status: "delivered",
                        error: None,
                    });
                }
                Err(error) => {
                    record.attempts = record.attempts.saturating_add(1);
                    record.last_attempted_at = current_timestamp()?;
                    self.store(&record)?;
                    failed += 1;
                    results.push(ReplayResult {
                        id: record.event.id,
                        status: "failed",
                        error: Some(format!("{error:#}")),
                    });
                }
            }
        }
        let remaining = self.load_all()?.len();
        Ok(ReplayReport {
            attempted: delivered + failed,
            delivered,
            failed,
            remaining,
            results,
        })
    }

    fn load_all(&self) -> Result<Vec<PendingEvent>> {
        if !self.directory.exists() {
            return Ok(Vec::new());
        }
        let mut paths = Vec::new();
        for entry in fs::read_dir(&self.directory)
            .with_context(|| format!("failed to read {}", self.directory.display()))?
        {
            let path = entry
                .with_context(|| format!("failed to read {}", self.directory.display()))?
                .path();
            if path
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                paths.push(path);
            }
        }
        paths.sort();
        let mut pending = paths
            .iter()
            .map(|path| self.load_path(path))
            .collect::<Result<Vec<_>>>()?;
        pending.sort_by(|left, right| {
            (&left.queued_at, &left.event.id).cmp(&(&right.queued_at, &right.event.id))
        });
        Ok(pending)
    }

    fn load_all_dead_letters(&self) -> Result<Vec<DeadLetterEvent>> {
        let directory = self.dead_letter_directory();
        if !directory.exists() {
            return Ok(Vec::new());
        }
        let mut paths = json_files(&directory)?;
        paths.sort();
        let mut dead_letters = paths
            .iter()
            .map(|path| self.load_dead_letter(path))
            .collect::<Result<Vec<_>>>()?;
        dead_letters.sort_by(|left, right| {
            (&left.dead_lettered_at, &left.pending.event.id)
                .cmp(&(&right.dead_lettered_at, &right.pending.event.id))
        });
        Ok(dead_letters)
    }

    fn load_dead_letter(&self, path: &Path) -> Result<DeadLetterEvent> {
        validate_regular_json_file(path)?;
        let file = fs::File::open(path)
            .with_context(|| format!("failed to open dead-letter event {}", path.display()))?;
        let record: DeadLetterEvent = serde_json::from_reader(BufReader::new(file))
            .with_context(|| format!("failed to parse dead-letter event {}", path.display()))?;
        validate_dead_letter(&record)?;
        if self.dead_letter_path(&record.pending.event.id)? != path {
            bail!(
                "dead-letter event ID does not match its filename: {}",
                path.display()
            );
        }
        Ok(record)
    }

    fn load_path(&self, path: &Path) -> Result<PendingEvent> {
        validate_regular_json_file(path)?;
        let file = fs::File::open(path)
            .with_context(|| format!("failed to open pending event {}", path.display()))?;
        let pending: PendingEvent = serde_json::from_reader(BufReader::new(file))
            .with_context(|| format!("failed to parse pending event {}", path.display()))?;
        validate_pending_event(&pending)?;
        let expected = self.record_path(&pending.event.id)?;
        if expected != path {
            bail!(
                "pending event ID does not match its filename: {}",
                path.display()
            );
        }
        Ok(pending)
    }

    fn store(&self, pending: &PendingEvent) -> Result<()> {
        validate_pending_event(pending)?;
        create_private_directories(&self.directory)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.directory, fs::Permissions::from_mode(0o700))?;
        }
        let destination = self.record_path(&pending.event.id)?;
        let sequence = EVENT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary = self.directory.join(format!(
            ".{}.{}.{}.tmp",
            pending.event.id,
            std::process::id(),
            sequence
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options
            .open(&temporary)
            .with_context(|| format!("failed to create {}", temporary.display()))?;
        let mut writer = BufWriter::new(file);
        serde_json::to_writer_pretty(&mut writer, pending)?;
        writer.write_all(b"\n")?;
        writer.flush()?;
        writer.get_ref().sync_all()?;
        fs::rename(&temporary, &destination)
            .with_context(|| format!("failed to atomically store event {}", pending.event.id))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&destination, fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }

    fn store_dead_letter(&self, record: &DeadLetterEvent) -> Result<()> {
        validate_dead_letter(record)?;
        let directory = self.dead_letter_directory();
        create_private_directories(&directory)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
        }
        let destination = self.dead_letter_path(&record.pending.event.id)?;
        let sequence = EVENT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary = directory.join(format!(
            ".{}.{}.{}.tmp",
            record.pending.event.id,
            std::process::id(),
            sequence
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options
            .open(&temporary)
            .with_context(|| format!("failed to create {}", temporary.display()))?;
        let mut writer = BufWriter::new(file);
        serde_json::to_writer_pretty(&mut writer, record)?;
        writer.write_all(b"\n")?;
        writer.flush()?;
        writer.get_ref().sync_all()?;
        fs::rename(&temporary, &destination).with_context(|| {
            format!(
                "failed to atomically store dead-letter event {}",
                record.pending.event.id
            )
        })?;
        Ok(())
    }

    fn record_path(&self, id: &str) -> Result<PathBuf> {
        validate_event_id(id)?;
        Ok(self.directory.join(format!("{id}.json")))
    }

    fn dead_letter_directory(&self) -> PathBuf {
        self.directory.join("dead-letter")
    }

    fn dead_letter_path(&self, id: &str) -> Result<PathBuf> {
        validate_event_id(id)?;
        Ok(self.dead_letter_directory().join(format!("{id}.json")))
    }
}

fn json_files(directory: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(directory)
        .with_context(|| format!("failed to read {}", directory.display()))?
    {
        let path = entry
            .with_context(|| format!("failed to read {}", directory.display()))?
            .path();
        if path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            paths.push(path);
        }
    }
    Ok(paths)
}

fn validate_regular_json_file(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect event record {}", path.display()))?;
    if !metadata.file_type().is_file() {
        bail!("event record is not a regular file: {}", path.display());
    }
    if metadata.len() > MAX_EVENT_RECORD_BYTES {
        bail!("event record {} exceeds the size limit", path.display());
    }
    Ok(())
}

fn validate_dead_letter(record: &DeadLetterEvent) -> Result<()> {
    if record.schema_version != DEAD_LETTER_SCHEMA_VERSION {
        bail!(
            "unsupported dead-letter schema version {}; expected {}",
            record.schema_version,
            DEAD_LETTER_SCHEMA_VERSION
        );
    }
    if record.reasons.is_empty() || record.reasons.iter().any(|reason| reason.trim().is_empty()) {
        bail!("dead-letter event must include at least one non-empty reason");
    }
    parse_timestamp(&record.dead_lettered_at, "dead_lettered_at")?;
    validate_pending_event(&record.pending)
}

fn cutoff_from_days(days: u64) -> Result<OffsetDateTime> {
    let days = i64::try_from(days).context("retention period is too large")?;
    OffsetDateTime::now_utc()
        .checked_sub(time::Duration::days(days))
        .context("retention period is too large")
}

fn parse_timestamp(value: &str, field: &str) -> Result<OffsetDateTime> {
    OffsetDateTime::parse(value, &Rfc3339).with_context(|| format!("invalid {field} timestamp"))
}

fn timestamp_range<'a>(
    values: impl IntoIterator<Item = &'a str>,
    field: &str,
) -> Result<(Option<String>, Option<String>)> {
    let mut oldest: Option<(OffsetDateTime, String)> = None;
    let mut latest: Option<(OffsetDateTime, String)> = None;
    for value in values {
        let parsed = parse_timestamp(value, field)?;
        if oldest
            .as_ref()
            .is_none_or(|(timestamp, _)| parsed < *timestamp)
        {
            oldest = Some((parsed, value.to_owned()));
        }
        if latest
            .as_ref()
            .is_none_or(|(timestamp, _)| parsed > *timestamp)
        {
            latest = Some((parsed, value.to_owned()));
        }
    }
    Ok((
        oldest.map(|(_, value)| value),
        latest.map(|(_, value)| value),
    ))
}

fn empty_event_type_counts() -> BTreeMap<String, usize> {
    [
        EventType::DriftChecked,
        EventType::MutationPending,
        EventType::MutationDenied,
        EventType::MutationFailed,
        EventType::MutationCompleted,
    ]
    .into_iter()
    .map(|event_type| (event_type.as_str().to_owned(), 0))
    .collect()
}

fn empty_event_source_counts() -> BTreeMap<String, usize> {
    [
        EventSource::Watcher,
        EventSource::Mcp,
        EventSource::Governance,
    ]
    .into_iter()
    .map(|source| (source.as_str().to_owned(), 0))
    .collect()
}

fn increment(counts: &mut BTreeMap<String, usize>, key: &str) {
    counts
        .entry(key.to_owned())
        .and_modify(|count| *count = count.saturating_add(1))
        .or_insert(1);
}

fn validate_pending_event(pending: &PendingEvent) -> Result<()> {
    if pending.schema_version != PENDING_EVENT_SCHEMA_VERSION {
        bail!(
            "unsupported pending event schema version {}; expected {}",
            pending.schema_version,
            PENDING_EVENT_SCHEMA_VERSION
        );
    }
    if pending.attempts == 0 {
        bail!("pending event attempt count must be at least 1");
    }
    parse_timestamp(&pending.queued_at, "queued_at")?;
    parse_timestamp(&pending.last_attempted_at, "last_attempted_at")?;
    validate_event(&pending.event)
}

fn validate_event(event: &EventEnvelope) -> Result<()> {
    if event.schema_version != EVENT_SCHEMA_VERSION {
        bail!(
            "unsupported event schema version {}; expected {}",
            event.schema_version,
            EVENT_SCHEMA_VERSION
        );
    }
    parse_timestamp(&event.emitted_at, "emitted_at")?;
    validate_event_id(&event.id)
}

fn validate_event_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > 256
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        bail!(
            "event ID must contain 1 to 256 ASCII letters, numbers, dots, underscores, or hyphens"
        );
    }
    Ok(())
}

fn current_timestamp() -> Result<String> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .context("failed to format event timestamp")
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
    let mut process = Command::new(command);
    process
        .env("DUTIS_EVENT_ID", &event.id)
        .env("DUTIS_EVENT_TYPE", event.event_type.as_str())
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

    #[cfg(unix)]
    #[test]
    fn failed_command_events_are_persisted_and_replayed_until_delivered() {
        use std::os::unix::fs::PermissionsExt;

        let root = temp_root("outbox-replay");
        fs::create_dir_all(&root).unwrap();
        let failing = root.join("fail.sh");
        fs::write(&failing, "#!/bin/sh\necho unavailable >&2\nexit 7\n").unwrap();
        fs::set_permissions(&failing, fs::Permissions::from_mode(0o700)).unwrap();
        let delivered = root.join("delivered.jsonl");
        let succeeding = root.join("succeed.sh");
        fs::write(
            &succeeding,
            "#!/bin/sh\n/bin/cat >> \"$DUTIS_TEST_EVENT_OUTPUT\"\n",
        )
        .unwrap();
        fs::set_permissions(&succeeding, fs::Permissions::from_mode(0o700)).unwrap();
        std::env::set_var("DUTIS_TEST_EVENT_OUTPUT", &delivered);

        let outbox = EventOutbox::new(root.join("outbox"));
        let dispatcher =
            EventDispatcher::with_outbox(None, Some(failing.clone()), Some(outbox.clone()))
                .unwrap();
        let error = dispatcher
            .emit(
                EventType::MutationCompleted,
                EventSource::Governance,
                &json!({"audit_id": "audit-1"}),
            )
            .unwrap_err();
        assert!(error.to_string().contains("queued for replay"));
        let pending = outbox.pending().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].attempts, 1);
        assert_eq!(
            fs::metadata(outbox.directory())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );

        let report = outbox.replay(&failing, 100).unwrap();
        assert_eq!(
            (report.delivered, report.failed, report.remaining),
            (0, 1, 1)
        );
        assert_eq!(outbox.pending().unwrap()[0].attempts, 2);

        let report = outbox.replay(&succeeding, 100).unwrap();
        assert_eq!(
            (report.delivered, report.failed, report.remaining),
            (1, 0, 0)
        );
        let event: EventEnvelope =
            serde_json::from_str(fs::read_to_string(&delivered).unwrap().trim()).unwrap();
        assert_eq!(event.event_type, EventType::MutationCompleted);
        assert!(outbox.pending().unwrap().is_empty());

        std::env::remove_var("DUTIS_TEST_EVENT_OUTPUT");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn outbox_rejects_records_whose_filename_does_not_match_the_event_id() {
        let root = temp_root("outbox-invalid");
        let outbox = EventOutbox::new(root.join("outbox"));
        let event = EventEnvelope::new(
            EventType::DriftChecked,
            EventSource::Watcher,
            &json!({"state": "in_sync"}),
        )
        .unwrap();
        let pending = outbox.enqueue(&event).unwrap();
        let original = outbox.record_path(&pending.event.id).unwrap();
        fs::rename(&original, outbox.directory().join("different.json")).unwrap();
        assert!(outbox
            .pending()
            .unwrap_err()
            .to_string()
            .contains("does not match its filename"));
        assert!(outbox
            .health()
            .unwrap_err()
            .to_string()
            .contains("does not match its filename"));
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn outbox_rejects_symlinked_records() {
        use std::os::unix::fs::symlink;

        let root = temp_root("outbox-symlink");
        let directory = root.join("outbox");
        fs::create_dir_all(&directory).unwrap();
        let external = root.join("external.json");
        fs::write(&external, b"{}\n").unwrap();
        symlink(&external, directory.join("linked.json")).unwrap();
        let error = EventOutbox::new(&directory).pending().unwrap_err();
        assert!(error.to_string().contains("not a regular file"));
        let error = EventOutbox::new(&directory).health().unwrap_err();
        assert!(error.to_string().contains("not a regular file"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn archive_and_purge_are_preview_first_and_preserve_dead_letters() {
        let root = temp_root("outbox-retention");
        let outbox = EventOutbox::new(root.join("outbox"));
        let first = EventEnvelope::new(
            EventType::DriftChecked,
            EventSource::Watcher,
            &json!({"sequence": 1}),
        )
        .unwrap();
        let second = EventEnvelope::new(
            EventType::MutationFailed,
            EventSource::Governance,
            &json!({"sequence": 2}),
        )
        .unwrap();
        outbox.enqueue(&first).unwrap();
        outbox.enqueue(&second).unwrap();
        outbox.enqueue(&second).unwrap();

        let preview = outbox.archive(Some(2), None, false).unwrap();
        assert!(!preview.applied);
        assert_eq!(
            (preview.matched, preview.pending, preview.dead_letters),
            (1, 2, 0)
        );

        let archived = outbox.archive(Some(2), None, true).unwrap();
        assert!(archived.applied);
        assert_eq!(
            (archived.matched, archived.pending, archived.dead_letters),
            (1, 1, 1)
        );
        let dead_letters = outbox.dead_letters().unwrap();
        assert_eq!(dead_letters[0].id, second.id);
        assert_eq!(dead_letters[0].attempts, 2);
        assert!(dead_letters[0].reasons[0].contains("attempts"));

        let mut old = outbox
            .load_path(&outbox.record_path(&first.id).unwrap())
            .unwrap();
        old.queued_at = "2020-01-01T00:00:00Z".to_owned();
        outbox.store(&old).unwrap();
        let archived = outbox.archive(None, Some(1), true).unwrap();
        assert_eq!(
            (archived.matched, archived.pending, archived.dead_letters),
            (1, 0, 2)
        );

        let preview = outbox.purge(0, false).unwrap();
        assert!(!preview.applied);
        assert_eq!(
            (preview.matched, preview.pending, preview.dead_letters),
            (2, 0, 2)
        );
        let purged = outbox.purge(0, true).unwrap();
        assert!(purged.applied);
        assert_eq!(
            (purged.matched, purged.pending, purged.dead_letters),
            (2, 0, 0)
        );
        assert!(outbox.dead_letters().unwrap().is_empty());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn health_summarizes_backlog_without_exposing_event_contents() {
        let root = temp_root("outbox-health");
        let outbox = EventOutbox::new(root.join("outbox"));
        let pending = EventEnvelope::new(
            EventType::DriftChecked,
            EventSource::Watcher,
            &json!({"sensitive_payload": "must-not-leak"}),
        )
        .unwrap();
        let dead_letter = EventEnvelope::new(
            EventType::MutationFailed,
            EventSource::Governance,
            &json!({"secret": "also-must-not-leak"}),
        )
        .unwrap();
        outbox.enqueue(&pending).unwrap();
        outbox.enqueue(&dead_letter).unwrap();
        outbox.enqueue(&dead_letter).unwrap();
        outbox.archive(Some(2), None, true).unwrap();

        let health = outbox.health().unwrap();
        assert_eq!(health.status, EventDeliveryHealthStatus::AttentionRequired);
        assert_eq!(health.schema_version, EVENT_HEALTH_SCHEMA_VERSION);
        assert_eq!(health.pending.count, 1);
        assert_eq!(health.pending.total_attempts, 1);
        assert_eq!(health.pending.max_attempts, 1);
        assert_eq!(health.pending.by_event_type["drift.checked"], 1);
        assert_eq!(health.pending.by_event_type["mutation.failed"], 0);
        assert_eq!(health.pending.by_source["watcher"], 1);
        assert!(health.pending.oldest_queued_at.is_some());
        assert!(health.pending.latest_attempted_at.is_some());
        assert_eq!(health.dead_letters.count, 1);
        assert_eq!(health.dead_letters.total_attempts, 2);
        assert_eq!(health.dead_letters.max_attempts, 2);
        assert_eq!(health.dead_letters.by_event_type["mutation.failed"], 1);
        assert_eq!(health.dead_letters.by_source["governance"], 1);
        assert!(health.dead_letters.oldest_dead_lettered_at.is_some());
        assert!(health.dead_letters.latest_dead_lettered_at.is_some());

        let encoded = serde_json::to_string(&health).unwrap();
        assert!(!encoded.contains("must-not-leak"));
        assert!(!encoded.contains(&pending.id));
        assert!(!encoded.contains(&dead_letter.id));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn empty_outbox_health_is_healthy_with_stable_zero_buckets() {
        let outbox = EventOutbox::new(temp_root("outbox-health-empty"));
        let health = outbox.health().unwrap();
        assert_eq!(health.status, EventDeliveryHealthStatus::Healthy);
        assert_eq!(health.pending.count, 0);
        assert_eq!(health.dead_letters.count, 0);
        assert_eq!(health.pending.by_event_type.len(), 5);
        assert_eq!(health.dead_letters.by_source.len(), 3);
        assert!(health.pending.oldest_queued_at.is_none());
        assert!(health.dead_letters.oldest_dead_lettered_at.is_none());
    }

    #[test]
    fn archive_requires_a_retention_threshold() {
        let outbox = EventOutbox::new(temp_root("outbox-missing-retention"));
        assert!(outbox.archive(None, None, false).is_err());
        assert!(outbox.archive(Some(0), None, false).is_err());
    }
}
