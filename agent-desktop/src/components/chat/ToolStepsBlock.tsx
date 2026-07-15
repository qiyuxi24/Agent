/**
 * ToolStepsBlock — 工具调用步骤展示
 *
 * 展示 Agent 模式下 MCP 工具调用的实时状态。
 * 折叠为一组，点击展开查看详情。
 *
 * 参考：Vercel AI Elements Tool component
 */
import { useState, useCallback } from "react";
import { getMcpErrorInfo } from "../../lib/mcpErrors";

export interface ToolStep {
  name: string;
  args: string;
  status: "running" | "done" | "error";
  result?: string;
  errorCode?: string | null;
  errorCategory?: string | null;
}

interface ToolStepsBlockProps {
  steps: ToolStep[];
}

/** 格式化 JSON，失败返回原文 */
function fmtJson(raw: string): string {
  try {
    return JSON.stringify(JSON.parse(raw), null, 2);
  } catch {
    return raw;
  }
}

/** 工具步骤组的状态摘要 */
function getGroupSummary(steps: ToolStep[]): { label: string; icon: string; isRunning: boolean } {
  const running = steps.filter((s) => s.status === "running").length;
  const done = steps.filter((s) => s.status === "done").length;
  const errors = steps.filter((s) => s.status === "error").length;

  if (running > 0) {
    return {
      label: `正在调用 ${steps.length} 个工具...`,
      icon: "⏳",
      isRunning: true,
    };
  }
  if (errors > 0) {
    return {
      label: `${done} 成功 · ${errors} 失败`,
      icon: "⚠️",
      isRunning: false,
    };
  }
  return {
    label: `已调用 ${done} 个工具`,
    icon: "✅",
    isRunning: false,
  };
}

/** 单个步骤行 */
function StepRow({ step }: { step: ToolStep }) {
  const [showArgs, setShowArgs] = useState(false);
  const [showResult, setShowResult] = useState(false);
  const [copied, setCopied] = useState(false);
  const isError = step.status === "error";
  const isRunning = step.status === "running";
  const errorInfo = isError ? getMcpErrorInfo(step.errorCode) : null;

  const handleCopy = useCallback(() => {
    if (step.result) {
      navigator.clipboard.writeText(step.result).catch(() => {});
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    }
  }, [step.result]);

  return (
    <div className={`tool-step-row ${step.status}`}>
      {/* 名称行 */}
      <div className="tool-step-row-header">
        <span className="tool-step-dot">
          {isRunning ? (
            <span className="tool-step-spinner" />
          ) : isError ? (
            <span className="tool-step-dot-error" />
          ) : (
            <span className="tool-step-dot-done" />
          )}
        </span>
        <code className="tool-step-row-name">{step.name}</code>
        {isRunning && <span className="tool-step-row-status-running">执行中</span>}
        {isError && errorInfo && (
          <span className="tool-step-row-error">{errorInfo.message}</span>
        )}
        {step.status === "done" && (
          <>
            {step.args && step.args !== "{}" && (
              <button
                type="button"
                className="tool-step-inline-btn"
                onClick={() => setShowArgs((v) => !v)}
              >
                {showArgs ? "隐藏参数" : "参数"}
              </button>
            )}
            {step.result && (
              <button
                type="button"
                className="tool-step-inline-btn"
                onClick={() => setShowResult((v) => !v)}
              >
                {showResult ? "隐藏结果" : "结果"}
              </button>
            )}
          </>
        )}
      </div>

      {/* 参数展开 */}
      {showArgs && (
        <pre className="tool-step-inline-code">{fmtJson(step.args)}</pre>
      )}

      {/* 结果展开 */}
      {showResult && step.result && (
        <div className="tool-step-inline-result">
          <div className="tool-step-result-actions">
            <button type="button" className="tool-step-copy-btn" onClick={handleCopy}>
              {copied ? "已复制" : "复制"}
            </button>
          </div>
          <pre className={`tool-step-inline-code ${isError ? "error" : ""}`}>
            {step.result.length > 2000
              ? step.result.slice(0, 2000) + `\n\n... (共 ${step.result.length} 字符)`
              : step.result}
          </pre>
        </div>
      )}
    </div>
  );
}

export default function ToolStepsBlock({ steps }: ToolStepsBlockProps) {
  const [expanded, setExpanded] = useState(true);
  const summary = getGroupSummary(steps);

  if (steps.length === 0) return null;

  return (
    <div className={`tool-steps-block ${summary.isRunning ? "tool-steps-running" : ""}`}>
      {/* 组头：折叠/展开 */}
      <button
        type="button"
        className="tool-steps-header"
        onClick={() => setExpanded((v) => !v)}
      >
        <span className="tool-steps-header-left">
          <span className="tool-steps-header-icon">{summary.icon}</span>
          <span className={`tool-steps-header-label ${summary.isRunning ? "shimmer-text" : ""}`}>
            {summary.label}
          </span>
        </span>
        <span className={`tool-steps-chevron ${expanded ? "" : "collapsed"}`}>
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <polyline points="6 9 12 15 18 9" />
          </svg>
        </span>
      </button>

      {/* 步骤列表 */}
      <div className={`tool-steps-list-wrapper ${expanded ? "" : "collapsed"}`}>
        <div className="tool-steps-list-inner">
          {steps.map((s, i) => (
            <StepRow key={`${s.name}-${i}`} step={s} />
          ))}
        </div>
      </div>
    </div>
  );
}
