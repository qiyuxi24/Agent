# Votek — 开发路线图 & TODO

> 最后更新：2026-07-14

---

## 已完成 ✅

### 核心对话
- [x] 流式对话（Tauri Event 推送 token）
- [x] 多轮对话 + 上下文管理
- [x] 多 Provider/多模型管理（OpenAI/Claude/DeepSeek/通义千问）
- [x] Markdown 渲染 + 代码高亮 + 复制按钮
- [x] 明暗主题切换
- [x] i18n 国际化（中/英）
- [x] 快捷键系统（Ctrl+K 新对话、Ctrl+B 侧边栏等）
- [x] API Key AES-GCM 加密存储
- [x] 取消生成 / 置顶 / 侧边栏收起 / 回底按钮

### MCP 工具系统
- [x] stdio + JSON-RPC 2.0 协议实现（纯手写，零 SDK 依赖）
- [x] 14 个结构化错误码（MCP-001 ~ MCP-014）
- [x] 3 层超时保护（握手 15s / 调用 30s / 读取 10s）
- [x] LLM ReAct 循环（工具调用 → 结果 → 继续推理，最多 10 轮）
- [x] stderr 环缓冲区（Arc<Mutex<>> 共享，真实可用）
- [x] 工具调用缓存（TTL 60s）
- [x] 自动重连（崩溃后自动恢复）
- [x] 健康检查 + 错误计数
- [x] 相对路径自动解析（dev/prod 双模式）
- [x] 内置 MCP Server：web（DuckDuckGo 搜索 + 网页抓取）、tavily（AI 搜索）
- [x] 内置 MCP Server：sqlite（本地数据库查询/建表/CRUD）
- [x] 7 个 MCP 市场推荐

### Agent Loop 引擎（2026-07-14 完善）
- [x] 将内联在 `chat_stream` 的循环抽为独立可测模块 `agent_loop.rs`（think→act→observe）
- [x] 依赖注入：`LlmClient` / `ToolExecutor` trait（参考 `mini_agent/core.py` 的 provider/tool_dispatcher 注入）
- [x] 复用开源可靠性层（`mini_agent/reliability.py`）：LLM 指数退避重试、工具可重试错误重试、结果截断、结构化脱敏日志、格式错误自愈回传
- [x] 护栏（`mini-swe-agent/default.py`）：max_iterations + wall_time 上限 + 取消信号
- [x] 并行工具执行 `join_all`（对齐 OpenAI Agents SDK 的并行 tool 调用）
- [x] Skills 注入 + MCP 工具聚合/调用原样保留并验证接入（7 个 `#[cfg(test)]` 单测全过）

### Skills 技能系统
- [x] Skills 管理模块（`skills.rs`，6 个 commands）
- [x] YAML frontmatter 解析
- [x] 启用/禁用（`.disabled` 标记文件）
- [x] GitHub Raw 市场（`market.json` 索引 + 下载）
- [x] 7 个预装开发 skills（mcp-builder/frontend-dev/fullstack-dev 等）
- [x] 双标签页 UI（已安装 / 市场）+ 搜索 + 分类筛选

### 浏览器模块
- [x] WebView2 内嵌浏览器（9 个 Tauri commands）
- [x] 地址栏 + 导航事件同步
- [x] 前进/后退（原生 history）
- [x] ResizeObserver 自动同步尺寸

### 插件系统
- [x] 基础框架（`plugins.rs` + `PluginsPanel.tsx`）— 占位实现，待完善

### IDE（code-server）
- [x] code-server v4.127.0 完整 VS Code 内核
- [x] 独立 Tauri 窗口 + 后台热备 + 秒开
- [x] 完整 VS Code 插件生态（.vsix）

### 深度思考
- [x] ThinkingBlock 折叠面板 + reasoning_content 解析
- [x] 前端/后端双端事件（ThinkingStart/Delta/Stop）

### 基础设施
- [x] Tauri v2 + React + TypeScript + Rust
- [x] SQLite 持久化（对话 + 设置）
- [x] Zustand 状态管理
- [x] Error Boundary
- [x] .gitignore / CHANGELOG / README
- [x] 子进程隐藏控制台窗口 — 已覆盖现有代码（⚠️ 后续新增 Command 需持续加 `creation_flags(0x08000000)`）
- [x] **子进程 Layer 2**：应用退出时自动清理所有子进程
  - [x] `code_server.rs`：`shutdown()` 从全局静态取出并 kill 子进程
  - [x] `mcp.rs`：`McpManager::shutdown()` 遍历 kill 所有 MCP 子进程
  - [x] `lib.rs`：`on_window_event(CloseRequested)` 钩子触发清理
  - [x] `main.rs`：`run()` 返回后兜底清理 code-server


