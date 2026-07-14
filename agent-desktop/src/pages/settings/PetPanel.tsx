import { useState, useEffect, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { PetStats } from "../../pet/types";

export default function PetPanel() {
  const { t } = useTranslation();
  const [visible, setVisible] = useState(false);
  const [stats, setStats] = useState<PetStats>({
    mood: 70,
    friendship: 50,
    energy: 80,
    daysTogether: 1,
  });
  const [loading, setLoading] = useState(true);

  // 初始化：获取当前状态
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const s = await invoke<PetStats>("get_pet_stats");
        if (!cancelled) setStats(s);
      } catch {
        /* 非 Tauri 环境忽略 */
      }
      if (!cancelled) setLoading(false);
    })();
    return () => { cancelled = true; };
  }, []);

  // 监听后端广播的数值更新
  useEffect(() => {
    const unlistens: Promise<UnlistenFn>[] = [];
    unlistens.push(
      listen<PetStats>("pet-stats", (e) => setStats(e.payload)),
    );
    return () => {
      unlistens.forEach((p) => p.then((u) => u()));
    };
  }, []);

  const togglePet = useCallback(async () => {
    try {
      const v = await invoke<boolean>("toggle_pet");
      setVisible(v);
    } catch {
      /* 非 Tauri 环境忽略 */
    }
  }, []);

  const interact = useCallback(async (action: "pet" | "feed" | "play") => {
    try {
      const s = await invoke<PetStats>("pet_interact", { action });
      setStats(s);
    } catch {
      /* 非 Tauri 环境忽略 */
    }
  }, []);

  return (
    <section className="settings-panel">
      {/* 开关 */}
      <div className="form-group">
        <label>{t("settings.pet.enable")}</label>
        <p className="form-desc">{t("settings.pet.enableDesc")}</p>
        <button
          className={`btn ${visible ? "btn-danger" : "btn-primary"}`}
          onClick={togglePet}
          style={{ marginTop: 8 }}
        >
          {visible ? t("settings.pet.hide") : t("settings.pet.show")}
        </button>
      </div>

      <hr className="settings-divider" />

      {/* 互动 */}
      <div className="form-group">
        <label>{t("settings.pet.interact")}</label>
        <div className="pet-interact-btns">
          <button className="btn" onClick={() => interact("pet")}>
            🖐️ {t("settings.pet.pet")}
          </button>
          <button className="btn" onClick={() => interact("feed")}>
            🍖 {t("settings.pet.feed")}
          </button>
          <button className="btn" onClick={() => interact("play")}>
            🎾 {t("settings.pet.play")}
          </button>
        </div>
      </div>

      {/* 数值 */}
      <div className="form-group">
        <label>{t("settings.pet.stats")}</label>
        {loading ? (
          <p className="form-desc">{t("app.loading")}</p>
        ) : (
          <div className="pet-stats-grid">
            <StatBar
              icon="♥"
              label={t("settings.pet.mood")}
              value={stats.mood}
            />
            <StatBar
              icon="🔗"
              label={t("settings.pet.friendship")}
              value={stats.friendship}
            />
            <StatBar
              icon="⚡"
              label={t("settings.pet.energy")}
              value={stats.energy}
            />
            <div className="pet-stat-item">
              <span className="pet-stat-icon">📅</span>
              <span className="pet-stat-label">{t("settings.pet.daysTogether")}</span>
              <span className="pet-stat-value">{stats.daysTogether}</span>
            </div>
          </div>
        )}
      </div>
    </section>
  );
}

function StatBar({
  icon,
  label,
  value,
}: {
  icon: string;
  label: string;
  value: number;
}) {
  const pct = Math.min(100, Math.max(0, value));
  return (
    <div className="pet-stat-item">
      <span className="pet-stat-icon">{icon}</span>
      <span className="pet-stat-label">{label}</span>
      <div className="pet-stat-bar-track">
        <div
          className="pet-stat-bar-fill"
          style={{ width: `${pct}%` }}
        />
      </div>
      <span className="pet-stat-value">{pct}%</span>
    </div>
  );
}
