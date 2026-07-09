import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { useAppStore, type McpServerUI } from "../../stores/appStore";
import { PlusIcon, DeleteIcon, ToolIcon, RefreshIcon, DownloadIcon, StoreIcon } from "../../components/Icons";

interface ServerInfo {
  name: string;
  status: string;
  tool_count: number;
}

interface ToolInfo {
  name: string;
  description?: string;
}

/** MCP 市场推荐条目 */
interface McpRecommend {
  name: string;
  description_zh: string;
  description_en: string;
  command: string;
  args: string;
  category: string;
}

// 内置 MCP 推荐列表（常用开源 MCP 服务器）
const MCP_RECOMMENDATIONS: McpRecommend[] = [
  {
    name: "filesystem",
    description_zh: "文件系统操作：读写文件、创建目录、搜索文件。让 AI 直接操作你的项目文件。",
    description_en: "File system operations: read/write files, create dirs, search files.",
    command: "npx",
    args: "-y @modelcontextprotocol/server-filesystem .",
    category: "tools",
  },
  {
    name: "github",
    description_zh: "GitHub API 集成：管理仓库、Issue、PR、搜索代码。需要 GitHub Personal Access Token。",
    description_en: "GitHub API: manage repos, issues, PRs, search code. Needs PAT.",
    command: "npx",
    args: "-y @modelcontextprotocol/server-github",
    category: "tools",
  },
  {
    name: "brave-search",
    description_zh: "Brave 搜索引擎：联网搜索网页、新闻。需要 Brave Search API Key（免费额度）。",
    description_en: "Brave Search: web & news search. Needs Brave Search API Key.",
    command: "npx",
    args: "-y @modelcontextprotocol/server-brave-search",
    category: "search",
  },
  {
    name: "postgres",
    description_zh: "PostgreSQL 数据库：SQL 查询、表结构查看。需要数据库连接字符串。",
    description_en: "PostgreSQL: SQL queries, schema inspection. Needs connection string.",
    command: "npx",
    args: "-y @modelcontextprotocol/server-postgres",
    category: "database",
  },
  {
    name: "puppeteer",
    description_zh: "Puppeteer 浏览器自动化：网页截图、内容抓取、表单操作。",
    description_en: "Puppeteer: browser automation, screenshots, scraping, form interaction.",
    command: "npx",
    args: "-y @modelcontextprotocol/server-puppeteer",
    category: "browser",
  },
];

