//! 进程管理
//!
//! 负责 code-server 子进程的启动（spawn）、健康检查、端口分配、
//! 以及环境和证书验证。

use crate::code_server::{
    format_cs_url, strip_verbatim_prefix, CS_HEALTH_POLL_MS, CS_HEALTH_TIMEOUT_SECS, CS_PROCESS,
};
use crate::code_server::{paths, theme::write_color_theme};
use crate::vscode_bridge;
use std::net::TcpListener;
use std::process::{Child, Command as StdCommand, Stdio};
use std::time::Duration;
use tauri::AppHandle;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// 检查端口是否可用（尝试绑定）
pub(crate) fn is_port_available(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

/// 检查全局进程是否存活。持有锁期间调用 try_wait，若已退出则自动丢弃句柄。
///
/// 返回 true 表示进程正在运行。
pub(crate) async fn ensure_process_alive() -> bool {
    let mut proc = CS_PROCESS.lock().await;
    if let Some(ref mut child) = *proc {
        match child.try_wait() {
            Ok(Some(status)) => {
                let code = status.code().map_or("signal".into(), |c| c.to_string());
                eprintln!(
                    "[CodeServer] 进程已退出（exit={}），自动清理僵尸句柄，下次调用将重启",
                    code
                );
                *proc = None;
                false
            }
            Ok(None) => true,   // 仍在运行
            Err(e) => {
                eprintln!("[CodeServer] try_wait 失败: {}，假定进程已死", e);
                *proc = None;
                false
            }
        }
    } else {
        false
    }
}

/// 查找可用端口（从 start_port 开始，最多尝试 max_attempts 次）
pub(crate) fn find_available_port(start_port: u16, max_attempts: u16) -> Option<u16> {
    for offset in 0..max_attempts {
        let port = start_port + offset;
        if is_port_available(port) {
            return Some(port);
        }
    }
    None
}

/// 等待 code-server 真正可访问（HTTP GET 200，绑定回环地址无安全风险）
///
/// 健康检查循环中同时检查进程存活状态：
/// - 如果 code-server 进程已退出（崩溃/异常终止），立即返回 false，避免在死进程上白白轮询到超时
/// - 区分"连接被拒绝"（端口尚无人监听，启动中的正常现象）和"连接超时"（已监听但无响应）两类错误
/// - debug 模式下打印所有检查细节，release 模式下静默跳过连接拒绝（避免刷屏）
pub(crate) async fn wait_for_code_server(port: u16, timeout_secs: u64) -> bool {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(CS_HEALTH_TIMEOUT_SECS))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    let url = format_cs_url(port);
    let start = std::time::Instant::now();
    let timeout = Duration::from_secs(timeout_secs);

    while start.elapsed() < timeout {
        // 进程存活检查：如果 code-server 已退出，立即返回失败
        if !ensure_process_alive().await {
            eprintln!("[CodeServer] 健康检查终止：code-server 进程已退出");
            return false;
        }

        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => return true,
            Ok(resp) => {
                eprintln!("[CodeServer] 健康检查 HTTP {}", resp.status());
            }
            Err(e) if e.is_connect() => {
                // debug 模式可见，release 模式静默
                if cfg!(debug_assertions) {
                    eprintln!("[CodeServer] 等待端口 {} 就绪...", port);
                }
            }
            Err(e) if e.is_timeout() => {
                eprintln!(
                    "[CodeServer] 健康检查：端口 {} 连接超时（已监听但无响应）",
                    port
                );
            }
            Err(e) => {
                eprintln!("[CodeServer] 健康检查失败: {}", e);
            }
        }
        tokio::time::sleep(Duration::from_millis(CS_HEALTH_POLL_MS)).await;
    }
    false
}

/// 验证 code-server 目录和 Node.js 是否就绪
pub(crate) fn verify_code_server(app: &AppHandle) -> Result<String, String> {
    let entry = paths::cs_entry_js(app);
    if !entry.exists() {
        return Err(format!(
            "Code Server 入口未找到: {}\n请运行 scripts/download-code-server.ps1",
            entry.display()
        ));
    }

    // 检查 node 是否可用
    StdCommand::new("node")
        .creation_flags(0x08000000)
        .arg("--version")
        .output()
        .map_err(|_| "未找到 Node.js，请安装 Node.js 后重试".to_string())?;

    // 获取 code-server 版本
    let version = StdCommand::new("node")
        .creation_flags(0x08000000)
        .arg(&entry)
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|v| v.trim().to_string())
        .unwrap_or_default();

    Ok(version)
}

