use crate::application::Application;
use crate::association::AssociationTarget;
use crate::association::{AssociationKind, HandlerRole};
use crate::config::{AssociationRule, DutisConfig, CONFIG_VERSION};
use crate::governance::Policy;
use crate::planner::{assemble_plan, AssociationPlan, PlanAction, PlanEntry, PlannedApplication};
use crate::system::DefaultApplication;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

pub const PROFILE_OVERLAY_VERSION: u32 = 1;
pub const PROFILE_FILE_ENV: &str = "DUTIS_PROFILE_FILE";
const MAX_PROFILE_FILE_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct ProfileDefinition {
    pub name: &'static str,
    pub description: &'static str,
    pub associations: Vec<ProfileAssociation>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct ProfileAssociation {
    pub extension: &'static str,
    pub candidates: Vec<ProfileCandidate>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct ProfileCandidate {
    pub bundle_id: &'static str,
    pub rationale: &'static str,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct EffectiveProfileDefinition {
    pub name: String,
    pub description: String,
    pub associations: Vec<EffectiveProfileAssociation>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct EffectiveProfileAssociation {
    pub association: AssociationTarget,
    pub extension: String,
    pub candidates: Vec<EffectiveProfileCandidate>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct EffectiveProfileCandidate {
    pub bundle_id: String,
    pub rationale: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileOverlayDocument {
    version: u32,
    #[serde(default)]
    profiles: Vec<ProfileOverlay>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileOverlay {
    name: String,
    description: Option<String>,
    #[serde(default)]
    replace: bool,
    #[serde(default)]
    associations: Vec<ProfileAssociationOverlay>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileAssociationOverlay {
    #[serde(default)]
    kind: AssociationKind,
    identifier: String,
    #[serde(default)]
    role: HandlerRole,
    #[serde(default)]
    replace_candidates: bool,
    applications: Vec<String>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecommendationAction {
    Change,
    KeepCurrent,
    PolicyBlocked,
    Unavailable,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateSource {
    ProtectedPolicy,
    ExtensionPreference,
    HandlerPreference,
    GlobalPreference,
    Profile,
}

impl CandidateSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ProtectedPolicy => "protected_policy",
            Self::ExtensionPreference => "extension_preference",
            Self::HandlerPreference => "handler_preference",
            Self::GlobalPreference => "global_preference",
            Self::Profile => "profile",
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct CandidateEvidence {
    pub bundle_id: String,
    pub priority: usize,
    pub source: CandidateSource,
    pub installed_paths: Vec<PathBuf>,
    pub declares_extension: bool,
    pub declares_target: bool,
    pub policy_eligible: bool,
    pub policy_reasons: Vec<String>,
    pub selected: bool,
    pub rationale: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct AssociationRecommendation {
    pub association: AssociationTarget,
    pub extension: String,
    pub action: RecommendationAction,
    pub current: Option<DefaultApplication>,
    pub target: Option<PlannedApplication>,
    pub explanation: String,
    pub evidence: Vec<CandidateEvidence>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct RecommendationSummary {
    pub total: usize,
    pub available: usize,
    pub changes: usize,
    pub kept_current: usize,
    pub policy_blocked: usize,
    pub unavailable: usize,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct ProfileRecommendation {
    pub profile: String,
    pub description: String,
    pub summary: RecommendationSummary,
    pub recommendations: Vec<AssociationRecommendation>,
    pub proposed_config: DutisConfig,
    pub proposed_toml: String,
    pub plan: AssociationPlan,
}

pub fn profiles() -> Vec<ProfileDefinition> {
    vec![
        developer_profile(),
        designer_profile(),
        media_profile(),
        minimal_profile(),
    ]
}

pub fn find_profile(name: &str) -> Option<ProfileDefinition> {
    profiles()
        .into_iter()
        .find(|profile| profile.name.eq_ignore_ascii_case(name.trim()))
}

pub fn effective_profiles() -> Result<Vec<EffectiveProfileDefinition>> {
    let mut effective = profiles()
        .into_iter()
        .map(EffectiveProfileDefinition::from)
        .collect::<Vec<_>>();
    let Some(path) = profile_overlay_path()? else {
        return Ok(effective);
    };
    let metadata = std::fs::symlink_metadata(&path)
        .with_context(|| format!("failed to inspect profile overlay {}", path.display()))?;
    if !metadata.file_type().is_file() {
        bail!("profile overlay is not a regular file: {}", path.display());
    }
    if metadata.len() > MAX_PROFILE_FILE_BYTES {
        bail!("profile overlay {} exceeds the 1 MiB limit", path.display());
    }
    let contents = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read profile overlay {}", path.display()))?;
    apply_profile_overlay_document(&mut effective, &contents)
        .with_context(|| format!("invalid profile overlay {}", path.display()))?;
    Ok(effective)
}

fn apply_profile_overlay_document(
    effective: &mut Vec<EffectiveProfileDefinition>,
    contents: &str,
) -> Result<()> {
    let document: ProfileOverlayDocument =
        toml::from_str(contents).context("failed to parse profile overlay TOML")?;
    if document.version != PROFILE_OVERLAY_VERSION {
        bail!(
            "unsupported profile overlay version {}; expected {}",
            document.version,
            PROFILE_OVERLAY_VERSION
        );
    }
    apply_profile_overlays(effective, document.profiles)?;
    Ok(())
}

pub fn find_effective_profile(name: &str) -> Result<Option<EffectiveProfileDefinition>> {
    let name = normalize_profile_name(name)?;
    Ok(effective_profiles()?
        .into_iter()
        .find(|profile| profile.name == name))
}

impl From<ProfileDefinition> for EffectiveProfileDefinition {
    fn from(profile: ProfileDefinition) -> Self {
        Self {
            name: profile.name.to_owned(),
            description: profile.description.to_owned(),
            associations: profile
                .associations
                .into_iter()
                .map(|association| {
                    let target = AssociationTarget::extension(association.extension)
                        .expect("built-in profile extension is valid");
                    EffectiveProfileAssociation {
                        extension: target.identifier.clone(),
                        association: target,
                        candidates: association
                            .candidates
                            .into_iter()
                            .map(|candidate| EffectiveProfileCandidate {
                                bundle_id: candidate.bundle_id.to_owned(),
                                rationale: candidate.rationale.to_owned(),
                            })
                            .collect(),
                    }
                })
                .collect(),
        }
    }
}

fn profile_overlay_path() -> Result<Option<PathBuf>> {
    if let Some(path) = std::env::var_os(PROFILE_FILE_ENV).filter(|value| !value.is_empty()) {
        let path = PathBuf::from(path);
        if !path.exists() {
            bail!("profile overlay does not exist: {}", path.display());
        }
        return Ok(Some(path));
    }
    let path = if let Some(state) =
        std::env::var_os("DUTIS_STATE_DIR").filter(|value| !value.is_empty())
    {
        PathBuf::from(state).join("profiles.toml")
    } else if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(home).join("Library/Application Support/dutis/profiles.toml")
    } else {
        return Ok(None);
    };
    Ok(path.exists().then_some(path))
}

fn apply_profile_overlays(
    profiles: &mut Vec<EffectiveProfileDefinition>,
    overlays: Vec<ProfileOverlay>,
) -> Result<()> {
    let mut seen_profiles = BTreeSet::new();
    for overlay in overlays {
        let name = normalize_profile_name(&overlay.name)?;
        if !seen_profiles.insert(name.clone()) {
            bail!("duplicate profile overlay '{name}'");
        }
        let existing = profiles.iter().position(|profile| profile.name == name);
        let description = overlay
            .description
            .map(|value| normalize_nonempty(&value, "profile description"))
            .transpose()?;
        let index = if let Some(index) = existing {
            if let Some(description) = description {
                profiles[index].description = description;
            }
            if overlay.replace {
                profiles[index].associations.clear();
            }
            index
        } else {
            let description = description
                .ok_or_else(|| anyhow::anyhow!("custom profile '{name}' requires a description"))?;
            profiles.push(EffectiveProfileDefinition {
                name: name.clone(),
                description,
                associations: Vec::new(),
            });
            profiles.len() - 1
        };

        let mut seen_targets = BTreeSet::new();
        for association in overlay.associations {
            let target = AssociationTarget::new(
                association.kind,
                &association.identifier,
                association.role,
            )?;
            if !seen_targets.insert(target.clone()) {
                bail!("duplicate profile target {target} in '{name}'");
            }
            let candidates = normalize_overlay_candidates(association.applications, &target)?;
            let profile = &mut profiles[index];
            if let Some(existing) = profile
                .associations
                .iter_mut()
                .find(|item| item.association == target)
            {
                if association.replace_candidates {
                    existing.candidates = candidates;
                } else {
                    let mut merged = candidates;
                    let mut seen = merged
                        .iter()
                        .map(|candidate| candidate.bundle_id.clone())
                        .collect::<BTreeSet<_>>();
                    merged.extend(
                        existing
                            .candidates
                            .iter()
                            .filter(|candidate| seen.insert(candidate.bundle_id.clone()))
                            .cloned(),
                    );
                    existing.candidates = merged;
                }
            } else {
                profile.associations.push(EffectiveProfileAssociation {
                    extension: target.identifier.clone(),
                    association: target,
                    candidates,
                });
            }
        }
        if profiles[index].associations.is_empty() {
            bail!("profile '{name}' must contain at least one association");
        }
    }
    Ok(())
}

fn normalize_profile_name(value: &str) -> Result<String> {
    let name = value.trim().to_ascii_lowercase();
    if name.is_empty()
        || name.len() > 64
        || !name
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphanumeric())
        || !name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        bail!("invalid profile name '{value}'");
    }
    Ok(name)
}

fn normalize_nonempty(value: &str, label: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        bail!("{label} cannot be empty");
    }
    Ok(value.to_owned())
}

fn normalize_overlay_candidates(
    applications: Vec<String>,
    target: &AssociationTarget,
) -> Result<Vec<EffectiveProfileCandidate>> {
    if applications.is_empty() {
        bail!("profile target {target} requires at least one application");
    }
    let mut seen = BTreeSet::new();
    applications
        .into_iter()
        .map(|application| {
            let bundle_id = normalize_nonempty(&application, "profile application")?;
            if !seen.insert(bundle_id.clone()) {
                bail!("duplicate profile application '{bundle_id}' for {target}");
            }
            Ok(EffectiveProfileCandidate {
                rationale: "Configured by the local profile overlay.".to_owned(),
                bundle_id,
            })
        })
        .collect()
}

pub fn recommend_profile<F>(
    profile: &ProfileDefinition,
    applications: &[Application],
    mut query_default: F,
) -> Result<ProfileRecommendation>
where
    F: FnMut(&str) -> Result<Option<DefaultApplication>>,
{
    let profile = EffectiveProfileDefinition::from(profile.clone());
    recommend_profile_internal(
        &profile,
        applications,
        &mut |target| query_default(&target.identifier),
        None,
        false,
    )
}

pub fn recommend_profile_with_policy<F>(
    profile: &ProfileDefinition,
    applications: &[Application],
    mut query_default: F,
    policy: &Policy,
) -> Result<ProfileRecommendation>
where
    F: FnMut(&str) -> Result<Option<DefaultApplication>>,
{
    let profile = EffectiveProfileDefinition::from(profile.clone());
    recommend_profile_internal(
        &profile,
        applications,
        &mut |target| query_default(&target.identifier),
        Some(policy),
        false,
    )
}

pub fn recommend_profile_with_policy_typed<F>(
    profile: &ProfileDefinition,
    applications: &[Application],
    mut query_default: F,
    policy: &Policy,
) -> Result<ProfileRecommendation>
where
    F: FnMut(&AssociationTarget) -> Result<Option<DefaultApplication>>,
{
    let profile = EffectiveProfileDefinition::from(profile.clone());
    recommend_profile_internal(
        &profile,
        applications,
        &mut query_default,
        Some(policy),
        true,
    )
}

pub fn recommend_effective_profile_with_policy<F>(
    profile: &EffectiveProfileDefinition,
    applications: &[Application],
    mut query_default: F,
    policy: &Policy,
) -> Result<ProfileRecommendation>
where
    F: FnMut(&AssociationTarget) -> Result<Option<DefaultApplication>>,
{
    recommend_profile_internal(
        profile,
        applications,
        &mut query_default,
        Some(policy),
        true,
    )
}

fn recommend_profile_internal<F>(
    profile: &EffectiveProfileDefinition,
    applications: &[Application],
    query_default: &mut F,
    policy: Option<&Policy>,
    include_typed_preferences: bool,
) -> Result<ProfileRecommendation>
where
    F: FnMut(&AssociationTarget) -> Result<Option<DefaultApplication>>,
{
    let mut proposed_associations = BTreeMap::new();
    let mut proposed_handlers = Vec::new();
    let mut plan_entries = Vec::new();
    let mut inputs = profile
        .associations
        .iter()
        .map(|association| RecommendationInput {
            target: association.association.clone(),
            profile_candidates: association
                .candidates
                .iter()
                .map(|candidate| OwnedProfileCandidate {
                    bundle_id: candidate.bundle_id.clone(),
                    rationale: candidate.rationale.clone(),
                })
                .collect(),
            requires_declaration: association.association.kind != AssociationKind::Extension,
        })
        .collect::<Vec<_>>();
    if include_typed_preferences {
        if let Some(policy) = policy {
            for preference in &policy.recommendations.handlers {
                let target = AssociationTarget::new(
                    preference.kind,
                    &preference.identifier,
                    preference.role,
                )?;
                if inputs.iter().any(|input| input.target == target) {
                    continue;
                }
                inputs.push(RecommendationInput {
                    target,
                    profile_candidates: Vec::new(),
                    requires_declaration: true,
                });
            }
        }
    }
    let mut recommendations = Vec::with_capacity(inputs.len());

    for input in inputs {
        let association = &input.target;
        let current = query_default(association)?;
        let required = policy.and_then(|policy| required_application(policy, association));
        let candidates = ordered_candidates(&input, policy, required);
        let candidates = candidates
            .into_iter()
            .enumerate()
            .map(|(index, candidate)| {
                let matches = applications
                    .iter()
                    .filter(|application| {
                        application.bundle_id.as_deref() == Some(candidate.bundle_id.as_str())
                    })
                    .collect::<Vec<_>>();
                let policy_reasons = policy.map_or_else(Vec::new, |policy| {
                    policy_reasons(policy, association, &candidate.bundle_id, required)
                });
                let declares_target = matches
                    .iter()
                    .any(|application| application_declares_target(application, association));
                CandidateEvidence {
                    bundle_id: candidate.bundle_id,
                    priority: index + 1,
                    source: candidate.source,
                    installed_paths: matches
                        .iter()
                        .map(|application| application.path.clone())
                        .collect(),
                    declares_extension: association.kind == AssociationKind::Extension
                        && declares_target,
                    declares_target,
                    policy_eligible: policy_reasons.is_empty(),
                    policy_reasons,
                    selected: false,
                    rationale: candidate.rationale,
                }
            })
            .collect::<Vec<_>>();

        let current_candidate = current.as_ref().and_then(|current| {
            candidates
                .iter()
                .position(|candidate| candidate.bundle_id == current.bundle_id)
        });
        let selected_index = candidates
            .iter()
            .position(|candidate| {
                candidate.source != CandidateSource::Profile
                    && candidate.policy_eligible
                    && candidate.installed_paths.len() == 1
                    && (!input.requires_declaration || candidate.declares_target)
            })
            .or_else(|| {
                current_candidate.filter(|index| {
                    candidates[*index].policy_eligible
                        && candidates[*index].installed_paths.len() == 1
                        && (!input.requires_declaration || candidates[*index].declares_target)
                })
            })
            .or_else(|| {
                candidates.iter().position(|candidate| {
                    candidate.policy_eligible
                        && candidate.installed_paths.len() == 1
                        && (!input.requires_declaration || candidate.declares_target)
                })
            });
        let mut evidence = candidates;

        let (action, target, explanation) = if let Some(index) = selected_index {
            evidence[index].selected = true;
            let selected = &evidence[index];
            let application = applications
                .iter()
                .find(|application| {
                    application.bundle_id.as_deref() == Some(selected.bundle_id.as_str())
                        && application.path == evidence[index].installed_paths[0]
                })
                .expect("selected recommendation candidate has one installed application");
            let target = PlannedApplication::from_application(application)
                .expect("recommendation candidates have bundle identifiers");
            let keep_current = current.as_ref().map(|value| value.bundle_id.as_str())
                == Some(selected.bundle_id.as_str());
            let action = if keep_current {
                RecommendationAction::KeepCurrent
            } else {
                RecommendationAction::Change
            };
            let explanation = if keep_current {
                if policy.is_some() {
                    format!(
                        "Keep {} because the current handler is compatible with the {} profile and local policy.",
                        application.name, profile.name
                    )
                } else {
                    format!(
                        "Keep {} because the current handler is compatible with the {} profile.",
                        application.name, profile.name
                    )
                }
            } else {
                match selected.source {
                    CandidateSource::ProtectedPolicy => format!(
                        "Recommend {} because local policy requires {} to use {}.",
                        application.name, association, selected.bundle_id
                    ),
                    CandidateSource::ExtensionPreference
                    | CandidateSource::HandlerPreference
                    | CandidateSource::GlobalPreference => {
                        format!(
                            "Recommend {} because it is the highest-priority uniquely installed local-policy preference for {}.",
                            application.name, association
                        )
                    }
                    CandidateSource::Profile => format!(
                        "Recommend {} because it is the highest-priority uniquely installed candidate for {}: {}",
                        application.name, association, selected.rationale
                    ),
                }
            };
            if association.kind == AssociationKind::Extension
                && association.role == HandlerRole::All
            {
                proposed_associations
                    .insert(association.identifier.clone(), selected.bundle_id.clone());
            } else {
                proposed_handlers.push(AssociationRule {
                    kind: association.kind,
                    identifier: association.identifier.clone(),
                    role: association.role,
                    application: selected.bundle_id.clone(),
                });
            }
            plan_entries.push(PlanEntry {
                kind: association.kind,
                role: association.role,
                extension: association.identifier.clone(),
                selector: selected.bundle_id.clone(),
                current: current.clone(),
                target: Some(target.clone()),
                action: if keep_current {
                    PlanAction::Unchanged
                } else {
                    PlanAction::Change
                },
                reason: None,
            });
            (action, Some(target), explanation)
        } else {
            let blocked = evidence.iter().any(|candidate| {
                candidate.installed_paths.len() == 1 && !candidate.policy_eligible
            });
            let action = if blocked {
                RecommendationAction::PolicyBlocked
            } else {
                RecommendationAction::Unavailable
            };
            let explanation = if blocked {
                format!(
                    "Installed candidates for {} are excluded by local policy; no change is proposed.",
                    association
                )
            } else if input.requires_declaration {
                format!(
                    "No uniquely installed policy-eligible candidate with a matching declaration is available for {}; no change is proposed.",
                    association
                )
            } else {
                format!(
                    "No uniquely installed policy-eligible candidate is available for {}; no change is proposed.",
                    association
                )
            };
            (action, None, explanation)
        };

        recommendations.push(AssociationRecommendation {
            association: association.clone(),
            extension: association.identifier.clone(),
            action,
            current,
            target,
            explanation,
            evidence,
        });
    }

    let proposed_config = DutisConfig {
        version: CONFIG_VERSION,
        associations: proposed_associations,
        handlers: proposed_handlers,
    };
    let proposed_toml = toml::to_string_pretty(&proposed_config)?;
    let plan = assemble_plan(CONFIG_VERSION, plan_entries)?;
    let summary = RecommendationSummary {
        total: recommendations.len(),
        available: recommendations
            .iter()
            .filter(|recommendation| recommendation.target.is_some())
            .count(),
        changes: recommendations
            .iter()
            .filter(|recommendation| recommendation.action == RecommendationAction::Change)
            .count(),
        kept_current: recommendations
            .iter()
            .filter(|recommendation| recommendation.action == RecommendationAction::KeepCurrent)
            .count(),
        policy_blocked: recommendations
            .iter()
            .filter(|recommendation| recommendation.action == RecommendationAction::PolicyBlocked)
            .count(),
        unavailable: recommendations
            .iter()
            .filter(|recommendation| recommendation.action == RecommendationAction::Unavailable)
            .count(),
    };

    Ok(ProfileRecommendation {
        profile: profile.name.to_owned(),
        description: profile.description.to_owned(),
        summary,
        recommendations,
        proposed_config,
        proposed_toml,
        plan,
    })
}

#[derive(Debug)]
struct OrderedCandidate {
    bundle_id: String,
    source: CandidateSource,
    rationale: String,
}

#[derive(Debug)]
struct OwnedProfileCandidate {
    bundle_id: String,
    rationale: String,
}

#[derive(Debug)]
struct RecommendationInput {
    target: AssociationTarget,
    profile_candidates: Vec<OwnedProfileCandidate>,
    requires_declaration: bool,
}

fn ordered_candidates(
    input: &RecommendationInput,
    policy: Option<&Policy>,
    required: Option<&str>,
) -> Vec<OrderedCandidate> {
    let association = &input.target;
    let mut candidates = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut push = |bundle_id: &str, source: CandidateSource, rationale: String| {
        if seen.insert(bundle_id.to_owned()) {
            candidates.push(OrderedCandidate {
                bundle_id: bundle_id.to_owned(),
                source,
                rationale,
            });
        }
    };
    if let Some(bundle_id) = required {
        push(
            bundle_id,
            CandidateSource::ProtectedPolicy,
            "Required by the local protected-association policy.".to_owned(),
        );
    }
    if let Some(policy) = policy {
        if association.kind == AssociationKind::Extension {
            if let Some(preferences) = policy
                .recommendations
                .extensions
                .get(&association.identifier)
            {
                for bundle_id in preferences {
                    push(
                        bundle_id,
                        CandidateSource::ExtensionPreference,
                        format!("Preferred by local policy for {association}."),
                    );
                }
            }
        } else if let Some(preference) = policy.recommendations.handlers.iter().find(|preference| {
            preference.kind == association.kind
                && preference.identifier == association.identifier
                && preference.role == association.role
        }) {
            for bundle_id in &preference.applications {
                push(
                    bundle_id,
                    CandidateSource::HandlerPreference,
                    format!("Preferred by local policy for {association}."),
                );
            }
        }
        for bundle_id in &policy.recommendations.preferred_applications {
            if input
                .profile_candidates
                .iter()
                .any(|candidate| &candidate.bundle_id == bundle_id)
            {
                push(
                    bundle_id,
                    CandidateSource::GlobalPreference,
                    "Preferred by local policy among profile candidates.".to_owned(),
                );
            }
        }
    }
    for candidate in &input.profile_candidates {
        push(
            &candidate.bundle_id,
            CandidateSource::Profile,
            candidate.rationale.clone(),
        );
    }
    candidates
}

fn required_application<'a>(
    policy: &'a Policy,
    association: &AssociationTarget,
) -> Option<&'a str> {
    (association.kind == AssociationKind::Extension && association.role == HandlerRole::All)
        .then(|| policy.protected_associations.get(&association.identifier))
        .flatten()
        .map(String::as_str)
        .or_else(|| {
            policy
                .protected_handlers
                .iter()
                .find(|handler| {
                    handler.kind == association.kind
                        && handler.identifier == association.identifier
                        && handler.role == association.role
                })
                .map(|handler| handler.application.as_str())
        })
}

fn policy_reasons(
    policy: &Policy,
    association: &AssociationTarget,
    bundle_id: &str,
    required: Option<&str>,
) -> Vec<String> {
    let mut reasons = Vec::new();
    if policy
        .allowed_kinds
        .as_ref()
        .is_some_and(|kinds| !kinds.contains(&association.kind))
    {
        reasons.push(format!("{} associations are not allowed", association.kind));
    }
    if association.kind == AssociationKind::Extension
        && policy
            .allowed_extensions
            .as_ref()
            .is_some_and(|extensions| !extensions.contains(&association.identifier))
    {
        reasons.push(format!(
            "extension .{} is not allowed",
            association.identifier
        ));
    }
    if policy
        .allowed_applications
        .as_ref()
        .is_some_and(|applications| !applications.contains(bundle_id))
    {
        reasons.push(format!("application {bundle_id} is not allowed"));
    }
    if let Some(required) = required {
        if required != bundle_id {
            reasons.push(format!(
                "protected association {association} requires {required}"
            ));
        }
    }
    reasons
}

fn application_declares_target(application: &Application, association: &AssociationTarget) -> bool {
    if association.kind == AssociationKind::Extension
        && application
            .extensions
            .iter()
            .any(|extension| extension.eq_ignore_ascii_case(&association.identifier))
    {
        return true;
    }
    application.handlers.iter().any(|handler| {
        handler.kind == association.kind
            && handler.identifier == association.identifier
            && (handler.role == HandlerRole::All
                || association.role == HandlerRole::All
                || handler.role == association.role
                || (association.role == HandlerRole::Viewer && handler.role == HandlerRole::Editor))
    })
}

fn developer_profile() -> ProfileDefinition {
    ProfileDefinition {
        name: "developer",
        description: "Source code, structured data, scripts, and technical documentation.",
        associations: vec![
            developer_association("md"),
            developer_association("json"),
            developer_association("yaml"),
            developer_association("yml"),
            developer_association("toml"),
            developer_association("rs"),
            developer_association("py"),
            developer_association("js"),
            developer_association("ts"),
            developer_association("sh"),
        ],
    }
}

fn developer_association(extension: &'static str) -> ProfileAssociation {
    ProfileAssociation {
        extension,
        candidates: vec![
            candidate(
                "dev.zed.Zed",
                "Fast native editor with strong project and language tooling.",
            ),
            candidate(
                "com.microsoft.VSCode",
                "Broad language and extension ecosystem for development files.",
            ),
            candidate(
                "com.sublimetext.4",
                "Fast general-purpose source and text editor.",
            ),
            candidate(
                "com.apple.dt.Xcode",
                "Apple development environment with source editing support.",
            ),
            candidate(
                "com.apple.TextEdit",
                "Built-in fallback for plain-text-compatible files.",
            ),
        ],
    }
}

fn designer_profile() -> ProfileDefinition {
    ProfileDefinition {
        name: "designer",
        description: "Images, vector assets, design documents, and PDFs.",
        associations: vec![
            image_association("png"),
            image_association("jpg"),
            image_association("jpeg"),
            image_association("svg"),
            ProfileAssociation {
                extension: "pdf",
                candidates: vec![
                    candidate(
                        "com.apple.Preview",
                        "Built-in fast PDF viewing and annotation.",
                    ),
                    candidate(
                        "com.adobe.Acrobat.Pro",
                        "Advanced PDF editing and production workflows.",
                    ),
                ],
            },
        ],
    }
}

fn image_association(extension: &'static str) -> ProfileAssociation {
    ProfileAssociation {
        extension,
        candidates: vec![
            candidate(
                "com.pixelmatorteam.pixelmator.x",
                "Native image editing workflow for design assets.",
            ),
            candidate(
                "com.apple.Preview",
                "Built-in lightweight image inspection.",
            ),
        ],
    }
}

fn media_profile() -> ProfileDefinition {
    ProfileDefinition {
        name: "media",
        description: "Audio and video playback with native and broad-codec options.",
        associations: vec![
            media_association("mp3"),
            media_association("m4a"),
            media_association("wav"),
            media_association("flac"),
            video_association("mp4"),
            video_association("mov"),
            video_association("mkv"),
        ],
    }
}

fn media_association(extension: &'static str) -> ProfileAssociation {
    ProfileAssociation {
        extension,
        candidates: vec![
            candidate(
                "com.apple.Music",
                "Built-in music library and audio playback.",
            ),
            candidate("org.videolan.vlc", "Broad codec support for audio files."),
            candidate(
                "com.colliderli.iina",
                "Native macOS player with broad format support.",
            ),
        ],
    }
}

fn video_association(extension: &'static str) -> ProfileAssociation {
    ProfileAssociation {
        extension,
        candidates: vec![
            candidate(
                "com.colliderli.iina",
                "Native macOS player with broad codec support.",
            ),
            candidate("org.videolan.vlc", "Broad codec support for video files."),
            candidate(
                "com.apple.QuickTimePlayerX",
                "Built-in playback for common Apple-supported formats.",
            ),
        ],
    }
}

fn minimal_profile() -> ProfileDefinition {
    ProfileDefinition {
        name: "minimal",
        description: "Built-in macOS applications with no third-party dependency.",
        associations: vec![
            ProfileAssociation {
                extension: "txt",
                candidates: vec![candidate(
                    "com.apple.TextEdit",
                    "Built-in macOS plain-text editor.",
                )],
            },
            ProfileAssociation {
                extension: "rtf",
                candidates: vec![candidate(
                    "com.apple.TextEdit",
                    "Built-in macOS rich-text editor.",
                )],
            },
            ProfileAssociation {
                extension: "pdf",
                candidates: vec![candidate(
                    "com.apple.Preview",
                    "Built-in macOS document viewer.",
                )],
            },
            ProfileAssociation {
                extension: "png",
                candidates: vec![candidate(
                    "com.apple.Preview",
                    "Built-in macOS image viewer.",
                )],
            },
            ProfileAssociation {
                extension: "jpg",
                candidates: vec![candidate(
                    "com.apple.Preview",
                    "Built-in macOS image viewer.",
                )],
            },
            ProfileAssociation {
                extension: "mov",
                candidates: vec![candidate(
                    "com.apple.QuickTimePlayerX",
                    "Built-in macOS media player.",
                )],
            },
        ],
    }
}

fn candidate(bundle_id: &'static str, rationale: &'static str) -> ProfileCandidate {
    ProfileCandidate {
        bundle_id,
        rationale,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(name: &str, bundle_id: &str, extensions: &[&str]) -> Application {
        Application {
            name: name.to_owned(),
            path: PathBuf::from(format!("/Applications/{name}.app")),
            bundle_id: Some(bundle_id.to_owned()),
            extensions: extensions.iter().map(|value| (*value).to_owned()).collect(),
            handlers: Vec::new(),
            type_declarations: Vec::new(),
        }
    }

    #[test]
    fn exposes_the_four_builtin_profiles() {
        assert_eq!(
            profiles()
                .iter()
                .map(|profile| profile.name)
                .collect::<Vec<_>>(),
            vec!["developer", "designer", "media", "minimal"]
        );
        assert!(find_profile("DEVELOPER").is_some());
        assert!(find_profile("unknown").is_none());
    }

    #[test]
    fn profile_overlays_extend_builtins_and_add_typed_custom_profiles() {
        let mut effective = profiles()
            .into_iter()
            .map(EffectiveProfileDefinition::from)
            .collect::<Vec<_>>();
        apply_profile_overlay_document(
            &mut effective,
            r#"
                version = 1

                [[profiles]]
                name = "Developer"
                description = "Team development defaults."

                [[profiles.associations]]
                kind = "extension"
                identifier = ".MD"
                applications = ["com.example.TeamEditor", "dev.zed.Zed"]

                [[profiles.associations]]
                kind = "uti"
                identifier = "Public.Source-Code"
                role = "editor"
                applications = ["com.example.TeamEditor"]

                [[profiles]]
                name = "support-team"
                description = "Support links and text."

                [[profiles.associations]]
                kind = "url_scheme"
                identifier = "HTTPS://"
                applications = ["com.example.Browser"]
            "#,
        )
        .unwrap();

        let developer = effective
            .iter()
            .find(|profile| profile.name == "developer")
            .unwrap();
        assert_eq!(developer.description, "Team development defaults.");
        let markdown = developer
            .associations
            .iter()
            .find(|item| item.association.identifier == "md")
            .unwrap();
        assert_eq!(markdown.candidates[0].bundle_id, "com.example.TeamEditor");
        assert_eq!(markdown.candidates[1].bundle_id, "dev.zed.Zed");
        assert_eq!(
            markdown
                .candidates
                .iter()
                .filter(|candidate| candidate.bundle_id == "dev.zed.Zed")
                .count(),
            1
        );
        assert!(developer.associations.iter().any(|item| {
            item.association.kind == AssociationKind::Uti
                && item.association.identifier == "public.source-code"
                && item.association.role == HandlerRole::Editor
        }));

        let custom = effective
            .iter()
            .find(|profile| profile.name == "support-team")
            .unwrap();
        assert_eq!(custom.associations.len(), 1);
        assert_eq!(
            custom.associations[0].association.kind,
            AssociationKind::UrlScheme
        );
        assert_eq!(custom.associations[0].association.identifier, "https");
    }

    #[test]
    fn repository_profile_overlay_example_uses_the_current_schema() {
        let mut effective = profiles()
            .into_iter()
            .map(EffectiveProfileDefinition::from)
            .collect::<Vec<_>>();
        apply_profile_overlay_document(
            &mut effective,
            include_str!("../dutis.profiles.example.toml"),
        )
        .unwrap();
        assert!(effective
            .iter()
            .any(|profile| profile.name == "support-team"));
    }

    #[test]
    fn profile_overlay_can_replace_builtin_candidates_and_rejects_invalid_input() {
        let mut effective = profiles()
            .into_iter()
            .map(EffectiveProfileDefinition::from)
            .collect::<Vec<_>>();
        apply_profile_overlay_document(
            &mut effective,
            r#"
                version = 1
                [[profiles]]
                name = "minimal"
                replace = true
                [[profiles.associations]]
                identifier = "txt"
                replace_candidates = true
                applications = ["com.example.Editor"]
            "#,
        )
        .unwrap();
        let minimal = effective
            .iter()
            .find(|profile| profile.name == "minimal")
            .unwrap();
        assert_eq!(minimal.associations.len(), 1);
        assert_eq!(minimal.associations[0].candidates.len(), 1);
        assert_eq!(
            minimal.associations[0].candidates[0].bundle_id,
            "com.example.Editor"
        );

        let invalid_documents = [
            "version = 2",
            "version = 1\n[[profiles]]\nname = 'new'",
            "version = 1\n[[profiles]]\nname = '--'\ndescription = 'invalid'\n[[profiles.associations]]\nidentifier = 'txt'\napplications = ['x']",
            "version = 1\n[[profiles]]\nname = 'developer'\n[[profiles.associations]]\nidentifier = 'md'\napplications = ['x', 'x']",
            "version = 1\n[[profiles]]\nname = 'developer'\n[[profiles.associations]]\nkind = 'url_scheme'\nidentifier = 'https'\nrole = 'viewer'\napplications = ['x']",
        ];
        for document in invalid_documents {
            let mut effective = profiles()
                .into_iter()
                .map(EffectiveProfileDefinition::from)
                .collect::<Vec<_>>();
            assert!(apply_profile_overlay_document(&mut effective, document).is_err());
        }
    }

    #[test]
    fn typed_overlay_profile_uses_the_governed_recommendation_pipeline() {
        let mut effective = profiles()
            .into_iter()
            .map(EffectiveProfileDefinition::from)
            .collect::<Vec<_>>();
        apply_profile_overlay_document(
            &mut effective,
            r#"
                version = 1
                [[profiles]]
                name = "writers"
                description = "Local writing tools."
                [[profiles.associations]]
                kind = "mime"
                identifier = "Text/Plain"
                role = "editor"
                applications = ["com.example.Writer"]
            "#,
        )
        .unwrap();
        let profile = effective
            .iter()
            .find(|profile| profile.name == "writers")
            .unwrap();
        let mut writer = app("Writer", "com.example.Writer", &[]);
        writer.handlers.push(crate::plist_parser::DeclaredHandler {
            kind: AssociationKind::Mime,
            identifier: "text/plain".to_owned(),
            role: HandlerRole::Editor,
            source: crate::plist_parser::HandlerDeclarationSource::DocumentType,
        });

        let recommendation = recommend_effective_profile_with_policy(
            profile,
            &[writer],
            |_| Ok(None),
            &Policy::default(),
        )
        .unwrap();
        assert_eq!(recommendation.summary.changes, 1);
        assert_eq!(recommendation.plan.entries[0].kind, AssociationKind::Mime);
        assert_eq!(recommendation.plan.entries[0].extension, "text/plain");
        assert!(recommendation.recommendations[0].evidence[0].declares_target);
    }

    #[test]
    fn keeps_a_compatible_current_handler_to_minimize_churn() {
        let profile = ProfileDefinition {
            name: "test",
            description: "test profile",
            associations: vec![ProfileAssociation {
                extension: "md",
                candidates: vec![
                    candidate("com.example.First", "first"),
                    candidate("com.example.Current", "current"),
                ],
            }],
        };
        let applications = vec![
            app("First", "com.example.First", &["md"]),
            app("Current", "com.example.Current", &["md"]),
        ];
        let recommendation = recommend_profile(&profile, &applications, |extension| {
            Ok(Some(DefaultApplication {
                kind: AssociationKind::Extension,
                role: HandlerRole::All,
                extension: extension.to_owned(),
                name: Some("Current".to_owned()),
                path: Some("/Applications/Current.app".to_owned()),
                bundle_id: "com.example.Current".to_owned(),
            }))
        })
        .unwrap();
        assert_eq!(recommendation.summary.kept_current, 1);
        assert_eq!(recommendation.summary.changes, 0);
        assert_eq!(
            recommendation.recommendations[0]
                .target
                .as_ref()
                .unwrap()
                .bundle_id,
            "com.example.Current"
        );
    }

    #[test]
    fn selects_first_uniquely_installed_candidate_with_evidence() {
        let profile = ProfileDefinition {
            name: "test",
            description: "test profile",
            associations: vec![ProfileAssociation {
                extension: "md",
                candidates: vec![
                    candidate("com.example.Missing", "preferred"),
                    candidate("com.example.Editor", "available"),
                ],
            }],
        };
        let recommendation = recommend_profile(
            &profile,
            &[app("Editor", "com.example.Editor", &["md"])],
            |_| Ok(None),
        )
        .unwrap();
        assert_eq!(recommendation.summary.changes, 1);
        assert_eq!(recommendation.plan.summary.changes, 1);
        assert_eq!(recommendation.recommendations[0].evidence[1].priority, 2);
        assert!(recommendation.recommendations[0].evidence[1].selected);
        assert!(recommendation.recommendations[0].evidence[1].declares_extension);
        assert!(recommendation.proposed_toml.contains("com.example.Editor"));
    }

    #[test]
    fn leaves_ambiguous_or_missing_candidates_out_of_the_plan() {
        let profile = ProfileDefinition {
            name: "test",
            description: "test profile",
            associations: vec![ProfileAssociation {
                extension: "md",
                candidates: vec![candidate("com.example.Editor", "candidate")],
            }],
        };
        let applications = vec![
            app("Editor", "com.example.Editor", &["md"]),
            Application {
                path: PathBuf::from("/Applications/Other/Editor.app"),
                ..app("Editor", "com.example.Editor", &["md"])
            },
        ];
        let recommendation = recommend_profile(&profile, &applications, |_| Ok(None)).unwrap();
        assert_eq!(recommendation.summary.unavailable, 1);
        assert_eq!(recommendation.plan.summary.total, 0);
        assert!(recommendation.proposed_config.associations.is_empty());
    }

    #[test]
    fn extension_policy_preference_overrides_profile_order_and_current_handler() {
        let profile = ProfileDefinition {
            name: "test",
            description: "test profile",
            associations: vec![ProfileAssociation {
                extension: "md",
                candidates: vec![
                    candidate("com.example.First", "profile first"),
                    candidate("com.example.Current", "current"),
                ],
            }],
        };
        let applications = vec![
            app("First", "com.example.First", &["md"]),
            app("Current", "com.example.Current", &["md"]),
            app("Team", "com.example.Team", &["md"]),
        ];
        let policy =
            Policy::parse("version = 1\n[recommendations.extensions]\nmd = ['com.example.Team']\n")
                .unwrap();
        let recommendation = recommend_profile_with_policy(
            &profile,
            &applications,
            |extension| {
                Ok(Some(DefaultApplication {
                    kind: AssociationKind::Extension,
                    role: HandlerRole::All,
                    extension: extension.to_owned(),
                    name: Some("Current".to_owned()),
                    path: Some("/Applications/Current.app".to_owned()),
                    bundle_id: "com.example.Current".to_owned(),
                }))
            },
            &policy,
        )
        .unwrap();
        let result = &recommendation.recommendations[0];
        assert_eq!(result.action, RecommendationAction::Change);
        assert_eq!(
            result.target.as_ref().unwrap().bundle_id,
            "com.example.Team"
        );
        assert_eq!(
            result.evidence[0].source,
            CandidateSource::ExtensionPreference
        );
        assert!(result.evidence[0].selected);
        assert_eq!(recommendation.plan.summary.changes, 1);
    }

    #[test]
    fn application_allowlist_skips_disallowed_profile_candidates() {
        let profile = ProfileDefinition {
            name: "test",
            description: "test profile",
            associations: vec![ProfileAssociation {
                extension: "md",
                candidates: vec![
                    candidate("com.example.Denied", "first"),
                    candidate("com.example.Allowed", "second"),
                ],
            }],
        };
        let applications = vec![
            app("Denied", "com.example.Denied", &["md"]),
            app("Allowed", "com.example.Allowed", &["md"]),
        ];
        let policy =
            Policy::parse("version = 1\nallowed_applications = ['com.example.Allowed']\n").unwrap();
        let recommendation =
            recommend_profile_with_policy(&profile, &applications, |_| Ok(None), &policy).unwrap();
        let result = &recommendation.recommendations[0];
        assert_eq!(
            result.target.as_ref().unwrap().bundle_id,
            "com.example.Allowed"
        );
        assert!(!result.evidence[0].policy_eligible);
        assert!(result.evidence[0].policy_reasons[0].contains("not allowed"));
        assert!(result.evidence[1].selected);
        assert!(policy.assess(&recommendation.plan).allowed);
    }

    #[test]
    fn global_policy_preference_reorders_existing_profile_candidates() {
        let profile = ProfileDefinition {
            name: "test",
            description: "test profile",
            associations: vec![ProfileAssociation {
                extension: "md",
                candidates: vec![
                    candidate("com.example.First", "first"),
                    candidate("com.example.Standard", "standard"),
                ],
            }],
        };
        let applications = vec![
            app("First", "com.example.First", &["md"]),
            app("Standard", "com.example.Standard", &["md"]),
        ];
        let policy = Policy::parse(
            "version = 1\n[recommendations]\npreferred_applications = ['com.example.Standard']\n",
        )
        .unwrap();
        let recommendation =
            recommend_profile_with_policy(&profile, &applications, |_| Ok(None), &policy).unwrap();
        let result = &recommendation.recommendations[0];
        assert_eq!(
            result.target.as_ref().unwrap().bundle_id,
            "com.example.Standard"
        );
        assert_eq!(result.evidence[0].source, CandidateSource::GlobalPreference);
    }

    #[test]
    fn protected_association_introduces_and_selects_the_required_application() {
        let profile = ProfileDefinition {
            name: "test",
            description: "test profile",
            associations: vec![ProfileAssociation {
                extension: "md",
                candidates: vec![candidate("com.example.Profile", "profile")],
            }],
        };
        let applications = vec![
            app("Profile", "com.example.Profile", &["md"]),
            app("Required", "com.example.Required", &["md"]),
        ];
        let policy =
            Policy::parse("version = 1\n[protected_associations]\nmd = 'com.example.Required'\n")
                .unwrap();
        let recommendation =
            recommend_profile_with_policy(&profile, &applications, |_| Ok(None), &policy).unwrap();
        let result = &recommendation.recommendations[0];
        assert_eq!(
            result.target.as_ref().unwrap().bundle_id,
            "com.example.Required"
        );
        assert_eq!(result.evidence[0].source, CandidateSource::ProtectedPolicy);
        assert!(!result.evidence[1].policy_eligible);
        assert!(policy.assess(&recommendation.plan).allowed);
    }

    #[test]
    fn reports_policy_blocked_when_only_installed_candidate_is_disallowed() {
        let profile = ProfileDefinition {
            name: "test",
            description: "test profile",
            associations: vec![ProfileAssociation {
                extension: "md",
                candidates: vec![candidate("com.example.Editor", "candidate")],
            }],
        };
        let policy =
            Policy::parse("version = 1\nallowed_applications = ['com.example.Other']\n").unwrap();
        let recommendation = recommend_profile_with_policy(
            &profile,
            &[app("Editor", "com.example.Editor", &["md"])],
            |_| Ok(None),
            &policy,
        )
        .unwrap();
        assert_eq!(
            recommendation.recommendations[0].action,
            RecommendationAction::PolicyBlocked
        );
        assert_eq!(recommendation.summary.policy_blocked, 1);
        assert_eq!(recommendation.summary.unavailable, 0);
        assert_eq!(recommendation.plan.summary.total, 0);
    }

    #[test]
    fn typed_policy_preference_builds_a_declared_handler_proposal() {
        let profile = ProfileDefinition {
            name: "test",
            description: "test profile",
            associations: Vec::new(),
        };
        let mut browser = app("Browser", "com.example.Browser", &[]);
        browser.handlers.push(crate::plist_parser::DeclaredHandler {
            kind: AssociationKind::Uti,
            identifier: "public.html".to_owned(),
            role: HandlerRole::Editor,
            source: crate::plist_parser::HandlerDeclarationSource::DocumentType,
        });
        let policy = Policy::parse(
            r#"
                version = 1
                allowed_kinds = ["uti"]

                [[recommendations.handlers]]
                kind = "uti"
                identifier = "Public.HTML"
                role = "viewer"
                applications = ["com.example.Browser"]
            "#,
        )
        .unwrap();
        let recommendation = recommend_profile_with_policy_typed(
            &profile,
            &[browser],
            |association| {
                assert_eq!(association.kind, AssociationKind::Uti);
                assert_eq!(association.identifier, "public.html");
                assert_eq!(association.role, HandlerRole::Viewer);
                Ok(None)
            },
            &policy,
        )
        .unwrap();

        let result = &recommendation.recommendations[0];
        assert_eq!(result.association.kind, AssociationKind::Uti);
        assert_eq!(result.association.role, HandlerRole::Viewer);
        assert_eq!(result.extension, "public.html");
        assert_eq!(result.action, RecommendationAction::Change);
        assert_eq!(
            result.evidence[0].source,
            CandidateSource::HandlerPreference
        );
        assert!(result.evidence[0].declares_target);
        assert!(result.evidence[0].selected);
        assert_eq!(recommendation.proposed_config.handlers.len(), 1);
        assert_eq!(recommendation.plan.entries[0].kind, AssociationKind::Uti);
        assert_eq!(recommendation.plan.entries[0].role, HandlerRole::Viewer);
        assert!(policy.assess(&recommendation.plan).allowed);
    }

    #[test]
    fn typed_policy_preference_requires_matching_application_metadata() {
        let profile = ProfileDefinition {
            name: "test",
            description: "test profile",
            associations: Vec::new(),
        };
        let policy = Policy::parse(
            r#"
                version = 1
                [[recommendations.handlers]]
                kind = "url_scheme"
                identifier = "HTTPS://"
                applications = ["com.example.Browser"]
            "#,
        )
        .unwrap();
        let recommendation = recommend_profile_with_policy_typed(
            &profile,
            &[app("Browser", "com.example.Browser", &[])],
            |_| Ok(None),
            &policy,
        )
        .unwrap();

        let result = &recommendation.recommendations[0];
        assert_eq!(result.action, RecommendationAction::Unavailable);
        assert!(!result.evidence[0].declares_target);
        assert!(!result.evidence[0].selected);
        assert!(recommendation.proposed_config.handlers.is_empty());
        assert_eq!(recommendation.plan.summary.total, 0);
    }

    #[test]
    fn typed_preferences_cover_mime_and_url_scheme_targets() {
        let profile = ProfileDefinition {
            name: "test",
            description: "test profile",
            associations: Vec::new(),
        };
        let mut application = app("Universal", "com.example.Universal", &[]);
        application.handlers.extend([
            crate::plist_parser::DeclaredHandler {
                kind: AssociationKind::Mime,
                identifier: "text/plain".to_owned(),
                role: HandlerRole::Editor,
                source: crate::plist_parser::HandlerDeclarationSource::DocumentType,
            },
            crate::plist_parser::DeclaredHandler {
                kind: AssociationKind::UrlScheme,
                identifier: "example".to_owned(),
                role: HandlerRole::All,
                source: crate::plist_parser::HandlerDeclarationSource::UrlType,
            },
        ]);
        let policy = Policy::parse(
            r#"
                version = 1
                [[recommendations.handlers]]
                kind = "mime"
                identifier = "Text/Plain"
                role = "editor"
                applications = ["com.example.Universal"]

                [[recommendations.handlers]]
                kind = "url_scheme"
                identifier = "EXAMPLE://"
                applications = ["com.example.Universal"]
            "#,
        )
        .unwrap();
        let recommendation =
            recommend_profile_with_policy_typed(&profile, &[application], |_| Ok(None), &policy)
                .unwrap();

        assert_eq!(recommendation.summary.changes, 2);
        assert_eq!(recommendation.proposed_config.handlers.len(), 2);
        assert_eq!(
            recommendation
                .recommendations
                .iter()
                .map(|item| (&item.association.kind, item.association.identifier.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (&AssociationKind::Mime, "text/plain"),
                (&AssociationKind::UrlScheme, "example")
            ]
        );
        assert!(recommendation
            .recommendations
            .iter()
            .all(|item| item.evidence[0].declares_target && item.evidence[0].selected));
    }
}
