use anyhow::Result;
use std::process::Command;

pub struct AppScanner;

impl AppScanner {
    pub fn new() -> Self {
        Self
    }

    /// Scan applications in the system
    pub fn scan_applications(&self) -> Result<Vec<String>> {
        let mut app_paths = Vec::new();

        // Use mdfind command to find applications, following the logic of the original script
        let output = Command::new("mdfind")
            .arg("kMDItemKind == 'Application'")
            .arg("-onlyin")
            .arg("/System/Applications")
            .arg("-onlyin")
            .arg("/Applications")
            .output()?;

        let content = String::from_utf8_lossy(&output.stdout);

        for line in content.lines() {
            let line = line.trim();
            if !line.is_empty() && line.ends_with(".app") {
                app_paths.push(line.to_string());
            }
        }

        // Add applications from user directory
        if let Ok(home) = std::env::var("HOME") {
            let user_apps = format!("{}/Applications", home);
            if let Ok(entries) = std::fs::read_dir(&user_apps) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension() == Some(std::ffi::OsStr::new("app")) {
                        if let Some(path_str) = path.to_str() {
                            app_paths.push(path_str.to_string());
                        }
                    }
                }
            }
        }

        // Remove duplicates and sort
        app_paths.sort();
        app_paths.dedup();

        Ok(app_paths)
    }

    /// Get the display name of the application
    pub fn get_app_display_name(&self, app_path: &str) -> Option<String> {
        Path::new(app_path)
            .file_stem()
            .and_then(|name| name.to_str())
            .map(|s| s.to_string())
    }
}

use std::ffi::OsStr;
use std::fs;
use std::path::Path;
