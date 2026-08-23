use crate::association::{AssociationKind, AssociationTarget, HandlerRole};
use crate::launch_services;
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::io;
use std::process::Command;

pub const HANDLER_READ_BACKEND_ENV: &str = "DUTIS_HANDLER_READ_BACKEND";

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum HandlerReadBackend {
    Duti,
    Native,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DefaultApplication {
    #[serde(default)]
    pub kind: AssociationKind,
    #[serde(default)]
    pub role: HandlerRole,
    /// Normalized identifier. The legacy field name preserves JSON compatibility.
    pub extension: String,
    pub name: Option<String>,
    pub path: Option<String>,
    pub bundle_id: String,
}

pub fn duti_version() -> Result<String> {
    let output = Command::new("duti").arg("-V").output().map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            anyhow!("duti is required; install it with `brew install duti`")
        } else {
            anyhow!(error).context("failed to check duti")
        }
    })?;

    if !output.status.success() {
        bail!(
            "duti is installed but unavailable: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let version = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    Ok(if version.is_empty() {
        "unknown".to_owned()
    } else {
        version
    })
}

pub fn get_default_app(extension: &str) -> Result<Option<DefaultApplication>> {
    let association = AssociationTarget::extension(extension)?;
    get_default_handler(&association)
}

pub fn get_default_handler(association: &AssociationTarget) -> Result<Option<DefaultApplication>> {
    let backend = handler_read_backend(association)?;
    if backend == HandlerReadBackend::Duti {
        duti_version()?;
    }
    query_default_handler_with_backend(association, backend)
}

pub fn query_default_app(extension: &str) -> Result<Option<DefaultApplication>> {
    let association = AssociationTarget::extension(extension)?;
    query_default_handler(&association)
}

pub fn query_default_handler(
    association: &AssociationTarget,
) -> Result<Option<DefaultApplication>> {
    query_default_handler_with_backend(association, handler_read_backend(association)?)
}

fn query_default_handler_with_backend(
    association: &AssociationTarget,
    backend: HandlerReadBackend,
) -> Result<Option<DefaultApplication>> {
    match backend {
        HandlerReadBackend::Duti => query_default_handler_with_duti(association),
        HandlerReadBackend::Native => query_default_handler_with_launch_services(association),
    }
}

fn query_default_handler_with_launch_services(
    association: &AssociationTarget,
) -> Result<Option<DefaultApplication>> {
    Ok(
        launch_services::query_default_handler(association)?.map(|default| DefaultApplication {
            kind: association.kind,
            role: association.role,
            extension: association.identifier.clone(),
            name: None,
            path: None,
            bundle_id: default.bundle_id,
        }),
    )
}

fn query_default_handler_with_duti(
    association: &AssociationTarget,
) -> Result<Option<DefaultApplication>> {
    let mut command = Command::new("duti");
    match association.kind {
        AssociationKind::Extension => {
            command.args(["-x", &association.identifier]);
        }
        AssociationKind::Uti | AssociationKind::Mime | AssociationKind::UrlScheme => {
            command.args(["-d", &association.identifier]);
        }
    }
    let output = command
        .output()
        .context("failed to query the default application")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.to_ascii_lowercase().contains("no default handler")
            || stderr.contains("Failed to get default application")
        {
            return Ok(None);
        }
        bail!("duti could not query {association}: {}", stderr.trim());
    }
    if association.kind == AssociationKind::Extension {
        parse_default_app(
            &association.identifier,
            &String::from_utf8_lossy(&output.stdout),
        )
        .map(|mut default| {
            default.kind = association.kind;
            default.role = association.role;
            Some(default)
        })
    } else {
        let bundle_id = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if bundle_id.is_empty() {
            return Ok(None);
        }
        Ok(Some(DefaultApplication {
            kind: association.kind,
            role: association.role,
            extension: association.identifier.clone(),
            name: None,
            path: None,
            bundle_id,
        }))
    }
}

fn handler_read_backend(association: &AssociationTarget) -> Result<HandlerReadBackend> {
    parse_handler_read_backend(
        std::env::var(HANDLER_READ_BACKEND_ENV).ok().as_deref(),
        association,
    )
}

fn parse_handler_read_backend(
    configured: Option<&str>,
    association: &AssociationTarget,
) -> Result<HandlerReadBackend> {
    match configured.map(str::trim).filter(|value| !value.is_empty()) {
        None | Some("auto") => {
            if cfg!(target_os = "macos")
                && association.kind != AssociationKind::UrlScheme
                && association.role != HandlerRole::All
            {
                Ok(HandlerReadBackend::Native)
            } else {
                Ok(HandlerReadBackend::Duti)
            }
        }
        Some("duti") => Ok(HandlerReadBackend::Duti),
        Some("native") => Ok(HandlerReadBackend::Native),
        Some(value) => bail!(
            "invalid {HANDLER_READ_BACKEND_ENV} value '{value}'; expected auto, native, or duti"
        ),
    }
}

