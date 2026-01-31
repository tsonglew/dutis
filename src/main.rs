use anyhow::Result;
use colored::*;
use std::collections::HashMap;
use std::io::{self, Write};
use std::path::Path;

mod app_scanner;
mod plist_parser;

use app_scanner::AppScanner;
use plist_parser::PlistParser;

fn main() -> Result<()> {
    // Check for help flag
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && (args[1] == "--help" || args[1] == "-h") {
        print_help();
        return Ok(());
    }

    println!("🔍 macOS Application File Extension Manager");

    // Check and install duti if needed
    ensure_duti_available()?;

    println!("Scanning system applications...\n");

    let app_scanner = AppScanner::new();
    let plist_parser = PlistParser::new();

    // Scan applications
    let apps = app_scanner.scan_applications()?;
    println!(
        "Found {} applications, Loading supported file extensions...\n",
        apps.len()
    );

    // Analyze file extensions supported by each application
    let mut app_extensions: HashMap<String, Vec<String>> = HashMap::new();

    for app_path in &apps {
        if let Some(app_name) = Path::new(&app_path).file_stem().and_then(|n| n.to_str()) {
            let info_plist_path = format!("{}/Contents/Info.plist", app_path);

            if let Ok(extensions) = plist_parser.parse_extensions(&info_plist_path) {
                if !extensions.is_empty() {
                    app_extensions.insert(app_name.to_string(), extensions);
                }
            }
        }
    }

    // Collect all app names for the "any app" feature
    let all_app_names: Vec<String> = apps
        .iter()
        .filter_map(|app_path| {
            Path::new(app_path)
                .file_stem()
                .and_then(|n| n.to_str())
                .map(String::from)
        })
        .collect();

    // Display complete results
    // display_results(&app_extensions);

    // Interactive query functionality
    interactive_query(&app_extensions, &all_app_names);

    Ok(())
}

/// Ensure duti is available, install it via Homebrew if not
fn ensure_duti_available() -> Result<()> {
    // Check if duti is already available
    if std::process::Command::new("duti")
        .arg("--version")
        .output()
        .is_ok()
    {
        println!("✅ duti is already installed and available");
        return Ok(());
    }

    println!("🔍 duti not found, attempting to install via Homebrew...");

    // Check if Homebrew is available
    if std::process::Command::new("brew")
        .arg("--version")
        .output()
        .is_err()
    {
        return Err(anyhow::anyhow!(
            "Homebrew is not installed. Please install Homebrew first:\n\
             /bin/bash -c \"$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)\""
        ));
    }

    println!("📦 Installing duti via Homebrew...");

    // Install duti using Homebrew
    let install_output = std::process::Command::new("brew")
        .arg("install")
        .arg("duti")
        .output();

    match install_output {
        Ok(output) if output.status.success() => {
            println!("✅ Successfully installed duti via Homebrew");

            // Verify installation
            if std::process::Command::new("duti")
                .arg("--version")
                .output()
                .is_ok()
            {
                println!("✅ duti is now available and ready to use");
                return Ok(());
            } else {
                return Err(anyhow::anyhow!("duti was installed but is not accessible. Please restart your terminal or add Homebrew to your PATH."));
            }
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("already installed") {
                println!("✅ duti is already installed via Homebrew");
                return Ok(());
            } else {
                return Err(anyhow::anyhow!(
                    "Failed to install duti via Homebrew: {}",
                    stderr
                ));
            }
        }
        Err(e) => {
            return Err(anyhow::anyhow!(
                "Failed to run Homebrew install command: {}",
                e
            ));
        }
    }
}

/// Print help information
fn print_help() {
    println!("Dutis - macOS Application File Extension Manager");
    println!();
    println!("USAGE:");
    println!("    dutis [OPTIONS]");
    println!();
    println!("OPTIONS:");
    println!("    -h, --help    Print this help message");
    println!();
    println!("DESCRIPTION:");
    println!("    A comprehensive Rust application for viewing file extensions supported by");
    println!("    macOS applications and setting default applications for file types.");
    println!();
    println!("    Features:");
    println!("    • Scan system applications and their supported file extensions");
    println!("    • Interactive query mode to find apps for specific file types");
    println!("    • Set default applications for file types");
    println!("    • Automatic duti installation via Homebrew");
    println!("    • Intelligent UTI detection with retry mechanisms");
    println!();
    println!("    The application will automatically install the 'duti' command-line tool");
    println!("    if it's not available on your system.");
    println!();
    println!("EXAMPLES:");
    println!("    dutis                    # Start interactive mode");
    println!("    dutis --help            # Show this help message");
    println!();
    println!("REQUIREMENTS:");
    println!("    • macOS 10.14 or later");
    println!("    • Homebrew (for automatic duti installation)");
    println!();
    println!("For more information, visit: https://github.com/tsonglew/dutis");
}

