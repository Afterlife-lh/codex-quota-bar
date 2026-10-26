use tauri::{Emitter, WebviewWindow};

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PopupMemoryTarget {
    Normal,
    Low,
}

#[cfg(any(windows, test))]
fn popup_memory_target(label: &str, active: bool) -> Option<PopupMemoryTarget> {
    match label {
        "detail" | "settings" | "menu" => Some(if active {
            PopupMemoryTarget::Normal
        } else {
            PopupMemoryTarget::Low
        }),
        // In particular, never trim or suspend the resident taskbar WebView.
        _ => None,
    }
}

pub fn set_active(window: &WebviewWindow, active: bool) {
    if window.label() == "taskbar" {
        sync_taskbar(window);
        return;
    }
    let _ = window.emit_to(window.label(), "window-activity-changed", active);
    // Hiding a Tauri window alone does not change the WebView2 controller's
    // IsVisible flag. Transparent, hidden pages can otherwise keep rendering.
    #[cfg(windows)]
    {
        use webview2_com::Microsoft::Web::WebView2::Win32::{
            ICoreWebView2_19, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW,
            COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
        };
        use windows_core::Interface;

        let target = popup_memory_target(window.label(), active);
        // Keep this callback COM-only: invoking Tauri IPC here re-enters the
        // runtime's window-id lock and can deadlock application startup.
        let _ = window.with_webview(move |webview| unsafe {
            let controller = webview.controller();
            if !active {
                let _ = controller.SetIsVisible(false);
            }
            if let Some(target) = target {
                if let Ok(memory) = controller
                    .CoreWebView2()
                    .and_then(|core| core.cast::<ICoreWebView2_19>())
                {
                    let desired = match target {
                        PopupMemoryTarget::Normal => COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
                        PopupMemoryTarget::Low => COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW,
                    };
                    let mut current = Default::default();
                    if memory.MemoryUsageTargetLevel(&mut current).is_err() || current != desired {
                        // Best-effort: older runtimes may not expose this
                        // interface, and a trim failure must never prevent show.
                        let _ = memory.SetMemoryUsageTargetLevel(desired);
                    }
                }
            }
            // Restore the normal target before making a popup visible. Do not
            // mix this policy with TrySuspend/Resume or process working-set hacks.
            if active {
                let _ = controller.SetIsVisible(true);
            }
        });
    }
}

/// The resident widget follows its HWND, never a popup's render policy. Read on
/// the UI thread so a queued hide/show cannot apply an obsolete visibility flag.
pub fn sync_taskbar(window: &WebviewWindow) {
    debug_assert_eq!(window.label(), "taskbar");
    #[cfg(windows)]
    {
        let Ok(hwnd) = window.hwnd() else {
            return;
        };
        let hwnd = hwnd.0 as isize;
        let active =
            unsafe { windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible(hwnd as _) != 0 };
        // Publish outside with_webview. Tauri executes that callback inline on
        // the UI thread while the runtime's window-id mutex can still be held;
        // emitting an event from it re-enters eval_script and deadlocks startup.
        let _ = window.emit_to(window.label(), "window-activity-changed", active);
        let _ = window.with_webview(move |webview| unsafe {
            let active =
                windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible(hwnd as _) != 0;
            let controller = webview.controller();
            let mut visible = Default::default();
            if controller.IsVisible(&mut visible).is_err() || visible.as_bool() != active {
                // Do not invalidate/repaint an already-correct controller on the
                // low-frequency watchdog; only repair a real mismatch.
                let _ = controller.SetIsVisible(active);
            }
        });
    }
    #[cfg(not(windows))]
    {
        let active = window.is_visible().unwrap_or(false);
        let _ = window.emit_to(window.label(), "window-activity-changed", active);
    }
}

pub fn hide(window: &WebviewWindow) -> tauri::Result<()> {
    if window.label() == "taskbar" {
        let result = window.hide();
        sync_taskbar(window);
        return result;
    }
    set_active(window, false);
    let result = window.hide();
    if result.is_err() {
        set_active(window, window.is_visible().unwrap_or(false));
    }
    result
}

pub fn show(window: &WebviewWindow) -> tauri::Result<()> {
    if window.label() == "taskbar" {
        let result = window.show();
        sync_taskbar(window);
        return result;
    }
    set_active(window, true);
    let result = window.show();
    if result.is_err() {
        set_active(window, false);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{popup_memory_target, PopupMemoryTarget};

    #[test]
    fn hidden_popups_request_low_memory_and_restore_normal_on_show() {
        for label in ["detail", "settings", "menu"] {
            assert_eq!(
                popup_memory_target(label, false),
                Some(PopupMemoryTarget::Low)
            );
            assert_eq!(
                popup_memory_target(label, true),
                Some(PopupMemoryTarget::Normal)
            );
        }
    }

    #[test]
    fn taskbar_and_unknown_windows_never_receive_popup_memory_policy() {
        for label in ["taskbar", "other"] {
            assert_eq!(popup_memory_target(label, false), None);
            assert_eq!(popup_memory_target(label, true), None);
        }
    }
}
