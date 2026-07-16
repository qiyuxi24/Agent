//! VSCode Bridge — WebSocket 客户端连接 Votek Companion 扩展
//!
//! ## 架构
//! ```
//! Votek Agent (Rust) ──WebSocket──→ Votek Companion (VS Code Extension in code-server)
//!                      JSON-RPC 2.0
//! ```
//!
//! ## 解耦设计
//! - 本模块不依赖 code-server 或 VS Code 的任何内部实现
//! - 通过标准 JSON-RPC 2.0 over WebSocket 协议通信
//! - 伴生扩展未就绪时，所有工具优雅降级返回错误信息
//! - 端口和 auth token 通过 `VotekBridgeConfig` 传入，由 `code_server.rs` 注入
//!
//! ## 使用方式
//! ```ignore
//! let bridge = VscodeBridge::new("ws://127.0.0.1:19527", "secret-token");
//! bridge.connect().await?;
//! let editor = bridge.get_active_editor().await?;
//! ```

use crate::tools::ToolRegistry;
use futures::stream::StreamExt;
use futures::SinkExt;
use rand::Rng;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio_tungstenite::{connect_async, tungstenite::Message};

// ── 模块常量 ──

/// 伴生扩展默认端口（可通过配置覆盖）
pub const BRIDGE_DEFAULT_PORT: u16 = 19527;
/// WebSocket 连接超时（秒）
const CONNECT_TIMEOUT_SECS: u64 = 5;
/// JSON-RPC 调用超时（秒）
const CALL_TIMEOUT_SECS: u64 = 15;
/// 重连间隔（秒）
const RECONNECT_INTERVAL_SECS: u64 = 3;
/// 最大重连次数
const MAX_RECONNECT_ATTEMPTS: u32 = 5;
/// Auth token 长度（字节）
const TOKEN_LEN: usize = 32;

// ── 配置 ──

/// 桥接配置（由 code_server.rs 创建并传入）
#[derive(Debug, Clone)]
pub struct VotekBridgeConfig {
    pub port: u16,
    pub token: String,
}

impl VotekBridgeConfig {
    /// 创建默认配置（随机 token）
    pub fn new(port: u16) -> Self {
        let mut rng = rand::thread_rng();
        let token: String = (0..TOKEN_LEN)
            .map(|_| format!("{:02x}", rng.gen::<u8>()))
            .collect();
        Self { port, token }
    }

    /// WebSocket URL
    pub fn ws_url(&self) -> String {
        format!("ws://127.0.0.1:{}/?token={}", self.port, self.token)
    }
}

// ── 桥接主结构 ──

/// VSCode Bridge 客户端
///
/// 使用短连接模式：每次工具调用都建立独立的 WebSocket 连接，
/// 发送单个 JSON-RPC 请求，等待响应后断开。这避免了复杂的异步
/// 响应路由，且 IDE 操作的调用频率完全在可接受范围内（单次连接 < 10ms）。
pub struct VscodeBridge {
    config: VotekBridgeConfig,
    connected: AtomicBool,
    request_id: AtomicU64,
}

impl VscodeBridge {
    /// 创建未连接的桥接实例
    pub fn new(config: VotekBridgeConfig) -> Arc<Self> {
        Arc::new(Self {
            config,
            connected: AtomicBool::new(false),
            request_id: AtomicU64::new(1),
        })
    }

    /// 获取配置引用
    pub fn config(&self) -> &VotekBridgeConfig {
        &self.config
    }

