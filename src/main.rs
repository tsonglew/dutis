use anyhow::{Context, Result};
use clap::Parser;
use cli::{
    ApplyArgs, Cli, CliCommand, ConfigArgs, ExtensionArgs, OutputArgs, RollbackArgs, SetArgs,
    SnapshotArgs, SnapshotCommand, SnapshotCreateArgs,
};
use colored::*;
use dutis::application::{
    find_apps_for_extension, find_fuzzy_matches, normalize_extension, resolve_app, Application,
    ApplicationCatalog,
};
use dutis::config::DutisConfig;
use dutis::planner::{
    build_plan, ApplyReport, AssociationPlan, PlanAction, PlanEntry, PlanSummary,
};
use dutis::snapshot::{
    apply_plan_with_snapshot, build_rollback_plan, capture_associations, SnapshotReason,
    SnapshotStore, SnapshotSummary,
};
use dutis::system;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeSet;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

mod cli;

const API_VERSION: &str = "1";

#[derive(Debug)]
struct CliError {
    code: u8,
    kind: &'static str,
    message: String,
    details: Option<Value>,
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

    fn stale_plan(message: impl Into<String>, details: Value) -> Self {
        Self::new(7, "stale_plan", message).with_details(details)
    }

    fn partial_failure(message: impl Into<String>, details: Value) -> Self {
        Self::new(8, "partial_failure", message).with_details(details)
    }

    fn new(code: u8, kind: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            kind,
            message: message.into(),
            details: None,
        }
    }

    fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
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
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<&'a Value>,
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

#[derive(Serialize)]
struct DiffResult<'a> {
    plan_digest: &'a str,
    summary: &'a PlanSummary,
    entries: Vec<&'a PlanEntry>,
}

#[derive(Serialize)]
struct SnapshotCreated {
    snapshot: SnapshotSummary,
    path: PathBuf,
}

#[derive(Serialize)]
struct MutationResult {
    safety_snapshot_id: Option<String>,
    #[serde(flatten)]
    report: ApplyReport,
}

#[derive(Serialize)]
struct RollbackPreview<'a> {
    snapshot_id: &'a str,
    plan: &'a AssociationPlan,
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
                        details: error.details.as_ref(),
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
        Some(CliCommand::Plan(args)) => run_plan(args),
        Some(CliCommand::Diff(args)) => run_diff(args),
        Some(CliCommand::Apply(args)) => run_apply(args),
        Some(CliCommand::Snapshot(args)) => run_snapshot(args),
        Some(CliCommand::History(args)) => run_history(args),
        Some(CliCommand::Rollback(args)) => run_rollback(args),
        Some(CliCommand::Doctor(args)) => run_doctor(args),
    }
}

fn command_name(command: &CliCommand) -> &'static str {
    match command {
        CliCommand::List(_) => "list",
        CliCommand::Query(_) => "query",
        CliCommand::Get(_) => "get",
        CliCommand::Set(_) => "set",
        CliCommand::Plan(_) => "plan",
        CliCommand::Diff(_) => "diff",
        CliCommand::Apply(_) => "apply",
        CliCommand::Snapshot(_) => "snapshot",
        CliCommand::History(_) => "history",
        CliCommand::Rollback(_) => "rollback",
        CliCommand::Doctor(_) => "doctor",
    }
}

