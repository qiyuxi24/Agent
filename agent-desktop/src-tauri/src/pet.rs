//! 桌宠模块：管理宠物窗口显隐、互动数值（心情/羁绊/精力），并把 Agent 状态广播给宠物窗。
//!
//! 宠物窗是独立的透明 Tauri 窗口（label = "pet"），前端 `src/pet/` 监听已有的
//! `thinking-start` / `tool-call` / `stream-done` / `stream-error` 事件来切换动画状态；
//! 本模块只负责：宠物窗开关、点击互动、数值持久化，以及 `pet-state` / `pet-stats` 事件。

use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

/// 宠物数值（0-100，daysTogether 为相处天数）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PetStats {
    pub mood: u8,
    pub friendship: u8,
    pub energy: u8,
    pub days_together: u32,
}

impl Default for PetStats {
    fn default() -> Self {
        Self {
            mood: 70,
            friendship: 50,
            energy: 80,
            days_together: 1,
        }
    }
}

/// 广播给宠物窗的高层状态（前端据此切换动画行）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PetStateEvent {
    pub state: String,
    /// 可选气泡文案（如 "♥ 好喜欢"）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Default)]
pub struct PetManager {
    pub stats: Mutex<PetStats>,
    pub visible: Mutex<bool>,
}

#[derive(Deserialize)]
pub struct PetInteractRequest {
    pub action: String, // "pet" | "feed" | "play"
}

fn pet_file(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("pet.json"))
}

/// 启动时从 app_data_dir/pet.json 读取数值（不存在则用默认）
pub fn load_stats(app: &AppHandle) -> PetStats {
    if let Some(p) = pet_file(app) {
        if let Ok(s) = std::fs::read_to_string(&p) {
            if let Ok(s) = serde_json::from_str::<PetStats>(&s) {
                return s;
            }
        }
    }
    PetStats::default()
}

fn save_stats(app: &AppHandle, stats: &PetStats) {
    if let Some(p) = pet_file(app) {
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(s) = serde_json::to_string_pretty(stats) {
            let _ = std::fs::write(&p, s);
        }
    }
}

/// 把宠物窗放到主显示器右下角
fn position_bottom_right(win: &WebviewWindow) {
    if let Ok(monitors) = win.available_monitors() {
        if let Some(m) = monitors.first() {
            let w = 220i32;
            let h = 260i32;
            let x = (m.size().width as i32) - w - 24;
            let y = (m.size().height as i32) - h - 24;
            let _ = win.set_position(tauri::LogicalPosition::new(x as f64, y as f64));
        }
    }
}

/// 切换宠物窗显隐，返回切换后的可见状态
#[tauri::command]
pub fn toggle_pet(app: AppHandle) -> Result<bool, String> {
    let state = app.state::<crate::AppState>();
    let mut visible = state.pet.visible.lock().unwrap();
    *visible = !*visible;
    let show = *visible;
    drop(visible);

    if let Some(win) = app.get_webview_window("pet") {
        if show {
            position_bottom_right(&win);
            let _ = win.show();
            let _ = win.set_focus();
        } else {
            let _ = win.hide();
        }
    }
    Ok(show)
}

/// 互动：抚摸/喂食/玩耍，更新数值并广播
#[tauri::command]
pub fn pet_interact(app: AppHandle, request: PetInteractRequest) -> Result<PetStats, String> {
    let state = app.state::<crate::AppState>();
    let mut stats = state.pet.stats.lock().unwrap();
    let (mood_d, friend_d, energy_d, label) = match request.action.as_str() {
        "pet" => (5i32, 3i32, 0i32, Some("♥ 好喜欢")),
        "feed" => (8, 2, 10, Some("🍖 好吃")),
        "play" => (10, 5, -6, Some("✨ 好玩")),
        _ => (0, 0, 0, None),
    };
    stats.mood = (stats.mood as i32 + mood_d).clamp(0, 100) as u8;
    stats.friendship = (stats.friendship as i32 + friend_d).clamp(0, 100) as u8;
    stats.energy = (stats.energy as i32 + energy_d).clamp(0, 100) as u8;
    let out = stats.clone();
    drop(stats);

    save_stats(&app, &out);
    let _ = app.emit("pet-stats", out.clone());
    if let Some(l) = label {
        let _ = app.emit(
            "pet-state",
            PetStateEvent {
                state: "waving".into(),
                label: Some(l.into()),
            },
        );
    }
    Ok(out)
}

/// 读取当前数值
#[tauri::command]
pub fn get_pet_stats(app: AppHandle) -> Result<PetStats, String> {
    let state = app.state::<crate::AppState>();
    let stats = state.pet.stats.lock().unwrap().clone();
    Ok(stats)
}
