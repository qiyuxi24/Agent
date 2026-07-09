import { useState, useRef, useEffect, useCallback, forwardRef, useImperativeHandle } from "react";
import { useTranslation } from "react-i18next";
import { useAppStore } from "../stores/appStore";
import type { Message } from "../stores/appStore";
import MarkdownRenderer from "../components/MarkdownRenderer";
import ErrorBoundary from "../components/ErrorBoundary";
import { getErrorMessage, isAbortError } from "../lib/errors";
import { parseSSEStream } from "../lib/sse";
import { SendIcon, StopIcon, ModelIcon, ChevronDownIcon, CheckIcon, ArrowDownIcon, EmptyChatIcon } from "../components/Icons";

// 稳定空数组引用，避免 Zustand selector 每次返回新引用导致无限重渲染
const EMPTY_MESSAGES: Message[] = [];

// 运行时检测是否在 Tauri 环境中
function isTauriEnv(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

interface ChatViewProps {
  conversationId: string | null;
}

/** 暴露给父组件调用的方法 */
export interface ChatViewHandle {
  focusInput: () => void;
  toggleModelPicker: () => void;
}

/** Markdown 渲染器 + Error Boundary：崩溃时自动降级为纯文本显示 */
function SafeMarkdown({ content }: { content: string }) {
  return (
    <ErrorBoundary fallback={<p className="md-fallback">{content}</p>}>
      <MarkdownRenderer content={content} />
    </ErrorBoundary>
  );
}

/** 工具调用步骤展示（连接 MCP 工具时显示 AI 调用的工具与结果） */
function ToolSteps({
  steps,
}: {
  steps: { name: string; args: string; status: "running" | "done"; result?: string }[];
}) {
  return (
    <div className="tool-steps">
      {steps.map((s, i) => (
        <div key={i} className={`tool-step ${s.status}`}>
          <span className="tool-step-icon">
            {s.status === "running" ? "⏳" : "✅"}
          </span>
          <span className="tool-step-name">{s.name}</span>
          {s.result && (
            <span className="tool-step-result">{s.result.slice(0, 240)}</span>
          )}
        </div>
      ))}
    </div>
  );
}

const ChatView = forwardRef<ChatViewHandle, ChatViewProps>(function ChatView({ conversationId }, ref) {
  const { t } = useTranslation();
  const [input, setInput] = useState("");
  const [isLoading, setIsLoading] = useState(false);
  const [showModelPicker, setShowModelPicker] = useState(false);
  const [showScrollToBottom, setShowScrollToBottom] = useState(false);
  const messagesContainerRef = useRef<HTMLDivElement>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const abortRef = useRef<AbortController | null>(null);
  const cancelledRef = useRef(false);
  const pickerRef = useRef<HTMLDivElement>(null);
  const shouldAutoScroll = useRef(true);

  // 工具调用步骤（MCP tool-call / tool-result 事件驱动，瞬时展示，不进 store）
  const [toolSteps, setToolSteps] = useState<{
    name: string;
    args: string;
    status: "running" | "done";
    result?: string;
  }[]>([]);

  // 暴露方法给父组件（快捷键用）
  useImperativeHandle(ref, () => ({
    focusInput: () => {
      textareaRef.current?.focus();
    },
    toggleModelPicker: () => {
      setShowModelPicker((prev) => !prev);
    },
  }));

  // === 精准 Selector 订阅 ===
  const currentMessages = useAppStore((s) =>
    conversationId ? (s.messages[conversationId] || EMPTY_MESSAGES) : EMPTY_MESSAGES,
  );
  const providers = useAppStore((s) => s.providers);
  const activeProviderId = useAppStore((s) => s.activeProviderId);

  // Actions（稳定引用）
  const addMessage = useAppStore((s) => s.addMessage);
  const updateLastAssistantMessage = useAppStore((s) => s.updateLastAssistantMessage);
  const setActiveProvider = useAppStore((s) => s.setActiveProvider);
  const setActiveModel = useAppStore((s) => s.setActiveModel);

  const activeProvider = providers.find((p) => p.id === activeProviderId);

  // 关闭下拉菜单（点击外部）
  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (pickerRef.current && !pickerRef.current.contains(e.target as Node)) {
        setShowModelPicker(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, []);

  // === 滚动检测（防抖 80ms） ===
  const scrollTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const checkAtBottom = useCallback(() => {
    const el = messagesContainerRef.current;
    if (!el) return true;
    return el.scrollHeight - el.scrollTop - el.clientHeight < 80;
  }, []);

  const handleScroll = useCallback(() => {
    if (scrollTimerRef.current) clearTimeout(scrollTimerRef.current);
    scrollTimerRef.current = setTimeout(() => {
      const atBottom = checkAtBottom();
      shouldAutoScroll.current = atBottom;
      setShowScrollToBottom(!atBottom && currentMessages.length > 0);
    }, 80);
  }, [checkAtBottom, currentMessages.length]);

  // 挂载时滚到底部
  useEffect(() => {
    const el = messagesContainerRef.current;
    if (el && currentMessages.length > 0) {
      el.scrollTop = el.scrollHeight;
    }
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // 内容变化时自动滚到底部（仅在用户未手动上滑时）
  useEffect(() => {
    if (shouldAutoScroll.current) {
      messagesEndRef.current?.scrollIntoView({ behavior: "instant" as ScrollBehavior });
    }
  }, [currentMessages]);

  // 停止流式时恢复自动滚动，隐藏按钮
  useEffect(() => {
    if (!isLoading) {
      shouldAutoScroll.current = true;
      setShowScrollToBottom(false);
    }
  }, [isLoading]);

  // 手动滚到底部
  const scrollToBottom = () => {
    const el = messagesContainerRef.current;
    if (el) {
      el.scrollTo({ top: el.scrollHeight, behavior: "smooth" });
    }
    shouldAutoScroll.current = true;
    setShowScrollToBottom(false);
  };

  // === Tauri 模式 ===
  const handleSendViaTauri = async (userMsg: Message) => {
    const { invoke } = await import("@tauri-apps/api/core");
    const { listen } = await import("@tauri-apps/api/event");

    let fullContent = "";
    setToolSteps([]);

    const unlistenTc = await listen<{ name: string; arguments: string }>(
      "tool-call",
      (event) => {
        if (cancelledRef.current) return;
        setToolSteps((prev) => [
          ...prev,
          { name: event.payload.name, args: event.payload.arguments, status: "running" },
        ]);
      },
    );

    const unlistenTr = await listen<{ name: string; result: string }>(
      "tool-result",
      (event) => {
        if (cancelledRef.current) return;
        setToolSteps((prev) =>
          prev.map((s) =>
            s.name === event.payload.name && s.status === "running"
              ? { ...s, status: "done", result: event.payload.result }
              : s,
          ),
        );
      },
    );

    const unlisten1 = await listen<{ token: string }>("stream-token", (event) => {
      if (cancelledRef.current) return;
      fullContent += event.payload.token;
      updateLastAssistantMessage(conversationId!, fullContent);
    });

    const unlisten2 = await listen<{ error: string }>("stream-error", (event) => {
      if (cancelledRef.current) return;
      updateLastAssistantMessage(conversationId!, `❌ ${event.payload.error}`);
      setIsLoading(false);
    });

    const unlisten3 = await listen("stream-done", () => {
      setIsLoading(false);
    });

    await invoke("chat_stream", {
      request: {
        api_base: activeProvider?.apiBase || "https://api.openai.com/v1",
        api_key: activeProvider?.apiKey || "",
        model: activeProvider?.activeModel || "gpt-4o",
        messages: [...currentMessages, userMsg].map((m) => ({
          role: m.role,
          content: m.content,
        })),
      },
    });

    unlisten1();
    unlisten2();
    unlisten3();
    unlistenTc();
    unlistenTr();
  };

  // === 浏览器模式 ===
  const handleSendViaFetch = async (userMsg: Message) => {
    const controller = new AbortController();
    abortRef.current = controller;

    const url = `${activeProvider?.apiBase || "https://api.openai.com/v1"}/chat/completions`;
    let fullContent = "";

    try {
      const response = await fetch(url, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Authorization: `Bearer ${activeProvider?.apiKey || ""}`,
        },
        body: JSON.stringify({
          model: activeProvider?.activeModel || "gpt-4o",
          messages: [...currentMessages, userMsg].map((m) => ({
            role: m.role,
            content: m.content,
          })),
          stream: true,
          temperature: 0.7,
          max_tokens: 4096,
        }),
        signal: controller.signal,
      });

      if (!response.ok) {
        const body = await response.text().catch(() => "");
        throw new Error(`${t("chat.error.apiError")} (${response.status}): ${body.slice(0, 200)}`);
      }

      const reader = response.body?.getReader();
      if (!reader) throw new Error(t("chat.error.cannotReadStream"));

      await parseSSEStream(reader, {
        onToken: (token) => {
          fullContent += token;
          updateLastAssistantMessage(conversationId!, fullContent);
        },
        onDone: () => {
          setIsLoading(false);
        },
        signal: controller.signal,
      });
    } catch (err: unknown) {
      if (isAbortError(err)) return;
      updateLastAssistantMessage(
        conversationId!,
        `❌ ${t("chat.error.requestFailed")}：${getErrorMessage(err)}`
      );
    } finally {
      setIsLoading(false);
      abortRef.current = null;
    }
  };

  const handleStop = async () => {
    cancelledRef.current = true;

    if (isTauriEnv()) {
      try {
        const { invoke } = await import("@tauri-apps/api/core");
        await invoke("cancel_chat");
      } catch { /* 忽略 */ }
    } else {
      abortRef.current?.abort();
      abortRef.current = null;
    }

    setIsLoading(false);
  };

  const handleSend = async () => {
    const trimmed = input.trim();
    if (!trimmed || !conversationId || isLoading) return;

    cancelledRef.current = false;

    if (!activeProvider?.apiKey) {
      alert(t("chat.needApiKey"));
      return;
    }

    const userMsg: Message = {
      id: crypto.randomUUID(),
      role: "user",
      content: trimmed,
      timestamp: new Date().toISOString(),
    };

    addMessage(conversationId, userMsg);
    setInput("");

    const assistantMsg: Message = {
      id: crypto.randomUUID(),
      role: "assistant",
      content: "",
      timestamp: new Date().toISOString(),
    };
    addMessage(conversationId, assistantMsg);

    setIsLoading(true);
    shouldAutoScroll.current = true;
    setShowScrollToBottom(false);

    try {
      if (isTauriEnv()) {
        await handleSendViaTauri(userMsg);
      } else {
        await handleSendViaFetch(userMsg);
      }
    } catch (err: unknown) {
      const errorText = `❌ ${t("chat.error.requestFailed")}：${getErrorMessage(err)}`;
      updateLastAssistantMessage(conversationId, errorText);
      setIsLoading(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  // 模型切换处理
  const handleModelSelect = (providerId: string, model: string) => {
    setActiveProvider(providerId);
    setActiveModel(providerId, model);
    setShowModelPicker(false);
  };

  // 获取当前选中模型的显示文本
  const currentModelLabel = activeProvider
    ? `${activeProvider.name} / ${activeProvider.activeModel}`
    : t("chat.noModel");

  return (
    <div className="chat-view">
      {/* 模型选择栏 */}
      <div className="chat-toolbar">
        <div className="model-picker" ref={pickerRef}>
          <button
            className="model-picker-btn"
            onClick={() => setShowModelPicker(!showModelPicker)}
            disabled={providers.length === 0}
          >
            <ModelIcon size={14} />
            <span className="model-label">{currentModelLabel}</span>
            <ChevronDownIcon size={12} />
          </button>

          {showModelPicker && (
            <div className="model-dropdown">
              {providers.length === 0 ? (
                <div className="model-dropdown-empty">
                  {t("chat.noModelHint")}
                </div>
              ) : (
                providers.map((provider) => (
                  <div key={provider.id} className="model-provider-group">
                    <div className="model-provider-name">{provider.name}</div>
                    {provider.models.map((model) => (
                      <button
                        key={model}
                        className={`model-option ${
                          provider.id === activeProviderId &&
                          model === provider.activeModel
                            ? "active"
                            : ""
                        }`}
                        onClick={() => handleModelSelect(provider.id, model)}
                      >
                        <span className="model-option-name">{model}</span>
                        {provider.id === activeProviderId &&
                          model === provider.activeModel && (
                            <CheckIcon size={14} />
                          )}
                      </button>
                    ))}
                  </div>
                ))
              )}
            </div>
          )}
        </div>
      </div>

      {/* 消息区域 */}
      {currentMessages.length === 0 ? (
        <div className="chat-empty">
          <div className="empty-icon">
            <EmptyChatIcon size={64} />
          </div>
          <h2>{t("chat.empty.title")}</h2>
          <p>{t("chat.empty.subtitle")}</p>
        </div>
      ) : (
        <div className="chat-messages-wrapper">
          <div
            className="chat-messages"
            ref={messagesContainerRef}
            onScroll={handleScroll}
          >
            {currentMessages.map((msg) => (
              <div key={msg.id} className={`message message-${msg.role}`}>
                <div className="message-avatar">
                  {msg.role === "user" ? "👤" : "🤖"}
                </div>
                <div className="message-content">
                  <div className={`message-bubble ${msg.role === "assistant" ? "message-bubble-markdown" : ""}`}>
                    {msg.role === "assistant" ? (
                      msg.content ? (
                        <SafeMarkdown content={msg.content} />
                      ) : isLoading ? (
                        <span className="thinking-text">{t("chat.thinking")}</span>
                      ) : null
                    ) : (
                      msg.content
                    )}
                  </div>
                </div>
              </div>
            ))}
            {toolSteps.length > 0 && (
              <div className="message message-tool-steps">
                <ToolSteps steps={toolSteps} />
              </div>
            )}
            <div ref={messagesEndRef} />
          </div>

          {/* 滚动到底部按钮 */}
          <button
            className={`scroll-to-bottom-btn ${showScrollToBottom || isLoading ? "visible" : ""} ${isLoading ? "loading" : ""}`}
            onClick={scrollToBottom}
            title={t("chat.scrollToBottom")}
          >
            <ArrowDownIcon size={18} className="scroll-icon" />
          </button>
        </div>
      )}

      {/* 输入区域 */}
      <div className="chat-input-area">
        <div className="chat-input-container">
          <textarea
            ref={textareaRef}
            className="chat-input"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={t("chat.input.placeholder")}
            rows={1}
            disabled={isLoading}
          />
          {isLoading ? (
            <button
              className="btn btn-stop"
              onClick={handleStop}
              title={t("chat.stop")}
            >
              <StopIcon size={16} />
            </button>
          ) : (
            <button
              className="btn btn-send"
              onClick={handleSend}
              disabled={!input.trim()}
            >
              <SendIcon size={20} />
            </button>
          )}
        </div>
      </div>
    </div>
  );
});

export default ChatView;