/// 读取日志文件最后 n 行，用于启动失败时排查
pub(crate) fn read_last_log_lines(app: &AppHandle, n: usize) -> String {
    let log = paths::cs_logs_dir(app).join("server.log");
    if !log.exists() {
        return "无日志文件".to_string();
    }
    let content = std::fs::read_to_string(&log).unwrap_or_default();
    let lines: Vec<&str> = content.lines().collect();
    lines
        .iter()
        .rev()
        .take(n)
        .rev()
        .copied()
        .collect::<Vec<_>>()
        .join("\n")
}

/// 启动 code-server 进程
///
/// 注意：code-server 使用 `--bind-addr host:port` 而不是 `--port`。
/// 绑定 127.0.0.1 确保只有本机可访问；使用 HTTP（--cert false）避免
/// Tauri Webview 对自签名证书的拦截问题。
pub(crate) fn spawn_code_server(
    app: &AppHandle,
    workspace: &str,
    port: u16,
    theme: &str,
) -> Result<Child, String> {
    let entry = paths::cs_entry_js(app);
    let data_dir = paths::cs_data_dir(app);
    let logs_dir = paths::cs_logs_dir(app);
    let extensions_dir = paths::cs_extensions_dir(app);

    std::fs::create_dir_all(&data_dir).map_err(|e| format!("创建数据目录失败: {}", e))?;
    std::fs::create_dir_all(&logs_dir).map_err(|e| format!("创建日志目录失败: {}", e))?;
    std::fs::create_dir_all(&extensions_dir)
        .map_err(|e| format!("创建扩展目录失败: {}", e))?;

    // 启动前写入 workbench.colorTheme，确保 IDE 打开时使用正确的主题
    let _ = write_color_theme(app, theme);

    // 防御：确保 entry.js 存在
    if !entry.exists() {
        return Err(format!(
            "Code Server 入口文件不存在: {}\n请先运行 scripts/download-code-server.mjs 下载 code-server",
            entry.display()
        ));
    }

    // 确保 entry.js 是绝对路径（Windows 下 node 需要完整路径）
    let entry_abs = entry.canonicalize().map_err(|e| {
        format!(
            "无法解析 Code Server 入口路径: {} ({})",
            entry.display(),
            e
        )
    })?;

    let log_path = logs_dir.join("server.log");
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| format!("打开日志文件失败: {}", e))?;

    let bind_addr = format!("127.0.0.1:{}", port);
    let entry_raw = entry_abs.to_string_lossy().to_string();
    let entry_str = strip_verbatim_prefix(&entry_raw);

    eprintln!(
        "[CodeServer] entry_raw=\"{}\" entry_str=\"{}\"",
        entry_raw, entry_str
    );
    eprintln!(
        "[CodeServer] node \"{}\" --bind-addr {} --auth none ... {}",
        entry_str, bind_addr, workspace
    );

    let mut cmd = StdCommand::new("node");
    cmd.creation_flags(0x08000000)
        .arg(&entry_str)
        .arg("--bind-addr")
        .arg(&bind_addr)
        .arg("--auth")
        .arg("none")
        .arg("--disable-telemetry")
        .arg("--disable-update-check")
        .arg("--disable-workspace-trust")
        .arg("--user-data-dir")
        .arg(data_dir.to_string_lossy().to_string())
        .arg("--extensions-dir")
        .arg(extensions_dir.to_string_lossy().to_string())
        .arg(workspace)
        .stdout(Stdio::from(
            log_file
                .try_clone()
                .map_err(|e| format!("克隆日志句柄失败: {}", e))?,
        ))
        .stderr(Stdio::from(log_file));

    // 持久化：记录本次工作区
    paths::save_last_workspace(app, workspace);

    // 确保 Votek Companion 扩展已安装
    paths::ensure_companion_extension(app);

    // 注入 Votek Companion 桥接环境变量
    let bridge_config = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(vscode_bridge::get_global_config())
    });
    if let Some(ref cfg) = bridge_config {
        vscode_bridge::inject_env(&mut cmd, cfg);
        eprintln!(
            "[CodeServer] Injected bridge env: {}={}",
            vscode_bridge::ENV_BRIDGE_PORT,
            cfg.port
        );
    }

    eprintln!(
        "[CodeServer] --user-data-dir={} --extensions-dir={}",
        data_dir.display(),
        extensions_dir.display()
    );

    cmd.spawn()
        .map_err(|e| format!("启动 Code Server 失败: {}", e))
}
