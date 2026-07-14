//! MCP 客户端实现（stdio + JSON-RPC 2.0）
//!
//! 设计目标：让桌面 App 作为 MCP Host，连接外部 MCP Server（stdio 子进程），
//! 聚合它们暴露的 tools，并供 LLM 通过 OpenAI function-calling 协议调用。
//!
//! 协议实现参考 MCP 规范 2024-11-05，支持：
//!   - stdio 传输（本地子进程）
//!   - 完整的 JSON-RPC 2.0 错误解析（code + message + data）
//!   - stderr 环缓冲区捕获（Arc<Mutex<StderrRing>> 真正生效）
//!   - 相对路径自动解析（dev 模式 → 项目根；prod 模式 → exe 同级）
//!   - 工具调用缓存（TTL 60s，避免重复 LLM 调用）
//!   - 自动重连（保存配置，崩溃后自动恢复）

use crate::error_codes::McpError;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex as StdMutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};

#[cfg(windows)]
use std::os::windows::process::CommandExt;
use tokio::sync::Mutex;
use tokio::time::{timeout, Duration, Instant};

/// 单次工具调用的超时时间（秒）
const TOOL_CALL_TIMEOUT_SECS: u64 = 30;
/// MCP 握手（initialize + tools/list）的超时时间（秒）
const HANDSHAKE_TIMEOUT_SECS: u64 = 15;
/// 每行 JSON-RPC 响应读取的超时时间（秒）
const READ_TIMEOUT_SECS: u64 = 10;
/// stderr 环缓冲区最大行数
const STDERR_RING_SIZE: usize = 50;
/// 工具调用缓存 TTL（秒）
const CACHE_TTL_SECS: u64 = 60;
/// 缓存最大条目数（超过则清空）
const CACHE_MAX_ENTRIES: usize = 200;

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
struct JsonRpcError {
    code: i64,
    message: String,
    #[serde(default)]
    data: Option<Value>,
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
struct JsonRpcRequest {
    jsonrpc: String,
    id: u64,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
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
            return abs.to_string_lossy().to_string();
        }

        // prod 模式：exe 同级目录
        let prod_path = exe_dir.join(input);
        if let Ok(abs) = std::fs::canonicalize(&prod_path) {
            return abs.to_string_lossy().to_string();
        }

        // 都找不到就保持原样（让系统报错）
        input.to_string()
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

/// 一个已连接的 MCP Server 客户端
pub struct McpClient {
    child: Child,
    stdin: Option<ChildStdin>,
    reader: BufReader<ChildStdout>,
    next_id: u64,
    pub tools: Vec<McpTool>,
    server_name: String,
    /// stderr 环缓冲区（Arc 共享，drain_stderr 后台任务可写入）
    pub(crate) stderr_ring: Arc<StdMutex<StderrRing>>,
    /// 工具调用错误计数
    pub(crate) error_count: usize,
    /// 最近一次错误消息
    pub(crate) last_error: Option<String>,
    /// 已解析的命令路径
    pub(crate) resolved_command: String,
}

impl McpClient {
    /// 启动子进程并完成 MCP 握手（initialize + tools/list）
    pub async fn connect(config: &McpServerConfig) -> Result<Self, McpError> {
        let resolved_config = config.clone().resolve_paths();

        let mut std_cmd = std::process::Command::new(&resolved_config.command);
        std_cmd.creation_flags(0x08000000);
        let mut cmd = Command::from(std_cmd);
        cmd.args(&resolved_config.args);
        if let Some(env) = &resolved_config.env {
            for (k, v) in env {
                cmd.env(k, v);
            }
        }
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped()); // 捕获 stderr 用于诊断

        let mut child = cmd
            .spawn()
            .map_err(|e| McpError::process_spawn(&resolved_config.command, &e.to_string()))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| McpError::io_error("无法获取 MCP 进程的 stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| McpError::io_error("无法获取 MCP 进程的 stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| McpError::io_error("无法获取 MCP 进程的 stderr"))?;

        // 创建共享 stderr 环缓冲区（Arc<StdMutex<>>），后台任务和 McpClient 共用
        let stderr_ring = Arc::new(StdMutex::new(StderrRing::new(STDERR_RING_SIZE)));
        let ring_for_drain = stderr_ring.clone();
        let server_name = config.name.clone();
        let stderr_reader = BufReader::new(stderr);
        tokio::spawn(Self::drain_stderr(
            server_name.clone(),
            stderr_reader,
            ring_for_drain,
        ));

        let mut client = McpClient {
            child,
            stdin: Some(stdin),
            reader: BufReader::new(stdout),
            next_id: 1,
            tools: Vec::new(),
            server_name: server_name.clone(),
            stderr_ring,
            error_count: 0,
            last_error: None,
            resolved_command: resolved_config.command.clone(),
        };

