use anyhow::{bail, Context, Result};
use application::{
    find_apps_for_extension, find_fuzzy_matches, resolve_app, Application, ApplicationCatalog,
};
use clap::Parser;
use cli::{Cli, CliCommand, ExtensionArgs, OutputArgs, SetArgs};
use colored::*;
use serde::Serialize;
use std::io::{self, Write};
use std::process::ExitCode;

mod app_scanner;
mod application;
mod cli;
mod plist_parser;
mod system;

const API_VERSION: &str = "1";

#[derive(Debug)]
struct CliError {
    code: u8,
    kind: &'static str,
    message: String,
}

impl CliError {
    fn usage(message: impl Into<String>) -> Self {
        Self::new(2, "usage", message)
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self::new(3, "not_found", message)
    }

    fn ambiguous(message: impl Into<String>) -> Self {
        Self::new(4, "ambiguous_selector", message)
    }

    fn dependency(message: impl Into<String>) -> Self {
        Self::new(5, "dependency_unavailable", message)
    }

    fn operation(message: impl Into<String>) -> Self {
        Self::new(6, "operation_failed", message)
    }

    fn new(code: u8, kind: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            kind,
            message: message.into(),
        }
    }
}

#[derive(Serialize)]
struct JsonEnvelope<T> {
    api_version: &'static str,
    command: &'static str,
    data: T,
}

#[derive(Serialize)]
struct JsonErrorEnvelope<'a> {
    api_version: &'static str,
    command: &'static str,
    error: JsonError<'a>,
}

#[derive(Serialize)]
struct JsonError<'a> {
    code: u8,
    kind: &'static str,
    message: &'a str,
}

#[derive(Serialize)]
struct ApplicationList<'a> {
    applications: &'a [Application],
    metadata_failures: usize,
}

#[derive(Serialize)]
struct QueryResult<'a> {
    extension: &'a str,
    applications: Vec<&'a Application>,
    metadata_failures: usize,
}

#[derive(Serialize)]
struct SetResult<'a> {
    status: &'static str,
    extension: &'a str,
    application: &'a Application,
    command: Vec<String>,
}

#[derive(Serialize)]
struct DoctorResult {
    platform: &'static str,
    duti_available: bool,
    duti_version: Option<String>,
    ready_for_read_only_commands: bool,
    ready_for_changes: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let command_name = cli
        .command
        .as_ref()
        .map(command_name)
        .unwrap_or("interactive");
    let json = cli.command.as_ref().is_some_and(command_uses_json);

    match dispatch(cli.command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if json {
                let response = JsonErrorEnvelope {
                    api_version: API_VERSION,
                    command: command_name,
                    error: JsonError {
                        code: error.code,
                        kind: error.kind,
                        message: &error.message,
                    },
                };
                if let Err(serialization_error) = write_json(&response) {
                    eprintln!("failed to serialize error response: {serialization_error:?}");
                }
            } else {
                eprintln!("Error: {}", error.message);
            }
            ExitCode::from(error.code)
        }
    }
}

fn dispatch(command: Option<CliCommand>) -> Result<(), CliError> {
    match command {
        None => run_interactive().map_err(|error| CliError::operation(format!("{error:#}"))),
        Some(CliCommand::List(args)) => run_list(args),
        Some(CliCommand::Query(args)) => run_query(args),
        Some(CliCommand::Get(args)) => run_get(args),
        Some(CliCommand::Set(args)) => run_set(args),
        Some(CliCommand::Doctor(args)) => run_doctor(args),
    }
}

fn command_name(command: &CliCommand) -> &'static str {
    match command {
        CliCommand::List(_) => "list",
        CliCommand::Query(_) => "query",
        CliCommand::Get(_) => "get",
        CliCommand::Set(_) => "set",
        CliCommand::Doctor(_) => "doctor",
    }
}

fn command_uses_json(command: &CliCommand) -> bool {
    match command {
        CliCommand::List(args) | CliCommand::Doctor(args) => args.json,
        CliCommand::Query(args) | CliCommand::Get(args) => args.json,
        CliCommand::Set(args) => args.json,
    }
}

