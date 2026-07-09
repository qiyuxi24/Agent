//! 极简 MCP 客户端实现（stdio 传输 + JSON-RPC 2.0）
//!
//! 设计目标：让桌面 App 作为 MCP Host，连接外部 MCP Server（stdio 子进程），
//! 聚合它们暴露的 tools，并供 LLM 通过 OpenAI function-calling 协议调用。
//! 不依赖第三方 MCP SDK，仅使用 tokio 的异步进程 + 标准 JSON-RPC。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

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
}

impl McpClient {
    /// 启动子进程并完成 MCP 握手（initialize + tools/list）
    pub async fn connect(config: &McpServerConfig) -> Result<Self, String> {
        let mut cmd = Command::new(&config.command);
        cmd.args(&config.args);
        if let Some(env) = &config.env {
            for (k, v) in env {
                cmd.env(k, v);
            }
        }
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("启动 MCP 进程失败 ({}): {}", config.command, e))?;

        let stdin = child.stdin.take().ok_or("无法获取 MCP 进程的 stdin")?;
        let stdout = child.stdout.take().ok_or("无法获取 MCP 进程的 stdout")?;

        let mut client = McpClient {
            child,
            stdin: Some(stdin),
            reader: BufReader::new(stdout),
            next_id: 1,
            tools: Vec::new(),
        };

        // 1) initialize 握手
        let init_params = json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "agent-desktop", "version": "0.2.0" }
        });
        let _ = client.request("initialize", Some(init_params)).await?;

        // 2) 发送 initialized 通知（无需响应）
        client
            .notify("notifications/initialized", Some(json!({})))
            .await?;

        // 3) 列出工具
        let result = client.request("tools/list", Some(json!({}))).await?;
        if let Some(arr) = result.get("tools").and_then(|t| t.as_array()) {
            for t in arr {
                if let Ok(tool) = serde_json::from_value::<McpTool>(t.clone()) {
                    client.tools.push(tool);
                }
            }
        }

        Ok(client)
    }

    fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// 发送 JSON-RPC 请求并等待匹配 id 的响应
    async fn request(&mut self, method: &str, params: Option<Value>) -> Result<Value, String> {
        let id = self.alloc_id();
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.to_string(),
            params,
        };
        let mut line = serde_json::to_string(&req).map_err(|e| e.to_string())?;
        line.push('\n');

        {
            let stdin = self.stdin.as_mut().ok_or("MCP stdin 已关闭")?;
            stdin
                .write_all(line.as_bytes())
                .await
                .map_err(|e| e.to_string())?;
            stdin.flush().await.map_err(|e| e.to_string())?;
        }

        // 持续读取，直到找到匹配 id 的响应（跳过通知）
        loop {
            let mut buf = String::new();
            let n = self
                .reader
                .read_line(&mut buf)
                .await
                .map_err(|e| e.to_string())?;
            if n == 0 {
                return Err("MCP 进程连接已关闭".to_string());
            }
            let line = buf.trim();
            if line.is_empty() {
                continue;
            }
            let msg: Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let msg_id = msg.get("id").and_then(|v| v.as_u64());
            if msg_id == Some(id) {
                if let Some(err) = msg.get("error") {
                    return Err(format!("MCP 错误: {}", err));
                }
                return Ok(msg.get("result").cloned().unwrap_or(Value::Null));
            }
        }
    }

    /// 发送无需响应的通知
    async fn notify(&mut self, method: &str, params: Option<Value>) -> Result<(), String> {
        let note = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        let mut line = serde_json::to_string(&note).map_err(|e| e.to_string())?;
        line.push('\n');
        let stdin = self.stdin.as_mut().ok_or("MCP stdin 已关闭")?;
        stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|e| e.to_string())?;
        stdin.flush().await.map_err(|e| e.to_string())?;
        Ok(())
    }

    /// 调用一个工具，返回拼接后的文本内容
    pub async fn call_tool(&mut self, name: &str, arguments: Value) -> Result<String, String> {
        let params = json!({ "name": name, "arguments": arguments });
        let result = self.request("tools/call", Some(params)).await?;

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
            return Err(format!("工具执行错误: {}", text));
        }
        Ok(text.trim().to_string())
    }

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
    pub async fn connect(&self, config: McpServerConfig) -> Result<usize, String> {
        let client = McpClient::connect(&config).await?;
        let count = client.tools.len();
        let mut servers = self.servers.lock().await;
        if let Some(mut old) = servers.remove(&config.name) {
            old.kill();
        }
        servers.insert(config.name.clone(), client);
        Ok(count)
    }

    pub async fn disconnect(&self, name: &str) -> Result<(), String> {
        let mut servers = self.servers.lock().await;
        if let Some(mut c) = servers.remove(name) {
            c.kill();
        }
        Ok(())
    }

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
    pub async fn call_namespaced(&self, namespaced: &str, arguments: &str) -> Result<String, String> {
        let (server, tool) = namespaced
            .split_once("::")
            .ok_or_else(|| "工具名格式错误（应为 server::tool）".to_string())?;
        let args: Value = serde_json::from_str(arguments).unwrap_or(Value::Null);
        let mut servers = self.servers.lock().await;
        let client = servers
            .get_mut(server)
            .ok_or_else(|| format!("MCP 服务器未连接: {}", server))?;
        client.call_tool(tool, args).await
    }
}
