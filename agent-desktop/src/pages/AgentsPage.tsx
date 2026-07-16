import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useAppStore } from "../stores/appStore";
import type { AgentConfig } from "../stores/appStore";
import { BotIcon, PlusIcon, DeleteIcon, EditIcon, ChatIcon, CopyIcon } from "../components/Icons";
import AgentEditor from "./agents/AgentEditor";

export default function AgentsPage() {
  const { t } = useTranslation();

  const agentConfigs = useAppStore((s) => s.agentConfigs);
  const removeAgentConfig = useAppStore((s) => s.removeAgentConfig);
  const setActiveAgent = useAppStore((s) => s.setActiveAgent);
  const createConversation = useAppStore((s) => s.createConversation);

  const [editorOpen, setEditorOpen] = useState(false);
  const [editTarget, setEditTarget] = useState<AgentConfig | undefined>(undefined);
  const [deleteConfirm, setDeleteConfirm] = useState<string | null>(null);

  const openCreate = () => {
    setEditTarget(undefined);
    setEditorOpen(true);
  };

  const openEdit = (agent: AgentConfig, e: React.MouseEvent) => {
    e.stopPropagation();
    setEditTarget(agent);
    setEditorOpen(true);
  };

  const handleDelete = (e: React.MouseEvent, agent: AgentConfig) => {
    e.stopPropagation();
    setDeleteConfirm(agent.id);
  };

  const confirmDelete = () => {
    if (deleteConfirm) {
      removeAgentConfig(deleteConfirm);
      setDeleteConfirm(null);
    }
  };

  const handleChat = (agent: AgentConfig) => {
    setActiveAgent(agent.id);
    createConversation();
  };

  const handleClone = (agent: AgentConfig, e: React.MouseEvent) => {
    e.stopPropagation();
    // 复用 editor 但标记为未编辑
    setEditTarget({
      ...agent,
      id: "",
      name: `${agent.name} (Copy)`,
      createdAt: "",
      updatedAt: "",
    });
    setEditorOpen(true);
  };

  return (
    <div className="agents-page">
      {/* Header */}
      <div className="agents-page-header">
        <div className="agents-header-left">
          <BotIcon size={24} />
          <div>
            <h1 className="agents-page-title">{t("agents.title")}</h1>
            <p className="agents-page-desc">{t("agents.desc")}</p>
          </div>
        </div>
        <button className="btn btn-primary agents-create-btn" onClick={openCreate}>
          <PlusIcon size={16} />
          <span>{t("agents.create")}</span>
        </button>
      </div>

      {/* Agent Grid */}
      {agentConfigs.length === 0 ? (
        <div className="agents-empty">
          <BotIcon size={48} className="agents-empty-icon" />
          <h2>{t("agents.empty")}</h2>
          <p>{t("agents.emptyHint")}</p>
          <button className="btn btn-primary" onClick={openCreate}>
            <PlusIcon size={16} />
            <span>{t("agents.create")}</span>
          </button>
        </div>
      ) : (
        <div className="agents-grid">
          {agentConfigs.map((agent) => (
            <div key={agent.id} className="agent-card">
              <div className="agent-card-header">
                <span className="agent-card-icon">{agent.icon}</span>
                <div className="agent-card-actions">
                  <button
                    className="btn-icon agent-card-action-btn"
                    onClick={(e) => handleClone(agent, e)}
                    title={t("agents.clone")}
                  >
                    <CopyIcon size={14} />
                  </button>
                  <button
                    className="btn-icon agent-card-action-btn"
                    onClick={(e) => openEdit(agent, e)}
                    title={t("agents.edit")}
                  >
                    <EditIcon size={14} />
                  </button>
                  <button
                    className="btn-icon agent-card-action-btn agent-card-action-danger"
                    onClick={(e) => handleDelete(e, agent)}
                    title={t("agents.delete")}
                  >
                    <DeleteIcon size={14} />
                  </button>
                </div>
              </div>
              <h3 className="agent-card-name">{agent.name}</h3>
              <p className="agent-card-desc">{agent.description || "—"}</p>

              {/* Metadata chips */}
              <div className="agent-card-meta">
                {agent.providerId && (
                  <span className="agent-card-chip" title={agent.model || "默认模型"}>
                    模型关联
                  </span>
                )}
                {agent.enabledSkillIds.length > 0 && (
                  <span className="agent-card-chip">
                    {agent.enabledSkillIds.length} Skills
                  </span>
                )}
                {agent.enabledMcpServerNames.length > 0 && (
                  <span className="agent-card-chip">
                    {agent.enabledMcpServerNames.length} MCP
                  </span>
                )}
              </div>

              <div className="agent-card-footer">
                <button
                  className="btn btn-primary agent-card-chat-btn"
                  onClick={() => handleChat(agent)}
                >
                  <ChatIcon size={14} />
                  <span>{t("agents.chatWith")}</span>
                </button>
              </div>
            </div>
          ))}
        </div>
      )}

      {/* Editor Modal */}
      {editorOpen && (
        <AgentEditor
          editAgent={editTarget?.id ? editTarget : undefined}
          onClose={() => setEditorOpen(false)}
        />
      )}

      {/* Delete Confirm Modal */}
      {deleteConfirm && (
        <div className="agent-delete-overlay" onClick={() => setDeleteConfirm(null)}>
          <div className="agent-delete-dialog" onClick={(e) => e.stopPropagation()}>
            <p>
              {t("agents.deleteConfirm", {
                name: agentConfigs.find((a) => a.id === deleteConfirm)?.name || "",
              })}
            </p>
            <div className="agent-delete-actions">
              <button className="btn btn-secondary" onClick={() => setDeleteConfirm(null)}>
                {t("agents.editor.cancel")}
              </button>
              <button className="btn btn-danger" onClick={confirmDelete}>
                {t("agents.delete")}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
