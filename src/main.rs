use anyhow::{anyhow, bail, Context, Result};
use colored::*;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;

mod app_scanner;
mod plist_parser;

use app_scanner::{AppScanner, InstalledApplication};
use plist_parser::PlistParser;

#[derive(Debug)]
struct Application {
    installed: InstalledApplication,
    extensions: Vec<String>,
}

fn main() -> Result<()> {
    if std::env::args()
        .nth(1)
        .is_some_and(|arg| arg == "--help" || arg == "-h")
    {
        print_help();
        return Ok(());
    }

    println!("🔍 macOS Application File Extension Manager");
    println!("Scanning system applications...\n");

    let installed_apps = AppScanner::new().scan_applications()?;
    println!(
        "Found {} applications, loading supported file extensions...\n",
        installed_apps.len()
    );

    let parser = PlistParser::new();
    let mut parse_failures = 0;
    let applications = installed_apps
        .into_iter()
        .map(|installed| {
            let plist_path = installed.path.join("Contents/Info.plist");
            let extensions = match parser.parse_extensions(&plist_path) {
                Ok(extensions) => extensions,
                Err(_) => {
                    parse_failures += 1;
                    Vec::new()
                }
            };
            Application {
                installed,
                extensions,
            }
        })
        .collect::<Vec<_>>();

    if parse_failures > 0 {
        eprintln!(
            "⚠️ Could not read metadata for {parse_failures} applications; they remain available in the full application list."
        );
    }

    interactive_query(&applications)
}

fn print_help() {
    println!("Dutis - macOS Application File Extension Manager\n");
    println!("USAGE:\n    dutis [OPTIONS]\n");
    println!("OPTIONS:\n    -h, --help    Print this help message\n");
    println!("DESCRIPTION:");
    println!("    View file extensions supported by installed macOS applications and");
    println!("    set default applications interactively. Setting defaults requires duti.\n");
    println!("EXAMPLES:");
    println!("    dutis                    # Start interactive mode");
    println!("    dutis --help             # Show this help message\n");
    println!("For more information, visit: https://github.com/tsonglew/dutis");
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
                        app.installed.name.bright_blue(),
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
                    app.installed.name.bright_blue(),
                    app.installed.path.display()
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
        bail!("Please enter a valid file extension");
    }
    if !extension
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '_'))
    {
        bail!("File extensions may only contain letters, numbers, '+', '-' or '_'");
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
            app.installed.name.bright_blue(),
            app.extensions.join(", ").yellow()
        );
    }
    println!();
}

fn find_apps_for_extension<'a>(
    applications: &'a [Application],
    extension: &str,
) -> Vec<&'a Application> {
    applications
        .iter()
        .filter(|app| {
            app.extensions
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(extension))
        })
        .collect()
}

fn find_fuzzy_matches<'a>(
    applications: &'a [Application],
    search_term: &str,
) -> Vec<&'a Application> {
    let search_term = search_term.to_ascii_lowercase();
    applications
        .iter()
        .filter(|app| {
            app.installed
                .name
                .to_ascii_lowercase()
                .contains(&search_term)
                || app
                    .extensions
                    .iter()
                    .any(|extension| extension.to_ascii_lowercase().contains(&search_term))
        })
        .collect()
}

fn set_default_and_report(extension: &str, app: &Application) {
    match set_default_app_for_extension(extension, app) {
        Ok(()) => println!(
            "✅ Successfully set {} as the default application for .{} files!",
            app.installed.name.bright_green(),
            extension.yellow()
        ),
        Err(error) => println!("❌ Failed to set default application: {error:#}"),
    }
}

fn set_default_app_for_extension(extension: &str, app: &Application) -> Result<()> {
    ensure_duti_available()?;
    let bundle_id = get_bundle_id(&app.installed.path)?;
    let extension_argument = format!(".{extension}");

    let output = Command::new("duti")
        .args(["-s", &bundle_id, &extension_argument, "all"])
        .output()
        .context("failed to run duti")?;
    if !output.status.success() {
        bail!(
            "duti could not apply the setting: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    verify_default_app(extension, &bundle_id)
}

fn ensure_duti_available() -> Result<()> {
    let output = Command::new("duti").arg("-V").output().map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            anyhow!("duti is required to change defaults; install it with `brew install duti`")
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
    Ok(())
}

fn get_bundle_id(app_path: &Path) -> Result<String> {
    let plist_path = app_path.join("Contents/Info.plist");
    let output = Command::new("/usr/libexec/PlistBuddy")
        .args(["-c", "Print :CFBundleIdentifier"])
        .arg(&plist_path)
        .output()
        .with_context(|| format!("failed to read {}", plist_path.display()))?;

    if !output.status.success() {
        bail!(
            "could not read the bundle identifier for {}: {}",
            app_path.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let bundle_id = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if bundle_id.is_empty() || bundle_id == "(null)" {
        bail!("{} has no bundle identifier", app_path.display());
    }
    Ok(bundle_id)
}

fn verify_default_app(extension: &str, expected_bundle_id: &str) -> Result<()> {
    let output = Command::new("duti")
        .args(["-x", extension])
        .output()
        .context("failed to verify the new default application")?;
    if !output.status.success() {
        bail!(
            "duti applied the setting but verification failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let actual_bundle_id = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next_back()
        .unwrap_or_default()
        .trim()
        .to_owned();
    if actual_bundle_id != expected_bundle_id {
        bail!(
            "verification returned bundle ID '{}' instead of '{}'",
            actual_bundle_id,
            expected_bundle_id
        );
    }
    Ok(())
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
                app.installed.name.bright_blue(),
                app.installed.path.display()
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
            installed: InstalledApplication {
                name: name.to_owned(),
                path: PathBuf::from(format!("/Applications/{name}.app")),
            },
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
        assert_eq!(matches[0].installed.name, "Editor");
    }

    #[test]
    fn fuzzy_searches_names_and_extensions() {
        let applications = vec![app("Text Editor", &["txt"]), app("Viewer", &["pdf"])];
        assert_eq!(find_fuzzy_matches(&applications, "editor").len(), 1);
        assert_eq!(find_fuzzy_matches(&applications, "pd").len(), 1);
    }
}
