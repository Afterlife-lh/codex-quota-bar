use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskbarSide {
    Left,
    Right,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub codex_home: Option<String>,
    pub display_width: f64,
    pub display_height: f64,
    pub horizontal_offset: f64,
    pub vertical_offset: f64,
    pub font_scale: f64,
    pub ring_size: f64,
    pub show_countdown: bool,
    pub animations: bool,
    pub follow_system_theme: bool,
    pub autostart: bool,
    pub coordinate_lyricify: bool,
    pub taskbar_region: TaskbarSide,
    pub window_alignment: TaskbarSide,
    pub reverse_layout: bool,
    pub auto_update: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            codex_home: None,
            display_width: 218.0,
            display_height: 42.0,
            horizontal_offset: 0.0,
            vertical_offset: 0.0,
            font_scale: 1.0,
            ring_size: 28.0,
            show_countdown: true,
            animations: true,
            follow_system_theme: true,
            autostart: true,
            coordinate_lyricify: true,
            taskbar_region: TaskbarSide::Right,
            window_alignment: TaskbarSide::Right,
            reverse_layout: false,
            auto_update: true,
        }
    }
}

impl AppSettings {
    pub fn normalized(mut self) -> Self {
        self.codex_home = self
            .codex_home
            .take()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        self.display_width = self.display_width.clamp(140.0, 420.0);
        self.display_height = self.display_height.clamp(30.0, 72.0);
        self.horizontal_offset = self.horizontal_offset.clamp(-240.0, 240.0);
        self.vertical_offset = self.vertical_offset.clamp(-48.0, 48.0);
        self.font_scale = self.font_scale.clamp(0.75, 1.6);
        self.ring_size = self.ring_size.clamp(18.0, 42.0);
        self
    }

    pub fn codex_home_path(&self) -> PathBuf {
        if let Some(path) = self.codex_home.as_deref() {
            return PathBuf::from(path);
        }
        if let Some(path) = std::env::var_os("CODEX_HOME").filter(|p| !p.is_empty()) {
            return PathBuf::from(path);
        }
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".codex")
    }
}

pub fn load(path: &Path) -> AppSettings {
    fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn save(path: &Path, settings: &AppSettings) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("无法创建设置目录: {e}"))?;
    }
    let temporary = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(settings).map_err(|e| format!("无法序列化设置: {e}"))?;
    fs::write(&temporary, bytes).map_err(|e| format!("无法保存设置: {e}"))?;
    if path.exists() {
        fs::remove_file(path).map_err(|e| format!("无法替换旧设置: {e}"))?;
    }
    fs::rename(&temporary, path).map_err(|e| format!("无法提交设置: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_settings_receive_new_personalization_defaults() {
        let legacy = r#"{"displayWidth":240,"fontScale":1.1,"animations":true,"followSystemTheme":true,"autostart":true}"#;
        let parsed: AppSettings =
            serde_json::from_str(legacy).expect("legacy settings should migrate");
        assert_eq!(parsed.display_width, 240.0);
        assert_eq!(parsed.display_height, 42.0);
        assert_eq!(parsed.ring_size, 28.0);
        assert!(parsed.coordinate_lyricify);
        assert_eq!(parsed.taskbar_region, TaskbarSide::Right);
        assert_eq!(parsed.window_alignment, TaskbarSide::Right);
        assert!(!parsed.reverse_layout);
        assert!(parsed.auto_update);
    }
}
