import { useState, useRef, useEffect, useCallback, forwardRef, useImperativeHandle } from "react";
import { useTranslation } from "react-i18next";
import { useAppStore } from "../stores/appStore";
import type { Message } from "../stores/appStore";
import { getErrorMessage, isAbortError } from "../lib/errors";
import { parseSSEStream } from "../lib/sse";
import MessageContent from "../components/chat/MessageContent";
import type { ThinkingStats } from "../components/chat/ThinkingBlock";
import type { ToolStep } from "../components/chat/ToolStepsBlock";
import { ModelIcon, ChevronDownIcon, CheckIcon, ArrowDownIcon, EmptyChatIcon } from "../components/Icons";
import ChatInput from "../components/chat/ChatInput";
import type { ChatInputHandle } from "../components/chat/ChatInput";

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

const ChatView = forwardRef<ChatViewHandle, ChatViewProps>(function ChatView({ conversationId }, ref) {
  const { t } = useTranslation();
  const [isLoading, setIsLoading] = useState(false);
  const [showModelPicker, setShowModelPicker] = useState(false);
  const [showScrollToBottom, setShowScrollToBottom] = useState(false);
  const chatInputRef = useRef<ChatInputHandle>(null);
  const messagesContainerRef = useRef<HTMLDivElement>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const abortRef = useRef<AbortController | null>(null);
  const cancelledRef = useRef(false);
  const pickerRef = useRef<HTMLDivElement>(null);
  const shouldAutoScroll = useRef(true);

  // 工具调用步骤（MCP tool-call / tool-result 事件驱动，瞬时展示，不进 store）
  const [toolSteps, setToolSteps] = useState<ToolStep[]>([]);
  // 用 ref 跟踪最新步骤，避免 stream-done 中的闭包过期问题
  const toolStepsRef = useRef<ToolStep[]>([]);

  // 思考状态（thinking-start/delta/stop 事件驱动）
  const [thinkingContent, setThinkingContent] = useState("");
  const [thinkingStats, setThinkingStats] = useState<ThinkingStats | undefined>();
  const [isThinking, setIsThinking] = useState(false);
  // 用 ref 累积思考内容，避免 React state 闭包问题
  const thinkingContentRef = useRef("");

  // 暴露方法给父组件（快捷键用）
  useImperativeHandle(ref, () => ({
    focusInput: () => {
      chatInputRef.current?.focus();
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
  const updateLastAssistantThinking = useAppStore((s) => s.updateLastAssistantThinking);
  const updateLastAssistantToolSteps = useAppStore((s) => s.updateLastAssistantToolSteps);
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

  // 流式输出期间：监听消息容器尺寸变化，保持滚底
  // （Markdown 渲染、代码块展开等会导致内容高度变化，message 数组不变）
  useEffect(() => {
    if (!isLoading || !messagesContainerRef.current) return;
    const el = messagesContainerRef.current;
    const observer = new ResizeObserver(() => {
      if (shouldAutoScroll.current) {
        el.scrollTop = el.scrollHeight;
      }
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, [isLoading]);

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
    toolStepsRef.current = [];
    setThinkingContent("");
    thinkingContentRef.current = "";
    setThinkingStats(undefined);
    setIsThinking(false);

    // 思考事件监听
    const unlistenThinkStart = await listen("thinking-start", () => {
      if (cancelledRef.current) return;
      setIsThinking(true);
      setThinkingContent("");
      thinkingContentRef.current = "";
      setThinkingStats(undefined);
    });

    const unlistenThinkDelta = await listen<{ delta: string }>("thinking-delta", (event) => {
      if (cancelledRef.current) return;
      thinkingContentRef.current += event.payload.delta;
      const full = thinkingContentRef.current;
      setThinkingContent(full);
      // 传递完整累积内容，而非单个 delta（避免 store 覆盖问题）
      updateLastAssistantThinking(conversationId!, full);
    });

    const unlistenThinkStop = await listen<{ tokens: number; duration_ms: number }>("thinking-stop", (event) => {
      // 思考结束时立即标记，让 ThinkingBlock 切换到 "done" 状态
      // （此时可能进入 tool-call 阶段或直接输出最终答案）
      setIsThinking(false);
      setThinkingStats({
        tokens: event.payload.tokens,
        durationMs: event.payload.duration_ms,
      });
    });

    const unlistenTc = await listen<{ name: string; arguments: string }>(
      "tool-call",
      (event) => {
        if (cancelledRef.current) return;
        const next = [
          ...toolStepsRef.current,
          { name: event.payload.name, args: event.payload.arguments, status: "running" as const },
        ];
        toolStepsRef.current = next;
        setToolSteps(next);
      },
    );

    const unlistenTr = await listen<{
      name: string;
      result: string;
      isError: boolean;
      error_code?: string | null;
      error_category?: string | null;
    }>("tool-result", (event) => {
      if (cancelledRef.current) return;
      const { name, result, isError, error_code, error_category } = event.payload;
      const next = toolStepsRef.current.map((s) =>
        s.name === name && s.status === "running"
          ? {
              ...s,
              status: (isError ? "error" : "done") as ToolStep["status"],
              result,
              errorCode: error_code || null,
              errorCategory: error_category || null,
            }
          : s,
      );
      toolStepsRef.current = next;
      setToolSteps(next);
    });

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

    // agent 多轮结束、进入面向用户的最终答案阶段
    const unlistenFinal = await listen("final-answer-start", () => {
      if (cancelledRef.current) return;
      // thinking-stop 已处理 isThinking=false，这里确保工具步骤区收起
      setIsThinking(false);
    });

    const unlisten3 = await listen("stream-done", () => {
      const finalThinking = thinkingContentRef.current || thinkingContent;
      if (finalThinking && conversationId) {
        // 写入最终 thinking 内容到 message（含 stats）
        updateLastAssistantThinking(conversationId, finalThinking, thinkingStats);
      }
      // 持久化工具调用步骤（过滤掉 running 状态，只保留 done/error）
      const finalSteps = toolStepsRef.current.filter((s) => s.status !== "running");
      if (finalSteps.length > 0 && conversationId) {
        updateLastAssistantToolSteps(conversationId, finalSteps);
      }
      setIsThinking(false);
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
    unlistenFinal();
    unlistenTc();
    unlistenTr();
    unlistenThinkStart();
    unlistenThinkDelta();
    unlistenThinkStop();
  };

  // === 浏览器模式 ===
  const handleSendViaFetch = async (userMsg: Message) => {
    const controller = new AbortController();
    abortRef.current = controller;

    const url = `${activeProvider?.apiBase || "https://api.openai.com/v1"}/chat/completions`;
    let fullContent = "";
    let fullThinking = "";
    const thinkingStartTime = Date.now();

    setThinkingContent("");
    thinkingContentRef.current = "";
    setThinkingStats(undefined);
    setIsThinking(false);

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
        onThinking: (token) => {
          if (!isThinking) {
            setIsThinking(true);
          }
          fullThinking += token;
          thinkingContentRef.current = fullThinking;
          setThinkingContent(fullThinking);
          // 传递完整累积内容
          updateLastAssistantThinking(conversationId!, fullThinking);
        },
        onToken: (token) => {
          if (isThinking) {
            // 思考阶段结束，保存 stats
            setThinkingStats({
              tokens: Math.max(1, Math.ceil(fullThinking.length / 4)),
              durationMs: Date.now() - thinkingStartTime,
            });
            setIsThinking(false);
          }
          fullContent += token;
          updateLastAssistantMessage(conversationId!, fullContent);
        },
        onDone: () => {
          if (fullThinking && conversationId) {
            updateLastAssistantThinking(conversationId, fullThinking, {
              tokens: Math.max(1, Math.ceil(fullThinking.length / 4)),
              durationMs: Date.now() - thinkingStartTime,
            });
          }
          setIsThinking(false);
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

  const handleSend = async (content: string) => {
    const trimmed = content.trim();
    if (!trimmed || !conversationId) return;

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
            {currentMessages.map((msg, idx) => {
              const isLast = msg === currentMessages[currentMessages.length - 1];
              return (
                <div key={msg.id} className={`message message-${msg.role}`}>
                  <div className="message-avatar">
                    {msg.role === "user" ? "👤" : "🤖"}
                  </div>
                  <MessageContent
                    role={msg.role}
                    content={msg.content}
                    storedThinking={msg.thinking}
                    thinkingStats={msg.thinkingStats}
                    isLoading={isLoading}
                    isLastMessage={isLast}
                    streamingThinking={thinkingContent}
                    isThinking={isThinking}
                    toolSteps={toolSteps.length > 0 ? toolSteps : undefined}
                    storedToolSteps={msg.toolSteps}
                  />
                </div>
              );
            })}
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
      <ChatInput
        ref={chatInputRef}
        isLoading={isLoading}
        onSend={handleSend}
        onStop={handleStop}
        placeholder={t("chat.input.placeholder")}
      />
    </div>
  );
});

export default ChatView;
