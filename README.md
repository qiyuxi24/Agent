# Votek

> AI Agent 桌面平台 — Chat 对话、Agent 工具调用、IDE 编程、RAG 知识库，一站式 AI 工作台。

基于 **Tauri v2 + React + TypeScript + Rust** 构建，纯本地运行，隐私优先。

---

## 核心能力

| 模块 | 状态 | 说明 |
|------|------|------|
| **Chat 对话** | ✅ | 流式 SSE、多模型切换、Markdown 渲染、i18n |
| **Agent 模式** | ✅ | ReAct 工具循环、MCP 工具调用、Skills 注入、深度思考可视化 |
| **IDE（code-server）** | ✅ | 完整 VS Code 内核、独立窗口、后台热备秒开、插件生态 |
| **RAG 知识库** | ✅ | 本地向量嵌入（BGE 模型）、LanceDB 存储、文档检索 |
| **MCP 工具系统** | ✅ | stdio 协议、3 个内置 Server、在线市场、14 类错误码 |
| **内置浏览器** | ✅ | WebView2 原生内核、地址栏导航、前进后退 |
| **Skills 技能系统** | ✅ | 市场安装/启用/禁用、7 个预装技能、YAML frontmatter |
| **Plugins 插件** | 🏗️ | 基础骨架完成，待实现真实功能 |
| **Agent 集群** | 📋 | 多 Agent 并行 + 共享工具层（规划中） |

---

## 架构

```
┌──────────────────────────────────────────────────────┐
│                  Tauri v2 桌面壳                       │
│                                                      │
│  React UI (Chat / IDE / Browser / Settings / RAG)     │
│       │ invoke / event                               │
│       ▼                                              │
│  Rust 后端                                           │
│  ├─ Agent Loop (ReAct 工具循环 + 流式输出)             │
│  ├─ MCP Manager (工具系统 + 市场)                      │
│  ├─ Skills Manager (技能注册 + 市场)                   │
│  ├─ RAG Engine (fastembed + LanceDB)                  │
│  ├─ IDE Manager (code-server 生命周期)                │
│  ├─ Browser (WebView2 子窗口)                         │
│  └─ Plugin Manager (扩展框架)                         │
│       │                                              │
│  LLM (OpenAI / DeepSeek / 通义 / Claude / ...)        │
│                                                      │
│  存储层：SQLite + store.json + Windows Credential     │
└──────────────────────────────────────────────────────┘
```

## 快速开始

```bash
# 开发模式（一键启动）
./start-dev.bat

# 完整打包发布
./build.bat

# 发布
./scripts/release.bat 0.2.0
```

### 环境要求

| 工具 | 版本 | 说明 |
|------|------|------|
| **Windows** | 10 1803+ / 11 | 系统自带 WebView2 |
| **Node.js** | **22.x**（必须） | code-server 要求 Node 22，v24 不兼容 |
| **Rust** | stable (MSVC) | `winget install Rustlang.Rustup` |
| **VS BuildTools** | 2022 | C++ 工作负载 + Spectre 缓解库 |
| **protoc** | 最新 | `winget install Google.Protobuf` |
| **Git** | 2.30+ | |

> 完整环境配置与故障排查见 [agent-desktop/README.md](agent-desktop/README.md)

---

## 项目结构

```
Agent/
├── README.md                   # 本文件
├── CHANGELOG.md                # 版本历史
├── RELEASE.md                  # 发布操作指南
├── .nvmrc                      # Node.js 22 版本锁定
├── start-dev.bat               # 一键开发环境
├── build.bat                   # 完整打包
├── setup-code-server.bat       # 下载 code-server
├── scripts/                    # 发布/下载脚本
├── agent-desktop/              # 主工程
│   ├── README.md               # 详细文档 + 开发指南
│   ├── TODO.md                 # 路线图 & 待办
│   ├── src/                    # React 前端
│   │   ├── pages/              # 页面组件
│   │   ├── components/         # 共用组件
│   │   ├── stores/             # Zustand 状态管理
│   │   └── styles/             # 样式
│   └── src-tauri/              # Rust 后端
│       ├── src/                # 核心模块
│       │   ├── lib.rs          # Agent Loop + 入口
│       │   ├── mcp.rs          # MCP 工具系统
│       │   ├── skills.rs       # Skills 技能系统
│       │   ├── rag.rs          # RAG 向量检索
│       │   ├── browser.rs      # 内置浏览器
│       │   ├── code_server.rs  # IDE 管理
│       │   ├── ide.rs          # 代码执行引擎
│       │   └── plugins.rs      # 插件框架
│       ├── Cargo.toml
│       └── tauri.conf.json
├── agent-loop-reference/       # Agent Loop 学习参考
└── .codebuddy/                 # 团队工作日志 + 项目记忆
    ├── memory/                 # 每日记录 + MEMORY.md
    └── skills/                 # 自定义 skills 定义
```

---

## 版本历史

见 [CHANGELOG.md](CHANGELOG.md) | [RELEASE.md](RELEASE.md)

**当前版本**：v0.1.0 / Unreleased

### 近期更新

- **IDE**：code-server v4.127.0 完整 VS Code 内核，独立窗口 + 后台热备
- **RAG**：本地向量嵌入（BGE）+ LanceDB，支持文档上传/检索/问答
- **Agent**：ReAct 工具循环 + 深度思考可视化（reasoning_content）
- **MCP**：14 类错误码 + 3 层超时 + 在线市场
- **Skills**：7 个预装技能 + GitHub Raw 市场
- **编译优化**：LTO + 分包 + 懒加载 + i18n 按需
- **可移植性**：Node 22 锁定、原生模块自动编译、构建前提文档完善

---

## License

MIT
