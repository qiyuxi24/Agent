import { useState, useEffect, useCallback } from "react";
import { useTranslation } from "react-i18next";
import {
  PackageIcon, DownloadIcon, DeleteIcon,
  StoreIcon, SearchIcon, EyeIcon, ToggleLeftIcon,
  ToggleRightIcon, SparklesIcon, RefreshIcon, CheckIcon,
} from "../../components/Icons";

interface SkillInfo {
  id: string;
  name: string;
  description: string;
  description_zh: string;
  description_en: string;
  version: string;
  category: string;
  installed: boolean;
  enabled: boolean;
  size_bytes: number;
}

interface SkillMarketEntry {
  id: string;
  name: string;
  description_zh: string;
  description_en: string;
  version: string;
  category: string;
  download_url: string;
  size_bytes: number;
  installed: boolean;
  source: string;           // "clawhub" | "local"
  downloads: number;        // ClawHub 下载量
  stars: number;            // ClawHub 星标数
  external_install: boolean; // 需通过 clawhub CLI 安装
}

const CATEGORIES = ["all", "frontend", "backend", "ai", "mcp", "research", "tools", "general"] as const;
type CategoryFilter = (typeof CATEGORIES)[number];

export default function SkillsPanel() {
  const { t, i18n } = useTranslation();
  const lang = i18n.language;

  // 已安装 Skills
  const [installed, setInstalled] = useState<SkillInfo[]>([]);
  const [installedLoading, setInstalledLoading] = useState(true);

  // 市场 Skills
  const [market, setMarket] = useState<SkillMarketEntry[]>([]);
  const [marketLoading, setMarketLoading] = useState(true);
  const [marketError, setMarketError] = useState("");

  // UI 状态
  const [activeTab, setActiveTab] = useState<"installed" | "market">("installed");
  const [search, setSearch] = useState("");
  const [categoryFilter, setCategoryFilter] = useState<CategoryFilter>("all");
  const [installingId, setInstallingId] = useState<string | null>(null);
  const [previewId, setPreviewId] = useState<string | null>(null);
  const [previewContent, setPreviewContent] = useState("");
  const [showPromptPreview, setShowPromptPreview] = useState(false);
  const [promptPreviewContent, setPromptPreviewContent] = useState("");

  // 加载已安装列表
  const loadInstalled = useCallback(async () => {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const list = await invoke<SkillInfo[]>("skills_list");
      setInstalled(list);
    } catch (e) {
      console.error("[skills] 加载已安装列表失败:", e);
    } finally {
      setInstalledLoading(false);
    }
  }, []);

  // 加载市场列表
  const loadMarket = useCallback(async () => {
    setMarketLoading(true);
    setMarketError("");
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const entries = await invoke<SkillMarketEntry[]>("skills_market_list");
      // 标记已安装的
      const installedIds = new Set(installed.map((s) => s.id));
      const marked = entries.map((e) => ({ ...e, installed: installedIds.has(e.id) }));
      setMarket(marked);
    } catch (e) {
      setMarketError(typeof e === "string" ? e : (e as Error)?.message || "加载失败");
    } finally {
      setMarketLoading(false);
    }
  }, [installed]);

  useEffect(() => {
    loadInstalled();
  }, [loadInstalled]);

  useEffect(() => {
    if (activeTab === "market") {
      loadMarket();
    }
  }, [activeTab, loadMarket]);

  // 启用/禁用
  const toggleSkill = async (id: string, enabled: boolean) => {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("skills_toggle", { id, enabled });
      setInstalled((prev) =>
        prev.map((s) => (s.id === id ? { ...s, enabled } : s)),
      );
    } catch (e) {
      console.error("[skills] 切换失败:", e);
    }
  };

  // 预览注入的 system prompt
  const previewPrompt = async () => {
    if (showPromptPreview) {
      setShowPromptPreview(false);
      setPromptPreviewContent("");
      return;
    }
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const content = await invoke<string>("skills_preview_prompt");
      if (!content) {
        setPromptPreviewContent("（暂无启用的 Skill）");
      } else {
        setPromptPreviewContent(content);
      }
      setShowPromptPreview(true);
    } catch (e) {
      setPromptPreviewContent("无法加载: " + (typeof e === "string" ? e : ""));
      setShowPromptPreview(true);
    }
  };

  // 卸载
  const deleteSkill = async (id: string, name: string) => {
    if (!confirm(t("settings.skills.deleteConfirm", { name }))) return;
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("skills_delete", { id });
      setInstalled((prev) => prev.filter((s) => s.id !== id));
      setMarket((prev) =>
        prev.map((e) => (e.id === id ? { ...e, installed: false } : e)),
      );
    } catch (e) {
      console.error("[skills] 删除失败:", e);
    }
  };

  // 安装
  const installSkill = async (entry: SkillMarketEntry) => {
    setInstallingId(entry.id);
    try {
      const { invoke } = await import("@tauri-apps/api/core");

      if (entry.external_install) {
        // ClawHub 技能：尝试通过 CLI 安装
        try {
          const result = await invoke<string>("skills_clawhub_install", { id: entry.id });
          console.log("[skills] ClawHub install:", result);
        } catch (cliErr: any) {
          // CLI 不可用，复制安装命令到剪贴板
          const cmd = `npm i -g clawhub && clawhub login && clawhub skill install ${entry.id}`;
          try {
            await navigator.clipboard.writeText(cmd);
            alert(`${t("settings.skills.installFailed")}: ${cliErr}\n\n安装命令已复制到剪贴板:\n${cmd}`);
          } catch {
            alert(`${t("settings.skills.installFailed")}: ${cliErr}\n\n请在终端运行:\n${cmd}`);
          }
          setInstallingId(null);
          return;
        }
      } else {
        // 本地技能：直接下载 SKILL.md
        await invoke("skills_install", { id: entry.id, downloadUrl: entry.download_url });
      }

      setMarket((prev) =>
        prev.map((e) => (e.id === entry.id ? { ...e, installed: true } : e)),
      );
      await loadInstalled();
    } catch (e) {
      console.error("[skills] 安装失败:", e);
      alert(t("settings.skills.installFailed") + ": " + (typeof e === "string" ? e : (e as Error)?.message));
    } finally {
      setInstallingId(null);
    }
  };

  // 预览
  const previewSkill = async (id: string) => {
    if (previewId === id) {
      setPreviewId(null);
      setPreviewContent("");
      return;
    }
    setPreviewId(id);
    setPreviewContent("");
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const content = await invoke<string>("skills_read_content", { id });
      // 只显示前 500 字符
      setPreviewContent(content.slice(0, 500) + (content.length > 500 ? "\n\n..." : ""));
    } catch (e) {
      setPreviewContent("无法加载内容: " + (typeof e === "string" ? e : ""));
    }
  };

  // 格式化大小
  const fmtSize = (bytes: number) => {
    if (bytes === 0) return "";
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  };

  // 格式化下载量
  const fmtDownloads = (n: number) => {
    if (n >= 1000) return `${(n / 1000).toFixed(1)}k`;
    return String(n);
  };

  // 获取描述（根据语言）
  const desc = (item: { description_zh: string; description_en: string }) =>
    lang.startsWith("zh") ? item.description_zh : item.description_en;

  // 分类翻译
  const catLabel = (cat: string) =>
    t(`settings.skills.categories.${cat}`, cat);

  // 过滤市场条目
  const filteredMarket = market.filter((e) => {
    if (categoryFilter !== "all" && e.category !== categoryFilter) return false;
    if (search) {
      const q = search.toLowerCase();
      return (
        e.name.toLowerCase().includes(q) ||
        e.description_zh.toLowerCase().includes(q) ||
        e.description_en.toLowerCase().includes(q) ||
        e.id.toLowerCase().includes(q)
      );
    }
    return true;
  });

  // 过滤已安装
  const filteredInstalled = installed.filter((s) => {
    if (search) {
      const q = search.toLowerCase();
      return (
        s.name.toLowerCase().includes(q) ||
        s.description_zh.toLowerCase().includes(q) ||
        s.id.toLowerCase().includes(q)
      );
    }
    return true;
  });

  return (
    <section className="settings-panel">
      <div className="section-header skills-section-header">
        <div className="section-header-actions">
          <div className="skills-tab-bar">
          <button
            className={`skills-tab ${activeTab === "installed" ? "active" : ""}`}
            onClick={() => setActiveTab("installed")}
          >
            <PackageIcon size={14} />
            {t("settings.skills.installed")} ({installed.length})
          </button>
          <button
            className={`skills-tab ${activeTab === "market" ? "active" : ""}`}
            onClick={() => setActiveTab("market")}
          >
            <StoreIcon size={14} />
            {t("settings.skills.market")}
          </button>
        </div>
        </div>
      </div>

      <p className="panel-desc">
        {t("settings.skills.desc")}
        <button
          className="btn btn-sm btn-outline"
          onClick={previewPrompt}
          style={{ marginLeft: 8 }}
          title="查看启用 Skills 会注入的 System Prompt"
        >
          <EyeIcon size={12} /> {showPromptPreview ? "收起" : "查看注入内容"}
        </button>
      </p>

      {/* Skills 注入的 System Prompt 预览 */}
      {showPromptPreview && (
        <div className="skill-prompt-preview" style={{
          background: "var(--sidebar-bg)",
          border: "1px solid var(--border-color)",
          borderRadius: 8,
          padding: 12,
          marginBottom: 12,
          maxHeight: 300,
          overflow: "auto",
          fontSize: 12,
          fontFamily: "monospace",
          whiteSpace: "pre-wrap",
          lineHeight: 1.5,
          opacity: 0.85,
        }}>
          {promptPreviewContent || "加载中..."}
        </div>
      )}

      {/* 搜索栏 */}
      <div className="skills-search-bar">
        <SearchIcon size={14} className="skills-search-icon" />
        <input
          className="skills-search-input"
          placeholder={t("settings.skills.search")}
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
        {activeTab === "market" && (
          <div className="skills-cat-filters">
            {CATEGORIES.map((cat) => (
              <button
                key={cat}
                className={`skills-cat-btn ${categoryFilter === cat ? "active" : ""}`}
                onClick={() => setCategoryFilter(cat)}
              >
                {cat === "all" ? t("settings.skills.all") : catLabel(cat)}
              </button>
            ))}
          </div>
        )}
      </div>

      {/* ========== 已安装标签 ========== */}
      {activeTab === "installed" && (
        <div className="skills-list">
          {installedLoading ? (
            <div className="skills-loading">{t("app.loading")}</div>
          ) : filteredInstalled.length === 0 ? (
            <div className="placeholder-section">
              <p>{t("settings.skills.noInstalled")}</p>
            </div>
          ) : (
            filteredInstalled.map((skill) => (
              <div key={skill.id} className="skill-card">
                <div className="skill-card-main">
                  <div className="skill-card-icon">
                    <PackageIcon size={20} />
                  </div>
                  <div className="skill-card-info">
                    <div className="skill-card-head">
                      <span className="skill-card-name">{skill.name}</span>
                      <span className="skill-card-version">v{skill.version}</span>
                      <span className="skill-card-cat">{catLabel(skill.category)}</span>
                    </div>
                    <p className="skill-card-desc">
                      {lang.startsWith("zh") ? skill.description_zh : skill.description_en || skill.description}
                    </p>
                    <div className="skill-card-meta">
                      <span>{fmtSize(skill.size_bytes)}</span>
                      <span className={`skill-status ${skill.enabled ? "on" : "off"}`}>
                        {skill.enabled ? "● 已启用" : "○ 已禁用"}
                      </span>
                    </div>
                  </div>
                </div>
                <div className="skill-card-actions">
                  <button
                    className="btn btn-icon-sm"
                    onClick={() => previewSkill(skill.id)}
                    title={t("settings.skills.preview")}
                  >
                    <EyeIcon size={14} />
                  </button>
                  <button
                    className="btn btn-icon-sm"
                    onClick={() => toggleSkill(skill.id, !skill.enabled)}
                    title={skill.enabled ? t("settings.skills.disable") : t("settings.skills.enable")}
                  >
                    {skill.enabled ? <ToggleRightIcon size={16} /> : <ToggleLeftIcon size={16} />}
                  </button>
                  <button
                    className="btn btn-icon-sm btn-danger"
                    onClick={() => deleteSkill(skill.id, skill.name)}
                    title={t("settings.skills.uninstall")}
                  >
                    <DeleteIcon size={14} />
                  </button>
                </div>
                {/* 预览区 */}
                {previewId === skill.id && previewContent && (
                  <div className="skill-preview">
                    <pre className="skill-preview-code">{previewContent}</pre>
                  </div>
                )}
              </div>
            ))
          )}
        </div>
      )}

      {/* ========== 市场标签 ========== */}
      {activeTab === "market" && (
        <div className="skills-list">
          {marketLoading ? (
            <div className="skills-loading">{t("app.loading")}</div>
          ) : marketError ? (
            <div className="skills-error">
              <p>{marketError}</p>
              <button className="btn btn-secondary" onClick={loadMarket}>
                <RefreshIcon size={14} /> 重试
              </button>
            </div>
          ) : filteredMarket.length === 0 ? (
            <div className="placeholder-section">
              <p>{t("settings.skills.noMarket")}</p>
            </div>
          ) : (
            filteredMarket.map((entry) => (
              <div key={entry.id} className={`skill-card market ${entry.installed ? "installed" : ""}`}>
                <div className="skill-card-main">
                  <div className="skill-card-icon">
                    {entry.source === "clawhub" ? <StoreIcon size={20} /> : <SparklesIcon size={20} />}
                  </div>
                  <div className="skill-card-info">
                    <div className="skill-card-head">
                      <span className="skill-card-name">{entry.name}</span>
                      <span className="skill-card-version">v{entry.version}</span>
                      <span className="skill-card-cat">{catLabel(entry.category)}</span>
                      {entry.source === "clawhub" && (
                        <span className="skill-source-badge" title="来自 ClawHub 技能市场">
                          ClawHub
                        </span>
                      )}
                      {entry.installed && (
                        <span className="skill-installed-badge">
                          <CheckIcon size={12} /> 已安装
                        </span>
                      )}
                    </div>
                    <p className="skill-card-desc">{desc(entry)}</p>
                    <div className="skill-card-meta">
                      {entry.size_bytes > 0 && <span>{fmtSize(entry.size_bytes)}</span>}
                      {entry.downloads > 0 && (
                        <span title="下载量">
                          <DownloadIcon size={11} /> {fmtDownloads(entry.downloads)}
                        </span>
                      )}
                      {entry.stars > 0 && (
                        <span title="星标">★ {entry.stars}</span>
                      )}
                      {entry.external_install && !entry.installed && (
                        <span className="skill-external-hint" title="需通过 ClawHub CLI 安装">
                          CLI 安装
                        </span>
                      )}
                    </div>
                  </div>
                </div>
                <div className="skill-card-actions">
                  {entry.installed ? (
                    <span className="skill-installed-check">
                      <CheckIcon size={16} />
                    </span>
                  ) : (
                    <button
                      className="btn btn-primary btn-sm"
                      onClick={() => installSkill(entry)}
                      disabled={installingId === entry.id}
                    >
                      <DownloadIcon size={14} />
                      {installingId === entry.id
                        ? t("settings.skills.installing")
                        : t("settings.skills.install")}
                    </button>
                  )}
                </div>
              </div>
            ))
          )}
        </div>
      )}
    </section>
  );
}
