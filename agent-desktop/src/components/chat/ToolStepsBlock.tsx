/**
 * ToolStepsBlock — 工具调用步骤展示
 *
 * 展示 Agent 模式下 MCP 工具调用的实时状态。
 *
 * 特性：
 * - 运行中/成功/失败三种状态视觉区分
 * - 点击展开查看完整参数（JSON 格式化）
 * - 点击展开查看完整结果
 * - 复制结果按钮
 * - 流畅展开/折叠动画
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

/** 格式化 JSON 字符串，失败则返回原文 */
function fmtJson(raw: string): string {
  try {
    return JSON.stringify(JSON.parse(raw), null, 2);
  } catch {
    return raw;
  }
}

/** 单个工具步骤 */
function ToolStepItem({ step }: { step: ToolStep }) {
  const [showArgs, setShowArgs] = useState(false);
  const [showResult, setShowResult] = useState(false);
  const [copied, setCopied] = useState(false);

  const isError = step.status === "error";
  const isRunning = step.status === "running";
  const errorInfo = isError ? getMcpErrorInfo(step.errorCode) : null;
  const hasArgs = step.args && step.args !== "{}";
  const hasResult = !!step.result;

  const handleCopy = useCallback(() => {
    if (step.result) {
      navigator.clipboard.writeText(step.result).catch(() => {});
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    }
  }, [step.result]);

  return (
    <div className={`tool-step ${step.status}`}>
      {/* 标题行：图标 + 名称 + 状态 */}
      <div className="tool-step-header">
        <span className="tool-step-icon">
          {isRunning ? "⏳" : isError ? errorInfo?.icon || "❌" : "✅"}
        </span>
        <span className="tool-step-name">{step.name}</span>
        {isError && errorInfo && (
          <span className="tool-step-error" title={errorInfo.action}>
            [{errorInfo.code}] {errorInfo.message}
          </span>
        )}
        {isRunning && <span className="tool-step-running-label">执行中...</span>}
      </div>

      {/* 可展开的参数区 */}
      {hasArgs && (
        <div className="tool-step-section">
          <button
            type="button"
            className="tool-step-section-toggle"
            onClick={() => setShowArgs((v) => !v)}
          >
            <span className={`tool-step-chevron ${showArgs ? "" : "collapsed"}`}>▾</span>
            参数
          </button>
          <div className={`tool-step-section-content ${showArgs ? "" : "collapsed"}`}>
            <pre className="tool-step-code">{fmtJson(step.args)}</pre>
          </div>
        </div>
      )}

      {/* 可展开的结果区 */}
      {hasResult && (
        <div className="tool-step-section">
          <button
            type="button"
            className="tool-step-section-toggle"
            onClick={() => setShowResult((v) => !v)}
          >
            <span className={`tool-step-chevron ${showResult ? "" : "collapsed"}`}>▾</span>
            结果
            <span className="tool-step-section-badge">
              {step.result!.length > 200 ? `${step.result!.length} 字符` : ""}
            </span>
          </button>
          <div className={`tool-step-section-content ${showResult ? "" : "collapsed"}`}>
            <div className="tool-step-result-header">
              <button type="button" className="tool-step-copy-btn" onClick={handleCopy}>
                {copied ? "已复制" : "复制"}
              </button>
            </div>
            <pre className={`tool-step-code ${isError ? "error" : ""}`}>
              {step.result!.length > 2000
                ? step.result!.slice(0, 2000) + `\n\n... (共 ${step.result!.length} 字符，已截断显示)`
                : step.result}
            </pre>
          </div>
        </div>
      )}
    </div>
  );
}

export default function ToolStepsBlock({ steps }: ToolStepsBlockProps) {
  if (steps.length === 0) return null;

  return (
    <div className="tool-steps">
      {steps.map((s, i) => (
        <ToolStepItem key={`${s.name}-${i}`} step={s} />
      ))}
    </div>
  );
}
