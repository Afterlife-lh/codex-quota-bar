mod models;
mod quota;
mod settings;
mod taskbar;
#[cfg(test)]
mod visual;

use models::QuotaSnapshot;
use quota::QuotaService;
use settings::AppSettings;
use std::{
    path::PathBuf,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{
    menu::MenuBuilder,
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tokio::sync::{Mutex, RwLock};

pub struct AppState {
    quota: QuotaService,
    pub(crate) settings: RwLock<AppSettings>,
    snapshot: RwLock<QuotaSnapshot>,
    refresh_lock: Mutex<()>,
    settings_path: PathBuf,
    last_reset_trigger: Mutex<Option<i64>>,
    pending_quota_jump: Mutex<Option<QuotaSnapshot>>,
    quota_confirmation_due: Mutex<Option<i64>>,
}

impl AppState {
    fn new(quota: QuotaService, settings: AppSettings, settings_path: PathBuf) -> Self {
        Self {
            quota,
            settings: RwLock::new(settings),
            snapshot: RwLock::new(QuotaSnapshot::default()),
            refresh_lock: Mutex::new(()),
            settings_path,
            last_reset_trigger: Mutex::new(None),
            pending_quota_jump: Mutex::new(None),
            quota_confirmation_due: Mutex::new(None),
        }
    }
}

async fn perform_refresh(app: &AppHandle, state: &Arc<AppState>) -> QuotaSnapshot {
    let _guard = state.refresh_lock.lock().await;
    let settings = state.settings.read().await.clone();
    let previous = state.snapshot.read().await.clone();
    let candidate = state.quota.refresh(&settings, &previous).await;
    let next = if quota::is_suspicious_premature_reset(&previous, &candidate, now_seconds() * 1000)
    {
        let mut pending = state.pending_quota_jump.lock().await;
        if pending
            .as_ref()
            .is_some_and(|value| quota::same_quota_values(value, &candidate))
        {
            *pending = None;
            *state.quota_confirmation_due.lock().await = None;
            candidate
        } else {
            *pending = Some(candidate);
            *state.quota_confirmation_due.lock().await = Some(now_seconds() + 3);
            previous
        }
    } else {
        *state.pending_quota_jump.lock().await = None;
        *state.quota_confirmation_due.lock().await = None;
        candidate
    };
    *state.snapshot.write().await = next.clone();
    let _ = app.emit("quota-updated", &next);
    next
}

#[tauri::command]
async fn get_status(state: State<'_, Arc<AppState>>) -> Result<QuotaSnapshot, String> {
    Ok(state.snapshot.read().await.clone())
}

#[tauri::command]
async fn refresh_now(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<QuotaSnapshot, String> {
    Ok(perform_refresh(&app, &state).await)
}

#[tauri::command]
async fn get_settings(state: State<'_, Arc<AppState>>) -> Result<AppSettings, String> {
    Ok(state.settings.read().await.clone())
}

#[tauri::command(rename_all = "camelCase")]
async fn save_settings(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    settings: AppSettings,
) -> Result<AppSettings, String> {
    let settings = settings.normalized();
    settings::save(&state.settings_path, &settings)?;
    *state.settings.write().await = settings.clone();
    taskbar::position_widget(&app, &settings)?;
    let _ = app.emit("settings-updated", &settings);
    let _ = perform_refresh(&app, &state).await;
    Ok(settings)
}

#[tauri::command]
fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    if enabled {
        app.autolaunch().enable()
    } else {
        app.autolaunch().disable()
    }
    .map_err(|e| format!("无法更新开机启动设置: {e}"))
}

#[tauri::command]
fn show_detail(app: AppHandle) -> Result<(), String> {
    let _ = app.get_webview_window("menu").map(|w| w.hide());
    taskbar::show_detail(&app)
}

#[tauri::command]
fn show_menu(app: AppHandle) -> Result<(), String> {
    let _ = app.get_webview_window("detail").map(|w| w.hide());
    taskbar::show_menu(&app)
}

#[tauri::command]
fn show_settings(app: AppHandle) -> Result<(), String> {
    let _ = app.get_webview_window("detail").map(|window| window.hide());
    taskbar::show_settings(&app)
}

#[tauri::command]
fn get_windows_generation() -> &'static str {
    taskbar::windows_generation()
}

#[tauri::command]
fn hide_current_window(window: tauri::WebviewWindow) -> Result<(), String> {
    window.hide().map_err(|e| e.to_string())
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

fn spawn_background(app: AppHandle, state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(1));
        let mut last_periodic = 0_i64;
        let initial_settings = state.settings.read().await.clone();
        let mut last_auth_stamp = auth_stamp(&initial_settings);
        loop {
            tick.tick().await;
            let now = now_seconds();
            let settings = state.settings.read().await.clone();
            let stamp = auth_stamp(&settings);
            let auth_changed = stamp != last_auth_stamp;
            if auth_changed {
                last_auth_stamp = stamp;
            }

            let reset_at = state
                .snapshot
                .read()
                .await
                .windows
                .iter()
                .filter_map(|window| window.reset_at)
                .filter(|reset| *reset <= now * 1000)
                .max();
            let reset_due = if let Some(reset) = reset_at {
                let mut last = state.last_reset_trigger.lock().await;
                if *last != Some(reset) {
                    *last = Some(reset);
                    true
                } else {
                    false
                }
            } else {
                false
            };

            let confirmation_due = {
                let mut due = state.quota_confirmation_due.lock().await;
                if due.is_some_and(|deadline| deadline <= now) {
                    *due = None;
                    true
                } else {
                    false
                }
            };

            if last_periodic == 0
                || now - last_periodic >= 60
                || auth_changed
                || reset_due
                || confirmation_due
            {
                last_periodic = now;
                perform_refresh(&app, &state).await;
            }
        }
    });
}

