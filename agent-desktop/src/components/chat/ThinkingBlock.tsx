/**
 * ThinkingBlock — 深度思考展示组件
 *
 * 设计参考：Vercel AI Elements Reasoning 组件
 * https://elements.ai-sdk.dev/components/reasoning
 *
 * 行为：
 * - 流式时自动展开，结束后 1 秒自动折叠（除非用户手动操作过）
 * - 标题栏显示 duration（秒）+ shimmer 动画
 * - 内容区 Markdown 渲染 + 流式自动滚底
 * - 完成态折叠后用户可点击重新展开
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

const AUTO_CLOSE_DELAY = 1000; // ms，与 Vercel AI Elements 一致

/** 安全的 Markdown 渲染：崩溃时降级为纯文本 */
function SafeMarkdown({ content }: { content: string }) {
  return (
    <ErrorBoundary fallback={<pre className="thinking-fallback-text">{content}</pre>}>
      <MarkdownRenderer content={content} />
    </ErrorBoundary>
  );
}

export default function ThinkingBlock({ thinking, stats, streaming }: ThinkingBlockProps) {
  const [open, setOpen] = useState(false);
  const [userToggled, setUserToggled] = useState(false);
  const contentRef = useRef<HTMLDivElement>(null);
  const prevStreamingRef = useRef(streaming);
  const autoCloseRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // --- 自动打开/关闭逻辑 ---
  useEffect(() => {
    // 清理之前的定时器
    if (autoCloseRef.current) {
      clearTimeout(autoCloseRef.current);
      autoCloseRef.current = null;
    }

    // 流式开始时：自动展开
    if (streaming && !prevStreamingRef.current) {
      if (!userToggled) setOpen(true);
    }

    // 流式结束时：延时后自动折叠
    if (!streaming && prevStreamingRef.current) {
      if (!userToggled) {
        autoCloseRef.current = setTimeout(() => {
          setOpen(false);
        }, AUTO_CLOSE_DELAY);
      }
    }

    prevStreamingRef.current = streaming;
    return () => {
      if (autoCloseRef.current) clearTimeout(autoCloseRef.current);
    };
  }, [streaming, userToggled]);

  // --- 流式时自动滚底 ---
  useEffect(() => {
    if (!streaming || !contentRef.current) return;
    const el = contentRef.current;
    const observer = new ResizeObserver(() => {
      el.scrollTop = el.scrollHeight;
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, [streaming]);

  // --- 用户点击切换 ---
  const handleToggle = useCallback(() => {
    setUserToggled(true);
    setOpen((prev) => !prev);
    // 取消自动关闭定时器
    if (autoCloseRef.current) {
      clearTimeout(autoCloseRef.current);
      autoCloseRef.current = null;
    }
  }, []);

  // --- 空内容且非流式时不渲染 ---
  if (!thinking && !streaming) return null;

  // --- 状态文本 ---
  const durationSec = stats ? Math.max(1, Math.ceil(stats.durationMs / 1000)) : undefined;

  return (
    <div className={`thinking-block ${streaming ? "thinking-streaming" : ""}`}>
      {/* Trigger：点击切换 */}
      <button
        type="button"
        className={`thinking-trigger ${streaming ? "thinking-trigger-active" : ""}`}
        onClick={handleToggle}
        aria-expanded={open}
      >
        {/* 左侧：状态 */}
        <span className="thinking-trigger-left">
          {streaming ? (
            <span className="thinking-label thinking-label-shimmer">思考中...</span>
          ) : (
            <span className="thinking-label">
              已深度思考{durationSec ? ` · ${durationSec}s` : ""}
            </span>
          )}
        </span>

        {/* 右侧：chevron */}
        <span className={`thinking-chevron ${open ? "" : "collapsed"}`}>
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <polyline points="6 9 12 15 18 9" />
          </svg>
        </span>
      </button>

      {/* Content：可折叠，grid 动画 */}
      <div className={`thinking-content-wrapper ${open ? "" : "thinking-content-collapsed"}`}>
        <div className="thinking-content-inner">
          <div className="thinking-content-scroll" ref={contentRef}>
            {thinking ? (
              <div className="thinking-markdown">
                <SafeMarkdown content={thinking} />
              </div>
            ) : streaming ? (
              <span className="thinking-loading">⏳ 思考中...</span>
            ) : null}
          </div>
        </div>
      </div>
    </div>
  );
}