        // 1) initialize 握手（带超时）
        let init_params = json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "agent-desktop", "version": "0.3.0" }
        });
        let _ = timeout(
            Duration::from_secs(HANDSHAKE_TIMEOUT_SECS),
            client.request("initialize", Some(init_params)),
        )
        .await
        .map_err(|_| McpError::timeout("initialize 握手超时"))?
        .map_err(|e| McpError::init_failed(&e.to_string()))?;

        // 2) 发送 initialized 通知（无需响应）
        client
            .notify("notifications/initialized", Some(json!({})))
            .await
            .map_err(|e| McpError::init_failed(&format!("initialized 通知失败: {}", e)))?;

        // 3) 列出工具（带超时）
        let result = timeout(
            Duration::from_secs(HANDSHAKE_TIMEOUT_SECS),
            client.request("tools/list", Some(json!({}))),
        )
        .await
        .map_err(|_| McpError::timeout("tools/list 超时"))??;

        if let Some(arr) = result.get("tools").and_then(|t| t.as_array()) {
            for t in arr {
                match serde_json::from_value::<McpTool>(t.clone()) {
                    Ok(tool) => client.tools.push(tool),
                    Err(e) => {
                        eprintln!(
                            "[MCP:{}] 跳过无法解析的工具定义: {}",
                            client.server_name, e
                        );
                    }
                }
            }
        }

        eprintln!(
            "[MCP:{}] 连接成功，注册 {} 个工具 @ {}",
            client.server_name,
            client.tools.len(),
            client.resolved_command
        );
        Ok(client)
    }

    /// 后台任务：将 stderr 输出同时写入共享环缓冲区和 eprintln
    async fn drain_stderr(
        server_name: String,
        mut reader: BufReader<ChildStderr>,
        ring: Arc<StdMutex<StderrRing>>,
    ) {
        let mut buf = String::new();
        loop {
            buf.clear();
            match reader.read_line(&mut buf).await {
                Ok(0) => {
                    eprintln!("[MCP:{}] stderr 流结束 (EOF)", server_name);
                    break;
                }
                Ok(_) => {
                    let line = buf.trim().to_string();
                    if !line.is_empty() {
                        eprintln!("[MCP:{}][stderr] {}", server_name, line);
                        if let Ok(mut r) = ring.lock() {
                            r.push(line);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[MCP:{}] stderr 读取错误: {}", server_name, e);
                    break;
                }
            }
        }
    }

    fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// 发送 JSON-RPC 请求并等待匹配 id 的响应（带超时）
    async fn request(&mut self, method: &str, params: Option<Value>) -> Result<Value, McpError> {
        let id = self.alloc_id();
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.to_string(),
            params,
        };
        let mut line =
            serde_json::to_string(&req).map_err(|e| McpError::json_parse(&e.to_string()))?;
        line.push('\n');

        // 写入请求
        {
            let stdin = self
                .stdin
                .as_mut()
                .ok_or_else(|| McpError::conn_closed())?;
            stdin
                .write_all(line.as_bytes())
                .await
                .map_err(|e| McpError::io_error(&format!("写入失败: {}", e)))?;
            stdin
                .flush()
                .await
                .map_err(|e| McpError::io_error(&format!("flush 失败: {}", e)))?;
        }

        // 持续读取，直到找到匹配 id 的响应（跳过通知），带超时保护
        let method_label = method.to_string();
        let result = timeout(
            Duration::from_secs(TOOL_CALL_TIMEOUT_SECS),
            self.read_response(id),
        )
        .await;

        match result {
            Ok(Ok(val)) => Ok(val),
            Ok(Err(e)) => Err(e),
            Err(_elapsed) => Err(McpError::timeout(&format!(
                "服务器 {} 上 {} 请求 (id={}) 在 {}s 内无响应",
                self.server_name, method_label, id, TOOL_CALL_TIMEOUT_SECS
            ))),
        }
    }

    /// 内部：读取 JSON-RPC 行直到匹配指定 id
    async fn read_response(&mut self, target_id: u64) -> Result<Value, McpError> {
        loop {
            let mut buf = String::new();

            // 单行读取也加超时（防止 MCP 进程卡在生成中）
            let read_result = timeout(
                Duration::from_secs(READ_TIMEOUT_SECS),
                self.reader.read_line(&mut buf),
            )
            .await;

            // 超时
            let n = read_result.map_err(|_| {
                McpError::timeout(&format!(
                    "等待 MCP 响应超时 ({}s)",
                    READ_TIMEOUT_SECS
                ))
            })?
            .map_err(|e| McpError::io_error(&format!("读取失败: {}", e)))?;

            if n == 0 {
                // EOF：进程退出
                self.stdin = None;
                return Err(McpError::process_exited(&self.server_name));
            }

            let line_str = buf.trim();
            if line_str.is_empty() {
                continue;
            }

            let msg: Value = match serde_json::from_str(line_str) {
                Ok(v) => v,
                Err(_e) => {
                    eprintln!(
                        "[MCP:{}] 收到非 JSON 行，跳过: {}",
                        self.server_name,
                        &line_str[..line_str.len().min(120)]
                    );
                    // 累加错误计数防止死循环（垃圾输出太多）
                    continue;
                }
            };

            let msg_id = msg.get("id").and_then(|v| v.as_u64());
            if msg_id == Some(target_id) {
                if let Some(err) = msg.get("error") {
                    // 完整解析 JSON-RPC 错误对象：code + message + data
                    let err_obj: JsonRpcError = match serde_json::from_value(err.clone()) {
                        Ok(e) => e,
                        Err(_) => JsonRpcError {
                            code: -1,
                            message: err
                                .get("message")
                                .and_then(|v| v.as_str())
                                .unwrap_or("未知 MCP 错误")
                                .to_string(),
                            data: None,
                        },
                    };
                    let detail = if let Some(ref data) = err_obj.data {
                        format!(
                            "{} (code={}, data={})",
                            err_obj.message,
                            err_obj.code,
                            serde_json::to_string(data).unwrap_or_default()
                        )
                    } else {
                        format!("{} (code={})", err_obj.message, err_obj.code)
                    };
                    return Err(McpError::tool_error(&detail));
                }
                return Ok(msg.get("result").cloned().unwrap_or(Value::Null));
            }

            // 不是目标响应，可能是通知或无关消息，继续读取
        }
    }

    /// 发送无需响应的通知
    async fn notify(&mut self, method: &str, params: Option<Value>) -> Result<(), McpError> {
        let note = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        let mut line =
            serde_json::to_string(&note).map_err(|e| McpError::json_parse(&e.to_string()))?;
        line.push('\n');
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| McpError::conn_closed())?;
        stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|e| McpError::io_error(&format!("写入失败: {}", e)))?;
        stdin.flush().await.map_err(|e| {
            McpError::io_error(&format!("flush 失败: {}", e))
        })?;
        Ok(())
    }

    /// 调用一个工具，返回拼接后的文本内容
    pub async fn call_tool(
        &mut self,
        name: &str,
        arguments: Value,
    ) -> Result<String, McpError> {
        let params = json!({ "name": name, "arguments": arguments });

        // 工具调用本身带超时
        let result = timeout(
            Duration::from_secs(TOOL_CALL_TIMEOUT_SECS),
            self.request("tools/call", Some(params)),
        )
        .await
        .map_err(|_| {
            McpError::timeout(&format!(
                "工具 {}::{} 执行超时 ({}s)",
                self.server_name, name, TOOL_CALL_TIMEOUT_SECS
            ))
        })?;

        match result {
            Ok(val) => {
                let mut text = String::new();
                if let Some(content) = val.get("content").and_then(|c| c.as_array()) {
                    for item in content {
                        if let Some(t) = item.get("text").and_then(|t| t.as_str()) {
                            text.push_str(t);
                            text.push('\n');
                        }
                    }
                }

                if let Some(true) = val.get("isError").and_then(|v| v.as_bool()) {
                    self.error_count += 1;
                    let err_text = if text.is_empty() {
                        "工具报告错误但未提供详情".to_string()
                    } else {
                        text.trim().to_string()
                    };
                    self.last_error = Some(err_text.clone());
                    return Err(McpError::tool_error(&err_text));
                }

                // 成功：清零最近错误
                self.last_error = None;
                Ok(text.trim().to_string())
            }
            Err(e) => {
                self.error_count += 1;
                self.last_error = Some(e.to_string());
                Err(e)
            }
        }
    }

    /// 检测子进程是否仍在运行
    pub fn is_alive(&mut self) -> bool {
        match self.child.try_wait() {
            Ok(Some(_)) => false, // 已退出
            Ok(None) => true,     // 仍在运行
            Err(e) => {
                eprintln!(
                    "[MCP:{}] 检测进程状态失败: {}",
                    self.server_name, e
                );
                false
            }
        }
    }

    /// 终止子进程
    pub fn kill(&mut self) {
        let _ = self.child.start_kill();
    }
}