fn auth_stamp(settings: &AppSettings) -> Option<(u64, u64)> {
    let metadata = std::fs::metadata(quota::auth_path(settings)).ok()?;
    let modified = metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_secs();
    Some((modified, metadata.len()))
}

fn now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let menu = MenuBuilder::new(app)
        .text("show", "显示额度详情")
        .text("settings", "个性化设置")
        .text("refresh", "立即刷新")
        .separator()
        .text("quit", "退出")
        .build()?;
    let mut builder = TrayIconBuilder::with_id("main-tray")
        .tooltip("Codex Quota Bar")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let _ = taskbar::show_detail(tray.app_handle());
            }
        })
        .on_menu_event(|app, event| match event.id.0.as_str() {
            "show" => {
                let _ = taskbar::show_detail(app);
            }
            "settings" => {
                let _ = taskbar::show_settings(app);
            }
            "refresh" => {
                let app = app.clone();
                let state = app.state::<Arc<AppState>>().inner().clone();
                tauri::async_runtime::spawn(async move {
                    perform_refresh(&app, &state).await;
                });
            }
            "quit" => app.exit(0),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            let _ = taskbar::show_detail(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            let settings_path = app.path().app_config_dir()?.join("settings.json");
            let settings = settings::load(&settings_path).normalized();
            if settings.autostart {
                let _ = app.handle().autolaunch().enable();
            }
            let quota = QuotaService::new().map_err(std::io::Error::other)?;
            let state = Arc::new(AppState::new(quota, settings.clone(), settings_path));
            app.manage(state.clone());
            taskbar::position_widget(app.handle(), &settings).ok();
            taskbar::spawn_reposition_loop(app.handle().clone(), state.clone());
            spawn_background(app.handle().clone(), state.clone());
            setup_tray(app)?;
            Ok(())
        })
        .on_page_load(|webview, payload| {
            if webview.label() != "taskbar"
                || payload.event() != tauri::webview::PageLoadEvent::Finished
            {
                return;
            }
            let app = webview.app_handle().clone();
            tauri::async_runtime::spawn(async move {
                // Give React one event-loop turn to paint before revealing the transparent widget.
                tokio::time::sleep(std::time::Duration::from_millis(80)).await;
                let state = app.state::<Arc<AppState>>().inner().clone();
                let settings = state.settings.read().await.clone();
                let _ = taskbar::show_widget(&app, &settings);
            });
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
            if let tauri::WindowEvent::Focused(false) = event {
                if window.label() == "menu" {
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            refresh_now,
            get_settings,
            save_settings,
            set_autostart,
            show_detail,
            show_menu,
            show_settings,
            get_windows_generation,
            hide_current_window,
            quit_app
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Codex Quota Bar");
}
