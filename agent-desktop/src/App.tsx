import { useState, useEffect, useRef, Suspense, lazy } from "react";
import { useTranslation } from "react-i18next";
import ErrorBoundary from "./components/ErrorBoundary";
import Sidebar from "./components/Sidebar";
import ChatView, { type ChatViewHandle } from "./pages/ChatView";
import { useAppStore } from "./stores/appStore";
import { useKeyboardShortcuts } from "./hooks/useKeyboardShortcuts";
import { PanelLeftOpenIcon } from "./components/Icons";

// 页面级懒加载 — 首次打开时才下载对应 chunk
const SettingsPage = lazy(() => import("./pages/SettingsPage"));
const BrowserPanel = lazy(() => import("./pages/BrowserPanel"));
const IdePage = lazy(() => import("./pages/IdePage"));

type Page = "chat" | "settings" | "browser" | "ide";

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
      const { invoke } = await import("@tauri-apps/api/core");

      // 先检测 node/npx 是否可用（内置服务器依赖 Node.js）
      let nodeOk = false;
      try {
        const prereqResults = await invoke<string[]>("mcp_check_prereq", { command: "node" });
        nodeOk = prereqResults.some((r) => r.startsWith("✓ node"));
        if (!nodeOk) {
          console.warn("[MCP] Node.js 未安装，内置服务器（web/tavily）将无法连接。请安装 Node.js v18+: https://nodejs.org");
        }
      } catch {
        console.warn("[MCP] 无法检测 Node.js 依赖，可能不在 Tauri 环境中");
      }

      for (const s of servers) {
        try {
          // 对 node/npx 命令，如果 node 不可用则跳过（避免无效连接尝试）
          const baseCmd = s.command.split(/\s+/)[0].toLowerCase();
          if ((baseCmd === "node" || baseCmd === "npx") && !nodeOk) {
            console.warn(`[MCP] 跳过 ${s.name}：Node.js 不可用`);
            continue;
          }
          await invoke("mcp_connect", {
            config: {
              name: s.name,
              command: s.command,
              args: s.args,
              env: s.env || null,
            },
          });
          console.log(`[MCP] 自动连接成功: ${s.name}`);
        } catch (e) {
          console.error(`[MCP] 自动连接失败: ${s.name}`, e);
        }
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

  // IDE 启动：打开独立 Tauri 窗口
  const handleNavigate = async (p: Page) => {
    if (p === "ide") {
      try {
        const { invoke } = await import("@tauri-apps/api/core");
        await invoke("code_server_open_ide_window");
      } catch (e) {
        console.error("打开 IDE 窗口失败:", e);
        // 降级：在主页面内显示 IdePage
        setPage("ide");
      }
      return;
    }
    setPage(p);
  };

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
          onNavigate={handleNavigate}
          collapsed={sidebarCollapsed}
          onToggleCollapse={toggleSidebar}
        />
        <main className="main-content">
          <Suspense fallback={<div className="app-loading"><p>{t("app.loading")}</p></div>}>
            {page === "browser" ? (
              <BrowserPanel />
            ) : page === "ide" ? (
              <IdePage />
            ) : (
              <ChatView ref={chatViewRef} conversationId={activeConversationId} />
            )}
          </Suspense>
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
          <Suspense fallback={<div className="app-loading"><p>{t("app.loading")}</p></div>}>
            <SettingsPage onClose={() => setPage("chat")} />
          </Suspense>
        )}
      </div>
    </ErrorBoundary>
  );
}

export default App;
