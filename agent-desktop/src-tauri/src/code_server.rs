//! Code Server 管理模块
//!
//! 管理 VS Code Server（code-server）的生命周期：
//! - code-server 随应用打包（Tauri resources），Node.js 方式运行
//! - 应用启动时后台热备（hot standby），点击 IDE 秒开
//! - 点击 IDE 时打开独立 Tauri 窗口，直接加载 code-server URL
//! - 完整 VS Code 体验，100% 插件兼容
//!
//! code-server 是 Coder 公司开源的 VS Code Web 版（MIT 协议）
//! GitHub: https://github.com/coder/code-server

use serde::Serialize;
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command as StdCommand, Stdio};

#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

/// code-server 运行状态
#[derive(Debug, Clone, Serialize)]
pub struct CodeServerStatus {
    pub installed: bool,
    pub running: bool,
    pub port: u16,
    pub workspace: String,
    pub url: String,
    pub version: String,
    /// 最近一次错误信息（启动失败/进程崩溃等），无错误时为 None
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// IDE 就绪事件（后端通知前端 code-server 已可访问）
#[derive(Debug, Clone, Serialize)]
pub struct IdeReadyEvent {
    pub url: String,
    pub port: u16,
    /// 失败原因（url 为空时表示启动失败，此字段含错误详情）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// ─── 模块级常量（单一修改点） ─────────────────────

/// 默认监听端口
const CS_DEFAULT_PORT: u16 = 8443;
/// 端口冲突时最大重试次数
const CS_PORT_MAX_ATTEMPTS: u16 = 10;
/// 健康检查 HTTP 客户端超时（秒）
const CS_HEALTH_TIMEOUT_SECS: u64 = 2;
/// 健康检查轮询间隔（毫秒）
const CS_HEALTH_POLL_MS: u64 = 500;
/// start_background 等待就绪超时（秒）
const CS_BG_READY_TIMEOUT_SECS: u64 = 30;
/// code_server_start（用户手动触发）等待就绪超时（秒）
const CS_MANUAL_READY_TIMEOUT_SECS: u64 = 15;
/// code_server_open_ide_window 启动后等待时间（毫秒）
const CS_OPEN_IDE_WAIT_MS: u64 = 1500;
/// IDE 窗口默认尺寸 (宽, 高)
const CS_WINDOW_SIZE: (f64, f64) = (1200.0, 800.0);
/// IDE 窗口最小尺寸 (宽, 高)
const CS_WINDOW_MIN_SIZE: (f64, f64) = (800.0, 500.0);
/// 日志读取行数
const CS_LOG_LINES: usize = 50;
/// 日志错误展示行数
const CS_LOG_ERROR_LINES: usize = 10;
/// IDE 窗口标签（Tauri window label）
const CS_WINDOW_LABEL: &str = "ide";

/// 构建 code-server 访问 URL（统一格式，改一处全局生效）
fn format_cs_url(port: u16) -> String {
    format!("http://127.0.0.1:{}", port)
}

/// 去除 Windows verbatim 路径前缀 `\\?\`。
///
/// Windows `canonicalize()` 返回 `\\?\C:\...` 格式，Node.js 无法识别
/// （EISDIR on 'C:'）。此函数安全地剥离该前缀，非 Windows 原样返回。
fn strip_verbatim_prefix(path: &str) -> String {
    #[cfg(windows)]
    {
        path.strip_prefix(r"\\?\").unwrap_or(path).to_string()
    }
    #[cfg(not(windows))]
    {
        path.to_string()
    }
}

/// 记录最近一次错误（供 code_server_status 查询返回给前端）
static CS_LAST_ERROR: tokio::sync::Mutex<Option<String>> = tokio::sync::Mutex::const_new(None);

/// 设置全局错误状态
async fn set_last_error(msg: impl Into<String>) {
    let msg = msg.into();
    eprintln!("[CodeServer] ERROR: {}", msg);
    *CS_LAST_ERROR.lock().await = Some(msg);
}

/// 清除全局错误状态
async fn clear_last_error() {
    *CS_LAST_ERROR.lock().await = None;
}

// ─── 全局状态 ───────────────────────────────────────────

static CS_PROCESS: tokio::sync::Mutex<Option<Child>> = tokio::sync::Mutex::const_new(None);
static CS_PORT: tokio::sync::Mutex<u16> = tokio::sync::Mutex::const_new(CS_DEFAULT_PORT);
static CS_WORKSPACE: tokio::sync::Mutex<String> = tokio::sync::Mutex::const_new(String::new());

// ─── 路径 / 工具 ──────────────────────────────────────

fn cs_data_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_default()
        .join("code-server")
        .join("user-data")
}

fn cs_logs_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_default()
        .join("code-server")
        .join("logs")
}

fn default_workspace() -> String {
    std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| {
            dirs_next::document_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| ".".to_string())
        })
}

