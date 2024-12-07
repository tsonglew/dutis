pub mod config;
pub mod groups;
pub mod uti;
pub mod utils;

pub use config::Config;
pub use groups::*;
pub use uti::*;
pub use utils::*;

use colored::Colorize;
use dialoguer::{theme::ColorfulTheme, Select};
use std::error::Error;

pub fn run_interactive(config: &Config) -> Result<(), Box<dyn Error>> {
    match crate::uti::get_role_handlers_from_uti(&config.uti) {
        Some(handlers) => {
            if handlers.is_empty() {
                eprintln!(
                    "{} {}",
                    "⚠️".yellow(),
                    format!("No handlers found for content type '{}'", config.uti).yellow()
                );
                return Ok(());
            }

            println!(
                "{} {}",
                "🔍".blue(),
                format!("Select a handler for content type '{}':", config.uti).blue()
            );

            let selection = Select::with_theme(&ColorfulTheme::default())
                .items(&handlers)
                .default(0)
                .interact()?;

            let selected_handler = &handlers[selection];
            println!(
                "\n{} {}",
                "✨".cyan(),
                format!("Selected handler: {}", selected_handler).cyan()
            );

            if let Err(e) = crate::uti::set_default_app_for_suffix(config, selected_handler) {
                eprintln!(
                    "{} {}",
                    "❌".red(),
                    format!("Failed to set default handler: {}", e).red()
                )
            }
            Ok(())
        }
        None => {
            eprintln!(
                "{} {}",
                "❌".red(),
                format!("No handlers found for content type '{}'", config).red()
            );
            Ok(())
        }
    }
}

pub fn run_interactive_group(uti2suf: BiMap<String, String>) -> Result<(), Box<dyn Error>> {
    if uti2suf.is_empty() {
        return Ok(());
    }

    match crate::uti::get_common_role_handlers(&uti2suf) {
        Some(handlers) => {
            if handlers.is_empty() {
                eprintln!(
                    "{} {}",
                    "⚠️".yellow(),
                    "No handlers found for the specified content types.".yellow()
                );
                return Ok(());
            }

            let selection = Select::with_theme(&ColorfulTheme::default())
                .with_prompt("🔍 Select a handler:")
                .items(&handlers)
                .default(0)
                .interact()?;

            let handler = &handlers[selection];
            for i in uti2suf.iter() {
                if let Err(e) = crate::uti::set_default_app_for_suffix(
                    &Config::new(i.0.clone(), i.1.clone()),
                    handler,
                ) {
                    eprintln!("Error setting handler for {}: {}", i.0, e);
                } else {
                    println!(
                        "{} {}",
                        "✅".green(),
                        format!(
                            "Successfully set '{}' as the default handler for '.{}' files",
                            handler, i.1
                        )
                        .green()
                    );
                }
            }
        }
        None => {
            eprintln!(
                "{} {}",
                "❌".red(),
                "No handlers found for the specified content types.".red()
            );
        }
    }

    Ok(())
}
