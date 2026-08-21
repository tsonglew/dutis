use clap::{Args, Parser, Subcommand};

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
    }
}
