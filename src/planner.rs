use crate::application::{resolve_app, Application};
use crate::config::DutisConfig;
use crate::system::DefaultApplication;
use anyhow::Result;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

pub const PLAN_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanAction {
    Change,
    Unchanged,
    Unresolved,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct PlannedApplication {
    pub name: String,
    pub path: PathBuf,
    pub bundle_id: String,
}

impl PlannedApplication {
    pub fn from_application(application: &Application) -> Option<Self> {
        Some(Self {
            name: application.name.clone(),
            path: application.path.clone(),
            bundle_id: application.bundle_id.clone()?,
        })
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct PlanEntry {
    pub extension: String,
    pub selector: String,
    pub current: Option<DefaultApplication>,
    pub target: Option<PlannedApplication>,
    pub action: PlanAction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct PlanSummary {
    pub total: usize,
    pub changes: usize,
    pub unchanged: usize,
    pub unresolved: usize,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct AssociationPlan {
    pub schema_version: u32,
    pub config_version: u32,
    pub digest: String,
    pub summary: PlanSummary,
    pub entries: Vec<PlanEntry>,
}

impl AssociationPlan {
    pub fn has_unresolved(&self) -> bool {
        self.summary.unresolved > 0
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplyStatus {
    Applied,
    Skipped,
    Failed,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct ApplyEntryResult {
    pub extension: String,
    pub bundle_id: Option<String>,
    pub status: ApplyStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct ApplyReport {
    pub plan_digest: String,
    pub applied: usize,
    pub skipped: usize,
    pub failed: usize,
    pub results: Vec<ApplyEntryResult>,
}

pub fn build_plan<F>(
    config: &DutisConfig,
    applications: &[Application],
    mut query_default: F,
) -> Result<AssociationPlan>
where
    F: FnMut(&str) -> Result<Option<DefaultApplication>>,
{
    let mut entries = Vec::with_capacity(config.associations.len());
    for (extension, selector) in &config.associations {
        let matches = resolve_app(applications, selector);
        let entry = match matches.as_slice() {
            [] => unresolved_entry(
                extension,
                selector,
                format!("no installed application matches '{selector}'"),
            ),
            [application] if application.bundle_id.is_none() => unresolved_entry(
                extension,
                selector,
                format!(
                    "{} has no readable bundle identifier",
                    application.path.display()
                ),
            ),
            [application] => {
                let current = query_default(extension)?;
                let action = if current.as_ref().map(|app| app.bundle_id.as_str())
                    == application.bundle_id.as_deref()
                {
                    PlanAction::Unchanged
                } else {
                    PlanAction::Change
                };
                PlanEntry {
                    extension: extension.clone(),
                    selector: selector.clone(),
                    current,
                    target: PlannedApplication::from_application(application),
                    action,
                    reason: None,
                }
            }
            matches => unresolved_entry(
                extension,
                selector,
                format!(
                    "selector is ambiguous: {}",
                    matches
                        .iter()
                        .map(|app| app.path.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ),
        };
        entries.push(entry);
    }

    assemble_plan(config.version, entries)
}

pub fn assemble_plan(config_version: u32, entries: Vec<PlanEntry>) -> Result<AssociationPlan> {
    let summary = summarize(&entries);
    let digest = calculate_digest(config_version, &entries)?;
    Ok(AssociationPlan {
        schema_version: PLAN_SCHEMA_VERSION,
        config_version,
        digest,
        summary,
        entries,
    })
}

pub fn apply_plan<F>(plan: &AssociationPlan, mut apply: F) -> ApplyReport
where
    F: FnMut(&str, &str) -> Result<()>,
{
    let mut results = Vec::with_capacity(plan.entries.len());
    for entry in &plan.entries {
        let result = match entry.action {
            PlanAction::Unchanged => ApplyEntryResult {
                extension: entry.extension.clone(),
                bundle_id: entry.target.as_ref().map(|target| target.bundle_id.clone()),
                status: ApplyStatus::Skipped,
                error: None,
            },
            PlanAction::Unresolved => ApplyEntryResult {
                extension: entry.extension.clone(),
                bundle_id: None,
                status: ApplyStatus::Failed,
                error: entry.reason.clone(),
            },
            PlanAction::Change => {
                let bundle_id = entry
                    .target
                    .as_ref()
                    .map(|target| target.bundle_id.as_str())
                    .expect("resolved plan entries have bundle IDs");
                match apply(&entry.extension, bundle_id) {
                    Ok(()) => ApplyEntryResult {
                        extension: entry.extension.clone(),
                        bundle_id: Some(bundle_id.to_owned()),
                        status: ApplyStatus::Applied,
                        error: None,
                    },
                    Err(error) => ApplyEntryResult {
                        extension: entry.extension.clone(),
                        bundle_id: Some(bundle_id.to_owned()),
                        status: ApplyStatus::Failed,
                        error: Some(format!("{error:#}")),
                    },
                }
            }
        };
        results.push(result);
    }

    ApplyReport {
        plan_digest: plan.digest.clone(),
        applied: results
            .iter()
            .filter(|result| result.status == ApplyStatus::Applied)
            .count(),
        skipped: results
            .iter()
            .filter(|result| result.status == ApplyStatus::Skipped)
            .count(),
        failed: results
            .iter()
            .filter(|result| result.status == ApplyStatus::Failed)
            .count(),
        results,
    }
}

fn unresolved_entry(extension: &str, selector: &str, reason: String) -> PlanEntry {
    PlanEntry {
        extension: extension.to_owned(),
        selector: selector.to_owned(),
        current: None,
        target: None,
        action: PlanAction::Unresolved,
        reason: Some(reason),
    }
}

fn summarize(entries: &[PlanEntry]) -> PlanSummary {
    PlanSummary {
        total: entries.len(),
        changes: entries
            .iter()
            .filter(|entry| entry.action == PlanAction::Change)
            .count(),
        unchanged: entries
            .iter()
            .filter(|entry| entry.action == PlanAction::Unchanged)
            .count(),
        unresolved: entries
            .iter()
            .filter(|entry| entry.action == PlanAction::Unresolved)
            .count(),
    }
}

#[derive(Serialize)]
struct DigestMaterial<'a> {
    schema_version: u32,
    config_version: u32,
    entries: Vec<DigestEntry<'a>>,
}

#[derive(Serialize)]
struct DigestEntry<'a> {
    extension: &'a str,
    selector: &'a str,
    current_bundle_id: Option<&'a str>,
    target_bundle_id: Option<&'a str>,
    target_path: Option<String>,
    action: PlanAction,
    reason: Option<&'a str>,
}

fn calculate_digest(config_version: u32, entries: &[PlanEntry]) -> Result<String> {
    let material = DigestMaterial {
        schema_version: PLAN_SCHEMA_VERSION,
        config_version,
        entries: entries
            .iter()
            .map(|entry| DigestEntry {
                extension: &entry.extension,
                selector: &entry.selector,
                current_bundle_id: entry.current.as_ref().map(|app| app.bundle_id.as_str()),
                target_bundle_id: entry.target.as_ref().map(|app| app.bundle_id.as_str()),
                target_path: entry
                    .target
                    .as_ref()
                    .map(|app| app.path.display().to_string()),
                action: entry.action,
                reason: entry.reason.as_deref(),
            })
            .collect(),
    };
    let canonical = serde_json::to_vec(&material)?;
    let hash = Sha256::digest(canonical);
    Ok(hash.iter().map(|byte| format!("{byte:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn app(name: &str, bundle_id: &str) -> Application {
        Application {
            name: name.to_owned(),
            path: PathBuf::from(format!("/Applications/{name}.app")),
            bundle_id: Some(bundle_id.to_owned()),
            extensions: vec!["md".to_owned(), "json".to_owned()],
        }
    }

    fn config(entries: &[(&str, &str)]) -> DutisConfig {
        DutisConfig {
            version: 1,
            associations: entries
                .iter()
                .map(|(extension, selector)| ((*extension).to_owned(), (*selector).to_owned()))
                .collect::<BTreeMap<_, _>>(),
        }
    }

    fn current(extension: &str, bundle_id: &str) -> DefaultApplication {
        DefaultApplication {
            extension: extension.to_owned(),
            name: None,
            path: None,
            bundle_id: bundle_id.to_owned(),
        }
    }

    #[test]
    fn builds_deterministic_change_and_unchanged_plan() {
        let applications = vec![app("Editor", "com.example.Editor")];
        let config = config(&[("json", "Editor"), ("md", "com.example.Editor")]);
        let build = || {
            build_plan(&config, &applications, |extension| {
                Ok(Some(if extension == "md" {
                    current(extension, "com.example.Editor")
                } else {
                    current(extension, "com.example.Other")
                }))
            })
            .unwrap()
        };

        let first = build();
        let second = build();
        assert_eq!(first.digest, second.digest);
        assert_eq!(first.digest.len(), 64);
        assert_eq!(first.summary.changes, 1);
        assert_eq!(first.summary.unchanged, 1);
        assert_eq!(first.entries[0].action, PlanAction::Change);
        assert_eq!(first.entries[1].action, PlanAction::Unchanged);
    }

    #[test]
    fn current_state_changes_the_digest() {
        let applications = vec![app("Editor", "com.example.Editor")];
        let config = config(&[("md", "Editor")]);
        let missing = build_plan(&config, &applications, |_| Ok(None)).unwrap();
        let converged = build_plan(&config, &applications, |extension| {
            Ok(Some(current(extension, "com.example.Editor")))
        })
        .unwrap();
        assert_ne!(missing.digest, converged.digest);
    }

    #[test]
    fn records_unresolved_and_ambiguous_selectors() {
        let applications = vec![
            app("Editor", "com.example.Editor"),
            app("Editor", "com.example.EditorBeta"),
        ];
        let config = config(&[("json", "Missing"), ("md", "Editor")]);
        let plan = build_plan(&config, &applications, |_| {
            panic!("unresolved entries must not query current state")
        })
        .unwrap();
        assert_eq!(plan.summary.unresolved, 2);
        assert!(plan.has_unresolved());
    }

    #[test]
    fn apply_continues_after_a_partial_failure() {
        let applications = vec![app("Editor", "com.example.Editor")];
        let config = config(&[("json", "Editor"), ("md", "Editor")]);
        let plan = build_plan(&config, &applications, |_| Ok(None)).unwrap();
        let report = apply_plan(&plan, |extension, _| {
            if extension == "json" {
                anyhow::bail!("simulated failure");
            }
            Ok(())
        });
        assert_eq!(report.applied, 1);
        assert_eq!(report.failed, 1);
        assert_eq!(report.results.len(), 2);
    }

    #[test]
    fn converged_apply_is_idempotent() {
        let applications = vec![app("Editor", "com.example.Editor")];
        let config = config(&[("md", "Editor")]);
        let plan = build_plan(&config, &applications, |extension| {
            Ok(Some(current(extension, "com.example.Editor")))
        })
        .unwrap();
        let report = apply_plan(&plan, |_, _| panic!("unchanged entry was applied"));
        assert_eq!(report.applied, 0);
        assert_eq!(report.skipped, 1);
        assert_eq!(report.failed, 0);
    }
}
