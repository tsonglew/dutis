use crate::application::{normalize_extension, resolve_app, Application};
use crate::association::{AssociationKind, AssociationTarget, HandlerRole};
use crate::planner::{
    apply_plan, assemble_plan, ApplyReport, AssociationPlan, PlanAction, PlanEntry,
    PlannedApplication,
};
use crate::system::DefaultApplication;
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

pub const SNAPSHOT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotReason {
    Manual,
    BeforeApply,
    BeforeRollback,
    BeforeRemediation,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct SnapshotAssociation {
    #[serde(default)]
    pub kind: AssociationKind,
    #[serde(default)]
    pub role: HandlerRole,
    /// Normalized identifier. The legacy field name preserves snapshot compatibility.
    pub extension: String,
    pub default: Option<DefaultApplication>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub schema_version: u32,
    pub id: String,
    pub created_at: String,
    pub reason: SnapshotReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_plan_digest: Option<String>,
    pub associations: Vec<SnapshotAssociation>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct SnapshotSummary {
    pub id: String,
    pub created_at: String,
    pub reason: SnapshotReason,
    pub source_plan_digest: Option<String>,
    pub associations: usize,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ProtectedApply {
    pub safety_snapshot: Option<Snapshot>,
    pub report: ApplyReport,
}

impl From<&Snapshot> for SnapshotSummary {
    fn from(snapshot: &Snapshot) -> Self {
        Self {
            id: snapshot.id.clone(),
            created_at: snapshot.created_at.clone(),
            reason: snapshot.reason,
            source_plan_digest: snapshot.source_plan_digest.clone(),
            associations: snapshot.associations.len(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SnapshotStore {
    root: PathBuf,
}

impl SnapshotStore {
    pub fn from_environment() -> Result<Self> {
        if let Some(path) = std::env::var_os("DUTIS_STATE_DIR").filter(|value| !value.is_empty()) {
            return Ok(Self::new(path));
        }
        let home = std::env::var_os("HOME")
            .ok_or_else(|| anyhow!("HOME is not set; set DUTIS_STATE_DIR explicitly"))?;
        Ok(Self::new(
            PathBuf::from(home).join("Library/Application Support/dutis"),
        ))
    }

    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn create(
        &self,
        reason: SnapshotReason,
        source_plan_digest: Option<String>,
        associations: Vec<SnapshotAssociation>,
    ) -> Result<Snapshot> {
        let associations = normalize_associations(associations)?;
        let now = OffsetDateTime::now_utc();
        let created_at = now.format(&Rfc3339)?;
        let hash_material = serde_json::to_vec(&(
            SNAPSHOT_SCHEMA_VERSION,
            &created_at,
            reason,
            &source_plan_digest,
            &associations,
        ))?;
        let hash = Sha256::digest(hash_material);
        let hash_prefix = hash
            .iter()
            .take(6)
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let id = format!("{}-{hash_prefix}", now.unix_timestamp_nanos());
        let snapshot = Snapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            id,
            created_at,
            reason,
            source_plan_digest,
            associations,
        };

        let directory = self.snapshots_directory();
        fs::create_dir_all(&directory)
            .with_context(|| format!("failed to create {}", directory.display()))?;
        let destination = self.snapshot_path(&snapshot.id)?;
        let temporary = directory.join(format!(".{}.{}.tmp", snapshot.id, std::process::id()));

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
        serde_json::to_writer_pretty(&mut writer, &snapshot)?;
        writer.write_all(b"\n")?;
        writer.flush()?;
        writer.get_ref().sync_all()?;
        fs::rename(&temporary, &destination).with_context(|| {
            format!(
                "failed to atomically store snapshot {}",
                destination.display()
            )
        })?;
        Ok(snapshot)
    }

    pub fn load(&self, id: &str) -> Result<Snapshot> {
        let path = self.snapshot_path(id)?;
        let file = fs::File::open(&path)
            .with_context(|| format!("failed to open snapshot {}", path.display()))?;
        let snapshot: Snapshot = serde_json::from_reader(BufReader::new(file))
            .with_context(|| format!("failed to parse snapshot {}", path.display()))?;
        validate_snapshot(&snapshot, Some(id))?;
        Ok(snapshot)
    }

    pub fn history(&self) -> Result<Vec<SnapshotSummary>> {
        let directory = self.snapshots_directory();
        if !directory.exists() {
            return Ok(Vec::new());
        }
        let mut snapshots = Vec::new();
        for entry in fs::read_dir(&directory)
            .with_context(|| format!("failed to read {}", directory.display()))?
        {
            let path = entry?.path();
            if path.extension().is_none_or(|extension| extension != "json") {
                continue;
            }
            let file = fs::File::open(&path)?;
            let snapshot: Snapshot = serde_json::from_reader(BufReader::new(file))
                .with_context(|| format!("failed to parse snapshot {}", path.display()))?;
            validate_snapshot(&snapshot, None)?;
            snapshots.push(SnapshotSummary::from(&snapshot));
        }
        snapshots.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        Ok(snapshots)
    }

    pub fn snapshot_path(&self, id: &str) -> Result<PathBuf> {
        validate_snapshot_id(id)?;
        Ok(self.snapshots_directory().join(format!("{id}.json")))
    }

    fn snapshots_directory(&self) -> PathBuf {
        self.root.join("snapshots")
    }
}

pub fn capture_associations<F, I>(extensions: I, mut query: F) -> Result<Vec<SnapshotAssociation>>
where
    I: IntoIterator,
    I::Item: AsRef<str>,
    F: FnMut(&str) -> Result<Option<DefaultApplication>>,
{
    let mut normalized = BTreeSet::new();
    for extension in extensions {
        normalized.insert(normalize_extension(extension.as_ref())?);
    }
    normalized
        .into_iter()
        .map(|extension| {
            let default = query(&extension)?;
            Ok(SnapshotAssociation {
                kind: AssociationKind::Extension,
                role: HandlerRole::All,
                extension,
                default,
            })
        })
        .collect()
}

pub fn capture_targets<F, I>(targets: I, mut query: F) -> Result<Vec<SnapshotAssociation>>
where
    I: IntoIterator<Item = AssociationTarget>,
    F: FnMut(&AssociationTarget) -> Result<Option<DefaultApplication>>,
{
    let normalized = targets.into_iter().collect::<BTreeSet<_>>();
    normalized
        .into_iter()
        .map(|target| {
            let default = query(&target)?;
            Ok(SnapshotAssociation {
                kind: target.kind,
                role: target.role,
                extension: target.identifier,
                default,
            })
        })
        .collect()
}

pub fn associations_from_plan(plan: &AssociationPlan) -> Vec<SnapshotAssociation> {
    plan.entries
        .iter()
        .map(|entry| SnapshotAssociation {
            kind: entry.kind,
            role: entry.role,
            extension: entry.extension.clone(),
            default: entry.current.clone(),
        })
        .collect()
}

pub fn apply_plan_with_snapshot<F>(
    store: &SnapshotStore,
    plan: &AssociationPlan,
    reason: SnapshotReason,
    apply: F,
) -> Result<ProtectedApply>
where
    F: FnMut(&AssociationTarget, &str) -> Result<()>,
{
    let safety_snapshot = if plan.summary.changes > 0 {
        Some(store.create(
            reason,
            Some(plan.digest.clone()),
            associations_from_plan(plan),
        )?)
    } else {
        None
    };
    let report = apply_plan(plan, apply);
    Ok(ProtectedApply {
        safety_snapshot,
        report,
    })
}

pub fn build_rollback_plan<F>(
    snapshot: &Snapshot,
    applications: &[Application],
    mut query_default: F,
) -> Result<AssociationPlan>
where
    F: FnMut(&AssociationTarget) -> Result<Option<DefaultApplication>>,
{
    validate_snapshot(snapshot, None)?;
    let mut entries = Vec::with_capacity(snapshot.associations.len());
    for association in &snapshot.associations {
        let target =
            AssociationTarget::new(association.kind, &association.extension, association.role)?;
        let current = query_default(&target)?;
        let entry = match &association.default {
            None if current.is_none() => PlanEntry {
                kind: association.kind,
                role: association.role,
                extension: association.extension.clone(),
                selector: "<no default>".to_owned(),
                current,
                target: None,
                action: PlanAction::Unchanged,
                reason: None,
            },
            None => PlanEntry {
                kind: association.kind,
                role: association.role,
                extension: association.extension.clone(),
                selector: "<no default>".to_owned(),
                current,
                target: None,
                action: PlanAction::Unresolved,
                reason: Some(
                    "snapshot recorded no default; duti cannot safely remove an association"
                        .to_owned(),
                ),
            },
            Some(previous) => {
                let matches = resolve_app(applications, &previous.bundle_id);
                match matches.as_slice() {
                    [application] => {
                        let action = if current.as_ref().map(|app| app.bundle_id.as_str())
                            == Some(previous.bundle_id.as_str())
                        {
                            PlanAction::Unchanged
                        } else {
                            PlanAction::Change
                        };
                        PlanEntry {
                            kind: association.kind,
                            role: association.role,
                            extension: association.extension.clone(),
                            selector: previous.bundle_id.clone(),
                            current,
                            target: PlannedApplication::from_application(application),
                            action,
                            reason: None,
                        }
                    }
                    [] => PlanEntry {
                        kind: association.kind,
                        role: association.role,
                        extension: association.extension.clone(),
                        selector: previous.bundle_id.clone(),
                        current,
                        target: None,
                        action: PlanAction::Unresolved,
                        reason: Some(format!(
                            "snapshot application '{}' is not installed",
                            previous.bundle_id
                        )),
                    },
                    matches => PlanEntry {
                        kind: association.kind,
                        role: association.role,
                        extension: association.extension.clone(),
                        selector: previous.bundle_id.clone(),
                        current,
                        target: None,
                        action: PlanAction::Unresolved,
                        reason: Some(format!(
                            "snapshot bundle ID is ambiguous: {}",
                            matches
                                .iter()
                                .map(|app| app.path.display().to_string())
                                .collect::<Vec<_>>()
                                .join(", ")
                        )),
                    },
                }
            }
        };
        entries.push(entry);
    }
    assemble_plan(snapshot.schema_version, entries)
}

fn normalize_associations(
    associations: Vec<SnapshotAssociation>,
) -> Result<Vec<SnapshotAssociation>> {
    let mut normalized = associations
        .into_iter()
        .map(|association| {
            let target =
                AssociationTarget::new(association.kind, &association.extension, association.role)?;
            Ok(SnapshotAssociation {
                kind: target.kind,
                role: target.role,
                extension: target.identifier,
                default: association.default,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    normalized.sort_by(|left, right| left.extension.cmp(&right.extension));
    if normalized
        .windows(2)
        .any(|pair| pair[0].extension == pair[1].extension)
    {
        bail!("snapshot contains duplicate extensions");
    }
    Ok(normalized)
}

fn validate_snapshot(snapshot: &Snapshot, expected_id: Option<&str>) -> Result<()> {
    if snapshot.schema_version != SNAPSHOT_SCHEMA_VERSION {
        bail!(
            "unsupported snapshot schema version {}; expected {}",
            snapshot.schema_version,
            SNAPSHOT_SCHEMA_VERSION
        );
    }
    validate_snapshot_id(&snapshot.id)?;
    if expected_id.is_some_and(|id| id != snapshot.id) {
        bail!("snapshot ID does not match its filename");
    }
    normalize_associations(snapshot.associations.clone())?;
    Ok(())
}

fn validate_snapshot_id(id: &str) -> Result<()> {
    if id.is_empty()
        || !id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
    {
        bail!("invalid snapshot ID '{id}'");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_store() -> SnapshotStore {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        SnapshotStore::new(
            std::env::temp_dir().join(format!("dutis-snapshots-{}-{unique}", std::process::id())),
        )
    }

    fn default(extension: &str, bundle_id: &str) -> DefaultApplication {
        DefaultApplication {
            kind: AssociationKind::Extension,
            role: HandlerRole::All,
            extension: extension.to_owned(),
            name: None,
            path: None,
            bundle_id: bundle_id.to_owned(),
        }
    }

    fn application(bundle_id: &str) -> Application {
        Application {
            name: "Editor".to_owned(),
            path: PathBuf::from("/Applications/Editor.app"),
            bundle_id: Some(bundle_id.to_owned()),
            extensions: vec!["md".to_owned()],
        }
    }

    #[test]
    fn atomically_stores_loads_and_lists_snapshots() {
        let store = temporary_store();
        let snapshot = store
            .create(
                SnapshotReason::Manual,
                None,
                vec![SnapshotAssociation {
                    kind: AssociationKind::Extension,
                    role: HandlerRole::All,
                    extension: ".MD".to_owned(),
                    default: Some(default("md", "com.example.Editor")),
                }],
            )
            .unwrap();
        assert!(store.snapshot_path(&snapshot.id).unwrap().is_file());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(store.snapshot_path(&snapshot.id).unwrap())
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        assert_eq!(store.load(&snapshot.id).unwrap(), snapshot);
        let history = store.history().unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].id, snapshot.id);
        fs::remove_dir_all(store.root()).unwrap();
    }

    #[test]
    fn rejects_path_traversal_snapshot_ids() {
        let store = temporary_store();
        assert!(store.load("../../secret").is_err());
    }

    #[test]
    fn captures_extensions_in_normalized_stable_order() {
        let associations = capture_associations(["TXT", ".md", "txt"], |extension| {
            Ok(Some(default(extension, "com.example.Editor")))
        })
        .unwrap();
        assert_eq!(
            associations
                .iter()
                .map(|association| association.extension.as_str())
                .collect::<Vec<_>>(),
            vec!["md", "txt"]
        );
    }

    #[test]
    fn captures_and_restores_typed_targets_with_roles() {
        let target =
            AssociationTarget::new(AssociationKind::Uti, "public.html", HandlerRole::Viewer)
                .unwrap();
        let associations = capture_targets([target.clone()], |value| {
            Ok(Some(DefaultApplication {
                kind: value.kind,
                role: value.role,
                extension: value.identifier.clone(),
                name: None,
                path: None,
                bundle_id: "com.example.Editor".to_owned(),
            }))
        })
        .unwrap();
        assert_eq!(associations[0].kind, AssociationKind::Uti);
        assert_eq!(associations[0].role, HandlerRole::Viewer);

        let snapshot = Snapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            id: "typed-snapshot".to_owned(),
            created_at: "2026-08-22T00:00:00Z".to_owned(),
            reason: SnapshotReason::Manual,
            source_plan_digest: None,
            associations,
        };
        let plan = build_rollback_plan(&snapshot, &[application("com.example.Editor")], |_| {
            Ok(None)
        })
        .unwrap();
        assert_eq!(plan.entries[0].kind, AssociationKind::Uti);
        assert_eq!(plan.entries[0].role, HandlerRole::Viewer);
    }

    #[test]
    fn builds_verified_rollback_plan() {
        let store = temporary_store();
        let snapshot = store
            .create(
                SnapshotReason::BeforeApply,
                Some("source-plan".to_owned()),
                vec![SnapshotAssociation {
                    kind: AssociationKind::Extension,
                    role: HandlerRole::All,
                    extension: "md".to_owned(),
                    default: Some(default("md", "com.example.Editor")),
                }],
            )
            .unwrap();
        let plan = build_rollback_plan(&snapshot, &[application("com.example.Editor")], |target| {
            Ok(Some(default(&target.identifier, "com.example.Other")))
        })
        .unwrap();
        assert_eq!(plan.summary.changes, 1);
        assert_eq!(plan.entries[0].action, PlanAction::Change);
        fs::remove_dir_all(store.root()).unwrap();
    }

    #[test]
    fn refuses_to_fake_restoring_an_absent_default() {
        let store = temporary_store();
        let snapshot = store
            .create(
                SnapshotReason::Manual,
                None,
                vec![SnapshotAssociation {
                    kind: AssociationKind::Extension,
                    role: HandlerRole::All,
                    extension: "md".to_owned(),
                    default: None,
                }],
            )
            .unwrap();
        let plan = build_rollback_plan(&snapshot, &[], |target| {
            Ok(Some(default(&target.identifier, "com.example.Current")))
        })
        .unwrap();
        assert_eq!(plan.summary.unresolved, 1);
        assert!(plan.has_unresolved());
        fs::remove_dir_all(store.root()).unwrap();
    }

    #[test]
    fn failed_apply_retains_its_pre_change_snapshot() {
        let store = temporary_store();
        let snapshot = store
            .create(
                SnapshotReason::Manual,
                None,
                vec![SnapshotAssociation {
                    kind: AssociationKind::Extension,
                    role: HandlerRole::All,
                    extension: "md".to_owned(),
                    default: Some(default("md", "com.example.Editor")),
                }],
            )
            .unwrap();
        let plan = build_rollback_plan(&snapshot, &[application("com.example.Editor")], |target| {
            Ok(Some(default(&target.identifier, "com.example.Other")))
        })
        .unwrap();
        let protected =
            apply_plan_with_snapshot(&store, &plan, SnapshotReason::BeforeRollback, |_, _| {
                anyhow::bail!("simulated failure")
            })
            .unwrap();
        assert_eq!(protected.report.failed, 1);
        let safety = protected.safety_snapshot.unwrap();
        assert_eq!(
            store.load(&safety.id).unwrap().associations[0]
                .default
                .as_ref()
                .unwrap()
                .bundle_id,
            "com.example.Other"
        );
        fs::remove_dir_all(store.root()).unwrap();
    }

    #[test]
    fn snapshot_storage_failure_prevents_mutation() {
        let store = temporary_store();
        fs::write(store.root(), "not a directory").unwrap();
        let snapshot = Snapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            id: "source-snapshot".to_owned(),
            created_at: "2026-08-22T00:00:00Z".to_owned(),
            reason: SnapshotReason::Manual,
            source_plan_digest: None,
            associations: vec![SnapshotAssociation {
                kind: AssociationKind::Extension,
                role: HandlerRole::All,
                extension: "md".to_owned(),
                default: Some(default("md", "com.example.Editor")),
            }],
        };
        let plan = build_rollback_plan(&snapshot, &[application("com.example.Editor")], |target| {
            Ok(Some(default(&target.identifier, "com.example.Other")))
        })
        .unwrap();
        let result =
            apply_plan_with_snapshot(&store, &plan, SnapshotReason::BeforeRollback, |_, _| {
                panic!("mutation ran without a safety snapshot")
            });
        assert!(result.is_err());
        fs::remove_file(store.root()).unwrap();
    }
}