fn run_list(args: OutputArgs) -> Result<(), CliError> {
    let catalog = scan_catalog()?;
    report_metadata_failures(catalog.metadata_failures);
    if args.json {
        write_json(&JsonEnvelope {
            api_version: API_VERSION,
            command: "list",
            data: ApplicationList {
                applications: &catalog.applications,
                metadata_failures: catalog.metadata_failures,
            },
        })?;
    } else {
        for app in &catalog.applications {
            let bundle_id = app.bundle_id.as_deref().unwrap_or("unknown bundle ID");
            println!("{}\t{}\t{}", app.name, bundle_id, app.path.display());
        }
        println!("\n{} applications", catalog.applications.len());
    }
    Ok(())
}

fn run_query(args: ExtensionArgs) -> Result<(), CliError> {
    let extension =
        normalize_extension(&args.extension).map_err(|error| CliError::usage(error.to_string()))?;
    let catalog = scan_catalog()?;
    report_metadata_failures(catalog.metadata_failures);
    let applications = find_apps_for_extension(&catalog.applications, &extension);
    if applications.is_empty() {
        return Err(CliError::not_found(format!(
            "no installed applications declare support for .{extension}"
        )));
    }

    if args.json {
        write_json(&JsonEnvelope {
            api_version: API_VERSION,
            command: "query",
            data: QueryResult {
                extension: &extension,
                applications,
                metadata_failures: catalog.metadata_failures,
            },
        })?;
    } else {
        println!("Applications supporting .{extension}:");
        for app in applications {
            println!("{}\t{}", app.name, app.path.display());
        }
    }
    Ok(())
}

fn run_get(args: ExtensionArgs) -> Result<(), CliError> {
    let extension =
        normalize_extension(&args.extension).map_err(|error| CliError::usage(error.to_string()))?;
    system::duti_version().map_err(|error| CliError::dependency(format!("{error:#}")))?;
    let default = system::get_default_app(&extension)
        .map_err(|error| CliError::operation(format!("{error:#}")))?
        .ok_or_else(|| {
            CliError::not_found(format!(
                "no default application is registered for .{extension}"
            ))
        })?;

    if args.json {
        write_json(&JsonEnvelope {
            api_version: API_VERSION,
            command: "get",
            data: default,
        })?;
    } else {
        println!("Default application for .{}:", extension);
        if let Some(name) = default.name {
            println!("Name: {name}");
        }
        if let Some(path) = default.path {
            println!("Path: {path}");
        }
        println!("Bundle ID: {}", default.bundle_id);
    }
    Ok(())
}

