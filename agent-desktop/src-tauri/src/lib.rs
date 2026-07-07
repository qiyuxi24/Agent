use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;
use std::collections::HashMap;
use std::fs;

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
    pub content: String,
}

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

#[tauri::command]
async fn chat_stream(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    request: ChatRequest,
) -> Result<(), String> {
    let url = format!("{}/chat/completions", request.api_base.trim_end_matches('/'));

    let client = reqwest::Client::new();
    let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);

    {
        let mut streams = state.active_streams.lock().await;
        streams.insert("chat".to_string(), cancel_tx);
    }

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", request.api_key))
        .json(&serde_json::json!({
            "model": request.model,
            "messages": request.messages,
            "stream": true,
            "temperature": 0.7,
            "max_tokens": 4096,
        }))
        .send()
        .await
        .map_err(|e| format!("网络请求失败: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let short: String = body.chars().take(300).collect();
        return Err(format!("LLM API 错误 ({}): {}", status, short));
    }

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();

    loop {
        // 用 select! 同时监听流数据和取消信号
        let next_chunk = tokio::select! {
            chunk = stream.next() => chunk,
            _ = cancel_rx.changed() => {
                // 被取消
                let _ = app.emit("stream-done", StreamDone);
                let mut streams = state.active_streams.lock().await;
                streams.remove("chat");
                return Ok(());
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
                            let _ = app.emit("stream-done", StreamDone);
                            let mut streams = state.active_streams.lock().await;
                            streams.remove("chat");
                            return Ok(());
                        }

                        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) {
                            if let Some(token) = parsed["choices"][0]["delta"]["content"].as_str() {
                                if !token.is_empty() {
                                    let _ = app.emit("stream-token", StreamToken {
                                        token: token.to_string(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
            Some(Err(e)) => {
                let _ = app.emit("stream-error", StreamError {
                    error: format!("流式读取错误: {}", e),
                });
                break;
            }
            None => break,
        }
    }

    let _ = app.emit("stream-done", StreamDone);
    let mut streams = state.active_streams.lock().await;
    streams.remove("chat");
    Ok(())
}

#[tauri::command]
async fn cancel_chat(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let streams = state.active_streams.lock().await;
    if let Some(cancel) = streams.get("chat") {
        let _ = cancel.send(true);
    }
    Ok(())
}

pub struct AppState {
    active_streams: Mutex<HashMap<String, tokio::sync::watch::Sender<bool>>>,
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
        })
        .invoke_handler(tauri::generate_handler![chat_stream, cancel_chat])
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

            let window = app.get_webview_window("main").unwrap();
            window.open_devtools();
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
