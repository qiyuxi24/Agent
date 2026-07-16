//! MCP 客户端实现（stdio + JSON-RPC 2.0）
//!
//! 设计目标：让桌面 App 作为 MCP Host，连接外部 MCP Server（stdio 子进程），
//! 聚合它们暴露的 tools，并供 LLM 通过 OpenAI function-calling 协议调用。
//!
//! 子模块结构：
//!   - `types`   — 核心类型定义（McpTool, McpServerConfig, StderrRing 等）
//!   - `client`  — McpClient：启动子进程、握手、工具调用
//!   - `manager` — McpManager：聚合多 Server、缓存、健康检查
//!   - `market`  — 市场抓取（npm/GitHub）+ 前置依赖检测 + 内置回退

pub mod types;
pub mod client;
pub mod manager;
pub mod market;

// 重导出：外部（lib.rs / tools.rs / agent_loop.rs）通过 `crate::mcp::*` 访问
pub use types::{McpServerConfig, McpServerInfo, McpTool};
pub use manager::McpManager;
// 注意：mcp_check_prereq 和 mcp_market_list 是 #[tauri::command]，需通过完整路径
// `mcp::market::mcp_check_prereq` 引用（Tauri 宏生成的辅助符号不在 re-export 范围内）
