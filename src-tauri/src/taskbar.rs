use crate::settings::{AppSettings, TaskbarSide};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Position, Size};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}
impl Rect {
    fn width(self) -> i32 {
        self.right - self.left
    }
    fn height(self) -> i32 {
        self.bottom - self.top
    }
}

#[derive(Debug, Clone, Copy)]
struct TaskbarGeometry {
    taskbar: Rect,
    tray: Option<Rect>,
    task_buttons: Option<Rect>,
    windows_11: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WidgetTarget {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

fn aligned_x(
    region_start: i32,
    region_end: i32,
    width: i32,
    gap: i32,
    alignment: TaskbarSide,
) -> i32 {
    let left = region_start + gap;
    let right = (region_end - width - gap).max(left);
    match alignment {
        TaskbarSide::Left => left,
        TaskbarSide::Right => right,
    }
}

fn effective_layout(
    windows_11: bool,
    region: TaskbarSide,
    alignment: TaskbarSide,
) -> (TaskbarSide, TaskbarSide) {
    if windows_11 {
        (region, alignment)
    } else {
        (TaskbarSide::Right, TaskbarSide::Right)
    }
}

fn ease_out_cubic(progress: f64) -> f64 {
    let progress = progress.clamp(0.0, 1.0);
    1.0 - (1.0 - progress).powi(3)
}

fn covers_primary_screen(rect: Rect, width: i32, height: i32) -> bool {
    width > 0
        && height > 0
        && rect.left <= 1
        && rect.top <= 1
        && rect.right >= width - 1
        && rect.bottom >= height - 1
}

#[cfg(windows)]
mod platform {
    use super::Rect;
    use std::{collections::HashSet, iter};
    use windows_sys::Win32::{
        Foundation::{CloseHandle, HWND, INVALID_HANDLE_VALUE, LPARAM, RECT},
        System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        },
        UI::WindowsAndMessaging::{
            EnumChildWindows, EnumWindows, FindWindowExW, FindWindowW, GetClassNameW, GetCursorPos,
            GetForegroundWindow, GetSystemMetrics, GetWindowLongPtrW, GetWindowRect,
            GetWindowThreadProcessId, IsWindowVisible, SetWindowLongPtrW, SetWindowPos, ShowWindow,
            GWL_EXSTYLE, HWND_TOPMOST, SM_CXSCREEN, SM_CYSCREEN, SWP_NOACTIVATE, SWP_NOMOVE,
            SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, SW_HIDE, SW_SHOWNOACTIVATE, WS_EX_APPWINDOW,
            WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
        },
    };

    #[repr(C)]
    struct RtlOsVersionInfo {
        size: u32,
        major: u32,
        minor: u32,
        build: u32,
        platform: u32,
        service_pack: [u16; 128],
    }

    #[link(name = "ntdll")]
    extern "system" {
        fn RtlGetVersion(version: *mut RtlOsVersionInfo) -> i32;
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(iter::once(0)).collect()
    }
    fn rect(hwnd: HWND) -> Option<Rect> {
        let mut value = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        (unsafe { GetWindowRect(hwnd, &mut value) } != 0).then_some(Rect {
            left: value.left,
            top: value.top,
            right: value.right,
            bottom: value.bottom,
        })
    }
    unsafe extern "system" fn find_task_buttons(hwnd: HWND, parameter: LPARAM) -> i32 {
        let found = &mut *(parameter as *mut Option<Rect>);
        let mut class_name = [0_u16; 128];
        let length = GetClassNameW(hwnd, class_name.as_mut_ptr(), class_name.len() as i32);
        if length > 0 {
            let name = String::from_utf16_lossy(&class_name[..length as usize]);
            if matches!(name.as_str(), "MSTaskListWClass" | "MSTaskSwWClass") {
                if let Some(candidate) = rect(hwnd) {
                    *found = Some(candidate);
                    if name == "MSTaskListWClass" {
                        return 0;
                    }
                }
            }
        }
        1
    }

    fn is_windows_11() -> bool {
        let mut version = RtlOsVersionInfo {
            size: std::mem::size_of::<RtlOsVersionInfo>() as u32,
            major: 0,
            minor: 0,
            build: 0,
            platform: 0,
            service_pack: [0; 128],
        };
        unsafe { RtlGetVersion(&mut version) == 0 && version.build >= 22_000 }
    }

    pub fn geometry() -> Option<super::TaskbarGeometry> {
        let taskbar = unsafe { FindWindowW(wide("Shell_TrayWnd").as_ptr(), std::ptr::null()) };
        if taskbar.is_null() {
            return None;
        }
        let tray = unsafe {
            FindWindowExW(
                taskbar,
                std::ptr::null_mut(),
                wide("TrayNotifyWnd").as_ptr(),
                std::ptr::null(),
            )
        };
        let mut task_buttons = None;
        unsafe {
            EnumChildWindows(
                taskbar,
                Some(find_task_buttons),
                &mut task_buttons as *mut _ as LPARAM,
            );
        }
        Some(super::TaskbarGeometry {
            taskbar: rect(taskbar)?,
            tray: (!tray.is_null())
                .then(|| rect(tray))
                .flatten()
                .filter(|candidate| candidate.width() > 0 && candidate.height() > 0),
            task_buttons,
            windows_11: is_windows_11(),
        })
    }
    pub fn cursor() -> (i32, i32) {
        let mut point = windows_sys::Win32::Foundation::POINT { x: 0, y: 0 };
        unsafe {
            GetCursorPos(&mut point);
        }
        (point.x, point.y)
    }

    #[derive(Debug, Clone, Copy)]
    pub struct ExternalWindow {
        pub hwnd: isize,
        pub rect: Rect,
    }

    struct SearchContext<'a> {
        pids: &'a HashSet<u32>,
        taskbar: Rect,
        found: Option<ExternalWindow>,
    }

