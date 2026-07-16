//! 主题同步
//!
//! 将 Votek 主题同步到 code-server 的 settings.json，
//! 并在 IDE 窗口打开时通过 URL 参数即时应用主题。

use crate::code_server::format_cs_url;
use crate::code_server::paths::cs_user_settings_path;
use tauri::AppHandle;

/// Votek 主题 → code-server 内置主题名映射
pub(crate) fn cs_theme_name(votek: &str) -> &'static str {
    match votek {
        "light" => "Default Light+",
        _ => "Default Dark+", // "dark" 或未知都 fallback 到深色
    }
}

/// 基础 URL 编码（仅处理主题名中可能含有的特殊字符：空格、+）
pub(crate) fn url_encode_theme(theme: &str) -> String {
    theme.replace(' ', "%20").replace('+', "%2B")
}

/// 将 `workbench.colorTheme` 写入 code-server 的 User/settings.json
/// 用读-改-写 merge 方式，不破坏用户其他设置
pub(crate) fn write_color_theme(app: &AppHandle, theme_name: &str) -> Result<(), String> {
    let path = cs_user_settings_path(app);
    let mut settings = if path.exists() {
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .unwrap_or_else(|| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };
    // 设置 workbench.colorTheme
    if let Some(obj) = settings.as_object_mut() {
        obj.insert(
            "workbench.colorTheme".to_string(),
            serde_json::Value::String(theme_name.to_string()),
        );
    } else {
        settings = serde_json::json!({"workbench.colorTheme": theme_name});
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("创建 User 目录失败: {}", e))?;
    }
    let content = serde_json::to_string_pretty(&settings)
        .map_err(|e| format!("序列化设置失败: {}", e))?;
    std::fs::write(&path, content)
        .map_err(|e| format!("写入设置文件失败: {}", e))?;
    eprintln!(
        "[CodeServer] 主题已写入: {}  ->  {}",
        path.display(),
        theme_name
    );
    Ok(())
}

/// 构建带主题参数的 code-server URL
pub(crate) fn cs_url_with_theme(port: u16, theme: Option<&str>) -> String {
    let base = format_cs_url(port);
    match theme {
        Some(t) => format!("{}?theme={}", base, url_encode_theme(t)),
        None => base,
    }
}
