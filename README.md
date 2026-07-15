# Votek

> **AI Agent 桌面平台** — 聊天、自动化、编程、知识管理，一站式 AI 工作台。

Votek 原名 Agent Desktop，命名灵感来自 Wojtek（英式发音：Votek），二战中波兰炮兵部队的叙利亚棕熊吉祥物——图标即为像素风棕熊。

基于 **Tauri v2 + React + TypeScript + Rust** 构建，纯本地运行，隐私优先。支持 OpenAI / DeepSeek / Claude / 通义千问 等任意兼容接口。

---

## 核心能力

| 模块 | 状态 | 说明 |
|------|------|------|
| **Chat 对话** | ✅ | 流式 SSE、多模型切换、Markdown 渲染、i18n（zh-CN/en） |
| **Agent 模式** | ✅ | ReAct 工具循环、深度思考可视化（reasoning_content）、并行工具调用 |
| **ToolRegistry 工具中介** | ✅ | MCP + 原生工具统一注册/调度/可见，新架构基石 |
| **MCP 工具系统** | ✅ | stdio 协议、3 个内置 Server、在线市场、14 类错误码+3 层超时 |
| **原生工具** | ✅ | 文件操作、代码执行（10 种语言）、终端命令、RAG 检索 |
| **IDE（code-server）** | ✅ | 完整 VS Code v4.127.0 内核、独立窗口、后台热备秒开 |
| **RAG 知识库** | ✅ | 本地向量嵌入（BGE 模型~47MB）、LanceDB 存储、语义分块 |
| **Skills 技能系统** | ✅ | 市场安装/启用/禁用、GitHub 源、YAML frontmatter |
| **内置浏览器** | ✅ | WebView2 原生内核、地址栏导航、前进后退 |
| **Plugins 插件框架** | 🏗️ | 基础骨架完成（npm registry + @agent-desktop/sdk） |
| **Agent 集群** | 📋 | 多 Agent 并行 + 共享工具层（规划中） |

---

## 架构

```
┌────────────────────────────────────────────────────────────┐
│                    Tauri v2 桌面壳                          │
│                                                            │
│   React UI (Chat / IDE / Browser / Settings / RAG / Pet)    │
│        │ invoke / event                                    │
│        ▼                                                    │
│   Rust 后端                                                 │
│   ┌─ ToolRegistry ──────────────────────────────────┐      │
│   │  统一工具注册表（MCP + 原生 + Skills）           │      │
│   │  ├── McpManager（MCP Server 子进程池）          │      │
│   │  ├── 原生工具（文件/代码/终端/RAG, 10 个）      │      │
│   │  └── Skills 工具（预留, SKILL.md 声明）         │      │
│   └────────────────────────────────────────────────┘      │
│   ├─ Agent Loop（think → act → observe，依赖注入）        │
│   ├─ Skills Manager（本地 + 市场安装/启用）               │
│   ├─ RAG Engine（fastembed ONNX + LanceDB）               │
│   ├─ IDE Manager（code-server 生命周期管理）               │
│   ├─ Browser（WebView2 子窗口 + 事件推 URL）              │
│   ├─ Plugin Manager（npm registry 扩展框架）              │
│   └─ Pet Desktop（像素风桌宠交互）                        │
│        │                                                  │
│   LLM 层（OpenAI 兼容接口 / DeepSeek / Claude / 通义...）  │
│                                                            │
│   存储层：SQLite + store.json + Windows Credential Manager │
└────────────────────────────────────────────────────────────┘
```

### 三大扩展体系（互相独立、不合并）

| 体系 | 给谁用 | 载体 | 来源 |
|------|--------|------|------|
| **MCP Server** | LLM 做 function-calling | stdio 子进程 | 在线市场 + 手写 |
| **Skills** | LLM 注入行为/提示词 | SKILL.md 文本 | GitHub 市场 |
| **Plugins** | 用户扩展桌面功能 | npm 包 | npm registry |

---

## 快速开始

```bash
# 开发模式（一键启动）
./start-dev.bat

# 完整打包发布
./build.bat

# 发布版本
./scripts/release.bat 0.2.0
```

### 环境要求

