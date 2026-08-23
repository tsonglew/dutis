use crate::application::normalize_extension;
use crate::association::{AssociationKind, AssociationTarget, HandlerRole};
use crate::events::{emit_best_effort, EventSource, EventType};
use crate::planner::{ApplyReport, AssociationPlan, PlanAction};
use crate::snapshot::{apply_plan_with_snapshot, SnapshotReason, SnapshotStore};
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

pub const POLICY_VERSION: u32 = 1;
pub const AUDIT_SCHEMA_VERSION: u32 = 1;
const POLICY_FILE_ENV: &str = "DUTIS_POLICY_FILE";

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalMode {
    #[default]
    Explicit,
    Token,
    Deny,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct Policy {
    pub version: u32,
    pub approval_mode: ApprovalMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_extensions: Option<BTreeSet<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_kinds: Option<BTreeSet<AssociationKind>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_applications: Option<BTreeSet<String>>,
    pub protected_associations: BTreeMap<String, String>,
    pub protected_handlers: Vec<ProtectedHandler>,
    pub recommendations: RecommendationPreferences,
    #[serde(skip_serializing)]
    approval_token_sha256: Option<String>,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize)]
pub struct RecommendationPreferences {
    pub preferred_applications: Vec<String>,
    pub extensions: BTreeMap<String, Vec<String>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPolicy {
    version: u32,
    #[serde(default)]
    approval_mode: ApprovalMode,
    allowed_extensions: Option<Vec<String>>,
    allowed_kinds: Option<Vec<AssociationKind>>,
    allowed_applications: Option<Vec<String>>,
    #[serde(default)]
    protected_associations: BTreeMap<String, String>,
    #[serde(default)]
    protected_handlers: Vec<ProtectedHandler>,
    #[serde(default)]
    recommendations: RawRecommendationPreferences,
    approval_token_sha256: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRecommendationPreferences {
    #[serde(default)]
    preferred_applications: Vec<String>,
    #[serde(default)]
    extensions: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectedHandler {
    pub kind: AssociationKind,
    pub identifier: String,
    #[serde(default)]
    pub role: HandlerRole,
    pub application: String,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            version: POLICY_VERSION,
            approval_mode: ApprovalMode::Explicit,
            allowed_extensions: None,
            allowed_kinds: None,
            allowed_applications: None,
            protected_associations: BTreeMap::new(),
            protected_handlers: Vec::new(),
            recommendations: RecommendationPreferences::default(),
            approval_token_sha256: None,
        }
    }
}

impl Policy {
    pub fn parse(contents: &str) -> Result<Self> {
        let raw: RawPolicy = toml::from_str(contents).context("failed to parse policy TOML")?;
        if raw.version != POLICY_VERSION {
            bail!(
                "unsupported policy version {}; expected {}",
                raw.version,
                POLICY_VERSION
            );
        }
        let allowed_extensions = raw
            .allowed_extensions
            .map(normalize_extension_set)
            .transpose()?;
        let allowed_applications = raw
            .allowed_applications
            .map(|values| normalize_nonempty_set(values, "allowed application"))
            .transpose()?;
        let recommendations = normalize_recommendation_preferences(raw.recommendations)?;
        let allowed_kinds = raw.allowed_kinds.map(|values| values.into_iter().collect());
        let mut protected_associations = BTreeMap::new();
        for (input_extension, input_bundle_id) in raw.protected_associations {
            let extension = normalize_extension(&input_extension)?;
            let bundle_id = input_bundle_id.trim();
            if bundle_id.is_empty() {
                bail!("protected application for .{extension} cannot be empty");
            }
            if protected_associations
                .insert(extension.clone(), bundle_id.to_owned())
                .is_some()
            {
                bail!("duplicate protected extension .{extension}");
            }
        }
        let mut seen_handlers = protected_associations
            .keys()
            .map(|extension| {
                AssociationTarget::extension(extension).expect("normalized extension is valid")
            })
            .collect::<BTreeSet<_>>();
        let mut protected_handlers = Vec::with_capacity(raw.protected_handlers.len());
        for handler in raw.protected_handlers {
            let target = AssociationTarget::new(handler.kind, &handler.identifier, handler.role)?;
            let application = handler.application.trim();
            if application.is_empty() {
                bail!("protected application for {target} cannot be empty");
            }
            if !seen_handlers.insert(target.clone()) {
                bail!("duplicate protected handler {target}");
            }
            protected_handlers.push(ProtectedHandler {
                kind: target.kind,
                identifier: target.identifier,
                role: target.role,
                application: application.to_owned(),
            });
        }
        protected_handlers.sort_by(|left, right| {
            (&left.kind, &left.identifier, &left.role).cmp(&(
                &right.kind,
                &right.identifier,
                &right.role,
            ))
        });
        if raw.approval_mode == ApprovalMode::Token {
            let digest = raw.approval_token_sha256.as_deref().ok_or_else(|| {
                anyhow!("approval_token_sha256 is required when approval_mode is 'token'")
            })?;
            if digest.len() != 64
                || !digest
                    .chars()
                    .all(|character| character.is_ascii_hexdigit())
            {
                bail!("approval_token_sha256 must be a 64-character SHA-256 hex digest");
            }
        }
        Ok(Self {
            version: raw.version,
            approval_mode: raw.approval_mode,
            allowed_extensions,
            allowed_kinds,
            allowed_applications,
            protected_associations,
            protected_handlers,
            recommendations,
            approval_token_sha256: raw
                .approval_token_sha256
                .map(|digest| digest.to_ascii_lowercase()),
        })
    }