---

## 近期 TODO（P0）

### Agent Loop 增强（2026-07-16 后端新增）

> 后端已完成以下增强，以下为前端消费任务：

#### 后端新增 ✅
- [x] **请求参数透传**：`ChatRequest` 新增 `tool_use_behavior` / `stop_at_tool_names` / `require_tool_approval_for` / `max_iterations` / `enrichment_threshold_chars` / `llm_timeout_secs`，从 Agent 配置透传到 `AgentLoopConfig`
- [x] **单次 LLM 超时保护**：`AgentLoopConfig.llm_timeout_secs`（0=使用底层 120s 读取超时），通过 `tokio::time::timeout` 包装
- [x] **L2 验证循环**：`verify_output()` — LLM-as-Judge 质量验证（Maker/Checker 分离原则），`AgentLoopConfig.enable_verification` 控制
- [x] **结构化轮次跟踪**：`agent-iteration` 事件（`AgentIterationEvent`：iteration/total/phase/elapsed_ms），`agent-loop-stats` 事件（`AgentLoopStats`：总轮次/耗时/工具调用数/压缩次数/是否验证）
- [x] 新增 3 个事件类型：`AgentIterationEvent`、`AgentLoopStats`、`agent-iteration-tick`（向后兼容旧格式）

#### 前端待消费
- [ ] **实时思考流式（ThinkingPanel）**：验证 `thinking-delta` 在非 reasoning 模型上的表现。后端现在将 agent 模式中间轮的 content 实时推为 `thinking-delta`，ThinkingPanel 应能正常增量显示（无需 thinking-start 重复触发）。如果面板闪烁/空白，需检查 `ThinkingPanel.tsx` 是否对无 thinking-start 的 thinking-delta 有兜底处理。
- [ ] **上下文压缩通知**：监听 `context-compacted` 事件，在聊天区域底部或通知栏显示一条轻提示（如"上下文已优化，节省 X tokens"），2-3s 自动消失。参考 `StreamNotify` 组件用法。
- [ ] **Token 用量面板**：监听 `token-usage` 事件，在聊天输入框附近或 footer 栏显示当前轮次的 input/output/total token 数。可选：仅 agent 模式显示；hover 展开详情。
- [ ] **工具审批弹窗（HITL）**：
  1. 监听 `tool-approval-required` 事件 → 弹出模态确认框，显示工具名、参数 JSON（格式化），以及"批准"/"拒绝"按钮 + 可选反馈输入框
  2. 用户点击后调 `tool_approval_response` Tauri command 写入审批决策
  3. 后端侧默认无需审批任何工具，通过设置页 `require_tool_approval_for` 数组配置敏感工具名（如 `native_write_file`、`native_terminal_exec`）
- [ ] **审批设置 UI**：在设置页 Agent 配置区域新增"敏感工具审批"输入框（逗号分隔工具名），调用后端 config 同步。当前后端 `require_tool_approval_for` 默认为空数组。
- [ ] **Agent Loop 统计展示**：监听 `agent-loop-stats` 事件，在最终答案末尾显示一个轻量总结（轮次/耗时/工具调用数）
- [ ] **轮次进度指示**：监听 `agent-iteration` 事件，在思考面板或输入框上方显示当前轮次进度（如"第 3/10 轮·思考中"）

#### 后续后端增强（低优先级）
- [ ] Verification Loop 通过 `ChatRequest` 控制而非硬编码 `false`
- [ ] `agent-iteration` 前端进度条对接
- [ ] L2 验证结果前端展示（"回答已通过质量验证" 标记）

### 子进程管理 — Layer 3（后续）

> Windows 子进程窗口闪现问题分三层解决：
> - [x] **Layer 1**：`CREATE_NO_WINDOW` — 隐藏子进程窗口（已覆盖现有代码，后续新增 `Command` 需持续加 flag）
> - [x] **Layer 2**：**进程生命周期管理** — 应用退出时自动清理所有子进程（CloseRequested 钩子 + shutdown 方法 + main.rs 兜底）
> - [ ] **Layer 3**：**静默启动 + 退出清理验证**
>   - [ ] Windows Job Object：`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` 确保父进程退出时所有子进程树被系统回收（兜底 Layer 2）
>   - [ ] 优雅关闭：先发 SIGTERM/关闭信号等待 2s，超时后再 start_kill（目前直接强杀）
>   - [ ] 测试：启动应用 → 打开 IDE (code-server) → 运行代码 → 关闭应用 → 确认无残留进程
>   - [ ] 测试：Agent 模式触发 MCP 工具调用 → 中途取消 → 确认 MCP 子进程被终止
>   - [ ] 测试：编译型语言执行超时 → 确认 taskkill 子进程树完整清理

