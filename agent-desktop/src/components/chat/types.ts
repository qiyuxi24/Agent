/**
 * MessagePart — 消息部分类型系统
 *
 * 一条消息可以由多种"部分"组成，按到达顺序排列：
 * thinking → tool_call → content
 *
 * 设计参考：Vercel AI SDK MessagePrimitive.Parts / MUI X Chat
 *
 * 每种 part 都有独立的渲染组件，通过 MessageContent 统一编排。
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

/** 工具调用部分 */
export interface ToolCallPartData {
  type: "tool_call";
  steps: ToolStep[];
}

/**
 * 将 Message 的扁平字段 + 流式上下文转换为 MessagePart 列表
 *
 * 这是 Message → Parts 的桥接层，使得 ChatView 无需关心
 * 数据是来自 store 还是实时流式状态。
 *
 * 顺序：thinking → tool_call → content（最终回答始终在最后，始终可见）
 */
export function messageToParts(params: {
  storedThinking?: string;
  thinkingStats?: ThinkingStats;
  content: string;
  isStreaming: boolean;
  isLastMessage: boolean;
  streamingThinking?: string;
  isThinking: boolean;
  toolSteps?: ToolStep[];
  storedToolSteps?: ToolStep[];
}): MessagePart[] {
  const parts: MessagePart[] = [];

  const {
    storedThinking, thinkingStats, content, isStreaming,
    isLastMessage, streamingThinking, isThinking, toolSteps, storedToolSteps,
  } = params;

  // 1) 思考部分：流式优先 → 已持久化
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

  // 2) 工具调用部分：最后一条用实时，历史用持久化
  const displaySteps = isLastMessage && toolSteps && toolSteps.length > 0
    ? toolSteps
    : storedToolSteps;

  if (displaySteps && displaySteps.length > 0) {
    parts.push({
      type: "tool_call",
      steps: displaySteps,
    });
  }

  // 3) 回答内容部分 — 始终最后，始终可见
  parts.push({
    type: "content",
    text: content,
  });

  return parts;
}