    /// 是否已通过健康检查
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    /// 健康检查：尝试连接伴生扩展（带重试），成功后标记为已连接
    pub async fn connect(&self) -> Result<(), String> {
        if self.is_connected() {
            return Ok(());
        }

        let url = self.config.ws_url();

        for attempt in 1..=MAX_RECONNECT_ATTEMPTS {
            match tokio::time::timeout(
                std::time::Duration::from_secs(CONNECT_TIMEOUT_SECS),
                connect_async(&url),
            )
            .await
            {
                Ok(Ok((ws_stream, _))) => {
                    // 连接成功，立即关闭（只做健康检查）
                    let (_write, _read) = ws_stream.split();
                    // ws_stream 会在 drop 时自动关闭
                    self.connected.store(true, Ordering::Relaxed);
                    eprintln!(
                        "[VscodeBridge] Companion health check OK (port {})",
                        self.config.port
                    );
                    return Ok(());
                }
                Ok(Err(e)) => {
                    eprintln!(
                        "[VscodeBridge] Health check {}/{} failed: {}",
                        attempt, MAX_RECONNECT_ATTEMPTS, e
                    );
                }
                Err(_) => {
                    eprintln!(
                        "[VscodeBridge] Health check {}/{} timed out",
                        attempt, MAX_RECONNECT_ATTEMPTS
                    );
                }
            }

            if attempt < MAX_RECONNECT_ATTEMPTS {
                tokio::time::sleep(std::time::Duration::from_secs(RECONNECT_INTERVAL_SECS)).await;
            }
        }

        Err(format!(
            "无法连接到 Votek Companion (端口 {}): 超过最大重试次数",
            self.config.port
        ))
    }

    /// JSON-RPC 调用核心：短连接模式
    ///
    /// 每次调用建立独立 WebSocket 连接，发送请求 → 等待响应 → 断开。
    /// 短连接简单可靠，避免了长连接的响应路由复杂性。
    /// 代价是每次调用多 1 次 TCP 握手（< 5ms），对 IDE 操作完全可接受。
    async fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        let url = self.config.ws_url();

        let (ws_stream, _) = tokio::time::timeout(
            std::time::Duration::from_secs(CONNECT_TIMEOUT_SECS),
            connect_async(&url),
        )
        .await
        .map_err(|_| "连接 Votek Companion 超时".to_string())?
        .map_err(|e| format!("连接 Votek Companion 失败: {}", e))?;

        let (mut write, mut read) = ws_stream.split();

        let id = self.request_id.fetch_add(1, Ordering::Relaxed);
        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        write
            .send(Message::Text(request.to_string().into()))
            .await
            .map_err(|e| format!("WebSocket 发送失败: {}", e))?;

        // 等待响应
        let response_raw = tokio::time::timeout(
            std::time::Duration::from_secs(CALL_TIMEOUT_SECS),
            read.next(),
        )
        .await
        .map_err(|_| format!("Votek Companion 响应超时 ({}s)", CALL_TIMEOUT_SECS))?;