fn run_set(args: SetArgs) -> Result<(), CliError> {
    let extension =
        normalize_extension(&args.extension).map_err(|error| CliError::usage(error.to_string()))?;
    if !args.dry_run && !args.yes {
        return Err(CliError::usage(
            "refusing to change the system without --yes; use --dry-run to preview",
        ));
    }

    let catalog = scan_catalog()?;
    report_metadata_failures(catalog.metadata_failures);
    let matches = resolve_app(&catalog.applications, &args.app_selector);
    let app = match matches.as_slice() {
        [] => {
            return Err(CliError::not_found(format!(
                "no installed application matches '{}'",
                args.app_selector
            )))
        }
        [app] => *app,
        matches => {
            let paths = matches
                .iter()
                .map(|app| app.path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(CliError::ambiguous(format!(
                "application name '{}' is ambiguous; use a bundle ID or exact path ({paths})",
                args.app_selector
            )));
        }
    };
    let bundle_id = app.bundle_id.as_deref().ok_or_else(|| {
        CliError::operation(format!(
            "{} has no readable bundle identifier",
            app.path.display()
        ))
    })?;
    let command = vec![
        "duti".to_owned(),
        "-s".to_owned(),
        bundle_id.to_owned(),
        format!(".{extension}"),
        "all".to_owned(),
    ];

    let status = apply_or_preview(
        args.dry_run,
        &extension,
        bundle_id,
        |extension, bundle_id| {
            system::duti_version().map_err(|error| CliError::dependency(format!("{error:#}")))?;
            system::set_default_app(extension, bundle_id)
                .map_err(|error| CliError::operation(format!("{error:#}")))
        },
    )?;

    if args.json {
        write_json(&JsonEnvelope {
            api_version: API_VERSION,
            command: "set",
            data: SetResult {
                status,
                extension: &extension,
                application: app,
                command,
            },
        })?;
    } else if args.dry_run {
        println!(
            "Dry run: would set .{extension} to {} ({bundle_id})",
            app.name
        );
        println!("Command: {}", shell_display(&command));
    } else {
        println!(
            "Set .{extension} to {} ({bundle_id}) and verified it",
            app.name
        );
    }
    Ok(())
}

fn apply_or_preview<F>(
    dry_run: bool,
    extension: &str,
    bundle_id: &str,
    apply: F,
) -> Result<&'static str, CliError>
where
    F: FnOnce(&str, &str) -> Result<(), CliError>,
{
    if dry_run {
        return Ok("planned");
    }
    apply(extension, bundle_id)?;
    Ok("applied")
}

fn run_doctor(args: OutputArgs) -> Result<(), CliError> {
    let duti = system::duti_version();
    let duti_available = duti.is_ok();
    let result = DoctorResult {
        platform: std::env::consts::OS,
        duti_available,
        duti_version: duti.ok(),
        ready_for_read_only_commands: cfg!(target_os = "macos"),
        ready_for_changes: cfg!(target_os = "macos") && duti_available,
    };
    if args.json {
        write_json(&JsonEnvelope {
            api_version: API_VERSION,
            command: "doctor",
            data: result,
        })?;
    } else {
        println!("Platform: {}", result.platform);
        println!(
            "duti: {}",
            result.duti_version.as_deref().unwrap_or("not available")
        );
        println!(
            "Read-only commands ready: {}",
            result.ready_for_read_only_commands
        );
        println!("Changes ready: {}", result.ready_for_changes);
    }
    Ok(())
}

fn scan_catalog() -> Result<ApplicationCatalog, CliError> {
    ApplicationCatalog::scan().map_err(|error| CliError::operation(format!("{error:#}")))
}

fn report_metadata_failures(count: usize) {
    if count > 0 {
        eprintln!(
            "Warning: could not read metadata for {count} applications; they remain in the application list"
        );
    }
}

fn write_json<T: Serialize>(value: &T) -> Result<(), CliError> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|error| CliError::operation(format!("failed to serialize JSON: {error}")))?;
    println!("{json}");
    Ok(())
}