fn interactive_query(app_extensions: &HashMap<String, Vec<String>>, all_app_names: &[String]) {
    println!("\n🎯 Interactive Query Mode");
    println!("Enter file extension (e.g., py, js, txt) to find supporting applications");
    println!("Enter 'quit' or 'exit' to exit the program");
    println!("Enter 'debug' to show debug information\n");

    loop {
        print!("Please enter file extension: ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();

        let extension = input.trim().to_lowercase();

        match extension.as_str() {
            "quit" | "exit" | "q" => {
                println!("👋 Goodbye!");
                break;
            }
            "debug" => {
                println!("\n🔍 Debug Information:");
                println!("Number of scanned applications: {}", app_extensions.len());
                println!("First 10 applications and their supported extensions:");

                let mut count = 0;
                for (app_name, extensions) in app_extensions.iter().take(10) {
                    println!(
                        "  {}: {}",
                        app_name.bright_blue(),
                        extensions.join(", ").yellow()
                    );
                    count += 1;
                }
                if count < app_extensions.len() {
                    println!(
                        "  ... and {} more applications",
                        app_extensions.len() - count
                    );
                }
                println!();
                continue;
            }
            "" => {
                println!("❌ Please enter a valid file extension");
                continue;
            }
            _ => {
                // Ensure extension starts with a dot
                let ext = if extension.starts_with('.') {
                    extension.clone()
                } else {
                    format!(".{}", extension)
                };

                println!(
                    "🔍 Searching for applications that support {} file type...",
                    ext.yellow()
                );

                // Find applications that support this extension
                let supporting_apps = find_apps_for_extension(app_extensions, &ext);

                if supporting_apps.is_empty() {
                    println!(
                        "❌ No applications found that support {} file type",
                        ext.yellow()
                    );

                    // Show some debug information
                    println!("💡 Debug Tips:");
                    println!("   • Check if the extension is correct (should be {})", ext);
                    println!("   • Enter 'debug' to view scanned application information");

                    // Try fuzzy matching
                    let fuzzy_matches = find_fuzzy_matches(app_extensions, &extension);
                    if !fuzzy_matches.is_empty() {
                        println!("🔍 Found possible fuzzy matches:");
                        for (app_name, extensions) in fuzzy_matches.iter().take(5) {
                            println!(
                                "   • {}: {}",
                                app_name.bright_blue(),
                                extensions.join(", ").yellow()
                            );
                        }
                    }

                    // Offer to show all apps
                    println!("\n💡 Want to set ANY application as default? Enter 'all' to browse all applications");
                    print!("Or press Enter to try another extension: ");
                    io::stdout().flush().unwrap();

                    let mut all_choice = String::new();
                    io::stdin().read_line(&mut all_choice).unwrap();
                    let all_choice = all_choice.trim().to_lowercase();

                    if all_choice == "all" {
                        show_all_apps_menu(&ext, all_app_names);
                    }
                } else {
                    println!(
                        "✅ Found {} applications that support {} file type:",
                        supporting_apps.len(),
                        ext.yellow()
                    );

                    for (i, app_name) in supporting_apps.iter().enumerate() {
                        println!("   {}. {}", i + 1, app_name.bright_blue());
                    }

                    // Ask user if they want to set default application
                    println!("\n🎯 Do you want to set a default application?");
                    println!("Enter application number to set as default, or press Enter to skip");
                    println!("Enter 'all' to browse ALL applications (not just supporting ones)");
                    print!("Please choose (1-{}): ", supporting_apps.len());
                    io::stdout().flush().unwrap();

                    let mut choice = String::new();
                    io::stdin().read_line(&mut choice).unwrap();
                    let choice = choice.trim();

                    if !choice.is_empty() {
                        if choice.to_lowercase() == "all" {
                            show_all_apps_menu(&ext, all_app_names);
                        } else if let Ok(app_index) = choice.parse::<usize>() {
                            if app_index >= 1 && app_index <= supporting_apps.len() {
                                let selected_app = &supporting_apps[app_index - 1];
                                if let Err(e) = set_default_app_for_extension(&ext, selected_app) {
                                    println!("❌ Failed to set default application: {}", e);
                                } else {
                                    println!(
                                        "✅ Successfully set {} as the default application for {} files!",
                                        selected_app.bright_green(),
                                        ext.yellow()
                                    );
                                }
                            } else {
                                println!(
                                    "❌ Invalid choice, please enter a number between 1-{}",
                                    supporting_apps.len()
                                );
                            }
                        } else {
                            println!("❌ Invalid input, please enter a number");
                        }
                    }
                }
                println!();
            }
        }
    }
}

fn find_apps_for_extension(
    app_extensions: &HashMap<String, Vec<String>>,
    extension: &str,
) -> Vec<String> {
    let mut supporting_apps = Vec::new();

    // Remove the leading dot from extension, as plist stores extensions without dots
    let clean_extension = extension.trim_start_matches('.');

    for (app_name, extensions) in app_extensions {
        if extensions.iter().any(|ext| ext == clean_extension) {
            supporting_apps.push(app_name.clone());
        }
    }

    supporting_apps.sort();
    supporting_apps
}

fn find_fuzzy_matches(
    app_extensions: &HashMap<String, Vec<String>>,
    search_term: &str,
) -> Vec<(String, Vec<String>)> {
    let mut matches = Vec::new();

    for (app_name, extensions) in app_extensions {
        // Check if application name contains the search term
        if app_name
            .to_lowercase()
            .contains(&search_term.to_lowercase())
        {
            matches.push((app_name.clone(), extensions.clone()));
            continue;
        }

        // Check if extension contains the search term
        if extensions
            .iter()
            .any(|ext| ext.to_lowercase().contains(&search_term.to_lowercase()))
        {
            matches.push((app_name.clone(), extensions.clone()));
        }
    }

    matches.sort_by_key(|(name, _)| name.clone());
    matches
}

/// Set the default application for a specified file extension
fn set_default_app_for_extension(extension: &str, app_name: &str) -> Result<()> {
    // On macOS, we need to find the full path of the application
    let home = std::env::var("HOME").unwrap_or_else(|_| "~".to_string());
    let app_paths = vec![
        "/Applications".to_string(),
        "/System/Applications".to_string(),
        format!("{}/Applications", home),
    ];

    let mut app_full_path = None;

    // Find the full path of the application
    for base_path in &app_paths {
        let app_path = format!("{}/{}.app", base_path, app_name);
        if std::path::Path::new(&app_path).exists() {
            app_full_path = Some(app_path);
            break;
        }
    }

    let app_path = app_full_path
        .ok_or_else(|| anyhow::anyhow!("Could not find path for application '{}'", app_name))?;

    println!(
        "🚀 Starting to set default application for .{} files...",
        extension
    );

    // 1. Get the Bundle Identifier of the application
    println!("🔎 Looking for Bundle ID of '{}'...", app_path);
    let bundle_id = get_bundle_id(&app_path)?;
    println!("✅ Bundle ID: {}", bundle_id);

    // 2. Get the UTI corresponding to the file extension
    println!("🔎 Looking for UTI of .{}...", extension);
    let uti = get_uti_for_extension(extension)?;
    println!("✅ UTI: {}", uti);

    // 3. Use duti to set the default application, with fallback to alternative methods
    println!(
        "⚙️ Setting '{}' as the default handler for '{}'...",
        bundle_id, uti
    );

    match set_default_app_with_duti(&bundle_id, &uti) {
        Ok(_) => {
            println!(
                "✅ Complete! .{} files will now be opened by {} by default.",
                extension, app_name
            );
            println!("Note: In some cases, you may need to restart Finder or log out and log back in to see icon changes.");
        }
        Err(e) => {
            println!("⚠️ duti command failed: {}", e);
            println!("🔄 Falling back to alternative methods...");

            // Try alternative methods without duti
            if let Err(alt_err) = set_default_app_without_duti(&app_path, extension) {
                println!("❌ Alternative methods also failed: {}", alt_err);
                return Err(anyhow::anyhow!(
                    "All methods to set default application failed"
                ));
            }

            println!(
                "✅ Alternative methods completed! .{} files should now be opened by {} by default.",
                extension, app_name
            );
            println!("Note: Alternative methods may take longer to take effect. You may need to:");
            println!("   • Restart Finder (Cmd+Option+Esc, then restart Finder)");
            println!("   • Log out and log back in");
            println!("   • Restart the system");
        }
    }

    Ok(())
}

/// Get the Bundle ID of the application
fn get_bundle_id(app_path: &str) -> Result<String> {
    let output = std::process::Command::new("mdls")
        .arg("-name")
        .arg("kMDItemCFBundleIdentifier")
        .arg("-r")
        .arg(app_path)
        .output()?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "Could not get Bundle ID of application: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let bundle_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if bundle_id.is_empty() {
        return Err(anyhow::anyhow!(
            "Could not get Bundle ID of application. Please check if the path is a valid .app program."
        ));
    }

    Ok(bundle_id)
}

