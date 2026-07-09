// 插件管理模块（类 VS Code 扩展市场）
// - 本地插件存储在 .codebuddy/plugins/<id>/
// - 每个插件目录包含 plugin.json（元信息）+ 入口脚本/资源
// - 启用/禁用通过 .disabled 标记

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::AppHandle;
use tauri::Manager;

const PLUGINS_DIR: &str = ".codebuddy/plugins";

/// 插件元信息（写入 plugin.json）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMeta {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub category: String,
    pub entry: Option<String>,
    pub contributes: Option<Vec<String>>,
}

/// 已安装插件（返回前端）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPlugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub category: String,
    pub enabled: bool,
    pub installed_at: Option<String>,
    pub entry: Option<String>,
    pub contributes: Option<Vec<String>>,
}

fn plugins_dir(app: &AppHandle) -> PathBuf {
    let resource = app
        .path()
        .resource_dir()
        .unwrap_or_else(|_| PathBuf::from("."));
    // 回到项目根目录
    let base = resource
        .ancestors()
        .find(|p| p.join(PLUGINS_DIR).exists())
        .unwrap_or(&resource);
    let dir = base.join(PLUGINS_DIR);
    fs::create_dir_all(&dir).ok();
    dir
}

fn is_disabled(plugin_dir: &PathBuf) -> bool {
    plugin_dir.join(".disabled").exists()
}

fn set_disabled(plugin_dir: &PathBuf, disabled: bool) -> std::io::Result<()> {
    let path = plugin_dir.join(".disabled");
    if disabled {
        fs::write(&path, "")?;
    } else if path.exists() {
        fs::remove_file(&path)?;
    }
    Ok(())
}

fn read_plugin_meta(dir: &PathBuf) -> Option<PluginMeta> {
    let meta_path = dir.join("plugin.json");
    let content = fs::read_to_string(&meta_path).ok()?;
    serde_json::from_str::<PluginMeta>(&content).ok()
}

fn format_timestamp(secs: u64) -> String {
    use std::time::SystemTime;
    if let Ok(dur) = SystemTime::UNIX_EPOCH.elapsed() {
        let delta = dur.as_secs() - secs;
        if delta < 60 {
            format!("{}秒前", delta)
        } else if delta < 3600 {
            format!("{}分钟前", delta / 60)
        } else if delta < 86400 {
            format!("{}小时前", delta / 3600)
        } else {
            format!("{}天前", delta / 86400)
        }
    } else {
        String::new()
    }
}

/// 列出已安装的插件
#[tauri::command]
pub fn plugins_list(app: AppHandle) -> Result<Vec<InstalledPlugin>, String> {
    let dir = plugins_dir(&app);
    let mut result = Vec::new();

    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let plugin_json = path.join("plugin.json");
            if !plugin_json.exists() {
                continue;
            }
            let meta = match read_plugin_meta(&path) {
                Some(m) => m,
                None => continue,
            };

            let disabled = is_disabled(&path);

            let installed_at = plugin_json
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| format_timestamp(d.as_secs()));

            result.push(InstalledPlugin {
                id: meta.id,
                name: meta.name,
                version: meta.version,
                author: meta.author,
                description: meta.description,
                category: meta.category,
                enabled: !disabled,
                installed_at,
                entry: meta.entry,
                contributes: meta.contributes,
            });
        }
    }

    result.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(result)
}

/// 安装插件（写入 plugin.json 到本地目录）
#[tauri::command]
pub fn plugins_install(app: AppHandle, plugin_id: String) -> Result<(), String> {
    let base = plugins_dir(&app);
    let plugin_dir = base.join(&plugin_id);
    fs::create_dir_all(&plugin_dir).map_err(|e| e.to_string())?;

    // 写入最小 plugin.json（后续可从远程下载完整配置）
    let meta = PluginMeta {
        id: plugin_id.clone(),
        name: plugin_id.clone(),
        version: "0.1.0".into(),
        author: "Unknown".into(),
        description: format!("Plugin: {}", plugin_id),
        category: "other".into(),
        entry: None,
        contributes: None,
    };

    let json = serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?;
    fs::write(plugin_dir.join("plugin.json"), json).map_err(|e| e.to_string())?;

    Ok(())
}

/// 卸载插件
#[tauri::command]
pub fn plugins_delete(app: AppHandle, plugin_id: String) -> Result<(), String> {
    let base = plugins_dir(&app);
    let plugin_dir = base.join(&plugin_id);
    if plugin_dir.exists() {
        fs::remove_dir_all(&plugin_dir).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 启用/禁用插件
#[tauri::command]
pub fn plugins_toggle(app: AppHandle, plugin_id: String, enabled: bool) -> Result<(), String> {
    let base = plugins_dir(&app);
    let plugin_dir = base.join(&plugin_id);
    if !plugin_dir.exists() {
        return Err(format!("Plugin '{}' not found", plugin_id));
    }
    set_disabled(&plugin_dir, !enabled).map_err(|e| e.to_string())
}
