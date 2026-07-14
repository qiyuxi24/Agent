# Agent Desktop

> 轻量级桌面 AI 对话客户端，支持多模型切换，对话记录本地存储。

基于 **Tauri v2 + React + TypeScript + Rust** 构建，纯端侧运行，无需额外服务。

## 特性

- 🖥️ **纯桌面端**：Tauri v2 壳 + Rust 后端，安装包仅 ~5MB
- 🤖 **多模型支持**：管理多个 AI 提供商（OpenAI / DeepSeek / 通义千问 等），对话中一键切换
- 💬 **流式对话**：SSE 流式输出，打字机效果，支持取消生成
- 🌗 **明暗主题**：跟随系统 / 浅色 / 深色 三种模式
- 💾 **本地存储**：对话记录、API Key、设置全部存在本地，不上传任何服务器
- 📦 **前后端分离**：改 UI 只需 `npm run build`（2 秒），无需重编译 Rust
- 🔌 **零配置启动**：首次运行自动创建数据目录，下载即用

## 快速开始

### 环境要求

- **Windows 10/11**（系统自带 WebView2）
- **Node.js 22.x**（必须，code-server 要求 Node 22；v24 不兼容）
- **Rust**（rustc + cargo，安装：`winget install Rustlang.Rustup`）
- **Visual Studio Build Tools 2022**（C++ 工作负载，用于编译 Rust 和原生 Node 模块）
  - 安装时勾选"使用 C++ 的桌面开发"
  - 额外安装"MSVC v143 - VS 2022 C++ Spectre-mitigated libs"（单个组件 → 搜索 Spectre）
