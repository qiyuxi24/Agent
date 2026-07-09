mod error_codes;
mod mcp;

use error_codes::McpError;
use futures::StreamExt;
use mcp::{McpManager, McpServerConfig};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;
use std::collections::HashMap;
use std::fs;
use std::time::Instant;

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
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

#[tauri::command]
async fn chat_stream(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    request: ChatRequest,
) -> Result<(), String> {
    // 聚合所有已连接 MCP Server 的工具
    let tools = state.mcp.llm_tools().await;

    let mut messages = request.messages.clone();
    let max_iterations = 10;

    // 注册取消信号（供 cancel_chat 命令触发）
    let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);
    {
        let mut streams = state.active_streams.lock().await;
        streams.insert("chat".to_string(), cancel_tx);
    }

    for _ in 0..max_iterations {
        // 本轮调用 LLM（流式输出 + 捕获 tool_calls）
        let (assistant_content, tool_calls) =
            run_completion(&app, &request, &messages, &tools, &mut cancel_rx).await?;

        // 把 assistant 消息加回上下文
        messages.push(ChatMessage {
            role: "assistant".to_string(),
            content: if assistant_content.is_empty() {
                None
            } else {
                Some(assistant_content)
            },
            tool_calls: if tool_calls.is_empty() {
                None
            } else {
                Some(tool_calls.clone())
            },
            tool_call_id: None,
        });

        // 没有工具调用 → 本轮即为最终回答
        if tool_calls.is_empty() {
            let _ = app.emit("stream-done", StreamDone);
            cleanup(&app, &state).await;
            return Ok(());
        }

        // 执行每个工具调用（带超时和错误码）
        for tc in &tool_calls {
            let _ = app.emit(
                "tool-call",
                ToolCallEvent {
                    name: tc.name.clone(),
                    arguments: tc.arguments.clone(),
                },
            );

            let start = Instant::now();
            let res = state.mcp.call_namespaced(&tc.name, &tc.arguments).await;
            let elapsed = start.elapsed();

            let (result_text, is_error, error_code, error_category, suggested_action) = match &res {
                Ok(t) => (t.clone(), false, None, None, None),
                Err(e) => {
                    let action = if e.is_retryable() {
                        "retry"
                    } else if e.needs_reconnect() {
                        "reconnect"
                    } else {
                        "none"
                    };
                    // 包含耗时信息帮助前端诊断
                    let msg = format!(
                        "{} (耗时 {:.1}s)",
                        e.message,
                        elapsed.as_secs_f64()
                    );
                    (
                        msg,
                        true,
                        Some(e.code.to_string()),
                        Some(e.category.to_string()),
                        Some(action.to_string()),
                    )
                }
            };

            let _ = app.emit(
                "tool-result",
                ToolResultEvent {
                    name: tc.name.clone(),
                    result: result_text.clone(),
                    is_error,
                    error_code: error_code.clone(),
                    error_category: error_category.clone(),
                    suggested_action: suggested_action.clone(),
                },
            );

            messages.push(ChatMessage {
                role: "tool".to_string(),
                content: Some(result_text),
                tool_calls: None,
                tool_call_id: Some(tc.id.clone()),
            });

            // 如果工具调用导致进程退出（MCP-002），通知前端
            if let Some(ref code) = error_code {
                if code == "MCP-002" || code == "MCP-004" {
                    eprintln!("[chat_stream] MCP 服务器断开，中止后续工具调用");
                    break;
                }
            }
        }
    }

    // 超出最大迭代，正常结束
    let _ = app.emit("stream-done", StreamDone);
    cleanup(&app, &state).await;
    Ok(())
}

