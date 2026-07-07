import { useState } from "react";
import { useAppStore, type ThemeMode, type ModelProvider } from "../stores/appStore";

export default function SettingsPage() {
  const {
    theme, setTheme,
    language, setLanguage,
    providers,
    addProvider,
    updateProvider,
    removeProvider,
    addModel,
    removeModel,
  } = useAppStore();

  const [showAddForm, setShowAddForm] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);

  // 新 provider 表单
  const [newName, setNewName] = useState("");
  const [newBase, setNewBase] = useState("https://api.openai.com/v1");
  const [newKey, setNewKey] = useState("");
  const [newModels, setNewModels] = useState("gpt-4o");

  // 编辑 provider 表单
  const [editName, setEditName] = useState("");
  const [editBase, setEditBase] = useState("");
  const [editKey, setEditKey] = useState("");

  // 添加模型输入
  const [addModelInputs, setAddModelInputs] = useState<Record<string, string>>({});

  const themeOptions: { value: ThemeMode; label: string }[] = [
    { value: "system", label: "跟随系统" },
    { value: "light", label: "浅色" },
    { value: "dark", label: "深色" },
  ];

  const langOptions = [
    { value: "zh-CN", label: "简体中文" },
    { value: "en", label: "English" },
  ];

  const handleAddProvider = () => {
    if (!newName.trim() || !newKey.trim()) return;
    const models = newModels
      .split(",")
      .map((m) => m.trim())
      .filter(Boolean);
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
    updateProvider(id, {
      name: editName,
      apiBase: editBase,
      apiKey: editKey,
    });
    setEditingId(null);
  };

  const handleAddModel = (providerId: string) => {
    const model = (addModelInputs[providerId] || "").trim();
    if (!model) return;
    addModel(providerId, model);
    setAddModelInputs((prev) => ({ ...prev, [providerId]: "" }));
  };

  return (
    <div className="settings-page">
      <div className="settings-container">
        <h2>设置</h2>

        {/* ===== 通用 ===== */}
        <section className="settings-section">
          <h3 className="section-title">通用</h3>

          <div className="form-group">
            <label>语言</label>
            <select
              value={language}
              onChange={(e) => setLanguage(e.target.value)}
              className="form-select"
            >
              {langOptions.map((opt) => (
                <option key={opt.value} value={opt.value}>
                  {opt.label}
                </option>
              ))}
            </select>
          </div>

          <div className="form-group">
            <label>主题</label>
            <div className="theme-selector">
              {themeOptions.map((opt) => (
                <button
                  key={opt.value}
                  className={`theme-btn ${theme === opt.value ? "active" : ""}`}
                  onClick={() => setTheme(opt.value)}
                >
                  <span className={`theme-preview theme-preview-${opt.value}`} />
                  <span>{opt.label}</span>
                </button>
              ))}
            </div>
          </div>
        </section>

        {/* ===== AI 模型管理 ===== */}
        <section className="settings-section">
          <div className="section-header">
            <h3 className="section-title">AI 模型</h3>
            <button
              className="btn btn-add-provider"
              onClick={() => setShowAddForm(!showAddForm)}
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <line x1="12" y1="5" x2="12" y2="19" />
                <line x1="5" y1="12" x2="19" y2="12" />
              </svg>
              添加
            </button>
          </div>

          {/* 添加新 provider 表单 */}
          {showAddForm && (
            <div className="provider-form">
              <div className="form-group">
                <label>名称</label>
                <input
                  type="text"
                  value={newName}
                  onChange={(e) => setNewName(e.target.value)}
                  placeholder="例如：OpenAI、DeepSeek"
                />
              </div>
              <div className="form-group">
                <label>API 地址</label>
                <input
                  type="text"
                  value={newBase}
                  onChange={(e) => setNewBase(e.target.value)}
                  placeholder="https://api.openai.com/v1"
                />
              </div>
              <div className="form-group">
                <label>API Key</label>
                <input
                  type="password"
                  value={newKey}
                  onChange={(e) => setNewKey(e.target.value)}
                  placeholder="sk-..."
                />
              </div>
              <div className="form-group">
                <label>模型列表（逗号分隔）</label>
                <input
                  type="text"
                  value={newModels}
                  onChange={(e) => setNewModels(e.target.value)}
                  placeholder="gpt-4o, gpt-4o-mini"
                />
                <span className="form-hint">用英文逗号分隔多个模型名称</span>
              </div>
              <div className="form-actions">
                <button className="btn btn-primary" onClick={handleAddProvider}>
                  确认添加
                </button>
                <button className="btn btn-cancel" onClick={() => setShowAddForm(false)}>
                  取消
                </button>
              </div>
            </div>
          )}

          {/* provider 列表 */}
          <div className="provider-list">
            {providers.length === 0 ? (
              <div className="placeholder-section">
                <p>尚未添加模型，点击上方"添加"按钮开始配置。</p>
              </div>
            ) : (
              providers.map((p) => (
                <div key={p.id} className="provider-card">
                  {editingId === p.id ? (
                    /* 编辑模式 */
                    <div className="provider-edit-form">
                      <div className="form-group">
                        <label>名称</label>
                        <input value={editName} onChange={(e) => setEditName(e.target.value)} />
                      </div>
                      <div className="form-group">
                        <label>API 地址</label>
                        <input value={editBase} onChange={(e) => setEditBase(e.target.value)} />
                      </div>
                      <div className="form-group">
                        <label>API Key</label>
                        <input
                          type="password"
                          value={editKey}
                          onChange={(e) => setEditKey(e.target.value)}
                        />
                      </div>
                      <div className="form-actions">
                        <button className="btn btn-primary" onClick={() => saveEdit(p.id)}>
                          保存
                        </button>
                        <button className="btn btn-cancel" onClick={() => setEditingId(null)}>
                          取消
                        </button>
                      </div>
                    </div>
                  ) : (
                    /* 查看模式 */
                    <>
                      <div className="provider-card-header">
                        <div className="provider-info">
                          <h4>{p.name}</h4>
                          <span className="provider-base">{p.apiBase}</span>
                        </div>
                        <div className="provider-actions">
                          <button
                            className="btn btn-icon-sm"
                            onClick={() => startEdit(p)}
                            title="编辑"
                          >
                            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                              <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7" />
                              <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z" />
                            </svg>
                          </button>
                          <button
                            className="btn btn-icon-sm btn-danger"
                            onClick={() => removeProvider(p.id)}
                            title="删除"
                          >
                            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                              <polyline points="3 6 5 6 21 6" />
                              <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
                            </svg>
                          </button>
                        </div>
                      </div>

                      <div className="provider-models">
                        <span className="models-label">模型：</span>
                        <div className="model-tags">
                          {p.models.map((m) => (
                            <span key={m} className="model-tag">
                              {m}
                              <button
                                className="model-tag-remove"
                                onClick={() => removeModel(p.id, m)}
                                title={`移除 ${m}`}
                              >
                                ×
                              </button>
                            </span>
                          ))}
                          <div className="model-add-inline">
                            <input
                              className="model-add-input"
                              placeholder="添加模型..."
                              value={addModelInputs[p.id] || ""}
                              onChange={(e) =>
                                setAddModelInputs((prev) => ({
                                  ...prev,
                                  [p.id]: e.target.value,
                                }))
                              }
                              onKeyDown={(e) => {
                                if (e.key === "Enter") handleAddModel(p.id);
                              }}
                            />
                            <button
                              className="btn btn-icon-sm"
                              onClick={() => handleAddModel(p.id)}
                            >
                              +
                            </button>
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

        {/* ===== 插件 (占位) ===== */}
        <section className="settings-section">
          <h3 className="section-title">插件</h3>
          <div className="placeholder-section">
            <p>插件系统即将推出，敬请期待。</p>
          </div>
        </section>

        {/* ===== 关于 ===== */}
        <section className="settings-section">
          <h3 className="section-title">关于</h3>
          <div className="about-info">
            <p>Agent Desktop v0.1.0</p>
            <p className="form-hint">基于 Tauri v2 + React + Rust 构建</p>
          </div>
        </section>
      </div>
    </div>
  );
}
