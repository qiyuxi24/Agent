import { useState, useRef, useEffect } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import { useAppStore } from "../stores/appStore";
import {
  PlusIcon, ChatIcon, SettingsIcon, DotsIcon,
  PinIcon, PinFilledIcon, DeleteIcon,
  PanelLeftCloseIcon, GlobeIcon, CodeIcon,
  WorkspaceIcon,
} from "./Icons";

interface SidebarProps {
  currentPage: "chat" | "settings" | "browser" | "ide" | "workspace";
  onNavigate: (page: "chat" | "settings" | "browser" | "ide" | "workspace") => void;
  collapsed: boolean;
  onToggleCollapse: () => void;
}

export default function Sidebar({ currentPage, onNavigate, collapsed, onToggleCollapse }: SidebarProps) {
  const { t } = useTranslation();

  // === 精准订阅 ===
  const conversations = useAppStore((s) => s.conversations);
  const activeConversationId = useAppStore((s) => s.activeConversationId);
  const canCreateNew = useAppStore((s) => (s.messages[s.activeConversationId || ""] || []).length > 0);

  // Actions（稳定引用）
  const setActiveConversation = useAppStore((s) => s.setActiveConversation);
  const createConversation = useAppStore((s) => s.createConversation);
  const deleteConversation = useAppStore((s) => s.deleteConversation);
  const togglePinConversation = useAppStore((s) => s.togglePinConversation);

  const [menuOpenId, setMenuOpenId] = useState<string | null>(null);
  const [menuPos, setMenuPos] = useState<{ left: number; top: number } | null>(null);
  const sidebarRef = useRef<HTMLElement>(null);
  const convListRef = useRef<HTMLDivElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  const closeMenu = () => {
    setMenuOpenId(null);
    setMenuPos(null);
  };

  // 点击菜单外部 / 滚动 / 缩放时关闭菜单
  useEffect(() => {
    if (menuOpenId === null) return;
    const onPointerDown = (e: MouseEvent) => {
      const target = e.target as Node;
      // 点在菜单内部：不关闭（让项目自身的 onClick 处理）
      if (menuRef.current && menuRef.current.contains(target)) return;
      // 点在侧边栏内部（如切换另一个菜单）：交给按钮的 onClick 处理
      if (sidebarRef.current && sidebarRef.current.contains(target)) return;
      closeMenu();
    };
    const onScrollOrResize = () => closeMenu();
    document.addEventListener("mousedown", onPointerDown);
    window.addEventListener("resize", onScrollOrResize);
    const list = convListRef.current;
    list?.addEventListener("scroll", onScrollOrResize);
    return () => {
      document.removeEventListener("mousedown", onPointerDown);
      window.removeEventListener("resize", onScrollOrResize);
      list?.removeEventListener("scroll", onScrollOrResize);
    };
  }, [menuOpenId]);

  const handleNewChat = () => {
    createConversation();
    onNavigate("chat");
  };

  const handleDotClick = (convId: string, e: React.MouseEvent<HTMLButtonElement>) => {
    e.stopPropagation();
    const rect = e.currentTarget.getBoundingClientRect();
    setMenuPos({ left: rect.right + 4, top: rect.top + rect.height / 2 });
    setMenuOpenId((prev) => (prev === convId ? null : convId));
  };

  const handleMenuAction = (convId: string, action: "pin" | "delete") => {
    if (action === "pin") {
      togglePinConversation(convId);
    } else {
      deleteConversation(convId);
    }
    closeMenu();
  };

  // 排序：置顶优先，然后按创建时间倒序
  const sortedConversations = [...conversations].sort((a, b) => {
    if (a.pinned && !b.pinned) return -1;
    if (!a.pinned && b.pinned) return 1;
    return new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime();
  });

  const openConv = sortedConversations.find((c) => c.id === menuOpenId) || null;

  return (
    <>
      <aside className={`sidebar ${collapsed ? "collapsed" : ""}`} ref={sidebarRef}>
        <div className="sidebar-header">
          <h1 className="sidebar-title">{t("sidebar.appTitle")}</h1>
          <div className="sidebar-header-actions">
            <button
              className={`btn btn-icon new-chat-btn ${!canCreateNew ? "disabled" : ""}`}
              onClick={handleNewChat}
              disabled={!canCreateNew}
              title={canCreateNew ? t("sidebar.newChat") : t("sidebar.emptyCantCreate")}
            >
              <PlusIcon size={18} />
            </button>
            <button
              className="sidebar-toggle-btn"
              onClick={onToggleCollapse}
              title={t("sidebar.collapseSidebar")}
            >
              <PanelLeftCloseIcon size={18} />
            </button>
          </div>
        </div>

        <nav className="sidebar-nav">
          <button
            className={`nav-item ${currentPage === "chat" ? "active" : ""}`}
            onClick={() => onNavigate("chat")}
          >
            <ChatIcon size={18} />
            <span>{t("sidebar.chat")}</span>
          </button>
          <button
            className={`nav-item ${currentPage === "ide" ? "active" : ""}`}
            onClick={() => onNavigate("ide")}
          >
            <CodeIcon size={18} />
            <span>{t("sidebar.ide")}</span>
          </button>
          <button
            className={`nav-item ${currentPage === "workspace" ? "active" : ""}`}
            onClick={() => onNavigate("workspace")}
          >
            <WorkspaceIcon size={18} />
            <span>{t("sidebar.workspace")}</span>
          </button>
          <button
            className={`nav-item ${currentPage === "browser" ? "active" : ""}`}
            onClick={() => onNavigate("browser")}
          >
            <GlobeIcon size={18} />
            <span>{t("sidebar.browser")}</span>
          </button>
          <button
            className={`nav-item ${currentPage === "settings" ? "active" : ""}`}
            onClick={() => onNavigate("settings")}
          >
            <SettingsIcon size={18} />
            <span>{t("sidebar.settings")}</span>
          </button>
        </nav>

        <div className="conversation-list" ref={convListRef}>
          <p className="list-label">{t("sidebar.history")}</p>
          {sortedConversations.map((conv) => (
            <div
              key={conv.id}
              className={`conversation-item-wrapper ${conv.id === activeConversationId ? "active" : ""}`}
            >
              <button
                className="conversation-item"
                onClick={() => {
                  setActiveConversation(conv.id);
                  onNavigate("chat");
                }}
              >
                {conv.pinned ? (
                  <PinFilledIcon size={14} className="conv-pin-icon" />
                ) : (
                  <ChatIcon size={14} />
                )}
                <span className="conversation-title">{conv.title}</span>
              </button>

              {/* 三点菜单按钮 */}
              <div className="conv-menu-container">
                <button
                  className={`conv-menu-btn ${menuOpenId === conv.id ? "active" : ""}`}
                  onClick={(e) => handleDotClick(conv.id, e)}
                  title={t("sidebar.moreOptions")}
                >
                  <DotsIcon size={14} />
                </button>
              </div>
            </div>
          ))}
        </div>
      </aside>

      {/* 菜单用 portal 渲染到 body，避免被侧边栏滚动容器裁切 */}
      {menuOpenId && menuPos && openConv && createPortal(
        <div
          ref={menuRef}
          className="conv-menu-dropdown"
          style={{ left: menuPos.left, top: menuPos.top }}
        >
          <button
            className="conv-menu-item"
            onClick={() => handleMenuAction(openConv.id, "pin")}
          >
            <PinIcon size={14} />
            <span>{openConv.pinned ? t("sidebar.unpinChat") : t("sidebar.pinChat")}</span>
          </button>
          <div className="conv-menu-divider" />
          <button
            className="conv-menu-item conv-menu-item-danger"
            onClick={() => handleMenuAction(openConv.id, "delete")}
          >
            <DeleteIcon size={14} />
            <span>{t("sidebar.deleteChat")}</span>
          </button>
        </div>,
        document.body,
      )}
    </>
  );
}
