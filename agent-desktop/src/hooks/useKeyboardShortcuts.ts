import { useEffect } from "react";
import { useAppStore, type ShortcutAction } from "../stores/appStore";

interface ShortcutHandlers {
  onNewConversation: () => void;
  onFocusInput: () => void;
  onToggleModelPicker: () => void;
  onOpenSettings: () => void;
}

/** action → handler 映射 */
const ACTION_HANDLER: Record<ShortcutAction, keyof ShortcutHandlers> = {
  newConversation: "onNewConversation",
  focusInput: "onFocusInput",
  toggleModelPicker: "onToggleModelPicker",
  openSettings: "onOpenSettings",
};

/** 从 KeyboardEvent 构建规范化的按键组合（ctrl → meta → shift → alt → 主键） */
function eventToCombo(e: KeyboardEvent): string {
  const parts: string[] = [];
  if (e.ctrlKey) parts.push("ctrl");
  if (e.metaKey) parts.push("meta");
  if (e.shiftKey) parts.push("shift");
  if (e.altKey) parts.push("alt");

  const key = e.key.toLowerCase();
  // 排除纯修饰键按下（只有 Control/Shift/Alt/Meta 而没有其他键时不触发）
  if (!["control", "meta", "shift", "alt"].includes(key)) {
    parts.push(key);
  }

  return parts.join("+");
}

/**
 * 全局键盘快捷键 hook
 * 从 store 读取用户自定义的快捷键绑定，动态匹配
 */
export function useKeyboardShortcuts(handlers: ShortcutHandlers) {
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const combo = eventToCombo(e);
      // combo 为空 = 只按了修饰键，忽略
      if (!combo) return;

      const { shortcuts } = useAppStore.getState();

      // 遍历所有快捷键绑定，匹配当前组合
      for (const [action, binding] of Object.entries(shortcuts) as [ShortcutAction, { keys: string[] }][]) {
        const storedCombo = binding.keys.join("+");
        if (storedCombo === combo) {
          e.preventDefault();
          const handlerKey = ACTION_HANDLER[action];
          handlers[handlerKey]?.();
          return;
        }
      }
    };

    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [handlers]);
}
