//! App-level settings, independent from the main store so users can hand-edit.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::store::{load_or_default, save_json_atomic};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeMode {
    #[default]
    System,
    Dark,
    Light,
}

impl ThemeMode {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Dark,
            2 => Self::Light,
            _ => Self::System,
        }
    }

    pub fn as_u8(self) -> u8 {
        match self {
            Self::System => 0,
            Self::Dark => 1,
            Self::Light => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Language {
    #[default]
    System,
    Zh,
    En,
}

impl Language {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Zh,
            2 => Self::En,
            _ => Self::System,
        }
    }

    pub fn as_u8(self) -> u8 {
        match self {
            Self::System => 0,
            Self::Zh => 1,
            Self::En => 2,
        }
    }

    /// Effective language, resolving System against the OS locale.
    pub fn effective(self) -> Self {
        match self {
            Self::System => {
                if system_is_chinese() {
                    Self::Zh
                } else {
                    Self::En
                }
            }
            other => other,
        }
    }
}

/// Cheap locale sniff: Windows GetUserDefaultUILanguage; elsewhere LANG env.
pub fn system_is_chinese() -> bool {
    #[cfg(target_os = "windows")]
    {
        // 0x0004 zh-CN family; primary language id low 10 bits
        let lang = unsafe { windows_user_default_ui_language() };
        (lang & 0x03FF) == 0x0004
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("LANG")
            .map(|v| v.to_ascii_lowercase().starts_with("zh"))
            .unwrap_or(false)
    }
}

#[cfg(target_os = "windows")]
unsafe fn windows_user_default_ui_language() -> u16 {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetUserDefaultUILanguage() -> u16;
    }
    unsafe { GetUserDefaultUILanguage() }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ProxyMode {
    Direct,
    Custom,
    #[default]
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionFilters {
    pub user: bool,
    pub assistant: bool,
    pub text: bool,
    pub thinking: bool,
    pub tool_call: bool,
    pub command: bool,
}

impl Default for SessionFilters {
    fn default() -> Self {
        Self {
            user: true,
            assistant: true,
            text: true,
            thinking: true,
            tool_call: true,
            command: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum BackupType {
    #[default]
    Local,
    Webdav,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebDavConfig {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default = "default_webdav_directory")]
    pub remote_directory: String,
}

fn default_webdav_directory() -> String {
    "aitoolplus".into()
}

impl Default for WebDavConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            username: String::new(),
            password: String::new(),
            remote_directory: default_webdav_directory(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupFileFilterRule {
    pub tool: String,
    pub file_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupCustomEntry {
    pub id: String,
    pub source_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restore_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default)]
    pub language: Language,
    #[serde(default)]
    pub theme_mode: ThemeMode,
    #[serde(default)]
    pub start_with_system: bool,
    #[serde(default = "default_true")]
    pub minimize_to_tray_on_close: bool,
    #[serde(default)]
    pub start_minimized: bool,
    #[serde(default)]
    pub proxy_mode: ProxyMode,
    #[serde(default)]
    pub proxy_url: String,
    #[serde(default)]
    pub backup_type: BackupType,
    #[serde(default)]
    pub webdav: WebDavConfig,
    #[serde(default = "default_true")]
    pub backup_cli_config_files_enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_backup_path: Option<String>,
    #[serde(default)]
    pub auto_backup_enabled: bool,
    #[serde(default = "default_backup_interval_days")]
    pub auto_backup_interval_days: u32,
    #[serde(default = "default_backup_max_keep")]
    pub auto_backup_max_keep: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_auto_backup_time: Option<String>,
    #[serde(default)]
    pub backup_custom_entries: Vec<BackupCustomEntry>,
    #[serde(default)]
    pub backup_file_filter_rules: Vec<BackupFileFilterRule>,
    #[serde(default = "default_true")]
    pub auto_update_check_enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_update_check_time: Option<String>,
    #[serde(default)]
    pub tool_root_overrides: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub cli_manual_paths: std::collections::BTreeMap<String, String>,
    #[serde(default = "default_visible_tools")]
    pub visible_tools: Vec<String>,
    #[serde(default)]
    pub session_filters: SessionFilters,
    #[serde(default)]
    pub claude_cli_launch_full_access: bool,
    #[serde(default)]
    pub codex_preserve_official_auth_on_switch: bool,
    #[serde(default)]
    pub opencode_use_legacy_oh_my_config: bool,
    #[serde(default)]
    pub opencode_allow_clear_applied_oh_my_config: bool,
    #[serde(default)]
    pub opencode_dual_write_reasoning_variant: bool,
    /// Remembered window bounds (x, y, w, h). Restored on launch.
    #[serde(default)]
    pub window_bounds: Option<(i32, i32, u32, u32)>,
    #[serde(default)]
    pub last_page: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            language: Language::System,
            theme_mode: ThemeMode::System,
            start_with_system: false,
            minimize_to_tray_on_close: true,
            start_minimized: false,
            proxy_mode: ProxyMode::System,
            proxy_url: String::new(),
            backup_type: BackupType::Local,
            webdav: WebDavConfig::default(),
            backup_cli_config_files_enabled: true,
            local_backup_path: None,
            auto_backup_enabled: false,
            auto_backup_interval_days: default_backup_interval_days(),
            auto_backup_max_keep: default_backup_max_keep(),
            last_auto_backup_time: None,
            backup_custom_entries: vec![],
            backup_file_filter_rules: vec![],
            auto_update_check_enabled: true,
            last_update_check_time: None,
            tool_root_overrides: Default::default(),
            cli_manual_paths: Default::default(),
            visible_tools: default_visible_tools(),
            session_filters: SessionFilters::default(),
            claude_cli_launch_full_access: false,
            codex_preserve_official_auth_on_switch: false,
            opencode_use_legacy_oh_my_config: false,
            opencode_allow_clear_applied_oh_my_config: false,
            opencode_dual_write_reasoning_variant: false,
            window_bounds: None,
            last_page: String::new(),
        }
    }
}

fn default_visible_tools() -> Vec<String> {
    crate::tools::ToolId::ALL
        .into_iter()
        .map(|tool| tool.key().to_string())
        .collect()
}

fn default_true() -> bool {
    true
}

fn default_backup_interval_days() -> u32 {
    7
}

fn default_backup_max_keep() -> u32 {
    10
}

impl AppSettings {
    pub fn load(path: &Path) -> Self {
        load_or_default::<Self>(path).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        save_json_atomic(path, self).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("settings.json");
        let mut s = AppSettings::load(&p);
        assert_eq!(s.language, Language::System);
        s.theme_mode = ThemeMode::Dark;
        s.last_page = "mcp".into();
        s.save(&p).unwrap();
        let back = AppSettings::load(&p);
        assert_eq!(back.theme_mode, ThemeMode::Dark);
        assert_eq!(back.last_page, "mcp");
    }

    #[test]
    fn enums_round_u8() {
        assert_eq!(ThemeMode::from_u8(ThemeMode::Dark.as_u8()), ThemeMode::Dark);
        assert_eq!(Language::from_u8(Language::En.as_u8()), Language::En);
    }
}