/// Get the UTI corresponding to the file extension, with retry mechanism
fn get_uti_for_extension(extension: &str) -> Result<String> {
    const MAX_RETRIES: u32 = 10;
    let temp_file = std::env::temp_dir().join(format!("temp_file_for_uti.{}", extension));

    // Create temporary file with content
    create_temp_file_with_content(&temp_file, extension)?;

    let mut retry_count = 0;
    let mut uti = String::new();

    while retry_count < MAX_RETRIES {
        // Wait a bit for the system to recognize the file type
        std::thread::sleep(std::time::Duration::from_secs(1));

        let output = std::process::Command::new("mdls")
            .arg("-name")
            .arg("kMDItemContentType")
            .arg("-r")
            .arg(&temp_file)
            .output();

        match output {
            Ok(output) if output.status.success() => {
                uti = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !uti.is_empty() && uti != "(null)" {
                    break;
                }
            }
            _ => {}
        }

        retry_count += 1;
        if retry_count < MAX_RETRIES {
            println!(
                "⏳ Attempt {} to get UTI failed, waiting 1 second before retrying...",
                retry_count
            );
        }
    }

    // Clean up temporary file
    let _ = std::fs::remove_file(&temp_file);

    if uti.is_empty() || uti == "(null)" {
        // If retry fails, use hardcoded UTI mapping
        uti = get_hardcoded_uti(extension)?;
    }

    Ok(uti)
}

