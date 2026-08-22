use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const LAUNCH_AGENT_LABEL: &str = "io.github.tsonglew.dutis.watch";
const LAUNCH_AGENT_DIR_ENV: &str = "DUTIS_LAUNCH_AGENT_DIR";

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LaunchAgentSpec {
    pub executable: PathBuf,
    pub config: PathBuf,
    pub interval_seconds: u64,
    pub notify: bool,
    pub remediation_requester: Option<String>,
    pub state_dir: PathBuf,
    pub environment: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct LaunchAgentStatus {
    pub label: &'static str,
    pub path: PathBuf,
    pub installed: bool,
    pub loaded: Option<bool>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct LaunchAgentPlist {
    label: &'static str,
    program_arguments: Vec<String>,
    run_at_load: bool,
    keep_alive: bool,
    throttle_interval: u64,
    process_type: &'static str,
    standard_out_path: String,
    standard_error_path: String,
    environment_variables: BTreeMap<String, String>,
}

pub struct LaunchAgentManager {
    directory: PathBuf,
}

impl LaunchAgentManager {
    pub fn from_environment() -> Result<Self> {
        if let Some(directory) =
            std::env::var_os(LAUNCH_AGENT_DIR_ENV).filter(|value| !value.is_empty())
        {
            return Ok(Self {
                directory: PathBuf::from(directory),
            });
        }
        let home = std::env::var_os("HOME")
            .ok_or_else(|| anyhow!("HOME is not set; cannot locate LaunchAgents"))?;
        Ok(Self {
            directory: PathBuf::from(home).join("Library/LaunchAgents"),
        })
    }

    pub fn path(&self) -> PathBuf {
        self.directory.join(format!("{LAUNCH_AGENT_LABEL}.plist"))
    }

    pub fn install(&self, spec: &LaunchAgentSpec) -> Result<LaunchAgentStatus> {
        validate_spec(spec)?;
        let log_directory = spec.state_dir.join("logs");
        fs::create_dir_all(&log_directory)
            .with_context(|| format!("failed to create {}", log_directory.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&log_directory, fs::Permissions::from_mode(0o700))?;
        }
        fs::create_dir_all(&self.directory)
            .with_context(|| format!("failed to create {}", self.directory.display()))?;

        let path = self.path();
        let temporary = self
            .directory
            .join(format!(".{LAUNCH_AGENT_LABEL}.{}.tmp", std::process::id()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .with_context(|| format!("failed to create {}", temporary.display()))?;
        plist::to_writer_xml(&mut file, &plist(spec))?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        fs::rename(&temporary, &path)
            .with_context(|| format!("failed to install {}", path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        }
        bootstrap(&path)?;
        self.status()
    }

    pub fn uninstall(&self) -> Result<LaunchAgentStatus> {
        let path = self.path();
        if path.exists() {
            let _ = bootout(&path);
            fs::remove_file(&path)
                .with_context(|| format!("failed to remove {}", path.display()))?;
        }
        self.status()
    }

    pub fn status(&self) -> Result<LaunchAgentStatus> {
        let path = self.path();
        let installed = path.is_file();
        let loaded = if installed {
            launchctl_loaded()?
        } else {
            Some(false)
        };
        Ok(LaunchAgentStatus {
            label: LAUNCH_AGENT_LABEL,
            path,
            installed,
            loaded,
        })
    }
}

fn validate_spec(spec: &LaunchAgentSpec) -> Result<()> {
    if !spec.executable.is_absolute() || !spec.executable.is_file() {
        bail!(
            "LaunchAgent executable must be an existing absolute path: {}",
            spec.executable.display()
        );
    }
    if !spec.config.is_absolute() || !spec.config.is_file() {
        bail!(
            "LaunchAgent configuration must be an existing absolute path: {}",
            spec.config.display()
        );
    }
    if spec.interval_seconds < 10 {
        bail!("LaunchAgent interval must be at least 10 seconds");
    }
    if spec
        .remediation_requester
        .as_deref()
        .is_some_and(|requester| requester.trim().is_empty())
    {
        bail!("remediation requester cannot be empty");
    }
    Ok(())
}

fn plist(spec: &LaunchAgentSpec) -> LaunchAgentPlist {
    let mut arguments = vec![
        spec.executable.display().to_string(),
        "watch".to_owned(),
        spec.config.display().to_string(),
        "--json".to_owned(),
        "--interval-seconds".to_owned(),
        spec.interval_seconds.to_string(),
    ];
    if spec.notify {
        arguments.push("--notify".to_owned());
    }
    if let Some(requester) = &spec.remediation_requester {
        arguments.extend([
            "--remediate".to_owned(),
            "--yes".to_owned(),
            "--requester".to_owned(),
            requester.clone(),
        ]);
    }
    LaunchAgentPlist {
        label: LAUNCH_AGENT_LABEL,
        program_arguments: arguments,
        run_at_load: true,
        keep_alive: true,
        throttle_interval: 10,
        process_type: "Background",
        standard_out_path: spec
            .state_dir
            .join("logs/watch.jsonl")
            .display()
            .to_string(),
        standard_error_path: spec
            .state_dir
            .join("logs/watch.error.log")
            .display()
            .to_string(),
        environment_variables: spec.environment.clone(),
    }
}

fn user_domain() -> Result<String> {
    let output = Command::new("/usr/bin/id")
        .arg("-u")
        .output()
        .context("failed to determine user ID for launchctl")?;
    if !output.status.success() {
        bail!("id -u failed");
    }
    Ok(format!(
        "gui/{}",
        String::from_utf8_lossy(&output.stdout).trim()
    ))
}

fn bootstrap(path: &Path) -> Result<()> {
    if !cfg!(target_os = "macos") {
        bail!("LaunchAgents are only available on macOS");
    }
    let _ = bootout(path);
    let domain = user_domain()?;
    let output = Command::new("/bin/launchctl")
        .args(["bootstrap", &domain])
        .arg(path)
        .output()
        .context("failed to run launchctl bootstrap")?;
    if !output.status.success() {
        bail!(
            "launchctl bootstrap failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

fn bootout(path: &Path) -> Result<()> {
    let domain = user_domain()?;
    let output = Command::new("/bin/launchctl")
        .args(["bootout", &domain])
        .arg(path)
        .output()
        .context("failed to run launchctl bootout")?;
    if !output.status.success() {
        bail!(
            "launchctl bootout failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

fn launchctl_loaded() -> Result<Option<bool>> {
    if !cfg!(target_os = "macos") {
        return Ok(None);
    }
    let domain = user_domain()?;
    let service = format!("{domain}/{LAUNCH_AGENT_LABEL}");
    let output = Command::new("/bin/launchctl")
        .args(["print", &service])
        .output()
        .context("failed to inspect LaunchAgent")?;
    Ok(Some(output.status.success()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_notification_only_agent_without_mutation_flags() {
        let spec = LaunchAgentSpec {
            executable: PathBuf::from("/opt/homebrew/bin/dutis"),
            config: PathBuf::from("/Users/test/dutis.toml"),
            interval_seconds: 300,
            notify: true,
            remediation_requester: None,
            state_dir: PathBuf::from("/Users/test/Library/Application Support/dutis"),
            environment: BTreeMap::from([(
                "PATH".to_owned(),
                "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin".to_owned(),
            )]),
        };
        let value = plist::to_value(&plist(&spec)).unwrap();
        let dictionary = value.as_dictionary().unwrap();
        let arguments = dictionary["ProgramArguments"].as_array().unwrap();
        assert!(arguments
            .iter()
            .any(|value| value.as_string() == Some("--notify")));
        assert!(!arguments
            .iter()
            .any(|value| value.as_string() == Some("--remediate")));
        assert_eq!(dictionary["KeepAlive"].as_boolean(), Some(true));
        assert!(arguments.windows(2).any(|pair| {
            pair[0].as_string() == Some("--interval-seconds") && pair[1].as_string() == Some("300")
        }));
    }

    #[test]
    fn remediation_agent_records_explicit_opt_in_and_requester() {
        let spec = LaunchAgentSpec {
            executable: PathBuf::from("/opt/homebrew/bin/dutis"),
            config: PathBuf::from("/Users/test/dutis.toml"),
            interval_seconds: 60,
            notify: false,
            remediation_requester: Some("launch-agent".to_owned()),
            state_dir: PathBuf::from("/tmp/dutis"),
            environment: BTreeMap::new(),
        };
        let value = plist::to_value(&plist(&spec)).unwrap();
        let arguments = value.as_dictionary().unwrap()["ProgramArguments"]
            .as_array()
            .unwrap();
        for expected in ["--remediate", "--yes", "--requester", "launch-agent"] {
            assert!(arguments
                .iter()
                .any(|value| value.as_string() == Some(expected)));
        }
    }
}
