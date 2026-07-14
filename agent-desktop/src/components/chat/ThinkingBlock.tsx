/**
 * ThinkingBlock — 深度思考展示组件
 *
 * 设计参考：assistant-ui Reasoning / MUI X Reasoning / DeepSeek Web
 *
 * 特性：
 * - Markdown 渲染（非纯文本），支持代码高亮、列表、表格等
 * - 流式时自动展开 + 底部渐变遮罩（预览模式）
 * - 完成时自动折叠（尊重用户手动操作）
 * - ResizeObserver 自动滚底（流式时跟踪最新内容）
 * - Shimmer 闪光动画（流式时标题栏）
 * - 统计信息友好展示（"已深度思考 · 用时 3.2s"）
 */
import { useState, useRef, useEffect, useCallback } from "react";
import MarkdownRenderer from "../MarkdownRenderer";
import ErrorBoundary from "../ErrorBoundary";

export interface ThinkingStats {
  tokens: number;
  durationMs: number;
}

interface ThinkingBlockProps {
  thinking: string;
  stats?: ThinkingStats;
  /** 是否正在流式输出思考内容 */
  streaming?: boolean;
}

/** 安全的 Markdown 渲染：崩溃时降级为纯文本 */
function SafeMarkdown({ content }: { content: string }) {
  return (
    <ErrorBoundary fallback={<pre className="thinking-fallback-text">{content}</pre>}>
      <MarkdownRenderer content={content} />
    </ErrorBoundary>
  );
}

export default function ThinkingBlock({ thinking, stats, streaming }: ThinkingBlockProps) {
  // --- 状态 ---
  const [collapsed, setCollapsed] = useState(false);
  // userToggled：用户手动点过一次后，不再自动折叠/展开
  const [userToggled, setUserToggled] = useState(false);

  // --- Refs ---
  const contentRef = useRef<HTMLDivElement>(null);
  const prevStreamingRef = useRef(streaming);
  const prevContentLen = useRef(thinking.length);

  // --- 流式状态变化时的自动行为 ---
  useEffect(() => {
    // 流式开始时：自动展开
    if (streaming && !prevStreamingRef.current && !userToggled) {
      setCollapsed(false);
    }
    // 流式结束时：自动折叠
    if (!streaming && prevStreamingRef.current && !userToggled) {
      setCollapsed(true);
    }
    prevStreamingRef.current = streaming;
  }, [streaming, userToggled]);

  // --- 流式时自动滚底（ResizeObserver） ---
  useEffect(() => {
    if (!streaming || !contentRef.current) return;

    const el = contentRef.current;
    const observer = new ResizeObserver(() => {
      el.scrollTop = el.scrollHeight;
    });
    observer.observe(el);

    return () => observer.disconnect();
  }, [streaming]);

  // 内容变化时也滚底（增量更新）
  useEffect(() => {
    if (streaming && contentRef.current && thinking.length > prevContentLen.current) {
      contentRef.current.scrollTop = contentRef.current.scrollHeight;
    }
    prevContentLen.current = thinking.length;
  }, [thinking, streaming]);

  // --- 用户点击折叠/展开 ---
  const handleToggle = useCallback(() => {
    setCollapsed((prev) => {
      const next = !prev;
      setUserToggled(true);
      return next;
    });
  }, []);

  // --- 空内容且非流式时不渲染 ---
  if (!thinking && !streaming) return null;

  // --- 状态文本 ---
  const durationStr = stats ? `${(stats.durationMs / 1000).toFixed(1)}s` : "";
  const tokensStr = stats ? `${stats.tokens} tokens` : "";

  let statusLabel: string;
  let statusDetail: string;

  if (streaming) {
    statusLabel = "正在深度思考...";
    statusDetail = "";
  } else if (stats) {
    statusLabel = "已深度思考";
    statusDetail = [tokensStr, durationStr ? `用时 ${durationStr}` : ""]
      .filter(Boolean)
      .join(" · ");
  } else {
    statusLabel = "深度思考";
    statusDetail = "";
  }

  const isPreview = streaming && !collapsed;

  return (
    <div className={`thinking-block ${streaming ? "thinking-streaming" : ""} ${isPreview ? "thinking-preview" : ""}`}>
      {/* Header：点击折叠/展开 */}
      <button
        type="button"
        className={`thinking-header ${streaming ? "thinking-header-shimmer" : ""}`}
        onClick={handleToggle}
        aria-expanded={!collapsed}
        aria-busy={streaming || undefined}
      >
        <span className="thinking-icon">🧠</span>
        <span className="thinking-label">{statusLabel}</span>
        {statusDetail && <span className="thinking-stats">{statusDetail}</span>}
        <span className={`thinking-chevron ${collapsed ? "collapsed" : ""}`}>▾</span>
      </button>

      {/* Content：折叠内容（带动画） */}
      <div
        className={`thinking-content-wrapper ${collapsed ? "thinking-content-collapsed" : ""}`}
      >
        <div className="thinking-content" ref={contentRef}>
          {thinking ? (
            <div className="thinking-markdown">
              <SafeMarkdown content={thinking} />
            </div>
          ) : streaming ? (
            <span className="thinking-loading">⏳ 思考中...</span>
          ) : null}

          {/* 流式预览模式：底部渐变遮罩 */}
          {isPreview && <div className="thinking-fade-bottom" />}
        </div>
      </div>
    </div>
  );
}
