import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import type { AgentConfig } from "../../stores/appStore";
import { useAppStore } from "../../stores/appStore";
import { XIcon } from "../../components/Icons";

/** 常用表情符号选单 */
const EMOJIS = [
  "🤖", "🦊", "🐻", "🐱", "🐶", "🦁", "🐯", "🐰",
  "🦉", "🐧", "🐸", "🐙", "🦋", "🐝", "🐳", "🐉",
  "⭐", "🔥", "⚡", "🌈", "🎯", "🎨", "📝", "💡",
  "🔧", "⚙️", "🛠️", "🧠", "👁️", "💬", "📚", "🎓",
];

interface AgentEditorProps {
  /** 编辑已有 agent，undefined=创建新 agent */
  editAgent?: AgentConfig;
  onClose: () => void;
}

export default function AgentEditor({ editAgent, onClose }: AgentEditorProps) {
  const { t } = useTranslation();

  // Store
  const addAgentConfig = useAppStore((s) => s.addAgentConfig);
  const updateAgentConfig = useAppStore((s) => s.updateAgentConfig);
  const providers = useAppStore((s) => s.providers);

  // Form state
  const [name, setName] = useState(editAgent?.name || "");
  const [description, setDescription] = useState(editAgent?.description || "");
  const [icon, setIcon] = useState(editAgent?.icon || "🤖");
  const [systemPrompt, setSystemPrompt] = useState(editAgent?.systemPrompt || "");
  const [showEmojiPicker, setShowEmojiPicker] = useState(false);

  const [enabledSkillIds] = useState<string[]>(
    editAgent?.enabledSkillIds || [],
  );
  const [enabledMcpServerNames] = useState<string[]>(
    editAgent?.enabledMcpServerNames || [],
  );
  const [providerId, setProviderId] = useState<string | null>(
    editAgent?.providerId || null,
  );
  const [model, setModel] = useState<string | null>(editAgent?.model || null);
  const [knowledgeBaseId] = useState<string | null>(
    editAgent?.knowledgeBaseId || null,
  );
  const [temperature, setTemperature] = useState(
    editAgent?.temperature ?? 0.7,
  );
  const [maxTokens, setMaxTokens] = useState(editAgent?.maxTokens ?? 4096);

  const isEditing = !!editAgent;

  // 当前选中的 provider
  const selectedProvider = providers.find((p) => p.id === providerId);
  const currentModels = selectedProvider?.models || [];

  const handleSave = () => {
    if (!name.trim()) return;

    const base = {
      name: name.trim(),
      description: description.trim(),
      icon,
      systemPrompt: systemPrompt.trim(),
      enabledSkillIds,
      enabledMcpServerNames,
      knowledgeBaseId,
      providerId,
      model,
      temperature,
      maxTokens,
    };

    if (isEditing && editAgent) {
      updateAgentConfig(editAgent.id, base);
    } else {
      addAgentConfig(base);
    }
    onClose();
  };

  const isValid = name.trim().length > 0;

  // 当 provider 变化时，如果之前的 model 不属于新 provider，就清空
  useEffect(() => {
    if (providerId && model) {
      const p = providers.find((pr) => pr.id === providerId);
      if (p && !p.models.includes(model)) {
        setModel(null);
      }
    }
  }, [providerId, model, providers]);

  return (
    <div className="agent-editor-overlay" onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div className="agent-editor">
        {/* Header */}
        <div className="agent-editor-header">
          <h2>{isEditing ? t("agents.editor.title") : t("agents.editor.createTitle")}</h2>
          <button className="btn btn-icon" onClick={onClose} title={t("agents.editor.cancel")}>
            <XIcon size={20} />
          </button>
        </div>

        <div className="agent-editor-body">
          {/* ===== 基本信息 ===== */}
          <section className="agent-editor-section">
            <h3 className="agent-editor-section-title">{t("agents.editor.basic")}</h3>

            {/* 头像选择 */}
            <div className="form-group">
              <label className="form-label">{t("agents.editor.icon")}</label>
              <p className="form-hint">{t("agents.editor.iconHint")}</p>
              <div className="agent-icon-picker">
                <button
                  className="agent-icon-selected"
                  onClick={() => setShowEmojiPicker(!showEmojiPicker)}
                  title={t("agents.editor.icon")}
                >
                  <span className="agent-emoji">{icon}</span>
                </button>
                {showEmojiPicker && (
                  <div className="agent-emoji-grid">
                    {EMOJIS.map((emoji) => (
                      <button
                        key={emoji}
                        className={`agent-emoji-option ${icon === emoji ? "active" : ""}`}
                        onClick={() => { setIcon(emoji); setShowEmojiPicker(false); }}
                      >
                        {emoji}
                      </button>
                    ))}
                  </div>
                )}
              </div>
            </div>

            {/* 名称 */}
            <div className="form-group">
              <label className="form-label">{t("agents.editor.name")}</label>
              <input
                className="form-input"
                type="text"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder={t("agents.editor.namePlaceholder")}
                autoFocus
              />
            </div>

            {/* 简介 */}
            <div className="form-group">
              <label className="form-label">{t("agents.editor.description")}</label>
              <input
                className="form-input"
                type="text"
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                placeholder={t("agents.editor.descriptionPlaceholder")}
              />
            </div>
          </section>

          {/* ===== 系统提示词 ===== */}
          <section className="agent-editor-section">
            <h3 className="agent-editor-section-title">{t("agents.editor.systemPrompt")}</h3>
            <p className="form-hint">{t("agents.editor.systemPromptHint")}</p>
            <div className="form-group">
              <textarea
                className="form-textarea agent-prompt-textarea"
                value={systemPrompt}
                onChange={(e) => setSystemPrompt(e.target.value)}
                placeholder={t("agents.editor.systemPromptPlaceholder")}
                rows={10}
              />
            </div>
          </section>

          {/* ===== 工具 ===== */}
          <section className="agent-editor-section">
            <h3 className="agent-editor-section-title">{t("agents.editor.tools")}</h3>

            {/* Skills */}
            <div className="form-group">
              <label className="form-label">{t("agents.editor.enabledSkills")}</label>
              <p className="form-hint">{t("agents.editor.skillsDesc")}</p>
              <p className="form-empty-hint">安装 Skills 后可在编辑器中启用</p>
            </div>

            {/* MCP Tools */}
            <div className="form-group">
              <label className="form-label">{t("agents.editor.enabledMcp")}</label>
              <p className="form-hint">{t("agents.editor.mcpDesc")}</p>
            </div>
          </section>

          {/* ===== 模型 ===== */}
          <section className="agent-editor-section">
            <h3 className="agent-editor-section-title">{t("agents.editor.model")}</h3>
            <p className="form-hint">{t("agents.editor.modelDesc")}</p>

            <div className="form-group">
              <label className="form-label">{t("settings.sections.models")}</label>
              <select
                className="form-select"
                value={providerId || ""}
                onChange={(e) => setProviderId(e.target.value || null)}
              >
                <option value="">{t("agents.editor.modelDefault")}</option>
                {providers.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
              </select>
            </div>

            {providerId && (
              <div className="form-group">
                <label className="form-label">模型</label>
                <select
                  className="form-select"
                  value={model || ""}
                  onChange={(e) => setModel(e.target.value || null)}
                >
                  <option value="">{`使用 ${selectedProvider?.name} 的默认模型`}</option>
                  {currentModels.map((m) => (
                    <option key={m} value={m}>
                      {m}
                    </option>
                  ))}
                </select>
              </div>
            )}
          </section>

          {/* ===== 知识库 ===== */}
          <section className="agent-editor-section">
            <h3 className="agent-editor-section-title">{t("agents.editor.knowledge")}</h3>
            <p className="form-hint">{t("agents.editor.knowledgeDesc")}</p>
            <div className="form-group">
              <select
                className="form-select"
                value={knowledgeBaseId || ""}
                disabled
              >
                <option value="">{t("agents.editor.knowledgeNone")}</option>
              </select>
            </div>
          </section>

          {/* ===== 高级设置 ===== */}
          <section className="agent-editor-section">
            <h3 className="agent-editor-section-title">{t("agents.editor.advanced")}</h3>

            <div className="form-group">
              <label className="form-label">{t("agents.editor.temperature")}: {temperature.toFixed(1)}</label>
              <p className="form-hint">{t("agents.editor.temperatureHint")}</p>
              <input
                className="form-range"
                type="range"
                min="0"
                max="2"
                step="0.1"
                value={temperature}
                onChange={(e) => setTemperature(parseFloat(e.target.value))}
              />
            </div>

            <div className="form-group">
              <label className="form-label">{t("agents.editor.maxTokens")}</label>
              <p className="form-hint">{t("agents.editor.maxTokensHint")}</p>
              <input
                className="form-input"
                type="number"
                min={256}
                max={131072}
                step={256}
                value={maxTokens}
                onChange={(e) => setMaxTokens(parseInt(e.target.value, 10) || 4096)}
              />
            </div>
          </section>
        </div>

        {/* Footer */}
        <div className="agent-editor-footer">
          <button className="btn btn-secondary" onClick={onClose}>
            {t("agents.editor.cancel")}
          </button>
          <button
            className="btn btn-primary"
            onClick={handleSave}
            disabled={!isValid}
          >
            {t("agents.editor.save")}
          </button>
        </div>
      </div>
    </div>
  );
}
