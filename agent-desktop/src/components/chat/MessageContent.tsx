/**
 * MessageContent — 消息内容统一渲染器
 *
 * 将 Message + 流式上下文转换为 MessagePart 列表后，逐个渲染。
 *
 * 设计原则：
 * - ChatView 只负责消息列表循环，不关心每种 part 如何渲染
 * - 新增 part 类型只需在 switch 里加一个 case
 * - 每种 part 的渲染组件完全独立，无耦合
 * - 当存在多阶段（thinking+tool+content）时，使用 AgentTimeline 统一展示工作流
 */
import { memo, useState, useMemo } from "react";
import { useTranslation } from "react-i18next";
import MarkdownRenderer from "../MarkdownRenderer";
import ErrorBoundary from "../ErrorBoundary";
import ThinkingBlock from "./ThinkingBlock";
import ToolStepsBlock from "./ToolStepsBlock";
import AgentTimeline from "./AgentTimeline";
import type { TimelinePhase } from "./AgentTimeline";
import { messageToParts, partsToTimeline } from "./types";
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
  /** 实时工具调用步骤（流式阶段，仅最后一条消息使用） */
  toolSteps?: ToolStep[];
  /** 已持久化的工具调用步骤（历史消息回顾） */
  storedToolSteps?: ToolStep[];
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
  storedToolSteps,
}: MessageContentProps) {
  const { t } = useTranslation();
  const [viewTimeline, setViewTimeline] = useState(true);

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
    storedToolSteps,
  });

  // 当存在多个阶段（thinking + tool + content）时，提供时间线视图
  const timeline = useMemo(() => partsToTimeline(parts), [parts]);
  const hasWorkflow = timeline.length > 1;

  return (
    <div className="message-content">
      {/* 工作流时间线视图（多阶段消息默认使用） */}
      {hasWorkflow && viewTimeline && (
        <>
          <div className="view-toggle">
            <button
              type="button"
              className={`view-toggle-btn active`}
              disabled
            >
              时间线
            </button>
            <button
              type="button"
              className="view-toggle-btn"
              onClick={() => setViewTimeline(false)}
              title="切换为卡片视图"
            >
              卡片
            </button>
          </div>
          <AgentTimeline phases={timeline} />
        </>
      )}

      {/* 卡片视图（单阶段或无工作流时使用） */}
      {(!hasWorkflow || !viewTimeline) &&
        parts.map((part, i) => (
          <PartRenderer
            key={`${part.type}-${i}`}
            part={part}
            isLoading={isLoading}
            isLastMessage={isLastMessage}
            t={t}
          />
        ))}

      {/* 存在工作流但用户在卡片视图时，显示切换回时间线的按钮 */}
      {hasWorkflow && !viewTimeline && (
        <button
          type="button"
          className="view-toggle-switch"
          onClick={() => setViewTimeline(true)}
        >
          切换为时间线视图
        </button>
      )}
    </div>
  );
});

export default MessageContent;