- **Git**（克隆仓库）
- **tar 或 7z**（解压 code-server，Windows 10 1803+ 自带 tar）
- **protoc**（Protocol Buffers 编译器，用于 RAG 向量检索）
  - `winget install Google.Protobuf` 或从 [GitHub Releases](https://github.com/protocolbuffers/protobuf/releases) 下载
  - 确保 `protoc --version` 可用

### 开发模式（热更新）

双击项目根目录的 `start-dev.bat`，自动：
1. 安装 npm 依赖（如未安装）
2. 启动 Vite dev server（前端热更新）
3. 启动 Tauri 桌面窗口

```bash
# 或手动：
cd agent-desktop
npm install
npm run tauri dev
```

### 下载 Code Server（IDE 功能，约 212MB）

IDE 功能需要 code-server，首次构建时会自动下载。也可手动下载：

```bash
# 方式 1：使用下载脚本
setup-code-server.bat
# 或
npm run download:code-server

# 方式 2：完整构建（自动下载 + 编译）
build.bat
```

> **注意**：下载脚本会自动检查并编译 code-server 的 7 个原生模块（`@vscode/*`）。
> 如编译失败，通常是缺少 VS BuildTools 的 Spectre 库，详见下方故障排查。

### 使用

1. 打开**设置** → **AI 模型** → 点击**添加**
2. 填写提供商名称、API 地址、API Key、模型列表
3. 回到**对话页** → 点击输入框上方的模型选择器 → 切换模型
4. 开始对话！

> 所有配置自动保存，下次启动无需重新填写。

## 架构

```
┌──────────────────────────────────────────┐
│              Tauri 桌面壳                  │
│                                          │
│  React UI (ChatView / Settings)           │
│       │ invoke("chat_stream")            │
│       ▼                                  │
│  Rust (lib.rs)                            │
│       │ reqwest SSE streaming            │
│       ▼                                  │
│  OpenAI / DeepSeek / 通义千问 ...          │
│                                          │
│  数据层                                   │
│  tauri-plugin-store → store.json (本地)   │
└──────────────────────────────────────────┘
```

### 前后端分离编译

```
agent-desktop.exe          ← Rust 编译一次（极少改动）
dist/                      ← 前端文件（独立更新）
  ├── index.html
  ├── assets/
  └── ...
```

| 改了什么 | 命令 | 耗时 |
|----------|------|------|
| 只改 UI（页面/样式/组件） | `build-frontend.bat` | ~2 秒 |
| 改 Rust 逻辑（LLM 调用等） | `cargo check` | ~30 秒 |
| 发布完整包 | `build.bat` | ~5 分钟 |

## 项目结构

```
agent-desktop/
├── index.html                  # HTML 入口
├── package.json                # 前端依赖
├── vite.config.ts              # Vite 配置
├── build-frontend.bat          # 只编译前端（快！）
├── src/                        # React 前端源码
│   ├── main.tsx                # React 入口
│   ├── App.tsx                 # 主布局 + 页面路由
│   ├── components/
│   │   └── Sidebar.tsx         # 侧边栏（导航 + 对话列表）
│   ├── pages/
│   │   ├── ChatView.tsx        # 对话页（流式输出 + 模型切换）
│   │   └── SettingsPage.tsx    # 设置页（Provider 卡片管理）
│   ├── stores/
│   │   └── appStore.ts         # Zustand 状态管理 + 持久化
│   └── styles/
│       └── global.css          # 全局样式（明暗主题）
├── src-tauri/                  # Rust 后端
│   ├── Cargo.toml              # Rust 依赖
│   ├── tauri.conf.json         # Tauri 窗口配置
│   ├── capabilities/           # 权限声明
│   ├── icons/                  # 全平台图标
│   └── src/
│       ├── main.rs             # 可执行入口
│       └── lib.rs              # 核心：LLM 流式 + 自定义协议 + 数据目录初始化
└── dist/                       # Vite 构建产物（外置，不上传 Git）
```

## 技术栈

| 层 | 技术 | 版本 |
|---|---|---|
| 桌面框架 | Tauri | v2 |
| 前端 | React + TypeScript | 19.x |
| 构建 | Vite | 6.x |
| 状态管理 | Zustand | 5.x |
| 后端 | Rust | 1.96 |
| 持久化 | tauri-plugin-store | 2.x |

## 路线图

详见 [TODO.md](TODO.md)

### 已完成
- [x] 流式对话 + 多对话管理 + 上下文管理
- [x] 多模型提供商管理 + 一键切换（OpenAI/Claude/DeepSeek/通义）
- [x] Markdown 渲染 + 代码高亮 + 复制按钮
- [x] 明暗主题 + i18n（中/英）
- [x] 快捷键系统（Ctrl+K/B/L 等）
- [x] 本地持久化（SQLite + store.json + Windows Credential）
- [x] Agent 模式（ReAct 工具循环 + 深度思考可视化）
- [x] MCP 工具系统（14 错误码 + 3 层超时 + 在线市场）
- [x] Skills 技能系统（市场安装 + 7 个预装技能）
- [x] IDE（code-server 独立窗口 + 后台热备）
- [x] RAG 知识库（本地向量嵌入 + LanceDB + 文档检索）
- [x] 内置浏览器（WebView2 + 地址栏导航）

### 进行中 / 规划
- [ ] Agent 集群架构（多 Agent 并行 + 共享工具层）
- [ ] 插件系统完善（npm 分发 + 沙箱 + SDK）
- [ ] 自动更新（Tauri updater）
- [ ] macOS / Linux 适配
- [ ] 代码签名
- [ ] 应用商店上架

## 故障排查

### `Cannot find module '../build/Release/xxx.node'`

code-server 的 7 个原生模块（`@vscode/*`）未被正确编译。原因通常是：

1. **Node.js 版本不对**：code-server v4.127.0 要求 Node.js 22，当前项目已配置 `.nvmrc` 锁定 22
2. **缺少 VS BuildTools 组件**：需安装"使用 C++ 的桌面开发" + Spectre 缓解库
3. **VS 版本太新**：VS 2026 Insiders 可能缺少某些组件

**修复步骤：**

```bash
# 1. 确认 Node.js 版本为 22
node -v  # 应输出 v22.x.x

# 2. 重新下载 code-server（含原生模块自动检测和编译）
npm run download:code-server

# 3. 如仍失败，手动编译 vscode 的原生模块
cd src-tauri/binaries/code-server/release/lib/vscode
npm install --production
```

如果遇到 `MSB8040: Spectre-mitigated libraries are required`，在 VS Installer 中安装 Spectre 库：
- 打开 Visual Studio Installer
- 修改 → 单个组件 → 搜索 "Spectre"
- 勾选 "MSVC v143 - VS 2022 C++ Spectre-mitigated libs"

### `error: failed to run custom build command for ... (protoc)`

缺少 Protocol Buffers 编译器：

```bash
winget install Google.Protobuf
# 或从 https://github.com/protocolbuffers/protobuf/releases 下载解压到 PATH
protoc --version  # 验证安装
```

## License

MIT