/// 管理所有已连接的 MCP Server，并聚合它们的工具供 LLM 使用
pub struct McpManager {
    pub servers: Mutex<HashMap<String, McpClient>>,
    /// 持久化服务器配置（用于崩溃后自动重连）
    server_configs: Mutex<HashMap<String, McpServerConfig>>,
    /// 工具调用缓存（key → 过期时间 + 结果），避免 LLM 重复调用
    tool_cache: Mutex<HashMap<String, (Instant, String)>>,
}

impl McpManager {
    pub fn new() -> Self {
        Self {
            servers: Mutex::new(HashMap::new()),
            server_configs: Mutex::new(HashMap::new()),
            tool_cache: Mutex::new(HashMap::new()),
        }
    }

    /// 连接（或重连）一个 MCP Server，同时保存配置用于自动重连
    pub async fn connect(&self, config: McpServerConfig) -> Result<usize, McpError> {
        let client = McpClient::connect(&config).await?;
        let count = client.tools.len();
        let name = config.name.clone();

        // 保存配置用于自动重连
        self.server_configs.lock().await.insert(name.clone(), config);

        let mut servers = self.servers.lock().await;
        if let Some(mut old) = servers.remove(&name) {
            old.kill();
        }
        servers.insert(name, client);
        Ok(count)
    }

    /// 断开指定服务器（同时清理配置）
    pub async fn disconnect(&self, name: &str) -> Result<(), String> {
        self.server_configs.lock().await.remove(name);
        let mut servers = self.servers.lock().await;
        if let Some(mut c) = servers.remove(name) {
            c.kill();
        }
        Ok(())
    }

    /// 重连指定服务器（使用之前保存的配置）
    pub async fn reconnect(&self, name: &str) -> Result<usize, McpError> {
        let config = {
            let configs = self.server_configs.lock().await;
            configs
                .get(name)
                .cloned()
                .ok_or_else(|| McpError::server_not_found(name))?
        };
        eprintln!("[MCP:{}] 自动重连...", name);
        self.connect(config).await
    }

    /// 列出已连接服务器信息
    pub async fn list_servers(&self) -> Vec<McpServerInfo> {
        let servers = self.servers.lock().await;
        servers
            .iter()
            .map(|(name, c)| McpServerInfo {
                name: name.clone(),
                status: "connected".to_string(),
                tool_count: c.tools.len(),
                resolved_command: c.resolved_command.clone(),
                error_count: c.error_count,
                last_error: c.last_error.clone(),
            })
            .collect()
    }

    /// 获取指定服务器的 stderr 日志（从共享环缓冲区读取）
    pub async fn get_stderr(&self, name: &str) -> Vec<String> {
        let servers = self.servers.lock().await;
        servers
            .get(name)
            .and_then(|c| c.stderr_ring.lock().ok())
            .map(|r| r.snapshot())
            .unwrap_or_default()
    }

    /// 生成给 LLM 的 tools 数组（工具名用 `server::tool` 命名空间避免冲突）
    pub async fn llm_tools(&self) -> Vec<Value> {
        let servers = self.servers.lock().await;
        let mut out = Vec::new();
        for (sname, client) in servers.iter() {
            for tool in &client.tools {
                let ns = format!("{}::{}", sname, tool.name);
                out.push(json!({
                    "type": "function",
                    "function": {
                        "name": ns,
                        "description": tool.description.clone().unwrap_or_default(),
                        "parameters": tool.input_schema,
                    }
                }));
            }
        }
        out
    }

