use crate::application::Application;
use crate::association::{AssociationKind, HandlerRole};
use crate::config::{DutisConfig, CONFIG_VERSION};
use crate::planner::{assemble_plan, AssociationPlan, PlanAction, PlanEntry, PlannedApplication};
use crate::system::DefaultApplication;
use anyhow::Result;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

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

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecommendationAction {
    Change,
    KeepCurrent,
    Unavailable,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct CandidateEvidence {
    pub bundle_id: String,
    pub priority: usize,
    pub installed_paths: Vec<PathBuf>,
    pub declares_extension: bool,
    pub selected: bool,
    pub rationale: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct AssociationRecommendation {
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

pub fn recommend_profile<F>(
    profile: &ProfileDefinition,
    applications: &[Application],
    mut query_default: F,
) -> Result<ProfileRecommendation>
where
    F: FnMut(&str) -> Result<Option<DefaultApplication>>,
{
    let mut proposed_associations = BTreeMap::new();
    let mut plan_entries = Vec::new();
    let mut recommendations = Vec::with_capacity(profile.associations.len());

    for association in &profile.associations {
        let current = query_default(association.extension)?;
        let candidates = association
            .candidates
            .iter()
            .enumerate()
            .map(|(index, candidate)| {
                let matches = applications
                    .iter()
                    .filter(|application| {
                        application.bundle_id.as_deref() == Some(candidate.bundle_id)
                    })
                    .collect::<Vec<_>>();
                CandidateEvidence {
                    bundle_id: candidate.bundle_id.to_owned(),
                    priority: index + 1,
                    installed_paths: matches
                        .iter()
                        .map(|application| application.path.clone())
                        .collect(),
                    declares_extension: matches.iter().any(|application| {
                        application
                            .extensions
                            .iter()
                            .any(|extension| extension.eq_ignore_ascii_case(association.extension))
                    }),
                    selected: false,
                    rationale: candidate.rationale.to_owned(),
                }
            })
            .collect::<Vec<_>>();

        let current_candidate = current.as_ref().and_then(|current| {
            association
                .candidates
                .iter()
                .position(|candidate| candidate.bundle_id == current.bundle_id)
        });
        let selected_index = current_candidate
            .filter(|index| candidates[*index].installed_paths.len() == 1)
            .or_else(|| {
                candidates
                    .iter()
                    .position(|candidate| candidate.installed_paths.len() == 1)
            });
        let mut evidence = candidates;

        let (action, target, explanation) = if let Some(index) = selected_index {
            evidence[index].selected = true;
            let selected = &association.candidates[index];
            let application = applications
                .iter()
                .find(|application| {
                    application.bundle_id.as_deref() == Some(selected.bundle_id)
                        && application.path == evidence[index].installed_paths[0]
                })
                .expect("selected profile candidate has one installed application");
            let target = PlannedApplication::from_application(application)
                .expect("profile candidates have bundle identifiers");
            let keep_current =
                current.as_ref().map(|value| value.bundle_id.as_str()) == Some(selected.bundle_id);
            let action = if keep_current {
                RecommendationAction::KeepCurrent
            } else {
                RecommendationAction::Change
            };
            let explanation = if keep_current {
                format!(
                    "Keep {} because the current handler is compatible with the {} profile.",
                    application.name, profile.name
                )
            } else {
                format!(
                    "Recommend {} because it is the highest-priority uniquely installed candidate for .{}: {}",
                    application.name, association.extension, selected.rationale
                )
            };
            proposed_associations.insert(
                association.extension.to_owned(),
                selected.bundle_id.to_owned(),
            );
            plan_entries.push(PlanEntry {
                kind: AssociationKind::Extension,
                role: HandlerRole::All,
                extension: association.extension.to_owned(),
                selector: selected.bundle_id.to_owned(),
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
            (
                RecommendationAction::Unavailable,
                None,
                format!(
                    "No uniquely installed candidate is available for .{}; no change is proposed.",
                    association.extension
                ),
            )
        };

        recommendations.push(AssociationRecommendation {
            extension: association.extension.to_owned(),
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
        handlers: Vec::new(),
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
}
