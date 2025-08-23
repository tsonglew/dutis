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
    println!("🔍 macOS Application File Extension Viewer");
    println!("Scanning system applications...\n");

    let app_scanner = AppScanner::new();
    let plist_parser = PlistParser::new();

    // Scan applications
    let apps = app_scanner.scan_applications()?;
    println!("Found {} applications\n", apps.len());

    // Analyze file extensions supported by each application
    let mut app_extensions: HashMap<String, Vec<String>> = HashMap::new();

    for app_path in apps {
        if let Some(app_name) = Path::new(&app_path).file_stem().and_then(|n| n.to_str()) {
            let info_plist_path = format!("{}/Contents/Info.plist", app_path);

            if let Ok(extensions) = plist_parser.parse_extensions(&info_plist_path) {
                if !extensions.is_empty() {
                    app_extensions.insert(app_name.to_string(), extensions);
                }
            }
        }
    }

    // Display complete results
    display_results(&app_extensions);

    // Interactive query functionality
    interactive_query(&app_extensions);

    Ok(())
}

fn interactive_query(app_extensions: &HashMap<String, Vec<String>>) {
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
                    print!("Please choose (1-{}): ", supporting_apps.len());
                    io::stdout().flush().unwrap();

                    let mut choice = String::new();
                    io::stdin().read_line(&mut choice).unwrap();
                    let choice = choice.trim();

                    if !choice.is_empty() {
                        if let Ok(app_index) = choice.parse::<usize>() {
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

fn display_results(app_extensions: &HashMap<String, Vec<String>>) {
    println!("📱 File Extensions Supported by Applications:");
    println!("{}", "=".repeat(60));

    let mut sorted_apps: Vec<_> = app_extensions.iter().collect();
    sorted_apps.sort_by_key(|(name, _)| *name);

    for (app_name, extensions) in sorted_apps {
        println!("\n🎯 {}", app_name.bright_blue().bold());
        println!("   📁 Supported file extensions:");

        // Group extensions by type for display
        let mut grouped_extensions: HashMap<&str, Vec<&str>> = HashMap::new();

        for ext in extensions {
            let category = get_extension_category(ext);
            grouped_extensions
                .entry(category)
                .or_insert_with(Vec::new)
                .push(ext);
        }

        for (category, exts) in grouped_extensions.iter() {
            let category_color = get_category_color(category);
            println!(
                "     {}: {}",
                category.color(category_color),
                exts.join(", ").yellow()
            );
        }
    }

    println!("\n{}", "=".repeat(60));
    println!("📊 Statistics:");
    println!("   • Total applications: {}", app_extensions.len());

    let total_extensions: usize = app_extensions.values().map(|v| v.len()).sum();
    println!("   • Total supported extensions: {}", total_extensions);

    let unique_extensions: std::collections::HashSet<_> =
        app_extensions.values().flat_map(|v| v.iter()).collect();
    println!("   • Unique extensions: {}", unique_extensions.len());
}

fn get_extension_category(extension: &str) -> &'static str {
    match extension.to_lowercase().as_str() {
        "py" | "js" | "ts" | "jsx" | "tsx" | "rs" | "cpp" | "c" | "h" | "java" | "kt" | "swift"
        | "go" | "php" | "rb" | "pl" | "sh" => "Programming Languages",
        "html" | "css" | "scss" | "sass" | "less" | "xml" | "json" | "yaml" | "toml" => {
            "Web/Markup Languages"
        }
        "txt" | "md" | "log" | "rtf" => "Text Documents",
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" => "Office Documents",
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "svg" | "ico" | "tiff" | "webp" => "Image Files",
        "mp3" | "mp4" | "avi" | "mov" | "wmv" | "flv" | "mkv" | "wav" | "aac" | "ogg" => {
            "Audio/Video Files"
        }
        "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" => "Compressed Files",
        "psd" | "ai" | "sketch" | "fig" => "Design Files",
        _ => "Other Files",
    }
}

fn get_category_color(category: &str) -> colored::Color {
    match category {
        "Programming Languages" => colored::Color::Green,
        "Web/Markup Languages" => colored::Color::Blue,
        "Text Documents" => colored::Color::Cyan,
        "Office Documents" => colored::Color::Magenta,
        "Image Files" => colored::Color::Yellow,
        "Audio/Video Files" => colored::Color::Red,
        "Compressed Files" => colored::Color::BrightBlack,
        "Design Files" => colored::Color::BrightMagenta,
        _ => colored::Color::White,
    }
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

    // 3. Use duti to set the default application
    println!(
        "⚙️ Setting '{}' as the default handler for '{}'...",
        bundle_id, uti
    );
    set_default_app_with_duti(&bundle_id, &uti)?;

    println!(
        "✅ Complete! .{} files will now be opened by {} by default.",
        extension, app_name
    );
    println!("Note: In some cases, you may need to restart Finder or log out and log back in to see icon changes.");

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
