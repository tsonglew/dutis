use clap::Parser;
use dutis::http_adapter::HttpAdapterConfig;
use serde_json::json;
use std::process::ExitCode;

#[derive(Debug, Parser)]
#[command(
    name = "dutis-event-http",
    version,
    about = "Forward Dutis events to an HTTPS endpoint"
)]
struct Cli {
    /// Validate environment configuration without sending an event
    #[arg(long)]
    check: bool,
    /// Emit a machine-readable result; only valid with --check
    #[arg(long, requires = "check")]
    json: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("dutis-event-http: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> anyhow::Result<()> {
    let config = HttpAdapterConfig::from_environment()?;
    if cli.check {
        let status = config.status();
        if cli.json {
            println!(
                "{}",
                serde_json::to_string(&json!({
                    "api_version": "1",
                    "command": "check",
                    "data": status,
                }))?
            );
        } else {
            println!("HTTP event adapter configuration is valid.");
            println!("Transport: {}", status.transport);
            println!("Authentication: {}", status.authentication);
            println!("Timeout: {} seconds", status.timeout_seconds);
            println!("Retries: {}", status.retries);
        }
        return Ok(());
    }
    config.deliver(std::io::stdin().lock())
}
