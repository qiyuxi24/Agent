//! Tauri 命令
//!
//! 对外暴露给前端的 Tauri 命令，以及 `start_background`（应用启动时自动调用）
//! 和 `shutdown`（应用退出时清理）。

use crate::code_server::paths::{cs_entry_js, cs_logs_dir, default_workspace};
use crate::code_server::process::{
    ensure_process_alive, find_available_port, read_last_log_lines, spawn_code_server,
    verify_code_server, wait_for_code_server,
};
use crate::code_server::theme::{cs_theme_name, cs_url_with_theme, write_color_theme};
use crate::code_server::{
    clear_last_error, format_cs_url, set_last_error, types, CS_BG_READY_TIMEOUT_SECS,
    CS_DEFAULT_PORT, CS_LAST_ERROR, CS_LOG_ERROR_LINES, CS_LOG_LINES,
    CS_MANUAL_READY_TIMEOUT_SECS, CS_OPEN_IDE_WAIT_MS, CS_PORT, CS_PORT_MAX_ATTEMPTS,
    CS_PROCESS, CS_WINDOW_LABEL, CS_WINDOW_MIN_SIZE, CS_WINDOW_SIZE, CS_WORKSPACE,
};
use std::process::Command as StdCommand;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

// ─── 查询类命令 ────────────────────────────────────

/// 检查 code-server 是否可用（二进制已随应用打包）
#[tauri::command]
pub async fn code_server_is_installed(app: AppHandle) -> Result<bool, String> {
    Ok(cs_entry_js(&app).exists())
}

/// 检查运行状态
#[tauri::command]
pub async fn code_server_status(app: AppHandle) -> Result<types::CodeServerStatus, String> {
    let installed = cs_entry_js(&app).exists();
    let running = ensure_process_alive().await;
    let port = *CS_PORT.lock().await;
    let ws = CS_WORKSPACE.lock().await.clone();
    let error = CS_LAST_ERROR.lock().await.clone();
    Ok(types::CodeServerStatus {
        installed,
        running,
        port,
        workspace: ws,
        url: if running {
            format_cs_url(port)
        } else {
            String::new()
        },
        version: String::new(),
        error,
    })
}

/// 读取 code-server 日志（最后 N 行），供前端诊断
#[tauri::command]
pub async fn code_server_read_logs(app: AppHandle) -> Result<String, String> {
    let log = cs_logs_dir(&app).join("server.log");
    if !log.exists() {
        return Ok("(尚无日志文件 — code-server 可能尚未启动)".to_string());
    }
    std::fs::read_to_string(&log)
        .map(|content| {
            let lines: Vec<&str> = content.lines().collect();
            if lines.len() > CS_LOG_LINES {
                lines[lines.len() - CS_LOG_LINES..].join("\n")
            } else {
                content
            }
        })
        .map_err(|e| format!("读取日志失败: {}", e))
}

/// 验证/初始化 code-server 环境
#[tauri::command]
pub async fn code_server_install(app: AppHandle) -> Result<String, String> {
    verify_code_server(&app).map(|v| {
        if v.is_empty() {
            "ready".to_string()
        } else {
            format!("ready (v{})", v)
        }
    })
}

// ─── 生命周期 ──────────────────────────────────────