    pub fn assess(&self, plan: &AssociationPlan) -> PolicyAssessment {
        let mut violations = Vec::new();
        if self.approval_mode == ApprovalMode::Deny {
            violations.push("policy denies all mutations".to_owned());
        }
        if plan.has_unresolved() {
            violations.push(format!(
                "plan contains {} unresolved association(s)",
                plan.summary.unresolved
            ));
        }
        for entry in plan
            .entries
            .iter()
            .filter(|entry| entry.action == PlanAction::Change)
        {
            if self
                .allowed_kinds
                .as_ref()
                .is_some_and(|allowed| !allowed.contains(&entry.kind))
            {
                violations.push(format!("association kind {:?} is not allowed", entry.kind));
            }
            if entry.kind == AssociationKind::Extension
                && self
                    .allowed_extensions
                    .as_ref()
                    .is_some_and(|allowed| !allowed.contains(&entry.extension))
            {
                violations.push(format!("extension .{} is not allowed", entry.extension));
            }
            let target_bundle_id = entry
                .target
                .as_ref()
                .map(|target| target.bundle_id.as_str());
            if self.allowed_applications.as_ref().is_some_and(|allowed| {
                target_bundle_id.is_none_or(|bundle_id| !allowed.contains(bundle_id))
            }) {
                violations.push(format!(
                    "target application for {} is not allowed",
                    entry.association()
                ));
            }
            if entry.kind == AssociationKind::Extension {
                if let Some(required) = self.protected_associations.get(&entry.extension) {
                    if target_bundle_id != Some(required.as_str()) {
                        violations.push(format!(
                            "protected association .{} must remain assigned to {}",
                            entry.extension, required
                        ));
                    }
                }
            }
            if let Some(required) = self.protected_handlers.iter().find(|handler| {
                handler.kind == entry.kind
                    && handler.identifier == entry.extension
                    && handler.role == entry.role
            }) {
                if target_bundle_id != Some(required.application.as_str()) {
                    violations.push(format!(
                        "protected handler {} must remain assigned to {}",
                        entry.association(),
                        required.application
                    ));
                }
            }
        }
        PolicyAssessment {
            allowed: violations.is_empty() && self.approval_mode != ApprovalMode::Deny,
            approval_mode: self.approval_mode,
            violations,
        }
    }

