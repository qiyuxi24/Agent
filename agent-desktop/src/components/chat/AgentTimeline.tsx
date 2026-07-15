/**
 * AgentTimeline — Agent 工作流时间线可视化
 *
 * 将思考→工具调用→结果→最终答案的流程可视化为垂直时间线。
 * 每个阶段作为一个节点，用连线串联，清晰展示 Agent 工作流全貌。
 *
 * 设计参考：LangSmith trace view / Vercel AI SDK useChat
 */
import { useState } from "react";
import MarkdownRenderer from "../MarkdownRenderer";
import type { ThinkingStats } from "./ThinkingBlock";
import type { ToolStep } from "./ToolStepsBlock";

export interface TimelinePhase {
  type: "thinking" | "tool" | "answer";
  /** 阶段标签 */
  label: string;
  /** 阶段状态 */
  status: "pending" | "active" | "done" | "error";
  /** 思考内容（thinking 阶段） */
  thinkingText?: string;
  thinkingStats?: ThinkingStats;
  /** 工具步骤（tool 阶段） */
  toolSteps?: ToolStep[];
  /** 回答内容（answer 阶段） */
  answerText?: string;
}

interface AgentTimelineProps {
  phases: TimelinePhase[];
}

/** 阶段图标映射 */
const phaseIcon: Record<TimelinePhase["type"], string> = {
  thinking: "🧠",
  tool: "🔧",
  answer: "💬",
};

function PhaseNode({ phase }: { phase: TimelinePhase }) {
  const [expanded, setExpanded] = useState(
    phase.status === "active" || phase.status === "pending",
  );

  const isThinking = phase.type === "thinking";
  const isTool = phase.type === "tool";
  const isAnswer = phase.type === "answer";

  const statusClass =
    phase.status === "active"
      ? "timeline-node-active"
      : phase.status === "error"
        ? "timeline-node-error"
        : phase.status === "pending"
          ? "timeline-node-pending"
          : "timeline-node-done";

  return (
    <div className={`timeline-node ${statusClass}`}>
      {/* 节点头部 */}
      <div className="timeline-node-header" onClick={() => setExpanded(!expanded)}>
        <span className="timeline-node-icon">{phaseIcon[phase.type]}</span>
        <span className="timeline-node-label">{phase.label}</span>
        <span className="timeline-node-status">
          {phase.status === "active" && <span className="timeline-spinner" />}
          {phase.status === "active" && "执行中"}
          {phase.status === "done" && "✓"}
          {phase.status === "error" && "✗"}
          {phase.status === "pending" && "等待中"}
        </span>
        {isThinking && phase.thinkingStats && phase.status === "done" && (
          <span className="timeline-node-meta">
            {(phase.thinkingStats.durationMs / 1000).toFixed(1)}s ·{" "}
            {phase.thinkingStats.tokens} tokens
          </span>
        )}
        <span className={`timeline-chevron ${expanded ? "" : "collapsed"}`}>▾</span>
      </div>

      {/* 节点内容 */}
      {expanded && (
        <div className="timeline-node-body">
          {isThinking && phase.thinkingText && (
            <div className="timeline-thinking-content">
              <MarkdownRenderer content={phase.thinkingText} />
            </div>
          )}
          {isTool &&
            phase.toolSteps?.map((step, i) => (
              <div key={`${step.name}-${i}`} className="timeline-tool-item">
                <span className="timeline-tool-name">{step.name}</span>
                <span className={`timeline-tool-status ${step.status}`}>
                  {step.status === "running" && "⏳"}
                  {step.status === "done" && "✅"}
                  {step.status === "error" && "❌"}
                </span>
              </div>
            ))}
          {isAnswer && phase.answerText && (
            <div className="timeline-answer-preview">
              {phase.answerText.slice(0, 120)}
              {phase.answerText.length > 120 && "..."}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

export default function AgentTimeline({ phases }: AgentTimelineProps) {
  if (phases.length === 0) return null;

  return (
    <div className="agent-timeline">
      {phases.map((phase, i) => (
        <div key={`${phase.type}-${i}`} className="timeline-row">
          {/* 时间线连线 + 圆点 */}
          <div className="timeline-track">
            <div className={`timeline-dot ${phase.status}`} />
            {i < phases.length - 1 && <div className="timeline-line" />}
          </div>
          {/* 节点内容 */}
          <div className="timeline-content">
            <PhaseNode phase={phase} />
          </div>
        </div>
      ))}
    </div>
  );
}