pub fn set_default_app(extension: &str, bundle_id: &str) -> Result<()> {
    let association = AssociationTarget::extension(extension)?;
    set_default_handler(&association, bundle_id)
}

pub fn set_default_handler(association: &AssociationTarget, bundle_id: &str) -> Result<()> {
    duti_version()?;
    let arguments = duti_set_arguments(association, bundle_id);
    let mut command = Command::new("duti");
    command.args(&arguments);
    let output = command.output().context("failed to run duti")?;
    if !output.status.success() {
        bail!(
            "duti could not apply the setting: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let actual = query_default_handler(association)?
        .ok_or_else(|| anyhow!("verification found no default application for {association}"))?;
    if actual.bundle_id != bundle_id {
        bail!(
            "verification returned bundle ID '{}' instead of '{}'",
            actual.bundle_id,
            bundle_id
        );
    }
    Ok(())
}

pub fn duti_set_arguments(association: &AssociationTarget, bundle_id: &str) -> Vec<String> {
    let mut arguments = vec![
        "-s".to_owned(),
        bundle_id.to_owned(),
        association.duti_identifier(),
    ];
    if association.kind != AssociationKind::UrlScheme {
        arguments.push(association.role.as_duti_argument().to_owned());
    }
    arguments
}

fn parse_default_app(extension: &str, output: &str) -> Result<DefaultApplication> {
    let lines = output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    let Some(bundle_id) = lines.last() else {
        bail!("no default application is registered for .{extension}");
    };

    Ok(DefaultApplication {
        kind: AssociationKind::Extension,
        role: HandlerRole::All,
        extension: extension.to_owned(),
        name: lines.first().map(|value| (*value).to_owned()),
        path: lines.get(1).map(|value| (*value).to_owned()),
        bundle_id: (*bundle_id).to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_duti_query_output() {
        let result = parse_default_app(
            "txt",
            "TextEdit\n/System/Applications/TextEdit.app\ncom.apple.TextEdit\n",
        )
        .unwrap();
        assert_eq!(result.extension, "txt");
        assert_eq!(result.name.as_deref(), Some("TextEdit"));
        assert_eq!(
            result.path.as_deref(),
            Some("/System/Applications/TextEdit.app")
        );
        assert_eq!(result.bundle_id, "com.apple.TextEdit");
    }

    #[test]
    fn rejects_empty_duti_query_output() {
        assert!(parse_default_app("unknown", "\n").is_err());
    }

    #[test]
    fn builds_kind_and_role_aware_set_arguments() {
        let extension = AssociationTarget::extension("md").unwrap();
        assert_eq!(
            duti_set_arguments(&extension, "com.example.Editor"),
            ["-s", "com.example.Editor", ".md", "all"]
        );
        let uti = AssociationTarget::new(AssociationKind::Uti, "public.html", HandlerRole::Viewer)
            .unwrap();
        assert_eq!(
            duti_set_arguments(&uti, "com.example.Browser"),
            ["-s", "com.example.Browser", "public.html", "viewer"]
        );
        let scheme =
            AssociationTarget::new(AssociationKind::UrlScheme, "https", HandlerRole::All).unwrap();
        assert_eq!(
            duti_set_arguments(&scheme, "com.example.Browser"),
            ["-s", "com.example.Browser", "https"]
        );
    }

    #[test]
    fn selects_native_backend_for_role_specific_content_queries() {
        let viewer =
            AssociationTarget::new(AssociationKind::Uti, "public.text", HandlerRole::Viewer)
                .unwrap();
        let all =
            AssociationTarget::new(AssociationKind::Uti, "public.text", HandlerRole::All).unwrap();
        let expected = if cfg!(target_os = "macos") {
            HandlerReadBackend::Native
        } else {
            HandlerReadBackend::Duti
        };
        assert_eq!(parse_handler_read_backend(None, &viewer).unwrap(), expected);
        assert_eq!(
            parse_handler_read_backend(None, &all).unwrap(),
            HandlerReadBackend::Duti
        );
        assert_eq!(
            parse_handler_read_backend(Some("duti"), &viewer).unwrap(),
            HandlerReadBackend::Duti
        );
        assert_eq!(
            parse_handler_read_backend(Some("native"), &all).unwrap(),
            HandlerReadBackend::Native
        );
        assert!(parse_handler_read_backend(Some("unknown"), &all).is_err());
    }
}
