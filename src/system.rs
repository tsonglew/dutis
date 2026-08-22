use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use std::io;
use std::process::Command;

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct DefaultApplication {
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
    duti_version()?;
    query_default_app(extension)
}

pub fn query_default_app(extension: &str) -> Result<Option<DefaultApplication>> {
    let output = Command::new("duti")
        .args(["-x", extension])
        .output()
        .context("failed to query the default application")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("Failed to get default application") {
            return Ok(None);
        }
        bail!("duti could not query .{extension}: {}", stderr.trim());
    }

    parse_default_app(extension, &String::from_utf8_lossy(&output.stdout)).map(Some)
}

pub fn set_default_app(extension: &str, bundle_id: &str) -> Result<()> {
    duti_version()?;
    let extension_argument = format!(".{extension}");
    let output = Command::new("duti")
        .args(["-s", bundle_id, &extension_argument, "all"])
        .output()
        .context("failed to run duti")?;
    if !output.status.success() {
        bail!(
            "duti could not apply the setting: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let actual = query_default_app(extension)?
        .ok_or_else(|| anyhow!("verification found no default application for .{extension}"))?;
    if actual.bundle_id != bundle_id {
        bail!(
            "verification returned bundle ID '{}' instead of '{}'",
            actual.bundle_id,
            bundle_id
        );
    }
    Ok(())
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
}
