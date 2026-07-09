import { useState, useEffect, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import {
  ExtensionIcon,
  DownloadIcon,
  StoreIcon,
  SearchIcon,
  FolderIcon,
  TrashIcon,
  CheckIcon,
  XIcon,
} from "../../components/Icons";

/* ===== 类型 ===== */

interface PluginMeta {
  id: string;
  name: string;
  version: string;
  author: string;
  description: string;
  category: string;
  /** 插件入口脚本路径 */
  entry?: string;
  /** 贡献点：panel / command / theme */
  contributes?: string[];
}

interface InstalledPlugin {
  id: string;
  name: string;
  version: string;
  author: string;
  description: string;
  category: string;
  enabled: boolean;
  installedAt?: string;
}

type PluginTab = "installed" | "market";

/* ===== 内置推荐插件（后续对接真实 API） ===== */

const MARKET_PLUGINS: PluginMeta[] = [
  {
    id: "theme-dark-pro",
    name: "Dark Pro Theme",
    version: "1.2.0",
    author: "Agent Team",
    description: "专业深色主题，优化代码高亮与对比度。",
    category: "theme",
    contributes: ["theme"],
  },
  {
    id: "markdown-preview",
    name: "Markdown Preview",
    version: "0.8.1",
    author: "Agent Team",
    description: "侧边栏实时 Markdown 预览，支持 GFM 语法与 Mermaid 图表。",
    category: "tool",
    contributes: ["panel"],
  },
  {
    id: "github-copilot-chat",
    name: "GitHub Copilot Chat",
    version: "1.0.0",
    author: "GitHub",
    description: "接入 GitHub Copilot 对话服务，作为备选 AI 后端。",
    category: "integration",
    contributes: ["command"],
  },
  {
    id: "todo-panel",
    name: "Todo Panel",
    version: "0.5.0",
    author: "Agent Team",
    description: "内置任务管理面板，支持看板视图与 AI 自动拆解任务。",
    category: "productivity",
    contributes: ["panel"],
  },
  {
    id: "chart-visualizer",
    name: "Chart Visualizer",
    version: "0.3.2",
    author: "Agent Team",
    description: "在对话中直接渲染 ECharts 图表，支持折线/柱状/饼图。",
    category: "visualization",
    contributes: ["panel"],
  },
  {
    id: "obsidian-sync",
    name: "Obsidian Sync",
    version: "0.2.0",
    author: "Community",
    description: "双向同步 Obsidian 笔记库，对话内容自动归档为 Markdown。",
    category: "integration",
    contributes: ["command"],
  },
];

/* ===== 组件 ===== */

export default function PluginsPanel() {
  const { t } = useTranslation();

  const [tab, setTab] = useState<PluginTab>("installed");
  const [installed, setInstalled] = useState<InstalledPlugin[]>([]);
  const [search, setSearch] = useState("");
  const [catFilter, setCatFilter] = useState("all");
  const [installing, setInstalling] = useState<string | null>(null);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  /* ---- 加载已安装插件 ---- */
  const loadInstalled = async () => {
    try {
      setLoading(true);
      setError("");
      const list: InstalledPlugin[] = await invoke("plugins_list");
      setInstalled(list);
    } catch (e: any) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadInstalled();
  }, []);

  /* ---- 已安装 ID 集合 ---- */
  const installedIds = useMemo(() => new Set(installed.map((p) => p.id)), [installed]);

  /* ---- 分类列表 ---- */
  const categories = useMemo(() => {
    const cats = new Set<string>();
    [...installed, ...MARKET_PLUGINS].forEach((p) => cats.add(p.category));
    return Array.from(cats);
  }, [installed]);

  const catLabel = (cat: string) => t(`settings.plugins.categories.${cat}`, cat);

  /* ---- 安装 ---- */
  const handleInstall = async (plugin: PluginMeta) => {
    try {
      setInstalling(plugin.id);
      setError("");
      await invoke("plugins_install", { pluginId: plugin.id });
      await loadInstalled();
    } catch (e: any) {
      setError(String(e));
    } finally {
      setInstalling(null);
    }
  };

  /* ---- 卸载 ---- */
  const handleUninstall = async (plugin: InstalledPlugin) => {
    const ok = window.confirm(t("settings.plugins.deleteConfirm", { name: plugin.name }));
    if (!ok) return;
    try {
      setError("");
      await invoke("plugins_delete", { pluginId: plugin.id });
      setInstalled((prev) => prev.filter((p) => p.id !== plugin.id));
    } catch (e: any) {
      setError(String(e));
    }
  };

  /* ---- 启用/禁用 ---- */
  const handleToggle = async (plugin: InstalledPlugin) => {
    try {
      setError("");
      await invoke("plugins_toggle", { pluginId: plugin.id, enabled: !plugin.enabled });
      setInstalled((prev) =>
        prev.map((p) => (p.id === plugin.id ? { ...p, enabled: !p.enabled } : p))
      );
    } catch (e: any) {
      setError(String(e));
    }
  };

  /* ---- 筛选 ---- */
  const filteredMarket = useMemo(() => {
    return MARKET_PLUGINS.filter((p) => {
      if (catFilter !== "all" && p.category !== catFilter) return false;
      if (search && !p.name.toLowerCase().includes(search.toLowerCase()) && !p.description.toLowerCase().includes(search.toLowerCase()))
        return false;
      return true;
    });
  }, [search, catFilter]);

  const filteredInstalled = useMemo(() => {
    return installed.filter((p) => {
      if (catFilter !== "all" && p.category !== catFilter) return false;
      if (search && !p.name.toLowerCase().includes(search.toLowerCase()))
        return false;
      return true;
    });
  }, [installed, search, catFilter]);

  /* ===== 渲染 ===== */

  return (
    <div className="plugins-panel">
      {/* 说明 */}
      <p className="settings-desc">{t("settings.plugins.desc")}</p>

      {/* Tab 切换 */}
      <div className="skills-tab-bar" style={{ marginBottom: 16 }}>
        <button
          className={`skills-tab ${tab === "installed" ? "active" : ""}`}
          onClick={() => setTab("installed")}
        >
          <FolderIcon size={14} />
          {t("settings.plugins.installed")}
          {installed.length > 0 && (
            <span className="badge-count">{installed.length}</span>
          )}
        </button>
        <button
          className={`skills-tab ${tab === "market" ? "active" : ""}`}
          onClick={() => setTab("market")}
        >
          <StoreIcon size={14} />
          {t("settings.plugins.market")}
        </button>
      </div>

      {/* 搜索 + 分类 */}
      <div className="skills-search-bar">
        <SearchIcon size={14} className="skills-search-icon" />
        <input
          className="skills-search-input"
          type="text"
          placeholder={t("settings.plugins.search")}
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
      </div>

      <div className="skills-cat-filters" style={{ marginBottom: 16 }}>
        <button
          className={`skills-cat-btn ${catFilter === "all" ? "active" : ""}`}
          onClick={() => setCatFilter("all")}
        >
          {t("settings.plugins.all")}
        </button>
        {categories.map((cat) => (
          <button
            key={cat}
            className={`skills-cat-btn ${catFilter === cat ? "active" : ""}`}
            onClick={() => setCatFilter(cat)}
          >
            {catLabel(cat)}
          </button>
        ))}
      </div>

      {/* 错误提示 */}
      {error && <div className="skills-error">{error}</div>}

      {/* 已安装列表 */}
      {tab === "installed" && (
        <>
          {loading && <div className="skills-loading">加载中...</div>}
          {!loading && filteredInstalled.length === 0 && (
            <div className="skills-loading">{t("settings.plugins.noInstalled")}</div>
          )}
          <div className="skills-list">
            {filteredInstalled.map((p) => (
              <div key={p.id} className="skill-card">
                <div className="skill-card-main">
                  <div className="skill-card-icon">
                    <ExtensionIcon size={18} />
                  </div>
                  <div className="skill-card-info">
                    <div className="skill-card-head">
                      <span className="skill-card-name">{p.name}</span>
                      <span className="skill-card-version">v{p.version}</span>
                      <span className="skill-card-cat">{catLabel(p.category)}</span>
                      {p.enabled ? (
                        <span className="skill-status on">● 已启用</span>
                      ) : (
                        <span className="skill-status off">○ 已禁用</span>
                      )}
                    </div>
                    <p className="skill-card-desc">{p.description}</p>
                    <div className="skill-card-meta">
                      <span>{t("settings.plugins.author")}: {p.author}</span>
                    </div>
                  </div>
                </div>
                <div className="skill-card-actions">
                  <button
                    className="btn btn-sm btn-outline"
                    onClick={() => handleToggle(p)}
                  >
                    {p.enabled ? t("settings.plugins.disable") : t("settings.plugins.enable")}
                  </button>
                  <button
                    className="btn btn-sm btn-outline"
                    onClick={() => handleUninstall(p)}
                    style={{ color: "var(--danger, #e53e3e)", borderColor: "var(--danger, #e53e3e)" }}
                  >
                    <TrashIcon size={14} />
                    {t("settings.plugins.uninstall")}
                  </button>
                </div>
              </div>
            ))}
          </div>
        </>
      )}

      {/* 市场列表 */}
      {tab === "market" && (
        <>
          {filteredMarket.length === 0 && (
            <div className="skills-loading">{t("settings.plugins.noMarket")}</div>
          )}
          <div className="skills-list">
            {filteredMarket.map((p) => {
              const isInstalled = installedIds.has(p.id);
              return (
                <div key={p.id} className={`skill-card market ${isInstalled ? "installed" : ""}`}>
                  <div className="skill-card-main">
                    <div className="skill-card-icon">
                      <ExtensionIcon size={18} />
                    </div>
                    <div className="skill-card-info">
                      <div className="skill-card-head">
                        <span className="skill-card-name">{p.name}</span>
                        <span className="skill-card-version">v{p.version}</span>
                        <span className="skill-card-cat">{catLabel(p.category)}</span>
                        {isInstalled && (
                          <span className="skill-installed-badge">
                            <CheckIcon size={10} />
                            已安装
                          </span>
                        )}
                      </div>
                      <p className="skill-card-desc">{p.description}</p>
                      <div className="skill-card-meta">
                        <span>{t("settings.plugins.author")}: {p.author}</span>
                        {p.contributes && p.contributes.length > 0 && (
                          <span>贡献: {p.contributes.join(", ")}</span>
                        )}
                      </div>
                    </div>
                  </div>
                  <div className="skill-card-actions">
                    {isInstalled ? (
                      <span className="skill-installed-check">
                        <CheckIcon size={16} />
                      </span>
                    ) : (
                      <button
                        className="btn btn-sm btn-primary"
                        onClick={() => handleInstall(p)}
                        disabled={installing === p.id}
                      >
                        <DownloadIcon size={14} />
                        {installing === p.id ? t("settings.plugins.installing") : t("settings.plugins.install")}
                      </button>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        </>
      )}

      {/* Badge 计数样式 */}
      <style>{`
        .badge-count {
          display: inline-flex;
          align-items: center;
          justify-content: center;
          min-width: 18px;
          height: 18px;
          padding: 0 5px;
          font-size: 11px;
          font-weight: 600;
          color: #fff;
          background: var(--accent);
          border-radius: 9px;
          line-height: 1;
        }
        .btn-sm {
          padding: 4px 10px;
          font-size: 12px;
          border-radius: 6px;
          display: inline-flex;
          align-items: center;
          gap: 4px;
        }
        .btn-outline {
          background: transparent;
          border: 1px solid var(--border-color);
          color: var(--text-primary);
          cursor: pointer;
        }
        .btn-outline:hover {
          border-color: var(--accent);
          color: var(--accent);
        }
        .btn-primary {
          background: var(--accent);
          border: 1px solid var(--accent);
          color: #fff;
          cursor: pointer;
        }
        .btn-primary:hover {
          opacity: .85;
        }
        .btn-primary:disabled {
          opacity: .5;
          cursor: not-allowed;
        }
      `}</style>
    </div>
  );
}
