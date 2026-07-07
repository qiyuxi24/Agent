import { useState, useRef, useEffect } from "react";
import { useAppStore } from "../stores/appStore";
import type { Message } from "../stores/appStore";

// 运行时检测是否在 Tauri 环境中
function isTauriEnv(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

interface ChatViewProps {
  conversationId: string | null;
}

interface SSEDelta {
  choices?: Array<{ delta?: { content?: string } }>;
}

export default function ChatView({ conversationId }: ChatViewProps) {
  const [input, setInput] = useState("");
  const [isLoading, setIsLoading] = useState(false);
  const [showModelPicker, setShowModelPicker] = useState(false);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const abortRef = useRef<AbortController | null>(null);
  const pickerRef = useRef<HTMLDivElement>(null);

  const {
    messages,
    addMessage,
    updateLastAssistantMessage,
    providers,
    activeProviderId,
    setActiveProvider,
    setActiveModel,
  } = useAppStore();

  const currentMessages: Message[] = conversationId ? (messages[conversationId] || []) : [];
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

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [currentMessages]);

  // === Tauri 模式 ===
  const handleSendViaTauri = async (conversationId: string, userMsg: Message) => {
    const { invoke } = await import("@tauri-apps/api/core");
    const { listen } = await import("@tauri-apps/api/event");

    let fullContent = "";

    const unlisten1 = await listen<{ token: string }>("stream-token", (event) => {
      fullContent += event.payload.token;
      updateLastAssistantMessage(conversationId, fullContent);
    });

    const unlisten2 = await listen<{ error: string }>("stream-error", (event) => {
      updateLastAssistantMessage(conversationId, `❌ ${event.payload.error}`);
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
  };

  // === 浏览器模式 ===
  const handleSendViaFetch = async (conversationId: string, userMsg: Message) => {
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
        throw new Error(`API 错误 (${response.status}): ${body.slice(0, 200)}`);
      }

      const reader = response.body?.getReader();
      if (!reader) throw new Error("无法读取响应流");

      const decoder = new TextDecoder();
      let buffer = "";

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });

        while (buffer.includes("\n")) {
          const idx = buffer.indexOf("\n");
          const line = buffer.slice(0, idx).trim();
          buffer = buffer.slice(idx + 1);

          if (!line || !line.startsWith("data: ")) continue;

          const data = line.slice(6);
          if (data === "[DONE]") {
            setIsLoading(false);
            return;
          }

          try {
            const parsed: SSEDelta = JSON.parse(data);
            const token = parsed.choices?.[0]?.delta?.content;
            if (token) {
              fullContent += token;
              updateLastAssistantMessage(conversationId, fullContent);
            }
          } catch {
            // 跳过
          }
        }
      }
    } catch (err: any) {
      if (err.name === "AbortError") return;
      updateLastAssistantMessage(
        conversationId,
        `❌ 请求失败：${err instanceof Error ? err.message : String(err)}`
      );
    } finally {
      setIsLoading(false);
      abortRef.current = null;
    }
  };

  const handleSend = async () => {
    const trimmed = input.trim();
    if (!trimmed || !conversationId || isLoading) return;

    if (!activeProvider?.apiKey) {
      alert("请先在设置中配置 API Key");
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

    try {
      if (isTauriEnv()) {
        await handleSendViaTauri(conversationId, userMsg);
      } else {
        await handleSendViaFetch(conversationId, userMsg);
      }
    } catch (err) {
      const errorText = `❌ 请求失败：${err instanceof Error ? err.message : String(err)}`;
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
    : "未配置模型";

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
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M12 2L2 7l10 5 10-5-10-5z" />
              <path d="M2 17l10 5 10-5" />
              <path d="M2 12l10 5 10-5" />
            </svg>
            <span className="model-label">{currentModelLabel}</span>
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <polyline points="6 9 12 15 18 9" />
            </svg>
          </button>

          {showModelPicker && (
            <div className="model-dropdown">
              {providers.length === 0 ? (
                <div className="model-dropdown-empty">
                  暂无模型配置，请前往设置添加
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
                            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
                              <polyline points="20 6 9 17 4 12" />
                            </svg>
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
            <svg width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1" opacity="0.3">
              <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
            </svg>
          </div>
          <h2>开始对话</h2>
          <p>在下方输入消息，与 AI 助手交流</p>
        </div>
      ) : (
        <div className="chat-messages">
          {currentMessages.map((msg) => (
            <div key={msg.id} className={`message message-${msg.role}`}>
              <div className="message-avatar">
                {msg.role === "user" ? "👤" : "🤖"}
              </div>
              <div className="message-content">
                <div className="message-bubble">
                  {msg.content || (isLoading && msg.role === "assistant" ? "思考中..." : "")}
                </div>
              </div>
            </div>
          ))}
          <div ref={messagesEndRef} />
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
            placeholder="输入消息... (Enter 发送，Shift+Enter 换行)"
            rows={1}
            disabled={isLoading}
          />
          <button
            className="btn btn-send"
            onClick={handleSend}
            disabled={!input.trim() || isLoading}
          >
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <line x1="22" y1="2" x2="11" y2="13" />
              <polygon points="22 2 15 22 11 13 2 9 22 2" />
            </svg>
          </button>
        </div>
      </div>
    </div>
  );
}