/// 应用启动时后台启动 code-server（不阻塞启动流程）
pub async fn start_background(app: &AppHandle) {
    // 检查是否已运行（含存活检测：如果进程已死，自动清理并重启）
    if ensure_process_alive().await {
        eprintln!("[CodeServer] 已在运行，跳过");
        return;
    }

    // 预检：entry.js 是否存在
    let entry = cs_entry_js(app);
    if !entry.exists() {
        let msg = format!(
            "Code Server 未安装。入口文件不存在: {}\n请运行: npm run download:code-server",
            entry.display()
        );
        set_last_error(&msg).await;
        let _ = app.emit(
            "ide-ready",
            types::IdeReadyEvent {
                url: String::new(),
                port: *CS_PORT.lock().await,
                error: Some(msg),
            },
        );
        return;
    }

    // 预检：node 是否可用
    let node_ok = StdCommand::new("node")
        .creation_flags(0x08000000)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !node_ok {
        let msg = "未找到 Node.js。请安装 Node.js 22.x: https://nodejs.org/".to_string();
        set_last_error(&msg).await;
        let _ = app.emit(
            "ide-ready",
            types::IdeReadyEvent {
                url: String::new(),
                port: *CS_PORT.lock().await,
                error: Some(msg),
            },
        );
        return;
    }

    let port = *CS_PORT.lock().await;
    // 端口冲突检测：自动寻找可用端口
    let port = match find_available_port(port, CS_PORT_MAX_ATTEMPTS) {
        Some(p) => {
            if p != port {
                eprintln!("[CodeServer] 端口 {} 被占用，自动切换至 {}", port, p);
            }
            p
        }
        None => {
            let msg = format!(
                "端口 {}-{} 均被占用，无法启动 Code Server",
                CS_DEFAULT_PORT,
                CS_DEFAULT_PORT + CS_PORT_MAX_ATTEMPTS
            );
            set_last_error(&msg).await;
            let _ = app.emit(
                "ide-ready",
                types::IdeReadyEvent {
                    url: String::new(),
                    port,
                    error: Some(msg),
                },
            );
            return;
        }
    };
    let workspace = default_workspace(app);

    let child = match spawn_code_server(app, &workspace, port, "Default Dark+") {
        Ok(c) => c,
        Err(e) => {
            let full_msg = format!(
                "启动失败: {}\n  入口: {}\n  工作区: {}",
                e,
                entry.display(),
                workspace
            );
            set_last_error(&full_msg).await;
            let _ = app.emit(
                "ide-ready",
                types::IdeReadyEvent {
                    url: String::new(),
                    port,
                    error: Some(full_msg),
                },
            );
            return;
        }
    };

    *CS_PROCESS.lock().await = Some(child);
    *CS_WORKSPACE.lock().await = workspace;
    *CS_PORT.lock().await = port;
    eprintln!("[CodeServer] 后台启动中... 端口 {}", port);

    // 轮询等待就绪（在后台任务中，不阻塞 setup）
    let app_handle = app.clone();
    tokio::spawn(async move {
        let url = format_cs_url(port);
        let ready = wait_for_code_server(port, CS_BG_READY_TIMEOUT_SECS).await;

        if ready {
            clear_last_error().await;
            let _ = app_handle.emit(
                "ide-ready",
                types::IdeReadyEvent {
                    url: url.clone(),
                    port,
                    error: None,
                },
            );
            eprintln!("[CodeServer] 热备就绪: {}", url);
        } else {
            let alive = ensure_process_alive().await;
            let tail = read_last_log_lines(&app_handle, CS_LOG_ERROR_LINES);
            let log_path = cs_logs_dir(&app_handle).join("server.log");
            let msg = if alive {
                format!(
                    "启动超时 ({}s)。\n日志: {}\n最近输出:\n{}",
                    CS_BG_READY_TIMEOUT_SECS,
                    log_path.display(),
                    tail
                )
            } else {
                format!(
                    "进程已退出。\n日志: {}\n最近输出:\n{}",
                    log_path.display(),
                    tail
                )
            };
            set_last_error(&msg).await;
            eprintln!("[CodeServer] {}", msg);
            let _ = app_handle.emit(
                "ide-ready",
                types::IdeReadyEvent {
                    url: String::new(),
                    port,
                    error: Some(msg),
                },
            );
        }
    });
}

