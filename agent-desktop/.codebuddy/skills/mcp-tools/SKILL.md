---
name: MCP 工具与 Agent 模式
description: MCP 工具来源、Agent 模式下的工具调用循环约定，以及如何新增 MCP 服务器
description_zh: MCP 工具来源、Agent 模式下的工具调用循环约定，以及如何新增 MCP 服务器
description_en: MCP tool sources, the agent-mode tool-calling loop, and how to add an MCP server
version: 0.1.0
metadata:
  category: mcp
---
MCP 工具与 Agent 模式下的工具调用约定。

## 工具来源
- 工具来自**已连接的 MCP 服务器**，后端 `McpManager::llm_tools()` 聚合成 OpenAI `tools` 格式。
- 用户在「设置 → Tools」面板管理 MCP 服务器（内置 `web`、`tavily`，也可一键安装推荐项或手动添加）。
- 启用的 **Skills** 会作为 system prompt 注入（本目录每个 `SKILL.md` 的正文）。
- 工具调用只在 **Tauri 桌面版 + Agent 模式** 时生效；浏览器版不支持工具。

## 循环（Agent Loop）
后端 `chat_stream` 已内置完整工具调用循环（即 agent loop 内核）：
1. 调 LLM（带 tools + 注入的 skills prompt）；
2. 若返回 `tool_calls` → 逐个执行（带超时 / 错误码），结果作为 `role: "tool"` 消息回传；
3. 单个工具失败不会中断对话，错误结果同样回传，由模型自行决定下一步；
4. 无 `tool_calls` 即视为最终答案；最多 `max_iterations=10` 轮。

## 新增一个 MCP 服务器
- 编写 stdio MCP server（Node / Python 实现 `tools/list` 与 `tools/call`）。
- 在 `appStore.mcpServers` 增加配置 `{ name, command, args, env? }`，或在 Tools 面板添加；运行时由 `mcp_connect` 拉起子进程。
