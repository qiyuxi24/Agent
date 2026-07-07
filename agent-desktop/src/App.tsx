import { useState, useEffect } from "react";
import Sidebar from "./components/Sidebar";
import ChatView from "./pages/ChatView";
import SettingsPage from "./pages/SettingsPage";
import { useAppStore } from "./stores/appStore";

type Page = "chat" | "settings";

function App() {
  const [page, setPage] = useState<Page>("chat");
  const { activeConversationId, ready, loadFromStore } = useAppStore();

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
        {page === "chat" && <ChatView conversationId={activeConversationId} />}
        {page === "settings" && <SettingsPage />}
      </main>
    </div>
  );
}

export default App;
