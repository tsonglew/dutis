use anyhow::Result;
use std::collections::HashSet;
use std::process::Command;

pub struct PlistParser;

impl PlistParser {
    pub fn new() -> Self {
        Self
    }

    /// Parse the Info.plist file of an application to extract supported file extensions
    pub fn parse_extensions(&self, plist_path: &str) -> Result<Vec<String>> {
        let mut extensions = HashSet::new();

        // Check if file exists
        if !std::path::Path::new(plist_path).exists() {
            return Ok(vec![]);
        }

        // Use PlistBuddy command to get document type count
        let count_output = Command::new("/usr/libexec/PlistBuddy")
            .arg("-c")
            .arg("Print :CFBundleDocumentTypes")
            .arg(plist_path)
            .output();

        if let Ok(output) = count_output {
            let content = String::from_utf8_lossy(&output.stdout);

            let mut is_collecting = false;
            for line in content.lines() {
                let line = line.trim();
                if line == "}" && is_collecting {
                    is_collecting = false;
                    continue;
                }
                if is_collecting && line != "" {
                    extensions.insert(line.to_string());
                    continue;
                }
                if line == "CFBundleTypeExtensions = Array {" {
                    is_collecting = true;
                }
            }
        }

        // Convert to vector and sort
        let mut result: Vec<String> = extensions.into_iter().collect();
        result.sort();
        Ok(result)
    }
}
