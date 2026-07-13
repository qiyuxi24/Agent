---
name: React 前端开发
description: React + TypeScript 前端开发约定与 Tauri 前后端交互模式
description_zh: React + TypeScript 前端开发约定与 Tauri 前后端交互模式
description_en: React + TypeScript frontend conventions and Tauri front-backend interaction
version: 0.1.0
metadata:
  category: frontend
---
React + TypeScript 前端开发约定（目录 `agent-desktop/src/`）。

## 结构
- 页面 `src/pages/`：`ChatView.tsx`（对话页）、`SettingsPage.tsx`（设置页）、`BrowserPanel.tsx`、`IdePage.tsx`。
- 全局状态 `src/stores/appStore.ts`（基于 Zustand）：对话、模型提供商、MCP 服务器、对话模式 `chatMode`（"chat" | "agent"）等；持久化到 `store.json`（API Key 加密）。
- 国际化 `src/i18n/locales/zh-CN.json` 与 `en.json`，用 `t("key")`。
- 样式放 `src/styles/*.css`，主题变量如 `--accent`、`--bg-primary`、`--text-primary`、`--border-color`（改样式优先复用这些变量）。

## 与后端交互
- Tauri 环境：`import("@tauri-apps/api/core")` 的 `invoke("命令名", { ... })` 调用 Rust 命令；`import("@tauri-apps/api/event")` 的 `listen` 监听事件流。
- 浏览器环境：直接 `fetch` LLM `/chat/completions`（纯文本流式，无工具）。

## 约定
- 用函数组件 + hooks；TypeScript 严格模式。
- 编辑优先 `replace_in_file` 局部修改；改动后 `read_lints` 自查（目标 0 错误）。
- 新增对话相关 UI 注意区分 chat / agent 模式（参考 `ChatView.tsx` 的 `chatMode`）。
- 切换类 UI 默认把 `margin-left: auto` 把控件推到工具栏右侧。