/// 启动 code-server（用户手动触发）
#[tauri::command]
pub async fn code_server_start(
    app: AppHandle,
    workspace: Option<String>,
    port: Option<u16>,
) -> Result<types::CodeServerStatus, String> {
    // 已运行（含存活检测：死进程自动清理后走重启逻辑）
    if ensure_process_alive().await {
        let p = *CS_PORT.lock().await;
        let w = CS_WORKSPACE.lock().await.clone();
        clear_last_error().await;
        return Ok(types::CodeServerStatus {
            installed: cs_entry_js(&app).exists(),
            running: true,
            port: p,
            workspace: w,
            url: format_cs_url(p),
            version: String::new(),
            error: None,
        });
    }

    let use_port = port.unwrap_or(CS_DEFAULT_PORT);
    let ws = workspace.unwrap_or_else(|| default_workspace(&app));

    if !std::path::Path::new(&ws).exists() {
        let msg = format!("工作区路径不存在: {}", ws);
        set_last_error(&msg).await;
        return Err(msg);
    }

    // 端口冲突检测
    let use_port = match find_available_port(use_port, CS_PORT_MAX_ATTEMPTS) {
        Some(p) => p,
        None => {
            let msg = format!("端口 {} 及后续 {} 个端口均被占用", use_port, CS_PORT_MAX_ATTEMPTS);
            set_last_error(&msg).await;
            return Err(msg);
        }
    };

    let child = match spawn_code_server(&app, &ws, use_port, "Default Dark+") {
        Ok(c) => c,
        Err(e) => {
            set_last_error(&e).await;
            return Err(e);
        }
    };

    *CS_PROCESS.lock().await = Some(child);
    *CS_PORT.lock().await = use_port;
    *CS_WORKSPACE.lock().await = ws.clone();

    // 等待就绪
    let url = format_cs_url(use_port);
    if !wait_for_code_server(use_port, CS_MANUAL_READY_TIMEOUT_SECS).await {
        let alive = ensure_process_alive().await;
        let logs = read_last_log_lines(&app, CS_LOG_ERROR_LINES);
        let msg = if alive {
            format!("Code Server 启动超时。\n最近日志:\n{}", logs)
        } else {
            format!("Code Server 进程已退出。\n最近日志:\n{}", logs)
        };
        set_last_error(&msg).await;
        return Err(msg);
    }

    clear_last_error().await;
    Ok(types::CodeServerStatus {
        installed: cs_entry_js(&app).exists(),
        running: true,
        port: use_port,
        workspace: ws,
        url,
        version: String::new(),
        error: None,
    })
}

// ─── 窗口管理 ──────────────────────────────────────

/// 打开 IDE 新窗口 — 前端点击 IDE 直接调用此命令
///
/// `theme`: Votek 主题（"dark"/"light"），前端解析 system 后传入，用于同步 code-server 主题
#[tauri::command]
pub async fn code_server_open_ide_window(
    app: AppHandle,
    theme: Option<String>,
) -> Result<(), String> {
    let (alive, port) = {
        let running = ensure_process_alive().await;
        let port = *CS_PORT.lock().await;
        (running, port)
    };

    if !alive {
        // 还没起来或已死，尝试启动
        start_background(&app).await;

        // 给 code-server 一点时间初始化
        tokio::time::sleep(Duration::from_millis(CS_OPEN_IDE_WAIT_MS)).await;

        // 再次检查是否已存活
        if !ensure_process_alive().await {
            let logs = read_last_log_lines(&app, CS_LOG_ERROR_LINES);
            let msg = format!(
                "Code Server 启动失败。请确认 Node.js 已安装。\n\n最近日志:\n{}",
                if logs.is_empty() { "(无输出)" } else { &logs }
            );
            set_last_error(&msg).await;
            return Err(msg);
        }
    }

    // 根据传入主题构建 URL，可选包含 ?theme= 参数
    let theme_name = theme.as_deref().map(cs_theme_name);
    let url = cs_url_with_theme(port, theme_name);

    // 检查是否已有 IDE 窗口
    if let Some(window) = app.get_webview_window(CS_WINDOW_LABEL) {
        // 已有窗口 → 聚焦并刷新
        let _ = window.eval(&format!("window.location.href = '{}'", url));
        let _ = window.set_focus();
        let _ = window.show();
        return Ok(());
    }

    // 创建新窗口
    let ws = CS_WORKSPACE.lock().await.clone();
    let title = if ws.is_empty() {
        "IDE".to_string()
    } else {
        let name = std::path::Path::new(&ws)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "IDE".to_string());
        format!("{} — IDE", name)
    };

    // 注入自定义标题栏（decorations:false 后无原生标题栏）
    let init_script = include_str!("../../scripts/ide-titlebar.js");

    let _webview = WebviewWindowBuilder::new(
        &app,
        CS_WINDOW_LABEL,
        WebviewUrl::External(url.parse().unwrap()),
    )
    .title(&title)
    .inner_size(CS_WINDOW_SIZE.0, CS_WINDOW_SIZE.1)
    .min_inner_size(CS_WINDOW_MIN_SIZE.0, CS_WINDOW_MIN_SIZE.1)
    .center()
    .visible(true)
    .resizable(true)
    .fullscreen(false)
    .decorations(false)
    .transparent(true)
    .initialization_script(init_script)
    .build()
    .map_err(|e| format!("创建 IDE 窗口失败: {}", e))?;

    Ok(())
}

/// 停止 code-server
#[tauri::command]
pub async fn code_server_stop(app: AppHandle) -> Result<(), String> {
    // 关闭 IDE 窗口（如果开着）
    if let Some(window) = app.get_webview_window(CS_WINDOW_LABEL) {
        let _ = window.close();
    }

    let mut proc = CS_PROCESS.lock().await;
    if let Some(mut child) = proc.take() {
        let _ = child.kill();
    }
    Ok(())
}