        match response_raw {
            Some(Ok(Message::Text(text))) => {
                let response: Value = serde_json::from_str(&text)
                    .map_err(|e| format!("解析响应失败: {}", e))?;

                if let Some(error) = response.get("error") {
                    let msg = error["message"]
                        .as_str()
                        .unwrap_or("Unknown error");
                    let code = error["code"].as_i64().unwrap_or(-1);
                    return Err(format!("VSCode 错误 [{}]: {}", code, msg));
                }

                Ok(response["result"].clone())
            }
            Some(Ok(Message::Close(frame))) => {
                Err(format!("Votek Companion 关闭了连接: {:?}", frame))
            }
            Some(Err(e)) => Err(format!("WebSocket 读取错误: {}", e)),
            None => Err("Votek Companion 未返回响应".to_string()),
            _ => Err("Votek Companion 返回了非文本消息".to_string()),
        }
    }

    // ── 工具方法（每个对应一个 companion method） ──

    /// 获取当前活动编辑器信息
    pub async fn get_active_editor(&self) -> Result<String, String> {
        let result = self.call("getActiveEditor", json!({})).await?;
        if result.is_null() {
            Ok("无活动编辑器".to_string())
        } else {
            Ok(serde_json::to_string_pretty(&result)
                .unwrap_or_else(|_| result.to_string()))
        }
    }

    /// 获取诊断信息（错误/警告）
    pub async fn get_diagnostics(
        &self,
        file_path: Option<&str>,
    ) -> Result<String, String> {
        let params = match file_path {
            Some(fp) => json!({ "filePath": fp }),
            None => json!({}),
        };
        let result = self.call("getDiagnostics", params).await?;

        let diags: Vec<Value> = serde_json::from_value(result)
            .map_err(|e| format!("解析诊断结果失败: {}", e))?;

        if diags.is_empty() {
            return Ok("无诊断问题".to_string());
        }

        // 格式化输出
        let lines: Vec<String> = diags
            .iter()
            .map(|d| {
                let severity = d["severity"].as_str().unwrap_or("?");
                let icon = match severity {
                    "error" => "❌",
                    "warning" => "⚠️",
                    "info" => "ℹ️",
                    _ => "•",
                };
                let file = d["filePath"].as_str().unwrap_or("");
                let line = d["line"].as_u64().unwrap_or(0);
                let col = d["column"].as_u64().unwrap_or(0);
                let msg = d["message"].as_str().unwrap_or("");
                let src = d["source"].as_str().unwrap_or("");
                let code = d["code"].as_str().unwrap_or("");
                let code_part = if code.is_empty() {
                    String::new()
                } else {
                    format!(" [{}]", code)
                };
                format!(
                    "{} {}:{}:{} {}{}: {}",
                    icon, file, line, col, src, code_part, msg
                )
            })
            .collect();

        Ok(format!(
            "共 {} 个诊断问题:\n{}",
            diags.len(),
            lines.join("\n")
        ))
    }

    /// 获取打开的文件标签页
    pub async fn get_open_tabs(&self) -> Result<String, String> {
        let result = self.call("getOpenTabs", json!({})).await?;
        let tabs: Vec<Value> = serde_json::from_value(result)
            .map_err(|e| format!("解析标签页结果失败: {}", e))?;

        if tabs.is_empty() {
            return Ok("无打开的文件".to_string());
        }

        let lines: Vec<String> = tabs
            .iter()
            .map(|t| {
                let fp = t["filePath"].as_str().unwrap_or("");
                let lang = t["language"].as_str().unwrap_or("");
                let dirty = if t["isDirty"].as_bool().unwrap_or(false) {
                    " [未保存]"
                } else {
                    ""
                };
                let active = if t["isActive"].as_bool().unwrap_or(false) {
                    " ◀ 当前"
                } else {
                    ""
                };
                format!("{}{}{} ({})", fp, dirty, active, lang)
            })
            .collect();

        Ok(format!("共 {} 个打开的标签页:\n{}", tabs.len(), lines.join("\n")))
    }

    /// 导航到指定文件（可选行列号）
    pub async fn open_file(
        &self,
        file_path: &str,
        line: Option<u32>,
        column: Option<u32>,
    ) -> Result<String, String> {
        let mut params = json!({ "filePath": file_path });
        if let Some(l) = line {
            params["line"] = json!(l);
        }
        if let Some(c) = column {
            params["column"] = json!(c);
        }
        let result = self.call("openFile", params).await?;
        let success = result["success"].as_bool().unwrap_or(false);
        if success {
            let loc = match (line, column) {
                (Some(l), Some(c)) => format!(" ({}:{})", l, c),
                (Some(l), None) => format!(" (第 {} 行)", l),
                _ => String::new(),
            };
            Ok(format!("已在 IDE 中打开: {}{}", file_path, loc))
        } else {
            Err(format!("无法打开文件: {} (文件可能不存在)", file_path))
        }
    }

    /// 在编辑器中应用文本编辑
    pub async fn apply_edit(
        &self,
        file_path: &str,
        start_line: u32,
        start_col: u32,
        end_line: u32,
        end_col: u32,
        text: &str,
    ) -> Result<String, String> {
        let params = json!({
            "filePath": file_path,
            "edits": [{
                "startLine": start_line,
                "startColumn": start_col,
                "endLine": end_line,
                "endColumn": end_col,
                "text": text,
            }]
        });
        let result = self.call("applyEdit", params).await?;
        let success = result["success"].as_bool().unwrap_or(false);
        let msg = result["message"].as_str().unwrap_or("");
        if success {
            Ok(format!("编辑成功: {}", msg))
        } else {
            Err(format!("编辑失败: {}", msg))
        }
    }

    /// 获取终端输出
    pub async fn get_terminal_output(
        &self,
        name: Option<&str>,
    ) -> Result<String, String> {
        let params = match name {
            Some(n) => json!({ "name": n }),
            None => json!({}),
        };
        let result = self.call("getTerminalOutput", params).await?;
        if result.is_null() {
            return Ok("无可用终端".to_string());
        }
        let term_name = result["name"].as_str().unwrap_or("");
        let content = result["content"].as_str().unwrap_or("");
        Ok(format!("终端 [{}] 输出:\n{}", term_name, content))
    }

    /// 执行 VS Code 命令
    pub async fn execute_command(&self, command: &str, args: Option<Value>) -> Result<String, String> {
        let params = json!({
            "command": command,
            "args": args.unwrap_or(json!([])),
        });
        let result = self.call("executeCommand", params).await?;
        let success = result["success"].as_bool().unwrap_or(false);
        let output = result["result"].as_str().unwrap_or("");
        if success {
            Ok(format!("命令执行成功: {}", output))
        } else {
            Ok(format!("命令执行返回: {}", output))
        }
    }

    /// 获取工作区信息
    pub async fn get_workspace_info(&self) -> Result<String, String> {
        let result = self.call("getWorkspaceInfo", json!({})).await?;
        if result.is_null() {
            return Ok("无打开的工作区".to_string());
        }
        let name = result["name"].as_str().unwrap_or("");
        let path = result["path"].as_str().unwrap_or("");
        let file_count = result["fileCount"].as_u64().unwrap_or(0);
        let folders: Vec<&str> = result["folders"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();

        Ok(format!(
            "工作区: {} ({})\n路径: {}\n文件数: {}\n文件夹: {}",
            name,
            path,
            path,
            file_count,
            folders.join(", ")
        ))
    }

    /// 获取文件符号（函数/类/变量等）
    pub async fn get_file_symbols(&self, file_path: &str) -> Result<String, String> {
        let result = self
            .call("getFileSymbols", json!({ "filePath": file_path }))
            .await?;
        let symbols: Vec<Value> = serde_json::from_value(result)
            .map_err(|e| format!("解析符号结果失败: {}", e))?;

        if symbols.is_empty() {
            return Ok("该文件无可用符号".to_string());
        }

        let lines: Vec<String> = symbols
            .iter()
            .map(|s| {
                let kind = s["kind"].as_str().unwrap_or("?");
                let name = s["name"].as_str().unwrap_or("");
                let line = s["line"].as_u64().unwrap_or(0);
                let container = s["containerName"]
                    .as_str()
                    .map(|c| format!(" (in {})", c))
                    .unwrap_or_default();
                format!("[{}] {}{} — 第 {} 行", kind, name, container, line)
            })
            .collect();

        Ok(format!("{} 的符号:\n{}", file_path, lines.join("\n")))
    }

    // ── 新增工具方法 ─────────────────────────────────

    /// 向 VS Code 终端发送命令
    pub async fn send_to_terminal(
        &self,
        text: &str,
        terminal_name: Option<&str>,
        new_terminal: bool,
    ) -> Result<String, String> {
        let params = json!({
            "text": text,
            "terminalName": terminal_name,
            "newTerminal": new_terminal,
        });
        let result = self.call("sendToTerminal", params).await?;
        let name = result["name"].as_str().unwrap_or("terminal");
        Ok(format!("已发送到终端 [{}]", name))
    }

    /// 在工作区文件中搜索文本
    pub async fn search_in_workspace(
        &self,
        query: &str,
        include_pattern: Option<&str>,
        max_results: Option<u32>,
    ) -> Result<String, String> {
        let mut params = json!({ "query": query });
        if let Some(p) = include_pattern {
            params["include"] = json!(p);
        }
        if let Some(m) = max_results {
            params["maxResults"] = json!(m);
        }
        let result = self.call("searchInWorkspace", params).await?;
        let results: Vec<Value> = serde_json::from_value(result)
            .map_err(|e| format!("解析搜索结果失败: {}", e))?;

        if results.is_empty() {
            return Ok(format!("在项目中未找到 \"{}\" 的匹配项", query));
        }

        let lines: Vec<String> = results
            .iter()
            .map(|r| {
                let file = r["file"].as_str().unwrap_or("");
                let line = r["line"].as_u64().unwrap_or(0);
                let col = r["column"].as_u64().unwrap_or(0);
                let preview = r["preview"].as_str().unwrap_or("");
                format!("{}:{}:{}  {}", file, line, col, preview.trim())
            })
            .collect();

        Ok(format!("在 {} 个文件中找到 \"{}\":\n{}", results.len(), query, lines.join("\n")))
    }

    /// 获取文件的 Git 差异
    pub async fn get_file_diff(&self, file_path: &str) -> Result<String, String> {
        let result = self
            .call("getFileDiff", json!({ "filePath": file_path }))
            .await?;

        if result.is_null() {
            return Ok(format!("文件无未暂存的更改: {}", file_path));
        }

        let diff = result["diff"].as_str().unwrap_or("");
        if diff.is_empty() {
            return Ok(format!("文件无更改: {}", file_path));
        }

        Ok(format!("文件差异 {}:\n{}", file_path, diff))
    }

    // ── 工具注册 ──

    /// 将所有 VSCode 工具注册到 ToolRegistry
    ///
    /// 名称约定：`vscode_<method>`（避免与 `native_*` 和 `mcp::*` 冲突）
    pub fn register_tools(self: &Arc<Self>, registry: &mut ToolRegistry) {
        let bridge = Arc::clone(self);

        // 1. vscode_get_active_editor
        {
            let b = Arc::clone(&bridge);
            registry.register_native(
                "vscode_get_active_editor",
                "获取 VS Code 当前活动编辑器的信息：文件路径、语言、光标位置（行/列）、选中文本、总行数。如果无活动编辑器返回提示。",
                json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
                Arc::new(move |_args: String| {
                    let b = Arc::clone(&b);
                    Box::pin(async move { b.get_active_editor().await })
                }),
            );
        }

        // 2. vscode_get_diagnostics
        {
            let b = Arc::clone(&bridge);
            registry.register_native(
                "vscode_get_diagnostics",
                "获取 VS Code 工作区中所有文件的诊断信息（错误、警告、提示）。可选传入 filePath 过滤特定文件。返回格式化的诊断列表，包含严重级别、位置、来源和错误代码。",
                json!({
                    "type": "object",
                    "properties": {
                        "filePath": {
                            "type": "string",
                            "description": "可选：只获取指定文件的诊断信息。不传则返回所有文件。"
                        }
                    },
                    "required": []
                }),
                Arc::new(move |args: String| {
                    let b = Arc::clone(&b);
                    Box::pin(async move {
                        let v: Value = serde_json::from_str(&args).unwrap_or(json!({}));
                        let fp = v["filePath"].as_str();
                        b.get_diagnostics(fp).await
                    })
                }),
            );
        }

        // 3. vscode_get_open_tabs
        {
            let b = Arc::clone(&bridge);
            registry.register_native(
                "vscode_get_open_tabs",
                "获取 VS Code 中所有打开的文件标签页列表，包括是否已修改（未保存）和是否为当前活动标签页。",
                json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
                Arc::new(move |_args: String| {
                    let b = Arc::clone(&b);
                    Box::pin(async move { b.get_open_tabs().await })
                }),
            );
        }

        // 4. vscode_open_file
        {
            let b = Arc::clone(&bridge);
            registry.register_native(
                "vscode_open_file",
                "在 VS Code 中打开指定文件，并可选导航到指定行和列。用于在 IDE 中定位代码位置。",
                json!({
                    "type": "object",
                    "properties": {
                        "filePath": {
                            "type": "string",
                            "description": "要打开的文件的绝对路径"
                        },
                        "line": {
                            "type": "integer",
                            "description": "可选：跳转到第几行（1-based）"
                        },
                        "column": {
                            "type": "integer",
                            "description": "可选：跳转到第几列（1-based）"
                        }
                    },
                    "required": ["filePath"]
                }),
                Arc::new(move |args: String| {
                    let b = Arc::clone(&b);
                    Box::pin(async move {
                        let v: Value = serde_json::from_str(&args)
                            .map_err(|e| format!("参数解析失败: {}", e))?;
                        let fp = v["filePath"].as_str()
                            .ok_or_else(|| "缺少 filePath 参数".to_string())?;
                        let line = v["line"].as_u64().map(|n| n as u32);
                        let col = v["column"].as_u64().map(|n| n as u32);
                        b.open_file(fp, line, col).await
                    })
                }),
            );
        }

        // 5. vscode_apply_edit
        {
            let b = Arc::clone(&bridge);
            registry.register_native(
                "vscode_apply_edit",
                "在 VS Code 编辑器中应用文本编辑。指定文件路径、行范围（行列均为 1-based）和替换文本。编辑会自动保存。",
                json!({
                    "type": "object",
                    "properties": {
                        "filePath": {
                            "type": "string",
                            "description": "要编辑的文件的绝对路径"
                        },
                        "startLine": {
                            "type": "integer",
                            "description": "编辑起始行（1-based）"
                        },
                        "startColumn": {
                            "type": "integer",
                            "description": "编辑起始列（1-based）"
                        },
                        "endLine": {
                            "type": "integer",
                            "description": "编辑结束行（1-based）"
                        },
                        "endColumn": {
                            "type": "integer",
                            "description": "编辑结束列（1-based）"
                        },
                        "text": {
                            "type": "string",
                            "description": "替换文本"
                        }
                    },
                    "required": ["filePath", "startLine", "startColumn", "endLine", "endColumn", "text"]
                }),
                Arc::new(move |args: String| {
                    let b = Arc::clone(&b);
                    Box::pin(async move {
                        let v: Value = serde_json::from_str(&args)
                            .map_err(|e| format!("参数解析失败: {}", e))?;
                        let fp = v["filePath"].as_str()
                            .ok_or_else(|| "缺少 filePath".to_string())?;
                        let sl = v["startLine"].as_u64().unwrap_or(1) as u32;
                        let sc = v["startColumn"].as_u64().unwrap_or(1) as u32;
                        let el = v["endLine"].as_u64().unwrap_or(1) as u32;
                        let ec = v["endColumn"].as_u64().unwrap_or(1) as u32;
                        let text = v["text"].as_str().unwrap_or("");
                        b.apply_edit(fp, sl, sc, el, ec, text).await
                    })
                }),
            );
        }

        // 6. vscode_get_terminal_output
        {
            let b = Arc::clone(&bridge);
            registry.register_native(
                "vscode_get_terminal_output",
                "获取 VS Code 集成终端的输出内容。可选传入终端名称，不传则获取活动终端。",
                json!({
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "可选：终端名称。不传则获取当前活动终端。"
                        }
                    },
                    "required": []
                }),
                Arc::new(move |args: String| {
                    let b = Arc::clone(&b);
                    Box::pin(async move {
                        let v: Value = serde_json::from_str(&args).unwrap_or(json!({}));
                        let name = v["name"].as_str();
                        b.get_terminal_output(name).await
                    })
                }),
            );
        }

        // 7. vscode_execute_command
        {
            let b = Arc::clone(&bridge);
            registry.register_native(
                "vscode_execute_command",
                "执行 VS Code 内置命令。例如格式化文档、显示 Git 面板等。注意：请谨慎使用，避免破坏性命令。",
                json!({
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "VS Code 命令 ID。例如 'editor.action.formatDocument' 或 'workbench.view.scm'"
                        }
                    },
                    "required": ["command"]
                }),
                Arc::new(move |args: String| {
                    let b = Arc::clone(&b);
                    Box::pin(async move {
                        let v: Value = serde_json::from_str(&args)
                            .map_err(|e| format!("参数解析失败: {}", e))?;
                        let cmd = v["command"].as_str()
                            .ok_or_else(|| "缺少 command 参数".to_string())?;
                        b.execute_command(cmd, None).await
                    })
                }),
            );
        }

        // 8. vscode_get_workspace_info
        {
            let b = Arc::clone(&bridge);
            registry.register_native(
                "vscode_get_workspace_info",
                "获取 VS Code 当前工作区信息：名称、路径、文件数量、文件夹列表。",
                json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
                Arc::new(move |_args: String| {
                    let b = Arc::clone(&b);
                    Box::pin(async move { b.get_workspace_info().await })
                }),
            );
        }

        // 9. vscode_get_file_symbols
        {
            let b = Arc::clone(&bridge);
            registry.register_native(
                "vscode_get_file_symbols",
                "获取指定文件的代码符号（函数、类、变量等定义），包含名称、类型、行列位置。需要语言服务器支持。",
                json!({
                    "type": "object",
                    "properties": {
                        "filePath": {
                            "type": "string",
                            "description": "文件的绝对路径"
                        }
                    },
                    "required": ["filePath"]
                }),
                Arc::new(move |args: String| {
                    let b = Arc::clone(&b);
                    Box::pin(async move {
                        let v: Value = serde_json::from_str(&args)
                            .map_err(|e| format!("参数解析失败: {}", e))?;
                        let fp = v["filePath"].as_str()
                            .ok_or_else(|| "缺少 filePath 参数".to_string())?;
                        b.get_file_symbols(fp).await
                    })
                }),
            );
        }

        // 10. vscode_send_to_terminal — 向 VS Code 终端发送命令
        {
            let b = Arc::clone(&bridge);
            registry.register_native(
                "vscode_send_to_terminal",
                "向 VS Code 集成终端发送命令。可以发送到现有终端或创建新终端。例如编译命令、运行测试等。",
                json!({
                    "type": "object",
                    "properties": {
                        "text": {
                            "type": "string",
                            "description": "要发送到终端的命令文本"
                        },
                        "terminalName": {
                            "type": "string",
                            "description": "可选：终端名称。不传则发送到活动终端。"
                        },
                        "newTerminal": {
                            "type": "boolean",
                            "description": "是否创建新终端（默认 false）"
                        }
                    },
                    "required": ["text"]
                }),
                Arc::new(move |args: String| {
                    let b = Arc::clone(&b);
                    Box::pin(async move {
                        let v: Value = serde_json::from_str(&args)
                            .map_err(|e| format!("参数解析失败: {}", e))?;
                        let text = v["text"].as_str()
                            .ok_or_else(|| "缺少 text 参数".to_string())?;
                        let name = v["terminalName"].as_str();
                        let new_term = v["newTerminal"].as_bool().unwrap_or(false);
                        b.send_to_terminal(text, name, new_term).await
                    })
                }),
            );
        }

        // 11. vscode_search_in_workspace — 在工作区文件中搜索文本
        {
            let b = Arc::clone(&bridge);
            registry.register_native(
                "vscode_search_in_workspace",
                "在 VS Code 工作区所有文件中搜索指定文本。支持 include glob 模式过滤文件类型。返回匹配的文件路径、行号、列号和预览片段。",
                json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "要搜索的文本"
                        },
                        "include": {
                            "type": "string",
                            "description": "可选：文件包含模式（glob），例如 '*.rs' 只搜索 Rust 文件"
                        },
                        "maxResults": {
                            "type": "integer",
                            "description": "可选：最大结果数（默认 50）"
                        }
                    },
                    "required": ["query"]
                }),
                Arc::new(move |args: String| {
                    let b = Arc::clone(&b);
                    Box::pin(async move {
                        let v: Value = serde_json::from_str(&args)
                            .map_err(|e| format!("参数解析失败: {}", e))?;
                        let query = v["query"].as_str()
                            .ok_or_else(|| "缺少 query 参数".to_string())?;
                        let include = v["include"].as_str();
                        let max_results = v["maxResults"].as_u64().map(|n| n as u32);
                        b.search_in_workspace(query, include, max_results).await
                    })
                }),
            );
        }

        // 12. vscode_get_file_diff — 获取文件的 Git 差异
        {
            let b = Arc::clone(&bridge);
            registry.register_native(
                "vscode_get_file_diff",
                "获取指定文件的 Git 差异（未暂存的更改）。返回标准 diff 格式，显示添加/删除/修改的行。需要 Git 仓库和 VS Code Git 扩展支持。",
                json!({
                    "type": "object",
                    "properties": {
                        "filePath": {
                            "type": "string",
                            "description": "文件的绝对路径"
                        }
                    },
                    "required": ["filePath"]
                }),
                Arc::new(move |args: String| {
                    let b = Arc::clone(&b);
                    Box::pin(async move {
                        let v: Value = serde_json::from_str(&args)
                            .map_err(|e| format!("参数解析失败: {}", e))?;
                        let fp = v["filePath"].as_str()
                            .ok_or_else(|| "缺少 filePath 参数".to_string())?;
                        b.get_file_diff(fp).await
                    })
                }),
            );
        }
    }
}

