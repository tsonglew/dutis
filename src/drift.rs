use crate::governance::{PolicyAssessment, PolicySummary};
use crate::planner::{AssociationPlan, PlanAction, PlanEntry};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::process::Command;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

pub const DRIFT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriftState {
    InSync,
    DriftDetected,
    Unresolved,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct DriftReport {
    pub schema_version: u32,
    pub checked_at: String,
    pub state: DriftState,
    pub plan_digest: String,
    pub changes: Vec<PlanEntry>,
    pub unresolved: Vec<PlanEntry>,
    pub plan: AssociationPlan,
    pub policy: PolicySummary,
    pub assessment: PolicyAssessment,
}

impl DriftReport {
    pub fn new(
        plan: AssociationPlan,
        policy: PolicySummary,
        assessment: PolicyAssessment,
    ) -> Result<Self> {
        let checked_at = OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .context("failed to format drift check time")?;
        Ok(Self::at(checked_at, plan, policy, assessment))
    }

    pub fn at(
        checked_at: String,
        plan: AssociationPlan,
        policy: PolicySummary,
        assessment: PolicyAssessment,
    ) -> Self {
        let changes = plan
            .entries
            .iter()
            .filter(|entry| entry.action == PlanAction::Change)
            .cloned()
            .collect::<Vec<_>>();
        let unresolved = plan
            .entries
            .iter()
            .filter(|entry| entry.action == PlanAction::Unresolved)
            .cloned()
            .collect::<Vec<_>>();
        let state = if !unresolved.is_empty() {
            DriftState::Unresolved
        } else if !changes.is_empty() {
            DriftState::DriftDetected
        } else {
            DriftState::InSync
        };
        Self {
            schema_version: DRIFT_SCHEMA_VERSION,
            checked_at,
            state,
            plan_digest: plan.digest.clone(),
            changes,
            unresolved,
            plan,
            policy,
            assessment,
        }
    }

    pub fn notification(&self) -> DriftNotification {
        match self.state {
            DriftState::InSync => DriftNotification {
                title: "Dutis associations restored".to_owned(),
                message: "All monitored file associations match the declared configuration."
                    .to_owned(),
            },
            DriftState::DriftDetected => DriftNotification {
                title: "Dutis drift detected".to_owned(),
                message: format!(
                    "{} file association(s) differ from the declared configuration.",
                    self.changes.len()
                ),
            },
            DriftState::Unresolved => DriftNotification {
                title: "Dutis drift check needs attention".to_owned(),
                message: format!(
                    "{} selector(s) could not be resolved; no automatic change is safe.",
                    self.unresolved.len()
                ),
            },
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DriftNotification {
    pub title: String,
    pub message: String,
}

#[derive(Debug, Default)]
pub struct DriftTracker {
    previous: Option<(DriftState, String)>,
}

impl DriftTracker {
    pub fn should_notify(&mut self, report: &DriftReport) -> bool {
        let current = (report.state, report.plan_digest.clone());
        let should_notify = match &self.previous {
            None => report.state != DriftState::InSync,
            Some((previous_state, previous_digest)) => {
                *previous_state != report.state
                    || (report.state != DriftState::InSync
                        && previous_digest != &report.plan_digest)
            }
        };
        self.previous = Some(current);
        should_notify
    }
}

pub fn send_macos_notification(notification: &DriftNotification) -> Result<()> {
    if !cfg!(target_os = "macos") {
        bail!("macOS notifications are unavailable on this platform");
    }
    let script = format!(
        "display notification \"{}\" with title \"{}\"",
        escape_applescript(&notification.message),
        escape_applescript(&notification.title)
    );
    let output = Command::new("/usr/bin/osascript")
        .args(["-e", &script])
        .output()
        .context("failed to send macOS notification")?;
    if !output.status.success() {
        bail!(
            "macOS notification failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

fn escape_applescript(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governance::{ApprovalMode, PolicyAssessment, PolicySummary};
    use crate::planner::{assemble_plan, PlanEntry};
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn policy() -> (PolicySummary, PolicyAssessment) {
        (
            PolicySummary {
                path: PathBuf::from("/tmp/policy.toml"),
                exists: false,
                digest: "policy-digest".to_owned(),
                version: 1,
                approval_mode: ApprovalMode::Explicit,
                approval_token_configured: false,
                allowed_extensions: None,
                allowed_kinds: None,
                allowed_applications: None,
                protected_associations: BTreeMap::new(),
                protected_handlers: Vec::new(),
            },
            PolicyAssessment {
                allowed: true,
                approval_mode: ApprovalMode::Explicit,
                violations: Vec::new(),
            },
        )
    }

    fn report(action: PlanAction) -> DriftReport {
        let (policy, assessment) = policy();
        let plan = assemble_plan(
            1,
            vec![PlanEntry {
                kind: crate::association::AssociationKind::Extension,
                role: crate::association::HandlerRole::All,
                extension: "md".to_owned(),
                selector: "com.example.Editor".to_owned(),
                current: None,
                target: None,
                action,
                reason: (action == PlanAction::Unresolved).then(|| "missing app".to_owned()),
            }],
        )
        .unwrap();
        DriftReport::at("2026-08-22T00:00:00Z".to_owned(), plan, policy, assessment)
    }

    #[test]
    fn classifies_clean_drifted_and_unresolved_plans() {
        assert_eq!(report(PlanAction::Unchanged).state, DriftState::InSync);
        assert_eq!(report(PlanAction::Change).state, DriftState::DriftDetected);
        assert_eq!(report(PlanAction::Unresolved).state, DriftState::Unresolved);
    }

    #[test]
    fn tracker_deduplicates_unchanged_drift_and_notifies_on_recovery() {
        let clean = report(PlanAction::Unchanged);
        let drift = report(PlanAction::Change);
        let mut tracker = DriftTracker::default();
        assert!(!tracker.should_notify(&clean));
        assert!(tracker.should_notify(&drift));
        assert!(!tracker.should_notify(&drift));
        assert!(tracker.should_notify(&clean));
    }

    #[test]
    fn escapes_notification_text_for_applescript() {
        assert_eq!(
            escape_applescript("a \\\"quote\\\""),
            "a \\\\\\\"quote\\\\\\\""
        );
    }
}
