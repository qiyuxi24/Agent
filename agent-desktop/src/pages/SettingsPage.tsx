import { useState, useEffect, useCallback } from "react";
import {
  useAppStore,
  type ThemeMode,
  type ModelProvider,
  type ShortcutAction,
  SHORTCUT_LABELS,
  DEFAULT_SHORTCUTS,
} from "../stores/appStore";
import { useKeyCapture, formatCombo } from "../hooks/useKeyCapture";

interface SettingsPageProps {
  onClose?: () => void;
}

type SettingsSection = "general" | "models" | "shortcuts" | "plugins" | "about";

const menuItems: { id: SettingsSection; label: string; icon: React.ReactNode }[] = [
  {
    id: "general",
    label: "通用",
    icon: (
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
        <circle cx="12" cy="12" r="3" />
        <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
      </svg>
    ),
  },
  {
    id: "models",
    label: "AI 模型",
    icon: (
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
        <rect x="2" y="3" width="20" height="14" rx="2" ry="2" />
        <line x1="8" y1="21" x2="16" y2="21" />
        <line x1="12" y1="17" x2="12" y2="21" />
      </svg>
    ),
  },
  {
    id: "shortcuts",
    label: "快捷键",
    icon: (
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
        <rect x="2" y="4" width="20" height="16" rx="2" ry="2" />
        <path d="M6 8h.01" />
        <path d="M10 8h.01" />
        <path d="M14 8h.01" />
      </svg>
    ),
  },
  {
    id: "plugins",
    label: "插件",
    icon: (
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
        <path d="M12 2L2 7l10 5 10-5-10-5z" />
        <path d="M2 17l10 5 10-5" />
        <path d="M2 12l10 5 10-5" />
      </svg>
    ),
  },
  {
    id: "about",
    label: "关于",
    icon: (
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
        <circle cx="12" cy="12" r="10" />
        <line x1="12" y1="16" x2="12" y2="12" />
        <line x1="12" y1="8" x2="12.01" y2="8" />
      </svg>
    ),
  },
];

