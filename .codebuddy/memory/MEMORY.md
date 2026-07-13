# Agent Desktop — 项目长期记忆

## 项目定位
Windows 桌面级 AI Agent 应用（面向普通用户：通用对话 + 自动化 + 个人知识管理）。
- 主工程 `agent-desktop/`：Tauri v2 + React/TS 桌面客户端。
- 兄弟目录 `agent-loop-reference/`：agent loop 学习/参考（第三方仓库 + 并发 agent 实现，不接入主工程）。

## 技术栈
Tauri v2 · React + TypeScript + TailwindCSS · Rust 后端（Agent 核心）· SQLite（对话/设置）· OpenAI 兼容接口（OpenAI/Claude/DeepSeek/通义等）· MCP 协议（工具生态）。

## 关键架构决策（务必遵守）
- **`.codebuddy/` 整体 gitignore，不提交 git**（含 skills 与 memory）。
- **Skills 仅本地开发使用，不提交**，由 `agent-desktop/.codebuddy/skills/` 提供（运行时 cwd 优先，`start-dev.bat`/`build.bat` 都 `cd agent-desktop/`）。
- 三大扩展体系互相独立、不合并：
  - **MCP Server** → 给 LLM 做 function-calling 工具（stdio 子进程 + 市场）。
  - **Skills** → 给 LLM 注入行为/提示词（本地 `.codebuddy/skills/` + 在线市场）。
  - **Plugins** → 给用户的传统桌面功能（npm registry + @agent-desktop/sdk）。
- 后端 Rust 通过 Tauri IPC 与前端通信；流式对话用 Tauri Event 推 token；API Key 加密存 Windows Credential Manager；纯本地 SQLite 隐私优先。

## 聊天 / Agent 模式（核心）
- 后端 `src-tauri/src/lib.rs` 的 `chat_stream` 命令**本身就是完整 tool-calling agent loop**：
  聚合 MCP 工具 `state.mcp.llm_tools()` + 注入启用 Skills 的 system prompt + `for` 工具循环（事件：`stream-token`/`tool-call`/`tool-result`/`stream-done`）。
- 模式由 `ChatRequest.mode` 开关控制（默认 `"agent"` 兼容旧行为）：
  - **agent 模式** = 完整行为（MCP 工具 + Skills + 工具循环，max_iterations=10）。
  - **chat 模式** = 单轮纯对话（无工具/无 skills/不跑循环，max_iterations=1）。
- 前端：`src/stores/appStore.ts` 的 `chatMode`（`"chat"|"agent"`）+ `setChatMode`，持久化到 store.json；`src/pages/ChatView.tsx` 工具栏「聊天」「Agent」切换，`!isTauriEnv()` 时禁用 Agent。

## 本地开发 Skills（不提交，4 个）
`agent-desktop/.codebuddy/skills/`：`agent-profile`(核心提示词) · `tauri-rust-dev`(Rust/Tauri 约定) · `react-frontend-dev`(前端约定) · `mcp-tools`(MCP 工具+Agent 循环)。启用靠"目录无 `.disabled` 文件"；`get_active_system_prompt` 把所有启用技能正文拼成 system prompt 注入（仅 agent 模式）。

