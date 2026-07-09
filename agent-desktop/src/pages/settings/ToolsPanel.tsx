import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { useAppStore, type McpServerUI } from "../../stores/appStore";
import { PlusIcon, DeleteIcon, ToolIcon } from "../../components/Icons";

interface ServerInfo {
  name: string;
  status: string;
  tool_count: number;
}

interface ToolInfo {
  name: string;
  description?: string;
}

export default function ToolsPanel() {
  const { t } = useTranslation();
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

  const refresh = async () => {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const s = await invoke<ServerInfo[]>("mcp_list_servers");
      setServers(s);
      const tl = await invoke<ToolInfo[]>("mcp_list_tools");
      setTools(tl);
    } catch {
      // 非 Tauri 环境（浏览器调试）忽略
    }
  };

  useEffect(() => {
    refresh();
    // eslint-disable-line react-hooks/exhaustive-deps
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

  return (
    <section className="settings-panel">
      <div className="section-header">
        <h3 className="panel-title">{t("settings.tools.title")}</h3>
      </div>

      <p className="panel-desc">{t("settings.tools.desc")}</p>

      {/* 已配置服务器 */}
      <div className="mcp-server-list">
        {mcpServers.length === 0 ? (
          <div className="placeholder-section">
            <p>{t("settings.tools.empty")}</p>
          </div>
        ) : (
          mcpServers.map((s: McpServerUI) => {
            const connected = connectedNames.has(s.name);
            return (
              <div
                key={s.name}
                className={`mcp-server-card ${connected ? "connected" : "offline"}`}
              >
                <div className="mcp-server-head">
                  <div className="mcp-server-info">
                    <span className="mcp-server-name">{s.name}</span>
                    <span className={`mcp-status ${connected ? "on" : "off"}`}>
                      {connected ? t("settings.tools.connected") : t("settings.tools.offline")}
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
                        ↻
                      </button>
                    )}
                    <button
                      className="btn btn-icon-sm btn-danger"
                      onClick={() => removeMcpServer(s.name)}
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
      <div className="provider-form">
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

      {/* 工具列表 */}
      <div className="mcp-tools-section">
        <h4 className="mcp-tools-title">
          {t("settings.tools.availableTools")} ({tools.length})
        </h4>
        {tools.length === 0 ? (
          <p className="mcp-tools-empty">{t("settings.tools.noTools")}</p>
        ) : (
          <div className="mcp-tools">
            {tools.map((tl, i) => (
              <div key={`${tl.name}-${i}`} className="mcp-tool-chip">
                <span className="mcp-tool-name">{tl.name}</span>
                {tl.description && (
                  <span className="mcp-tool-desc">{tl.description}</span>
                )}
              </div>
            ))}
          </div>
        )}
      </div>
    </section>
  );
}