### 对话增强
- [ ] 对话历史搜索
- [ ] 对话导出（Markdown/JSON）
- [ ] System Prompt 自定义（用户可编辑 Agent 角色）

### MCP 完善
- [ ] 支持 SSE/WebSocket 传输（目前仅 stdio）
- [ ] MCP Server 环境变量加密存储（目前明文存在 store.json）
- [ ] 更多内置 MCP Server（playwright、filesystem 增强版）

### IDE 遗留项
- [ ] 插件/设置持久化（重启不丢失已装扩展和配置）
- [ ] 主题与 Votek 自动同步（dark/light）
- [ ] 扩展市场默认配置（预装推荐插件列表）
- [ ] 系统原生目录选择对话框（接入 tauri-plugin-dialog）
- [ ] Agent ↔ IDE 联动协议（Agent 打开文件/跳转行号/运行命令）
- [ ] IDE 内选中代码 → 发给指定 Agent 处理

### 知识库 RAG（V0.3）
- [x] 文档导入（PDF/Word/TXT/Markdown）— 通过 RagPanel 上传
- [x] 向量嵌入（本地 embedding 模型）— fastembed + BGE 中文模型
- [x] 向量检索 + LLM 问答 — LanceDB + text-splitter 语义分块
- [ ] 文件变更自动增量索引
- [ ] 支持更多文档格式（图片 OCR、HTML、EPUB）

---

## 中期 TODO（P1）— 插件 & 扩展生态

### 传统桌面插件系统
> 定义：非 AI 工作流的功能扩展（VPN、日历、脚本、工具面板等）

- [ ] 插件协议设计：`plugin.json` manifest + permissions 声明
- [ ] 分发方案：npm registry（npm 发布 + 你的 App 内一键安装）
- [ ] 沙箱模型：iframe + postMessage API bridge
- [ ] `@agent-desktop/sdk` npm 包（给插件开发者的 API）
- [ ] 脚手架 CLI：`npx create-agent-plugin`
- [ ] 插件 UI 贡献点：面板（panel）/ 命令（command）/ 主题（theme）/ 托盘（tray）
- [ ] PluginsPanel 从占位改为真正可用的安装/管理界面
- [ ] 市场数据源：GitHub Raw market.json（复用 Skills 模式）

## Agent 集群架构 🏗️

> 核心理念：这不是一个 Agent 应用，而是一个 Agent 集群平台。
> IDE / 浏览器 / 终端 等都是基础设施工具，所有 Agent 共享，按需调用。

### 架构设计
```
Agent 集群管理器
├─ Agent A（前端开发）──┐
├─ Agent B（后端开发）──┤
├─ Agent C（测试）    ──┤
├─ Agent D（DevOps）  ──┤
└─ ...                ──┤
                         ├── IDE 工具（code-server）
共享工具层                ├── 浏览器工具（WebView2）
                         ├── 终端工具（xterm.js）
                         ├── 文件系统工具
                         ├── MCP 工具系统
                         └── Skills 技能系统
```

### Agent 集群核心能力
- [ ] 多 Agent 并行运行，各自独立上下文
- [ ] Agent 间通信/协作机制
- [ ] 工具调用队列与互斥（两个 Agent 不能同时写同一个文件）
- [ ] Agent 角色模板（前端/后端/全栈/测试/DevOps）
- [ ] 任务分发与调度

---

### IDE 工具（完整 VS Code 体验）
> 定位：IDE 不是给某个 Agent 独有的，而是集群的共享工具。
> 目标：功能和页面与 VS Code / Trae 等主流 IDE 对齐。

- [x] **Step 1**：Monaco Editor 嵌入（代码面板 + 语法高亮）
- [x] **编译器内核**：Rust 后端支持 Python/JS/TS/Rust/Go/C/C++/Ruby/PHP/Bash 代码执行，带超时保护
- [x] **Step 2**：xterm.js 终端面板（内嵌终端 + 命令历史 + 上下箭头切换）
- [x] **文件系统增强**：创建文件/文件夹、重命名、删除（含确认）、移动、右键菜单
- [x] **工作区管理**：面包屑导航 + 切换工作目录 + 最近工作区列表（localStorage）
- [x] **全局搜索**：Ctrl+Shift+F 搜索文件名和内容（1MB 以下文本文件）
- [x] **文件树右键菜单**：新建文件/文件夹、重命名、复制路径、删除
- [x] **底部面板**：输出/终端/搜索 三标签切换
- [x] **Step 3**：code-server 完整 VS Code IDE 内核
  - 自动下载安装（GitHub Releases，~100MB）
  - Tauri Rust 后端管理进程生命周期（start/stop/status）
  - 🔥 独立 Tauri 窗口（不再 iframe 嵌入 — 点击 IDE 打开新窗口，完整 VS Code 体验）
  - 支持完整 VS Code 插件生态（.vsix）
  - `--auth none` 本地免密模式
  - 🔥 **应用启动时后台热备**：setup() 阶段 spawn code-server，轮询端口就绪后发 `ide-ready` 事件
  - 🔥 **独立窗口**：`code_server_open_ide_window` 创建新 Tauri WebviewWindow，直接加载 `https://localhost:8443`
  - IdePage 降级为状态页（显示"VS Code 已在独立窗口中运行" + 打开/重试按钮）
