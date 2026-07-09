/**
 * MCP 错误码表 — 前端映射与处理策略
 *
 * 后端通过 tool-result 事件传递 error_code + error_category + suggested_action，
 * 前端据此决定 UI 展示和用户引导。
 */

/** 错误码到中文描述的映射 */
export const MCP_ERROR_MESSAGES: Record<string, string> = {
  "MCP-001": "工具调用超时",
  "MCP-002": "MCP 进程已退出",
  "MCP-003": "工具执行错误",
  "MCP-004": "MCP 连接已关闭",
  "MCP-005": "MCP 服务器未连接",
  "MCP-006": "工具名格式错误",
  "MCP-007": "参数解析失败",
  "MCP-008": "通信 I/O 错误",
  "MCP-009": "JSON 解析失败",
  "MCP-010": "进程启动失败",
  "MCP-011": "MCP 初始化失败",
  "MCP-012": "LLM API 网络错误",
  "MCP-013": "LLM API 错误",
  "MCP-014": "LLM 流式读取错误",
};

/** 错误码对应的操作建议 */
export const MCP_ERROR_ACTIONS: Record<string, string> = {
  "MCP-001": "可重试：工具计算时间过长，建议 LLM 更换策略重试",
  "MCP-002": "需重连：工具服务进程已崩溃，请前往设置重新连接",
  "MCP-003": "可修正：工具执行出错，错误信息已传回 AI，它会尝试修正",
  "MCP-004": "需重连：MCP 连接异常断开，请前往设置重新连接",
  "MCP-005": "需连接：该服务器未连接，请前往设置添加",
  "MCP-006": "内部错误：工具名称格式不正确，请反馈给开发者",
  "MCP-007": "可修正：工具参数格式有误，错误信息已传回 AI",
  "MCP-008": "需重连：进程通信中断，请前往设置重新连接",
  "MCP-009": "内部错误：数据解析异常，可尝试重试",
  "MCP-010": "需配置：进程无法启动，请检查命令路径和依赖",
  "MCP-011": "需配置：MCP 握手失败，请检查服务器兼容性",
  "MCP-012": "网络问题：连接 LLM API 失败，请检查网络和配置",
  "MCP-013": "API 问题：LLM 服务返回错误，请检查 API Key 或配额",
  "MCP-014": "网络问题：响应中断，已显示的内容已保留",
};

/** 错误码对应显示的图标 */
export const MCP_ERROR_ICONS: Record<string, string> = {
  "MCP-001": "⏱️",
  "MCP-002": "💥",
  "MCP-003": "⚠️",
  "MCP-004": "🔌",
  "MCP-005": "❓",
  "MCP-006": "🐛",
  "MCP-007": "📝",
  "MCP-008": "🔌",
  "MCP-009": "🔧",
  "MCP-010": "🚫",
  "MCP-011": "🤝",
  "MCP-012": "🌐",
  "MCP-013": "🔑",
  "MCP-014": "🌊",
};

/** 根据错误码获取完整错误信息 */
export function getMcpErrorInfo(code: string | null | undefined) {
  if (!code) return null;
  return {
    code,
    message: MCP_ERROR_MESSAGES[code] || `未知错误 (${code})`,
    action: MCP_ERROR_ACTIONS[code] || "",
    icon: MCP_ERROR_ICONS[code] || "❌",
  };
}

/** 判断错误码是否需要提示用户重新连接 */
export function needsReconnect(code: string | null | undefined): boolean {
  if (!code) return false;
  return ["MCP-002", "MCP-004", "MCP-005", "MCP-008", "MCP-010", "MCP-011"].includes(code);
}

/** 判断错误码是否可自动重试 */
export function isRetryable(code: string | null | undefined): boolean {
  if (!code) return false;
  return ["MCP-001", "MCP-003", "MCP-007", "MCP-009", "MCP-012", "MCP-014"].includes(code);
}