| 工具 | 版本 | 说明 |
|------|------|------|
| **Windows** | 10 1803+ / 11 | 系统自带 WebView2 |
| **Node.js** | **22.x**（必须） | code-server 要求 Node 22，v24 不兼容 |
| **Rust** | stable (MSVC) | `winget install Rustlang.Rustup` |
| **VS BuildTools** | 2022 |  C++ 桌面工作负载 + Spectre 缓解库 |
| **protoc** | 最新 | `winget install Google.Protobuf` |
| **Git** | 2.30+ | |

### LLM 配置

首次启动后，在设置面板中配置 API Key：
- 支持任意 OpenAI 兼容接口（OpenAI / DeepSeek / Claude / 通义千问 等）
- 默认模型 `qwen-plus`，模型名可在设置中修改
- API Key 加密存储在 Windows Credential Manager 中

---

## 项目结构

```
Agent/
├── README.md                   # 本文件
├── CHANGELOG.md                # 版本历史
├── RELEASE.md                  # 发布操作指南
├── branding.json               # 品牌名称真相源（改名字改此处）
├── build.config.json           # 构建配置真相源（版本/原生模块/Node 要求）
├── .nvmrc                      # Node.js 22 版本锁定
├── start-dev.bat               # 一键开发环境
├── build.bat                   # 完整打包
├── setup-code-server.bat       # 下载 code-server
├── scripts/                    # CLI 构建/发布脚本
│   ├── build/index.mjs         # 统一 CLI（check/prepare/dev/build）
│   ├── check-env.mjs           # 环境校验
│   └── gen-bear-icon.mjs       # 像素风棕熊图标生成
├── agent-desktop/              # 主工程
│   ├── README.md               # 详细开发指南
│   ├── TODO.md                 # 路线图 & 待办
│   ├── src/                    # React 前端
│   │   ├── pages/              # ChatView / IDE / Settings / RAG 等
│   │   ├── components/         # 共用 UI 组件
│   │   ├── stores/             # Zustand 状态管理
│   │   └── styles/             # 全局样式
│   └── src-tauri/              # Rust 后端
│       └── src/
│           ├── lib.rs          # 入口 + AppState + Tauri 命令注册
│           ├── tools.rs        # ▸ ToolRegistry 统一工具中介 ◂
│           ├── agent_loop.rs   # Agent Loop 引擎（依赖注入）
│           ├── mcp.rs          # MCP 客户端/管理器/市场
│           ├── skills.rs       # Skills 技能系统
│           ├── rag.rs          # RAG 向量检索引擎
│           ├── browser.rs      # 内置浏览器
│           ├── code_server.rs  # IDE 管理
│           ├── ide.rs          # 代码执行引擎
│           ├── plugins.rs      # 插件框架
│           ├── pet.rs          # 桌面宠物
│           └── error_codes.rs  # 14 类 MCP 错误码
├── agent-loop-reference/       # Agent Loop 学习参考（第三方）
└── .codebuddy/                 # 团队工作日志 + 项目记忆（gitignore）
```

---

## 技术详解

### Agent Loop（核心引擎）

`src-tauri/src/agent_loop.rs` — 完整的 tool-calling agent 循环，通过两个 trait 实现依赖注入：

```rust
trait LlmClient:     // LLM 调用抽象（可注入 FakeClient 做单元测试）
trait ToolExecutor:  // 工具执行抽象（可注入 FakeExecutor）
```

循环流程：**THINK**（LLM 推理）→ **ACT**（并行/串行执行工具）→ **OBSERVE**（回传结果）→ 直到无工具调用返回。

护栏机制：
- 最大迭代轮次（agent=10, chat=1）
- 墙钟时间限制（300s）
- 取消信号（watch channel）
- LLM 调用重试（指数退避+抖动，最多 3 次）
- 格式错误自愈（未知工具返回结构化错误）
- 工具结果截断（8000 字符上限）

### ToolRegistry 工具中介

`src-tauri/src/tools.rs` — 2026-07-15 新增的统一工具注册表。

**设计参考**：OpenAI function calling + Anthropic tool use + LangChain Tool。

