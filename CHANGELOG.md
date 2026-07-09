# Changelog

本项目遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/) 语义化版本规范。

格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.0.0/)。

---

## [0.2.0] - Unreleased

### 新增

- **MCP 工具系统（V0.2 核心）**：App 作为 MCP Host，可连接外部 MCP Server（stdio 子进程），聚合其暴露的 tools 供 LLM 通过 OpenAI function-calling 协议调用
  - Rust 端新增极简 MCP 客户端（`src-tauri/src/mcp.rs`）：stdio 传输 + JSON-RPC 2.0，完成 `initialize` / `tools/list` / `tools/call` 握手与调用，无第三方 MCP SDK 依赖
  - `chat_stream` 改造为**工具调用循环**：LLM 返回 `tool_calls` → 路由到对应 MCP Server 执行 → 结果回灌上下文 → 多轮直到无工具调用（最多 10 轮）
  - 新增 Tauri 命令：`mcp_connect` / `mcp_disconnect` / `mcp_list_servers` / `mcp_list_tools` / `mcp_call_tool`
  - 工具名采用 `server::tool` 命名空间，避免多 Server 工具名冲突
- **设置页「工具 / MCP」面板**：可视化添加 / 连接 / 断开 / 移除 MCP 服务器，实时展示可用工具列表
- **对话内工具步骤展示**：监听 `tool-call` / `tool-result` 事件，在对话流中实时显示 AI 调用的工具与结果
- **启动自动重连**：应用启动时自动重连已保存的 MCP 服务器配置
  - **持久化**：MCP 服务器配置写入 store.json
  - **内置 Web 工具服务器**（`mcp-servers/web/index.mjs`）：零依赖、无需 API Key 的 Node MCP Server，提供 `web_search`（DuckDuckGo 联网搜索）与 `fetch_url`（爬取网页正文）。首次启动自动种子并连接，agent 开箱即用具备联网能力
  - **内置 Tavily 搜索引擎**（`mcp-servers/tavily/index.mjs`）：零依赖 Node MCP Server，提供 `tavily_search`（AI 深度搜索）与 `tavily_extract`（结构化内容提取）。需设置 `TAVILY_API_KEY` 环境变量（可从 tavily.com 免费获取）。首次启动自动种子
  - `McpServerUI` 新增 `env` 字段，支持为 MCP Server 进程传递环境变量
  - `mcp-servers/` 目录随 Tauri bundle 打包为资源文件

### 技术细节

- 流式增量解析 OpenAI 分片 `tool_calls`（按 index 合并 name/arguments）
- 取消生成信号与工具循环解耦，保持单次 stream-done 发射

---

## [0.0.1] - 2026-07-08

> 🎉 首个公开发布版本

### 核心功能

- **流式 AI 对话**：打字机效果实时展示，支持中途取消生成
- **多模型管理**：支持 OpenAI / DeepSeek / 通义千问等任意兼容接口，对话中一键切换
- **多对话管理**：侧边栏创建/切换/删除对话，自动生成标题
- **Markdown 渲染**：AI 回复支持代码高亮、表格、列表等富文本
- **明暗主题**：跟随系统 / 浅色 / 深色三种模式，即时切换
- **本地持久化**：对话记录、API Key、设置全部存本地，不上传任何服务器
- **环境自适应**：Tauri 桌面端走 Rust 调用；浏览器打开自动降级 fetch 直连，方便调试

### 设置系统

- 快捷键：Ctrl+N/L/K/, 四个常用快捷键，支持点击录制自定义，最多 3 键
- Provider 管理：卡片式增删改查，每个 Provider 下可管理多个模型
- 主题 / 语言切换

### 技术栈

- **桌面壳**：Tauri v2（Rust） + React 19 + TypeScript
- **状态管理**：Zustand 5 + tauri-plugin-store 持久化
- **后端**：reqwest SSE 流式 + tokio 异步
- **构建**：前后端分离编译（改 UI 无需重编译 Rust）

### 构建

- `start-dev.bat` — 一键开发
- `build.bat` — 完整打包

---

## [0.1.0] - 2026-07-07

### 新增

- **流式 AI 对话**：Rust 端通过 reqwest SSE 流式调用 LLM API，前端打字机效果实时展示
- **多模型管理系统**：支持添加多个 AI 提供商（OpenAI、DeepSeek、通义千问等），每个提供商下管理多个模型，对话中一键切换
- **多对话管理**：侧边栏创建/切换/删除对话，自动根据首条消息生成标题
- **明暗主题**：支持跟随系统 / 浅色 / 深色三种模式，即时切换
- **本地持久化**：基于 tauri-plugin-store，对话记录、API Key、设置全部存本地 JSON，不上传任何服务器
- **设置页**：Provider 卡片式管理，支持添加/编辑/删除提供商，动态增删模型
- **环境自适应**：在 Tauri WebView 中走 Rust invoke 调用；在普通浏览器中自动降级为 fetch 直连 LLM API（方便调试）
- **前后端分离编译**：Rust 注册自定义 URI scheme（agentui://），非 dev 模式下从 exe 旁边 dist/ 文件夹加载前端，改 UI 完全不需要重编译 Rust
- **首次启动自动初始化**：自动创建应用数据目录，零配置即可运行
- **取消生成**：对话中可随时中断 LLM 响应

### 构建脚本

- `start-dev.bat`：一键启动开发环境（Vite + Tauri）
- `build-frontend.bat`：只编译前端（~2 秒）
- `build.bat`：完整打包发布

### 技术架构

- Tauri v2 桌面壳 + React 19 + TypeScript 前端
- Rust 后端：reqwest 流式 HTTP + tokio 异步 + SSE 解析
- Zustand 5 状态管理 + tauri-plugin-store 持久化
- 自定义 agentui:// 协议加载本地前端文件