fn shell_display(arguments: &[String]) -> String {
    arguments
        .iter()
        .map(|argument| {
            if argument
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "./_+-".contains(character))
            {
                argument.clone()
            } else {
                format!("'{argument}'")
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn run_interactive() -> Result<()> {
    println!("🔍 macOS Application File Extension Manager");
    println!("Scanning system applications...\n");

    let catalog = ApplicationCatalog::scan()?;
    println!(
        "Found {} applications, loading supported file extensions...\n",
        catalog.applications.len()
    );
    if catalog.metadata_failures > 0 {
        eprintln!(
            "⚠️ Could not read metadata for {} applications; they remain available in the full application list.",
            catalog.metadata_failures
        );
    }
    interactive_query(&catalog.applications)
}

fn interactive_query(applications: &[Application]) -> Result<()> {
    println!("\n🎯 Interactive Query Mode");
    println!("Enter a file extension (for example: py, js, txt)");
    println!("Enter 'quit' or 'exit' to exit the program");
    println!("Enter 'debug' to show scan information\n");

    loop {
        let Some(input) = read_prompt("Please enter file extension: ")? else {
            println!("\n👋 Goodbye!");
            break;
        };
        let input = input.trim();

        match input.to_ascii_lowercase().as_str() {
            "quit" | "exit" | "q" => {
                println!("👋 Goodbye!");
                break;
            }
            "debug" => {
                display_debug_info(applications);
                continue;
            }
            "" => {
                println!("❌ Please enter a valid file extension");
                continue;
            }
            _ => {}
        }

        let extension = match normalize_extension(input) {
            Ok(extension) => extension,
            Err(error) => {
                println!("❌ {error}");
                continue;
            }
        };
        let display_extension = format!(".{extension}");
        println!(
            "🔍 Searching for applications that support {} files...",
            display_extension.yellow()
        );

        let supporting_apps = find_apps_for_extension(applications, &extension);
        if supporting_apps.is_empty() {
            println!(
                "❌ No applications found that explicitly declare support for {} files",
                display_extension.yellow()
            );

            let fuzzy_matches = find_fuzzy_matches(applications, &extension);
            if !fuzzy_matches.is_empty() {
                println!("🔍 Possible matches:");
                for app in fuzzy_matches.iter().take(5) {
                    println!(
                        "   • {}: {}",
                        app.name.bright_blue(),
                        app.extensions.join(", ").yellow()
                    );
                }
            }

            let Some(choice) = read_prompt(
                "Enter 'all' to browse all applications, or press Enter to continue: ",
            )?
            else {
                break;
            };
            if choice.trim().eq_ignore_ascii_case("all") {
                show_all_apps_menu(&extension, applications)?;
            }
        } else {
            println!(
                "✅ Found {} applications that support {} files:",
                supporting_apps.len(),
                display_extension.yellow()
            );
            for (index, app) in supporting_apps.iter().enumerate() {
                println!(
                    "   {}. {} ({})",
                    index + 1,
                    app.name.bright_blue(),
                    app.path.display()
                );
            }

            println!("\nEnter an application number to set it as default");
            println!("Enter 'all' to browse every application, or press Enter to skip");
            let Some(choice) = read_prompt("Your choice: ")? else {
                break;
            };
            let choice = choice.trim();

            if choice.eq_ignore_ascii_case("all") {
                show_all_apps_menu(&extension, applications)?;
            } else if !choice.is_empty() {
                match choice.parse::<usize>() {
                    Ok(index) if (1..=supporting_apps.len()).contains(&index) => {
                        set_default_and_report(&extension, supporting_apps[index - 1]);
                    }
                    _ => println!(
                        "❌ Invalid choice; enter a number between 1 and {}",
                        supporting_apps.len()
                    ),
                }
            }
        }
        println!();
    }

    Ok(())
}

fn read_prompt(prompt: &str) -> Result<Option<String>> {
    print!("{prompt}");
    io::stdout().flush().context("failed to write prompt")?;

    let mut input = String::new();
    let bytes_read = io::stdin()
        .read_line(&mut input)
        .context("failed to read input")?;
    Ok((bytes_read != 0).then_some(input))
}

fn normalize_extension(input: &str) -> Result<String> {
    let extension = input.trim().trim_start_matches('.').to_ascii_lowercase();
    if extension.is_empty() {
        bail!("please enter a valid file extension");
    }
    if !extension
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '_'))
    {
        bail!("file extensions may only contain letters, numbers, '+', '-' or '_'");
    }
    Ok(extension)
}

fn display_debug_info(applications: &[Application]) {
    let with_extensions = applications
        .iter()
        .filter(|app| !app.extensions.is_empty())
        .count();
    println!("\n🔍 Debug Information:");
    println!("Applications scanned: {}", applications.len());
    println!("Applications declaring extensions: {with_extensions}");
    for app in applications
        .iter()
        .filter(|app| !app.extensions.is_empty())
        .take(10)
    {
        println!(
            "  {}: {}",
            app.name.bright_blue(),
            app.extensions.join(", ").yellow()
        );
    }
    println!();
}

fn set_default_and_report(extension: &str, app: &Application) {
    let result = app
        .bundle_id
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("{} has no readable bundle identifier", app.path.display()))
        .and_then(|bundle_id| system::set_default_app(extension, bundle_id));
    match result {
        Ok(()) => println!(
            "✅ Successfully set {} as the default application for .{} files!",
            app.name.bright_green(),
            extension.yellow()
        ),
        Err(error) => println!("❌ Failed to set default application: {error:#}"),
    }
}