    fn authorize(&self, plan: &AssociationPlan, request: &MutationRequest) -> PolicyAssessment {
        let mut assessment = self.assess(plan);
        if request.requester.trim().is_empty() {
            assessment
                .violations
                .push("requester must be a non-empty identifier".to_owned());
        }
        match self.approval_mode {
            ApprovalMode::Explicit if !request.explicit_approval => assessment
                .violations
                .push("explicit approval is required".to_owned()),
            ApprovalMode::Token => {
                let authorized = self
                    .approval_token_sha256
                    .as_deref()
                    .zip(request.approval_token.as_deref())
                    .is_some_and(|(expected, provided)| token_digest_matches(expected, provided));
                if !authorized {
                    assessment
                        .violations
                        .push("policy approval token is missing or invalid".to_owned());
                }
            }
            ApprovalMode::Deny => {}
            ApprovalMode::Explicit => {}
        }
        assessment.allowed = assessment.violations.is_empty();
        assessment
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct PolicyAssessment {
    pub allowed: bool,
    pub approval_mode: ApprovalMode,
    pub violations: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct LoadedPolicy {
    pub path: PathBuf,
    pub exists: bool,
    pub digest: String,
    pub policy: Policy,
}

impl LoadedPolicy {
    pub fn from_environment() -> Result<Self> {
        let path = if let Some(path) =
            std::env::var_os(POLICY_FILE_ENV).filter(|value| !value.is_empty())
        {
            PathBuf::from(path)
        } else {
            SnapshotStore::from_environment()?
                .root()
                .join("policy.toml")
        };
        Self::load(path)
    }

    pub fn load(path: PathBuf) -> Result<Self> {
        let (policy, exists) = if path.exists() {
            let contents = fs::read_to_string(&path)
                .with_context(|| format!("failed to read policy {}", path.display()))?;
            (
                Policy::parse(&contents)
                    .with_context(|| format!("invalid policy {}", path.display()))?,
                true,
            )
        } else {
            (Policy::default(), false)
        };
        let digest = policy_digest(&policy)?;
        Ok(Self {
            path,
            exists,
            digest,
            policy,
        })
    }

    pub fn summary(&self) -> PolicySummary {
        PolicySummary {
            path: self.path.clone(),
            exists: self.exists,
            digest: self.digest.clone(),
            version: self.policy.version,
            approval_mode: self.policy.approval_mode,
            approval_token_configured: self.policy.approval_token_sha256.is_some(),
            allowed_extensions: self.policy.allowed_extensions.clone(),
            allowed_kinds: self.policy.allowed_kinds.clone(),
            allowed_applications: self.policy.allowed_applications.clone(),
            protected_associations: self.policy.protected_associations.clone(),
            protected_handlers: self.policy.protected_handlers.clone(),
            recommendations: self.policy.recommendations.clone(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct PolicySummary {
    pub path: PathBuf,
    pub exists: bool,
    pub digest: String,
    pub version: u32,
    pub approval_mode: ApprovalMode,
    pub approval_token_configured: bool,
    pub allowed_extensions: Option<BTreeSet<String>>,
    pub allowed_kinds: Option<BTreeSet<AssociationKind>>,
    pub allowed_applications: Option<BTreeSet<String>>,
    pub protected_associations: BTreeMap<String, String>,
    pub protected_handlers: Vec<ProtectedHandler>,
    pub recommendations: RecommendationPreferences,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationChannel {
    Cli,
    Interactive,
    Mcp,
    Watcher,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationOperation {
    Set,
    Apply,
    Rollback,
    Remediate,
}

#[derive(Debug, Clone)]
pub struct MutationRequest {
    pub requester: String,
    pub channel: MutationChannel,
    pub operation: MutationOperation,
    pub explicit_approval: bool,
    pub approval_token: Option<String>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditOutcome {
    Pending,
    Succeeded,
    PartialFailure,
    Denied,
    FailedBeforeMutation,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct VerificationSummary {
    pub succeeded: bool,
    pub applied: usize,
    pub skipped: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct MutationAuditRecord {
    pub schema_version: u32,
    pub id: String,
    pub timestamp: String,
    pub requester: String,
    pub channel: MutationChannel,
    pub operation: MutationOperation,
    pub policy_digest: String,
    pub approval_mode: ApprovalMode,
    pub plan_digest: String,
    pub plan: AssociationPlan,
    pub outcome: AuditOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safety_snapshot_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<ApplyReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification: Option<VerificationSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AuditStore {
    root: PathBuf,
}

impl AuditStore {
    pub fn from_environment() -> Result<Self> {
        Ok(Self::new(
            SnapshotStore::from_environment()?.root().join("audit"),
        ))
    }

    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn save(&self, record: &MutationAuditRecord) -> Result<PathBuf> {
        validate_record_id(&record.id)?;
        fs::create_dir_all(&self.root)
            .with_context(|| format!("failed to create {}", self.root.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.root, fs::Permissions::from_mode(0o700))?;
        }
        let destination = self.root.join(format!("{}.json", record.id));
        let temporary = self.root.join(format!(
            ".{}.{}.{}.tmp",
            record.id,
            std::process::id(),
            OffsetDateTime::now_utc().unix_timestamp_nanos()
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
        fs::rename(&temporary, &destination)
            .with_context(|| format!("failed to store audit record {}", destination.display()))?;
        Ok(destination)
    }

    pub fn history(&self) -> Result<Vec<MutationAuditRecord>> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }
        let mut records = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let path = entry?.path();
            if path.extension().is_none_or(|extension| extension != "json") {
                continue;
            }
            let record: MutationAuditRecord =
                serde_json::from_reader(BufReader::new(fs::File::open(&path)?))
                    .with_context(|| format!("failed to parse audit record {}", path.display()))?;
            let filename_id = path.file_stem().and_then(|value| value.to_str());
            validate_audit_record(&record, filename_id)?;
            records.push(record);
        }
        records.sort_by(|left: &MutationAuditRecord, right| right.id.cmp(&left.id));
        Ok(records)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct GovernedMutation {
    pub audit_id: String,
    pub safety_snapshot_id: Option<String>,
    #[serde(flatten)]
    pub report: ApplyReport,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum GovernanceErrorKind {
    PolicyDenied,
    AuditFailed,
    SnapshotFailed,
}

#[derive(Debug)]
pub struct GovernanceError {
    kind: GovernanceErrorKind,
    message: String,
    audit_id: Option<String>,
    violations: Vec<String>,
}

impl GovernanceError {
    pub fn kind(&self) -> GovernanceErrorKind {
        self.kind
    }

    pub fn audit_id(&self) -> Option<&str> {
        self.audit_id.as_deref()
    }

    pub fn violations(&self) -> &[String] {
        &self.violations
    }
}

impl fmt::Display for GovernanceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for GovernanceError {}

pub fn execute_governed_plan<F>(
    plan: &AssociationPlan,
    reason: SnapshotReason,
    request: &MutationRequest,
    apply: F,
) -> std::result::Result<GovernedMutation, GovernanceError>
where
    F: FnMut(&AssociationTarget, &str) -> Result<()>,
{
    let policy = LoadedPolicy::from_environment().map_err(|error| GovernanceError {
        kind: GovernanceErrorKind::PolicyDenied,
        message: format!("failed to load mutation policy: {error:#}"),
        audit_id: None,
        violations: vec![format!("policy could not be loaded: {error:#}")],
    })?;
    let audit_store = AuditStore::from_environment().map_err(|error| GovernanceError {
        kind: GovernanceErrorKind::AuditFailed,
        message: format!("failed to resolve audit storage: {error:#}"),
        audit_id: None,
        violations: Vec::new(),
    })?;
    let snapshot_store = SnapshotStore::from_environment().map_err(|error| GovernanceError {
        kind: GovernanceErrorKind::SnapshotFailed,
        message: format!("failed to resolve snapshot storage: {error:#}"),
        audit_id: None,
        violations: Vec::new(),
    })?;
    execute_governed_plan_with(
        &policy,
        &audit_store,
        &snapshot_store,
        plan,
        reason,
        request,
        apply,
    )
}

fn execute_governed_plan_with<F>(
    loaded_policy: &LoadedPolicy,
    audit_store: &AuditStore,
    snapshot_store: &SnapshotStore,
    plan: &AssociationPlan,
    reason: SnapshotReason,
    request: &MutationRequest,
    apply: F,
) -> std::result::Result<GovernedMutation, GovernanceError>
where
    F: FnMut(&AssociationTarget, &str) -> Result<()>,
{
    let assessment = loaded_policy.policy.authorize(plan, request);
    let mut record = new_audit_record(loaded_policy, plan, request);
    if !assessment.allowed {
        record.outcome = AuditOutcome::Denied;
        record.error = Some(assessment.violations.join("; "));
        audit_store.save(&record).map_err(|error| GovernanceError {
            kind: GovernanceErrorKind::AuditFailed,
            message: format!("policy denied mutation and audit storage failed: {error:#}"),
            audit_id: Some(record.id.clone()),
            violations: assessment.violations.clone(),
        })?;
        emit_best_effort(EventType::MutationDenied, EventSource::Governance, &record);
        return Err(GovernanceError {
            kind: GovernanceErrorKind::PolicyDenied,
            message: format!(
                "policy denied mutation: {}",
                assessment.violations.join("; ")
            ),
            audit_id: Some(record.id),
            violations: assessment.violations,
        });
    }

    audit_store.save(&record).map_err(|error| GovernanceError {
        kind: GovernanceErrorKind::AuditFailed,
        message: format!("failed to create pending audit record; no changes were made: {error:#}"),
        audit_id: Some(record.id.clone()),
        violations: Vec::new(),
    })?;
    emit_best_effort(EventType::MutationPending, EventSource::Governance, &record);

    let protected = match apply_plan_with_snapshot(snapshot_store, plan, reason, apply) {
        Ok(protected) => protected,
        Err(error) => {
            record.outcome = AuditOutcome::FailedBeforeMutation;
            record.error = Some(format!("{error:#}"));
            let _ = audit_store.save(&record);
            emit_best_effort(EventType::MutationFailed, EventSource::Governance, &record);
            return Err(GovernanceError {
                kind: GovernanceErrorKind::SnapshotFailed,
                message: format!(
                    "failed to store safety snapshot; no changes were made: {error:#}"
                ),
                audit_id: Some(record.id),
                violations: Vec::new(),
            });
        }
    };
    let verification = VerificationSummary {
        succeeded: protected.report.failed == 0,
        applied: protected.report.applied,
        skipped: protected.report.skipped,
        failed: protected.report.failed,
    };
    record.outcome = if verification.succeeded {
        AuditOutcome::Succeeded
    } else {
        AuditOutcome::PartialFailure
    };
    record.safety_snapshot_id = protected
        .safety_snapshot
        .as_ref()
        .map(|snapshot| snapshot.id.clone());
    record.result = Some(protected.report.clone());
    record.verification = Some(verification);
    audit_store.save(&record).map_err(|error| GovernanceError {
        kind: GovernanceErrorKind::AuditFailed,
        message: format!(
            "mutation completed but final audit record could not be stored; pending record {} remains: {error:#}",
            record.id
        ),
        audit_id: Some(record.id.clone()),
        violations: Vec::new(),
    })?;
    emit_best_effort(
        EventType::MutationCompleted,
        EventSource::Governance,
        &record,
    );

    Ok(GovernedMutation {
        audit_id: record.id,
        safety_snapshot_id: record.safety_snapshot_id,
        report: protected.report,
    })
}

fn new_audit_record(
    policy: &LoadedPolicy,
    plan: &AssociationPlan,
    request: &MutationRequest,
) -> MutationAuditRecord {
    let now = OffsetDateTime::now_utc();
    let timestamp = now
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned());
    let hash = Sha256::digest(
        format!(
            "{}:{}:{}:{:?}",
            now.unix_timestamp_nanos(),
            request.requester,
            plan.digest,
            request.operation
        )
        .as_bytes(),
    );
    let suffix = hash
        .iter()
        .take(6)
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    MutationAuditRecord {
        schema_version: AUDIT_SCHEMA_VERSION,
        id: format!("{}-{suffix}", now.unix_timestamp_nanos()),
        timestamp,
        requester: request.requester.trim().to_owned(),
        channel: request.channel,
        operation: request.operation,
        policy_digest: policy.digest.clone(),
        approval_mode: policy.policy.approval_mode,
        plan_digest: plan.digest.clone(),
        plan: plan.clone(),
        outcome: AuditOutcome::Pending,
        safety_snapshot_id: None,
        result: None,
        verification: None,
        error: None,
    }
}

fn policy_digest(policy: &Policy) -> Result<String> {
    let material = serde_json::to_vec(&(
        policy,
        policy.approval_token_sha256.as_deref().unwrap_or(""),
    ))?;
    Ok(Sha256::digest(material)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn token_digest_matches(expected: &str, provided: &str) -> bool {
    let actual = Sha256::digest(provided.as_bytes());
    let actual = actual
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    expected
        .as_bytes()
        .iter()
        .zip(actual.as_bytes())
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

fn normalize_extension_set(values: Vec<String>) -> Result<BTreeSet<String>> {
    let mut normalized = BTreeSet::new();
    for value in values {
        let extension = normalize_extension(&value)?;
        if !normalized.insert(extension.clone()) {
            bail!("duplicate allowed extension .{extension}");
        }
    }
    Ok(normalized)
}

fn normalize_nonempty_set(values: Vec<String>, label: &str) -> Result<BTreeSet<String>> {
    let mut normalized = BTreeSet::new();
    for value in values {
        let value = value.trim();
        if value.is_empty() {
            bail!("{label} cannot be empty");
        }
        if !normalized.insert(value.to_owned()) {
            bail!("duplicate {label} '{value}'");
        }
    }
    Ok(normalized)
}

fn normalize_recommendation_preferences(
    raw: RawRecommendationPreferences,
) -> Result<RecommendationPreferences> {
    let preferred_applications = normalize_nonempty_ordered(
        raw.preferred_applications,
        "preferred recommendation application",
    )?;
    let mut extensions = BTreeMap::new();
    for (input_extension, applications) in raw.extensions {
        let extension = normalize_extension(&input_extension)?;
        let applications = normalize_nonempty_ordered(
            applications,
            &format!("recommendation application for .{extension}"),
        )?;
        if extensions.insert(extension.clone(), applications).is_some() {
            bail!("duplicate recommendation extension .{extension}");
        }
    }
    Ok(RecommendationPreferences {
        preferred_applications,
        extensions,
    })
}

fn normalize_nonempty_ordered(values: Vec<String>, label: &str) -> Result<Vec<String>> {
    let mut seen = BTreeSet::new();
    let mut normalized = Vec::with_capacity(values.len());
    for value in values {
        let value = value.trim();
        if value.is_empty() {
            bail!("{label} cannot be empty");
        }
        if !seen.insert(value.to_owned()) {
            bail!("duplicate {label} '{value}'");
        }
        normalized.push(value.to_owned());
    }
    Ok(normalized)
}

fn validate_record_id(id: &str) -> Result<()> {
    if id.is_empty()
        || !id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
    {
        bail!("invalid audit record ID '{id}'");
    }
    Ok(())
}

fn validate_audit_record(record: &MutationAuditRecord, expected_id: Option<&str>) -> Result<()> {
    if record.schema_version != AUDIT_SCHEMA_VERSION {
        bail!(
            "unsupported audit schema version {}; expected {}",
            record.schema_version,
            AUDIT_SCHEMA_VERSION
        );
    }
    validate_record_id(&record.id)?;
    if expected_id.is_some_and(|id| id != record.id) {
        bail!("audit record ID does not match its filename");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planner::{assemble_plan, PlanEntry, PlannedApplication};
    use crate::system::DefaultApplication;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("dutis-{label}-{}-{unique}", std::process::id()))
    }

    fn plan(extension: &str, target: &str) -> AssociationPlan {
        assemble_plan(
            1,
            vec![PlanEntry {
                kind: crate::association::AssociationKind::Extension,
                role: crate::association::HandlerRole::All,
                extension: extension.to_owned(),
                selector: target.to_owned(),
                current: Some(DefaultApplication {
                    kind: crate::association::AssociationKind::Extension,
                    role: crate::association::HandlerRole::All,
                    extension: extension.to_owned(),
                    name: None,
                    path: None,
                    bundle_id: "com.example.Old".to_owned(),
                }),
                target: Some(PlannedApplication {
                    name: "Target".to_owned(),
                    path: PathBuf::from("/Applications/Target.app"),
                    bundle_id: target.to_owned(),
                }),
                action: PlanAction::Change,
                reason: None,
            }],
        )
        .unwrap()
    }

    fn loaded(policy: Policy, root: &Path) -> LoadedPolicy {
        LoadedPolicy {
            path: root.join("policy.toml"),
            exists: true,
            digest: policy_digest(&policy).unwrap(),
            policy,
        }
    }

    fn request(token: Option<&str>) -> MutationRequest {
        MutationRequest {
            requester: "test-agent".to_owned(),
            channel: MutationChannel::Mcp,
            operation: MutationOperation::Apply,
            explicit_approval: true,
            approval_token: token.map(str::to_owned),
        }
    }

    #[test]
    fn parses_and_normalizes_policy() {
        let policy = Policy::parse(
            r#"
                version = 1
                approval_mode = "explicit"
                allowed_extensions = [".MD", "txt"]
                allowed_applications = ["com.example.Editor"]

                [protected_associations]
                ".PDF" = "com.apple.Preview"

                [recommendations]
                preferred_applications = [" com.example.Editor "]

                [recommendations.extensions]
                ".MD" = ["com.example.TeamEditor", "com.example.Editor"]
            "#,
        )
        .unwrap();
        assert!(policy.allowed_extensions.unwrap().contains("md"));
        assert_eq!(policy.protected_associations["pdf"], "com.apple.Preview");
        assert_eq!(
            policy.recommendations.preferred_applications,
            ["com.example.Editor"]
        );
        assert_eq!(
            policy.recommendations.extensions["md"],
            ["com.example.TeamEditor", "com.example.Editor"]
        );
    }

    #[test]
    fn rejects_duplicate_or_empty_recommendation_preferences() {
        assert!(Policy::parse(
            "version = 1\n[recommendations]\npreferred_applications = ['com.example.App', 'com.example.App']\n"
        )
        .unwrap_err()
        .to_string()
        .contains("duplicate preferred recommendation application"));
        assert!(
            Policy::parse("version = 1\n[recommendations.extensions]\nmd = ['   ']\n")
                .unwrap_err()
                .to_string()
                .contains("cannot be empty")
        );
    }

    #[test]
    fn policy_denies_disallowed_targets_before_mutation() {
        let root = temp_root("policy-denial");
        let policy = Policy::parse(
            r#"
                version = 1
                allowed_extensions = ["md"]
                allowed_applications = ["com.example.Allowed"]
            "#,
        )
        .unwrap();
        let audit_store = AuditStore::new(root.join("audit"));
        let snapshot_store = SnapshotStore::new(root.join("state"));
        let result = execute_governed_plan_with(
            &loaded(policy, &root),
            &audit_store,
            &snapshot_store,
            &plan("md", "com.example.Denied"),
            SnapshotReason::BeforeApply,
            &request(None),
            |_, _| panic!("policy denial reached mutation"),
        );
        assert_eq!(
            result.unwrap_err().kind(),
            GovernanceErrorKind::PolicyDenied
        );
        let records = audit_store.history().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].outcome, AuditOutcome::Denied);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn token_policy_requires_the_matching_digest() {
        let digest = Sha256::digest(b"correct-token")
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let policy = Policy::parse(&format!(
            "version = 1\napproval_mode = 'token'\napproval_token_sha256 = '{digest}'\n"
        ))
        .unwrap();
        let assessment =
            policy.authorize(&plan("md", "com.example.Editor"), &request(Some("wrong")));
        assert!(!assessment.allowed);
        let assessment = policy.authorize(
            &plan("md", "com.example.Editor"),
            &request(Some("correct-token")),
        );
        assert!(assessment.allowed);
    }

    #[test]
    fn protected_association_allows_restoration_but_denies_replacement() {
        let policy =
            Policy::parse("version = 1\n[protected_associations]\nmd = 'com.example.Protected'\n")
                .unwrap();
        assert!(policy.assess(&plan("md", "com.example.Protected")).allowed);
        let denied = policy.assess(&plan("md", "com.example.Other"));
        assert!(!denied.allowed);
        assert!(denied.violations[0].contains("must remain assigned"));
    }

    #[test]
    fn typed_policy_restricts_kinds_and_protects_role_specific_handlers() {
        let policy = Policy::parse(
            r#"
                version = 1
                allowed_kinds = ["uti"]

                [[protected_handlers]]
                kind = "uti"
                identifier = "Public.HTML"
                role = "viewer"
                application = "com.example.Browser"
            "#,
        )
        .unwrap();
        let mut typed = plan("public.html", "com.example.Other");
        typed.entries[0].kind = AssociationKind::Uti;
        typed.entries[0].role = HandlerRole::Viewer;
        let denied = policy.assess(&typed);
        assert!(!denied.allowed);
        assert!(denied.violations[0].contains("must remain assigned"));

        let extension = plan("md", "com.example.Browser");
        let denied = policy.assess(&extension);
        assert!(!denied.allowed);
        assert!(denied.violations[0].contains("kind"));
    }

    #[test]
    fn successful_mutation_persists_plan_result_and_verification() {
        let root = temp_root("audit-success");
        let policy = Policy::default();
        let audit_store = AuditStore::new(root.join("audit"));
        let snapshot_store = SnapshotStore::new(root.join("state"));
        let result = execute_governed_plan_with(
            &loaded(policy, &root),
            &audit_store,
            &snapshot_store,
            &plan("md", "com.example.Editor"),
            SnapshotReason::BeforeApply,
            &request(None),
            |_, _| Ok(()),
        )
        .unwrap();
        assert_eq!(result.report.applied, 1);
        let records = audit_store.history().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].requester, "test-agent");
        assert_eq!(records[0].outcome, AuditOutcome::Succeeded);
        assert_eq!(records[0].result.as_ref().unwrap().applied, 1);
        assert!(records[0].verification.as_ref().unwrap().succeeded);
        assert!(records[0].safety_snapshot_id.is_some());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let record_path = audit_store.root().join(format!("{}.json", result.audit_id));
            assert_eq!(
                fs::metadata(record_path).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(
                fs::metadata(audit_store.root())
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn repository_policy_example_uses_the_current_schema() {
        let policy = Policy::parse(include_str!("../dutis.policy.example.toml")).unwrap();
        assert_eq!(policy.version, POLICY_VERSION);
        assert!(!policy.protected_associations.is_empty());
    }

    #[test]
    fn partial_failure_is_persisted_with_failed_verification() {
        let root = temp_root("audit-partial");
        let audit_store = AuditStore::new(root.join("audit"));
        let result = execute_governed_plan_with(
            &loaded(Policy::default(), &root),
            &audit_store,
            &SnapshotStore::new(root.join("state")),
            &plan("md", "com.example.Editor"),
            SnapshotReason::BeforeApply,
            &request(None),
            |_, _| bail!("simulated verification failure"),
        )
        .unwrap();
        assert_eq!(result.report.failed, 1);
        let record = audit_store.history().unwrap().pop().unwrap();
        assert_eq!(record.outcome, AuditOutcome::PartialFailure);
        assert!(!record.verification.unwrap().succeeded);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn audit_storage_failure_prevents_mutation() {
        let root = temp_root("audit-failure");
        fs::write(&root, "not a directory").unwrap();
        let result = execute_governed_plan_with(
            &loaded(Policy::default(), Path::new("/policy")),
            &AuditStore::new(root.clone()),
            &SnapshotStore::new(temp_root("unused-snapshot")),
            &plan("md", "com.example.Editor"),
            SnapshotReason::BeforeApply,
            &request(None),
            |_, _| panic!("mutation ran without an audit record"),
        );
        assert_eq!(result.unwrap_err().kind(), GovernanceErrorKind::AuditFailed);
        fs::remove_file(root).unwrap();
    }
}
