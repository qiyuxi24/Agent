/**
 * 自定义窗口标题栏（TitleBar）
 *
 * 支持：窗口拖拽、最小化、最大化/还原、关闭。
 * 使用 useWindowManager Hook 统一管理窗口操作，非 Tauri 环境不显示控制按钮。
 */

import { useWindowManager } from "../hooks/useWindowManager";
import { MinusIcon, MaximizeIcon, CloseIcon } from "./Icons";

export default function TitleBar() {
  const { isTauri, isMaximized, minimize, toggleMaximize, close } =
    useWindowManager();

  return (
    <div className="titlebar" data-tauri-drag-region>
      <span className="titlebar-title">Votek</span>
      {isTauri && (
        <div className="titlebar-controls">
          <button
            className="titlebar-btn"
            onClick={minimize}
            title="最小化"
          >
            <MinusIcon size={14} />
          </button>
          <button
            className="titlebar-btn"
            onClick={toggleMaximize}
            title={isMaximized ? "还原" : "最大化"}
          >
            <MaximizeIcon size={12} />
          </button>
          <button
            className="titlebar-btn titlebar-btn-close"
            onClick={close}
            title="关闭"
          >
            <CloseIcon size={16} />
          </button>
        </div>
      )}
    </div>
  );
}
