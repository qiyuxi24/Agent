//! 路径解析与 Companion 扩展安装
//!
//! 集中管理所有 code-server 相关目录/文件路径，确保跨平台一致性。
//! 同时负责 Votek Companion 扩展的自动编译安装。

use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use tauri::{AppHandle, Manager};

// ─── 基础路径 ──────────────────────────────────────

/// 持久化基础目录：优先 Tauri app_data_dir，回退到系统 data_dir（%APPDATA%\Votek），最后用当前目录
pub(crate) fn persistent_base_dir(app: &AppHandle) -> PathBuf {
    let root = app
        .path()
        .app_data_dir()
        .ok()
        .or_else(|| dirs_next::data_dir().map(|d| d.join("Votek")))
        .unwrap_or_else(|| PathBuf::from("."));
    root.join("code-server")
}

/// code-server user data 目录（持久化存储扩展、设置、工作区数据）
pub(crate) fn cs_data_dir(app: &AppHandle) -> PathBuf {
    persistent_base_dir(app).join("user-data")
}

/// code-server 日志目录
pub(crate) fn cs_logs_dir(app: &AppHandle) -> PathBuf {
    persistent_base_dir(app).join("logs")
}

/// code-server 扩展安装目录
pub(crate) fn cs_extensions_dir(app: &AppHandle) -> PathBuf {
    persistent_base_dir(app).join("extensions")
}

/// code-server user data 中的 settings.json 路径
pub(crate) fn cs_user_settings_path(app: &AppHandle) -> PathBuf {
    cs_data_dir(app).join("User").join("settings.json")
}

/// code-server 本地配置（Votek 管理，记录上次工作区等）
pub(crate) fn cs_config_path(app: &AppHandle) -> PathBuf {
    persistent_base_dir(app).join("config.json")
}

/// 解析 code-server release 目录
/// 优先级：Tauri resource_dir（生产安装包） > exe 同级（备用） > CARGO_MANIFEST_DIR（开发）
pub(crate) fn cs_release_dir(app: &AppHandle) -> PathBuf {
    // 1. Tauri 打包资源目录（NSIS 安装后自动解压至此）
    if let Ok(resource_dir) = app.path().resource_dir() {
        let cs = resource_dir
            .join("binaries")
            .join("code-server")
            .join("release");
        if cs.join("out").join("node").join("entry.js").exists() {
            return cs;
        }
    }

    // 2. 当前 exe 同目录（备用：手动部署场景）
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            let prod = exe_dir.join("code-server");
            if prod.join("out").join("node").join("entry.js").exists() {
                return prod;
            }
        }
    }

    // 3. 开发模式：CARGO_MANIFEST_DIR/binaries/code-server/release
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("binaries")
        .join("code-server")
        .join("release")
}

/// code-server 入口脚本路径
pub(crate) fn cs_entry_js(app: &AppHandle) -> PathBuf {
    cs_release_dir(app).join("out").join("node").join("entry.js")
}

/// Votek Companion 扩展在 extensions 目录中的路径（VS Code 格式：publisher.name-version）
pub(crate) fn companion_ext_path(app: &AppHandle) -> PathBuf {
    cs_extensions_dir(app).join("votek.votek-companion-0.1.0")
}

// ─── 持久化配置 ────────────────────────────────────

/// 持久化上次工作区路径到 config.json
pub(crate) fn save_last_workspace(app: &AppHandle, workspace: &str) {
    let path = cs_config_path(app);
    let mut cfg = if let Ok(content) = std::fs::read_to_string(&path) {
        serde_json::from_str::<serde_json::Value>(&content)
            .unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };
    cfg["lastWorkspace"] = serde_json::Value::String(workspace.to_string());
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, cfg.to_string());
}

/// 从 config.json 读取上次工作区
pub(crate) fn read_last_workspace(app: &AppHandle) -> Option<String> {
    let path = cs_config_path(app);
    let content = std::fs::read_to_string(path).ok()?;
    let cfg: serde_json::Value = serde_json::from_str(&content).ok()?;
    cfg["lastWorkspace"].as_str().map(|s| s.to_string())
}

/// 获取默认工作区路径 — 优先使用上次记录的工作区，其次当前目录
pub(crate) fn default_workspace(app: &AppHandle) -> String {
    if let Some(ws) = read_last_workspace(app) {
        if std::path::Path::new(&ws).exists() {
            return ws;
        }
    }
    std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| {
            dirs_next::document_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| ".".to_string())
        })
}

// ─── Companion 扩展安装 ────────────────────────────

