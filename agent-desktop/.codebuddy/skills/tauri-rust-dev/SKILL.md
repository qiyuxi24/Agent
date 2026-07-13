---
name: Tauri Rust 后端开发
description: Rust / Tauri v2 后端开发约定与命令编写模式
description_zh: Rust / Tauri v2 后端开发约定与命令编写模式
description_en: Rust / Tauri v2 backend conventions and command patterns
version: 0.1.0
metadata:
  category: backend
---
Rust / Tauri v2 后端开发约定（目录 `agent-desktop/src-tauri/`）。

## 结构
- 入口 `src-tauri/src/lib.rs`：`#[tauri::command]` 命令注册、AppState、对话流 `chat_stream`。
- 各能力模块：`mcp.rs`（MCP 管理器）、`skills.rs`（Skills 管理 + system prompt 注入）、`code_server.rs`（Code Server IDE 后端）、`ide.rs`。
- 状态统一放 `AppState`，使用 `tokio::sync::Mutex` + `.lock().await`（**不要用 std::sync::Mutex**，会在 tokio 上下文死锁）。

## 命令编写
- 命令签名：`#[tauri::command] async fn xxx(app: AppHandle, state: State<AppState>, ...)`。
- 向后端发事件：`app.emit("事件名", payload)`，前端用 `listen("事件名", ...)` 接收（对话流事件：`stream-token` / `tool-call` / `tool-result` / `stream-done`）。
- 流式对话在 `chat_stream` 内循环：聚合 MCP 工具 → 调用 LLM（`run_completion` 流式 + 收集 tool_calls）→ 有 tool_calls 就执行并把结果作为 `role: "tool"` 消息回传 → 再调，直到无工具调用或达到 `max_iterations`。

## 编译验证
- 改动后运行 `cargo check`（在 `agent-desktop/src-tauri` 下）。
- serde 字段常用 `#[serde(default)]` / `skip_serializing_if` 保持与前端兼容。
- 前端通过 `invoke("命令名", { 参数 })` 调用；命令签名变了，前端调用处要同步改。
