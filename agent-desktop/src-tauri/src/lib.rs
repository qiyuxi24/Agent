mod agent_loop;
mod browser;
mod code_server;
mod error_codes;
mod ide;
mod mcp;
mod pet;
mod plugins;
mod rag;
mod sandbox;
mod skills;
mod tools;
mod vscode_bridge;

use error_codes::McpError;
use futures::StreamExt;
use mcp::McpServerConfig;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::{Arc, LazyLock};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;
use std::collections::HashMap;
use vscode_bridge::VscodeBridge;
use std::fs;
use std::time::Instant;
use tools::ToolRegistry;

// ── 全局常量 ──
/// Agent Loop 最大迭代轮数
const MAX_ITERATIONS: usize = 200;
/// LLM 请求温度
const DEFAULT_TEMPERATURE: f64 = 0.7;
/// LLM 最大输出 token 数
const DEFAULT_MAX_TOKENS: u32 = 4096;
/// 取消流注册 key
const CANCEL_STREAM_KEY: &str = "chat";

/// 统一 User-Agent（从 Cargo.toml 版本号手动同步：搜索 "USER_AGENT" 可找到所有位置）
pub(crate) const USER_AGENT: &str = "votek/0.3.0";

/// 复用 reqwest Client（内建连接池），避免每次 LLM 调用重新建立 TCP 连接
///
/// 设置连接超时 10s + 读取超时 120s（LLM 流式 token 可长达数分钟）。
/// 注：不使用整体 request timeout，因为 LLM 流式响应可能持续很久。
static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .read_timeout(std::time::Duration::from_secs(120))
        .build()
        .expect("构建 HTTP Client 失败，请检查网络环境")
});

/// 构建用于市场抓取的 HTTP Client（统一 user_agent + timeout）
pub(crate) fn build_market_client(timeout_secs: u64) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .build()
        .map_err(|e| format!("构建 HTTP 客户端失败: {e}"))
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChatRequest {
    pub api_base: String,
    pub api_key: String,
    pub model: String,
    pub messages: Vec<ChatMessage>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChatMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// DeepSeek 思考链 / Claude thinking content（用于多轮工具调用上下文保持）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: "system".into(), content: Some(content.into()), reasoning_content: None, tool_calls: None, tool_call_id: None }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: "user".into(), content: Some(content.into()), reasoning_content: None, tool_calls: None, tool_call_id: None }
    }
    pub fn tool(tool_call_id: String, content: impl Into<String>) -> Self {
        Self { role: "tool".into(), content: Some(content.into()), reasoning_content: None, tool_calls: None, tool_call_id: Some(tool_call_id) }
    }
    pub fn assistant(content: String, reasoning: String, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: "assistant".into(),
            content: if content.is_empty() { None } else { Some(content) },
            reasoning_content: if reasoning.is_empty() { None } else { Some(reasoning) },
            tool_calls: if tool_calls.is_empty() { None } else { Some(tool_calls) },
            tool_call_id: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub arguments: String,
}

