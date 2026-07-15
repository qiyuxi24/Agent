import { useState, useRef, useEffect, useCallback, forwardRef, useImperativeHandle } from "react";
import { useTranslation } from "react-i18next";
import { SendIcon, StopIcon } from "../Icons";

export interface ChatInputProps {
  /** 是否正在等待 LLM 响应（禁用输入 + 显示停止按钮） */
  isLoading: boolean;
  /** 用户点击发送或按 Enter 时触发 */
  onSend: (content: string) => void;
  /** 用户点击停止时触发 */
  onStop: () => void;
  /** placeholder 文本（支持 i18n） */
  placeholder?: string;
  /** 是否禁用输入框（额外控制，如无 provider） */
  disabled?: boolean;
}

/** 暴露给父组件的命令式方法（快捷键聚焦等） */
export interface ChatInputHandle {
  focus: () => void;
}

const ChatInput = forwardRef<ChatInputHandle, ChatInputProps>(function ChatInput(
  { isLoading, onSend, onStop, placeholder, disabled = false },
  ref,
) {
  const { t } = useTranslation();
  const [value, setValue] = useState("");
  const textareaRef = useRef<HTMLTextAreaElement>(null);

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
    if (!trimmed || isLoading || disabled) return;
    onSend(trimmed);
    setValue("");
  }, [value, isLoading, disabled, onSend]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        send();
      }
    },
    [send],
  );

  const isDisabled = isLoading || disabled;

  return (
    <div className="chat-input-area">
      <div className="chat-input-container">
        <textarea
          ref={textareaRef}
          className="chat-input"
          value={value}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder={placeholder ?? t("chat.input.placeholder")}
          rows={1}
          disabled={isDisabled}
        />
        <div className="chat-input-footer">
          <span className="chat-input-hint">Enter 发送 · Shift+Enter 换行</span>
          <div className="chat-input-actions">
            {isLoading ? (
              <button
                className="btn btn-stop"
                onClick={onStop}
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
