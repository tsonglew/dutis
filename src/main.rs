use colored::Colorize;
use dutis::{
    groups, run_interactive, run_interactive_group,
    uti::{get_common_suffix, get_friendly_name, get_uti_from_suffix},
    utils::BiMap,
    Config,
};
use std::env;
use std::process;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn check_os() {
    if !cfg!(target_os = "macos") {
        eprintln!(
            "{} {}",
            "⚠️ Warning:".yellow(),
            "Dutis is designed for macOS and may not work correctly on other operating systems.".yellow()
        );
        process::exit(1);
    }
}

fn main() {
    check_os();
    
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage();
        process::exit(1);
    }

    match args[1].as_str() {
        "--version" => {
            println!("dutis {}", VERSION);
            process::exit(0);
        }
        "--generate-shell-completion" => {
            // TODO: Add shell completion generation
            process::exit(0);
        }
        "--group" => {
            if args.len() < 3 {
                eprintln!(
                    "{} {}",
                    "❌".red(),
                    "Please specify a group name after --group".red()
                );
                print_available_groups();
                process::exit(1);
            }
            handle_group_mode(&args[2]);
        }
        suffix => handle_single_suffix(suffix),
    }
}

fn print_usage() {
    eprintln!(
        "{} {}",
        "❌".red(),
        "Usage: dutis <file-suffix> OR dutis --group <group-name>".red()
    );
    eprintln!("{} {}", "💡".yellow(), "Example: dutis html".yellow());
    eprintln!(
        "{} {}",
        "💡".yellow(),
        "Example: dutis --group video".yellow()
    );
}

fn print_available_groups() {
    eprintln!("{} {}", "ℹ️".blue(), "Available groups:".blue());
    for group in groups::list_available_groups() {
        eprintln!("   • {}", group.cyan());
    }
}

fn handle_group_mode(group_name: &str) {
    match groups::get_suffix_group(group_name) {
        Some(suffixes) => {
            println!(
                "{} {}",
                "🎯".cyan(),
                format!(
                    "Processing group '{}' with {} suffixes",
                    group_name,
                    suffixes.len()
                )
                .cyan()
            );

            // Collect UTIs for all suffixes
            let mut uti2suf: BiMap<String, String> = BiMap::new();
            for suffix in &suffixes {
                match get_uti_from_suffix(suffix) {
                    Some(uti) => {
                        uti2suf.insert(uti.clone(), suffix.to_string());
                        println!(
                            "{} {}",
                            "📝".blue(),
                            format!("Found UTI '{}' for '.{}'", uti, suffix).blue()
                        );
                    }
                    None => {
                        eprintln!(
                            "{} {}",
                            "⚠️".yellow(),
                            format!("Warning: No UTI found for suffix '{}', skipping.", suffix)
                                .yellow()
                        );
                    }
                }
            }

            if uti2suf.is_empty() {
                eprintln!(
                    "{} {}",
                    "❌".red(),
                    "No valid UTIs found for any suffix in the group".red()
                );
                process::exit(1);
            }

            if let Err(e) = run_interactive_group(uti2suf) {
                eprintln!(
                    "{} {}",
                    "❌".red(),
                    format!("Application error: {}", e).red()
                );
                process::exit(1);
            }
        }
        None => {
            eprintln!(
                "{} {}",
                "❌".red(),
                format!("Group '{}' not found", group_name).red()
            );
            print_available_groups();
            process::exit(1);
        }
    }
}

fn handle_single_suffix(suffix: &str) {
    let uti = match get_uti_from_suffix(suffix) {
        Some(uti) => {
            println!(
                "{} {}",
                "📝".blue(),
                format!(
                    "Found UTI '{}' ({}) [{}] for '{}'",
                    uti,
                    get_friendly_name(&uti),
                    get_common_suffix(&uti, suffix),
                    suffix
                )
                .blue()
            );
            uti.to_string()
        }
        None => {
            eprintln!(
                "{} {}",
                "❌".red(),
                format!("Error: No UTI found for suffix '{}'", suffix).red()
            );
            eprintln!(
                "{} {}",
                "💡".yellow(),
                "The suffix might not be recognized by the system".yellow()
            );
            process::exit(1);
        }
    };

    let config = Config::new(uti, suffix.to_string());

    if let Err(e) = run_interactive(&config) {
        eprintln!(
            "{} {}",
            "❌".red(),
            format!("Application error: {}", e).red()
        );
        process::exit(1);
    }
}
