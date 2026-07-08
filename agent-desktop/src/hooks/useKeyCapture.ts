import { useState, useCallback, useRef } from "react";

/**
 * 按键录制 Hook
 * 类似 CS:GO 的设置方式：点击快捷键 → 按键 → 松开后自动确认
 */
export function useKeyCapture(onCapture: (keys: string[]) => void) {
  const [listening, setListening] = useState(false);
  const [currentKeys, setCurrentKeys] = useState<string[]>([]);
  const pressedRef = useRef<Set<string>>(new Set());
  const hasMainKey = useRef(false);

  const startCapture = useCallback(() => {
    pressedRef.current.clear();
    hasMainKey.current = false;
    setCurrentKeys([]);
    setListening(true);
  }, []);

  const cancelCapture = useCallback(() => {
    pressedRef.current.clear();
    hasMainKey.current = false;
    setCurrentKeys([]);
    setListening(false);
  }, []);

  // 返回 keydown/keyup handler（挂到 document 上）
  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (!listening) return;
      e.preventDefault();
      e.stopPropagation();

      const key = e.key.toLowerCase();
      const isModifier = ["control", "meta", "shift", "alt"].includes(key);

      pressedRef.current.add(key);

      // 构建当前组合（按规范排序：ctrl → meta → shift → alt → 主按键）
      const order = ["control", "meta", "shift", "alt"];
      const parts: string[] = [];
      for (const mod of order) {
        if (pressedRef.current.has(mod)) parts.push(mod === "control" ? "ctrl" : mod);
      }
      // 主按键（非修饰键）
      if (!isModifier) {
        hasMainKey.current = true;
        parts.push(key);
      }

      // 最多 3 键
      const combo = parts.slice(0, 3);
      setCurrentKeys([...combo]);
    },
    [listening],
  );

  const handleKeyUp = useCallback(
    (e: KeyboardEvent) => {
      if (!listening) return;
      e.preventDefault();
      e.stopPropagation();

      const key = e.key.toLowerCase();

      // Escape 取消录制
      if (key === "escape") {
        cancelCapture();
        return;
      }

      pressedRef.current.delete(key);

      // 所有键都已松开 → 确认录制
      if (pressedRef.current.size === 0) {
        // 必须有非修饰键才算有效
        if (hasMainKey.current && currentKeys.length > 0) {
          onCapture([...currentKeys]);
        }
        cancelCapture();
      }
    },
    [listening, currentKeys, onCapture, cancelCapture],
  );

  return { listening, currentKeys, startCapture, cancelCapture, handleKeyDown, handleKeyUp };
}

const PLATFORM_MAP: Record<string, string> = {
  ctrl: "Ctrl",
  meta: "Cmd",
  shift: "Shift",
  alt: "Alt",
  escape: "Esc",
  arrowup: "↑",
  arrowdown: "↓",
  arrowleft: "←",
  arrowright: "→",
  " ": "Space",
  space: "Space",
  enter: "Enter",
  tab: "Tab",
  backspace: "Backspace",
  delete: "Del",
  pageup: "PgUp",
  pagedown: "PgDn",
  home: "Home",
  end: "End",
  insert: "Ins",
  capslock: "Caps",
  numlock: "NumLk",
  scrolllock: "ScrLk",
  "`": "`",
  "-": "-",
  "=": "=",
  "[": "[",
  "]": "]",
  "\\": "\\",
  ";": ";",
  "'": "'",
  ",": ",",
  ".": ".",
  "/": "/",
};

/** 用户友好的按键显示名 */
export function formatKeyName(key: string): string {
  const lower = key.toLowerCase();
  return PLATFORM_MAP[lower] || key.toUpperCase();
}

/** 组合键显示字符串，如 "Ctrl + N" */
export function formatCombo(keys: string[]): string {
  return keys.map(formatKeyName).join(" + ");
}