    /// 构建缓存 key（server::tool + 序列化参数）
    fn cache_key(server: &str, tool: &str, args: &Value) -> String {
        format!(
            "{}::{}::{}",
            server,
            tool,
            serde_json::to_string(args).unwrap_or_default()
        )
    }

    /// 通过命名空间工具名调用对应 Server 的工具（带缓存）
    ///
    /// 返回：
    /// - `Ok(text)`: 调用成功
    /// - `Err(McpError)`: 结构化错误
    pub async fn call_namespaced(
        &self,
        namespaced: &str,
        arguments: &str,
    ) -> Result<String, McpError> {
        let (server, tool) = namespaced
            .split_once("::")
            .ok_or_else(McpError::name_format)?;

        let args: Value =
            serde_json::from_str(arguments).map_err(|e| McpError::args_parse(&e.to_string()))?;

        // ---- 缓存检查 ----
        let key = Self::cache_key(server, tool, &args);
        {
            let cache = self.tool_cache.lock().await;
            if let Some((ts, result)) = cache.get(&key) {
                if ts.elapsed() < Duration::from_secs(CACHE_TTL_SECS) {
                    eprintln!("[MCP:{}] 缓存命中: {}", server, tool);
                    return Ok(result.clone());
                }
            }
        }

        // ---- 实际调用 ----
        let mut servers = self.servers.lock().await;
        let client = servers
            .get_mut(server)
            .ok_or_else(|| McpError::server_not_found(server))?;

        // 执行前检查进程存活
        if !client.is_alive() {
            eprintln!(
                "[MCP:{}] 进程已退出，尝试自动重连后重试...",
                server
            );
            // 自动重连
            drop(servers); // 释放锁
            match self.reconnect(server).await {
                Ok(_) => {
                    // 重连成功，重新获取 client
                    let mut servers = self.servers.lock().await;
                    match servers.get_mut(server) {
                        Some(c) => {
                            let result = c.call_tool(tool, args.clone()).await;
                            // 缓存结果
                            if let Ok(ref text) = result {
                                let mut cache = self.tool_cache.lock().await;
                                Self::trim_cache(&mut cache);
                                cache.insert(key, (Instant::now(), text.clone()));
                            }
                            return result;
                        }
                        None => return Err(McpError::server_not_found(server)),
                    }
                }
                Err(e) => return Err(e),
            }
        }

        let result = client.call_tool(tool, args.clone()).await;

        // 调用后检查（如果失败可能是进程崩溃）
        if result.is_err() && !client.is_alive() {
            eprintln!(
                "[MCP:{}] 工具调用失败且进程已退出，可能为崩溃",
                server
            );
            client.stdin = None;
        }

        // 成功时写入缓存
        if let Ok(ref text) = result {
            let mut cache = self.tool_cache.lock().await;
            Self::trim_cache(&mut cache);
            cache.insert(key, (Instant::now(), text.clone()));
        }

        result
    }

    /// 裁剪缓存（超过上限则清空）
    fn trim_cache(cache: &mut HashMap<String, (Instant, String)>) {
        if cache.len() > CACHE_MAX_ENTRIES {
            // 先清理过期条目
            cache.retain(|_, (ts, _)| ts.elapsed() < Duration::from_secs(CACHE_TTL_SECS));
            // 如果还是太多，清空
            if cache.len() > CACHE_MAX_ENTRIES {
                cache.clear();
            }
        }
    }

    /// 检查所有服务器的健康状态，返回已断开列表
    /// 如果 auto_reconnect=true，对已断开的服务器尝试自动重连
    pub async fn health_check(&self, auto_reconnect: bool) -> Vec<String> {
        let mut servers = self.servers.lock().await;
        let mut dead = Vec::new();
        // 需要收集 key 来避免借用冲突
        let names: Vec<String> = servers.keys().cloned().collect();
        for name in &names {
            let is_dead = servers
                .get_mut(name)
                .map(|c| !c.is_alive())
                .unwrap_or(true);
            if is_dead {
                dead.push(name.clone());
            }
        }
        for name in &dead {
            servers.remove(name);
            eprintln!("[MCP:{}] 健康检查发现进程已退出，已移除", name);
        }
        drop(servers); // 释放锁

        // 自动重连
        if auto_reconnect && !dead.is_empty() {
            for name in &dead {
                match self.reconnect(name).await {
                    Ok(n) => eprintln!("[MCP:{}] 自动重连成功，{} 个工具", name, n),
                    Err(e) => eprintln!("[MCP:{}] 自动重连失败: {}", name, e),
                }
            }
        }

        dead
    }

    /// 清空缓存（用于调试或手动刷新）
    pub async fn clear_cache(&self) {
        self.tool_cache.lock().await.clear();
        eprintln!("[MCP] 工具调用缓存已清空");
    }

    /// 应用退出时清理所有 MCP 子进程
    pub async fn shutdown(&self) {
        let mut servers = self.servers.lock().await;
        if servers.is_empty() {
            return;
        }
        eprintln!("[MCP] 正在关闭 {} 个服务器...", servers.len());
        for (name, client) in servers.iter_mut() {
            eprintln!("[MCP] 终止子进程: {}", name);
            client.kill();
        }
        servers.clear();
        self.tool_cache.lock().await.clear();
        eprintln!("[MCP] 所有服务器已关闭");
    }
}

impl Default for McpManager {
    fn default() -> Self {
        Self::new()
    }
}

// ===================== MCP 市场（在线抓取） =====================

