use tauri::{Emitter, WebviewWindow};

pub fn set_active(window: &WebviewWindow, active: bool) {
    let _ = window.emit_to(window.label(), "window-activity-changed", active);
    // Hiding a Tauri window alone does not change the WebView2 controller's
    // IsVisible flag. Transparent, hidden pages can otherwise keep rendering.
    #[cfg(windows)]
    let _ = window.with_webview(move |webview| unsafe {
        let _ = webview.controller().SetIsVisible(active);
    });
}

pub fn hide(window: &WebviewWindow) -> tauri::Result<()> {
    set_active(window, false);
    let result = window.hide();
    if result.is_err() {
        set_active(window, window.is_visible().unwrap_or(false));
    }
    result
}

pub fn show(window: &WebviewWindow) -> tauri::Result<()> {
    set_active(window, true);
    let result = window.show();
    if result.is_err() {
        set_active(window, false);
    }
    result
}
