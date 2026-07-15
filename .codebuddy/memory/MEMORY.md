# Votek — 项目长期记忆（原名 Agent Desktop）

## 项目定位
Windows 桌面级 AI Agent 应用（面向普通用户：通用对话 + 自动化 + 个人知识管理）。
- 主工程 `agent-desktop/`：Tauri v2 + React/TS 桌面客户端。
- 学习/参考在 `reference/`：agent loop 第三方仓库 + 并发 agent 实现，不接入主工程。

## 品牌与改名机制（2026-07-14）
- **产品已改名 `Agent Desktop` → `Votek`**（Wojtek 的英式发音；Wojtek 是二战中波兰炮兵部队的叙利亚棕熊吉祥物，故图标为熊）。
- **唯一真相源 = `agent-desktop/branding.json`**（`productName`/`identifier`/`appTitle`/`packageName`/`userAgent`）。
- 改完跑 `npm run sync-branding`（在 `agent-desktop/` 下）即可把名字同步到所有文件；脚本在 `scripts/sync-branding.mjs`，只替换显示名 `Agent Desktop` 与标识符 `com.agent.desktop`，**不动目录名 `agent-desktop` 与 Rust crate 名**（crate 名与 `main.rs` 的 `agent_desktop_lib::run()` 绑定，改了会编译失败）。
- 图标源 `agent-desktop/src-tauri/icons/icon.svg`（像素风棕熊），通过 `scripts/gen-bear-icon.mjs`（64×64 网格 → 8×8 SVG 方块）生成；运行 `node scripts/gen-bear-icon.mjs` 后，再用 `npx tauri icon src-tauri/icons/icon.svg` 生成全套尺寸。
- 图标风格：透明底、熊几乎撑满画布、轻微方块像素感（`shape-rendering="crispEdges"`）。
- **一键生成脚本 `scripts/gen-icon.bat`**（CRLF + 纯 ASCII，符合 .bat 约定）：内含使用说明与颗粒度变量 `set GRID=64`（越小越像素化），双击即生成 icon.svg 并 `npx tauri icon` 替换全部平台图标；也支持 `gen-icon.bat 32` 传参。`gen-bear-icon.mjs` 的 N 已从 `process.argv[2]` 读取。

## 技术栈
Tauri v2 · React + TypeScript · 纯 CSS（无 Tailwind）· Rust 后端（Agent 核心）· SQLite（对话/设置）· OpenAI 兼容接口（OpenAI/Claude/DeepSeek/通义等）· MCP 协议（工具生态）。

## 前端配色方案（2026-07-15 重构）
- **Accent 色系**：熊棕色（Votek 品牌色）— `#c08040`(主) / `#daa06d`(暗色主题) / `#9e6530`(hover)
- **三层 Token 体系**：所有颜色定义在 `src/styles/variables.css`（~260行），分 Layer 1 Primitives（原始色板，跨主题不变）→ Layer 2 Semantic（语义映射，随主题切换）→ Layer 3 Component（组件 token，消费端唯一引用）。
- **零硬编码**：14 个 CSS 文件中的全部 `#xxxxxx` 和硬编码 `rgba()` 已替换为 CSS 变量引用。
- **TS 侧常量**：`src/tokens/theme.ts`，导出 `palette` / `semanticLight` / `tokensLight` / `tokensDark`。
- **主题切换**：CSS 类 `.theme-light` / `.theme-dark` + `@media (prefers-color-scheme: dark)` 系统跟随。

## 关键架构决策（务必遵守）
- **`.codebuddy/` 整体 gitignore，不提交 git**（含 skills 与 memory）。
- **Skills 仅本地开发使用，不提交**，由 `agent-desktop/.codebuddy/skills/` 提供。
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
`agent-desktop/.codebuddy/skills/`：`agent-profile`(核心提示词) · `tauri-rust-dev`(Rust/Tauri 约定) · `react-frontend-dev`(前端约定) · `mcp-tools`(MCP 工具+Agent 循环)。启用靠"目录无 `.disabled` 文件"；`get_active_system_prompt` 扫描「内置 `.codebuddy/skills` + 应用数据目录 `skills`」双目录，把所有启用技能正文拼成 system prompt 注入（仅 agent 模式）。