/// MCP 市场条目（前端展示用）
#[derive(Debug, Clone, Serialize)]
pub struct McpMarketEntry {
    pub name: String,
    pub description: String,
    pub description_zh: String,
    pub command: String,
    pub args: String,
    pub category: String,
    pub stars: u64,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
}

/// GitHub 搜索响应
#[derive(Debug, Deserialize)]
struct GitHubSearchResponse {
    items: Vec<GitHubRepo>,
}

#[derive(Debug, Deserialize)]
struct GitHubRepo {
    full_name: String,
    description: Option<String>,
    stargazers_count: u64,
    html_url: String,
    topics: Option<Vec<String>>,
}

/// npm 搜索响应
#[derive(Debug, Deserialize)]
struct NpmSearchResponse {
    objects: Vec<NpmPackage>,
    #[allow(dead_code)]
    total: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct NpmPackage {
    package: NpmPackageInfo,
    #[allow(dead_code)]
    score: Option<NpmScore>,
}

#[derive(Debug, Deserialize)]
struct NpmPackageInfo {
    name: String,
    #[allow(dead_code)]
    version: String,
    description: Option<String>,
    keywords: Option<Vec<String>>,
    #[allow(dead_code)]
    links: Option<NpmLinks>,
}

#[derive(Debug, Deserialize)]
struct NpmLinks {
    #[allow(dead_code)]
    npm: Option<String>,
    #[allow(dead_code)]
    homepage: Option<String>,
    #[allow(dead_code)]
    repository: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NpmScore {
    #[allow(dead_code)]
    final_score: Option<f64>,
}

/// 市场数据缓存（避免频繁请求 API）
static MARKET_CACHE: std::sync::OnceLock<tokio::sync::Mutex<Option<(Instant, Vec<McpMarketEntry>)>>> =
    std::sync::OnceLock::new();

fn market_cache() -> &'static tokio::sync::Mutex<Option<(Instant, Vec<McpMarketEntry>)>> {
    MARKET_CACHE.get_or_init(|| tokio::sync::Mutex::new(None))
}

/// 获取 MCP 市场列表（从 npm registry + GitHub 动态抓取）
#[tauri::command]
pub async fn mcp_market_list() -> Result<Vec<McpMarketEntry>, String> {
    // 检查缓存（5 分钟内有效）
    {
        let cache = market_cache().lock().await;
        if let Some((ts, entries)) = cache.as_ref() {
            if ts.elapsed() < Duration::from_secs(300) {
                eprintln!("[MCP市场] 缓存命中 ({} 条)", entries.len());
                return Ok(entries.clone());
            }
        }
    }

    let mut entries: Vec<McpMarketEntry> = Vec::new();
    let mut seen = HashSet::new();

    // 1. 从 npm registry 搜索 mcp-server 包（主要数据源）
    match fetch_npm_mcp_packages().await {
        Ok(pkgs) => {
            for pkg in pkgs {
                let entry = npm_to_entry(&pkg);
                if seen.insert(entry.name.clone()) {
                    entries.push(entry);
                }
            }
            eprintln!("[MCP市场] npm: {} 个包", entries.len());
        }
        Err(e) => {
            eprintln!("[MCP市场] npm 请求失败: {}", e);
        }
    }

    // 2. 从 GitHub 搜索 topic:mcp 补充（热门仓库）
    match fetch_github_mcp_repos().await {
        Ok(repos) => {
            let mut github_count = 0;
            for repo in repos {
                if let Some(entry) = github_to_entry(&repo) {
                    if seen.insert(entry.name.clone()) {
                        github_count += 1;
                        entries.push(entry);
                    }
                }
            }
            eprintln!("[MCP市场] GitHub: {} 个新条目", github_count);
        }
        Err(e) => {
            eprintln!("[MCP市场] GitHub 请求失败: {}", e);
        }
    }

    // 3. 如果在线源完全失败，回退到内置列表
    if entries.is_empty() {
        eprintln!("[MCP市场] 所有在线源失败，使用内置列表");
        entries = builtin_mcp_market();
    }

    // 按星标数降序排列
    entries.sort_by(|a, b| b.stars.cmp(&a.stars));

    // 写入缓存
    {
        let mut cache = market_cache().lock().await;
        *cache = Some((Instant::now(), entries.clone()));
    }

    eprintln!("[MCP市场] 共 {} 个条目", entries.len());
    Ok(entries)
}

/// 从 npm registry 搜索 mcp-server 包
async fn fetch_npm_mcp_packages() -> Result<Vec<NpmPackageInfo>, String> {
    let urls = [
        "https://registry.npmjs.org/-/v1/search?text=keywords:mcp-server&size=50",
        "https://registry.npmjs.org/-/v1/search?text=keywords:mcp&size=30",
    ];

    let mut all_packages: Vec<NpmPackageInfo> = Vec::new();
    let client = reqwest::Client::builder()
        .user_agent("agent-desktop/0.3.0")
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("构建 HTTP 客户端失败: {}", e))?;

    for url in &urls {
        let resp = client.get(*url).send().await.map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            eprintln!("[MCP市场] npm {} 返回 {}", url, resp.status());
            continue;
        }
        match resp.json::<NpmSearchResponse>().await {
            Ok(data) => {
                for obj in data.objects {
                    // 过滤：只保留真正的 MCP server 包
                    let name = &obj.package.name;
                    if name.contains("mcp-server") || name.contains("mcp_server") {
                        all_packages.push(obj.package);
                    }
                }
            }
            Err(e) => {
                eprintln!("[MCP市场] npm 解析失败: {}", e);
            }
        }
    }

    Ok(all_packages)
}

