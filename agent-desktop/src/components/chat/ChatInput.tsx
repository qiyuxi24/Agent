import { useState, useRef, useEffect, useCallback, forwardRef, useImperativeHandle } from "react";
import { useTranslation } from "react-i18next";
import { SendIcon, StopIcon } from "../Icons";

export interface ChatInputProps {
  /** 是否正在等待 LLM 响应（不阻塞输入，仅切换按钮 + 启用排队） */
  isLoading: boolean;
  /** 用户点击发送或按 Enter 时触发 */
  onSend: (content: string) => void;
  /** 用户点击停止时触发（同时清空排队消息） */
  onStop: () => void;
  /** placeholder 文本（支持 i18n） */
  placeholder?: string;
  /** 是否禁用输入框（如无 provider） */
  disabled?: boolean;
}

export interface ChatInputHandle {
  focus: () => void;
}

const HINT_KEY = "votek.input-hint-shown";

const ChatInput = forwardRef<ChatInputHandle, ChatInputProps>(function ChatInput(
  { isLoading, onSend, onStop, placeholder, disabled = false },
  ref,
) {
  const { t } = useTranslation();
  const [value, setValue] = useState("");
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // --- 消息队列：思考中也可以输入，排队依次发送 ---
  const queueRef = useRef<string[]>([]);
  const [queueSize, setQueueSize] = useState(0);
  const prevLoadingRef = useRef(isLoading);
  const onSendRef = useRef(onSend);
  onSendRef.current = onSend;

  // isLoading 从 true → false 时，出队并发送下一条
  useEffect(() => {
    const wasLoading = prevLoadingRef.current;
    prevLoadingRef.current = isLoading;

    if (wasLoading && !isLoading) {
      const next = queueRef.current.shift();
      if (next !== undefined) {
        setQueueSize(queueRef.current.length);
        setTimeout(() => onSendRef.current(next), 0);
      }
    }
  }, [isLoading]);

  // --- 一次性快捷键提示 ---
  const [showHint, setShowHint] = useState(() => {
    return localStorage.getItem(HINT_KEY) !== "1";
  });

  const dismissHint = useCallback(() => {
    if (showHint) {
      setShowHint(false);
      localStorage.setItem(HINT_KEY, "1");
    }
  }, [showHint]);

  // 暴露聚焦方法
  useImperativeHandle(ref, () => ({
    focus: () => textareaRef.current?.focus(),
  }));

  // textarea 自动扩展高度
  useEffect(() => {
    const ta = textareaRef.current;
    if (!ta) return;
    ta.style.height = "auto";
    ta.style.height = Math.min(ta.scrollHeight, 200) + "px";
  }, [value]);

  // 发送完成后重置高度
  useEffect(() => {
    if (!isLoading && value === "") {
      const ta = textareaRef.current;
      if (ta) ta.style.height = "auto";
    }
  }, [isLoading, value]);

  const send = useCallback(() => {
    const trimmed = value.trim();
    if (!trimmed || disabled) return;

    dismissHint();

    if (isLoading) {
      // 模型正在思考 → 加入队列
      queueRef.current.push(trimmed);
      setQueueSize(queueRef.current.length);
    } else {
      onSend(trimmed);
    }
    setValue("");
  }, [value, isLoading, disabled, onSend, dismissHint]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        send();
      }
    },
    [send],
  );

  // 停止时清空排队，避免残留消息被意外发送
  const handleStop = useCallback(() => {
    queueRef.current = [];
    setQueueSize(0);
    onStop();
  }, [onStop]);

  return (
    <div className="chat-input-area">
      <div className="chat-input-container">
        {showHint && (
          <div className="chat-input-once-hint">
            {t("chat.input.hint")}
            <button className="chat-input-once-hint-close" onClick={dismissHint}>&times;</button>
          </div>
        )}
        <textarea
          ref={textareaRef}
          className="chat-input"
          value={value}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder={placeholder ?? t("chat.input.placeholder")}
          rows={1}
          disabled={disabled}
        />
        <div className="chat-input-footer">
          {queueSize > 0 ? (
            <span className="chat-input-queue-hint">
              {t("chat.input.queued", { count: queueSize })}
            </span>
          ) : (
            <span />
          )}
          <div className="chat-input-actions">
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
                className={`btn btn-send ${value.trim() ? "has-content" : ""}`}
                onClick={send}
                disabled={!value.trim()}
                title={t("chat.send")}
              >
                <SendIcon size={18} />
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
});

export default ChatInput;
