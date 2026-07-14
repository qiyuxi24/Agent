# Changelog

本项目遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/) 语义化版本规范。

格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.0.0/)。

---

## [Unreleased]

### 新增

- **Code Server IDE**：完整 VS Code 内核，独立窗口体验
  - code-server v4.127.0 自动下载安装（~212MB）
  - 独立 Tauri 窗口（非 iframe 嵌入）
  - 应用启动时后台热备，秒开 IDE
  - 完整 VS Code 插件生态（.vsix）
  - `--auth none` 本地免密模式

- **RAG 知识库**：本地向量检索 + 文档问答
  - fastembed 本地 ONNX 嵌入（BGE 中文模型，~47MB）
  - text-splitter 语义分块 + LanceDB 向量存储
  - 文档上传/列表/删除 + 检索测试面板
  - Embedder trait 解耦，切换嵌入方案零成本

- **深度思考可视化**：支持 reasoning_content 展示
  - 后端 `ThinkingStart/ThinkingDelta/ThinkingStop` 事件
  - 前端 ThinkingBlock 折叠面板组件
  - 适用于 DeepSeek-R1 等推理模型

- **内置浏览器**：基于 WebView2 原生内核的完整浏览器
  - 使用 Tauri WebviewBuilder 子 webview（与 Edge 同内核）
  - Rust 侧 `browser.rs` 模块 + 9 个 Tauri commands
  - 事件驱动地址栏同步 + 原生历史栈

- **Skills 技能生态系统**：设置页新增 Skills 面板
  - Rust 侧 `skills.rs` 模块 + 6 个 Tauri commands
  - 技能市场：从 GitHub 获取列表，支持分类筛选和一键安装
  - 7 个预设技能：前端开发、全栈开发、MCP 构建器、Prompt 工程等
  - Agent 模式自动注入启用的 Skills 到 system prompt

- **MCP 工具系统增强**：
  - 3 个内置 Server（web/tavily/sqlite）+ 在线市场
  - 14 类错误码 + 多级超时保护 + 自动重连
  - 工具调用缓存（TTL 60s）+ 健康检查 + 错误计数

### 修复

- **可移植性修复**（2026-07-13）：
  - Node.js 版本锁定 22（.nvmrc + engines 字段）
  - PROTOC 路径注释指引（去除硬编码用户路径依赖）
  - 原生模块自动检测 + 编译（7 个 @vscode/* .node 文件）
  - build.rs 新增 `check_native_modules()` 编译时检查
  - tauri.conf.json $schema 修正为 tauri 官方 URL
  - start-dev.bat 自动检查并下载 code-server
- MCP 错误码体系：14 类错误码 + 重试/重连策略标记
- 工具调用超时保护：多种超时机制防止对话阻塞
- 进程健康检测 + stderr 捕获
- 对话降级保护：进程退出时自动中止后续工具调用
- build.bat CMD 语法转义修复（`<--` → `^<--`）

---

## [0.1.0] - 2026-07-09

> 当前发布版本

### 核心功能

- **流式 AI 对话**：打字机效果实时展示，支持中途取消生成
- **多模型管理**：支持 OpenAI / DeepSeek / 通义千问等兼容接口
- **多对话管理**：侧边栏创建/切换/删除对话，自动生成标题
- **Markdown 渲染**：代码高亮、表格、列表等富文本
- **明暗主题**：跟随系统 / 浅色 / 深色，即时切换
- **本地持久化**：对话记录、API Key 全部存本地，不上传服务器
- **环境自适应**：Tauri 桌面端 Rust 调用；浏览器直接 fetch 直连调试

### 设置系统

- 快捷键：Ctrl+N/L/K/, 支持点击录制自定义
- Provider 管理：卡片式增删改查，多模型
- 主题 / 语言切换
- MCP 服务器连接管理
- 插件管理面板
- Skills 管理面板

### 技术栈

- **桌面壳**：Tauri v2 (Rust) + React 19 + TypeScript
- **状态管理**：Zustand 5 + tauri-plugin-store 持久化
- **后端**：reqwest SSE 流式 + tokio 异步
- **打包**：NSIS 安装器，内置 WebView2 引导
- **CI/CD**：GitHub Actions 自动构建发布

### 构建

- `start-dev.bat` — 一键开发环境
- `build.bat` — 完整打包
- `scripts/release.bat` — 发布脚本（版本号管理 + 构建 + tag）
- 详见 [RELEASE.md](RELEASE.md)
