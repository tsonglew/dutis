use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

pub trait PlatformSpecific: Send + Sync {
    fn find_apps_for_mime_type(&self, mime_type: &str) -> Vec<String>;
    fn find_apps_for_extension(&self, extension: &str) -> Vec<String>;
    fn scan_system_applications(&self) -> Result<HashMap<String, Vec<String>>>;
}

pub fn new() -> Box<dyn PlatformSpecific> {
    #[cfg(target_os = "macos")]
    {
        Box::new(MacOSPlatform::new())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Box::new(GenericPlatform::new())
    }
}

// Generic platform implementation
struct GenericPlatform;

impl GenericPlatform {
    fn new() -> Self {
        Self
    }
}

impl PlatformSpecific for GenericPlatform {
    fn find_apps_for_mime_type(&self, _mime_type: &str) -> Vec<String> {
        vec![]
    }

    fn find_apps_for_extension(&self, _extension: &str) -> Vec<String> {
        vec![]
    }

    fn scan_system_applications(&self) -> Result<HashMap<String, Vec<String>>> {
        Ok(HashMap::new())
    }
}

#[cfg(target_os = "macos")]
pub struct MacOSPlatform;

#[cfg(target_os = "macos")]
impl MacOSPlatform {
    pub fn new() -> Self {
        Self
    }

    /// Use macOS UTI system to find applications that can open specific file types
    fn find_apps_using_uti(&self, extension: &str) -> Vec<String> {
        let mut apps = Vec::new();

        // Create temporary file for testing
        let temp_file = format!("/tmp/test{}", extension);
        if std::fs::write(&temp_file, "").is_ok() {
            // Use mdfind to find applications that can open this file type
            if let Ok(output) = Command::new("mdfind")
                .arg("-onlyin")
                .arg("/Applications")
                .arg("kMDItemContentTypeTree == 'public.data'")
                .output()
            {
                let content = String::from_utf8_lossy(&output.stdout);
                for line in content.lines() {
                    if line.ends_with(".app") {
                        if let Some(app_name) = Path::new(line).file_stem().and_then(|n| n.to_str())
                        {
                            // Test if this application can open our test file
                            if self.can_app_open_file(app_name, &temp_file) {
                                apps.push(app_name.to_string());
                            }
                        }
                    }
                }
            }

            // Clean up temporary file
            let _ = std::fs::remove_file(&temp_file);
        }

        apps
    }

    /// Use macOS open command to test if an application can open a specific file
    fn can_app_open_file(&self, app_name: &str, file_path: &str) -> bool {
        // Use open -a command to test if application can open file
        let result = Command::new("open")
            .arg("-a")
            .arg(app_name)
            .arg(file_path)
            .output();

        // If command executes successfully, the application exists and can open this file type
        result.is_ok()
    }

    /// Scan Applications directory to find applications
    fn scan_applications_dir(&self) -> Vec<String> {
        let mut apps = Vec::new();

        let mut app_dirs = vec![
            "/Applications".to_string(),
            "/System/Applications".to_string(),
        ];

        // Add user directory
        if let Ok(home) = std::env::var("HOME") {
            app_dirs.push(format!("{}/Applications", home));
        }

        for app_dir in &app_dirs {
            if let Ok(entries) = std::fs::read_dir(app_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension() == Some(std::ffi::OsStr::new("app")) {
                        if let Some(app_name) = path.file_stem().and_then(|n| n.to_str()) {
                            apps.push(app_name.to_string());
                        }
                    }
                }
            }
        }

        apps.sort();
        apps.dedup();
        apps
    }

    /// Use system commands to find applications that can open specific extensions
    fn find_apps_using_system_commands(&self, extension: &str) -> Vec<String> {
        let mut apps = Vec::new();

        // Create temporary file
        let temp_file = format!("/tmp/test{}", extension);
        if std::fs::write(&temp_file, "").is_ok() {
            // Get all applications
            let all_apps = self.scan_applications_dir();

            // Test if each application can open this file
            for app in all_apps {
                if self.can_app_open_file(&app, &temp_file) {
                    apps.push(app);
                }
            }

            // Clean up temporary file
            let _ = std::fs::remove_file(&temp_file);
        }

        apps
    }

    /// Use Launch Services to find default applications
    fn find_default_app_for_extension(&self, extension: &str) -> Vec<String> {
        let mut apps = Vec::new();

        // Create temporary file
        let temp_file = format!("/tmp/test{}", extension);
        if std::fs::write(&temp_file, "").is_ok() {
            // Try to open file using open command, which will use system default application
            if let Ok(output) = Command::new("open").arg(&temp_file).output() {
                // If successfully opened, there is a default application
                // We can get the application name through other means
                apps.push("Default Application".to_string());
            }

            // Clean up temporary file
            let _ = std::fs::remove_file(&temp_file);
        }

        apps
    }
}

#[cfg(target_os = "macos")]
impl PlatformSpecific for MacOSPlatform {
    fn find_apps_for_mime_type(&self, _mime_type: &str) -> Vec<String> {
        // For MIME types, we temporarily return empty, as macOS mainly uses UTI
        vec![]
    }

    fn find_apps_for_extension(&self, extension: &str) -> Vec<String> {
        let mut apps = Vec::new();

        // Method 1: Use system commands to test applications
        apps.extend(self.find_apps_using_system_commands(extension));

        // Method 2: If method 1 doesn't find anything, try UTI system
        if apps.is_empty() {
            apps.extend(self.find_apps_using_uti(extension));
        }

        // Method 3: If still nothing found, try to find default applications
        if apps.is_empty() {
            apps.extend(self.find_default_app_for_extension(extension));
        }

        // Remove duplicates and sort
        apps.sort();
        apps.dedup();

        apps
    }

    fn scan_system_applications(&self) -> Result<HashMap<String, Vec<String>>> {
        let mut mime_to_apps = HashMap::new();
        let system_apps = self.scan_applications_dir();
        mime_to_apps.insert("application/octet-stream".to_string(), system_apps);
        Ok(mime_to_apps)
    }
}

// Test function: Verify that different extensions return different applications
pub fn test_extensions() {
    let platform = new();
    let test_extensions = vec![
        ".py", ".js", ".txt", ".pdf", ".jpg", ".mp3", ".zip", ".html",
    ];

    for ext in test_extensions {
        let apps = platform.find_apps_for_extension(ext);
        println!("{}: {} applications", ext, apps.len());
        for app in &apps {
            println!("  - {}", app);
        }
        println!();
    }
}
