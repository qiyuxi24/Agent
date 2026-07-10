# Changelog

本项目遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/) 语义化版本规范。

格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.0.0/)。

---

## [Unreleased]

### 新增

- **内置浏览器**：基于 WebView2 原生内核的完整浏览器
  - 使用 Tauri WebviewBuilder 子 webview（与 Edge 同内核）
  - Rust 侧 `browser.rs` 模块 + 9 个 Tauri commands
  - 事件驱动地址栏同步 + 原生历史栈
  - ResizeObserver 自动适配窗口大小

- **Skills 技能生态系统**：设置页新增 Skills 面板
  - Rust 侧 `skills.rs` 模块 + 6 个 Tauri commands
  - 技能市场：从 GitHub 获取列表，支持分类筛选和一键安装
  - 7 个预设技能：前端开发、全栈开发、MCP 构建器、Prompt 工程等

- **MCP 工具面板增强**：市场推荐 + 实时状态
  - 5 个推荐服务器一键安装
  - 15 秒定时刷新连接状态，连接脉冲动画

### 修复

- MCP 错误码体系：14 类错误码 + 重试/重连策略标记
- 工具调用超时保护：多种超时机制防止对话阻塞
- 进程健康检测 + stderr 捕获
- 对话降级保护：进程退出时自动中止后续工具调用

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
