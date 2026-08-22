use crate::application::normalize_extension;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub const CONFIG_VERSION: u32 = 1;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DutisConfig {
    pub version: u32,
    pub associations: BTreeMap<String, String>,
}

impl DutisConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read configuration {}", path.display()))?;
        Self::parse(&contents).with_context(|| format!("invalid configuration {}", path.display()))
    }

    pub fn parse(contents: &str) -> Result<Self> {
        let parsed: Self = toml::from_str(contents).context("failed to parse TOML")?;
        if parsed.version != CONFIG_VERSION {
            bail!(
                "unsupported configuration version {}; expected {}",
                parsed.version,
                CONFIG_VERSION
            );
        }

        let mut associations = BTreeMap::new();
        for (input_extension, input_selector) in parsed.associations {
            let extension = normalize_extension(&input_extension)?;
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

        Ok(Self {
            version: parsed.version,
            associations,
        })
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
    }

    #[test]
    fn rejects_unknown_versions_and_duplicate_normalized_extensions() {
        assert!(DutisConfig::parse("version = 2\n[associations]\nmd = 'Editor'").is_err());
        assert!(
            DutisConfig::parse("version = 1\n[associations]\nmd = 'Editor'\n'.MD' = 'Other'")
                .is_err()
        );
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