export default function SettingsPage({ onClose }: SettingsPageProps) {
  const [activeSection, setActiveSection] = useState<SettingsSection>("general");
  const {
    theme, setTheme,
    language, setLanguage,
    shortcuts, setShortcut,
    providers,
    addProvider,
    updateProvider,
    removeProvider,
    addModel,
    removeModel,
  } = useAppStore();

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

  // ---- 快捷键录制 ----
  const [capturingAction, setCapturingAction] = useState<ShortcutAction | null>(null);

  const handleCapture = useCallback(
    (keys: string[]) => {
      if (capturingAction) {
        setShortcut(capturingAction, keys);
        setCapturingAction(null);
      }
    },
    [capturingAction, setShortcut],
  );

  const capture = useKeyCapture(handleCapture);

  // 挂载/卸载录制的事件监听
  useEffect(() => {
    if (capture.listening) {
      document.addEventListener("keydown", capture.handleKeyDown, true);
      document.addEventListener("keyup", capture.handleKeyUp, true);
      return () => {
        document.removeEventListener("keydown", capture.handleKeyDown, true);
        document.removeEventListener("keyup", capture.handleKeyUp, true);
      };
    }
  }, [capture.listening, capture.handleKeyDown, capture.handleKeyUp]);

  const startRebind = (action: ShortcutAction) => {
    setCapturingAction(action);
    capture.startCapture();
  };

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

  const renderGeneral = () => (
    <section className="settings-panel">
      <h3 className="panel-title">通用</h3>
      <div className="form-group">
        <label>语言</label>
        <select value={language} onChange={(e) => setLanguage(e.target.value)} className="form-select">
          {langOptions.map((opt) => (
            <option key={opt.value} value={opt.value}>{opt.label}</option>
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
  );

  const renderModels = () => (
    <section className="settings-panel">
      <div className="section-header">
        <h3 className="panel-title">AI 模型</h3>
        <button className="btn btn-add-provider" onClick={() => setShowAddForm(!showAddForm)}>
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <line x1="12" y1="5" x2="12" y2="19" />
            <line x1="5" y1="12" x2="19" y2="12" />
          </svg>
          添加
        </button>
      </div>

      {showAddForm && (
        <div className="provider-form">
          <div className="form-group">
            <label>名称</label>
            <input type="text" value={newName} onChange={(e) => setNewName(e.target.value)} placeholder="例如：OpenAI、DeepSeek" />
          </div>
          <div className="form-group">
            <label>API 地址</label>
            <input type="text" value={newBase} onChange={(e) => setNewBase(e.target.value)} placeholder="https://api.openai.com/v1" />
          </div>
          <div className="form-group">
            <label>API Key</label>
            <input type="password" value={newKey} onChange={(e) => setNewKey(e.target.value)} placeholder="sk-..." />
          </div>
          <div className="form-group">
            <label>模型列表（逗号分隔）</label>
            <input type="text" value={newModels} onChange={(e) => setNewModels(e.target.value)} placeholder="gpt-4o, gpt-4o-mini" />
            <span className="form-hint">用英文逗号分隔多个模型名称</span>
          </div>
          <div className="form-actions">
            <button className="btn btn-primary" onClick={handleAddProvider}>确认添加</button>
            <button className="btn btn-cancel" onClick={() => setShowAddForm(false)}>取消</button>
          </div>
        </div>
      )}

      <div className="provider-list">
        {providers.length === 0 ? (
          <div className="placeholder-section">
            <p>尚未添加模型，点击上方"添加"按钮开始配置。</p>
          </div>
        ) : (
          providers.map((p) => (
            <div key={p.id} className="provider-card">
              {editingId === p.id ? (
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
                    <input type="password" value={editKey} onChange={(e) => setEditKey(e.target.value)} />
                  </div>
                  <div className="form-actions">
                    <button className="btn btn-primary" onClick={() => saveEdit(p.id)}>保存</button>
                    <button className="btn btn-cancel" onClick={() => setEditingId(null)}>取消</button>
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
                      <button className="btn btn-icon-sm" onClick={() => startEdit(p)} title="编辑">
                        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                          <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7" />
                          <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z" />
                        </svg>
                      </button>
                      <button className="btn btn-icon-sm btn-danger" onClick={() => removeProvider(p.id)} title="删除">
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
                          <button className="model-tag-remove" onClick={() => removeModel(p.id, m)} title={`移除 ${m}`}>×</button>
                        </span>
                      ))}
                      <div className="model-add-inline">
                        <input
                          className="model-add-input"
                          placeholder="添加模型..."
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

  const renderShortcuts = () => (
    <section className="settings-panel">
      <h3 className="panel-title">快捷键</h3>
      <div className="shortcut-list">
        {(Object.keys(shortcuts) as ShortcutAction[]).map((action) => {
          const binding = shortcuts[action];
          const isCapturing = capturingAction === action && capture.listening;
          const displayKeys = isCapturing
            ? capture.currentKeys
            : binding.keys;

          return (
            <div
              key={action}
              className={`shortcut-item ${isCapturing ? "shortcut-recording" : ""}`}
              onClick={() => {
                if (!capture.listening) startRebind(action);
              }}
              role="button"
              tabIndex={0}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  if (!capture.listening) startRebind(action);
                }
              }}
            >
              <span className="shortcut-label">{SHORTCUT_LABELS[action]}</span>
              <div className="shortcut-keys">
                {isCapturing ? (
                  <>
                    <span className="shortcut-recording-hint">按下快捷键...</span>
                    {displayKeys.length > 0 && (
                      <kbd className="shortcut-kbd recording">{formatCombo(displayKeys)}</kbd>
                    )}
                  </>
                ) : displayKeys.length > 0 ? (
                  <kbd className="shortcut-kbd">{formatCombo(displayKeys)}</kbd>
                ) : (
                  <span className="shortcut-empty">点击设置</span>
                )}
              </div>
              {/* 重置按钮 */}
              {!isCapturing && binding.keys.length > 0 && (
                <button
                  className="shortcut-reset-btn"
                  title="恢复默认"
                  onClick={(e) => {
                    e.stopPropagation();
                    setShortcut(action, DEFAULT_SHORTCUTS[action].keys);
                  }}
                >
                  <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <polyline points="1 4 1 10 7 10" />
                    <path d="M3.51 15a9 9 0 1 0 2.13-9.36L1 10" />
                  </svg>
                </button>
              )}
            </div>
          );
        })}
      </div>
      <p className="form-hint" style={{ marginTop: "12px" }}>
        点击快捷键 → 按下组合键 → 松开自动保存。最多支持 3 个键。按 Esc 取消。
      </p>
    </section>
  );

  const renderPlugins = () => (
    <section className="settings-panel">
      <h3 className="panel-title">插件</h3>
      <div className="placeholder-section">
        <p>插件系统即将推出，敬请期待。</p>
      </div>
    </section>
  );

  const renderAbout = () => (
    <section className="settings-panel">
      <h3 className="panel-title">关于</h3>
      <div className="about-info">
        <p>Agent Desktop v0.1.0</p>
        <p className="form-hint">基于 Tauri v2 + React + Rust 构建</p>
      </div>
    </section>
  );

  const panels: Record<SettingsSection, React.ReactNode> = {
    general: renderGeneral(),
    models: renderModels(),
    shortcuts: renderShortcuts(),
    plugins: renderPlugins(),
    about: renderAbout(),
  };

  return (
    <div className="settings-overlay" onClick={(e) => { if (e.target === e.currentTarget) onClose?.(); }}>
      <div className="settings-layout">
        {/* 左侧导航 */}
        <aside className="settings-sidebar">
          <div className="settings-sidebar-header">
            <h2>设置</h2>
          </div>
          <nav className="settings-nav">
            {menuItems.map((item) => (
              <button
                key={item.id}
                className={`settings-nav-item ${activeSection === item.id ? "active" : ""}`}
                onClick={() => setActiveSection(item.id)}
              >
                {item.icon}
                <span>{item.label}</span>
              </button>
            ))}
          </nav>
        </aside>

        {/* 右侧内容 */}
        <div className="settings-content">
          <div className="settings-content-header">
            <h2>{menuItems.find((i) => i.id === activeSection)?.label}</h2>
            {onClose && (
              <button className="btn btn-icon settings-close-btn" onClick={onClose} title="返回对话">
                <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <line x1="18" y1="6" x2="6" y2="18" />
                  <line x1="6" y1="6" x2="18" y2="18" />
                </svg>
              </button>
            )}
          </div>
          <div className="settings-content-body">
            {panels[activeSection]}
          </div>
        </div>
      </div>
    </div>
  );
}
