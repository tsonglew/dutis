use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "dutis",
    version,
    about = "Manage default macOS applications by file extension",
    after_help = "Run without a command to start interactive mode.\nMore information: https://github.com/tsonglew/dutis"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<CliCommand>,
}

#[derive(Debug, Subcommand)]
pub enum CliCommand {
    /// List installed applications and their declared extensions
    List(OutputArgs),
    /// Find installed applications that declare support for an extension
    Query(ExtensionArgs),
    /// Read the current default application for an extension
    Get(ExtensionArgs),
    /// Set the default application for an extension
    Set(SetArgs),
    /// Build a deterministic plan from a declarative configuration
    Plan(ConfigArgs),
    /// Show associations that differ from a declarative configuration
    Diff(ConfigArgs),
    /// Apply and verify a previously reviewed declarative plan
    Apply(ApplyArgs),
    /// Create a local snapshot of current associations
    Snapshot(SnapshotArgs),
    /// List locally stored snapshots
    History(OutputArgs),
    /// Restore associations from a local snapshot
    Rollback(RollbackArgs),
    /// Inspect the effective local mutation policy
    Policy(PolicyArgs),
    /// List persistent local mutation audit records
    Audit(OutputArgs),
    /// Run the local Model Context Protocol server over stdio
    Mcp(McpArgs),
    /// Check whether dutis and its runtime dependency are ready
    Doctor(OutputArgs),
}

#[derive(Debug, Args)]
pub struct OutputArgs {
    /// Emit stable machine-readable JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct ExtensionArgs {
    /// Filename extension, with or without a leading dot
    pub extension: String,
    /// Emit stable machine-readable JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct SetArgs {
    /// Filename extension, with or without a leading dot
    pub extension: String,
    /// Exact bundle ID, application path, or unambiguous application name
    pub app_selector: String,
    /// Resolve and display the operation without changing the system
    #[arg(long)]
    pub dry_run: bool,
    /// Confirm a non-interactive system change
    #[arg(long)]
    pub yes: bool,
    /// Emit stable machine-readable JSON
    #[arg(long)]
    pub json: bool,
    /// Identity recorded in the local mutation audit
    #[arg(long)]
    pub requester: Option<String>,
}

#[derive(Debug, Args)]
pub struct ConfigArgs {
    /// Path to a versioned dutis TOML configuration
    pub config: PathBuf,
    /// Emit stable machine-readable JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct ApplyArgs {
    /// Path to a versioned dutis TOML configuration
    pub config: PathBuf,
    /// Digest returned by a freshly reviewed plan command
    #[arg(long)]
    pub plan_digest: Option<String>,
    /// Rebuild and display the plan without changing the system
    #[arg(long)]
    pub dry_run: bool,
    /// Confirm a non-interactive system change
    #[arg(long)]
    pub yes: bool,
    /// Emit stable machine-readable JSON
    #[arg(long)]
    pub json: bool,
    /// Identity recorded in the local mutation audit
    #[arg(long)]
    pub requester: Option<String>,
}

#[derive(Debug, Args)]
pub struct SnapshotArgs {
    #[command(subcommand)]
    pub command: SnapshotCommand,
}

#[derive(Debug, Subcommand)]
pub enum SnapshotCommand {
    /// Capture current associations without changing the system
    Create(SnapshotCreateArgs),
}

#[derive(Debug, Args)]
pub struct SnapshotCreateArgs {
    /// Limit the snapshot to extensions in this configuration
    #[arg(long)]
    pub config: Option<PathBuf>,
    /// Emit stable machine-readable JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct RollbackArgs {
    /// Snapshot identifier shown by the history command
    pub snapshot_id: String,
    /// Build and display the rollback plan without changing the system
    #[arg(long)]
    pub dry_run: bool,
    /// Confirm a non-interactive rollback
    #[arg(long)]
    pub yes: bool,
    /// Emit stable machine-readable JSON
    #[arg(long)]
    pub json: bool,
    /// Identity recorded in the local mutation audit
    #[arg(long)]
    pub requester: Option<String>,
}

#[derive(Debug, Args)]
pub struct PolicyArgs {
    #[command(subcommand)]
    pub command: PolicyCommand,
}

#[derive(Debug, Subcommand)]
pub enum PolicyCommand {
    /// Show the effective policy and its source
    Show(OutputArgs),
    /// Check a declarative plan against the effective policy
    Check(PolicyCheckArgs),
}

#[derive(Debug, Args)]
pub struct PolicyCheckArgs {
    /// Path to a versioned dutis TOML configuration
    pub config: PathBuf,
    /// Emit stable machine-readable JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct McpArgs {
    /// Register mutation tools; also requires DUTIS_MCP_APPROVAL_TOKEN
    #[arg(long)]
    pub allow_writes: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parses_agent_facing_commands() {
        let cli = Cli::try_parse_from(["dutis", "query", ".md", "--json"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(CliCommand::Query(ExtensionArgs { json: true, .. }))
        ));

        let cli = Cli::try_parse_from([
            "dutis",
            "set",
            "md",
            "com.example.Editor",
            "--dry-run",
            "--json",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Some(CliCommand::Set(SetArgs {
                dry_run: true,
                yes: false,
                json: true,
                ..
            }))
        ));

        let cli = Cli::try_parse_from([
            "dutis",
            "apply",
            "dutis.toml",
            "--plan-digest",
            "abc123",
            "--yes",
            "--json",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Some(CliCommand::Apply(ApplyArgs {
                dry_run: false,
                yes: true,
                json: true,
                ..
            }))
        ));
    }

    #[test]
    fn parses_apply_safety_options_for_structured_validation() {
        assert!(Cli::try_parse_from(["dutis", "apply", "dutis.toml", "--yes"]).is_ok());
        assert!(
            Cli::try_parse_from(["dutis", "apply", "dutis.toml", "--plan-digest", "abc123"])
                .is_ok()
        );
        assert!(Cli::try_parse_from(["dutis", "apply", "dutis.toml", "--dry-run"]).is_ok());
    }

    #[test]
    fn parses_snapshot_history_and_rollback_commands() {
        let cli = Cli::try_parse_from([
            "dutis",
            "snapshot",
            "create",
            "--config",
            "dutis.toml",
            "--json",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Some(CliCommand::Snapshot(SnapshotArgs {
                command: SnapshotCommand::Create(SnapshotCreateArgs { json: true, .. })
            }))
        ));
        assert!(Cli::try_parse_from(["dutis", "history", "--json"]).is_ok());
        assert!(
            Cli::try_parse_from(["dutis", "rollback", "snapshot-id", "--dry-run", "--json"])
                .is_ok()
        );
    }

    #[test]
    fn parses_read_only_and_write_enabled_mcp_server() {
        let cli = Cli::try_parse_from(["dutis", "mcp"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(CliCommand::Mcp(McpArgs {
                allow_writes: false
            }))
        ));
        let cli = Cli::try_parse_from(["dutis", "mcp", "--allow-writes"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(CliCommand::Mcp(McpArgs { allow_writes: true }))
        ));
    }

    #[test]
    fn parses_policy_and_audit_commands() {
        assert!(Cli::try_parse_from(["dutis", "policy", "show", "--json"]).is_ok());
        assert!(Cli::try_parse_from(["dutis", "policy", "check", "dutis.toml", "--json"]).is_ok());
        assert!(Cli::try_parse_from(["dutis", "audit", "--json"]).is_ok());
    }
}
