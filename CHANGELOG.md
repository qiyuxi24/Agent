# Changelog

本项目遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/) 语义化版本规范。

格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.0.0/)。

---

## [0.2.0] - Unreleased

### 新增

- **内置浏览器**：基于 WebView2 原生内核的完整浏览器（6 files, +319/-237）
  - **架构升级**：从 iframe 改为 Tauri WebviewBuilder 子 webview（与 Edge 同内核）
  - Rust 侧：browser.rs 模块 + 9 个 Tauri commands（navigate/reload/back/forward/resize/destroy 等）
  - 事件驱动：`browser-url-changed` 实时同步地址栏，`browser-page-loaded` 停止 loading
  - 原生历史栈：前进后退走 WebView2 原生 history，无跨域限制
  - ResizeObserver：窗口大小变化时自动调整 webview 位置/尺寸
  - 生命周期管理：切换页面自动销毁 webview，回到浏览器自动重建
  - 地址栏：输入 URL 自动补全 https://，中文/空格自动转为 Google 搜索
  - 键盘快捷键：`Ctrl+L` 聚焦地址栏
  - 需要 `tauri` 的 `unstable` feature（multiwebview 支持）

- **Skills 技能生态系统**：设置页新增 Skills 面板，连接真实技能市场（11 files, +1610/-73）
  - Rust 侧：`skills.rs` 模块 + 6 个 Tauri commands（list/toggle/market_list/install/delete/read_content）
  - 解析 SKILL.md 的 YAML frontmatter 提取元信息（名称/描述/版本/分类）
  - 已安装管理：启用/禁用（`.disabled` 标记文件）、卸载、预览 SKILL.md 内容
  - 技能市场：从 GitHub raw 获取市场列表 JSON，支持分类筛选（前端/后端/AI/MCP/研究/工具）
  - 一键安装：下载 SKILL.md + README.md 到本地 `.codebuddy/skills/` 目录
  - 离线回退：无网络时使用内置预定义市场列表
  - 7 个预设技能：前端开发工作室、全栈开发、MCP 构建器、Prompt 工程、智能体编排、深度研究、浏览器自动化
  - 新增 10 个图标组件：Package/Download/Code/Store/Eye/ToggleLeft/ToggleRight/Search/Folder/Sparkles
  - 完整中英文 i18n（skills 命名空间 23 个 key）

- **MCP 工具面板增强**：重构 ToolsPanel，新增市场推荐 + 实时状态
  - MCP 市场折叠面板：5 个推荐服务器（filesystem/github/brave-search/postgres/puppeteer），一键安装
  - 15 秒定时刷新连接状态，连接脉冲动画（`.mcp-status-dot`）
  - 工具按服务器分组显示，手动添加折叠到 `<details>` 中
  - 已连接服务器显示工具计数徽章

### 技术细节

- 流式增量解析 OpenAI 分片 `tool_calls`（按 index 合并 name/arguments）
- 取消生成信号与工具循环解耦，保持单次 stream-done 发射

### 修复（健壮性）

- **MCP 错误码体系**：新增 `error_codes.rs`（Rust）+ `mcpErrors.ts`（前端），覆盖 MCP-001 到 MCP-014 共 14 类错误
  - MCP-001 超时 / MCP-002 进程退出 / MCP-003 工具执行错误 / MCP-004 连接关闭 / MCP-005 服务器未连接 / MCP-006 工具名格式 / MCP-007 参数解析 / MCP-008 IO 错误 / MCP-009 JSON 解析 / MCP-010 进程启动 / MCP-011 初始化失败 / MCP-012 LLM 网络 / MCP-013 LLM API / MCP-014 流式中断
  - 每类错误携带 `is_retryable` / `needs_reconnect` 策略标记
  - 前端 `ToolResultEvent` 新增 `is_error` / `error_code` / `error_category` / `suggested_action` 字段
- **工具调用超时保护**：`request()` 30s 超时 / 握手 15s 超时 / 单行读取 10s 超时，防止 MCP 子进程卡死导致对话永久阻塞
- **进程健康检测**：`McpClient::is_alive()` 在每次工具调用前后检测进程存活，崩溃时自动标记+移除
- **stderr 捕获**：MCP 子进程 stderr 改为 piped（原为 inherit），便于诊断
- **静默吞错修复**：工具反序列化失败 / JSON 行解析失败 均输出 eprintln 日志
- **参数类型安全**：`McpError` 替代原始 `String`，类型安全的错误传播
- **对话降级保护**：进程退出类错误（MCP-002/004）自动中止后续工具调用，返回已生成文本

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
