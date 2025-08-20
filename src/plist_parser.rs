use anyhow::Result;
use std::collections::HashSet;
use std::process::Command;

pub struct PlistParser;

impl PlistParser {
    pub fn new() -> Self {
        Self
    }

    /// 解析应用程序的 Info.plist 文件，提取支持的文件扩展名
    pub fn parse_extensions(&self, plist_path: &str) -> Result<Vec<String>> {
        let mut extensions = HashSet::new();

        // 检查文件是否存在
        if !std::path::Path::new(plist_path).exists() {
            return Ok(vec![]);
        }

        // 使用 PlistBuddy 命令获取文档类型数量
        let count_output = Command::new("/usr/libexec/PlistBuddy")
            .arg("-c")
            .arg("Print :CFBundleDocumentTypes")
            .arg(plist_path)
            .output();

        if let Ok(output) = count_output {
            let content = String::from_utf8_lossy(&output.stdout);

            // 计算文档类型数量
            let doc_type_count = content.lines().filter(|line| line.contains("Dict")).count();

            if doc_type_count > 0 {
                // 遍历每个文档类型
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

                        // 解析扩展名
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

        // 转换为向量并排序
        let mut result: Vec<String> = extensions.into_iter().collect();
        result.sort();
        Ok(result)
    }

    /// 获取应用程序的显示名称
    pub fn get_app_display_name(&self, plist_path: &str) -> Result<Option<String>> {
        if !std::path::Path::new(plist_path).exists() {
            return Ok(None);
        }

        let output = Command::new("/usr/libexec/PlistBuddy")
            .arg("-c")
            .arg("Print :CFBundleDisplayName")
            .arg(plist_path)
            .output();

        if let Ok(output) = output {
            let content = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !content.is_empty() && !content.contains("Does Not Exist") {
                return Ok(Some(content));
            }
        }

        // 如果没有显示名称，尝试获取包名称
        let output = Command::new("/usr/libexec/PlistBuddy")
            .arg("-c")
            .arg("Print :CFBundleName")
            .arg(plist_path)
            .output();

        if let Ok(output) = output {
            let content = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !content.is_empty() && !content.contains("Does Not Exist") {
                return Ok(Some(content));
            }
        }

        Ok(None)
    }
}
