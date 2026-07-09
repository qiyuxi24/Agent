//! 极简 MCP 客户端实现（stdio 传输 + JSON-RPC 2.0）
//!
//! 设计目标：让桌面 App 作为 MCP Host，连接外部 MCP Server（stdio 子进程），
//! 聚合它们暴露的 tools，并供 LLM 通过 OpenAI function-calling 协议调用。
//! 不依赖第三方 MCP SDK，仅使用 tokio 的异步进程 + 标准 JSON-RPC。

use crate::error_codes::McpError;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;
use tokio::time::{timeout, Duration};

/// 单次工具调用的超时时间（秒）
const TOOL_CALL_TIMEOUT_SECS: u64 = 30;
/// MCP 握手（initialize + tools/list）的超时时间（秒）
const HANDSHAKE_TIMEOUT_SECS: u64 = 15;
/// 每行 JSON-RPC 响应读取的超时时间（秒）
const READ_TIMEOUT_SECS: u64 = 10;

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

/// 用于前端展示的 Server 状态
#[derive(Debug, Serialize, Clone)]
pub struct McpServerInfo {
    pub name: String,
    pub status: String,
    pub tool_count: usize,
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

/// 一个已连接的 MCP Server 客户端
pub struct McpClient {
    child: Child,
    stdin: Option<ChildStdin>,
    reader: BufReader<ChildStdout>,
    next_id: u64,
    pub tools: Vec<McpTool>,
    server_name: String,
}

impl McpClient {
    /// 启动子进程并完成 MCP 握手（initialize + tools/list）
    pub async fn connect(config: &McpServerConfig) -> Result<Self, McpError> {
        let mut cmd = Command::new(&config.command);
        cmd.args(&config.args);
        if let Some(env) = &config.env {
            for (k, v) in env {
                cmd.env(k, v);
            }
        }
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped()); // 捕获 stderr 用于诊断

        let mut child = cmd
            .spawn()
            .map_err(|e| McpError::process_spawn(&config.command, &e.to_string()))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| McpError::io_error("无法获取 MCP 进程的 stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| McpError::io_error("无法获取 MCP 进程的 stdout"))?;

        let mut client = McpClient {
            child,
            stdin: Some(stdin),
            reader: BufReader::new(stdout),
            next_id: 1,
            tools: Vec::new(),
            server_name: config.name.clone(),
        };

        // 1) initialize 握手（带超时）
        let init_params = json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "agent-desktop", "version": "0.2.0" }
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
                            config.name, e
                        );
                    }
                }
            }
        }

        eprintln!(
            "[MCP:{}] 连接成功，注册 {} 个工具",
            config.name,
            client.tools.len()
        );
        Ok(client)
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
                    let err_msg = err
                        .get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("未知 MCP 错误");
                    return Err(McpError::tool_error(err_msg));
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
        })??;

        let mut text = String::new();
        if let Some(content) = result.get("content").and_then(|c| c.as_array()) {
            for item in content {
                if let Some(t) = item.get("text").and_then(|t| t.as_str()) {
                    text.push_str(t);
                    text.push('\n');
                }
            }
        }

        if let Some(true) = result.get("isError").and_then(|v| v.as_bool()) {
            let err_text = if text.is_empty() {
                "工具报告错误但未提供详情".to_string()
            } else {
                text.trim().to_string()
            };
            return Err(McpError::tool_error(&err_text));
        }

        Ok(text.trim().to_string())
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
#[derive(Default)]
pub struct McpManager {
    pub servers: Mutex<HashMap<String, McpClient>>,
}

impl McpManager {
    pub fn new() -> Self {
        Self {
            servers: Mutex::new(HashMap::new()),
        }
    }

    /// 连接（或重连）一个 MCP Server
    pub async fn connect(&self, config: McpServerConfig) -> Result<usize, McpError> {
        let client = McpClient::connect(&config).await?;
        let count = client.tools.len();
        let mut servers = self.servers.lock().await;
        if let Some(mut old) = servers.remove(&config.name) {
            old.kill();
        }
        servers.insert(config.name.clone(), client);
        Ok(count)
    }

    /// 断开指定服务器
    pub async fn disconnect(&self, name: &str) -> Result<(), String> {
        let mut servers = self.servers.lock().await;
        if let Some(mut c) = servers.remove(name) {
            c.kill();
        }
        Ok(())
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
            })
            .collect()
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

    /// 通过命名空间工具名调用对应 Server 的工具
    ///
    /// 返回 `(result_text, error_code)`：
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

        let mut servers = self.servers.lock().await;
        let client = servers
            .get_mut(server)
            .ok_or_else(|| McpError::server_not_found(server))?;

        // 执行前检查进程存活
        if !client.is_alive() {
            return Err(McpError::process_exited(server));
        }

        let result = client.call_tool(tool, args).await;

        // 调用后再次检查（如果失败可能是进程崩溃）
        if result.is_err() && !client.is_alive() {
            eprintln!(
                "[MCP:{}] 工具调用失败且进程已退出，可能为崩溃",
                server
            );
            // 清理已死进程
            client.stdin = None;
        }

        result
    }

    /// 检查所有服务器的健康状态，返回已断开列表
    pub async fn health_check(&self) -> Vec<String> {
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
        dead
    }
}
