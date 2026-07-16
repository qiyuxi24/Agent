//! MCP 客户端：启动子进程、完成握手、调用工具。

use super::types::*;
use crate::error_codes::McpError;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex as StdMutex};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use tokio::time::{timeout, Duration};

/// 一个已连接的 MCP Server 客户端
pub struct McpClient {
    pub(crate) child: Child,
    pub(crate) stdin: Option<ChildStdin>,
    pub(crate) reader: BufReader<ChildStdout>,
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
        // CREATE_NO_WINDOW: 防止 MCP 子进程弹出控制台窗口（仅 Windows）
        #[cfg(windows)]
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
            "clientInfo": { "name": "votek", "version": env!("CARGO_PKG_VERSION") }
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
                    // UTF-8 安全截断：按字符边界截取，避免在多字节字符中间切断
                    let preview = {
                        let end = line_str.len().min(120);
                        if line_str.is_char_boundary(end) {
                            &line_str[..end]
                        } else {
                            // 向前找最近的字符边界
                            let mut boundary = end;
                            while boundary > 0 && !line_str.is_char_boundary(boundary) {
                                boundary -= 1;
                            }
                            &line_str[..boundary]
                        }
                    };
                    eprintln!(
                        "[MCP:{}] 收到非 JSON 行，跳过: {}",
                        self.server_name,
                        preview
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
