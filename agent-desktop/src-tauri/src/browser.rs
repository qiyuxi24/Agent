//! 内置浏览器模块 — 通过 Tauri WebviewBuilder 创建真正的子 webview
//! 底层使用 WebView2（Windows），与 Edge 浏览器同内核
//! 需要 tauri 的 "unstable" feature 以启用 multiwebview 支持

use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewUrl};
use url::Url;

#[derive(Clone, serde::Serialize)]
pub struct BrowserUrlChanged {
    pub url: String,
}

#[derive(Clone, serde::Serialize)]
pub struct BrowserPageLoaded {
    pub url: String,
}

/// 创建浏览器子 webview，嵌入到主窗口内容区
#[tauri::command]
pub async fn browser_create(
    app: AppHandle,
    url: String,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) -> Result<(), String> {
    // 先销毁旧的浏览器 webview
    if let Some(old) = app.get_webview("browser") {
        let _ = old.close();
    }

    let parsed: Url = url
        .parse()
        .map_err(|e| format!("URL 解析失败: {}", e))?;

    let app_nav = app.clone();
    let app_load = app.clone();

    let builder = tauri::WebviewBuilder::new("browser", WebviewUrl::External(parsed))
        .auto_resize()
        .on_navigation(move |nav_url| {
            let _ = app_nav.emit("browser-url-changed", BrowserUrlChanged {
                url: nav_url.to_string(),
            });
            true
        })
        .on_page_load(move |_wv, payload| {
            let _ = app_load.emit("browser-page-loaded", BrowserPageLoaded {
                url: payload.url().to_string(),
            });
        });

    let window = app
        .get_window("main")
        .ok_or_else(|| "主窗口未找到".to_string())?;

    window
        .add_child(
            builder,
            LogicalPosition::new(x, y),
            LogicalSize::new(w, h),
        )
        .map_err(|e| format!("创建浏览器 webview 失败: {}", e))?;

    Ok(())
}

/// 导航到指定 URL
#[tauri::command]
pub async fn browser_navigate(app: AppHandle, url: String) -> Result<(), String> {
    let wv = app
        .get_webview("browser")
        .ok_or_else(|| "浏览器未创建，请先调用 browser_create".to_string())?;
    let parsed: Url = url
        .parse()
        .map_err(|e| format!("URL 解析失败: {}", e))?;
    wv.navigate(parsed)
        .map_err(|e| format!("导航失败: {}", e))?;
    Ok(())
}

/// 获取当前 URL
#[tauri::command]
pub async fn browser_get_url(app: AppHandle) -> Result<String, String> {
    let wv = app
        .get_webview("browser")
        .ok_or_else(|| "浏览器未创建".to_string())?;
    Ok(wv.url().map(|u| u.to_string()).unwrap_or_default())
}

/// 后退（WebView2 原生历史栈）
#[tauri::command]
pub async fn browser_go_back(app: AppHandle) -> Result<(), String> {
    let wv = app
        .get_webview("browser")
        .ok_or_else(|| "浏览器未创建".to_string())?;
    wv.eval("history.back()")
        .map_err(|e| format!("后退失败: {}", e))?;
    Ok(())
}

/// 前进（WebView2 原生历史栈）
#[tauri::command]
pub async fn browser_go_forward(app: AppHandle) -> Result<(), String> {
    let wv = app
        .get_webview("browser")
        .ok_or_else(|| "浏览器未创建".to_string())?;
    wv.eval("history.forward()")
        .map_err(|e| format!("前进失败: {}", e))?;
    Ok(())
}

/// 刷新页面
#[tauri::command]
pub async fn browser_reload(app: AppHandle) -> Result<(), String> {
    let wv = app
        .get_webview("browser")
        .ok_or_else(|| "浏览器未创建".to_string())?;
    wv.eval("location.reload()")
        .map_err(|e| format!("刷新失败: {}", e))?;
    Ok(())
}

/// 停止加载
#[tauri::command]
pub async fn browser_stop(app: AppHandle) -> Result<(), String> {
    let wv = app
        .get_webview("browser")
        .ok_or_else(|| "浏览器未创建".to_string())?;
    wv.eval("window.stop()")
        .map_err(|e| format!("停止加载失败: {}", e))?;
    Ok(())
}

/// 调整浏览器 webview 位置和大小
#[tauri::command]
pub async fn browser_resize(
    app: AppHandle,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) -> Result<(), String> {
    let wv = app
        .get_webview("browser")
        .ok_or_else(|| "浏览器未创建".to_string())?;
    wv.set_position(LogicalPosition::new(x, y))
        .map_err(|e| format!("调整位置失败: {}", e))?;
    wv.set_size(LogicalSize::new(w, h))
        .map_err(|e| format!("调整大小失败: {}", e))?;
    Ok(())
}

/// 销毁浏览器 webview
#[tauri::command]
pub async fn browser_destroy(app: AppHandle) -> Result<(), String> {
    if let Some(wv) = app.get_webview("browser") {
        wv.close()
            .map_err(|_| "关闭浏览器失败".to_string())?;
    }
    Ok(())
}
