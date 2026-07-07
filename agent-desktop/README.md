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
- **Node.js** >= 18
- **Rust**（rustc + cargo）

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

- [x] 流式对话 + 多对话管理
- [x] 多模型提供商管理 + 一键切换
- [x] 明暗主题
- [x] 本地持久化（对话记录 + 设置）
- [x] 前后端分离编译
- [x] 首次启动自动初始化
- [ ] 消息 Markdown 渲染
- [ ] 快捷键支持
- [ ] 系统托盘
- [ ] 自动更新

## License

MIT