/// 解析 code-server release 目录
/// 优先级：Tauri resource_dir（生产安装包） > exe 同级（备用） > CARGO_MANIFEST_DIR（开发）
fn cs_release_dir(app: &AppHandle) -> PathBuf {
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

fn cs_entry_js(app: &AppHandle) -> PathBuf {
    cs_release_dir(app).join("out").join("node").join("entry.js")
}

/// 检查端口是否可用（尝试绑定）
fn is_port_available(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

/// 检查全局进程是否存活。持有锁期间调用 try_wait，若已退出则自动丢弃句柄。
///
/// 返回 true 表示进程正在运行。
async fn ensure_process_alive() -> bool {
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
fn find_available_port(start_port: u16, max_attempts: u16) -> Option<u16> {
    for offset in 0..max_attempts {
        let port = start_port + offset;
        if is_port_available(port) {
            return Some(port);
        }
    }
    None
}

/// 等待 code-server 真正可访问（HTTP GET 200，绑定回环地址无安全风险）
async fn wait_for_code_server(port: u16, timeout_secs: u64) -> bool {
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
        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => return true,
            Ok(resp) => {
                eprintln!("[CodeServer] 健康检查 HTTP {}", resp.status());
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
fn verify_code_server(app: &AppHandle) -> Result<String, String> {
    let entry = cs_entry_js(app);
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
fn read_last_log_lines(app: &AppHandle, n: usize) -> String {
    let log = cs_logs_dir(app).join("server.log");
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
fn spawn_code_server(app: &AppHandle, workspace: &str, port: u16) -> Result<Child, String> {
    let entry = cs_entry_js(app);
    let data_dir = cs_data_dir(app);
    let logs_dir = cs_logs_dir(app);

    std::fs::create_dir_all(&data_dir).map_err(|e| format!("创建数据目录失败: {}", e))?;
    std::fs::create_dir_all(&logs_dir).map_err(|e| format!("创建日志目录失败: {}", e))?;

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
    // Windows canonicalize() 会加上 \\?\ 前缀，Node.js v24 无法识别（EISDIR on 'C:'）
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

    StdCommand::new("node")
        .creation_flags(0x08000000)
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
        .arg(workspace)
        .stdout(Stdio::from(log_file.try_clone().map_err(|e| format!("克隆日志句柄失败: {}", e))?))
        .stderr(Stdio::from(log_file))
        .spawn()
        .map_err(|e| format!("启动 Code Server 失败: {}", e))
}

// ─── Tauri 命令 ─────────────────────────────────────────

/// 检查 code-server 是否可用（二进制已随应用打包）
#[tauri::command]
pub async fn code_server_is_installed(app: AppHandle) -> Result<bool, String> {
    Ok(cs_entry_js(&app).exists())
}

/// 检查运行状态
#[tauri::command]
pub async fn code_server_status(app: AppHandle) -> Result<CodeServerStatus, String> {
    let installed = cs_entry_js(&app).exists();
    let running = ensure_process_alive().await;
    let port = *CS_PORT.lock().await;
    let ws = CS_WORKSPACE.lock().await.clone();
    let error = CS_LAST_ERROR.lock().await.clone();
    Ok(CodeServerStatus {
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

/// 读取 code-server 日志（最后 N 行，由 CS_LOG_LINES 控制），供前端诊断
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
            IdeReadyEvent {
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
            IdeReadyEvent {
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
                IdeReadyEvent {
                    url: String::new(),
                    port,
                    error: Some(msg),
                },
            );
            return;
        }
    };
    let workspace = default_workspace();

    let child = match spawn_code_server(app, &workspace, port) {
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
                IdeReadyEvent {
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
                IdeReadyEvent {
                    url: url.clone(),
                    port,
                    error: None,
                },
            );
            eprintln!("[CodeServer] 热备就绪: {}", url);
        } else {
            let tail = read_last_log_lines(&app_handle, CS_LOG_ERROR_LINES);
            let log_path = cs_logs_dir(&app_handle).join("server.log");
            let msg = format!(
                "启动超时 ({}s)。\n日志: {}\n最近输出:\n{}",
                CS_BG_READY_TIMEOUT_SECS,
                log_path.display(),
                tail
            );
            set_last_error(&msg).await;
            eprintln!("[CodeServer] {}", msg);
            let _ = app_handle.emit(
                "ide-ready",
                IdeReadyEvent {
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
) -> Result<CodeServerStatus, String> {
    // 已运行（含存活检测：死进程自动清理后走重启逻辑）
    if ensure_process_alive().await {
        let p = *CS_PORT.lock().await;
        let w = CS_WORKSPACE.lock().await.clone();
        clear_last_error().await;
        return Ok(CodeServerStatus {
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
    let ws = workspace.unwrap_or_else(default_workspace);

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

    let child = match spawn_code_server(&app, &ws, use_port) {
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
        let logs = read_last_log_lines(&app, CS_LOG_ERROR_LINES);
        let msg = format!("Code Server 启动超时。\n最近日志:\n{}", logs);
        set_last_error(&msg).await;
        return Err(msg);
    }

    clear_last_error().await;
    Ok(CodeServerStatus {
        installed: cs_entry_js(&app).exists(),
        running: true,
        port: use_port,
        workspace: ws,
        url,
        version: String::new(),
        error: None,
    })
}

/// 打开 IDE 新窗口 — 前端点击 IDE 直接调用此命令
#[tauri::command]
pub async fn code_server_open_ide_window(app: AppHandle) -> Result<(), String> {
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

    let url = format_cs_url(port);

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

    // 轻量品牌初始化脚本：确保页面标题一致（product.json 已处理大部分命名）
    let init_script = r#"
(function() {
    // 轮询修正页面标题（VS Code 动态加载后可能覆盖 title）
    var attempts = 0;
    var fixTitle = function() {
        if (document.title && document.title.indexOf('Votek') === -1 && document.title.indexOf('code-server') !== -1) {
            document.title = document.title.replace(/code-server/gi, 'Votek');
        }
        // 也修正可能的 "Code - " 前缀
        if (document.title && document.title.startsWith('Code - ')) {
            document.title = document.title.replace('Code - ', '');
        }
        if (++attempts < 30) setTimeout(fixTitle, 1000);
    };
    setTimeout(fixTitle, 500);
})();
"#;

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

/// 重启 code-server（停止当前实例并重新启动）
#[tauri::command]
pub async fn code_server_restart(app: AppHandle) -> Result<CodeServerStatus, String> {
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
        format!("重启失败：端口 {}-{} 均被占用", port, port + CS_PORT_MAX_ATTEMPTS)
    })?;

    let workspace = CS_WORKSPACE.lock().await.clone();
    let workspace = if workspace.is_empty() {
        default_workspace()
    } else {
        workspace
    };

    let child = spawn_code_server(&app, &workspace, port)?;
    *CS_PROCESS.lock().await = Some(child);
    *CS_PORT.lock().await = port;
    *CS_WORKSPACE.lock().await = workspace.clone();

    // 6. 等待就绪
    if !wait_for_code_server(port, CS_MANUAL_READY_TIMEOUT_SECS).await {
        let logs = read_last_log_lines(&app, CS_LOG_ERROR_LINES);
        let msg = format!("重启后启动超时。\n最近日志:\n{}", logs);
        set_last_error(&msg).await;
        return Err(msg);
    }

    clear_last_error().await;
    eprintln!("[CodeServer] 重启成功，端口 {}", port);
    Ok(CodeServerStatus {
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
