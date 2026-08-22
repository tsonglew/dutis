use crate::association::{AssociationKind, AssociationTarget, HandlerRole};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub const CONFIG_VERSION: u32 = 2;
pub const LEGACY_CONFIG_VERSION: u32 = 1;

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct DutisConfig {
    pub version: u32,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub associations: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub handlers: Vec<AssociationRule>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssociationRule {
    pub kind: AssociationKind,
    pub identifier: String,
    #[serde(default)]
    pub role: HandlerRole,
    pub application: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    version: u32,
    #[serde(default)]
    associations: BTreeMap<String, String>,
    #[serde(default)]
    handlers: Vec<AssociationRule>,
}

impl DutisConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read configuration {}", path.display()))?;
        Self::parse(&contents).with_context(|| format!("invalid configuration {}", path.display()))
    }

    pub fn parse(contents: &str) -> Result<Self> {
        let parsed: RawConfig = toml::from_str(contents).context("failed to parse TOML")?;
        if !matches!(parsed.version, LEGACY_CONFIG_VERSION | CONFIG_VERSION) {
            bail!(
                "unsupported configuration version {}; expected {} or {}",
                parsed.version,
                LEGACY_CONFIG_VERSION,
                CONFIG_VERSION
            );
        }
        if parsed.version == LEGACY_CONFIG_VERSION && !parsed.handlers.is_empty() {
            bail!("typed handlers require configuration version {CONFIG_VERSION}");
        }

        let mut associations = BTreeMap::new();
        for (input_extension, input_selector) in parsed.associations {
            let extension = AssociationTarget::extension(&input_extension)?.identifier;
            let selector = input_selector.trim();
            if selector.is_empty() {
                bail!("application selector for .{extension} cannot be empty");
            }
            if associations
                .insert(extension.clone(), selector.to_owned())
                .is_some()
            {
                bail!("duplicate normalized extension .{extension}");
            }
        }

        let mut seen = associations
            .keys()
            .map(|extension| {
                AssociationTarget::extension(extension).expect("normalized extension is valid")
            })
            .collect::<BTreeSet<_>>();
        let mut handlers = Vec::with_capacity(parsed.handlers.len());
        for handler in parsed.handlers {
            let target = AssociationTarget::new(handler.kind, &handler.identifier, handler.role)?;
            let application = handler.application.trim();
            if application.is_empty() {
                bail!("application selector for {target} cannot be empty");
            }
            if !seen.insert(target.clone()) {
                bail!("duplicate association target {target}");
            }
            handlers.push(AssociationRule {
                kind: target.kind,
                identifier: target.identifier,
                role: target.role,
                application: application.to_owned(),
            });
        }
        handlers.sort_by(|left, right| {
            (&left.kind, &left.identifier, &left.role).cmp(&(
                &right.kind,
                &right.identifier,
                &right.role,
            ))
        });

        Ok(Self {
            version: parsed.version,
            associations,
            handlers,
        })
    }

    pub fn rules(&self) -> Result<Vec<(AssociationTarget, &str)>> {
        let mut rules = self
            .associations
            .iter()
            .map(|(extension, application)| {
                Ok((
                    AssociationTarget::extension(extension)?,
                    application.as_str(),
                ))
            })
            .chain(self.handlers.iter().map(|handler| {
                Ok((
                    AssociationTarget::new(handler.kind, &handler.identifier, handler.role)?,
                    handler.application.as_str(),
                ))
            }))
            .collect::<Result<Vec<_>>>()?;
        rules.sort_by(|left, right| left.0.cmp(&right.0));
        Ok(rules)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_normalizes_versioned_configuration() {
        let config = DutisConfig::parse(
            r#"
                version = 1

                [associations]
                ".MD" = "com.example.Editor"
                json = "/Applications/Editor.app"
            "#,
        )
        .unwrap();

        assert_eq!(config.version, 1);
        assert_eq!(config.associations["md"], "com.example.Editor");
        assert_eq!(config.associations["json"], "/Applications/Editor.app");
        assert!(config.handlers.is_empty());
    }

    #[test]
    fn rejects_unknown_versions_and_duplicate_normalized_extensions() {
        assert!(DutisConfig::parse("version = 3\n[associations]\nmd = 'Editor'").is_err());
        assert!(
            DutisConfig::parse("version = 1\n[associations]\nmd = 'Editor'\n'.MD' = 'Other'")
                .is_err()
        );
    }

    #[test]
    fn parses_and_orders_typed_handlers_in_version_two() {
        let config = DutisConfig::parse(
            r#"
                version = 2

                [associations]
                md = "com.example.Editor"

                [[handlers]]
                kind = "url_scheme"
                identifier = "HTTPS://"
                application = "com.example.Browser"

                [[handlers]]
                kind = "uti"
                identifier = "Public.HTML"
                role = "viewer"
                application = "com.example.Browser"
            "#,
        )
        .unwrap();
        let rules = config.rules().unwrap();
        assert_eq!(rules.len(), 3);
        assert_eq!(rules[0].0.kind, AssociationKind::Extension);
        assert_eq!(rules[1].0.kind, AssociationKind::Uti);
        assert_eq!(rules[1].0.identifier, "public.html");
        assert_eq!(rules[1].0.role, HandlerRole::Viewer);
        assert_eq!(rules[2].0.kind, AssociationKind::UrlScheme);
        assert_eq!(rules[2].0.identifier, "https");
    }

    #[test]
    fn keeps_version_one_compatible_and_rejects_typed_handlers() {
        assert!(DutisConfig::parse("version = 1\n[associations]\nmd = 'Editor'").is_ok());
        assert!(DutisConfig::parse(
            "version = 1\n[[handlers]]\nkind = 'uti'\nidentifier = 'public.text'\napplication = 'Editor'"
        )
        .is_err());
    }

    #[test]
    fn rejects_duplicate_targets_across_legacy_and_typed_entries() {
        assert!(DutisConfig::parse(
            "version = 2\n[associations]\nmd = 'Editor'\n[[handlers]]\nkind = 'extension'\nidentifier = '.MD'\napplication = 'Other'"
        )
        .is_err());
    }

    #[test]
    fn rejects_unknown_fields_and_empty_selectors() {
        assert!(
            DutisConfig::parse("version = 1\nextra = true\n[associations]\nmd = 'Editor'").is_err()
        );
        assert!(DutisConfig::parse("version = 1\n[associations]\nmd = '   '").is_err());
    }

    #[test]
    fn repository_example_uses_the_current_schema() {
        let config = DutisConfig::parse(include_str!("../dutis.example.toml")).unwrap();
        assert_eq!(config.version, CONFIG_VERSION);
        assert!(!config.associations.is_empty());
    }
}