    unsafe extern "system" fn enum_window(hwnd: HWND, parameter: LPARAM) -> i32 {
        let context = &mut *(parameter as *mut SearchContext<'_>);
        if IsWindowVisible(hwnd) == 0 {
            return 1;
        }
        let mut pid = 0_u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if !context.pids.contains(&pid) {
            return 1;
        }
        let Some(candidate) = rect(hwnd) else {
            return 1;
        };
        let horizontal_overlap =
            candidate.right > context.taskbar.left && candidate.left < context.taskbar.right;
        let vertical_overlap =
            candidate.bottom > context.taskbar.top && candidate.top < context.taskbar.bottom;
        let plausible =
            candidate.width() >= 40 && candidate.height() >= 20 && candidate.width() <= 1600;
        if horizontal_overlap && vertical_overlap && plausible {
            if context
                .found
                .is_none_or(|current| candidate.right > current.rect.right)
            {
                context.found = Some(ExternalWindow {
                    hwnd: hwnd as isize,
                    rect: candidate,
                });
            }
        }
        1
    }

    pub fn lyricify_window(taskbar: Rect) -> Option<ExternalWindow> {
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snapshot == INVALID_HANDLE_VALUE {
            return None;
        }
        let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut pids = HashSet::new();
        let mut has_entry = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
        while has_entry {
            let end = entry
                .szExeFile
                .iter()
                .position(|value| *value == 0)
                .unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..end]).to_ascii_lowercase();
            if name.contains("lyricify") {
                pids.insert(entry.th32ProcessID);
            }
            has_entry = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
        }
        unsafe {
            CloseHandle(snapshot);
        }
        if pids.is_empty() {
            return None;
        }
        let mut context = SearchContext {
            pids: &pids,
            taskbar,
            found: None,
        };
        unsafe {
            EnumWindows(Some(enum_window), &mut context as *mut _ as LPARAM);
        }
        context.found
    }

    pub fn move_window(window: ExternalWindow, x: i32, y: i32) {
        unsafe {
            SetWindowPos(
                window.hwnd as HWND,
                std::ptr::null_mut(),
                x,
                y,
                0,
                0,
                SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER,
            );
        }
    }

    pub fn prepare_widget(hwnd: HWND) {
        unsafe {
            let styles = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            let styles = (styles & !(WS_EX_APPWINDOW as isize))
                | WS_EX_TOOLWINDOW as isize
                | WS_EX_NOACTIVATE as isize;
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, styles);
            SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }

    pub fn show_widget(hwnd: HWND) {
        unsafe {
            prepare_widget(hwnd);
            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
    }

    pub fn keep_widget_visible(hwnd: HWND) {
        unsafe {
            // Explorer can reorder topmost taskbar windows whenever a task button
            // is activated. Reassert our place in that band with one cheap call;
            // unlike show_widget this does not rewrite styles or call ShowWindow.
            SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
    }

    pub fn hide_widget(hwnd: HWND) {
        unsafe {
            ShowWindow(hwnd, SW_HIDE);
        }
    }

    pub fn should_hide_widget() -> bool {
        let taskbar = unsafe { FindWindowW(wide("Shell_TrayWnd").as_ptr(), std::ptr::null()) };
        if taskbar.is_null() || unsafe { IsWindowVisible(taskbar) } == 0 {
            return true;
        }
        let Some(taskbar_rect) = rect(taskbar) else {
            return true;
        };
        let (screen_width, screen_height) =
            unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
        let taskbar_on_screen = taskbar_rect.right > 0
            && taskbar_rect.bottom > 0
            && taskbar_rect.left < screen_width
            && taskbar_rect.top < screen_height
            && taskbar_rect.width().abs().min(taskbar_rect.height().abs()) >= 8;
        if !taskbar_on_screen {
            return true;
        }
        let foreground = unsafe { GetForegroundWindow() };
        if foreground.is_null() || foreground == taskbar {
            return false;
        }
        let mut class_name = [0_u16; 128];
        let length =
            unsafe { GetClassNameW(foreground, class_name.as_mut_ptr(), class_name.len() as i32) };
        let class_name = String::from_utf16_lossy(&class_name[..length.max(0) as usize]);
        if matches!(class_name.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd") {
            return false;
        }
        rect(foreground)
            .is_some_and(|value| super::covers_primary_screen(value, screen_width, screen_height))
    }

    pub fn move_widget(hwnd: HWND, x: i32, y: i32) {
        unsafe {
            SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                x,
                y,
                0,
                0,
                SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::Rect;
    #[derive(Debug, Clone, Copy)]
    pub struct ExternalWindow {
        pub rect: Rect,
    }
    pub fn geometry() -> Option<super::TaskbarGeometry> {
        None
    }
    pub fn cursor() -> (i32, i32) {
        (0, 0)
    }
    pub fn lyricify_window(_: Rect) -> Option<ExternalWindow> {
        None
    }
    pub fn move_window(_: ExternalWindow, _: i32, _: i32) {}
    pub fn prepare_widget(_: *mut std::ffi::c_void) {}
    pub fn show_widget(_: *mut std::ffi::c_void) {}
    pub fn keep_widget_visible(_: *mut std::ffi::c_void) {}
    pub fn hide_widget(_: *mut std::ffi::c_void) {}
    pub fn should_hide_widget() -> bool {
        false
    }
    pub fn move_widget(_: *mut std::ffi::c_void, _: i32, _: i32) {}
}

pub fn windows_generation() -> &'static str {
    if platform::geometry().is_some_and(|geometry| geometry.windows_11) {
        "windows11"
    } else {
        "windows10"
    }
}

fn prepare_native_widget(window: &tauri::WebviewWindow) -> Result<(), String> {
    let hwnd = window.hwnd().map_err(|error| error.to_string())?;
    platform::prepare_widget(hwnd.0);
    Ok(())
}

fn widget_target(
    app: &AppHandle,
    settings: &AppSettings,
    lyricify: Option<platform::ExternalWindow>,
    move_lyricify: bool,
) -> Result<WidgetTarget, String> {
    let window = app
        .get_webview_window("taskbar")
        .ok_or("taskbar window missing")?;
    let scale = window.scale_factor().unwrap_or(1.0);
    let width = (settings.display_width * scale).round() as i32;
    let geometry = platform::geometry()
        .or_else(|| fallback_geometry(app))
        .ok_or("cannot locate primary taskbar")?;
    let taskbar = geometry.taskbar;
    let horizontal = taskbar.width().abs() >= taskbar.height().abs();
    let requested_height = (settings.display_height * scale).round() as i32;
    let desired_height = if horizontal {
        requested_height.min(taskbar.height()).max(28)
    } else {
        requested_height.max(28)
    };
    let (x, y) = if horizontal {
        let tray_edge = geometry
            .tray
            .map(|r| r.left)
            .unwrap_or(taskbar.right - (190.0 * scale) as i32);
        let gap = (6.0 * scale) as i32;
        let buttons = geometry.task_buttons;
        // Windows 10 keeps the established tray-left placement by default.
        let (region, alignment) = effective_layout(
            geometry.windows_11,
            settings.taskbar_region,
            settings.window_alignment,
        );
        let (region_start, region_end) = match region {
            TaskbarSide::Left => (
                taskbar.left,
                buttons.map(|rect| rect.left).unwrap_or(tray_edge),
            ),
            TaskbarSide::Right => (
                buttons.map(|rect| rect.right).unwrap_or(taskbar.left),
                tray_edge,
            ),
        };
        let min_x = aligned_x(region_start, region_end, width, gap, TaskbarSide::Left);
        let max_x = aligned_x(region_start, region_end, width, gap, TaskbarSide::Right);
        let mut widget_x = aligned_x(region_start, region_end, width, gap, alignment);
        if let Some(lyricify) = lyricify {
            if region == TaskbarSide::Left && alignment == TaskbarSide::Left {
                let lyricify_x = min_x;
                widget_x = (lyricify_x + lyricify.rect.width() + gap).min(max_x);
                let lyricify_y = taskbar.top + (taskbar.height() - lyricify.rect.height()) / 2;
                if move_lyricify {
                    platform::move_window(lyricify, lyricify_x, lyricify_y);
                }
            } else {
                let lyricify_x = widget_x - lyricify.rect.width() - gap;
                let lyricify_y = taskbar.top + (taskbar.height() - lyricify.rect.height()) / 2;
                if move_lyricify {
                    platform::move_window(lyricify, lyricify_x, lyricify_y);
                }
            }
        }
        widget_x += (settings.horizontal_offset * scale) as i32;
        (
            widget_x,
            taskbar.top
                + (taskbar.height() - desired_height) / 2
                + (settings.vertical_offset * scale) as i32,
        )
    } else {
        let tray_edge = geometry
            .tray
            .map(|r| r.top)
            .unwrap_or(taskbar.bottom - (190.0 * scale) as i32);
        (
            taskbar.left
                + (taskbar.width() - width.min(taskbar.width())) / 2
                + (settings.horizontal_offset * scale) as i32,
            tray_edge - desired_height - 4 + (settings.vertical_offset * scale) as i32,
        )
    };
    Ok(WidgetTarget {
        x,
        y,
        width: width as u32,
        height: desired_height as u32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aligns_widget_within_selected_region() {
        assert_eq!(aligned_x(0, 800, 200, 6, TaskbarSide::Left), 6);
        assert_eq!(aligned_x(0, 800, 200, 6, TaskbarSide::Right), 594);
    }

    #[test]
    fn narrow_region_never_places_widget_before_its_start() {
        assert_eq!(aligned_x(500, 600, 200, 6, TaskbarSide::Right), 506);
    }

    #[test]
    fn windows_10_forces_tray_left_while_windows_11_keeps_preferences() {
        assert_eq!(
            effective_layout(false, TaskbarSide::Left, TaskbarSide::Left),
            (TaskbarSide::Right, TaskbarSide::Right)
        );
        assert_eq!(
            effective_layout(true, TaskbarSide::Left, TaskbarSide::Right),
            (TaskbarSide::Left, TaskbarSide::Right)
        );
    }

    #[test]
    fn movement_easing_has_stable_endpoints() {
        assert_eq!(ease_out_cubic(0.0), 0.0);
        assert_eq!(ease_out_cubic(1.0), 1.0);
        assert!(ease_out_cubic(0.5) > 0.5);
    }

    #[test]
    fn detects_primary_fullscreen_bounds() {
        assert!(covers_primary_screen(
            Rect {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            },
            1920,
            1080
        ));
        assert!(!covers_primary_screen(
            Rect {
                left: 0,
                top: 0,
                right: 1600,
                bottom: 900,
            },
            1920,
            1080
        ));
    }
}

fn apply_target(window: &tauri::WebviewWindow, target: WidgetTarget) -> Result<(), String> {
    window
        .set_size(Size::Physical(PhysicalSize::new(
            target.width,
            target.height,
        )))
        .map_err(|e| e.to_string())?;
    window
        .set_position(Position::Physical(PhysicalPosition::new(
            target.x, target.y,
        )))
        .map_err(|e| e.to_string())?;
    prepare_native_widget(&window)?;
    Ok(())
}

pub fn position_widget(app: &AppHandle, settings: &AppSettings) -> Result<(), String> {
    let window = app
        .get_webview_window("taskbar")
        .ok_or("taskbar window missing")?;
    let taskbar = platform::geometry().map(|geometry| geometry.taskbar);
    let lyricify = taskbar.and_then(platform::lyricify_window);
    apply_target(&window, widget_target(app, settings, lyricify, true)?)
}

pub fn show_widget(app: &AppHandle, settings: &AppSettings) -> Result<(), String> {
    // Explorer lays out TrayNotifyWnd asynchronously during sign-in. Keep the
    // widget hidden until that real boundary exists instead of flashing at an
    // estimated position; the 60 Hz reposition loop will reveal it precisely.
    if platform::geometry().is_none_or(|geometry| geometry.tray.is_none()) {
        return Ok(());
    }
    position_widget(app, settings)?;
    let window = app
        .get_webview_window("taskbar")
        .ok_or("taskbar window missing")?;
    let hwnd = window.hwnd().map_err(|error| error.to_string())?;
    platform::show_widget(hwnd.0);
    Ok(())
}

pub fn show_detail(app: &AppHandle) -> Result<(), String> {
    let anchor = app
        .get_webview_window("taskbar")
        .ok_or("taskbar window missing")?;
    let detail = app
        .get_webview_window("detail")
        .ok_or("detail window missing")?;
    let position = anchor.outer_position().map_err(|e| e.to_string())?;
    let size = anchor.outer_size().map_err(|e| e.to_string())?;
    let detail_size = detail.outer_size().map_err(|e| e.to_string())?;
    let taskbar = platform::geometry()
        .map(|value| value.taskbar)
        .unwrap_or_default();
    let y = if taskbar.top > 0 {
        position.y - detail_size.height as i32 - 10
    } else {
        position.y + size.height as i32 + 10
    };
    let x = (position.x + size.width as i32 - detail_size.width as i32).max(8);
    detail
        .set_position(Position::Physical(PhysicalPosition::new(x, y.max(8))))
        .map_err(|e| e.to_string())?;
    app.emit_to("detail", "prepare-detail-show", ())
        .map_err(|e| e.to_string())?;
    let detail_for_show = detail.clone();
    tauri::async_runtime::spawn(async move {
        // Let the hidden WebView apply its entering state before Windows paints it.
        tokio::time::sleep(std::time::Duration::from_millis(34)).await;
        let _ = detail_for_show
            .show()
            .and_then(|_| detail_for_show.set_focus());
    });
    Ok(())
}

pub fn show_settings(app: &AppHandle) -> Result<(), String> {
    let anchor = app
        .get_webview_window("taskbar")
        .ok_or("taskbar window missing")?;
    let settings = app
        .get_webview_window("settings")
        .ok_or("settings window missing")?;
    let position = anchor.outer_position().map_err(|e| e.to_string())?;
    let size = anchor.outer_size().map_err(|e| e.to_string())?;
    let settings_size = settings.outer_size().map_err(|e| e.to_string())?;
    let taskbar = platform::geometry()
        .map(|value| value.taskbar)
        .unwrap_or_default();
    let y = if taskbar.top > 0 {
        position.y - settings_size.height as i32 - 10
    } else {
        position.y + size.height as i32 + 10
    };
    let x = (position.x + size.width as i32 - settings_size.width as i32).max(8);
    settings
        .set_position(Position::Physical(PhysicalPosition::new(x, y.max(8))))
        .map_err(|e| e.to_string())?;
    settings
        .show()
        .and_then(|_| settings.set_focus())
        .map_err(|e| e.to_string())
}

pub fn show_menu(app: &AppHandle) -> Result<(), String> {
    let menu = app
        .get_webview_window("menu")
        .ok_or("menu window missing")?;
    let size = menu.outer_size().map_err(|e| e.to_string())?;
    let (x, y) = platform::cursor();
    let taskbar = platform::geometry()
        .map(|value| value.taskbar)
        .unwrap_or_default();
    let menu_y = if taskbar.top > 0 {
        y - size.height as i32 - 8
    } else {
        y + 8
    };
    menu.set_position(Position::Physical(PhysicalPosition::new(
        x - size.width as i32,
        menu_y.max(8),
    )))
    .map_err(|e| e.to_string())?;
    menu.show()
        .and_then(|_| menu.set_focus())
        .map_err(|e| e.to_string())
}

fn fallback_geometry(app: &AppHandle) -> Option<TaskbarGeometry> {
    let monitor = app.primary_monitor().ok().flatten()?;
    let size = monitor.size();
    Some(TaskbarGeometry {
        taskbar: Rect {
            left: 0,
            top: size.height as i32 - 48,
            right: size.width as i32,
            bottom: size.height as i32,
        },
        tray: None,
        task_buttons: None,
        windows_11: false,
    })
}

pub fn spawn_reposition_loop(app: AppHandle, state: std::sync::Arc<crate::AppState>) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let Some(window) = app.get_webview_window("taskbar") else {
            return;
        };
        let Ok(hwnd) = window.hwnd() else {
            return;
        };
        let hwnd = hwnd.0 as isize;
        let mut lyricify = None;
        let mut last_lyricify_scan = std::time::Instant::now()
            .checked_sub(std::time::Duration::from_secs(1))
            .unwrap_or_else(std::time::Instant::now);
        let mut last_target: Option<WidgetTarget> = None;
        let mut animation_from = (0_i32, 0_i32);
        let mut animation_started = std::time::Instant::now();
        let mut suppressed = false;
        let mut shown = false;
        let mut last_applied_position: Option<(i32, i32)> = None;
        loop {
            if platform::should_hide_widget() {
                if !suppressed {
                    platform::hide_widget(hwnd as _);
                    for label in ["detail", "menu", "settings"] {
                        if let Some(auxiliary) = app.get_webview_window(label) {
                            let _ = auxiliary.hide();
                        }
                    }
                }
                suppressed = true;
                shown = false;
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                continue;
            }
            suppressed = false;
            let settings = state.settings.read().await.clone();
            let now = std::time::Instant::now();
            let coordinate_lyricify =
                now.duration_since(last_lyricify_scan) >= settings.polling_mode.lyricify_interval();
            if coordinate_lyricify {
                lyricify = platform::geometry()
                    .map(|geometry| geometry.taskbar)
                    .and_then(platform::lyricify_window);
                last_lyricify_scan = now;
            }

            if let Ok(target) = widget_target(&app, &settings, lyricify, coordinate_lyricify) {
                if last_target != Some(target) {
                    let current = window
                        .outer_position()
                        .unwrap_or(PhysicalPosition::new(target.x, target.y));
                    animation_from = (current.x, current.y);
                    animation_started = now;
                    last_target = Some(target);
                }

                if window
                    .outer_size()
                    .is_ok_and(|size| size.width != target.width || size.height != target.height)
                {
                    let _ = window.set_size(Size::Physical(PhysicalSize::new(
                        target.width,
                        target.height,
                    )));
                }

                let elapsed = now.duration_since(animation_started).as_secs_f64();
                let duration = 0.18_f64;
                let progress = if settings.animations {
                    (elapsed / duration).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                let eased = ease_out_cubic(progress);
                let x = animation_from.0
                    + ((target.x - animation_from.0) as f64 * eased).round() as i32;
                let y = animation_from.1
                    + ((target.y - animation_from.1) as f64 * eased).round() as i32;
                if last_applied_position != Some((x, y)) {
                    platform::move_widget(hwnd as _, x, y);
                    last_applied_position = Some((x, y));
                }
                let animating = settings.animations
                    && progress < 1.0
                    && animation_from != (target.x, target.y);
                if !shown {
                    platform::show_widget(hwnd as _);
                    shown = true;
                } else if !animating {
                    // A low-frequency Z-order keepalive repairs Explorer taskbar
                    // reordering without returning to the former 60Hz Win32 loop.
                    platform::keep_widget_visible(hwnd as _);
                }

                let interval = if animating {
                    std::time::Duration::from_millis(16)
                } else {
                    settings.polling_mode.idle_interval()
                };
                tokio::time::sleep(interval).await;
                continue;
            }
            tokio::time::sleep(settings.polling_mode.idle_interval()).await;
        }
    });
}
