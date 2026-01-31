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

            // Count document types
            let doc_type_count = content.lines().filter(|line| line.contains("Dict")).count();

            if doc_type_count > 0 {
                // Iterate through each document type
                for i in 0..doc_type_count {
                    let ext_output = Command::new("/usr/libexec/PlistBuddy")
                        .arg("-c")
                        .arg(&format!(
                            "Print :CFBundleDocumentTypes:{}:CFBundleTypeExtensions",
                            i
                        ))
                        .arg(plist_path)
                        .output();

                    if let Ok(ext_output) = ext_output {
                        let ext_content = String::from_utf8_lossy(&ext_output.stdout);

                        // Parse extensions
                        for line in ext_content.lines() {
                            let line = line.trim();
                            if !line.is_empty()
                                && !line.contains("Array {")
                                && !line.contains("}")
                                && !line.contains("Dict")
                            {
                                extensions.insert(line.to_string());
                            }
                        }
                    }
                }
            }
        }

        // Convert to vector and sort
        let mut result: Vec<String> = extensions.into_iter().collect();
        result.sort();
        Ok(result)
    }
}