fn command_uses_json(command: &CliCommand) -> bool {
    match command {
        CliCommand::List(args) | CliCommand::Doctor(args) => args.json,
        CliCommand::Query(args) | CliCommand::Get(args) => args.json,
        CliCommand::Set(args) => args.json,
        CliCommand::Plan(args) | CliCommand::Diff(args) => args.json,
        CliCommand::Apply(args) => args.json,
        CliCommand::Snapshot(args) => match &args.command {
            SnapshotCommand::Create(args) => args.json,
        },
        CliCommand::History(args) => args.json,
        CliCommand::Rollback(args) => args.json,
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

fn run_plan(args: ConfigArgs) -> Result<(), CliError> {
    let plan = build_declarative_plan(&args.config)?;
    if args.json {
        write_json(&JsonEnvelope {
            api_version: API_VERSION,
            command: "plan",
            data: plan,
        })?;
    } else {
        print_plan(&plan, false);
    }
    Ok(())
}

fn run_diff(args: ConfigArgs) -> Result<(), CliError> {
    let plan = build_declarative_plan(&args.config)?;
    if args.json {
        let entries = plan
            .entries
            .iter()
            .filter(|entry| entry.action != PlanAction::Unchanged)
            .collect();
        write_json(&JsonEnvelope {
            api_version: API_VERSION,
            command: "diff",
            data: DiffResult {
                plan_digest: &plan.digest,
                summary: &plan.summary,
                entries,
            },
        })?;
    } else {
        print_plan(&plan, true);
    }
    Ok(())
}

fn run_apply(args: ApplyArgs) -> Result<(), CliError> {
    if !args.dry_run && !args.yes {
        return Err(CliError::usage(
            "refusing to apply configuration without --yes; use --dry-run to preview",
        ));
    }
    if !args.dry_run && args.plan_digest.is_none() {
        return Err(CliError::usage(
            "--plan-digest is required when applying changes",
        ));
    }

    let plan = build_declarative_plan(&args.config)?;
    if args.dry_run {
        if args.json {
            write_json(&JsonEnvelope {
                api_version: API_VERSION,
                command: "apply",
                data: plan,
            })?;
        } else {
            println!("Dry run; no associations will be changed.\n");
            print_plan(&plan, false);
        }
        return Ok(());
    }

    if plan.has_unresolved() {
        let details = serde_json::to_value(&plan)
            .map_err(|error| CliError::operation(format!("failed to serialize plan: {error}")))?;
        if !args.json {
            print_plan(&plan, false);
        }
        return Err(CliError::not_found(format!(
            "plan contains {} unresolved association(s); no changes were made",
            plan.summary.unresolved
        ))
        .with_details(details));
    }

    let reviewed_digest = args
        .plan_digest
        .as_deref()
        .ok_or_else(|| CliError::usage("--plan-digest is required when applying changes"))?;
    if reviewed_digest != plan.digest {
        return Err(CliError::stale_plan(
            "current state no longer matches the reviewed plan; run `dutis plan` again",
            serde_json::json!({
                "reviewed_digest": reviewed_digest,
                "current_digest": plan.digest,
            }),
        ));
    }

    let result = execute_plan_with_snapshot(&plan, SnapshotReason::BeforeApply)?;
    if result.report.failed > 0 {
        let details = serde_json::to_value(&result)
            .map_err(|error| CliError::operation(format!("failed to serialize report: {error}")))?;
        if !args.json {
            print_mutation_result(&result);
        }
        return Err(CliError::partial_failure(
            format!(
                "{} association(s) failed; {} applied and {} skipped",
                result.report.failed, result.report.applied, result.report.skipped
            ),
            details,
        ));
    }

    if args.json {
        write_json(&JsonEnvelope {
            api_version: API_VERSION,
            command: "apply",
            data: result,
        })?;
    } else {
        print_mutation_result(&result);
    }
    Ok(())
}

fn run_snapshot(args: SnapshotArgs) -> Result<(), CliError> {
    match args.command {
        SnapshotCommand::Create(args) => run_snapshot_create(args),
    }
}

fn run_snapshot_create(args: SnapshotCreateArgs) -> Result<(), CliError> {
    let extensions = if let Some(path) = args.config {
        DutisConfig::load(&path)
            .map_err(|error| CliError::usage(format!("{error:#}")))?
            .associations
            .into_keys()
            .collect::<BTreeSet<_>>()
    } else {
        let catalog = scan_catalog()?;
        report_metadata_failures(catalog.metadata_failures);
        catalog
            .applications
            .into_iter()
            .flat_map(|application| application.extensions)
            .collect::<BTreeSet<_>>()
    };

    system::duti_version().map_err(|error| CliError::dependency(format!("{error:#}")))?;
    let associations =
        capture_associations(extensions, system::query_default_app).map_err(|error| {
            CliError::operation(format!("failed to capture associations: {error:#}"))
        })?;
    let store = snapshot_store()?;
    let snapshot = store
        .create(SnapshotReason::Manual, None, associations)
        .map_err(|error| CliError::operation(format!("failed to store snapshot: {error:#}")))?;
    let path = store
        .snapshot_path(&snapshot.id)
        .map_err(|error| CliError::operation(format!("{error:#}")))?;
    let created = SnapshotCreated {
        snapshot: SnapshotSummary::from(&snapshot),
        path,
    };
    if args.json {
        write_json(&JsonEnvelope {
            api_version: API_VERSION,
            command: "snapshot",
            data: created,
        })?;
    } else {
        println!("Snapshot: {}", created.snapshot.id);
        println!("Associations: {}", created.snapshot.associations);
        println!("Stored at: {}", created.path.display());
    }
    Ok(())
}

fn run_history(args: OutputArgs) -> Result<(), CliError> {
    let store = snapshot_store()?;
    let history = store.history().map_err(|error| {
        CliError::operation(format!("failed to read snapshot history: {error:#}"))
    })?;
    if args.json {
        write_json(&JsonEnvelope {
            api_version: API_VERSION,
            command: "history",
            data: history,
        })?;
    } else if history.is_empty() {
        println!("No snapshots found in {}", store.root().display());
    } else {
        for snapshot in history {
            println!(
                "{}\t{}\t{:?}\t{} association(s)",
                snapshot.id, snapshot.created_at, snapshot.reason, snapshot.associations
            );
        }
    }
    Ok(())
}

fn run_rollback(args: RollbackArgs) -> Result<(), CliError> {
    if !args.dry_run && !args.yes {
        return Err(CliError::usage(
            "refusing to roll back without --yes; use --dry-run to preview",
        ));
    }

    let store = snapshot_store()?;
    let snapshot_path = store
        .snapshot_path(&args.snapshot_id)
        .map_err(|error| CliError::usage(format!("{error:#}")))?;
    if !snapshot_path.is_file() {
        return Err(CliError::not_found(format!(
            "snapshot '{}' was not found",
            args.snapshot_id
        )));
    }
    let snapshot = store
        .load(&args.snapshot_id)
        .map_err(|error| CliError::operation(format!("failed to load snapshot: {error:#}")))?;
    let catalog = scan_catalog()?;
    report_metadata_failures(catalog.metadata_failures);
    system::duti_version().map_err(|error| CliError::dependency(format!("{error:#}")))?;
    let plan = build_rollback_plan(&snapshot, &catalog.applications, system::query_default_app)
        .map_err(|error| {
            CliError::operation(format!("failed to build rollback plan: {error:#}"))
        })?;

    if args.dry_run {
        if args.json {
            write_json(&JsonEnvelope {
                api_version: API_VERSION,
                command: "rollback",
                data: RollbackPreview {
                    snapshot_id: &snapshot.id,
                    plan: &plan,
                },
            })?;
        } else {
            println!("Rollback snapshot: {}\n", snapshot.id);
            print_plan(&plan, false);
        }
        return Ok(());
    }

    if plan.has_unresolved() {
        let details = serde_json::to_value(RollbackPreview {
            snapshot_id: &snapshot.id,
            plan: &plan,
        })
        .map_err(|error| CliError::operation(format!("failed to serialize plan: {error}")))?;
        if !args.json {
            print_plan(&plan, false);
        }
        return Err(CliError::not_found(format!(
            "rollback contains {} unresolved association(s); no changes were made",
            plan.summary.unresolved
        ))
        .with_details(details));
    }

    let result = execute_plan_with_snapshot(&plan, SnapshotReason::BeforeRollback)?;
    if result.report.failed > 0 {
        let details = serde_json::to_value(&result)
            .map_err(|error| CliError::operation(format!("failed to serialize report: {error}")))?;
        if !args.json {
            print_mutation_result(&result);
        }
        return Err(CliError::partial_failure(
            format!(
                "rollback failed for {} association(s); safety snapshot retained",
                result.report.failed
            ),
            details,
        ));
    }

    if args.json {
        write_json(&JsonEnvelope {
            api_version: API_VERSION,
            command: "rollback",
            data: result,
        })?;
    } else {
        print_mutation_result(&result);
    }
    Ok(())
}

fn execute_plan_with_snapshot(
    plan: &AssociationPlan,
    reason: SnapshotReason,
) -> Result<MutationResult, CliError> {
    let store = snapshot_store()?;
    let protected = apply_plan_with_snapshot(&store, plan, reason, system::set_default_app)
        .map_err(|error| {
            CliError::operation(format!(
                "failed to store safety snapshot; no changes were made: {error:#}"
            ))
        })?;
    Ok(MutationResult {
        safety_snapshot_id: protected.safety_snapshot.map(|snapshot| snapshot.id),
        report: protected.report,
    })
}

fn snapshot_store() -> Result<SnapshotStore, CliError> {
    SnapshotStore::from_environment().map_err(|error| {
        CliError::operation(format!("failed to resolve snapshot storage: {error:#}"))
    })
}

fn build_declarative_plan(path: &Path) -> Result<AssociationPlan, CliError> {
    let config = DutisConfig::load(path).map_err(|error| CliError::usage(format!("{error:#}")))?;
    let catalog = scan_catalog()?;
    report_metadata_failures(catalog.metadata_failures);
    system::duti_version().map_err(|error| CliError::dependency(format!("{error:#}")))?;
    build_plan(&config, &catalog.applications, system::query_default_app)
        .map_err(|error| CliError::operation(format!("failed to inspect current state: {error:#}")))
}

fn print_plan(plan: &AssociationPlan, changes_only: bool) {
    for entry in &plan.entries {
        if changes_only && entry.action == PlanAction::Unchanged {
            continue;
        }
        match entry.action {
            PlanAction::Change => {
                let current = entry
                    .current
                    .as_ref()
                    .map(|app| app.bundle_id.as_str())
                    .unwrap_or("<none>");
                let target = entry
                    .target
                    .as_ref()
                    .map(|app| app.bundle_id.as_str())
                    .unwrap_or("<unresolved>");
                println!("CHANGE    .{}: {} -> {}", entry.extension, current, target);
            }
            PlanAction::Unchanged => {
                let bundle_id = entry
                    .target
                    .as_ref()
                    .map(|app| app.bundle_id.as_str())
                    .unwrap_or("<unknown>");
                println!("UNCHANGED .{}: {}", entry.extension, bundle_id);
            }
            PlanAction::Unresolved => println!(
                "UNRESOLVED .{}: {}",
                entry.extension,
                entry.reason.as_deref().unwrap_or("unknown reason")
            ),
        }
    }
    println!(
        "\nSummary: {} change(s), {} unchanged, {} unresolved",
        plan.summary.changes, plan.summary.unchanged, plan.summary.unresolved
    );
    println!("Plan digest: {}", plan.digest);
}

fn print_mutation_result(result: &MutationResult) {
    if let Some(snapshot_id) = &result.safety_snapshot_id {
        println!("Safety snapshot: {snapshot_id}\n");
    }
    for entry in &result.report.results {
        println!(
            "{:?} .{}{}",
            entry.status,
            entry.extension,
            entry
                .error
                .as_ref()
                .map(|error| format!(": {error}"))
                .unwrap_or_default()
        );
    }
    println!(
        "\nApplied: {}, skipped: {}, failed: {}",
        result.report.applied, result.report.skipped, result.report.failed
    );
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
