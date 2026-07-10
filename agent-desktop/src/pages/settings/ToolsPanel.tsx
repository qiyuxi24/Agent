import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { useAppStore, type McpServerUI } from "../../stores/appStore";
import { PlusIcon, DeleteIcon, ToolIcon, RefreshIcon, DownloadIcon, StoreIcon, BugIcon, ErrorIcon, SearchIcon } from "../../components/Icons";

interface ServerInfo {
  name: string;
  status: string;
  tool_count: number;
  resolved_command: string;
  error_count: number;
  last_error: string | null;
}

interface ToolInfo {
  name: string;
  description?: string;
}

/** MCP 市场条目（从后端在线抓取） */
interface McpMarketEntry {
  name: string;
  description: string;
  description_zh: string;
  command: string;
  args: string;
  category: string;
  stars: number;
  source: string;
  env?: Record<string, string>;
  homepage?: string;
}

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
  const [envKeys, setEnvKeys] = useState<[string, string][]>([]);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [showMarket, setShowMarket] = useState(false);
  const [expandedServer, setExpandedServer] = useState<string | null>(null);
  const [stderrLogs, setStderrLogs] = useState<Record<string, string[]>>({});

  // MCP 市场（从后端在线抓取）
  const [mcpMarket, setMcpMarket] = useState<McpMarketEntry[]>([]);
  const [marketLoading, setMarketLoading] = useState(false);
  const [marketError, setMarketError] = useState("");
  const [marketSearch, setMarketSearch] = useState("");
  const [marketCat, setMarketCat] = useState("all");

  const loadMarket = async () => {
    setMarketLoading(true);
    setMarketError("");
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const entries = await invoke<McpMarketEntry[]>("mcp_market_list");
      setMcpMarket(entries);
    } catch (e) {
      setMarketError(typeof e === "string" ? e : (e as Error)?.message || "加载失败");
    } finally {
      setMarketLoading(false);
    }
  };

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

  const buildEnvMap = (): Record<string, string> | undefined => {
    const nonEmpty = envKeys.filter(([, v]) => v.trim());
    if (nonEmpty.length === 0) return undefined;
    const map: Record<string, string> = {};
    for (const [k, v] of nonEmpty) {
      map[k.trim()] = v.trim();
    }
    return map;
  };

  const connect = async (cfg: { name: string; command: string; args: string[]; env?: Record<string, string> }) => {
    setBusy(true);
    setError("");
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const env = cfg.env || buildEnvMap() || null;
      await invoke("mcp_connect", {
        config: { name: cfg.name, command: cfg.command, args: cfg.args, env },
      });
      addMcpServer({ name: cfg.name, command: cfg.command, args: cfg.args, env: env || undefined });
      setName("");
      setCommand("");
      setArgs("");
      setEnvKeys([]);
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
  const quickInstall = async (rec: McpMarketEntry) => {
    setBusy(true);
    setError("");
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const argList = rec.args.split(/\s+/).filter(Boolean);
      // 使用包名作为 server name（去掉 scope 前缀）
      const serverName = rec.name.startsWith("@") 
        ? rec.name.split("/").pop() || rec.name
        : rec.name;
      const env = rec.env && Object.keys(rec.env).length > 0 ? rec.env : undefined;
      await invoke("mcp_connect", {
        config: { name: serverName, command: rec.command, args: argList, env: env || null },
      });
      addMcpServer({ name: serverName, command: rec.command, args: argList, env });
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

  const viewStderr = async (serverName: string) => {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const lines = await invoke<string[]>("mcp_server_stderr", { name: serverName });
      setStderrLogs((prev) => ({ ...prev, [serverName]: lines }));
      setExpandedServer(expandedServer === serverName ? null : serverName);
    } catch {
      // ignore
    }
  };

  const runHealthCheck = async () => {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const dead = await invoke<string[]>("mcp_health_check", { autoReconnect: true });
      if (dead.length > 0) {
        setError(`以下服务器已断开并尝试自动重连: ${dead.join(", ")}`);
      } else {
        setError("");
      }
      await refresh();
    } catch {
      // ignore
    }
  };

  const reconnectServer = async (serverName: string) => {
    setBusy(true);
    setError("");
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const count = await invoke<number>("mcp_reconnect", { name: serverName });
      setError(`${serverName} 重连成功（${count} 个工具）`);
      await refresh();
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : (e as Error)?.message || String(e));
    } finally {
      setBusy(false);
    }
  };

  const clearCache = async () => {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("mcp_clear_cache");
      setError("工具调用缓存已清空");
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
      <div className="section-header mcp-section-header">
        <div className="mcp-header-actions">
          <button
            className={`btn btn-sm ${showMarket ? "btn-secondary" : "btn-outline"}`}
            onClick={() => {
              if (!showMarket && mcpMarket.length === 0) loadMarket();
              setShowMarket(!showMarket);
            }}
          >
            <StoreIcon size={14} />
            {showMarket ? "收起市场" : `MCP 市场${mcpMarket.length > 0 ? ` (${mcpMarket.length})` : ""}`}
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
          <div className="skills-search-bar" style={{ marginBottom: 12 }}>
            <SearchIcon size={14} className="skills-search-icon" />
            <input
              className="skills-search-input"
              placeholder="搜索 MCP 服务器..."
              value={marketSearch}
              onChange={(e) => setMarketSearch(e.target.value)}
            />
          </div>
          <div className="skills-cat-filters" style={{ marginBottom: 12 }}>
            {["all", "tools", "browser", "search", "database", "ai", "communication", "design", "infra"].map((cat) => (
              <button
                key={cat}
                className={`skills-cat-btn ${marketCat === cat ? "active" : ""}`}
                onClick={() => setMarketCat(cat)}
              >
                {cat === "all" ? "全部" : cat}
              </button>
            ))}
            <button className="btn btn-sm btn-outline" onClick={loadMarket} disabled={marketLoading} title="从在线源刷新">
              <RefreshIcon size={12} />
            </button>
          </div>

          {marketLoading ? (
            <div className="skills-loading">正在从 npm/GitHub 获取最新 MCP 服务器...</div>
          ) : marketError && mcpMarket.length === 0 ? (
            <div className="skills-error">
              <p>{marketError}</p>
              <button className="btn btn-secondary" onClick={loadMarket}>重试</button>
            </div>
          ) : (
            <div className="mcp-market-list">
              {((): McpMarketEntry[] => {
                let filtered = mcpMarket;
                if (marketCat !== "all") filtered = filtered.filter(e => e.category === marketCat);
                if (marketSearch) {
                  const q = marketSearch.toLowerCase();
                  filtered = filtered.filter(e =>
                    e.name.toLowerCase().includes(q) ||
                    e.description.toLowerCase().includes(q) ||
                    e.description_zh.toLowerCase().includes(q)
                  );
                }
                return filtered;
              })().map((rec) => {
                const serverName = rec.name.startsWith("@") ? rec.name.split("/").pop() || rec.name : rec.name;
                const alreadyInstalled = installedNames.has(serverName!);
                const connected = connectedNames.has(serverName!);
                return (
                  <div key={rec.name} className={`mcp-market-card ${connected ? "connected" : ""}`}>
                    <div className="mcp-market-info">
                      <span className="mcp-market-name" title={`${rec.name} (${rec.source})`}>
                        {serverName}
                        {rec.stars > 0 && <span className="mcp-market-stars" title={`${rec.stars} stars`}> ★ {rec.stars >= 1000 ? `${(rec.stars / 1000).toFixed(1)}k` : rec.stars}</span>}
                      </span>
                      <span className="skill-card-cat">{rec.category}</span>
                      {rec.source !== "builtin" && <span className="skill-source-badge" title={`来源: ${rec.source}`}>{rec.source}</span>}
                      {connected && <span className="skill-status on">● 已连接</span>}
                      <p className="mcp-market-desc">
                        {lang.startsWith("zh") && rec.description_zh ? rec.description_zh : rec.description}
                      </p>
                    </div>
                    <div className="mcp-market-actions">
                      {connected ? (
                        <button
                          className="btn btn-sm btn-secondary"
                          onClick={() => handleDisconnect(serverName!)}
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
              {((): number => {
                let filtered = mcpMarket;
                if (marketCat !== "all") filtered = filtered.filter(e => e.category === marketCat);
                if (marketSearch) {
                  const q = marketSearch.toLowerCase();
                  filtered = filtered.filter(e =>
                    e.name.toLowerCase().includes(q) ||
                    e.description.toLowerCase().includes(q) ||
                    e.description_zh.toLowerCase().includes(q)
                  );
                }
                return filtered.length;
              })() === 0 && mcpMarket.length > 0 && (
                <div className="placeholder-section"><p>没有匹配的 MCP 服务器</p></div>
              )}
            </div>
          )}
          <div className="mcp-market-footer" style={{ marginTop: 8, fontSize: 11, opacity: 0.5, textAlign: "center" }}>
            数据来源: npm registry + GitHub API · 5分钟缓存 · 共 {mcpMarket.length} 个
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
            const errCount = serverInfo?.error_count ?? 0;
            const lastError = serverInfo?.last_error;
            const resolvedCmd = serverInfo?.resolved_command;
            const isExpanded = expandedServer === s.name;
            const envPairs = s.env ? Object.entries(s.env) : [];
            return (
              <div
                key={s.name}
                className={`mcp-server-card ${connected ? "connected" : "offline"} ${isExpanded ? "expanded" : ""}`}
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
                    {errCount > 0 && (
                      <span className="mcp-error-count" title={`${errCount} 次错误`}>
                        <ErrorIcon size={11} /> {errCount}
                      </span>
                    )}
                  </div>
                  <div className="mcp-server-actions">
                    {connected && (
                      <button
                        className="btn btn-icon-sm"
                        onClick={() => viewStderr(s.name)}
                        title="查看日志"
                      >
                        <BugIcon size={14} />
                      </button>
                    )}
                    {connected ? (
                      <button
                        className="btn btn-icon-sm"
                        onClick={() => handleDisconnect(s.name)}
                        title={t("settings.tools.disconnect")}
                      >
                        <ToolIcon size={14} />
                      </button>
                    ) : (
                      <>
                        <button
                          className="btn btn-icon-sm"
                          onClick={() => connect({ name: s.name, command: s.command, args: s.args, env: s.env })}
                          title={t("settings.tools.connect")}
                        >
                          <RefreshIcon size={14} />
                        </button>
                        <button
                          className="btn btn-icon-sm"
                          onClick={() => reconnectServer(s.name)}
                          title="使用保存的配置重连"
                          disabled={busy}
                        >
                          <ToolIcon size={14} />
                        </button>
                      </>
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
                  {resolvedCmd && resolvedCmd !== `${s.command} ${s.args.join(" ")}` && (
                    <div className="mcp-server-resolved">
                      → {resolvedCmd}
                    </div>
                  )}
                </div>
                {/* 环境变量展示 */}
                {envPairs.length > 0 && (
                  <div className="mcp-server-env">
                    {envPairs.map(([k, v]) => (
                      <span key={k} className="mcp-env-chip" title={`${k}=${v}`}>
                        {k}{v ? `=***` : ""}
                      </span>
                    ))}
                  </div>
                )}
                {/* 错误诊断 */}
                {lastError && (
                  <div className="mcp-server-error">
                    <ErrorIcon size={12} /> {lastError}
                  </div>
                )}
                {/* 展开的 stderr 日志 */}
                {isExpanded && connected && (
                  <div className="mcp-stderr-log">
                    <div className="mcp-stderr-header">
                      stderr 输出（最近 50 行）
                      {stderrLogs[s.name]?.length === 0 && (
                        <span className="mcp-stderr-empty">（无输出）</span>
                      )}
                    </div>
                    {stderrLogs[s.name]?.map((line, i) => (
                      <div key={i} className="mcp-stderr-line">
                        {line}
                      </div>
                    ))}
                  </div>
                )}
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
              placeholder="-y @anthropic/mcp-server-filesystem ."
            />
            <span className="form-hint">{t("settings.tools.argsHint")}</span>
          </div>
          {/* 环境变量配置 */}
          <div className="form-group">
            <label>环境变量</label>
            {envKeys.map(([k, v], i) => (
              <div key={i} className="mcp-env-row">
                <input
                  value={k}
                  onChange={(e) => {
                    const next = [...envKeys];
                    next[i] = [e.target.value, next[i][1]];
                    setEnvKeys(next);
                  }}
                  placeholder="VAR_NAME"
                  className="mcp-env-key"
                />
                <span className="mcp-env-eq">=</span>
                <input
                  value={v}
                  onChange={(e) => {
                    const next = [...envKeys];
                    next[i] = [next[i][0], e.target.value];
                    setEnvKeys(next);
                  }}
                  placeholder="value"
                  className="mcp-env-val"
                />
                <button
                  className="btn btn-icon-sm"
                  onClick={() => setEnvKeys(envKeys.filter((_, j) => j !== i))}
                >
                  ×
                </button>
              </div>
            ))}
            <button
              className="btn btn-sm btn-outline"
              onClick={() => setEnvKeys([...envKeys, ["", ""]])}
              style={{ marginTop: 4 }}
            >
              + 添加环境变量
            </button>
          </div>
          {error && <div className="mcp-error">{error}</div>}
          <div className="form-actions">
            <button className="btn btn-primary" onClick={handleConnect} disabled={busy}>
              <PlusIcon size={14} />
              {t("settings.tools.connect")}
            </button>
            <button className="btn btn-sm btn-outline" onClick={runHealthCheck} disabled={busy}>
              <RefreshIcon size={12} /> 健康检查+自动重连
            </button>
            <button className="btn btn-sm btn-outline" onClick={clearCache} disabled={busy}>
              清空缓存
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