- [ ] **Step 4**：IDE 作为 Agent 集群工具
  - [ ] Agent ↔ IDE 联动协议：Agent 打开文件/跳转行号/运行命令
  - [ ] IDE 内选中代码 → 发给指定 Agent 处理
  - [ ] 多实例支持：不同 Agent 在同一 IDE 中打开不同工作区
  - [ ] IDE 服务发现：Agent 自动发现当前可用的 IDE 实例
- [x] **Step 5**：VS Code 体验对齐
  - [x] 启动速度优化（code-server 常驻后台，秒开）
  - [x] 全屏原生窗口（独立 Tauri 窗口，无任何遮挡）
  - [ ] 插件/设置持久化（重启不丢失已装扩展和配置）
  - [ ] 主题与 Votek 自动同步（dark/light）
  - [x] 快捷键透传（独立窗口 = 完整 VS Code 体验，所有快捷键正常使用）
  - [ ] 扩展市场默认配置（预装推荐插件列表）
- [x] **Step 6**：插件生态
  - [x] LSP Client → code-server 已内置
  - [x] Extension Host → code-server 已内置
  - [x] VS Code Extension API → code-server 已完整实现
  - [ ] 系统原生目录选择对话框（tauri dialog 插件）

### Windows 自动化（V0.4）
> 集成方案：sbroenne/mcp-windows (v1.3.16) — 语义 UI 自动化 MCP Server，Standalone .exe 零依赖
> 17 个工具：ui_find/ui_click/ui_type/ui_read/screenshot/mouse/keyboard/window_management/app/file_save
> 特点：按名称定位控件（非坐标），内置 OCR 回退，LLM token 优化 60%
>
> 架构：作为内置 MCP Server（stdio + JSON-RPC），与 web/tavily 同层，通过 ToolRegistry 暴露给 Agent
- [x] 下载脚本 + 构建集成（scripts/download-windows-mcp.mjs, build.config.json, build.rs）
- [x] 前端种子（appStore.ts DEFAULT_WINDOWS_MCP_SERVER）
- [x] 打包资源（tauri.conf.json binaries/windows-mcp/*.exe）
- [x] 解耦设计：二进制可选、平台无感、连接失败不影响应用运行
- [ ] UI Automation 树读取（Windows UIA API） → sbroenne ui_find/ui_read
- [ ] 窗口/控件定位 + 点击/输入 → sbroenne ui_click/ui_type + mouse/keyboard 后备
- [ ] 屏幕截图 + OCR → sbroenne screenshot + Windows.Media.Ocr 回退
- [ ] 宏录制/回放 → 待自行封装

---

## 长期 TODO（P2）— 生态 & 发布

### 扩展市场
- [ ] 统一三个入口（MCP/Skills/Plugins）的用户界面
- [ ] 自建扩展注册表 `extensions.yourapp.com`
- [ ] 扩展评分/评论/版本管理
- [ ] 扩展自动更新

### 多平台
- [ ] macOS 适配
- [ ] Linux 适配
- [ ] 鸿蒙适配

### 发布
- [ ] 自动更新（Tauri updater）
- [ ] 代码签名（Windows + macOS）
- [ ] 安装包构建 CI/CD
- [ ] 应用商店上架（Microsoft Store / Mac App Store）

---

## 技术债（需重构）

- [ ] **深度思考与 SSE 解析解耦**：`lib.rs` 的 `run_completion()` 中 thinking 解析与 SSE 逻辑耦合；前端 `ChatView.tsx` 中 Tauri 事件监听和 fetch SSE 回调有两套重复的思考处理代码。应抽为独立 `ThinkingParser`。

---

## 想法 & 待研究

- [ ] 本地模型：Ollama / llama.cpp 集成（纯离线场景）
- [ ] 手机端：Tauri Mobile？
- [ ] 语音输入/输出（Whisper + TTS）
- [ ] Agent 间任务编排：任务拆解 → 分派给多个 Agent → 结果汇总
- [ ] IDE 工具与主流 IDE 对标：VS Code / Trae / Cursor 功能差异清单
