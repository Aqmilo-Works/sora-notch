//! Extra windows: the Settings window (opened on demand) and the title-bar buttons its custom chrome needs.
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

#[tauri::command]
pub async fn open_settings(app: tauri::AppHandle, section: Option<String>) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        crate::platform::show_on_top(&w);
        if let Some(section) = section { let _ = w.emit("settings-section", section); }
        return Ok(());
    }
    let url = match section.as_deref() { Some(s) if s.chars().all(|c| c.is_ascii_alphanumeric()) => format!("settings.html?section={}", s), _ => "settings.html".to_string() };
    WebviewWindowBuilder::new(&app, "settings", WebviewUrl::App(url.into()))
        .title("Sora Settings").inner_size(900.0, 620.0).resizable(false).decorations(false).transparent(true).shadow(false).center().focused(true)
        .build().map(|w| crate::platform::show_on_top(&w)).map_err(|e| e.to_string())
}
#[tauri::command]
pub fn window_close(window: tauri::WebviewWindow) { let _ = window.close(); }
#[tauri::command]
pub fn window_minimize(window: tauri::WebviewWindow) { let _ = window.minimize(); }
