import { useState, useEffect, useRef } from "react";
import Sidebar from "./components/Sidebar";
import ChatView, { type ChatViewHandle } from "./pages/ChatView";
import SettingsPage from "./pages/SettingsPage";
import { useAppStore } from "./stores/appStore";
import { useKeyboardShortcuts } from "./hooks/useKeyboardShortcuts";

type Page = "chat" | "settings";

function App() {
  const [page, setPage] = useState<Page>("chat");
  const { activeConversationId, ready, loadFromStore, createConversation } = useAppStore();
  const chatViewRef = useRef<ChatViewHandle>(null);

  useEffect(() => {
    loadFromStore();
  }, []);

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
        <p>加载中...</p>
      </div>
    );
  }

  return (
    <div className="app-container">
      <Sidebar currentPage={page} onNavigate={setPage} />
      <main className="main-content">
        <ChatView ref={chatViewRef} conversationId={activeConversationId} />
      </main>

      {/* 设置页作为独立浮层覆盖在主页面之上 */}
      {page === "settings" && (
        <SettingsPage onClose={() => setPage("chat")} />
      )}
    </div>
  );
}

export default App;