fn show_all_apps_menu(extension: &str, applications: &[Application]) -> Result<()> {
    const PAGE_SIZE: usize = 20;
    if applications.is_empty() {
        println!("❌ No applications were found");
        return Ok(());
    }

    let mut page = 0;
    let total_pages = applications.len().div_ceil(PAGE_SIZE);
    loop {
        println!("\n📋 All Applications - Page {}/{}", page + 1, total_pages);
        println!("Setting default for .{} files\n", extension.yellow());

        let start = page * PAGE_SIZE;
        let end = usize::min(start + PAGE_SIZE, applications.len());
        for (index, app) in applications[start..end].iter().enumerate() {
            println!(
                "   {}. {} ({})",
                start + index + 1,
                app.name.bright_blue(),
                app.path.display()
            );
        }

        println!("\nOptions:");
        println!("   • Enter a number (1-{})", applications.len());
        if page > 0 {
            println!("   • 'p' or 'prev' for previous page");
        }
        if page + 1 < total_pages {
            println!("   • 'n' or 'next' for next page");
        }
        println!("   • 'q' to return to the main menu");

        let Some(choice) = read_prompt("Your choice: ")? else {
            break;
        };
        let choice = choice.trim().to_ascii_lowercase();
        match choice.as_str() {
            "q" => break,
            "n" | "next" if page + 1 < total_pages => page += 1,
            "p" | "prev" if page > 0 => page -= 1,
            "n" | "next" => println!("❌ Already on the last page"),
            "p" | "prev" => println!("❌ Already on the first page"),
            _ => match choice.parse::<usize>() {
                Ok(index) if (1..=applications.len()).contains(&index) => {
                    set_default_and_report(extension, &applications[index - 1]);
                    break;
                }
                _ => println!(
                    "❌ Invalid choice; enter a number between 1 and {}",
                    applications.len()
                ),
            },
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn app(name: &str, extensions: &[&str]) -> Application {
        Application {
            name: name.to_owned(),
            path: PathBuf::from(format!("/Applications/{name}.app")),
            bundle_id: Some(format!("example.{name}")),
            extensions: extensions.iter().map(|value| (*value).to_owned()).collect(),
        }
    }

    #[test]
    fn normalizes_extensions() {
        assert_eq!(normalize_extension(" .TXT ").unwrap(), "txt");
        assert_eq!(normalize_extension("c++").unwrap(), "c++");
        assert!(normalize_extension("../../txt").is_err());
        assert!(normalize_extension("...").is_err());
    }

    #[test]
    fn searches_extensions_case_insensitively() {
        let applications = vec![app("Editor", &["txt", "MD"]), app("Viewer", &["pdf"])];
        let matches = find_apps_for_extension(&applications, "md");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "Editor");
    }

    #[test]
    fn fuzzy_searches_names_and_extensions() {
        let applications = vec![app("Text Editor", &["txt"]), app("Viewer", &["pdf"])];
        assert_eq!(find_fuzzy_matches(&applications, "editor").len(), 1);
        assert_eq!(find_fuzzy_matches(&applications, "pd").len(), 1);
    }

    #[test]
    fn json_envelope_includes_api_version() {
        let value = JsonEnvelope {
            api_version: API_VERSION,
            command: "test",
            data: vec!["ok"],
        };
        let json = serde_json::to_value(value).unwrap();
        assert_eq!(json["api_version"], "1");
        assert_eq!(json["command"], "test");
        assert_eq!(json["data"][0], "ok");
    }

    #[test]
    fn dry_run_never_calls_mutating_operation() {
        let status = apply_or_preview(true, "md", "com.example.Editor", |_, _| {
            panic!("dry-run attempted a mutation")
        })
        .unwrap();
        assert_eq!(status, "planned");
    }
}