/// 从 GitHub 搜索 MCP 相关仓库
async fn fetch_github_mcp_repos() -> Result<Vec<GitHubRepo>, String> {
    let urls = [
        "https://api.github.com/search/repositories?q=topic:mcp-server&sort=stars&order=desc&per_page=30",
        "https://api.github.com/search/repositories?q=mcp+server+in:name&sort=stars&order=desc&per_page=20",
    ];

    let mut all_repos: Vec<GitHubRepo> = Vec::new();
    let client = reqwest::Client::builder()
        .user_agent("agent-desktop/0.3.0")
        .default_headers({
            let mut h = reqwest::header::HeaderMap::new();
            h.insert(reqwest::header::ACCEPT, reqwest::header::HeaderValue::from_static("application/vnd.github.v3+json"));
            h
        })
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("构建 HTTP 客户端失败: {}", e))?;

    for url in &urls {
        let resp = client.get(*url).send().await.map_err(|e| e.to_string())?;
        if resp.status() == 403 {
            // GitHub API 限流
            eprintln!("[MCP市场] GitHub API 限流，跳过");
            continue;
        }
        if !resp.status().is_success() {
            eprintln!("[MCP市场] GitHub {} 返回 {}", url, resp.status());
            continue;
        }
        match resp.json::<GitHubSearchResponse>().await {
            Ok(data) => all_repos.extend(data.items),
            Err(e) => eprintln!("[MCP市场] GitHub 解析失败: {}", e),
        }
    }

    Ok(all_repos)
}

/// npm 包 → 市场条目
fn npm_to_entry(pkg: &NpmPackageInfo) -> McpMarketEntry {
    let category = infer_mcp_category(&pkg.name, pkg.keywords.as_deref().unwrap_or(&[]));
    McpMarketEntry {
        name: pkg.name.clone(),
        description: pkg.description.clone().unwrap_or_default(),
        description_zh: String::new(), // npm 没有中文描述
        command: "npx".into(),
        args: format!("-y {}", pkg.name),
        category,
        stars: 0,
        source: "npm".into(),
        env: infer_mcp_env(&pkg.name),
        homepage: None,
    }
}

/// GitHub 仓库 → 市场条目（仅处理标准 MCP server 仓库）
fn github_to_entry(repo: &GitHubRepo) -> Option<McpMarketEntry> {
    let name = repo.full_name.clone();
    // 跳过聚合仓库（包含多个 server 的）
    if name == "modelcontextprotocol/servers" {
        return None;
    }

    // 从仓库名推断 npm 包名
    let npm_pkg = if name.contains("mcp-server") {
        // e.g., "anthropic/mcp-server-filesystem" → "@anthropic/mcp-server-filesystem"
        // e.g., "modelcontextprotocol/server-filesystem" → "@modelcontextprotocol/server-filesystem"
        let org = name.split('/').next()?;
        let pkg = name.split('/').nth(1)?;
        format!("@{}//{}", org, pkg)  // Will be cleaned below
    } else {
        return None; // 不是标准 MCP server
    };

    // 清理包名
    let npm_pkg = npm_pkg.replace("//", "/");

    let desc = repo.description.clone().unwrap_or_default();
    let category = infer_mcp_category(&npm_pkg, repo.topics.as_deref().unwrap_or(&[]));

    Some(McpMarketEntry {
        name: npm_pkg.clone(),
        description: desc.clone(),
        description_zh: String::new(),
        command: "npx".into(),
        args: format!("-y {}", npm_pkg),
        category,
        stars: repo.stargazers_count,
        source: "github".into(),
        env: infer_mcp_env(&npm_pkg),
        homepage: Some(repo.html_url.clone()),
    })
}

/// 根据包名和关键词推断分类
fn infer_mcp_category(name: &str, keywords: &[String]) -> String {
    let lower = name.to_lowercase();
    let kw_lower: Vec<String> = keywords.iter().map(|k| k.to_lowercase()).collect();

    if lower.contains("file") || lower.contains("fs") || kw_lower.iter().any(|k| k.contains("file")) {
        "tools".into()
    } else if lower.contains("github") || lower.contains("git") || kw_lower.iter().any(|k| k.contains("git")) {
        "tools".into()
    } else if lower.contains("search") || lower.contains("brave") || lower.contains("tavily") || kw_lower.iter().any(|k| k.contains("search")) {
        "search".into()
    } else if lower.contains("puppeteer") || lower.contains("playwright") || lower.contains("browser") || lower.contains("chrome") || kw_lower.iter().any(|k| k.contains("browser")) {
        "browser".into()
    } else if lower.contains("postgres") || lower.contains("sqlite") || lower.contains("mysql") || lower.contains("database") || lower.contains("qdrant") || lower.contains("redis") || kw_lower.iter().any(|k| k.contains("database") || k.contains("sql")) {
        "database".into()
    } else if lower.contains("memory") || lower.contains("think") || lower.contains("reason") || lower.contains("ai") || lower.contains("llm") || kw_lower.iter().any(|k| k.contains("ai") || k.contains("memory")) {
        "ai".into()
    } else if lower.contains("slack") || lower.contains("notion") || lower.contains("linear") || lower.contains("jira") || kw_lower.iter().any(|k| k.contains("communication")) {
        "communication".into()
    } else if lower.contains("figma") || lower.contains("design") || lower.contains("map") || kw_lower.iter().any(|k| k.contains("design")) {
        "design".into()
    } else if lower.contains("docker") || lower.contains("sentry") || lower.contains("cloudflare") || lower.contains("k8s") || lower.contains("kubernetes") || kw_lower.iter().any(|k| k.contains("infra")) {
        "infra".into()
    } else if lower.contains("image") || lower.contains("replicate") || lower.contains("everart") || kw_lower.iter().any(|k| k.contains("image") || k.contains("generation")) {
        "ai".into()
    } else {
        "tools".into()
    }
}

