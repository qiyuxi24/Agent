/**
 * MessageContent — 消息内容统一渲染器
 *
 * 将 Message + 流式上下文转换为 MessagePart 列表后，逐个渲染。
 *
 * 设计原则：
 * - ChatView 只负责消息列表循环，不关心每种 part 如何渲染
 * - 新增 part 类型只需在 switch 里加一个 case
 * - 每种 part 的渲染组件完全独立，无耦合
 */
import { memo } from "react";
import { useTranslation } from "react-i18next";
import MarkdownRenderer from "../MarkdownRenderer";
import ErrorBoundary from "../ErrorBoundary";
import ThinkingBlock from "./ThinkingBlock";
import ToolStepsBlock from "./ToolStepsBlock";
import { messageToParts } from "./types";
import type { MessagePart } from "./types";
import type { ThinkingStats } from "./ThinkingBlock";
import type { ToolStep } from "./ToolStepsBlock";

// ========== 子渲染器 ==========

/** Markdown + Error Boundary 降级 */
function SafeMarkdown({ content }: { content: string }) {
  return (
    <ErrorBoundary fallback={<p className="md-fallback">{content}</p>}>
      <MarkdownRenderer content={content} />
    </ErrorBoundary>
  );
}

/** 思考 part → ThinkingBlock */
function RenderThinking({ part }: { part: Extract<MessagePart, { type: "thinking" }> }) {
  return (
    <ThinkingBlock
      thinking={part.text}
      stats={part.stats}
      streaming={part.state === "streaming"}
    />
  );
}

/** 工具调用 part → ToolStepsBlock */
function RenderToolCall({ part }: { part: Extract<MessagePart, { type: "tool_call" }> }) {
  return <ToolStepsBlock steps={part.steps} />;
}

/** 回答内容 part → Markdown 气泡 */
function RenderContent({
  part,
  isLoading,
  isLastMessage,
  t,
}: {
  part: Extract<MessagePart, { type: "content" }>;
  isLoading: boolean;
  isLastMessage: boolean;
  t: (key: string) => string;
}) {
  return (
    <div className="message-bubble message-bubble-markdown">
      {part.text ? (
        <SafeMarkdown content={part.text} />
      ) : isLoading && isLastMessage ? (
        <span className="thinking-text">{t("chat.thinking")}</span>
      ) : null}
    </div>
  );
}

// ========== 分区渲染器注册表 ==========

interface PartRendererProps {
  part: MessagePart;
  isLoading: boolean;
  isLastMessage: boolean;
  t: (key: string) => string;
}

function PartRenderer({ part, isLoading, isLastMessage, t }: PartRendererProps) {
  switch (part.type) {
    case "thinking":
      return <RenderThinking part={part} />;
    case "tool_call":
      return <RenderToolCall part={part} />;
    case "content":
      return <RenderContent part={part} isLoading={isLoading} isLastMessage={isLastMessage} t={t} />;
    default:
      return null;
  }
}

// ========== 主组件 ==========

interface MessageContentProps {
  role: "user" | "assistant" | "system";
  content: string;
  /** 已持久化的思考内容 */
  storedThinking?: string;
  thinkingStats?: ThinkingStats;
  /** 是否正在等待 LLM 响应 */
  isLoading: boolean;
  /** 是否为当前消息列表的最后一条 */
  isLastMessage: boolean;
  /** 实时思考内容（流式，尚未持久化到 store） */
  streamingThinking?: string;
  /** 是否仍在思考阶段（thinking-start 已触发但 thinking-stop 未触发） */
  isThinking: boolean;
  /** 工具调用步骤 */
  toolSteps?: ToolStep[];
}

/** 用 memo 避免非必要重渲染 */
const MessageContent = memo(function MessageContent({
  role,
  content,
  storedThinking,
  thinkingStats,
  isLoading,
  isLastMessage,
  streamingThinking,
  isThinking,
  toolSteps,
}: MessageContentProps) {
  const { t } = useTranslation();

  // 用户消息：纯文本，不走 part 系统
  if (role === "user") {
    return <div className="message-bubble">{content}</div>;
  }

  // Assistant 消息：转换为 parts 后逐个渲染
  const parts = messageToParts({
    storedThinking,
    thinkingStats,
    content,
    isStreaming: isLoading,
    isLastMessage,
    streamingThinking,
    isThinking,
    toolSteps,
  });

  return (
    <div className="message-content">
      {parts.map((part, i) => (
        <PartRenderer
          key={`${part.type}-${i}`}
          part={part}
          isLoading={isLoading}
          isLastMessage={isLastMessage}
          t={t}
        />
      ))}
    </div>
  );
});

export default MessageContent;
