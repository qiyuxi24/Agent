import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { CodeIcon } from "../components/Icons";
import "../styles/code-server-status.css";

// ─── 类型（与 Rust code_server.rs 保持同步） ─────────

interface CodeServerStatus {
  installed: boolean;
  running: boolean;
  port: number;
  workspace: string;
  url: string;
  version: string;
  error?: string | null;
}

interface IdeReadyEvent {
  url: string;
  port: number;
  error?: string | null;
}

type IdePhase = "loading" | "starting" | "running" | "error";

/// 启动阶段超时（秒），超过后自动转为 error
const STARTING_TIMEOUT_SECS = 35;

// ─── 组件（状态页 — VS Code 在独立窗口中运行）─────────

export default function IdePage() {
  const [phase, setPhase] = useState<IdePhase>("loading");
  const [status, setStatus] = useState<CodeServerStatus | null>(null);
  const [error, setError] = useState("");
  const [logs, setLogs] = useState("");
  const [restarting, setRestarting] = useState(false);
  const startingTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  /// 清理启动超时定时器
  const clearStartingTimer = useCallback(() => {
    if (startingTimerRef.current) {
      clearTimeout(startingTimerRef.current);
      startingTimerRef.current = null;
    }
  }, []);

  /// 设置启动超时定时器
  const setStartingTimer = useCallback(() => {
    clearStartingTimer();
    startingTimerRef.current = setTimeout(() => {
      setPhase("error");
      setError(`Code Server 启动超时（${STARTING_TIMEOUT_SECS}秒无响应）。可能原因：Node.js 未安装、端口被占用、或 code-server 文件损坏。`);
    }, STARTING_TIMEOUT_SECS * 1000);
  }, [clearStartingTimer]);

  useEffect(() => {
    let unlisten: UnlistenFn | null = null;

    (async () => {
      try {
        // 监听 IDE 就绪事件（含错误事件）
        const p = listen<IdeReadyEvent>("ide-ready", (event) => {
          clearStartingTimer();
          if (event.payload.error) {
            // 启动失败
            setError(event.payload.error);
            setPhase("error");
          } else if (event.payload.url) {
            // 启动成功
            setError("");
            setPhase("running");
          }
        });
        unlisten = await p;

        // 查询当前状态
        const s = await invoke<CodeServerStatus>("code_server_status");
        setStatus(s);

        if (s.error && !s.running) {
          // 有历史错误且未运行 → 直接进错误页
          setError(s.error);
          setPhase("error");
        } else if (s.running) {
          setPhase("running");
        } else if (!s.installed) {
          setError("Code Server 未安装。请运行 npm run download:code-server 下载。");
          setPhase("error");
        } else {
          setPhase("starting");
          setStartingTimer();
        }
      } catch (e) {
        clearStartingTimer();
        setError(String(e));
        setPhase("error");
      }
    })();

    return () => {
      clearStartingTimer();
      unlisten?.();
    };
  }, [clearStartingTimer, setStartingTimer]);

  /// 打开 IDE 窗口
  const handleOpen = async () => {
    try {
      setPhase("loading");
      await invoke("code_server_open_ide_window");
    } catch (e) {
      setError(String(e));
      setPhase("error");
    }
  };

  /// 重启 Code Server
  const handleRestart = async () => {
    try {
      setRestarting(true);
      setError("");
      setLogs("");
      setPhase("loading");
      const s = await invoke<CodeServerStatus>("code_server_restart");
      setStatus(s);
      setPhase("running");
    } catch (e) {
      setError(String(e));
      setPhase("error");
    } finally {
      setRestarting(false);
    }
  };

  /// 重新检查状态（轻量重试，不重启进程）
  const handleRecheck = async () => {
    try {
      setError("");
      setLogs("");
      setPhase("loading");
      const s = await invoke<CodeServerStatus>("code_server_status");
      setStatus(s);
      if (s.running) {
        setPhase("running");
      } else if (s.installed) {
        setPhase("starting");
        setStartingTimer();
      } else {
        setError("Code Server 未安装。");
        setPhase("error");
      }
    } catch (e) {
      setError(String(e));
      setPhase("error");
    }
  };

  /// 读取日志
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
          <div className="cs-btn-row">
            <button className="cs-btn cs-btn-primary" onClick={handleOpen}>
              手动打开 VS Code
            </button>
            <button className="cs-btn" onClick={handleBackToChat}>
              返回对话
            </button>
          </div>
        </div>
      </div>
    );
  }

  // ─── 运行中 ──────────────────────────────────────

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
            <button className="cs-btn" onClick={handleRestart} disabled={restarting}>
              {restarting ? "重启中..." : "重启 Code Server"}
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
          {error && <pre className="cs-error">{error}</pre>}
          <div className="cs-btn-row">
            <button
              className="cs-btn cs-btn-primary"
              onClick={handleRestart}
              disabled={restarting}
            >
              {restarting ? "重启中..." : "重启 Code Server"}
            </button>
            <button className="cs-btn" onClick={handleRecheck}>
              重新检查
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