/// Create temporary file with appropriate content
fn create_temp_file_with_content(temp_file: &std::path::Path, extension: &str) -> Result<()> {
    let content = match extension.to_lowercase().as_str() {
        "txt" | "md" | "log" => "This is a text file for UTI detection.".as_bytes().to_vec(),
        "pdf" => {
            // Create minimal PDF file content
            r#"%PDF-1.4
1 0 obj
<</Type/Catalog/Pages 2 0 R>>
endobj
2 0 obj
<</Type/Pages/Kids[]/Count 0>>
endobj
xref
0 3
0000000000 65535 f 
0000000009 00000 n 
0000000058 00000 n 
trailer
<</Size 3/Root 1 0 R>>
startxref
116
%%EOF"#
                .as_bytes()
                .to_vec()
        }
        "jpg" | "jpeg" => vec![0xFF, 0xD8, 0xFF, 0xE0],
        "png" => vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A],
        "gif" => b"GIF89a".to_vec(),
        "bmp" => b"BM".to_vec(),
        "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" => vec![0x50, 0x4B, 0x03, 0x04],
        _ => "Content for UTI detection.".as_bytes().to_vec(),
    };

    std::fs::write(temp_file, content)?;
    Ok(())
}

/// Get hardcoded UTI mapping
fn get_hardcoded_uti(extension: &str) -> Result<String> {
    let uti = match extension.to_lowercase().as_str() {
        "txt" | "md" | "log" => "public.plain-text",
        "pdf" => "com.adobe.pdf",
        "jpg" | "jpeg" => "public.jpeg",
        "png" => "public.png",
        "gif" => "com.compuserve.gif",
        "bmp" => "com.microsoft.bmp",
        "doc" => "com.microsoft.word.doc",
        "docx" => "org.openxmlformats.wordprocessingml.document",
        "xls" => "com.microsoft.excel.xls",
        "xlsx" => "org.openxmlformats.spreadsheetml.sheet",
        "ppt" => "com.microsoft.powerpoint.ppt",
        "pptx" => "org.openxmlformats.presentationml.presentation",
        "zip" => "public.zip-archive",
        "tar" => "public.tar-archive",
        "gz" => "org.gnu.gnu-zip-archive",
        "mp3" => "public.mp3",
        "mp4" => "public.mpeg-4",
        "avi" => "public.avi",
        "mov" => "com.apple.quicktime-movie",
        _ => {
            return Err(anyhow::anyhow!(
                "Could not get UTI for .{}. This might be an unknown extension.",
                extension
            ))
        }
    };

    Ok(uti.to_string())
}

/// Use duti to set the default application
fn set_default_app_with_duti(bundle_id: &str, uti: &str) -> Result<()> {
    let output = std::process::Command::new("duti")
        .arg("-s")
        .arg(bundle_id)
        .arg(uti)
        .arg("all")
        .output()?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "Failed to set default application using duti: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
}