export default function ToolsPanel() {
  const { t, i18n } = useTranslation();
  const lang = i18n.language;
  const mcpServers = useAppStore((s) => s.mcpServers);
  const addMcpServer = useAppStore((s) => s.addMcpServer);
  const removeMcpServer = useAppStore((s) => s.removeMcpServer);

  const [servers, setServers] = useState<ServerInfo[]>([]);
  const [tools, setTools] = useState<ToolInfo[]>([]);
  const [name, setName] = useState("");
  const [command, setCommand] = useState("");
  const [args, setArgs] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [showMarket, setShowMarket] = useState(false);

  const refresh = async () => {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const s = await invoke<ServerInfo[]>("mcp_list_servers");
      setServers(s);
      const tl = await invoke<ToolInfo[]>("mcp_list_tools");
      setTools(tl);
    } catch {
      // 非 Tauri 环境忽略
    }
  };

  useEffect(() => {
    refresh();
  }, []);

  // 定时刷新服务器状态（每 15 秒）
  useEffect(() => {
    const interval = setInterval(refresh, 15000);
    return () => clearInterval(interval);
  }, []);

  const connect = async (cfg: { name: string; command: string; args: string[]; env?: Record<string, string> }) => {
    setBusy(true);
    setError("");
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("mcp_connect", {
        config: { name: cfg.name, command: cfg.command, args: cfg.args, env: cfg.env || null },
      });
      addMcpServer({ name: cfg.name, command: cfg.command, args: cfg.args, env: cfg.env });
      setName("");
      setCommand("");
      setArgs("");
      await refresh();
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : (e as Error)?.message || String(e));
    } finally {
      setBusy(false);
    }
  };

  const handleConnect = () => {
    if (!name.trim() || !command.trim()) {
      setError(t("settings.tools.needFields"));
      return;
    }
    const argList = args.split(/\s+/).map((a) => a.trim()).filter(Boolean);
    connect({ name: name.trim(), command: command.trim(), args: argList });
  };

  // 一键安装推荐 MCP 服务器
  const quickInstall = async (rec: McpRecommend) => {
    setBusy(true);
    setError("");
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const argList = rec.args.split(/\s+/).filter(Boolean);
      await invoke("mcp_connect", {
        config: { name: rec.name, command: rec.command, args: argList, env: null },
      });
      addMcpServer({ name: rec.name, command: rec.command, args: argList });
      await refresh();
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : (e as Error)?.message || String(e));
    } finally {
      setBusy(false);
    }
  };

  const handleDisconnect = async (n: string) => {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("mcp_disconnect", { name: n });
      await refresh();
    } catch {
      // ignore
    }
  };

  const connectedNames = new Set(servers.map((s) => s.name));
  const installedNames = new Set(mcpServers.map((s) => s.name));

  // 分组工具（按服务器前缀分组）
  const toolsByServer: Record<string, ToolInfo[]> = {};
  for (const tl of tools) {
    const parts = tl.name.split("::");
    const server = parts.length > 1 ? parts[0] : "unknown";
    if (!toolsByServer[server]) toolsByServer[server] = [];
    toolsByServer[server].push(tl);
  }

  return (
    <section className="settings-panel">
      <div className="section-header">
        <h3 className="panel-title">{t("settings.tools.title")}</h3>
        <div className="mcp-header-actions">
          <button
            className={`btn btn-sm ${showMarket ? "btn-secondary" : "btn-outline"}`}
            onClick={() => setShowMarket(!showMarket)}
          >
            <StoreIcon size={14} />
            {showMarket ? "收起市场" : "MCP 市场"}
          </button>
          <button className="btn btn-icon-sm" onClick={refresh} title="刷新">
            <RefreshIcon size={14} />
          </button>
        </div>
      </div>

      <p className="panel-desc">{t("settings.tools.desc")}</p>

      {/* MCP 市场（可折叠） */}
      {showMarket && (
        <div className="mcp-market-section">
          <h4 className="mcp-market-title">
            <StoreIcon size={14} /> 推荐 MCP 服务器
          </h4>
          <div className="mcp-market-list">
            {MCP_RECOMMENDATIONS.map((rec) => {
              const alreadyInstalled = installedNames.has(rec.name);
              const connected = connectedNames.has(rec.name);
              return (
                <div key={rec.name} className={`mcp-market-card ${connected ? "connected" : ""}`}>
                  <div className="mcp-market-info">
                    <span className="mcp-market-name">{rec.name}</span>
                    <span className="skill-card-cat">{rec.category}</span>
                    {connected && <span className="skill-status on">● 已连接</span>}
                    <p className="mcp-market-desc">
                      {lang.startsWith("zh") ? rec.description_zh : rec.description_en}
                    </p>
                  </div>
                  <div className="mcp-market-actions">
                    {connected ? (
                      <button
                        className="btn btn-sm btn-secondary"
                        onClick={() => handleDisconnect(rec.name)}
                      >
                        断开
                      </button>
                    ) : (
                      <button
                        className="btn btn-sm btn-primary"
                        onClick={() => quickInstall(rec)}
                        disabled={busy || alreadyInstalled}
                      >
                        <DownloadIcon size={12} />
                        {alreadyInstalled ? "已添加" : "一键安装"}
                      </button>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      )}

      {/* 已配置服务器 */}
      <div className="mcp-server-list">
        {mcpServers.length === 0 ? (
          <div className="placeholder-section">
            <p>{t("settings.tools.empty")}</p>
          </div>
        ) : (
          mcpServers.map((s: McpServerUI) => {
            const connected = connectedNames.has(s.name);
            const serverInfo = servers.find((si) => si.name === s.name);
            const toolCount = serverInfo?.tool_count ?? 0;
            return (
              <div
                key={s.name}
                className={`mcp-server-card ${connected ? "connected" : "offline"}`}
              >
                <div className="mcp-server-head">
                  <div className="mcp-server-info">
                    <span className="mcp-server-name">{s.name}</span>
                    <span className={`mcp-status ${connected ? "on" : "off"}`}>
                      {connected ? (
                        <>
                          <span className="mcp-status-dot" />
                          {t("settings.tools.connected")}
                          {toolCount > 0 && (
                            <span className="mcp-tool-count">{toolCount} 工具</span>
                          )}
                        </>
                      ) : (
                        t("settings.tools.offline")
                      )}
                    </span>
                  </div>
                  <div className="mcp-server-actions">
                    {connected ? (
                      <button
                        className="btn btn-icon-sm"
                        onClick={() => handleDisconnect(s.name)}
                        title={t("settings.tools.disconnect")}
                      >
                        <ToolIcon size={14} />
                      </button>
                    ) : (
                      <button
                        className="btn btn-icon-sm"
                        onClick={() => connect({ name: s.name, command: s.command, args: s.args, env: s.env })}
                        title={t("settings.tools.connect")}
                      >
                        <RefreshIcon size={14} />
                      </button>
                    )}
                    <button
                      className="btn btn-icon-sm btn-danger"
                      onClick={() => {
                        if (connected) handleDisconnect(s.name);
                        removeMcpServer(s.name);
                      }}
                      title={t("settings.tools.remove")}
                    >
                      <DeleteIcon size={14} />
                    </button>
                  </div>
                </div>
                <div className="mcp-server-cmd">
                  <code>{s.command} {s.args.join(" ")}</code>
                </div>
              </div>
            );
          })
        )}
      </div>

      {/* 添加表单 */}
      <details className="mcp-add-form">
        <summary className="mcp-add-summary">
          <PlusIcon size={14} />
          手动添加 MCP 服务器
        </summary>
        <div className="provider-form" style={{ marginTop: 12 }}>
          <div className="form-group">
            <label>{t("settings.tools.name")}</label>
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder={t("settings.tools.namePlaceholder")}
            />
          </div>
          <div className="form-group">
            <label>{t("settings.tools.command")}</label>
            <input
              value={command}
              onChange={(e) => setCommand(e.target.value)}
              placeholder="npx"
            />
          </div>
          <div className="form-group">
            <label>{t("settings.tools.args")}</label>
            <input
              value={args}
              onChange={(e) => setArgs(e.target.value)}
              placeholder="-y @modelcontextprotocol/server-filesystem ."
            />
            <span className="form-hint">{t("settings.tools.argsHint")}</span>
          </div>
          {error && <div className="mcp-error">{error}</div>}
          <div className="form-actions">
            <button className="btn btn-primary" onClick={handleConnect} disabled={busy}>
              <PlusIcon size={14} />
              {t("settings.tools.connect")}
            </button>
          </div>
        </div>
      </details>

      {/* 工具列表（按服务器分组） */}
      <div className="mcp-tools-section">
        <h4 className="mcp-tools-title">
          {t("settings.tools.availableTools")} ({tools.length})
        </h4>
        {tools.length === 0 ? (
          <p className="mcp-tools-empty">{t("settings.tools.noTools")}</p>
        ) : (
          Object.entries(toolsByServer).map(([server, serverTools]) => (
            <div key={server} className="mcp-tools-group">
              <div className="mcp-tools-server-label">{server}</div>
              <div className="mcp-tools">
                {serverTools.map((tl, i) => (
                  <div key={`${tl.name}-${i}`} className="mcp-tool-chip">
                    <span className="mcp-tool-name">{tl.name.split("::").pop() || tl.name}</span>
                    {tl.description && (
                      <span className="mcp-tool-desc">{tl.description}</span>
                    )}
                  </div>
                ))}
              </div>
            </div>
          ))
        )}
      </div>
    </section>
  );
}
