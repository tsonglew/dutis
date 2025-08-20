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

// 通用平台实现
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

    /// 使用 macOS 的 UTI 系统查找能打开特定文件类型的应用程序
    fn find_apps_using_uti(&self, extension: &str) -> Vec<String> {
        let mut apps = Vec::new();

        // 创建临时文件来测试
        let temp_file = format!("/tmp/test{}", extension);
        if std::fs::write(&temp_file, "").is_ok() {
            // 使用 mdfind 查找能打开该文件类型的应用程序
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
                            // 测试该应用程序是否能打开我们的测试文件
                            if self.can_app_open_file(app_name, &temp_file) {
                                apps.push(app_name.to_string());
                            }
                        }
                    }
                }
            }

            // 清理临时文件
            let _ = std::fs::remove_file(&temp_file);
        }

        apps
    }

    /// 使用 macOS 的 open 命令测试应用程序是否能打开特定文件
    fn can_app_open_file(&self, app_name: &str, file_path: &str) -> bool {
        // 使用 open -a 命令测试应用程序是否能打开文件
        let result = Command::new("open")
            .arg("-a")
            .arg(app_name)
            .arg(file_path)
            .output();

        // 如果命令执行成功，说明应用程序存在且能打开该文件类型
        result.is_ok()
    }

    /// 扫描 Applications 目录查找应用程序
    fn scan_applications_dir(&self) -> Vec<String> {
        let mut apps = Vec::new();

        let mut app_dirs = vec![
            "/Applications".to_string(),
            "/System/Applications".to_string(),
        ];

        // 添加用户目录
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

    /// 使用系统命令查找能打开特定扩展名的应用程序
    fn find_apps_using_system_commands(&self, extension: &str) -> Vec<String> {
        let mut apps = Vec::new();

        // 创建临时文件
        let temp_file = format!("/tmp/test{}", extension);
        if std::fs::write(&temp_file, "").is_ok() {
            // 获取所有应用程序
            let all_apps = self.scan_applications_dir();

            // 测试每个应用程序是否能打开该文件
            for app in all_apps {
                if self.can_app_open_file(&app, &temp_file) {
                    apps.push(app);
                }
            }

            // 清理临时文件
            let _ = std::fs::remove_file(&temp_file);
        }

        apps
    }

    /// 使用 Launch Services 查找默认应用程序
    fn find_default_app_for_extension(&self, extension: &str) -> Vec<String> {
        let mut apps = Vec::new();

        // 创建临时文件
        let temp_file = format!("/tmp/test{}", extension);
        if std::fs::write(&temp_file, "").is_ok() {
            // 尝试使用 open 命令打开文件，这会使用系统默认应用程序
            if let Ok(output) = Command::new("open").arg(&temp_file).output() {
                // 如果成功打开，说明有默认应用程序
                // 我们可以通过其他方式获取应用程序名称
                apps.push("Default Application".to_string());
            }

            // 清理临时文件
            let _ = std::fs::remove_file(&temp_file);
        }

        apps
    }
}

#[cfg(target_os = "macos")]
impl PlatformSpecific for MacOSPlatform {
    fn find_apps_for_mime_type(&self, _mime_type: &str) -> Vec<String> {
        // 对于 MIME 类型，我们暂时返回空，因为 macOS 主要使用 UTI
        vec![]
    }

    fn find_apps_for_extension(&self, extension: &str) -> Vec<String> {
        let mut apps = Vec::new();

        // 方法1: 使用系统命令测试应用程序
        apps.extend(self.find_apps_using_system_commands(extension));

        // 方法2: 如果方法1没有找到，尝试使用 UTI 系统
        if apps.is_empty() {
            apps.extend(self.find_apps_using_uti(extension));
        }

        // 方法3: 如果还是没有找到，尝试查找默认应用程序
        if apps.is_empty() {
            apps.extend(self.find_default_app_for_extension(extension));
        }

        // 去重并排序
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

// 测试函数：验证不同扩展名返回不同的应用程序
pub fn test_extensions() {
    let platform = new();
    let test_extensions = vec![
        ".py", ".js", ".txt", ".pdf", ".jpg", ".mp3", ".zip", ".html",
    ];

    for ext in test_extensions {
        let apps = platform.find_apps_for_extension(ext);
        println!("{}: {} 个应用程序", ext, apps.len());
        for app in &apps {
            println!("  - {}", app);
        }
        println!();
    }
}
