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
}

/// IDE 就绪事件（后端通知前端 code-server 已可访问）
#[derive(Debug, Clone, Serialize)]
pub struct IdeReadyEvent {
    pub url: String,
    pub port: u16,
}

// ─── 全局状态 ───────────────────────────────────────────

static CS_PROCESS: tokio::sync::Mutex<Option<Child>> = tokio::sync::Mutex::const_new(None);
static CS_PORT: tokio::sync::Mutex<u16> = tokio::sync::Mutex::const_new(8443);
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
        .timeout(Duration::from_secs(2))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    let url = format!("http://127.0.0.1:{}/", port);
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
        tokio::time::sleep(Duration::from_millis(500)).await;
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
    // 因此必须去掉此前缀
    let entry_raw = entry_abs.to_string_lossy().to_string();
    let entry_str = entry_raw
        .strip_prefix(r"\\?\")
        .unwrap_or(&entry_raw)
        .to_string();

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
pub async fn code_server_status() -> Result<CodeServerStatus, String> {
    let running = CS_PROCESS.lock().await.is_some();
    let port = *CS_PORT.lock().await;
    let ws = CS_WORKSPACE.lock().await.clone();
    Ok(CodeServerStatus {
        installed: true,
        running,
        port,
        workspace: ws,
        url: if running {
            format!("http://127.0.0.1:{}", port)
        } else {
            String::new()
        },
        version: String::new(),
    })
}

/// 读取 code-server 日志（最后 50 行），供前端诊断
#[tauri::command]
pub async fn code_server_read_logs(app: AppHandle) -> Result<String, String> {
    let log = cs_logs_dir(&app).join("server.log");
    if !log.exists() {
        return Ok("(尚无日志文件 — code-server 可能尚未启动)".to_string());
    }
    std::fs::read_to_string(&log)
        .map(|content| {
            let lines: Vec<&str> = content.lines().collect();
            let n = 50usize;
            if lines.len() > n {
                lines[lines.len() - n..].join("\n")
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

    // 预检：node 是否可用
    let node_ok = StdCommand::new("node")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !node_ok {
        eprintln!("[CodeServer] 未找到 Node.js，请安装 Node.js 后重试");
        eprintln!("[CodeServer] 下载: https://nodejs.org/");
        return;
    }

    let port = *CS_PORT.lock().await;
    // 端口冲突检测：自动寻找可用端口
    let port = match find_available_port(port, 10) {
        Some(p) => {
            if p != port {
                eprintln!("[CodeServer] 端口 {} 被占用，自动切换至 {}", port, p);
            }
            p
        }
        None => {
            eprintln!("[CodeServer] 8443-8452 端口均被占用，放弃启动");
            return;
        }
    };
    let workspace = default_workspace();

    let child = match spawn_code_server(app, &workspace, port) {
        Ok(c) => c,
        Err(e) => {
            let entry = cs_entry_js(app);
            eprintln!(
                "[CodeServer] 启动失败: {}\n  入口: {}\n  工作区: {}\n  请确认 Node.js 已安装且 entry.js 路径正确",
                e,
                entry.display(),
                workspace
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
        let url = format!("http://127.0.0.1:{}", port);
        let ready = wait_for_code_server(port, 30).await;

        if ready {
            let _ = app_handle.emit(
                "ide-ready",
                IdeReadyEvent {
                    url: url.clone(),
                    port,
                },
            );
            eprintln!("[CodeServer] 热备就绪: {}", url);
        } else {
            let _ = app_handle.emit(
                "ide-ready",
                IdeReadyEvent {
                    url: String::new(),
                    port,
                },
            );
            let log_path = cs_logs_dir(&app_handle).join("server.log");
            let tail = read_last_log_lines(&app_handle, 10);
            eprintln!(
                "[CodeServer] 启动超时 (30s)。\n日志: {}\n最近输出:\n{}",
                log_path.display(),
                tail
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
        return Ok(CodeServerStatus {
            installed: true,
            running: true,
            port: p,
            workspace: w,
            url: format!("http://127.0.0.1:{}", p),
            version: String::new(),
        });
    }

    let use_port = port.unwrap_or(8443);
    let ws = workspace.unwrap_or_else(default_workspace);

    if !std::path::Path::new(&ws).exists() {
        return Err(format!("工作区路径不存在: {}", ws));
    }

    // 端口冲突检测
    let use_port = find_available_port(use_port, 10)
        .ok_or_else(|| format!("端口 {} 及后续 9 个端口均被占用", use_port))?;

    let child = spawn_code_server(&app, &ws, use_port)?;

    *CS_PROCESS.lock().await = Some(child);
    *CS_PORT.lock().await = use_port;
    *CS_WORKSPACE.lock().await = ws.clone();

    // 等待就绪
    let url = format!("http://127.0.0.1:{}", use_port);
    if !wait_for_code_server(use_port, 15).await {
        let logs = read_last_log_lines(&app, 10);
        return Err(format!(
            "Code Server 启动超时。\n最近日志:\n{}",
            logs
        ));
    }

    Ok(CodeServerStatus {
        installed: true,
        running: true,
        port: use_port,
        workspace: ws,
        url,
        version: String::new(),
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
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

        // 再次检查是否已存活
        if !ensure_process_alive().await {
            let logs = read_last_log_lines(&app, 10);
            return Err(format!(
                "Code Server 启动失败。请确认 Node.js 已安装。\n\n最近日志:\n{}",
                if logs.is_empty() { "(无输出)" } else { &logs }
            ));
        }
    }

    let url = format!("http://127.0.0.1:{}", port);

    // 检查是否已有 IDE 窗口
    if let Some(window) = app.get_webview_window("ide") {
        // 已有窗口 → 聚焦并刷新
        let _ = window.eval(&format!("window.location.href = '{}'", url));
        let _ = window.set_focus();
        let _ = window.show();
        return Ok(());
    }

    // 创建新窗口
    let ws = CS_WORKSPACE.lock().await.clone();
    let title = if ws.is_empty() {
        "VS Code IDE".to_string()
    } else {
        let name = std::path::Path::new(&ws)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "IDE".to_string());
        format!("{} — VS Code", name)
    };

    let _webview = WebviewWindowBuilder::new(
        &app,
        "ide",
        WebviewUrl::External(url.parse().unwrap()),
    )
    .title(&title)
    .inner_size(1200.0, 800.0)
    .min_inner_size(800.0, 500.0)
    .center()
    .visible(true)
    .resizable(true)
    .fullscreen(false)
    .build()
    .map_err(|e| format!("创建 IDE 窗口失败: {}", e))?;

    Ok(())
}

/// 停止 code-server
#[tauri::command]
pub async fn code_server_stop(app: AppHandle) -> Result<(), String> {
    // 关闭 IDE 窗口（如果开着）
    if let Some(window) = app.get_webview_window("ide") {
        let _ = window.close();
    }

    let mut proc = CS_PROCESS.lock().await;
    if let Some(mut child) = proc.take() {
        let _ = child.kill();
    }
    Ok(())
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
