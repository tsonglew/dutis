use anyhow::{bail, Result};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(
    Debug, Clone, Copy, Default, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize, ValueEnum,
)]
#[serde(rename_all = "snake_case")]
pub enum AssociationKind {
    #[default]
    Extension,
    Uti,
    Mime,
    UrlScheme,
}

#[derive(
    Debug, Clone, Copy, Default, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize, ValueEnum,
)]
#[serde(rename_all = "snake_case")]
pub enum HandlerRole {
    #[default]
    All,
    Viewer,
    Editor,
    Shell,
}

impl HandlerRole {
    pub fn as_duti_argument(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Viewer => "viewer",
            Self::Editor => "editor",
            Self::Shell => "shell",
        }
    }
}

#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct AssociationTarget {
    #[serde(default)]
    pub kind: AssociationKind,
    pub identifier: String,
    #[serde(default)]
    pub role: HandlerRole,
}

impl AssociationTarget {
    pub fn extension(value: &str) -> Result<Self> {
        Self::new(AssociationKind::Extension, value, HandlerRole::All)
    }

    pub fn new(kind: AssociationKind, value: &str, role: HandlerRole) -> Result<Self> {
        if kind == AssociationKind::UrlScheme && role != HandlerRole::All {
            bail!("URL schemes do not accept a Launch Services role");
        }
        let identifier = normalize_identifier(kind, value)?;
        Ok(Self {
            kind,
            identifier,
            role,
        })
    }

    pub fn duti_identifier(&self) -> String {
        if self.kind == AssociationKind::Extension {
            format!(".{}", self.identifier)
        } else {
            self.identifier.clone()
        }
    }

    pub fn display_name(&self) -> String {
        match self.kind {
            AssociationKind::Extension => format!(".{}", self.identifier),
            AssociationKind::Uti => format!("UTI {}", self.identifier),
            AssociationKind::Mime => format!("MIME {}", self.identifier),
            AssociationKind::UrlScheme => format!("{}://", self.identifier),
        }
    }
}

impl fmt::Display for AssociationTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.display_name())?;
        if self.kind != AssociationKind::UrlScheme && self.role != HandlerRole::All {
            write!(formatter, " ({})", self.role.as_duti_argument())?;
        }
        Ok(())
    }
}

pub fn normalize_identifier(kind: AssociationKind, value: &str) -> Result<String> {
    let value = value.trim();
    match kind {
        AssociationKind::Extension => {
            let normalized = value.trim_start_matches('.').to_ascii_lowercase();
            if normalized.is_empty()
                || normalized.len() > 64
                || !normalized.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '+')
                })
            {
                bail!("invalid filename extension '{value}'");
            }
            Ok(normalized)
        }
        AssociationKind::Uti => {
            let normalized = value.to_ascii_lowercase();
            if normalized.is_empty()
                || normalized.len() > 255
                || !normalized.contains('.')
                || !normalized.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '.' | '-')
                })
            {
                bail!("invalid UTI '{value}'");
            }
            Ok(normalized)
        }
        AssociationKind::Mime => {
            let normalized = value.to_ascii_lowercase();
            let mut parts = normalized.split('/');
            let type_name = parts.next().unwrap_or_default();
            let subtype = parts.next().unwrap_or_default();
            if type_name.is_empty()
                || subtype.is_empty()
                || parts.next().is_some()
                || !normalized.chars().all(|character| {
                    character.is_ascii_alphanumeric()
                        || matches!(
                            character,
                            '/' | '!' | '#' | '$' | '&' | '^' | '_' | '+' | '-' | '.'
                        )
                })
            {
                bail!("invalid MIME type '{value}'");
            }
            Ok(normalized)
        }
        AssociationKind::UrlScheme => {
            let normalized = value
                .trim_end_matches("://")
                .trim_end_matches(':')
                .to_ascii_lowercase();
            let mut characters = normalized.chars();
            if normalized.is_empty()
                || normalized.len() > 64
                || !characters
                    .next()
                    .is_some_and(|character| character.is_ascii_alphabetic())
                || !characters.all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.')
                })
            {
                bail!("invalid URL scheme '{value}'");
            }
            Ok(normalized)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_each_supported_kind() {
        assert_eq!(
            AssociationTarget::extension(".MD").unwrap().identifier,
            "md"
        );
        assert_eq!(
            AssociationTarget::new(AssociationKind::Uti, "Public.HTML", HandlerRole::Viewer)
                .unwrap()
                .identifier,
            "public.html"
        );
        assert_eq!(
            AssociationTarget::new(AssociationKind::Mime, "Text/Plain", HandlerRole::All)
                .unwrap()
                .identifier,
            "text/plain"
        );
        assert_eq!(
            AssociationTarget::new(AssociationKind::UrlScheme, "HTTPS://", HandlerRole::All)
                .unwrap()
                .identifier,
            "https"
        );
    }

    #[test]
    fn rejects_invalid_identifiers_and_url_roles() {
        assert!(AssociationTarget::extension("../../etc/passwd").is_err());
        assert!(AssociationTarget::new(AssociationKind::Uti, "plain", HandlerRole::All).is_err());
        assert!(AssociationTarget::new(AssociationKind::Mime, "text", HandlerRole::All).is_err());
        assert!(
            AssociationTarget::new(AssociationKind::UrlScheme, "123bad", HandlerRole::All).is_err()
        );
        assert!(
            AssociationTarget::new(AssociationKind::UrlScheme, "https", HandlerRole::Viewer)
                .is_err()
        );
    }
}