## Skills 市场与安装（2026-07-14 重构）
- **市场源**：GitHub 主题搜索（`topic:codebuddy-skill`/`claude-skill`）+ 本仓库 `346379/Agent/.codebuddy/skills`（我们自己的 skills，见 `market.json`）+ 内置兜底。**已移除 ClawHub**：其实测无直连下载地址、必须走私有 CLI+登录，无法可靠下载，且用户要求用我们自己 agent 的 skills。
- **安装**：`skills_install` 用 GitHub Contents API 递归下载整个 skill 目录，落地到**应用数据目录** `app.path().app_data_dir()/skills`（不混入源码树 `.codebuddy`，符合用户"不要装进 .codebuddy"的要求）。分支 `main` 404 回退 `master`，5MB 体积上限，装完校验 `SKILL.md`。
- **LLM 消费**：`get_active_system_prompt` / `skills_list` / `skills_toggle` / `skills_read_content` 统一走 `all_skill_dirs`（内置+安装目录），装好的 skill 会被 Agent LLM 自动注入。
- 前端 `SkillsPanel.tsx` 的 `installSkill` 始终调 `skills_install(id, downloadUrl)`，无 ClawHub CLI 分支。

## 已完成的模块（状态）
- ✅ V0.1+：Markdown、快捷键、API Key AES-GCM 加密、i18n(zh-CN/en)、取消生成、置顶、侧边栏收起等。
- ✅ 内置浏览器模块：`src-tauri/src/browser.rs`（WebView2 内核，子 webview + 事件推 URL）。
- ✅ Skills 市场：`src-tauri/src/skills.rs`（含 GitHub/ClawHub 在线抓取）+ `SkillsPanel.tsx`。
- ✅ MCP 工具系统：3 个内置 Server（web/tavily/sqlite）+ 在线市场 `mcp_market_list`（npm+GitHub 抓取，5min 缓存）。
- ✅ Code Server IDE：code-server v4.127.0 随 NSIS 安装包分发（`binaries/code-server/release/`，~212MB→41MB 压缩），独立窗口 + 后台热备，`http://127.0.0.1:port` 免密（**HTTP 非 HTTPS**，未传 --cert 时 code-server 不监听 HTTPS）。**坑 1**：Windows 上 `PathBuf::canonicalize()` 会加 `\\?\` 前缀，Node.js v24 无法识别（EISDIR on 'C:'），必须 strip 掉。**坑 2**：health check/webview URL 必须用 HTTP，否则 code-server 不监听 HTTPS 端口导致持续超时。**坑 3**：code-server 的 @vscode 原生模块（7个: winregistry/windows_process_tree/windows(deviceid)/watchdog/spdlog/vscode-sqlite3/crypt32）在 VS 2026 Insiders 上编译失败（MSB8040：缺 Spectre 缓解库），因为 `*.gyp` 设了 `SpectreMitigation: Spectre`。**已自动化修复**：`scripts/download-code-server.mjs` 的 `stripSpectreMitigation()` 在 `npm install --production` 编译前**递归**扫描 `@vscode/**/*.gyp`（含 sqlite3 的 `deps/sqlite3.gyp`）并删除该设置，改用常规 MSVC 库即可编译。手动兜底：MSBuild 加 `/p:SpectreMitigation=false`，路径 `D:\program files\Microsoft Visual Studio\18\Insiders\MSBuild\Current\Bin\MSBuild.exe`。

## 构建环境 / 可移植性约定（2026-07-13 排查）
- **Node.js 版本**：必须 22.x（code-server v4.127.0 engines 指定 node 22，v24 不兼容）。根目录 `.nvmrc` = `22`，`package.json` engines = `"node": ">=18 <=22"`。
- **PROTOC**：`lance-encoding`/`prost-build` 依赖 protoc。`.cargo/config.toml` 中 `[env] PROTOC` 指向本机路径，其他开发者需改为自己的路径或确保 protoc 在 PATH 中。
- **VS BuildTools**：必须安装"使用 C++ 的桌面开发" + Spectre 缓解库，否则原生模块编译失败（MSB8040）。
- **原生模块自动检测**：`download-code-server.mjs` 检测 7 个 `.node` 缺失时，先 `stripSpectreMitigation()` 递归去除所有 `*.gyp` 的 `SpectreMitigation`，再 `npm install --production`（不带 --ignore-scripts）在 `lib/vscode` 下重新编译。`build.rs` 的 `check_native_modules()` 仅检测不编译（build.rs 的 npm install 用 `--ignore-scripts` 故意不编）。**注意**：`npm rebuild` 在 `lib/vscode` 会因无关依赖（如 kerberos）编译失败而中断，导致 @vscode 模块编不全；应逐个对缺失模块目录 `node-gyp rebuild`（sqlite3 还需先清 `build/` 缓存）。
- **$schema**：`tauri.conf.json` 已修正为 tauri 官方 schema URL。
- **dev 模式**：`start-dev.bat` 现在会自动检查并下载 code-server。`build.rs` 在 cargo build 时也会自动下载。
- ✅ 插件框架：`plugins.rs` + `PluginsPanel.tsx`（基础骨架，待实现真实功能）。
- ✅ RAG 骨架搭建完成：`src-tauri/src/rag.rs`（~760行），8 个 Tauri 命令。技术选型：fastembed（本地 ONNX 嵌入，BGE 中文模型） + text-splitter（语义分块，非滑动窗口） + LanceDB（向量存储）。消费级设计：首次自动下载模型 ~47MB，之后完全离线。Embedder trait 解耦，切换嵌入方案只需实现 trait。
- ✅ RAG 前端面板完成：`src/pages/settings/RagPanel.tsx` + `src/styles/rag.css`，参考 Dify 设计简化。含初始化引导、文档上传/列表/删除、检索测试、高级设置（chunk_size/top_k）。已接入 i18n（zh-CN/en）。
- ✅ 编译优化：Rust 添加 `[profile.release]`（LTO+strip+panic=abort）+ `.cargo/config.toml`（FASTLINK+jobs=8）；前端 Vite 分包（vendor/monaco/xterm/i18n/markdown）+ `@/` 路径别名 + `React.lazy` 页面懒加载 + i18n 动态按需加载 + 语言自动检测（localStorage→navigator→fallback en）。

## 项目约定：.bat 文件必须 CRLF + 纯 ASCII（2026-07-14 踩坑）
- **所有 Windows `.bat` 批处理文件必须用 CRLF 换行 + 纯 ASCII 内容**。
- 症状：LF-only 换行会让 CMD 逐字符解析错乱，报 `'ho' is not recognized`（echo）、`'gent-desktop'`（agent-desktop）、`'RRORLEVEL'`、`... was unexpected at this time` 等；中文全角括号 `（）` 注释加剧字节错位。
- **重要**：`write_to_file` 工具写出的是 LF 换行！写完 `.bat` 后必须用 PowerShell 转 CRLF：
  `$t=[IO.File]::ReadAllText($f); $t=($t -replace "\`r\`n","\`n") -replace "\`n","\`r\`n"; [IO.File]::WriteAllText($f,$t,(New-Object Text.UTF8Encoding($false)))`
- 注释用 `REM` 或 ASCII，不要用中文（尤其含全角括号）。
- 已修复文件：start-dev.bat / build.bat / build-frontend.bat / setup-code-server.bat / release.bat 全部转为 CRLF。

## 授权协议（2026-07-14 决策）
- **采用 BSL 1.1（Business Source License 1.1），非完全开源**。源码可见但限制商业竞争使用。
- 参数：Licensed Work = Votek；Additional Use Grant = 个人/学习/内部业务免费，禁止以托管或嵌入式方式提供给第三方做商业竞争；Change Date = 2029-07-14；Change License = Apache 2.0（到 Change Date 自动转开源）。
- 文件：`agent-desktop/LICENSE`。README License 段已从错误的 "MIT" 改为 BSL 说明。
- 背景：用户明确要求"不要全开源的"，对比 SSPL/ELv2/Commons Clause 后选 BSL（桌面端非 SaaS，BSL 的"限制商业竞争+定时转开源"最契合）。

## Agent Loop 参考结论（学习用）
- agent loop 三大要素：**Loop（循环）、Tools（工具）、Memory（本地维护的 messages 上下文）**。两大学派：文本解析派（正则解析模型文本动作，如 mini-swe-agent）vs 原生 tool-calling 派（API 结构化 tool_calls，如 OpenAI Agents SDK）。
- LLM API 无状态：每次调用重发完整 `messages` 历史；"记忆"是代码本地列表维护的。
- 并发 agent：LLM 调用是 I/O 密集，用 `ThreadPoolExecutor`（主推，几十并发够用）或 `asyncio.Semaphore`+`gather`（超大规模），无需多进程。参考实现见 `reference/agent-loop/concurrent_agent.py` 与 `concurrent_agent_async.py`。
