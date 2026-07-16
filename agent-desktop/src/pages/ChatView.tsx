import { useState, useRef, useEffect, useCallback, forwardRef, useImperativeHandle } from "react";
import { useTranslation } from "react-i18next";
import { useAppStore } from "../stores/appStore";
import type { Message } from "../stores/appStore";
import { getErrorMessage, isAbortError } from "../lib/errors";
import { parseSSEStream } from "../lib/sse";
import MessageContent from "../components/chat/MessageContent";
import type { ThinkingStats } from "../components/chat/ThinkingBlock";
import type { ToolStep } from "../components/chat/ToolStepsBlock";
import { ModelIcon, ChevronDownIcon, CheckIcon, ArrowDownIcon, EmptyChatIcon, XIcon } from "../components/Icons";
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
  const replaceMessages = useAppStore((s) => s.replaceMessages);
  const updateLastAssistantMessage = useAppStore((s) => s.updateLastAssistantMessage);
  const updateLastAssistantThinking = useAppStore((s) => s.updateLastAssistantThinking);
  const updateLastAssistantToolSteps = useAppStore((s) => s.updateLastAssistantToolSteps);
  const setActiveProvider = useAppStore((s) => s.setActiveProvider);
  const setActiveModel = useAppStore((s) => s.setActiveModel);
  const markModelExhausted = useAppStore((s) => s.markModelExhausted);

  const activeProvider = providers.find((p) => p.id === activeProviderId);

  // Agent 集成
  const activeAgentId = useAppStore((s) => s.activeAgentId);
  const agentConfigs = useAppStore((s) => s.agentConfigs);
  const setActiveAgent = useAppStore((s) => s.setActiveAgent);
  const activeAgent = activeAgentId
    ? agentConfigs.find((a) => a.id === activeAgentId)
    : null;

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

  // === Tauri 模式（支持自动模型切换重试） ===
  const handleSendViaTauri = async (userMsg: Message) => {
    const { invoke } = await import("@tauri-apps/api/core");
    const { listen } = await import("@tauri-apps/api/event");

    // 跨 provider 跨模型的最大重试次数 = 所有可用模型数
    const totalModels = useAppStore.getState().providers.reduce(
      (sum, p) => sum + p.models.length, 0
    );
    const maxAttempts = Math.max(totalModels, 5); // 至少 5 次

    // 内部重试：发起一次流式对话，返回 true 表示需要重试（模型已耗尽）
    const doAttempt = async (): Promise<boolean> => {
      const store = useAppStore.getState();
      const provider = store.providers.find((p) => p.id === store.activeProviderId);
      if (!provider) return false;

      let fullContent = "";
      let retryNeeded = false;
      let exhaustedModelName = "";
      let nextModel = "";

      // 重置单次尝试的状态
      setToolSteps([]);
      toolStepsRef.current = [];
      setThinkingContent("");
      thinkingContentRef.current = "";
      setThinkingStats(undefined);
      setIsThinking(false);

      // 监听模型配额耗尽事件
      const unlistenQuota = await listen<{ api_base: string; model: string; error_message: string }>(
        "model-quota-exhausted",
        (event) => {
          if (cancelledRef.current) return;

          // 在 store 中找到匹配的 provider（通过 api_base 匹配）
          const currentStore = useAppStore.getState();
          const matchedProvider = currentStore.providers.find(
            (p) => p.apiBase === event.payload.api_base
          );
          if (matchedProvider) {
            const pid = matchedProvider.id;
            exhaustedModelName = event.payload.model;

            // 标记模型为已耗尽
            markModelExhausted(pid, exhaustedModelName, event.payload.error_message);

            // 查找下一个可用模型
            const next = currentStore.findNextAvailableModel();
            if (next) {
              retryNeeded = true;
              nextModel = next.model;
              // 立即切换激活的 provider/model，让本次 invoke 继续走完
              setActiveProvider(next.providerId);
              setActiveModel(next.providerId, next.model);
            }
          }
        },
      );

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
        updateLastAssistantThinking(conversationId!, full);
      });

      const unlistenThinkStop = await listen<{ tokens: number; duration_ms: number }>("thinking-stop", (event) => {
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

      const unlistenFinal = await listen("final-answer-start", () => {
        if (cancelledRef.current) return;
        setIsThinking(false);
      });

      const unlisten3 = await listen("stream-done", () => {
        const finalThinking = thinkingContentRef.current || thinkingContent;
        if (finalThinking && conversationId) {
          updateLastAssistantThinking(conversationId, finalThinking, thinkingStats);
        }
        const finalSteps = toolStepsRef.current.filter((s) => s.status !== "running");
        if (finalSteps.length > 0 && conversationId) {
          updateLastAssistantToolSteps(conversationId, finalSteps);
        }
        setIsThinking(false);
        setIsLoading(false);
      });

      type RustChatMessage = {
        role: string;
        content: string | null;
        reasoning_content?: string;
        tool_calls?: Array<{ id: string; name: string; arguments: string }>;
        tool_call_id?: string;
      };
      const unlistenMessages = await listen<{ messages: RustChatMessage[] }>("stream-messages", (event) => {
        if (!conversationId) return;
        const msgs: Message[] = event.payload.messages
          .filter((m) => m.role !== "system")
          .map((m) => ({
            id: crypto.randomUUID(),
            role: m.role as Message["role"],
            content: m.content ?? "",
            timestamp: new Date().toISOString(),
            tool_calls: m.tool_calls,
            tool_call_id: m.tool_call_id,
            thinking: m.reasoning_content,
          }));
        replaceMessages(conversationId, msgs);
      });

      // Agent 集成：注入系统提示词 + 切换模型
      let agentMessages = [...currentMessages, userMsg];
      if (activeAgent) {
        // 确保 system prompt 在第一条
        if (activeAgent.systemPrompt) {
          const hasSystemMsg = agentMessages.some((m) => m.role === "system");
          if (!hasSystemMsg) {
            agentMessages = [
              {
                id: "agent-system",
                role: "system" as const,
                content: activeAgent.systemPrompt,
                timestamp: new Date().toISOString(),
              },
              ...agentMessages,
            ];
          }
        }
        // 如果有指定的 provider/model，尝试切换
        if (activeAgent.providerId && activeAgent.model) {
          setActiveProvider(activeAgent.providerId);
          setActiveModel(activeAgent.providerId, activeAgent.model);
        } else if (activeAgent.providerId) {
          setActiveProvider(activeAgent.providerId);
        }
      }

      // 获取当前 provider（在 agent 切换之后，确保拿到正确的）
      const currentProvider = useAppStore.getState().providers.find(
        (p) => p.id === useAppStore.getState().activeProviderId
      );

      await invoke("chat_stream", {
        request: {
          api_base: currentProvider?.apiBase || provider.apiBase,
          api_key: currentProvider?.apiKey || provider.apiKey,
          model: currentProvider?.activeModel || provider.activeModel,
          messages: agentMessages.map((m) => {
            const msg: Record<string, unknown> = {
              role: m.role,
            };
            if (m.role === "tool" || !m.content) {
              msg.content = null;
            } else {
              msg.content = m.content;
            }
            if (m.tool_calls && m.tool_calls.length > 0) {
              msg.tool_calls = m.tool_calls;
            }
            if (m.tool_call_id) {
              msg.tool_call_id = m.tool_call_id;
            }
            if (m.thinking) {
              msg.reasoning_content = m.thinking;
            }
            return msg;
          }),
        },
      });

      // 清理事件监听
      unlistenQuota();
      unlisten1();
      unlisten2();
      unlisten3();
      unlistenFinal();
      unlistenTc();
      unlistenTr();
      unlistenThinkStart();
      unlistenThinkDelta();
      unlistenThinkStop();
      unlistenMessages();

      // 如果本轮检测到模型耗尽且找到了备用模型，更新提示信息并返回 true 触发重试
      if (retryNeeded && nextModel) {
        updateLastAssistantMessage(
          conversationId!,
          `⚠️ 模型「${exhaustedModelName || provider.activeModel}」配额不足或无权限，已自动切换到「${nextModel}」`,
        );
        return true;
      }
      return false;
    };

    // 重试循环：最多尝试 maxAttempts 次，每次失败自动切换到下一个可用模型
    for (let attempt = 0; attempt < maxAttempts; attempt++) {
      // 用户取消时立即停止重试
      if (cancelledRef.current) break;

      if (attempt > 0) {
        // 从第二次尝试开始，先重置 assistant 消息内容
        updateLastAssistantMessage(conversationId!, "");
      }
      const shouldRetry = await doAttempt();
      if (!shouldRetry) break;
    }
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

      {/* Agent 指示栏 */}
      {activeAgent && (
        <div className="chat-agent-bar">
          <span className="chat-agent-bar-icon">{activeAgent.icon}</span>
          <span className="chat-agent-bar-name">{activeAgent.name}</span>
          <span className="chat-agent-bar-desc">{activeAgent.description}</span>
          <button
            className="chat-agent-bar-clear"
            onClick={() => setActiveAgent(null)}
            title="清除 Agent 绑定"
          >
            <XIcon size={14} />
          </button>
        </div>
      )}

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
