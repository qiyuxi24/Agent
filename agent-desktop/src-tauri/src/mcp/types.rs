//! MCP 核心类型定义：工具描述、服务器配置、JSON-RPC 结构等。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::path::Path;

/// 单次工具调用的超时时间（秒）
pub(crate) const TOOL_CALL_TIMEOUT_SECS: u64 = 30;
/// MCP 握手（initialize + tools/list）的超时时间（秒）
pub(crate) const HANDSHAKE_TIMEOUT_SECS: u64 = 15;
/// 每行 JSON-RPC 响应读取的超时时间（秒）
pub(crate) const READ_TIMEOUT_SECS: u64 = 10;
/// stderr 环缓冲区最大行数
pub(crate) const STDERR_RING_SIZE: usize = 50;
/// 工具调用缓存 TTL（秒）
pub(crate) const CACHE_TTL_SECS: u64 = 60;
/// 缓存最大条目数（超过则清空）
pub(crate) const CACHE_MAX_ENTRIES: usize = 200;
/// llm_tools 列表缓存 TTL（秒）：工具定义很少变动，缓冲避免每轮对话重建
pub(crate) const TOOLS_CACHE_TTL_SECS: u64 = 30;

/// 单个 MCP 工具的描述（来自 tools/list）
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct McpTool {
    pub name: String,
    pub description: Option<String>,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

/// 用户配置的 MCP Server 连接信息
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    #[serde(default)]
    pub env: Option<HashMap<String, String>>,
}

/// 用于前端展示的 Server 状态（含诊断信息）
#[derive(Debug, Serialize, Clone)]
pub struct McpServerInfo {
    pub name: String,
    pub status: String,
    pub tool_count: usize,
    /// 已解析的实际命令路径
    pub resolved_command: String,
    /// 工具调用错误计数（用于健康度判断）
    pub error_count: usize,
    /// 最近一次错误消息（成功时为 null）
    pub last_error: Option<String>,
}

/// JSON-RPC 2.0 完整错误对象
#[derive(Debug, Deserialize)]
pub(crate) struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(default)]
    pub data: Option<Value>,
}

/// stderr 环缓冲区，用于捕获 MCP Server 进程的调试输出
/// 通过 Arc<StdMutex<>> 在 McpClient 和 drain_stderr 后台任务间共享
pub(crate) struct StderrRing {
    lines: VecDeque<String>,
    capacity: usize,
}

impl StderrRing {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            lines: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub(crate) fn push(&mut self, line: String) {
        if self.lines.len() >= self.capacity {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
    }

    pub(crate) fn snapshot(&self) -> Vec<String> {
        self.lines.iter().cloned().collect()
    }
}

/// JSON-RPC 2.0 请求
#[derive(Serialize)]
pub(crate) struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl McpServerConfig {
    /// 解析配置中的相对路径为绝对路径
    ///
    /// - 绝对路径：原样返回
    /// - 相对路径：相对于可执行文件所在目录解析
    ///   - 先尝试 dev 模式路径（target/debug → 项目根）
    ///   - 再尝试 prod 模式路径（exe 同级目录）
    pub fn resolve_paths(mut self) -> Self {
        // 解析命令路径
        self.command = Self::resolve_one(&self.command);

        // 解析参数中的路径（只解析看起来像路径的参数）
        self.args = self
            .args
            .iter()
            .map(|arg| {
                if arg.starts_with("./") || arg.starts_with(".\\") || Self::looks_like_relative(arg) {
                    Self::resolve_one(arg)
                } else {
                    arg.clone()
                }
            })
            .collect();

        self
    }

    fn resolve_one(input: &str) -> String {
        let p = Path::new(input);
        if p.is_absolute() {
            return input.to_string();
        }

        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_default();

        // dev 模式：target/debug/ → ../../../ (项目根)
        let dev_path = exe_dir.join("..").join("..").join("..").join(input);
        let canon_dev = std::fs::canonicalize(&dev_path);
        if let Ok(abs) = canon_dev {
            return Self::strip_verbatim_prefix(&abs);
        }

        // prod 模式：exe 同级目录
        let prod_path = exe_dir.join(input);
        if let Ok(abs) = std::fs::canonicalize(&prod_path) {
            return Self::strip_verbatim_prefix(&abs);
        }

        // 都找不到就保持原样（让系统报错）
        input.to_string()
    }

    /// Windows 上 `canonicalize()` 返回 `\\?\C:\...` 格式的扩展路径，
    /// Node.js 无法识别这种路径（EISDIR），需要去掉 `\\?\` 前缀。
    fn strip_verbatim_prefix(path: &std::path::Path) -> String {
        let raw = path.to_string_lossy().to_string();
        #[cfg(windows)]
        {
            raw.strip_prefix(r"\\?\").unwrap_or(&raw).to_string()
        }
        #[cfg(not(windows))]
        {
            raw
        }
    }

    fn looks_like_relative(path: &str) -> bool {
        // 不带协议、不带 -y/-- 这种 flag 前缀的路径
        let p = Path::new(path);
        p.is_relative()
            && !path.starts_with('-')
            && !path.contains("://")
            && (path.contains('/') || path.contains('\\'))
    }
}
