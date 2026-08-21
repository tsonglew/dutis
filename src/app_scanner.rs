use anyhow::Result;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct InstalledApplication {
    pub name: String,
    pub path: PathBuf,
}

pub struct AppScanner;

impl AppScanner {
    pub fn new() -> Self {
        Self
    }

    pub fn scan_applications(&self) -> Result<Vec<InstalledApplication>> {
        let mut roots = vec![
            PathBuf::from("/Applications"),
            PathBuf::from("/System/Applications"),
        ];
        if let Some(home) = std::env::var_os("HOME") {
            roots.push(PathBuf::from(home).join("Applications"));
        }

        let mut applications = Vec::new();
        for root in roots.iter().filter(|root| root.is_dir()) {
            scan_root(root, &mut applications);
        }
        applications.sort_by_cached_key(|app| (app.name.to_ascii_lowercase(), app.path.clone()));
        applications.dedup_by(|left, right| left.path == right.path);
        Ok(applications)
    }
}

fn scan_root(root: &Path, applications: &mut Vec<InstalledApplication>) {
    let mut pending_directories = vec![root.to_path_buf()];
    while let Some(directory) = pending_directories.pop() {
        let Ok(entries) = std::fs::read_dir(directory) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_dir() {
                continue;
            }

            if path.extension().is_some_and(|extension| extension == "app") {
                if let Some(name) = path.file_stem().and_then(|name| name.to_str()) {
                    applications.push(InstalledApplication {
                        name: name.to_owned(),
                        path,
                    });
                }
            } else {
                pending_directories.push(path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn finds_nested_apps_without_descending_into_bundles() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("dutis-scanner-{}-{unique}", std::process::id()));
        let terminal = root.join("Utilities/Terminal.app");
        let nested_helper = terminal.join("Contents/Helpers/Hidden.app");
        fs::create_dir_all(&nested_helper).unwrap();

        let mut applications = Vec::new();
        scan_root(&root, &mut applications);

        assert_eq!(
            applications,
            vec![InstalledApplication {
                name: "Terminal".to_owned(),
                path: terminal,
            }]
        );
        fs::remove_dir_all(root).unwrap();
    }
}