/// 流式 token（普通文本增量）
#[derive(Debug, Clone, Serialize)]
pub struct StreamToken {
    pub token: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StreamError {
    pub error: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StreamDone;

/// LLM 调用失败重试通知：告诉前端清空之前的 token 缓冲，准备接收新一轮流式输出
#[derive(Debug, Clone, Serialize)]
pub struct StreamRetry {
    pub attempt: u32,
}

/// 最终答案开始：agent 多轮循环结束、进入面向用户的正式回答阶段
/// （区分「中间轮的推理草稿」与「终止轮的最终答案」，前端可据此收起思考面板）
#[derive(Debug, Clone, Serialize)]
pub struct FinalAnswerStart;

/// 思考过程：开始（DeepSeek/Claude 的 reasoning_content 阶段启动）
#[derive(Debug, Clone, Serialize)]
pub struct ThinkingStart;

/// 思考过程：增量文本片段
#[derive(Debug, Clone, Serialize)]
pub struct ThinkingDelta {
    pub delta: String,
}

/// 思考过程：结束（统计信息）
#[derive(Debug, Clone, Serialize)]
pub struct ThinkingStop {
    /// 估算 token 数（约 1 token ≈ 4 字符，中英文混合）
    pub tokens: u64,
    /// 思考耗时（毫秒）
    pub duration_ms: u64,
}

/// 工具调用开始事件（前端可渲染「正在调用 xxx」）
#[derive(Debug, Clone, Serialize)]
pub struct ToolCallEvent {
    pub name: String,
    pub arguments: String,
}

/// 工具调用结果事件
#[derive(Debug, Clone, Serialize)]
pub struct ToolResultEvent {
    pub name: String,
    pub result: String,
    /// 是否执行成功
    #[serde(rename = "isError")]
    pub is_error: bool,
    /// 错误码（成功时为 None）
    pub error_code: Option<String>,
    /// 错误分类（如 TIMEOUT、TOOL_ERROR）
    pub error_category: Option<String>,
    /// 建议操作：retry | reconnect | none
    pub suggested_action: Option<String>,
}

/// 把内部 ChatMessage 转为 OpenAI 接口需要的 JSON（精确控制字段，避免 null 陷阱）
fn msg_to_value(m: &ChatMessage) -> Value {
    let mut map = serde_json::Map::new();
    map.insert("role".to_string(), Value::String(m.role.clone()));
    match &m.content {
        Some(c) => map.insert("content".to_string(), Value::String(c.clone())),
        None => map.insert("content".to_string(), Value::Null),
    };
    // DeepSeek/Claude：工具调用场景需要回传 reasoning_content 以保持思考上下文
    if let Some(rc) = &m.reasoning_content {
        if !rc.is_empty() {
            map.insert("reasoning_content".to_string(), Value::String(rc.clone()));
        }
    }
    if let Some(tcs) = &m.tool_calls {
        let arr: Vec<Value> = tcs
            .iter()
            .map(|tc| {
                serde_json::json!({
                    "id": tc.id,
                    "type": "function",
                    "function": { "name": tc.name, "arguments": tc.arguments }
                })
            })
            .collect();
        map.insert("tool_calls".to_string(), Value::Array(arr));
    }
    if let Some(tid) = &m.tool_call_id {
        map.insert("tool_call_id".to_string(), Value::String(tid.clone()));
    }
    Value::Object(map)
}

/// 累积流式工具调用（OpenAI 分片推送 tool_calls）
#[derive(Default)]
struct ToolCallAcc {
    id: String,
    name: String,
    arguments: String,
}

/// 准备 agent loop 上下文：聚合所有工具（MCP + 原生） + 注入 Skills prompt + 准备 messages。
async fn prepare_loop_messages(
    app: &AppHandle,
    registry: &ToolRegistry,
    request: &ChatRequest,
) -> (Vec<ChatMessage>, Vec<Value>) {
    let tools = registry.all_tools().await;
    eprintln!("[chat_stream] 已准备 {} 个工具, Skills prompt={} bytes",
        tools.len(),
        skills::get_active_system_prompt(app).len());

    let skills_prompt = skills::get_active_system_prompt(app);
    let mut messages = request.messages.clone();
    if !skills_prompt.is_empty() {
        let prompt_len = skills_prompt.len();
        if let Some(first) = messages.first_mut() {
            if first.role == "system" {
                let existing = first.content.clone().unwrap_or_default();
                first.content = Some(format!("{skills_prompt}\n\n{existing}"));
            } else {
                messages.insert(0, ChatMessage::system(&skills_prompt));
            }
        } else {
            messages.insert(0, ChatMessage::system(&skills_prompt));
        }
        eprintln!("[chat_stream] 已注入 {} 字节的 Skills system prompt", prompt_len);
    }

    (messages, tools)
}

#[tauri::command]
async fn chat_stream(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    request: ChatRequest,
) -> Result<(), String> {
    eprintln!("[chat_stream] 收到请求 model={} api_base={} messages={}",
        request.model, request.api_base, request.messages.len());

    // 1. 准备消息和工具（走统一 ToolRegistry）
    let (messages, tools) = prepare_loop_messages(&app, &state.tools, &request).await;

    // 2. 注册取消信号
    let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);
    {
        let mut streams = state.active_streams.lock().await;
        streams.insert(CANCEL_STREAM_KEY.to_string(), cancel_tx);
    }

    // 3. 组装 agent loop 上下文
    let llm = agent_loop::RealLlmClient {
        app: app.clone(),
        request: request.clone(),
    };
    let executor = agent_loop::RegistryToolExecutor { registry: &state.tools };
    let config = agent_loop::AgentLoopConfig {
        max_iterations: MAX_ITERATIONS,
        ..Default::default()
    };
    let ctx = agent_loop::LoopContext {
        app: Some(&app),
        config,
        initial_messages: messages,
        tools,
        llm: &llm,
        executor: &executor,
        cancel: &mut cancel_rx,
    };

    // 4. 运行 loop + 清理
    let result = agent_loop::run_agent_loop(ctx).await;
    cleanup(&app, &state).await;
    result
}

/// 调用一次 LLM 流式接口，实时推送思考 token（reasoning_content），并收集 content / tool_calls
///
/// `stream_content` 控制普通 `content` 的实时推送策略：
/// - `true`：实时以 `stream-token` 推送到答案框（逐字流式）。
/// - `false`：仅静默累积，由 agent loop 判定本轮是中间轮还是终止轮后再决定归属。
///
/// 返回值：(content, reasoning_content, tool_calls)
pub(crate) async fn run_completion(
    app: &AppHandle,
    request: &ChatRequest,
    messages: &[ChatMessage],
    tools: &[Value],
    cancel_rx: &mut tokio::sync::watch::Receiver<bool>,
    stream_content: bool,
) -> Result<(String, String, Vec<ToolCall>), String> {
    let url = format!("{}/chat/completions", request.api_base.trim_end_matches('/'));
    eprintln!("[LLM] POST {} (model={} messages={} tools={})",
        url, request.model, messages.len(), tools.len());

    let mut body = serde_json::json!({
        "model": request.model,
        "messages": messages.iter().map(msg_to_value).collect::<Vec<_>>(),
        "stream": true,
        "temperature": DEFAULT_TEMPERATURE,
        "max_tokens": DEFAULT_MAX_TOKENS,
    });
    if !tools.is_empty() {
        body["tools"] = Value::Array(tools.to_vec());
    }

    let response = HTTP_CLIENT
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", request.api_key))
        .json(&body)
        .send()
        .await
        .map_err(|e| McpError::llm_network(&e.to_string()).to_string())?;

    eprintln!("[LLM] 响应状态: {} {}", response.status().as_u16(), response.status().canonical_reason().unwrap_or(""));
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.map_err(|e| {
            McpError::llm_api_error(status.as_u16(), &format!("(无法读取响应体: {e})")).to_string()
        })?;
        let short: String = body.chars().take(300).collect();
        return Err(McpError::llm_api_error(status.as_u16(), &short).to_string());
    }

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut content = String::new();
    let mut thinking = String::new();
    let mut tool_accs: Vec<ToolCallAcc> = Vec::new();