- MCP 工具：`server::tool` 命名空间（`McpManager` 管理子进程池）
- 原生工具：`native_*` 前缀（10 个预装工具）
- `all_tools()` → 统一 `Vec<Value>`（OpenAI function-calling 格式）注入 LLM
- `execute()` → 自动分派到 MCP 子进程或 Rust 函数

```
LLM tools[] = [MCP 工具...] + [原生工具...]
     │
     ▼
ToolRegistry.execute(name, args)
     │
     ├── name 含 "::"  → McpManager.call_namespaced()
     └── 否则          → NativeToolFn(args)
```

**原生工具列表**：read_file / write_file / create_file / delete_file / rename_file / list_directory / search_files / execute_code（10 种语言沙盒） / terminal_exec / rag_search。

### 聊天模式

前端 `ChatView.tsx` 工具栏切换「聊天」/「Agent」模式：
- **chat 模式**：单轮纯对话（无工具、无 skills、不跑循环）
- **agent 模式**：完整 tool-calling agent 循环（MCP 工具 + 原生工具 + Skills 注入 + 深度思考可视化）

### RAG 检索增强生成

`src-tauri/src/rag.rs` — 技术选型：
- 嵌入：fastembed（本地 ONNX 推理，BGE 中文模型，首次下载 ~47MB，之后纯离线）
- 分块：text-splitter（语义分块，非滑动窗口）
- 存储：LanceDB（列式向量存储，本地文件）
- 架构：`Embedder` trait 解耦，可替换为远程 API

### Skills 技能系统

Skills 以 **纯文本形式** 注入到 LLM 的 system prompt 中（而非 tools 格式），提供行为指导和上下文。当前 4 个内置技能：
- `agent-profile` — 核心提示词
- `tauri-rust-dev` — Rust/Tauri 约定
- `react-frontend-dev` — 前端约定
- `mcp-tools` — MCP 工具+Agent 循环说明

安装路径：应用数据目录（非源码树），支持 GitHub 主题搜索安装。

### Code Server IDE

内嵌完整 VS Code 内核（code-server v4.127.0）：
- 独立 Tauri 窗口加载
- 后台热备（应用启动即预热，打开秒开）
- HTTP 免密访问（`http://127.0.0.1:port`）
- 随 NSIS 安装包分发（压缩 ~41MB）
- 原生模块自动编译（自动去除 SpectreMitigation 构建配置）

---

## 构建提示

### 编译优化

- Rust：LTO + Strip + panic=abort + FASTLINK + 并行 jobs=8
- 前端：Vite 分包（vendor/monaco/xterm/i18n/markdown）+ `@/` 路径别名 + `React.lazy` 懒加载 + i18n 按需加载
- 首次构建因 code-server 原生模块编译较慢（约 3-5 分钟）

### 常见坑

| 问题 | 原因 | 解决 |
|------|------|------|
| `MSB8040` 编译失败 | 缺 Spectre 缓解库 | VS Installer → 修改 → 单个组件 → 搜索 "Spectre" 并安装 |
| `protoc` 未找到 | lance-encoding 编译依赖 | `.cargo/config.toml` 指定 PROTOC 路径或 `winget install Google.Protobuf` |
| code-server 端口超时 | HTTPS 而非 HTTP | health check 必须用 HTTP（code-server 启动未传 --cert 时不监听 HTTPS） |
| `.bat` 报 `'ho' is not recognized` | 换行符不是 CRLF | 用 PowerShell 脚本转 CRLF |
| Node v24 报 EISDIR | code-server 不兼容 Node 24 | 用 `.nvmrc` 锁定 Node 22 |

---

## License

**BSL 1.1**（Business Source License 1.1）

- Licensed Work：Votek
- Additional Use Grant：个人/学习/内部业务免费，禁止以托管或嵌入式方式提供给第三方做商业竞争
- Change Date：2029-07-14
- Change License：Apache 2.0

源码可见但限制商业竞争使用。到期自动转为 Apache 2.0 完全开源。

---

## 版本历史

见 [CHANGELOG.md](CHANGELOG.md) | [RELEASE.md](RELEASE.md)

**当前版本**：v0.3.0 / Unreleased
