import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { CodeIcon } from "../components/Icons";
import "../styles/ide.css";

// ─── 类型 ─────────────────────────────────────────────

interface CodeServerStatus {
  installed: boolean;
  running: boolean;
  port: number;
  workspace: string;
  url: string;
  version: string;
}

interface IdeReadyEvent {
  url: string;
  port: number;
}

type IdePhase = "loading" | "starting" | "running" | "error";

// ─── 组件（状态页 — VS Code 在独立窗口中运行）─────────
// code-server 二进制已随应用打包（Tauri sidecar），无需用户下载

export default function IdePage() {
  const [phase, setPhase] = useState<IdePhase>("loading");
  const [status, setStatus] = useState<CodeServerStatus | null>(null);
  const [error, setError] = useState("");
  const [logs, setLogs] = useState("");

  useEffect(() => {
    let unlisten: UnlistenFn | null = null;

    (async () => {
      try {
        // 监听 IDE 就绪事件
        const p = listen<IdeReadyEvent>("ide-ready", (event) => {
          if (event.payload.url) {
            setPhase("running");
          }
        });
        unlisten = await p;

        // 查询状态
        const s = await invoke<CodeServerStatus>("code_server_status");
        setStatus(s);

        if (s.running) {
          setPhase("running");
        } else {
          setPhase("starting");
        }
      } catch (e) {
        setError(String(e));
        setPhase("error");
      }
    })();

    return () => {
      unlisten?.();
    };
  }, []);

  const handleOpen = async () => {
    try {
      setPhase("loading");
      await invoke("code_server_open_ide_window");
    } catch (e) {
      setError(String(e));
      setPhase("error");
    }
  };

  const handleReadLogs = async () => {
    try {
      const logContent = await invoke<string>("code_server_read_logs");
      setLogs(logContent);
    } catch (e) {
      setLogs("读取日志失败: " + String(e));
    }
  };

  const handleBackToChat = () => {
    window.dispatchEvent(new CustomEvent("navigate", { detail: "chat" }));
  };

  // ─── 启动中 ──────────────────────────────────────

  if (phase === "starting") {
    return (
      <div className="cs-container">
        <div className="cs-center-box">
          <div className="cs-spinner" />
          <h2 className="cs-title">Code Server 启动中...</h2>
          <p className="cs-text">
            IDE 内核正在后台启动，就绪后将自动打开独立窗口。
          </p>
          <p className="cs-note">首次启动需要 5-10 秒，后续将秒开。</p>
          <button className="cs-btn cs-btn-primary" onClick={handleOpen}>
            手动打开 VS Code
          </button>
        </div>
      </div>
    );
  }

  // ─── 运行中（IDE 在独立窗口中打开） ──────────────

  if (phase === "running") {
    return (
      <div className="cs-container">
        <div className="cs-center-box">
          <div className="cs-icon-wrap">
            <CodeIcon size={48} />
          </div>
          <h2 className="cs-title">VS Code 已就绪</h2>
          <p className="cs-text">
            IDE 正在独立窗口中运行。
            <br />
            这是一个完整的 VS Code，拥有全部插件生态和智能代码能力。
          </p>
          {status && (
            <p className="cs-note">
              端口: {status.port} &nbsp;|&nbsp; 工作区: {status.workspace || "默认"}
            </p>
          )}
          <div className="cs-btn-row">
            <button className="cs-btn cs-btn-primary" onClick={handleOpen}>
              打开 VS Code 窗口
            </button>
            <button className="cs-btn" onClick={handleBackToChat}>
              返回对话
            </button>
          </div>
        </div>
      </div>
    );
  }

  // ─── 错误 ────────────────────────────────────────

  if (phase === "error") {
    return (
      <div className="cs-container">
        <div className="cs-center-box">
          <div className="cs-icon-wrap cs-error-icon">
            <CodeIcon size={48} />
          </div>
          <h2 className="cs-title">IDE 异常</h2>
          <pre className="cs-error">{error}</pre>
          <div className="cs-btn-row">
            <button
              className="cs-btn cs-btn-primary"
              onClick={() => {
                setPhase("loading");
                setError("");
                setLogs("");
                window.location.reload();
              }}
            >
              重试
            </button>
            <button className="cs-btn" onClick={handleReadLogs}>
              查看日志
            </button>
            <button className="cs-btn" onClick={handleBackToChat}>
              返回对话
            </button>
          </div>
          {logs && (
            <pre className="cs-error" style={{ marginTop: 12 }}>
              {logs}
            </pre>
          )}
        </div>
      </div>
    );
  }

  // ─── 加载中 ──────────────────────────────────────

  return (
    <div className="cs-container">
      <div className="cs-center-box">
        <div className="cs-spinner" />
        <p className="cs-text">正在检查 IDE 环境...</p>
      </div>
    </div>
  );
}