/// 调用一次 LLM 流式接口，实时推送文本 token，并收集 tool_calls
async fn run_completion(
    app: &AppHandle,
    request: &ChatRequest,
    messages: &[ChatMessage],
    tools: &[Value],
    cancel_rx: &mut tokio::sync::watch::Receiver<bool>,
) -> Result<(String, Vec<ToolCall>), String> {
    let url = format!("{}/chat/completions", request.api_base.trim_end_matches('/'));
    let client = reqwest::Client::new();

    let mut body = serde_json::json!({
        "model": request.model,
        "messages": messages.iter().map(msg_to_value).collect::<Vec<_>>(),
        "stream": true,
        "temperature": 0.7,
        "max_tokens": 4096,
    });
    if !tools.is_empty() {
        body["tools"] = Value::Array(tools.to_vec());
    }

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", request.api_key))
        .json(&body)
        .send()
        .await
        .map_err(|e| McpError::llm_network(&e.to_string()).to_string())?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let short: String = body.chars().take(300).collect();
        return Err(McpError::llm_api_error(status.as_u16(), &short).to_string());
    }

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut content = String::new();
    let mut tool_accs: Vec<ToolCallAcc> = Vec::new();

    loop {
        let next_chunk = tokio::select! {
            chunk = stream.next() => chunk,
            _ = cancel_rx.changed() => {
                return Ok((content, Vec::new()));
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
                            return Ok((content, finalize_tool_calls(tool_accs)));
                        }

                        if let Ok(parsed) = serde_json::from_str::<Value>(data) {
                            let delta = &parsed["choices"][0]["delta"];
                            if let Some(token) = delta["content"].as_str() {
                                if !token.is_empty() {
                                    content.push_str(token);
                                    let _ = app.emit(
                                        "stream-token",
                                        StreamToken {
                                            token: token.to_string(),
                                        },
                                    );
                                }
                            }
                            // 累积分片 tool_calls
                            if let Some(tcs) = delta["tool_calls"].as_array() {
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

    Ok((content, finalize_tool_calls(tool_accs)))
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

async fn cleanup(app: &AppHandle, state: &tauri::State<'_, AppState>) {
    let _ = app.emit("stream-done", StreamDone);
    let mut streams = state.active_streams.lock().await;
    streams.remove("chat");
}

#[tauri::command]
async fn cancel_chat(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let streams = state.active_streams.lock().await;
    if let Some(cancel) = streams.get("chat") {
        let _ = cancel.send(true);
    }
    Ok(())
}

// ===================== MCP 管理命令 =====================

#[tauri::command]
async fn mcp_connect(
    state: tauri::State<'_, AppState>,
    config: McpServerConfig,
) -> Result<usize, String> {
    state.mcp.connect(config).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn mcp_disconnect(
    state: tauri::State<'_, AppState>,
    name: String,
) -> Result<(), String> {
    state.mcp.disconnect(&name).await
}

#[tauri::command]
async fn mcp_list_servers(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<mcp::McpServerInfo>, String> {
    Ok(state.mcp.list_servers().await)
}

#[tauri::command]
async fn mcp_list_tools(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<mcp::McpTool>, String> {
    let servers = state.mcp.servers.lock().await;
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
        .mcp
        .call_namespaced(&namespaced, &args)
        .await
        .map_err(|e| e.to_string())
}

pub struct AppState {
    active_streams: Mutex<HashMap<String, tokio::sync::watch::Sender<bool>>>,
    pub mcp: McpManager,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 非开发模式：从 exe 旁边的 dist/ 文件夹加载前端
    // 这样改前端只需要 npm run build，完全不需要重编译 Rust
    #[cfg(not(dev))]
    let builder = {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_default();
        let dist_dir = exe_dir.join("dist");
        println!("[Agent] Loading frontend from: {:?}", dist_dir);

        tauri::Builder::default()
            .register_uri_scheme_protocol("agentui", move |_ctx, request| {
                let path = request.uri().path().trim_start_matches('/');
                let file_path = if path.is_empty() { "index.html" } else { path };
                let full_path = dist_dir.join(file_path);

                match std::fs::read(&full_path) {
                    Ok(data) => {
                        let mime = mime_guess::from_path(&full_path)
                            .first_or_octet_stream()
                            .essence_str()
                            .to_string();
                        http::Response::builder()
                            .header("Content-Type", mime)
                            .header("Access-Control-Allow-Origin", "*")
                            .body(data)
                            .unwrap()
                    }
                    Err(_) => {
                        // SPA fallback: 所有路由返回 index.html
                        let index = dist_dir.join("index.html");
                        if let Ok(data) = std::fs::read(&index) {
                            http::Response::builder()
                                .header("Content-Type", "text/html")
                                .header("Access-Control-Allow-Origin", "*")
                                .body(data)
                                .unwrap()
                        } else {
                            http::Response::builder()
                                .status(404)
                                .header("Content-Type", "text/plain")
                                .body(b"Not Found".to_vec())
                                .unwrap()
                        }
                    }
                }
            })
    };

    #[cfg(dev)]
    let builder = tauri::Builder::default();

    builder
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .manage(AppState {
            active_streams: Mutex::new(HashMap::new()),
            mcp: McpManager::new(),
        })
        .invoke_handler(tauri::generate_handler![
            chat_stream,
            cancel_chat,
            mcp_connect,
            mcp_disconnect,
            mcp_list_servers,
            mcp_list_tools,
            mcp_call_tool
        ])
        .setup(|app| {
            // 首次启动：自动创建应用数据目录
            // Tauri store 插件会把 store.json 存到这里
            let app_data = app.path().app_data_dir().unwrap_or_else(|_| {
                std::env::current_exe()
                    .ok()
                    .and_then(|p| p.parent().map(|p| p.to_path_buf()))
                    .unwrap_or_default()
                    .join("data")
            });
            if !app_data.exists() {
                fs::create_dir_all(&app_data).unwrap_or_else(|e| {
                    eprintln!("[Agent] 无法创建数据目录 {:?}: {}", app_data, e);
                });
                println!("[Agent] 已创建数据目录: {:?}", app_data);
            }

            // 非开发模式：导航到自定义协议加载前端
            #[cfg(not(dev))]
            {
                use tauri::Manager;
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.navigate(tauri::Url::parse("https://agentui.localhost/index.html").unwrap());
                }
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
