import { useState, useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import ErrorBoundary from "./components/ErrorBoundary";
import Sidebar from "./components/Sidebar";
import ChatView, { type ChatViewHandle } from "./pages/ChatView";
import SettingsPage from "./pages/SettingsPage";
import BrowserPanel from "./pages/BrowserPanel";
import { useAppStore } from "./stores/appStore";
import { useKeyboardShortcuts } from "./hooks/useKeyboardShortcuts";
import { PanelLeftOpenIcon } from "./components/Icons";

type Page = "chat" | "settings" | "browser";

function App() {
  const { t } = useTranslation();
  const [page, setPage] = useState<Page>("chat");
  const chatViewRef = useRef<ChatViewHandle>(null);
  const [appErrorKey, setAppErrorKey] = useState(0);

  // Store
  const {
    activeConversationId, ready,
    sidebarCollapsed,
    loadFromStore, createConversation, toggleSidebar,
  } = useAppStore();

  useEffect(() => {
    loadFromStore();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // 启动后自动重连已保存的 MCP 服务器
  useEffect(() => {
    if (!ready) return;
    if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) return;
    const servers = useAppStore.getState().mcpServers;
    if (servers.length === 0) return;
    (async () => {
      try {
        const { invoke } = await import("@tauri-apps/api/core");
        for (const s of servers) {
          try {
            await invoke("mcp_connect", {
              config: {
                name: s.name,
                command: s.command,
                args: s.args,
                env: s.env || null,
              },
            });
          } catch (e) {
            console.error("[MCP] 自动连接失败:", s.name, e);
          }
        }
      } catch {
        // 非 Tauri 环境忽略
      }
    })();
  }, [ready]);

  // 监听系统主题变化
  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const handleChange = () => {
      const { theme } = useAppStore.getState();
      if (theme === "system") {
        const root = document.documentElement;
        root.classList.remove("theme-light", "theme-dark");
        root.classList.add(mq.matches ? "theme-dark" : "theme-light");
      }
    };
    mq.addEventListener("change", handleChange);
    return () => mq.removeEventListener("change", handleChange);
  }, []);

  // 全局快捷键
  useKeyboardShortcuts({
    onNewConversation: () => createConversation(),
    onFocusInput: () => chatViewRef.current?.focusInput(),
    onToggleModelPicker: () => chatViewRef.current?.toggleModelPicker(),
    onOpenSettings: () => setPage("settings"),
  });

  if (!ready) {
    return (
      <div className="app-loading">
        <p>{t("app.loading")}</p>
      </div>
    );
  }

  return (
    <ErrorBoundary
      key={appErrorKey}
      fallback={
        <div className="app-error-fallback">
          <div className="error-fallback-card">
            <h2>{t("errorBoundary.title")}</h2>
            <p>{t("errorBoundary.message")}</p>
            <button
              className="btn btn-primary"
              onClick={() => setAppErrorKey((k) => k + 1)}
            >
              {t("errorBoundary.retry")}
            </button>
          </div>
        </div>
      }
    >
      <div className="app-container">
        <Sidebar
          currentPage={page}
          onNavigate={setPage}
          collapsed={sidebarCollapsed}
          onToggleCollapse={toggleSidebar}
        />
        <main className="main-content">
          {page === "browser" ? (
            <BrowserPanel />
          ) : (
            <ChatView ref={chatViewRef} conversationId={activeConversationId} />
          )}
        </main>

        {/* 侧边栏折叠时，左上角浮动展开按钮 */}
        {sidebarCollapsed && (
          <button
            className="sidebar-expand-float"
            onClick={toggleSidebar}
            title={t("sidebar.expandSidebar")}
          >
            <PanelLeftOpenIcon size={18} />
          </button>
        )}

        {/* 设置页作为独立浮层覆盖在主页面之上 */}
        {page === "settings" && (
          <SettingsPage onClose={() => setPage("chat")} />
        )}
      </div>
    </ErrorBoundary>
  );
}

export default App;