// ── 环境变量注入（供 code_server.rs 调用） ──

/// Bridge 环境变量名（传递给 code-server 子进程）
pub const ENV_BRIDGE_PORT: &str = "VOTEK_BRIDGE_PORT";
pub const ENV_BRIDGE_TOKEN: &str = "VOTEK_BRIDGE_TOKEN";

/// 将 bridge 配置注入到 std::process::Command 的环境变量中
pub fn inject_env(command: &mut std::process::Command, config: &VotekBridgeConfig) {
    command.env(ENV_BRIDGE_PORT, config.port.to_string());
    command.env(ENV_BRIDGE_TOKEN, &config.token);
}

// ── 全局配置存储（lib.rs ↔ code_server.rs 之间共享） ──

/// 模块级静态：bridge 配置
///
/// `lib.rs` 在应用启动时设置，`code_server.rs` 在 spawn 时读取以注入环境变量。
/// 使用 tokio::sync::Mutex 而非 std::sync::Mutex，避免在异步上下文中阻塞。
static BRIDGE_CONFIG: tokio::sync::Mutex<Option<VotekBridgeConfig>> =
    tokio::sync::Mutex::const_new(None);

/// 设置全局 bridge 配置（由 lib.rs 在应用启动时调用一次）
pub async fn set_global_config(config: VotekBridgeConfig) {
    *BRIDGE_CONFIG.lock().await = Some(config);
}

/// 获取全局 bridge 配置的克隆（由 code_server.rs 在 spawn 时读取）
pub async fn get_global_config() -> Option<VotekBridgeConfig> {
    BRIDGE_CONFIG.lock().await.clone()
}
