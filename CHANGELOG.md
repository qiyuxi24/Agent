# Changelog

本项目遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/) 语义化版本规范。

格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.0.0/)。

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