## 已完成的模块（状态）
- ✅ V0.1+：Markdown、快捷键、API Key AES-GCM 加密、i18n(zh-CN/en)、取消生成、置顶、侧边栏收起等。
- ✅ 内置浏览器模块：`src-tauri/src/browser.rs`（WebView2 内核，子 webview + 事件推 URL）。
- ✅ Skills 市场：`src-tauri/src/skills.rs`（含 GitHub/ClawHub 在线抓取）+ `SkillsPanel.tsx`。
- ✅ MCP 工具系统：3 个内置 Server（web/tavily/sqlite）+ 在线市场 `mcp_market_list`（npm+GitHub 抓取，5min 缓存）。
- ✅ Code Server IDE：code-server v4.127.0 随 NSIS 安装包分发（`binaries/code-server/release/`，~212MB→41MB 压缩），独立窗口 + 后台热备，`http://127.0.0.1:port` 免密（**HTTP 非 HTTPS**，未传 --cert 时 code-server 不监听 HTTPS）。**坑 1**：Windows 上 `PathBuf::canonicalize()` 会加 `\\?\` 前缀，Node.js v24 无法识别（EISDIR on 'C:'），必须 strip 掉。**坑 2**：health check/webview URL 必须用 HTTP，否则 code-server 不监听 HTTPS 端口导致持续超时。**坑 3**：code-server 的 @vscode 原生模块（7个: winregistry/windows_process_tree/windows(deviceid)/watchdog/spdlog/vscode-sqlite3/crypt32）在 VS 2026 Insiders 上编译失败，原因是 binding.gyp 设了 `SpectreMitigation: Spectre` 但未装 Spectre 库。修复方法：手动 MSBuild，加 `/p:SpectreMitigation=false` 绕过。MSBuild 路径：`D:\program files\Microsoft Visual Studio\18\Insiders\MSBuild\Current\Bin\MSBuild.exe`。

## 构建环境 / 可移植性约定（2026-07-13 排查）
- **Node.js 版本**：必须 22.x（code-server v4.127.0 engines 指定 node 22，v24 不兼容）。根目录 `.nvmrc` = `22`，`package.json` engines = `"node": ">=18 <=22"`。
- **PROTOC**：`lance-encoding`/`prost-build` 依赖 protoc。`.cargo/config.toml` 中 `[env] PROTOC` 指向本机路径，其他开发者需改为自己的路径或确保 protoc 在 PATH 中。
- **VS BuildTools**：必须安装"使用 C++ 的桌面开发" + Spectre 缓解库，否则原生模块编译失败（MSB8040）。
- **原生模块自动检测**：`download-code-server.mjs` 安装后逐项检查 7 个 `.node` 文件，缺失时自动 `npm install --production`（不带 --ignore-scripts）在 `lib/vscode` 下重新编译。`build.rs` 也加了 `check_native_modules()` 检查。
- **$schema**：`tauri.conf.json` 已修正为 tauri 官方 schema URL。
- **dev 模式**：`start-dev.bat` 现在会自动检查并下载 code-server。`build.rs` 在 cargo build 时也会自动下载。
- ✅ 插件框架：`plugins.rs` + `PluginsPanel.tsx`（基础骨架，待实现真实功能）。
- ✅ RAG 骨架搭建完成：`src-tauri/src/rag.rs`（~760行），8 个 Tauri 命令。技术选型：fastembed（本地 ONNX 嵌入，BGE 中文模型） + text-splitter（语义分块，非滑动窗口） + LanceDB（向量存储）。消费级设计：首次自动下载模型 ~47MB，之后完全离线。Embedder trait 解耦，切换嵌入方案只需实现 trait。
- ✅ RAG 前端面板完成：`src/pages/settings/RagPanel.tsx` + `src/styles/rag.css`，参考 Dify 设计简化。含初始化引导、文档上传/列表/删除、检索测试、高级设置（chunk_size/top_k）。已接入 i18n（zh-CN/en）。
- ✅ 编译优化：Rust 添加 `[profile.release]`（LTO+strip+panic=abort）+ `.cargo/config.toml`（FASTLINK+jobs=8）；前端 Vite 分包（vendor/monaco/xterm/i18n/markdown）+ `@/` 路径别名 + `React.lazy` 页面懒加载 + i18n 动态按需加载 + 语言自动检测（localStorage→navigator→fallback en）。

## Agent Loop 参考结论（学习用）
- agent loop 三大要素：**Loop（循环）、Tools（工具）、Memory（本地维护的 messages 上下文）**。两大学派：文本解析派（正则解析模型文本动作，如 mini-swe-agent）vs 原生 tool-calling 派（API 结构化 tool_calls，如 OpenAI Agents SDK）。
- LLM API 无状态：每次调用重发完整 `messages` 历史；"记忆"是代码本地列表维护的。
- 并发 agent：LLM 调用是 I/O 密集，用 `ThreadPoolExecutor`（主推，几十并发够用）或 `asyncio.Semaphore`+`gather`（超大规模），无需多进程。参考实现见 `agent-loop-reference/concurrent_agent.py` 与 `concurrent_agent_async.py`。