/// 根据包名推断需要的环境变量
fn infer_mcp_env(pkg_name: &str) -> Option<HashMap<String, String>> {
    let lower = pkg_name.to_lowercase();
    let mut env = HashMap::new();

    if lower.contains("github") {
        env.insert("GITHUB_PERSONAL_ACCESS_TOKEN".into(), "".into());
    } else if lower.contains("brave") {
        env.insert("BRAVE_API_KEY".into(), "".into());
    } else if lower.contains("tavily") {
        env.insert("TAVILY_API_KEY".into(), "".into());
    } else if lower.contains("postgres") {
        env.insert("DATABASE_URL".into(), "postgresql://localhost:5432/...".into());
    } else if lower.contains("slack") {
        env.insert("SLACK_BOT_TOKEN".into(), "".into());
    } else if lower.contains("notion") {
        env.insert("NOTION_API_KEY".into(), "".into());
    } else if lower.contains("linear") {
        env.insert("LINEAR_API_KEY".into(), "".into());
    } else if lower.contains("figma") {
        env.insert("FIGMA_ACCESS_TOKEN".into(), "".into());
    } else if lower.contains("sentry") {
        env.insert("SENTRY_AUTH_TOKEN".into(), "".into());
    } else if lower.contains("cloudflare") {
        env.insert("CLOUDFLARE_API_TOKEN".into(), "".into());
    } else if lower.contains("supabase") {
        env.insert("SUPABASE_ACCESS_TOKEN".into(), "".into());
    } else if lower.contains("everart") {
        env.insert("EVERART_API_KEY".into(), "".into());
    } else if lower.contains("replicate") {
        env.insert("REPLICATE_API_TOKEN".into(), "".into());
    } else if lower.contains("browserbase") {
        env.insert("BROWSERBASE_API_KEY".into(), "".into());
        env.insert("BROWSERBASE_PROJECT_ID".into(), "".into());
    } else {
        return None;
    }

    Some(env)
}