/// 确保 Votek Companion 扩展已安装到 code-server 的 extensions 目录。
///
/// 如果扩展尚未安装，自动从源码编译安装。
/// 不会阻塞 code-server 启动失败 — 扩展缺失时 bridge 工具优雅降级。
pub(crate) fn ensure_companion_extension(app: &AppHandle) {
    let ext_path = companion_ext_path(app);
    let ext_js = ext_path.join("out").join("extension.js");

    // 已安装，无需操作
    if ext_js.exists() {
        eprintln!("[CodeServer] Votek Companion 扩展已就绪: {:?}", ext_path);
        return;
    }

    // 解析 companion 源码目录（相对于 crate 目录: ../../votek-companion/）
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let companion_src = manifest_dir
        .parent()       // agent-desktop/
        .and_then(Path::parent) // Agent/
        .map(|p| p.join("votek-companion"));
    let companion_src = match companion_src {
        Some(p) if p.join("package.json").exists() => p,
        _ => {
            eprintln!("[CodeServer] ⚠ 找不到 Votek Companion 源码目录 (votek-companion/), 跳过安装");
            return;
        }
    };

    eprintln!("[CodeServer] 正在编译 Votek Companion 扩展...");

    // Step 1: npm install
    let install_result = StdCommand::new("npm")
        .creation_flags(0x08000000)
        .arg("install")
        .arg("--no-fund")
        .arg("--no-audit")
        .current_dir(&companion_src)
        .output();
    match install_result {
        Ok(out) if out.status.success() => {
            eprintln!("[CodeServer] npm install 完成");
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let short = stderr.lines().take(5).collect::<Vec<_>>().join("; ");
            eprintln!("[CodeServer] ⚠ npm install 警告: {}", short);
        }
        Err(e) => {
            eprintln!("[CodeServer] ⚠ npm install 失败: {} (bridge 工具不可用)", e);
            return;
        }
    }

    // Step 2: npm run compile (tsc)
    let compile_result = StdCommand::new("npx")
        .creation_flags(0x08000000)
        .arg("tsc")
        .arg("-p")
        .arg("./")
        .current_dir(&companion_src)
        .output();
    match compile_result {
        Ok(out) if out.status.success() => {
            eprintln!("[CodeServer] tsc 编译完成");
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let short = stderr.lines().take(5).collect::<Vec<_>>().join("; ");
            eprintln!("[CodeServer] ⚠ tsc 编译有警告: {}", short);
        }
        Err(e) => {
            eprintln!("[CodeServer] ⚠ tsc 编译失败: {} (bridge 工具不可用)", e);
            return;
        }
    }

    // Step 3: 复制到 extensions 目录
    // 需要的文件：package.json + out/*.js + node_modules/（仅 ws 子集）
    let _ = std::fs::create_dir_all(&ext_path);
    let _ = std::fs::create_dir_all(ext_path.join("out"));

    // 复制 package.json
    let src_pkg = companion_src.join("package.json");
    if src_pkg.exists() {
        let _ = std::fs::copy(&src_pkg, ext_path.join("package.json"));
    }

    // 复制 out/*.js
    let src_out = companion_src.join("out");
    if src_out.exists() {
        if let Ok(entries) = std::fs::read_dir(&src_out) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |e| e == "js") {
                    let dest = ext_path.join("out").join(path.file_name().unwrap());
                    let _ = std::fs::copy(&path, &dest);
                }
            }
        }
    }

    // 复制 node_modules（仅 ws 依赖）
    let src_nm = companion_src.join("node_modules");
    let dest_nm = ext_path.join("node_modules");
    if src_nm.exists() {
        let _ = std::fs::create_dir_all(&dest_nm);
        copy_dir_recursive(&src_nm.join("ws"), &dest_nm.join("ws"));
        if src_nm.join("ws").exists() {
            let ws_nm = src_nm.join("ws").join("node_modules");
            if ws_nm.exists() {
                copy_dir_recursive(&ws_nm, &dest_nm);
            }
        }
    }

    eprintln!("[CodeServer] ✅ Votek Companion 扩展已安装到: {:?}", ext_path);
}

/// 递归复制目录内容
fn copy_dir_recursive(src: &Path, dest: &Path) {
    if !src.exists() {
        return;
    }
    let _ = std::fs::create_dir_all(dest);
    if let Ok(entries) = std::fs::read_dir(src) {
        for entry in entries.flatten() {
            let path = entry.path();
            let file_name = match path.file_name() {
                Some(n) => n,
                None => continue,
            };
            let dest_path = dest.join(file_name);
            if path.is_dir() {
                copy_dir_recursive(&path, &dest_path);
            } else {
                let _ = std::fs::copy(&path, &dest_path);
            }
        }
    }
}
