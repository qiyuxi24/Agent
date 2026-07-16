import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useAppStore, type ModelProvider } from "../../stores/appStore";
import { EditIcon, DeleteIcon, PlusIcon, ErrorIcon, RefreshIcon } from "../../components/Icons";

export default function ModelsPanel() {
  const { t } = useTranslation();
  const providers = useAppStore((s) => s.providers);
  const addProvider = useAppStore((s) => s.addProvider);
  const updateProvider = useAppStore((s) => s.updateProvider);
  const removeProvider = useAppStore((s) => s.removeProvider);
  const addModel = useAppStore((s) => s.addModel);
  const removeModel = useAppStore((s) => s.removeModel);
  const quotaExhaustedModels = useAppStore((s) => s.quotaExhaustedModels);
  const clearModelExhausted = useAppStore((s) => s.clearModelExhausted);
  const clearAllExhausted = useAppStore((s) => s.clearAllExhausted);

  const [showAddForm, setShowAddForm] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);

  const [newName, setNewName] = useState("");
  const [newBase, setNewBase] = useState("https://api.openai.com/v1");
  const [newKey, setNewKey] = useState("");
  const [newModels, setNewModels] = useState("gpt-4o");

  const [editName, setEditName] = useState("");
  const [editBase, setEditBase] = useState("");
  const [editKey, setEditKey] = useState("");

  const [addModelInputs, setAddModelInputs] = useState<Record<string, string>>({});

  const handleAddProvider = () => {
    if (!newName.trim() || !newKey.trim()) return;
    const models = newModels.split(",").map((m) => m.trim()).filter(Boolean);
    if (models.length === 0) return;
    addProvider(newName.trim(), newBase.trim() || "https://api.openai.com/v1", newKey.trim(), models);
    setNewName("");
    setNewBase("https://api.openai.com/v1");
    setNewKey("");
    setNewModels("gpt-4o");
    setShowAddForm(false);
  };

  const startEdit = (p: ModelProvider) => {
    setEditingId(p.id);
    setEditName(p.name);
    setEditBase(p.apiBase);
    setEditKey(p.apiKey);
  };

  const saveEdit = (id: string) => {
    updateProvider(id, { name: editName, apiBase: editBase, apiKey: editKey });
    setEditingId(null);
  };

  const handleAddModel = (providerId: string) => {
    const model = (addModelInputs[providerId] || "").trim();
    if (!model) return;
    addModel(providerId, model);
    setAddModelInputs((prev) => ({ ...prev, [providerId]: "" }));
  };

  return (
    <section className="settings-panel">
      <div className="section-header models-section-header">
        <div className="section-header-actions">
          {Object.keys(quotaExhaustedModels).length > 0 && (
            <button className="btn btn-sm btn-ghost" onClick={clearAllExhausted} title={t("settings.models.clearExhausted")}>
              <RefreshIcon size={13} />
              {t("settings.models.clearExhausted")}
            </button>
          )}
          <button className="btn btn-add-provider" onClick={() => setShowAddForm(!showAddForm)}>
            <PlusIcon size={14} />
            {t("settings.models.add")}
          </button>
        </div>
      </div>

      {showAddForm && (
        <div className="provider-form">
          <div className="form-group">
            <label>{t("settings.models.name")}</label>
            <input type="text" value={newName} onChange={(e) => setNewName(e.target.value)} placeholder={t("settings.models.namePlaceholder")} />
          </div>
          <div className="form-group">
            <label>{t("settings.models.apiBase")}</label>
            <input type="text" value={newBase} onChange={(e) => setNewBase(e.target.value)} placeholder="https://api.openai.com/v1" />
          </div>
          <div className="form-group">
            <label>{t("settings.models.apiKey")}</label>
            <input type="password" value={newKey} onChange={(e) => setNewKey(e.target.value)} placeholder="sk-..." />
          </div>
          <div className="form-group">
            <label>{t("settings.models.modelsLabel")}</label>
            <input type="text" value={newModels} onChange={(e) => setNewModels(e.target.value)} placeholder={t("settings.models.modelsPlaceholder")} />
            <span className="form-hint">{t("settings.models.modelsHint")}</span>
          </div>
          <div className="form-actions">
            <button className="btn btn-primary" onClick={handleAddProvider}>{t("settings.models.confirm")}</button>
            <button className="btn btn-cancel" onClick={() => setShowAddForm(false)}>{t("settings.models.cancel")}</button>
          </div>
        </div>
      )}

      <div className="provider-list">
        {providers.length === 0 ? (
          <div className="placeholder-section">
            <p>{t("settings.models.empty")}</p>
          </div>
        ) : (
          providers.map((p) => (
            <div key={p.id} className="provider-card">
              {editingId === p.id ? (
                <div className="provider-edit-form">
                  <div className="form-group">
                    <label>{t("settings.models.name")}</label>
                    <input value={editName} onChange={(e) => setEditName(e.target.value)} />
                  </div>
                  <div className="form-group">
                    <label>{t("settings.models.apiBase")}</label>
                    <input value={editBase} onChange={(e) => setEditBase(e.target.value)} />
                  </div>
                  <div className="form-group">
                    <label>{t("settings.models.apiKey")}</label>
                    <input type="password" value={editKey} onChange={(e) => setEditKey(e.target.value)} />
                  </div>
                  <div className="form-actions">
                    <button className="btn btn-primary" onClick={() => saveEdit(p.id)}>{t("settings.models.save")}</button>
                    <button className="btn btn-cancel" onClick={() => setEditingId(null)}>{t("settings.models.cancel")}</button>
                  </div>
                </div>
              ) : (
                <>
                  <div className="provider-card-header">
                    <div className="provider-info">
                      <h4>{p.name}</h4>
                      <span className="provider-base">{p.apiBase}</span>
                    </div>
                    <div className="provider-actions">
                      <button className="btn btn-icon-sm" onClick={() => startEdit(p)} title={t("settings.models.edit")}>
                        <EditIcon size={14} />
                      </button>
                      <button className="btn btn-icon-sm btn-danger" onClick={() => removeProvider(p.id)} title={t("settings.models.delete")}>
                        <DeleteIcon size={14} />
                      </button>
                    </div>
                  </div>

                  <div className="provider-models">
                    <span className="models-label">{t("settings.models.currentModels")}</span>
                    <div className="model-tags">
                      {p.models.map((m) => {
                        const isExhausted = !!quotaExhaustedModels[`${p.id}::${m}`];
                        return (
                          <span key={m} className={`model-tag ${isExhausted ? "model-tag-exhausted" : ""}`}>
                            {isExhausted && (
                              <ErrorIcon size={12} className="model-exhausted-icon" />
                            )}
                            {m}
                            {isExhausted && (
                              <button
                                className="model-tag-restore"
                                onClick={(e) => { e.stopPropagation(); clearModelExhausted(p.id, m); }}
                                title={t("settings.models.restoreModel", { model: m })}
                              >
                                <RefreshIcon size={11} />
                              </button>
                            )}
                            <button className="model-tag-remove" onClick={() => removeModel(p.id, m)} title={t("settings.models.removeModel", { model: m })}>×</button>
                          </span>
                        );
                      })}
                      <div className="model-add-inline">
                        <input
                          className="model-add-input"
                          placeholder={t("settings.models.addModelPlaceholder")}
                          value={addModelInputs[p.id] || ""}
                          onChange={(e) => setAddModelInputs((prev) => ({ ...prev, [p.id]: e.target.value }))}
                          onKeyDown={(e) => { if (e.key === "Enter") handleAddModel(p.id); }}
                        />
                        <button className="btn btn-icon-sm" onClick={() => handleAddModel(p.id)}>+</button>
                      </div>
                    </div>
                  </div>
                </>
              )}
            </div>
          ))
        )}
      </div>
    </section>
  );
}