// ─── 主题同步 ──────────────────────────────────────

/// 同步 Votek 主题到 code-server
///
/// 1. 更新 User/settings.json 的 workbench.colorTheme
/// 2. 如果 IDE 窗口已打开，重载其 URL 带 ?theme= 参数即时生效
/// 3. 如果 IDE 窗口未打开，仅写 settings.json，下次打开时自动应用
#[tauri::command]
pub async fn code_server_sync_theme(app: AppHandle, theme: String) -> Result<(), String> {
    let theme_name = cs_theme_name(&theme);

    // 1. 更新 settings.json
    write_color_theme(&app, theme_name)?;

    // 2. 如果 IDE 窗口存在，重载 URL 带 ?theme=
    let port = *CS_PORT.lock().await;
    let url = cs_url_with_theme(port, Some(theme_name));

    if let Some(window) = app.get_webview_window(CS_WINDOW_LABEL) {
        let _ = window.eval(&format!("window.location.href = '{}'", url));
        eprintln!(
            "[CodeServer] 主题同步: 已重载 IDE 窗口 (theme={})",
            theme_name
        );
    } else {
        eprintln!(
            "[CodeServer] 主题同步: 已更新 settings.json (theme={})，IDE 未打开",
            theme_name
        );
    }

    Ok(())
}

/// 重启 code-server（停止当前实例并重新启动）
#[tauri::command]
pub async fn code_server_restart(
    app: AppHandle,
    theme: Option<String>,
) -> Result<types::CodeServerStatus, String> {
    eprintln!("[CodeServer] 用户请求重启...");

    // 1. 关闭 IDE 窗口
    if let Some(window) = app.get_webview_window(CS_WINDOW_LABEL) {
        let _ = window.close();
    }

    // 2. 终止当前进程
    {
        let mut proc = CS_PROCESS.lock().await;
        if let Some(mut child) = proc.take() {
            let _ = child.kill();
            let _ = child.wait();
            eprintln!("[CodeServer] 旧进程已终止");
        }
    }

    // 3. 清除错误状态
    clear_last_error().await;

    // 4. 等待端口释放
    tokio::time::sleep(Duration::from_millis(500)).await;

    // 5. 重新启动
    let port = *CS_PORT.lock().await;
    let port = find_available_port(port, CS_PORT_MAX_ATTEMPTS).ok_or_else(|| {
        format!(
            "重启失败：端口 {}-{} 均被占用",
            port,
            port + CS_PORT_MAX_ATTEMPTS
        )
    })?;

    let workspace = CS_WORKSPACE.lock().await.clone();
    let workspace = if workspace.is_empty() {
        default_workspace(&app)
    } else {
        workspace
    };

    let child = spawn_code_server(
        &app,
        &workspace,
        port,
        theme.as_deref().unwrap_or("Default Dark+"),
    )?;
    *CS_PROCESS.lock().await = Some(child);
    *CS_PORT.lock().await = port;
    *CS_WORKSPACE.lock().await = workspace.clone();

    // 6. 等待就绪
    if !wait_for_code_server(port, CS_MANUAL_READY_TIMEOUT_SECS).await {
        let alive = ensure_process_alive().await;
        let logs = read_last_log_lines(&app, CS_LOG_ERROR_LINES);
        let msg = if alive {
            format!("重启后启动超时。\n最近日志:\n{}", logs)
        } else {
            format!("重启后进程已退出。\n最近日志:\n{}", logs)
        };
        set_last_error(&msg).await;
        return Err(msg);
    }

    clear_last_error().await;
    eprintln!("[CodeServer] 重启成功，端口 {}", port);
    Ok(types::CodeServerStatus {
        installed: cs_entry_js(&app).exists(),
        running: true,
        port,
        workspace,
        url: format_cs_url(port),
        version: String::new(),
        error: None,
    })
}

/// 应用退出时清理 code-server 子进程（不关窗口，仅杀进程）
pub async fn shutdown() {
    let mut proc = CS_PROCESS.lock().await;
    if let Some(mut child) = proc.take() {
        eprintln!("[CodeServer] 终止子进程...");
        let _ = child.kill();
        let _ = child.wait();
        eprintln!("[CodeServer] 子进程已终止");
    }
}