/// 内置 MCP 市场列表（离线回退用）
fn builtin_mcp_market() -> Vec<McpMarketEntry> {
    vec![
        McpMarketEntry { name: "@modelcontextprotocol/server-filesystem".into(), description: "File system operations: read/write files, create dirs, search files.".into(), description_zh: "文件系统操作：读写文件、创建目录、搜索文件、编辑代码。".into(), command: "npx".into(), args: "-y @modelcontextprotocol/server-filesystem .".into(), category: "tools".into(), stars: 500, source: "builtin".into(), env: None, homepage: None },
        McpMarketEntry { name: "@modelcontextprotocol/server-github".into(), description: "GitHub API integration: repos, issues, PRs, search code.".into(), description_zh: "GitHub API：管理仓库、Issue、PR、搜索代码。需 GITHUB_PERSONAL_ACCESS_TOKEN。".into(), command: "npx".into(), args: "-y @modelcontextprotocol/server-github".into(), category: "tools".into(), stars: 400, source: "builtin".into(), env: Some(HashMap::from([("GITHUB_PERSONAL_ACCESS_TOKEN".into(), "".into())])), homepage: None },
        McpMarketEntry { name: "@modelcontextprotocol/server-git".into(), description: "Git version control: commit, branch, log, diff, blame.".into(), description_zh: "Git 版本控制：提交、分支、日志、diff、blame。".into(), command: "npx".into(), args: "-y @modelcontextprotocol/server-git --repository .".into(), category: "tools".into(), stars: 300, source: "builtin".into(), env: None, homepage: None },
        McpMarketEntry { name: "@modelcontextprotocol/server-fetch".into(), description: "Web content fetching: convert URLs to Markdown for AI reading.".into(), description_zh: "网页抓取：将 URL 转为 Markdown 文本。".into(), command: "npx".into(), args: "-y @modelcontextprotocol/server-fetch".into(), category: "tools".into(), stars: 250, source: "builtin".into(), env: None, homepage: None },
        McpMarketEntry { name: "@modelcontextprotocol/server-puppeteer".into(), description: "Puppeteer browser automation: screenshots, scraping, form filling.".into(), description_zh: "Puppeteer 浏览器自动化：截图、抓取、表单填写。".into(), command: "npx".into(), args: "-y @modelcontextprotocol/server-puppeteer".into(), category: "browser".into(), stars: 450, source: "builtin".into(), env: None, homepage: None },
        McpMarketEntry { name: "@modelcontextprotocol/server-brave-search".into(), description: "Brave Search: web & news search. Free 2000/month. Needs BRAVE_API_KEY.".into(), description_zh: "Brave 搜索引擎：网页+新闻搜索，免费2000次/月。需 BRAVE_API_KEY。".into(), command: "npx".into(), args: "-y @modelcontextprotocol/server-brave-search".into(), category: "search".into(), stars: 350, source: "builtin".into(), env: Some(HashMap::from([("BRAVE_API_KEY".into(), "".into())])), homepage: None },
        McpMarketEntry { name: "@modelcontextprotocol/server-postgres".into(), description: "PostgreSQL database: SQL queries, schema inspection. Needs DATABASE_URL.".into(), description_zh: "PostgreSQL 数据库：SQL查询、表结构查看。需 DATABASE_URL。".into(), command: "npx".into(), args: "-y @modelcontextprotocol/server-postgres".into(), category: "database".into(), stars: 300, source: "builtin".into(), env: Some(HashMap::from([("DATABASE_URL".into(), "postgresql://localhost:5432/...".into())])), homepage: None },
        McpMarketEntry { name: "@modelcontextprotocol/server-memory".into(), description: "Persistent memory with vector search for AI across conversations.".into(), description_zh: "持久化记忆：向量检索，跨对话记住关键信息。".into(), command: "npx".into(), args: "-y @modelcontextprotocol/server-memory".into(), category: "ai".into(), stars: 400, source: "builtin".into(), env: None, homepage: None },
        McpMarketEntry { name: "@modelcontextprotocol/server-sequential-thinking".into(), description: "Sequential thinking engine for complex reasoning.".into(), description_zh: "分步推理引擎：复杂问题逐步思考、假设检验。".into(), command: "npx".into(), args: "-y @modelcontextprotocol/server-sequential-thinking".into(), category: "ai".into(), stars: 350, source: "builtin".into(), env: None, homepage: None },
        McpMarketEntry { name: "@modelcontextprotocol/server-slack".into(), description: "Slack integration: send messages, read channels. Needs SLACK_BOT_TOKEN.".into(), description_zh: "Slack 集成：发送消息、读取频道。需 SLACK_BOT_TOKEN。".into(), command: "npx".into(), args: "-y @modelcontextprotocol/server-slack".into(), category: "communication".into(), stars: 200, source: "builtin".into(), env: Some(HashMap::from([("SLACK_BOT_TOKEN".into(), "".into())])), homepage: None },
        McpMarketEntry { name: "@modelcontextprotocol/server-notion".into(), description: "Notion workspace: pages, databases, comments. Needs NOTION_API_KEY.".into(), description_zh: "Notion 工作空间：页面、数据库、评论。需 NOTION_API_KEY。".into(), command: "npx".into(), args: "-y @modelcontextprotocol/server-notion".into(), category: "communication".into(), stars: 180, source: "builtin".into(), env: Some(HashMap::from([("NOTION_API_KEY".into(), "".into())])), homepage: None },
        McpMarketEntry { name: "@modelcontextprotocol/server-linear".into(), description: "Linear project management: issues, projects. Needs LINEAR_API_KEY.".into(), description_zh: "Linear 项目管理：Issue、项目跟踪。需 LINEAR_API_KEY。".into(), command: "npx".into(), args: "-y @modelcontextprotocol/server-linear".into(), category: "communication".into(), stars: 150, source: "builtin".into(), env: Some(HashMap::from([("LINEAR_API_KEY".into(), "".into())])), homepage: None },
        McpMarketEntry { name: "@modelcontextprotocol/server-figma".into(), description: "Figma design integration. Needs FIGMA_ACCESS_TOKEN.".into(), description_zh: "Figma 设计集成。需 FIGMA_ACCESS_TOKEN。".into(), command: "npx".into(), args: "-y @modelcontextprotocol/server-figma".into(), category: "design".into(), stars: 120, source: "builtin".into(), env: Some(HashMap::from([("FIGMA_ACCESS_TOKEN".into(), "".into())])), homepage: None },
        McpMarketEntry { name: "@modelcontextprotocol/server-google-maps".into(), description: "Google Maps: places, directions, geocoding. Needs GOOGLE_MAPS_API_KEY.".into(), description_zh: "Google Maps：地点搜索、路线、地理编码。需 GOOGLE_MAPS_API_KEY。".into(), command: "npx".into(), args: "-y @modelcontextprotocol/server-google-maps".into(), category: "design".into(), stars: 100, source: "builtin".into(), env: Some(HashMap::from([("GOOGLE_MAPS_API_KEY".into(), "".into())])), homepage: None },
        McpMarketEntry { name: "@modelcontextprotocol/server-sentry".into(), description: "Sentry error monitoring: errors, issues, performance. Needs SENTRY_AUTH_TOKEN.".into(), description_zh: "Sentry 错误监控：查询错误、跟踪Issue。需 SENTRY_AUTH_TOKEN。".into(), command: "npx".into(), args: "-y @modelcontextprotocol/server-sentry".into(), category: "infra".into(), stars: 90, source: "builtin".into(), env: Some(HashMap::from([("SENTRY_AUTH_TOKEN".into(), "".into())])), homepage: None },
        McpMarketEntry { name: "@modelcontextprotocol/server-docker".into(), description: "Docker container management. Needs local Docker.".into(), description_zh: "Docker 容器管理：列表、启停、日志。需本地 Docker。".into(), command: "npx".into(), args: "-y @modelcontextprotocol/server-docker".into(), category: "infra".into(), stars: 80, source: "builtin".into(), env: None, homepage: None },
        McpMarketEntry { name: "@modelcontextprotocol/server-cloudflare".into(), description: "Cloudflare services: Workers, KV, R2, D1. Needs CLOUDFLARE_API_TOKEN.".into(), description_zh: "Cloudflare：Workers、KV、R2、D1。需 CLOUDFLARE_API_TOKEN。".into(), command: "npx".into(), args: "-y @modelcontextprotocol/server-cloudflare".into(), category: "infra".into(), stars: 70, source: "builtin".into(), env: Some(HashMap::from([("CLOUDFLARE_API_TOKEN".into(), "".into())])), homepage: None },
        McpMarketEntry { name: "@modelcontextprotocol/server-everything".into(), description: "MCP reference server with all standard features demo.".into(), description_zh: "MCP 参考服务器：演示所有标准功能。学习MCP最佳实践。".into(), command: "npx".into(), args: "-y @modelcontextprotocol/server-everything".into(), category: "tools".into(), stars: 60, source: "builtin".into(), env: None, homepage: None },
        McpMarketEntry { name: "@executeautomation/playwright-mcp-server".into(), description: "Playwright multi-browser automation: Chromium, Firefox, WebKit.".into(), description_zh: "Playwright 多浏览器自动化(Chromium/Firefox/WebKit)。".into(), command: "npx".into(), args: "-y @executeautomation/playwright-mcp-server".into(), category: "browser".into(), stars: 200, source: "builtin".into(), env: None, homepage: None },
    ]
}
