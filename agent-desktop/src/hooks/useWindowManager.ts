/**
 * 统一窗口管理 Hook
 *
 * 直接使用 Tauri JS API（@tauri-apps/api/window），不经过 Rust 后端。
 * 非 Tauri 环境所有方法静默无操作，浏览器 dev 不崩溃。
 * 只管理窗口本身的操作，不含分辨率/尺寸逻辑。
 *
 * 设计原则：
 * - 窗口引用初始化时一次缓存，避免每次操作重复动态 import
 * - isMaximized/isAlwaysOnTop 状态只作初始渲染用，不随系统行为实时同步
 *   （用户按 Win+↑ 等 OS 快捷键改变状态后，下次点击 toggle 按钮时自动读取真实值）
 */

import { useCallback, useEffect, useState, useRef } from "react";

/** 检测是否在 Tauri 环境 */
function detectTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** 延迟获取当前窗口引用（避免非 Tauri 环境构建报错） */
async function getWin() {
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    return getCurrentWindow();
  } catch {
    return null;
  }
}

/**
 * 窗口管理 Hook
 *
 * 返回值：
 * - isTauri       — 是否在 Tauri 中（决定 UI 是否渲染控制按钮）
 * - isMaximized   — 是否最大化（用于切换按钮图标）
 * - isAlwaysOnTop — 是否置顶
 * - minimize / toggleMaximize / close / toggleAlwaysOnTop
 */
export function useWindowManager() {
  const isTauri = detectTauri();
  const [isMaximized, setIsMaximized] = useState(false);
  const [isAlwaysOnTop, setIsAlwaysOnTop] = useState(false);
  const winRef = useRef<any>(null);

  // 初始化时缓存窗口引用，后续操作直接读 ref（零成本）
  useEffect(() => {
    if (!isTauri) return;
    let cancelled = false;
    (async () => {
      const win = await getWin();
      if (!win || cancelled) return;
      winRef.current = win;
      setIsMaximized(await win.isMaximized());
      setIsAlwaysOnTop(await win.isAlwaysOnTop());
    })();
    return () => { cancelled = true; };
  }, [isTauri]);

  // ── 操作方法 ──

  const minimize = useCallback(async () => {
    await winRef.current?.minimize();
  }, []);

  const toggleMaximize = useCallback(async () => {
    const win = winRef.current;
    if (!win) return;
    await win.toggleMaximize();
    setIsMaximized(await win.isMaximized());
  }, []);

  const close = useCallback(async () => {
    await winRef.current?.close();
  }, []);

  const toggleAlwaysOnTop = useCallback(async () => {
    const win = winRef.current;
    if (!win) return;
    const next = !(await win.isAlwaysOnTop());
    await win.setAlwaysOnTop(next);
    setIsAlwaysOnTop(next);
  }, []);

  return {
    isTauri,
    isMaximized,
    isAlwaysOnTop,
    minimize,
    toggleMaximize,
    close,
    toggleAlwaysOnTop,
  };
}
