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
    println!("🔍 macOS 应用程序文件扩展名查看器");
    println!("正在扫描系统应用程序...\n");

    let app_scanner = AppScanner::new();
    let plist_parser = PlistParser::new();

    // 扫描应用程序
    let apps = app_scanner.scan_applications()?;
    println!("找到 {} 个应用程序\n", apps.len());

    // 分析每个应用程序支持的文件扩展名
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

    // 显示完整结果
    display_results(&app_extensions);

    // 交互式查询功能
    interactive_query(&app_extensions);

    Ok(())
}

fn interactive_query(app_extensions: &HashMap<String, Vec<String>>) {
    println!("\n🎯 交互式查询模式");
    println!("输入文件后缀（如: py, js, txt）来查找支持的应用程序");
    println!("输入 'quit' 或 'exit' 退出程序");
    println!("输入 'debug' 显示调试信息\n");

    loop {
        print!("请输入文件后缀: ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();

        let extension = input.trim().to_lowercase();

        match extension.as_str() {
            "quit" | "exit" | "q" => {
                println!("👋 再见！");
                break;
            }
            "debug" => {
                println!("\n🔍 调试信息:");
                println!("扫描到的应用程序数量: {}", app_extensions.len());
                println!("前10个应用程序及其支持的扩展名:");

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
                    println!("  ... 还有 {} 个应用程序", app_extensions.len() - count);
                }
                println!();
                continue;
            }
            "" => {
                println!("❌ 请输入有效的文件后缀");
                continue;
            }
            _ => {
                // 确保扩展名以 . 开头
                let ext = if extension.starts_with('.') {
                    extension.clone()
                } else {
                    format!(".{}", extension)
                };

                println!("🔍 正在查找支持 {} 文件类型的应用程序...", ext.yellow());

                // 查找支持该扩展名的应用程序
                let supporting_apps = find_apps_for_extension(app_extensions, &ext);

                if supporting_apps.is_empty() {
                    println!("❌ 未找到支持 {} 文件类型的应用程序", ext.yellow());

                    // 显示一些调试信息
                    println!("💡 调试提示:");
                    println!("   • 检查扩展名是否正确（应该是 {}）", ext);
                    println!("   • 输入 'debug' 查看扫描到的应用程序信息");

                    // 尝试模糊匹配
                    let fuzzy_matches = find_fuzzy_matches(app_extensions, &extension);
                    if !fuzzy_matches.is_empty() {
                        println!("🔍 找到可能的模糊匹配:");
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
                        "✅ 找到 {} 个支持 {} 文件类型的应用程序:",
                        supporting_apps.len(),
                        ext.yellow()
                    );

                    for (i, app_name) in supporting_apps.iter().enumerate() {
                        println!("   {}. {}", i + 1, app_name.bright_blue());
                    }

                    // 询问用户是否要设置默认应用
                    println!("\n🎯 是否要设置默认应用？");
                    println!("输入应用程序编号来设置默认应用，或按回车跳过");
                    print!("请选择 (1-{}): ", supporting_apps.len());
                    io::stdout().flush().unwrap();

                    let mut choice = String::new();
                    io::stdin().read_line(&mut choice).unwrap();
                    let choice = choice.trim();

                    if !choice.is_empty() {
                        if let Ok(app_index) = choice.parse::<usize>() {
                            if app_index >= 1 && app_index <= supporting_apps.len() {
                                let selected_app = &supporting_apps[app_index - 1];
                                if let Err(e) = set_default_app_for_extension(&ext, selected_app) {
                                    println!("❌ 设置默认应用失败: {}", e);
                                } else {
                                    println!(
                                        "✅ 成功设置 {} 为 {} 文件的默认应用！",
                                        selected_app.bright_green(),
                                        ext.yellow()
                                    );
                                }
                            } else {
                                println!(
                                    "❌ 无效的选择，请输入 1-{} 之间的数字",
                                    supporting_apps.len()
                                );
                            }
                        } else {
                            println!("❌ 无效的输入，请输入数字");
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

    // 移除扩展名开头的点号，因为 plist 中存储的是不带点的扩展名
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
        // 检查应用程序名称是否包含搜索词
        if app_name
            .to_lowercase()
            .contains(&search_term.to_lowercase())
        {
            matches.push((app_name.clone(), extensions.clone()));
            continue;
        }

        // 检查扩展名是否包含搜索词
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
    println!("📱 应用程序支持的文件扩展名:");
    println!("{}", "=".repeat(60));

    let mut sorted_apps: Vec<_> = app_extensions.iter().collect();
    sorted_apps.sort_by_key(|(name, _)| *name);

    for (app_name, extensions) in sorted_apps {
        println!("\n🎯 {}", app_name.bright_blue().bold());
        println!("   📁 支持的文件扩展名:");

        // 按扩展名类型分组显示
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
    println!("📊 统计信息:");
    println!("   • 总应用程序数量: {}", app_extensions.len());

    let total_extensions: usize = app_extensions.values().map(|v| v.len()).sum();
    println!("   • 总支持扩展名数量: {}", total_extensions);

    let unique_extensions: std::collections::HashSet<_> =
        app_extensions.values().flat_map(|v| v.iter()).collect();
    println!("   • 唯一扩展名数量: {}", unique_extensions.len());
}

fn get_extension_category(extension: &str) -> &'static str {
    match extension.to_lowercase().as_str() {
        "py" | "js" | "ts" | "jsx" | "tsx" | "rs" | "cpp" | "c" | "h" | "java" | "kt" | "swift"
        | "go" | "php" | "rb" | "pl" | "sh" => "编程语言",
        "html" | "css" | "scss" | "sass" | "less" | "xml" | "json" | "yaml" | "toml" => {
            "Web/标记语言"
        }
        "txt" | "md" | "log" | "rtf" => "文本文档",
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" => "办公文档",
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "svg" | "ico" | "tiff" | "webp" => "图像文件",
        "mp3" | "mp4" | "avi" | "mov" | "wmv" | "flv" | "mkv" | "wav" | "aac" | "ogg" => {
            "音视频文件"
        }
        "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" => "压缩文件",
        "psd" | "ai" | "sketch" | "fig" => "设计文件",
        _ => "其他文件",
    }
}

fn get_category_color(category: &str) -> colored::Color {
    match category {
        "编程语言" => colored::Color::Green,
        "Web/标记语言" => colored::Color::Blue,
        "文本文档" => colored::Color::Cyan,
        "办公文档" => colored::Color::Magenta,
        "图像文件" => colored::Color::Yellow,
        "音视频文件" => colored::Color::Red,
        "压缩文件" => colored::Color::BrightBlack,
        "设计文件" => colored::Color::BrightMagenta,
        _ => colored::Color::White,
    }
}

/// 设置指定文件扩展名的默认应用程序
fn set_default_app_for_extension(extension: &str, app_name: &str) -> Result<()> {
    // 在 macOS 上，我们需要找到应用程序的完整路径
    let home = std::env::var("HOME").unwrap_or_else(|_| "~".to_string());
    let app_paths = vec![
        "/Applications".to_string(),
        "/System/Applications".to_string(),
        format!("{}/Applications", home),
    ];

    let mut app_full_path = None;

    // 查找应用程序的完整路径
    for base_path in &app_paths {
        let app_path = format!("{}/{}.app", base_path, app_name);
        if std::path::Path::new(&app_path).exists() {
            app_full_path = Some(app_path);
            break;
        }
    }

    let app_path =
        app_full_path.ok_or_else(|| anyhow::anyhow!("找不到应用程序 '{}' 的路径", app_name))?;

    // 使用 macOS 的 Launch Services 来设置默认应用
    // 这需要创建一个临时文件来测试关联
    let temp_dir = std::env::temp_dir();
    let temp_file = temp_dir.join(format!("test{}", extension));

    // 创建临时文件
    std::fs::write(&temp_file, "test")?;

    // 使用 open 命令设置默认应用
    let output = std::process::Command::new("open")
        .arg("-a")
        .arg(&app_path)
        .arg(&temp_file)
        .output()?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "设置默认应用失败: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    // 清理临时文件
    let _ = std::fs::remove_file(temp_file);

    // 使用 duti 命令来设置默认应用（如果可用）
    if let Ok(duti_output) = std::process::Command::new("duti")
        .arg("-s")
        .arg(&app_path)
        .arg(extension)
        .output()
    {
        if duti_output.status.success() {
            println!("💡 使用 duti 命令成功设置默认应用");
        }
    }

    Ok(())
}
