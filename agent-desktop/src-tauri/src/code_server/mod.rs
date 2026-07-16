//! Code Server 管理模块
//!
//! 管理 VS Code Server（code-server）的生命周期：
//! - code-server 随应用打包（Tauri resources），Node.js 方式运行
//! - 应用启动时后台热备（hot standby），点击 IDE 秒开
//! - 完整 VS Code 体验，100% 插件兼容
//! - 启动时自动注入 Votek Companion 扩展的环境变量，用于 agent ↔ IDE 桥接
//!
//! code-server 是 Coder 公司开源的 VS Code Web 版（MIT 协议）
//! GitHub: https://github.com/coder/code-server

pub mod commands;
pub mod paths;
pub mod process;
pub mod theme;
pub mod types;

pub use commands::*;
#[allow(unused_imports)]
pub use types::*;

use std::process::Child;

// ─── 模块级常量（单一修改点） ─────────────────────

/// 默认监听端口
pub(crate) const CS_DEFAULT_PORT: u16 = 8443;
/// 端口冲突时最大重试次数
pub(crate) const CS_PORT_MAX_ATTEMPTS: u16 = 10;
/// 健康检查 HTTP 客户端超时（秒）
pub(crate) const CS_HEALTH_TIMEOUT_SECS: u64 = 2;
/// 健康检查轮询间隔（毫秒）
pub(crate) const CS_HEALTH_POLL_MS: u64 = 500;
/// start_background 等待就绪超时（秒）
pub(crate) const CS_BG_READY_TIMEOUT_SECS: u64 = 30;
/// code_server_start（用户手动触发）等待就绪超时（秒）
pub(crate) const CS_MANUAL_READY_TIMEOUT_SECS: u64 = 15;
/// code_server_open_ide_window 启动后等待时间（毫秒）
pub(crate) const CS_OPEN_IDE_WAIT_MS: u64 = 1500;
/// IDE 窗口默认尺寸 (宽, 高)
pub(crate) const CS_WINDOW_SIZE: (f64, f64) = (1200.0, 800.0);
/// IDE 窗口最小尺寸 (宽, 高)
pub(crate) const CS_WINDOW_MIN_SIZE: (f64, f64) = (800.0, 500.0);
/// 日志读取行数
pub(crate) const CS_LOG_LINES: usize = 50;
/// 日志错误展示行数
pub(crate) const CS_LOG_ERROR_LINES: usize = 10;
/// IDE 窗口标签（Tauri window label）
pub(crate) const CS_WINDOW_LABEL: &str = "ide";

/// 构建 code-server 访问 URL（统一格式，改一处全局生效）
pub(crate) fn format_cs_url(port: u16) -> String {
    format!("http://127.0.0.1:{}", port)
}

/// 去除 Windows verbatim 路径前缀 `\\?\`。
///
/// Windows `canonicalize()` 返回 `\\?\C:\...` 格式，Node.js 无法识别
/// （EISDIR on 'C:'）。此函数安全地剥离该前缀，非 Windows 原样返回。
pub(crate) fn strip_verbatim_prefix(path: &str) -> String {
    #[cfg(windows)]
    {
        path.strip_prefix(r"\\?\").unwrap_or(path).to_string()
    }
    #[cfg(not(windows))]
    {
        path.to_string()
    }
}

// ─── 全局错误状态 ──────────────────────────────────

/// 记录最近一次错误（供 code_server_status 查询返回给前端）
pub(crate) static CS_LAST_ERROR: tokio::sync::Mutex<Option<String>> =
    tokio::sync::Mutex::const_new(None);

/// 设置全局错误状态
pub(crate) async fn set_last_error(msg: impl Into<String>) {
    let msg = msg.into();
    eprintln!("[CodeServer] ERROR: {}", msg);
    *CS_LAST_ERROR.lock().await = Some(msg);
}

/// 清除全局错误状态
pub(crate) async fn clear_last_error() {
    *CS_LAST_ERROR.lock().await = None;
}

// ─── 全局状态 ──────────────────────────────────────

pub(crate) static CS_PROCESS: tokio::sync::Mutex<Option<Child>> =
    tokio::sync::Mutex::const_new(None);
pub(crate) static CS_PORT: tokio::sync::Mutex<u16> =
    tokio::sync::Mutex::const_new(CS_DEFAULT_PORT);
pub(crate) static CS_WORKSPACE: tokio::sync::Mutex<String> =
    tokio::sync::Mutex::const_new(String::new());
