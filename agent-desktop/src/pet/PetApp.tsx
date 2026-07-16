/**
 * 桌宠主组件（Agent 窗口）
 *
 * 功能：动画循环、Agent 状态驱动、点击抚摸、窗口控制（关闭、置顶）。
 * 这是一个独立的 Tauri 窗口（label="pet"），通过 `pet.html` 加载。
 *
 * 窗口控制走 Rust `window_*` 命令（通过 useWindowManager 间接调用）。
 */

import { useEffect, useRef } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { usePetStore } from "./petStore";
import { drawPet, DEFAULT_ROW_MAP, type SpriteSheet } from "./spriteRenderer";
import type { PetStats, PetVisualState } from "./types";
import { useWindowManager } from "../hooks/useWindowManager";

/** 尝试加载 public/pets/default/ 下的真实 Codex 宠物；没有则用程序化宠物 */
function loadSprite(): Promise<SpriteSheet | null> {
  return fetch("./pets/default/pet.json")
    .then((r) => (r.ok ? r.json() : null))
    .then(
      (cfg) =>
        new Promise<SpriteSheet | null>((resolve) => {
          if (!cfg || !cfg.spritesheet) return resolve(null);
          const img = new Image();
          img.onload = () =>
            resolve({
              img,
              cols: cfg.frameCols ?? 8,
              rows: cfg.frameRows ?? 9,
              frameW: cfg.frameWidth ?? 192,
              frameH: cfg.frameHeight ?? 208,
              rowMap: { ...DEFAULT_ROW_MAP, ...(cfg.rowMap || {}) },
            });
          img.onerror = () => resolve(null);
          img.src = cfg.spritesheet;
        }),
    )
    .catch(() => null);
}

export default function PetApp() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const spriteRef = useRef<SpriteSheet | null>(null);
  const bubble = usePetStore((s) => s.bubble);
  const stats = usePetStore((s) => s.stats);

  // 宠物窗口的窗口管理（关闭/置顶）
  const { close, toggleAlwaysOnTop, isAlwaysOnTop } = useWindowManager();

  // 载入真实精灵图（失败则保持程序化）
  useEffect(() => {
    let cancelled = false;
    loadSprite().then((s) => {
      if (!cancelled) spriteRef.current = s;
    });
    return () => {
      cancelled = true;
    };
  }, []);

  // 动画循环
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    let raf = 0;
    const loop = (t: number) => {
      const st = usePetStore.getState().state;
      drawPet(ctx, canvas.width, canvas.height, st, t, spriteRef.current);
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  }, []);

  // 事件驱动的宠物状态
  useEffect(() => {
    const unlistens: Promise<UnlistenFn>[] = [];
    const { setState, setStats } = usePetStore.getState();

    const add = (name: string, s: PetVisualState, b?: string) =>
      unlistens.push(listen(name, () => setState(s, b ?? null)));

    add("thinking-start", "thinking", "🤔 思考中");
    add("tool-call", "working", "🛠️ 调用工具");
    add("tool-result", "waiting", "⏳ 等待结果");
    add("stream-done", "done", "✅ 完成啦");
    add("stream-error", "error", "❌ 出错了");

    // 一段时间后回到 idle
    let idleTimer: number | undefined;
    const resetToIdle = () => {
      window.clearTimeout(idleTimer);
      idleTimer = window.setTimeout(() => setState("idle"), 3500);
    };
    unlistens.push(listen("stream-done", resetToIdle));
    unlistens.push(listen("stream-error", resetToIdle));
    unlistens.push(listen("thinking-stop", resetToIdle));

    // 后端广播的数值与状态
    unlistens.push(
      listen<PetStats>("pet-stats", (e) => setStats(e.payload)),
    );
    unlistens.push(
      listen<{ state: string; label?: string }>("pet-state", (e) =>
        setState(e.payload.state as PetVisualState, e.payload.label ?? null),
      ),
    );

    // 初始数值
    invoke<PetStats>("get_pet_stats")
      .then((s) => setStats(s))
      .catch(() => {});

    return () => {
      window.clearTimeout(idleTimer);
      unlistens.forEach((p) => p.then((u) => u()));
    };
  }, []);

  // 点击抚摸
  const handleClick = async () => {
    try {
      const s = await invoke<PetStats>("pet_interact", { action: "pet" });
      usePetStore.getState().setStats(s);
    } catch {
      /* 非 Tauri 环境忽略 */
    }
  };

  // 关闭按钮：阻止事件冒泡避免触发拖拽
  const handleClose = (e: React.MouseEvent) => {
    e.stopPropagation();
    close();
  };

  return (
    <div className="pet-root" data-tauri-drag-region onClick={handleClick}>
      {/* 窗口控制按钮（阻止冒泡避免触发 drag） */}
      <div className="pet-controls">
        <button
          className="pet-btn pet-btn-pin"
          onClick={(e) => {
            e.stopPropagation();
            toggleAlwaysOnTop();
          }}
          title={isAlwaysOnTop ? "取消置顶" : "置顶"}
        >
          {isAlwaysOnTop ? "📌" : "📍"}
        </button>
        <button
          className="pet-btn pet-btn-close"
          onClick={handleClose}
          title="关闭宠物"
        >
          ✕
        </button>
      </div>

      <canvas ref={canvasRef} width={220} height={260} className="pet-canvas" />
      {bubble && <div className="pet-bubble">{bubble}</div>}
      <div className="pet-stats">
        <span title="心情">♥{stats.mood}</span>
        <span title="羁绊">🔗{stats.friendship}</span>
        <span title="精力">⚡{stats.energy}</span>
      </div>
    </div>
  );
}