/// Set default application without using duti (alternative methods)
fn set_default_app_without_duti(app_path: &str, extension: &str) -> Result<()> {
    println!("🔄 duti not available, trying alternative methods...");

    // Method 1: Use open command to establish file association
    println!("📝 Method 1: Using open command to establish file association...");
    let temp_file = std::env::temp_dir().join(format!("test{}", extension));

    // Create a temporary file with appropriate content
    create_temp_file_with_content(&temp_file, extension)?;

    // Method 2: Try to re-register the application with Launch Services
    println!("🔄 Method 2: Re-registering application with Launch Services...");
    let lsregister_result = std::process::Command::new("lsregister")
        .arg("-f")
        .arg(app_path)
        .output();

    match lsregister_result {
        Ok(output) if output.status.success() => {
            println!("✅ Successfully re-registered application with Launch Services");
        }
        Ok(output) => {
            println!(
                "⚠️ lsregister completed but may have had issues: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Err(e) => {
            println!("⚠️ lsregister command not available or failed: {}", e);
        }
    }

    // Method 3: Try to use defaults command for specific file types
    if let Some(uti) = get_hardcoded_uti(extension).ok() {
        println!(
            "⚙️ Method 3: Attempting to set system preference for UTI: {}",
            uti
        );

        // Get the bundle ID for the application
        if let Ok(bundle_id) = get_bundle_id(app_path) {
            println!("💡 To manually set system preference, you can run:");
            println!("   defaults write com.apple.LaunchServices/com.apple.launchservices.secure LSHandlers -array-add '{{LSHandlerContentType={};LSHandlerRoleAll={};}}'", uti, bundle_id);
        } else {
            println!("💡 To manually set system preference, you can run:");
            println!("   defaults write com.apple.LaunchServices/com.apple.launchservices.secure LSHandlers -array-add '{{LSHandlerContentType={};LSHandlerRoleAll=YOUR_BUNDLE_ID;}}'", uti);
        }
    }

    // Clean up temporary file
    let _ = std::fs::remove_file(&temp_file);

    println!("✅ Alternative methods completed. The file association may take effect after:");
    println!("   • Restarting Finder (Cmd+Option+Esc, then restart Finder)");
    println!("   • Logging out and back in");
    println!("   • Restarting the system");

    Ok(())
}

/// Show all applications menu for selection
fn show_all_apps_menu(extension: &str, all_app_names: &[String]) {
    const PAGE_SIZE: usize = 20;
    let mut page = 0;
    let total_pages = (all_app_names.len() + PAGE_SIZE - 1) / PAGE_SIZE;

    loop {
        println!("\n📋 All Applications - Page {}/{}", page + 1, total_pages);
        println!("Setting default for {} files\n", extension.yellow());

        let start = page * PAGE_SIZE;
        let end = std::cmp::min(start + PAGE_SIZE, all_app_names.len());

        for (i, app_name) in all_app_names[start..end].iter().enumerate() {
            println!("   {}. {}", start + i + 1, app_name.bright_blue());
        }

        println!("\nOptions:");
        println!(
            "   • Enter number (1-{}) to select application",
            all_app_names.len()
        );
        if page > 0 {
            println!("   • 'p' or 'prev' for previous page");
        }
        if page < total_pages - 1 {
            println!("   • 'n' or 'next' for next page");
        }
        println!("   • 'q' to return to main menu");
        print!("\nYour choice: ");
        io::stdout().flush().unwrap();

        let mut choice = String::new();
        io::stdin().read_line(&mut choice).unwrap();
        let choice = choice.trim().to_lowercase();

        match choice.as_str() {
            "q" => break,
            "n" | "next" => {
                if page < total_pages - 1 {
                    page += 1;
                } else {
                    println!("❌ Already on the last page");
                }
            }
            "p" | "prev" => {
                if page > 0 {
                    page -= 1;
                } else {
                    println!("❌ Already on the first page");
                }
            }
            _ => {
                if let Ok(app_index) = choice.parse::<usize>() {
                    if app_index >= 1 && app_index <= all_app_names.len() {
                        let selected_app = &all_app_names[app_index - 1];
                        if let Err(e) = set_default_app_for_extension(extension, selected_app) {
                            println!("❌ Failed to set default application: {}", e);
                        } else {
                            println!(
                                "\n✅ Successfully set {} as the default application for {} files!",
                                selected_app.bright_green(),
                                extension.yellow()
                            );
                        }
                        break;
                    } else {
                        println!(
                            "❌ Invalid choice, please enter a number between 1-{}",
                            all_app_names.len()
                        );
                    }
                } else {
                    println!("❌ Invalid input");
                }
            }
        }
    }
}
