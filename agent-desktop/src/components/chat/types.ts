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
import type { TimelinePhase } from "./AgentTimeline";

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
  /** 实时工具调用步骤（流式） */
  toolSteps?: ToolStep[];
  /** 已持久化的工具调用步骤（历史消息） */
  storedToolSteps?: ToolStep[];
}): MessagePart[] {
  const parts: MessagePart[] = [];

  const { storedThinking, thinkingStats, content, isStreaming, isLastMessage, streamingThinking, isThinking, toolSteps, storedToolSteps } = params;

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

  // 2) 工具调用部分：最后一条消息用实时 toolSteps，历史消息用 storedToolSteps
  const displaySteps = isLastMessage && toolSteps && toolSteps.length > 0
    ? toolSteps
    : storedToolSteps;

  if (displaySteps && displaySteps.length > 0) {
    parts.push({
      type: "tool_call",
      steps: displaySteps,
    });
  }

  // 3) 回答内容部分
  parts.push({
    type: "content",
    text: content,
  });

  return parts;
}

/**
 * 将 MessagePart 列表转换为 AgentTimeline 的阶段列表。
 * 当消息包含多阶段工作流时，用时间线统一展示。
 */
export function partsToTimeline(parts: MessagePart[]): TimelinePhase[] {
  const phases: TimelinePhase[] = [];

  for (const part of parts) {
    if (part.type === "thinking") {
      phases.push({
        type: "thinking",
        label: "深度思考",
        status: part.state === "streaming" ? "active" : "done",
        thinkingText: part.text,
        thinkingStats: part.stats,
      });
    } else if (part.type === "tool_call") {
      const hasRunning = part.steps.some((s) => s.status === "running");
      const hasError = part.steps.some((s) => s.status === "error");
      phases.push({
        type: "tool",
        label: `工具调用 (${part.steps.length})`,
        status: hasRunning ? "active" : hasError ? "error" : "done",
        toolSteps: part.steps,
      });
    } else if (part.type === "content" && part.text) {
      phases.push({
        type: "answer",
        label: "最终回答",
        status: "done",
        answerText: part.text,
      });
    }
  }

  return phases;
}