    // 思考阶段计时
    let thinking_start = Instant::now();
    let mut thinking_started = false;

    loop {
        let next_chunk = tokio::select! {
            chunk = stream.next() => chunk,
            _ = cancel_rx.changed() => {
                // 如果正在思考中被取消，也要发 thinking-stop
                if thinking_started {
                    let _ = app.emit("thinking-stop", ThinkingStop {
                        tokens: (thinking.len() as u64 / 4).max(1),
                        duration_ms: thinking_start.elapsed().as_millis() as u64,
                    });
                }
                return Ok((content, thinking, Vec::new()));
            }
        };

        match next_chunk {
            Some(Ok(bytes)) => {
                let text = String::from_utf8_lossy(&bytes);
                buffer.push_str(&text);

                while let Some(pos) = buffer.find('\n') {
                    let line = buffer[..pos].trim().to_string();
                    buffer.drain(..=pos);

                    if line.is_empty() {
                        continue;
                    }

                    if let Some(data) = line.strip_prefix("data: ") {
                        if data == "[DONE]" {
                            if thinking_started {
                                let _ = app.emit("thinking-stop", ThinkingStop {
                                    tokens: (thinking.len() as u64 / 4).max(1),
                                    duration_ms: thinking_start.elapsed().as_millis() as u64,
                                });
                            }
                            return Ok((content, thinking, finalize_tool_calls(tool_accs)));
                        }

                        if let Ok(parsed) = serde_json::from_str::<Value>(data) {
                            let delta = &parsed["choices"][0]["delta"];

                            // 1) 检测 reasoning_content（DeepSeek 思考链）
                            if let Some(reasoning) = delta["reasoning_content"].as_str() {
                                if !reasoning.is_empty() {
                                    if !thinking_started {
                                        thinking_started = true;
                                        let _ = app.emit("thinking-start", ThinkingStart);
                                    }
                                    thinking.push_str(reasoning);
                                    let _ = app.emit("thinking-delta", ThinkingDelta {
                                        delta: reasoning.to_string(),
                                    });
                                }
                                // reasoning_content 和 content 互斥，跳过后续 content 检测
                                continue;
                            }

                            // 2) 普通 content token
                            if let Some(token) = delta["content"].as_str() {
                                // 首个 content token 到达 = 思考阶段结束
                                if thinking_started {
                                    thinking_started = false;
                                    let _ = app.emit("thinking-stop", ThinkingStop {
                                        tokens: (thinking.len() as u64 / 4).max(1),
                                        duration_ms: thinking_start.elapsed().as_millis() as u64,
                                    });
                                }
                                if !token.is_empty() {
                                    content.push_str(token);
                                    // 仅在 stream_content=true（chat 模式）时实时推到答案框；
                                    // agent 模式静默累积，交由 agent loop 判定归属后回放。
                                    if stream_content {
                                        let _ = app.emit(
                                            "stream-token",
                                            StreamToken {
                                                token: token.to_string(),
                                            },
                                        );
                                    }
                                }
                            }

                            // 3) 累积分片 tool_calls
                            if let Some(tcs) = delta["tool_calls"].as_array() {
                                // tool_calls 出现时思考也结束了
                                if thinking_started {
                                    thinking_started = false;
                                    let _ = app.emit("thinking-stop", ThinkingStop {
                                        tokens: (thinking.len() as u64 / 4).max(1),
                                        duration_ms: thinking_start.elapsed().as_millis() as u64,
                                    });
                                }
                                for tc in tcs {
                                    let index = tc["index"].as_u64().unwrap_or(0) as usize;
                                    if index >= tool_accs.len() {
                                        tool_accs.resize_with(index + 1, ToolCallAcc::default);
                                    }
                                    let acc = &mut tool_accs[index];
                                    if let Some(id) = tc["id"].as_str() {
                                        acc.id = id.to_string();
                                    }
                                    if let Some(name) = tc["function"]["name"].as_str() {
                                        acc.name = name.to_string();
                                    }
                                    if let Some(args) = tc["function"]["arguments"].as_str() {
                                        acc.arguments.push_str(args);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Some(Err(e)) => {
                // 错误时也结束思考
                if thinking_started {
                    let _ = app.emit("thinking-stop", ThinkingStop {
                        tokens: (thinking.len() as u64 / 4).max(1),
                        duration_ms: thinking_start.elapsed().as_millis() as u64,
                    });
                }
                let _ = app.emit(
                    "stream-error",
                    StreamError {
                        error: McpError::llm_stream_err(&e.to_string()).to_string(),
                    },
                );
                break;
            }
            None => break,
        }
    }

    if thinking_started {
        let _ = app.emit("thinking-stop", ThinkingStop {
            tokens: (thinking.len() as u64 / 4).max(1),
            duration_ms: thinking_start.elapsed().as_millis() as u64,
        });
    }
    Ok((content, thinking, finalize_tool_calls(tool_accs)))
}

/// 从累积器中产出最终的 ToolCall 列表（忽略没有名字的无效项）
fn finalize_tool_calls(accs: Vec<ToolCallAcc>) -> Vec<ToolCall> {
    accs.into_iter()
        .filter(|a| !a.name.is_empty())
        .map(|a| ToolCall {
            id: a.id,
            name: a.name,
            arguments: a.arguments,
        })
        .collect()
}

async fn cleanup(_app: &AppHandle, state: &tauri::State<'_, AppState>) {
    // 不再重复 emit stream-done：run_agent_loop 的每个出口都已经 emit 过
    let mut streams = state.active_streams.lock().await;
    streams.remove(CANCEL_STREAM_KEY);
    eprintln!("[chat_stream] 清理完毕，stream-done 已由 agent loop 发出");
}

#[tauri::command]
async fn cancel_chat(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let streams = state.active_streams.lock().await;
    if let Some(cancel) = streams.get(CANCEL_STREAM_KEY) {
        let _ = cancel.send(true);
    }
    Ok(())
}

// ===================== 工作空间 + 沙箱命令 =====================

fn gen_id() -> String {
    use rand::Rng;
    let r: String = rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(12)
        .map(char::from)
        .collect();
    format!("ws-{r}")
}

/// 创建新工作空间（选择本地文件夹即创建沙箱）
#[tauri::command]
async fn workspace_create(
    state: tauri::State<'_, AppState>,
    name: String,
    path: String,
) -> Result<sandbox::SandboxInfo, String> {
    let root = std::path::Path::new(&path);
    let id = gen_id();
    let mut mgr = state.sandbox_manager.lock().await;
    let info = mgr.create(&id, &name, root).map_err(|e| e.to_string())?;
    // 自动设为当前工作空间
    *state.current_workspace_id.lock().await = Some(id);
    Ok(info)
}

/// 列出所有工作空间
#[tauri::command]
async fn workspace_list(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<sandbox::SandboxInfo>, String> {
    let mgr = state.sandbox_manager.lock().await;
    Ok(mgr.list())
}

/// 删除工作空间（不删用户文件，仅清理内部 .votek-sandbox）
#[tauri::command]
async fn workspace_remove(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let mut mgr = state.sandbox_manager.lock().await;
    mgr.remove(&id).map_err(|e| e.to_string())?;
    let mut cur = state.current_workspace_id.lock().await;
    if cur.as_deref() == Some(&id) {
        *cur = None;
    }
    Ok(())
}

/// 设置当前活跃工作空间
#[tauri::command]
async fn workspace_set_current(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    *state.current_workspace_id.lock().await = Some(id);
    Ok(())
}

/// 获取当前活跃工作空间信息
#[tauri::command]
async fn workspace_get_current(
    state: tauri::State<'_, AppState>,
) -> Result<Option<sandbox::SandboxInfo>, String> {
    let cur = state.current_workspace_id.lock().await;
    let mgr = state.sandbox_manager.lock().await;
    match cur.as_deref() {
        Some(id) => {
            let sb = mgr.get(id).ok_or_else(|| "工作空间不存在".to_string())?;
            Ok(Some(sandbox::SandboxInfo {
                id: sb.id.clone(),
                root: sb.root.display().to_string(),
                name: sb.root.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default(),
            }))
        }
        None => Ok(None),
    }
}

// ── 沙箱操作（作用在当前工作空间的沙箱内） ──

/// 从 AppState 获取当前沙箱 ID
async fn resolve_sandbox_id(state: &AppState) -> Result<String, String> {
    state.current_workspace_id.lock().await.clone()
        .ok_or_else(|| "没有选中工作空间，请先在侧边栏创建工作空间".to_string())
}

#[tauri::command]
async fn sandbox_read_file(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<String, String> {
    let sid = resolve_sandbox_id(&state).await?;
    let mgr = state.sandbox_manager.lock().await;
    mgr.read_file(&sid, &path).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn sandbox_write_file(
    state: tauri::State<'_, AppState>,
    path: String,
    content: String,
) -> Result<(), String> {
    let sid = resolve_sandbox_id(&state).await?;
    let mgr = state.sandbox_manager.lock().await;
    mgr.write_file(&sid, &path, &content).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn sandbox_delete_file(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<(), String> {
    let sid = resolve_sandbox_id(&state).await?;
    let mgr = state.sandbox_manager.lock().await;
    mgr.delete_file(&sid, &path).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn sandbox_create_dir(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<(), String> {
    let sid = resolve_sandbox_id(&state).await?;
    let mgr = state.sandbox_manager.lock().await;
    mgr.create_dir(&sid, &path).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn sandbox_list_dir(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<Vec<sandbox::FileEntry>, String> {
    let sid = resolve_sandbox_id(&state).await?;
    let mgr = state.sandbox_manager.lock().await;
    mgr.list_dir(&sid, &path).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn sandbox_file_tree(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<sandbox::FileEntry>, String> {
    let sid = resolve_sandbox_id(&state).await?;
    let mgr = state.sandbox_manager.lock().await;
    mgr.file_tree(&sid, 200).await.map_err(|e| e.to_string())
}

// ===================== MCP 管理命令 =====================

#[tauri::command]
async fn mcp_connect(
    state: tauri::State<'_, AppState>,
    config: McpServerConfig,
) -> Result<usize, String> {
    state.tools.mcp().connect(config).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn mcp_disconnect(
    state: tauri::State<'_, AppState>,
    name: String,
) -> Result<(), String> {
    state.tools.mcp().disconnect(&name).await
}

#[tauri::command]
async fn mcp_list_servers(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<mcp::McpServerInfo>, String> {
    Ok(state.tools.mcp().list_servers().await)
}

#[tauri::command]
async fn mcp_list_tools(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<mcp::McpTool>, String> {
    let servers = state.tools.mcp().servers.lock().await;
    let mut out = Vec::new();
    for client in servers.values() {
        for t in &client.tools {
            out.push(t.clone());
        }
    }
    Ok(out)
}

#[tauri::command]
async fn mcp_call_tool(
    state: tauri::State<'_, AppState>,
    server: String,
    tool: String,
    args: String,
) -> Result<String, String> {
    let namespaced = format!("{}::{}", server, tool);
    state
        .tools
        .mcp()
        .call_namespaced(&namespaced, &args)
        .await
        .map_err(|e| e.to_string())
}

/// 获取指定 MCP 服务器的 stderr 日志（最近 50 行）
#[tauri::command]
async fn mcp_server_stderr(
    state: tauri::State<'_, AppState>,
    name: String,
) -> Result<Vec<String>, String> {
    Ok(state.tools.mcp().get_stderr(&name).await)
}

/// 重连指定的 MCP 服务器（使用之前保存的配置）
#[tauri::command]
async fn mcp_reconnect(
    state: tauri::State<'_, AppState>,
    name: String,
) -> Result<usize, String> {
    state.tools.mcp().reconnect(&name).await.map_err(|e| e.to_string())
}

/// 执行 MCP 服务器健康检查，返回已断开的服务器名称列表
/// auto_reconnect: 是否自动重连已断开的服务器
#[tauri::command]
async fn mcp_health_check(
    state: tauri::State<'_, AppState>,
    auto_reconnect: Option<bool>,
) -> Result<Vec<String>, String> {
    Ok(state.tools.mcp().health_check(auto_reconnect.unwrap_or(false)).await)
}

/// 清空 MCP 工具调用缓存
#[tauri::command]
async fn mcp_clear_cache(
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    state.tools.mcp().clear_cache().await;
    Ok(())
}

pub struct AppState {
    active_streams: Mutex<HashMap<String, tokio::sync::watch::Sender<bool>>>,
    pub tools: ToolRegistry,
    pub rag: Arc<rag::RagManager>,
    pub pet: pet::PetManager,
    /// VSCode Bridge：与 code-server 中 Votek Companion 扩展通信
    pub vscode_bridge: Arc<vscode_bridge::VscodeBridge>,
    /// 沙箱管理器：管理所有工作空间沙箱
    pub sandbox_manager: Mutex<sandbox::SandboxManager>,
    /// 当前活跃工作空间 ID（内存状态，重启丢失，后续持久化到 store.json）
    pub current_workspace_id: Mutex<Option<String>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
    .manage({
        let mut registry = ToolRegistry::new();
        let rag_manager = Arc::new(rag::RagManager::new());

        // 注册原生工具（IDE 文件操作/代码执行/终端/RAG 等）
        registry.register_native_tools(tools::default_native_tools(rag_manager.clone()));

        // 创建 VSCode Bridge（伴生扩展通信通道）
        let bridge_config = vscode_bridge::VotekBridgeConfig::new(vscode_bridge::BRIDGE_DEFAULT_PORT);
        let vscode_bridge = VscodeBridge::new(bridge_config);

        // 注册 VSCode IDE 控制工具（getActiveEditor/getDiagnostics/openFile 等）
        vscode_bridge.register_tools(&mut registry);

        AppState {
            active_streams: Mutex::new(HashMap::new()),
            tools: registry,
            rag: rag_manager,
            pet: pet::PetManager::default(),
            vscode_bridge,
            sandbox_manager: Mutex::new(sandbox::SandboxManager::new()),
            current_workspace_id: Mutex::new(None),
        }
    })
        .invoke_handler(tauri::generate_handler![
            chat_stream,
            cancel_chat,
            mcp_connect,
            mcp_disconnect,
            mcp_list_servers,
            mcp_list_tools,
            mcp_call_tool,
            mcp_server_stderr,
            mcp_health_check,
            mcp_reconnect,
            mcp_clear_cache,
            mcp::mcp_check_prereq,
            mcp::mcp_market_list,
            // RAG — 检索增强生成
            rag::rag_init,
            rag::rag_get_config,
            rag::rag_get_stats,
            rag::rag_index_documents,
            rag::rag_search,
            rag::rag_delete_document,
            rag::rag_clear_all,
            rag::rag_search_for_chat,
            // 浏览器模块
            browser::browser_create,
            browser::browser_navigate,
            browser::browser_get_url,
            browser::browser_go_back,
            browser::browser_go_forward,
            browser::browser_reload,
            browser::browser_stop,
            browser::browser_resize,
            browser::browser_destroy,
            // Skills 管理
            skills::skills_list,
            skills::skills_toggle,
            skills::skills_market_list,
            skills::skills_install,
            skills::skills_delete,
            skills::skills_read_content,
            skills::skills_preview_prompt,
            // 插件管理
            plugins::plugins_list,
            plugins::plugins_install,
            plugins::plugins_delete,
            plugins::plugins_toggle,
            plugins::plugin_market_list,
            // IDE / 编译器内核
            ide::ide_execute_code,
            ide::ide_get_languages,
            ide::ide_read_file,
            ide::ide_write_file,
            ide::ide_create_file,
            ide::ide_delete_file,
            ide::ide_rename_file,
            ide::ide_move_file,
            ide::ide_list_dir,
            ide::ide_search_files,
            ide::ide_get_file_info,
            ide::ide_get_workspace,
            ide::ide_set_workspace,
            ide::ide_terminal_exec,
            // Code Server (VS Code IDE 内核)
            code_server::code_server_is_installed,
            code_server::code_server_install,
            code_server::code_server_start,
            code_server::code_server_stop,
            code_server::code_server_restart,
            code_server::code_server_status,
            code_server::code_server_open_ide_window,
            code_server::code_server_read_logs,
            // 桌宠
            pet::toggle_pet,
            pet::pet_interact,
            pet::get_pet_stats,
            // 工作空间 + 沙箱
            workspace_create,
            workspace_list,
            workspace_remove,
            workspace_set_current,
            workspace_get_current,
            sandbox_read_file,
            sandbox_write_file,
            sandbox_delete_file,
            sandbox_create_dir,
            sandbox_list_dir,
            sandbox_file_tree,
        ])
        .on_window_event(|window, event| {
            // 仅主窗口关闭时阻止默认行为并清理子进程；宠物窗等其它窗口直接关闭
            if window.label() != "main" {
                return;
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // 阻止默认关闭，先清理子进程
                api.prevent_close();
                let handle = window.app_handle().clone();
                let main_window = window.clone();
                tauri::async_runtime::spawn(async move {
                    let state = handle.state::<AppState>();
                    eprintln!("[Agent] 正在清理子进程...");
                    state.tools.shutdown().await;
                    code_server::shutdown().await;
                    eprintln!("[Agent] 子进程清理完毕，退出应用");
                    main_window.destroy().ok();
                });
            }
        })
        .setup(|app| {
            // 首次启动：自动创建应用数据目录
            let app_data = app.path().app_data_dir().unwrap_or_else(|_| {
                std::env::current_exe()
                    .ok()
                    .and_then(|p| p.parent().map(|p| p.to_path_buf()))
                    .unwrap_or_default()
                    .join("data")
            });
            if !app_data.exists() {
                if let Err(e) = fs::create_dir_all(&app_data) {
                    eprintln!(
                        "[Agent] 无法创建数据目录 {:?}: {}，应用可能无法保存数据",
                        app_data, e
                    );
                } else {
                    println!("[Agent] 已创建数据目录: {:?}", app_data);
                }
            }

            // 将 VSCode Bridge 配置设为全局（code_server 启动时注入环境变量用）
            let bridge_config = {
                let state = app.state::<AppState>();
                state.vscode_bridge.config().clone()
            };
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                vscode_bridge::set_global_config(bridge_config).await;
                // 后台启动 Code Server（热备，应用打开时 IDE 秒开）
                code_server::start_background(&app_handle).await;

                // Code Server 就绪后，尝试连接 VSCode Bridge
                let state = app_handle.state::<AppState>();
                match state.vscode_bridge.connect().await {
                    Ok(()) => eprintln!("[Agent] VSCode Bridge connected"),
                    Err(e) => eprintln!("[Agent] VSCode Bridge unavailable: {} (IDE context tools will be disabled)", e),
                }
            });

            // 载入持久化的宠物数值
            {
                let stats = pet::load_stats(app.handle());
                let state = app.state::<AppState>();
                *state.pet.stats.lock().unwrap() = stats;
            }

            #[cfg(dev)]
            {
                let window = app.get_webview_window("main").unwrap();
                window.open_devtools();
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// 供 main.rs 兜底清理：终止 code-server 子进程
pub async fn shutdown_code_server() {
    code_server::shutdown().await;
}
