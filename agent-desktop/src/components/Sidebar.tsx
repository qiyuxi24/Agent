import { useAppStore, type Conversation } from "../stores/appStore";

interface SidebarProps {
  currentPage: "chat" | "settings";
  onNavigate: (page: "chat" | "settings") => void;
}

export default function Sidebar({ currentPage, onNavigate }: SidebarProps) {
  const {
    conversations,
    activeConversationId,
    messages,
    setActiveConversation,
    createConversation,
    deleteConversation,
  } = useAppStore();

  const handleNewChat = () => {
    createConversation();
    onNavigate("chat");
  };

  const currentMsgs = messages[activeConversationId || ""] || [];
  const canCreateNew = currentMsgs.length > 0;

  return (
    <aside className="sidebar">
      <div className="sidebar-header">
        <h1 className="sidebar-title">Agent Desktop</h1>
        <button
          className={`btn btn-icon new-chat-btn ${!canCreateNew ? "disabled" : ""}`}
          onClick={handleNewChat}
          disabled={!canCreateNew}
          title={canCreateNew ? "新建对话" : "当前对话为空，无法新建"}
        >
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <line x1="12" y1="5" x2="12" y2="19" />
            <line x1="5" y1="12" x2="19" y2="12" />
          </svg>
        </button>
      </div>

      <nav className="sidebar-nav">
        <button
          className={`nav-item ${currentPage === "chat" ? "active" : ""}`}
          onClick={() => onNavigate("chat")}
        >
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
          </svg>
          <span>对话</span>
        </button>
        <button
          className={`nav-item ${currentPage === "settings" ? "active" : ""}`}
          onClick={() => onNavigate("settings")}
        >
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <circle cx="12" cy="12" r="3" />
            <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
          </svg>
          <span>设置</span>
        </button>
      </nav>

      <div className="conversation-list">
        <p className="list-label">对话历史</p>
        {conversations.map((conv: Conversation) => (
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
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
              </svg>
              <span className="conversation-title">{conv.title}</span>
            </button>
            <button
              className="conversation-delete"
              onClick={(e) => {
                e.stopPropagation();
                deleteConversation(conv.id);
              }}
              title="删除对话"
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <line x1="18" y1="6" x2="6" y2="18" />
                <line x1="6" y1="6" x2="18" y2="18" />
              </svg>
            </button>
          </div>
        ))}
      </div>
    </aside>
  );
}
