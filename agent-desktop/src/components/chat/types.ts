/**
 * MessagePart — 消息部分类型系统
 *
 * 一条消息可以由多种"部分"组成，按到达顺序排列：
 * thinking → tool_call → tool_result → thinking → content
 *
 * 设计参考：MUI X Chat (ChatReasoningMessagePart) / assistant-ui (MessagePrimitive.Parts)
 *
 * 每种 part 都有独立的渲染组件，通过 MessageContent 统一编排。
 * 未来新增 part（如 file_preview、image_gen_progress）只需加类型 + 组件。
 */

import type { ThinkingStats } from "./ThinkingBlock";
import type { ToolStep } from "./ToolStepsBlock";

/** 所有消息部分的联合类型 */
export type MessagePart =
  | ThinkingPartData
  | ContentPartData
  | ToolCallPartData;

/** 思考部分 */
export interface ThinkingPartData {
  type: "thinking";
  text: string;
  state: "streaming" | "done";
  stats?: ThinkingStats;
}

/** 回答内容部分 */
export interface ContentPartData {
  type: "content";
  text: string;
}

/** 工具调用部分（运行中的调用 + 最终结果合并为一个部分） */
export interface ToolCallPartData {
  type: "tool_call";
  steps: ToolStep[];
}

/**
 * 将 Message 的扁平字段 + 流式上下文转换为 MessagePart 列表
 *
 * 这是 Message → Parts 的桥接层，使得 ChatView 无需关心
 * 数据是来自 store 还是实时流式状态。
 */
export function messageToParts(params: {
  /** 消息的 thinking 字段（已持久化） */
  storedThinking?: string;
  /** 消息的 thinkingStats */
  thinkingStats?: ThinkingStats;
  /** 消息的 content */
  content: string;
  /** 是否正在流式输出 */
  isStreaming: boolean;
  /** 是否是最后一条消息 */
  isLastMessage: boolean;
  /** 实时思考内容（尚未持久化到 store） */
  streamingThinking?: string;
  /** 是否仍在思考阶段 */
  isThinking: boolean;
  /** 工具调用步骤 */
  toolSteps?: ToolStep[];
}): MessagePart[] {
  const parts: MessagePart[] = [];

  const { storedThinking, thinkingStats, content, isStreaming, isLastMessage, streamingThinking, isThinking, toolSteps } = params;

  // 1) 思考部分：优先用流式内容，否则用已持久化的
  const thinkingText = isLastMessage && streamingThinking
    ? streamingThinking
    : storedThinking || "";

  const hasThinking = !!thinkingText || (isLastMessage && isThinking);

  if (hasThinking) {
    parts.push({
      type: "thinking",
      text: thinkingText,
      state: isLastMessage && isThinking && isStreaming ? "streaming" : "done",
      stats: thinkingStats,
    });
  }

  // 2) 工具调用部分
  if (toolSteps && toolSteps.length > 0) {
    parts.push({
      type: "tool_call",
      steps: toolSteps,
    });
  }

  // 3) 回答内容部分
  parts.push({
    type: "content",
    text: content,
  });

  return parts;
}
