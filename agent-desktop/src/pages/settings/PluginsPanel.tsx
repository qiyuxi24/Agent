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
  RefreshIcon,
} from "../../components/Icons";

/* ===== 类型 ===== */

interface InstalledPlugin {
  id: string;
  name: string;
  version: string;
  author: string;
  description: string;
  category: string;
  enabled: boolean;
  installedAt?: string;
  entry?: string;
  contributes?: string[];
}

/** 市场条目（从后端动态抓取） */
interface PluginMarketEntry {
  id: string;
  name: string;
  version: string;
  author: string;
  description: string;
  description_zh: string;
  category: string;
  stars: number;
  source: string;
  download_url?: string;
  homepage?: string;
  contributes: string[];
}

type PluginTab = "installed" | "market";

/* ===== 组件 ===== */

export default function PluginsPanel() {
  const { t } = useTranslation();

  const [tab, setTab] = useState<PluginTab>("installed");
  const [installed, setInstalled] = useState<InstalledPlugin[]>([]);
  const [market, setMarket] = useState<PluginMarketEntry[]>([]);
  const [marketLoading, setMarketLoading] = useState(false);
  const [marketError, setMarketError] = useState("");
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

  /* ---- 加载市场数据 ---- */
  const loadMarket = async () => {
    try {
      setMarketLoading(true);
      setMarketError("");
      const list: PluginMarketEntry[] = await invoke("plugin_market_list");
      setMarket(list);
    } catch (e: any) {
      setMarketError(String(e));
    } finally {
      setMarketLoading(false);
    }
  };

  useEffect(() => {
    loadInstalled();
  }, []);

  // 切换到市场 Tab 时加载数据
  useEffect(() => {
    if (tab === "market" && market.length === 0 && !marketLoading) {
      loadMarket();
    }
  }, [tab]);

  /* ---- 已安装 ID 集合 ---- */
  const installedIds = useMemo(() => new Set(installed.map((p) => p.id)), [installed]);

  /* ---- 分类列表 ---- */
  const categories = useMemo(() => {
    const cats = new Set<string>();
    installed.forEach((p) => cats.add(p.category));
    market.forEach((p) => cats.add(p.category));
    return Array.from(cats);
  }, [installed, market]);

  const catLabel = (cat: string) => t(`settings.plugins.categories.${cat}`, cat);

  /* ---- 安装 ---- */
  const handleInstall = async (entry: PluginMarketEntry) => {
    try {
      setInstalling(entry.id);
      setError("");
      await invoke("plugins_install", {
        pluginId: entry.id,
        downloadUrl: entry.download_url ?? null,
        pluginName: entry.name,
        pluginVersion: entry.version,
        pluginAuthor: entry.author,
        pluginDescription: entry.description,
        pluginCategory: entry.category,
        pluginContributes: entry.contributes.length > 0 ? entry.contributes : null,
      });
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
    return market.filter((p) => {
      if (catFilter !== "all" && p.category !== catFilter) return false;
      const s = search.toLowerCase();
      if (s && !p.name.toLowerCase().includes(s) &&
          !p.description.toLowerCase().includes(s) &&
          !p.description_zh.toLowerCase().includes(s))
        return false;
      return true;
    });
  }, [market, search, catFilter]);

  const filteredInstalled = useMemo(() => {
    return installed.filter((p) => {
      if (catFilter !== "all" && p.category !== catFilter) return false;
      if (search && !p.name.toLowerCase().includes(search.toLowerCase()))
        return false;
      return true;
    });
  }, [installed, search, catFilter]);

  /* ---- 星标格式化 ---- */
  const formatStars = (n: number): string => {
    if (n >= 1000) return (n / 1000).toFixed(1) + "k";
    return String(n);
  };

  /* ---- 来源标识 ---- */
  const sourceLabel = (source: string): string => {
    switch (source) {
      case "npm": return "npm";
      case "github": return "GitHub";
      case "builtin": return t("settings.plugins.builtin", "内置");
      default: return source;
    }
  };

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
                      {p.contributes && p.contributes.length > 0 && (
                        <span>贡献: {p.contributes.join(", ")}</span>
                      )}
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
          {/* 加载与错误状态 */}
          {marketLoading && <div className="skills-loading">加载市场中...</div>}
          {marketError && (
            <div className="skills-error">
              {t("settings.plugins.marketError", "市场加载失败")}: {marketError}
              <button
                className="btn btn-sm btn-outline"
                style={{ marginLeft: 8 }}
                onClick={loadMarket}
              >
                <RefreshIcon size={12} />
                重试
              </button>
            </div>
          )}

          {!marketLoading && !marketError && filteredMarket.length === 0 && (
            <div className="skills-loading">{t("settings.plugins.noMarket")}</div>
          )}

          <div className="skills-list">
            {filteredMarket.map((p) => {
              const isInstalled = installedIds.has(p.id);
              const displayName = useDisplayName(p);
              const displayDesc = useDisplayDesc(p);
              return (
                <div key={p.id} className={`skill-card market ${isInstalled ? "installed" : ""}`}>
                  <div className="skill-card-main">
                    <div className="skill-card-icon">
                      <ExtensionIcon size={18} />
                    </div>
                    <div className="skill-card-info">
                      <div className="skill-card-head">
                        <span className="skill-card-name">{displayName}</span>
                        <span className="skill-card-version">v{p.version}</span>
                        <span className="skill-card-cat">{catLabel(p.category)}</span>
                        {p.stars > 0 && (
                          <span className="skill-stars">★ {formatStars(p.stars)}</span>
                        )}
                        <span className="skill-source-tag">{sourceLabel(p.source)}</span>
                        {isInstalled && (
                          <span className="skill-installed-badge">
                            <CheckIcon size={10} />
                            已安装
                          </span>
                        )}
                      </div>
                      <p className="skill-card-desc">{displayDesc}</p>
                      <div className="skill-card-meta">
                        <span>{t("settings.plugins.author")}: {p.author}</span>
                        {p.contributes && p.contributes.length > 0 && (
                          <span>贡献: {p.contributes.join(", ")}</span>
                        )}
                        {p.homepage && (
                          <a
                            href={p.homepage}
                            target="_blank"
                            rel="noopener noreferrer"
                            className="skill-homepage-link"
                            onClick={(e) => e.stopPropagation()}
                          >
                            主页 ↗
                          </a>
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
        .skill-stars {
          font-size: 11px;
          color: #e3b341;
          font-weight: 600;
        }
        .skill-source-tag {
          font-size: 10px;
          padding: 1px 6px;
          border-radius: 8px;
          background: var(--bg-tertiary);
          color: var(--text-secondary);
          text-transform: uppercase;
        }
        .skill-homepage-link {
          font-size: 12px;
          color: var(--accent);
          text-decoration: none;
        }
        .skill-homepage-link:hover {
          text-decoration: underline;
        }
      `}</style>
    </div>
  );
}

/** 根据可用字段选择显示名称（npm 包名可能很长，优先用短名称） */
function useDisplayName(entry: PluginMarketEntry): string {
  // 如果 name 包含 @scope/，用短格式或 id
  if (entry.name.startsWith("@") && entry.name.includes("/")) {
    const short = entry.name.split("/").pop() || entry.name;
    if (short.length > 25) return short.substring(0, 22) + "...";
    return short;
  }
  if (entry.name.length > 30) return entry.name.substring(0, 27) + "...";
  return entry.name;
}

/** 优先显示中文描述，回退到英文 */
function useDisplayDesc(entry: PluginMarketEntry): string {
  if (entry.description_zh && entry.description_zh.trim().length > 0) {
    return entry.description_zh;
  }
  return entry.description;
}
