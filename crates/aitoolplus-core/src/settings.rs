//! App-level settings, independent from the main store so users can hand-edit.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::store::{load_or_default, save_json_atomic};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
    System,
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
    #[default]
    Direct,
    Custom,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ProxyType {
    #[default]
    Direct,
    Http,
    Https,
    Socks5,
    Socks4,
    System,
}

impl ProxyType {
    pub fn is_custom(&self) -> bool {
        matches!(self, Self::Http | Self::Https | Self::Socks5 | Self::Socks4)
    }

    pub fn scheme(&self) -> &'static str {
        match self {
            Self::Direct => "",
            Self::Http => "http",
            Self::Https => "https",
            Self::Socks5 => "socks5",
            Self::Socks4 => "socks4",
            Self::System => "",
        }
    }

    pub fn label(&self, is_zh: bool) -> &'static str {
        match self {
            Self::Direct => if is_zh { "直接连接" } else { "Direct" },
            Self::Http => "HTTP",
            Self::Https => "HTTPS",
            Self::Socks5 => "SOCKS5",
            Self::Socks4 => "SOCKS4",
            Self::System => if is_zh { "跟随系统" } else { "System" },
        }
    }

    pub fn all() -> &'static [ProxyType] {
        &[
            ProxyType::Direct,
            ProxyType::Http,
            ProxyType::Https,
            ProxyType::Socks5,
            ProxyType::Socks4,
            ProxyType::System,
        ]
    }
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
    S3,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct S3Config {
    #[serde(default)]
    pub endpoint: String,
    #[serde(default = "default_s3_region")]
    pub region: String,
    #[serde(default)]
    pub bucket: String,
    #[serde(default)]
    pub access_key_id: String,
    #[serde(default)]
    pub secret_access_key: String,
    #[serde(default = "default_s3_prefix")]
    pub prefix: String,
    #[serde(default = "default_true")]
    pub path_style: bool,
}

fn default_s3_region() -> String {
    "us-east-1".into()
}

fn default_s3_prefix() -> String {
    "aitoolplus".into()
}

impl Default for S3Config {
    fn default() -> Self {
        Self {
            endpoint: String::new(),
            region: default_s3_region(),
            bucket: String::new(),
            access_key_id: String::new(),
            secret_access_key: String::new(),
            prefix: default_s3_prefix(),
            path_style: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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

/// Machine-local sync and remote backup destination settings (WebDAV, S3, etc.).
/// Kept strictly isolated in `sync.json` and never included in backup bundles or sync payloads.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncSettings {
    #[serde(default)]
    pub backup_type: BackupType,
    #[serde(default)]
    pub webdav: WebDavConfig,
    #[serde(default)]
    pub s3: S3Config,
}

impl Default for SyncSettings {
    fn default() -> Self {
        Self {
            backup_type: BackupType::Local,
            webdav: WebDavConfig::default(),
            s3: S3Config::default(),
        }
    }
}

impl SyncSettings {
    pub fn load(path: &Path) -> Self {
        let mut sync = load_or_default::<Self>(path).unwrap_or_default();
        sync.webdav.password = crate::security::unprotect_secret(&sync.webdav.password);
        sync.s3.secret_access_key = crate::security::unprotect_secret(&sync.s3.secret_access_key);
        sync
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let mut cloned = self.clone();
        cloned.webdav.password = crate::security::protect_secret(&cloned.webdav.password);
        cloned.s3.secret_access_key = crate::security::protect_secret(&cloned.s3.secret_access_key);
        save_json_atomic(path, &cloned).map_err(|e| e.to_string())
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
    pub proxy_type: ProxyType,
    #[serde(default = "default_proxy_host")]
    pub proxy_host: String,
    #[serde(default = "default_proxy_port")]
    pub proxy_port: String,
    #[serde(default)]
    pub proxy_url: String,
    #[serde(skip)]
    pub backup_type: BackupType,
    #[serde(skip)]
    pub webdav: WebDavConfig,
    #[serde(skip)]
    pub s3: S3Config,
    #[serde(default = "default_true")]
    pub backup_cli_config_files_enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_backup_path: Option<String>,
    #[serde(default = "default_true")]
    pub auto_backup_enabled: bool,
    #[serde(default = "default_backup_interval_hours")]
    pub backup_interval_hours: u32,
    #[serde(default = "default_backup_retain_count")]
    pub backup_retain_count: usize,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dismissed_update_version: Option<String>,
    #[serde(default = "default_update_mirror")]
    pub update_mirror: crate::updater::UpdateMirror,
    #[serde(default)]
    pub custom_update_mirror_url: String,
    #[serde(default)]
    pub custom_update_api_url: String,
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
    #[serde(default = "default_true")]
    pub antigravity_auto_refresh: bool,
    #[serde(default = "default_antigravity_refresh_interval")]
    pub antigravity_refresh_interval_minutes: u32,
    #[serde(default = "default_true")]
    pub antigravity_auto_sync: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_antigravity_refresh_time: Option<String>,
    /// Remembered window bounds (x, y, w, h). Restored on launch.
    #[serde(default)]
    pub window_bounds: Option<(i32, i32, u32, u32)>,
    #[serde(default = "default_true")]
    pub usage_auto_scan_sessions: bool,
    #[serde(default)]
    pub last_page: String,
}

fn default_antigravity_refresh_interval() -> u32 {
    15
}

pub fn default_proxy_host() -> String {
    "127.0.0.1".into()
}

pub fn default_proxy_port() -> String {
    "7890".into()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            language: Language::System,
            theme_mode: ThemeMode::System,
            start_with_system: false,
            minimize_to_tray_on_close: true,
            start_minimized: false,
            proxy_mode: ProxyMode::Direct,
            proxy_type: ProxyType::Direct,
            proxy_host: default_proxy_host(),
            proxy_port: default_proxy_port(),
            proxy_url: String::new(),
            backup_type: BackupType::Local,
            webdav: WebDavConfig::default(),
            s3: S3Config::default(),
            backup_cli_config_files_enabled: true,
            local_backup_path: None,
            auto_backup_enabled: true,
            backup_interval_hours: default_backup_interval_hours(),
            backup_retain_count: default_backup_retain_count(),
            auto_backup_interval_days: default_backup_interval_days(),
            auto_backup_max_keep: default_backup_max_keep(),
            last_auto_backup_time: None,
            backup_custom_entries: vec![],
            backup_file_filter_rules: vec![],
            auto_update_check_enabled: true,
            last_update_check_time: None,
            dismissed_update_version: None,
            update_mirror: default_update_mirror(),
            custom_update_mirror_url: String::new(),
            custom_update_api_url: String::new(),
            tool_root_overrides: Default::default(),
            cli_manual_paths: Default::default(),
            visible_tools: default_visible_tools(),
            session_filters: SessionFilters::default(),
            claude_cli_launch_full_access: false,
            codex_preserve_official_auth_on_switch: false,
            opencode_use_legacy_oh_my_config: false,
            opencode_allow_clear_applied_oh_my_config: false,
            opencode_dual_write_reasoning_variant: false,
            antigravity_auto_refresh: true,
            antigravity_refresh_interval_minutes: default_antigravity_refresh_interval(),
            antigravity_auto_sync: true,
            last_antigravity_refresh_time: None,
            window_bounds: None,
            usage_auto_scan_sessions: true,
            last_page: String::new(),
        }
    }
}

impl AppSettings {
    /// Compute the effective proxy URL from proxy_type, proxy_host, and proxy_port
    pub fn effective_proxy_url(&self) -> String {
        if !self.proxy_type.is_custom() {
            return String::new();
        }
        let scheme = self.proxy_type.scheme();
        let host = if self.proxy_host.trim().is_empty() {
            "127.0.0.1"
        } else {
            self.proxy_host.trim()
        };
        let port = if self.proxy_port.trim().is_empty() {
            "7890"
        } else {
            self.proxy_port.trim()
        };
        format!("{scheme}://{host}:{port}")
    }

    /// Synchronize proxy_mode, proxy_type, proxy_host, proxy_port, and proxy_url
    pub fn sync_proxy(&mut self) {
        if self.proxy_type.is_custom() {
            self.proxy_mode = ProxyMode::Custom;
            self.proxy_url = self.effective_proxy_url();
        } else if self.proxy_type == ProxyType::System {
            self.proxy_mode = ProxyMode::System;
            self.proxy_url = String::new();
        } else {
            self.proxy_mode = ProxyMode::Direct;
            self.proxy_type = ProxyType::Direct;
            self.proxy_url = String::new();
        }
    }

    /// Normalize proxy settings when loading from file or legacy config
    pub fn normalize_proxy(&mut self) {
        if self.proxy_type == ProxyType::Direct && self.proxy_mode == ProxyMode::Custom && !self.proxy_url.is_empty() {
            if let Ok(parsed) = url::Url::parse(&self.proxy_url) {
                let scheme = parsed.scheme();
                self.proxy_type = match scheme {
                    "http" => ProxyType::Http,
                    "https" => ProxyType::Https,
                    "socks4" => ProxyType::Socks4,
                    "socks5" | "socks5h" => ProxyType::Socks5,
                    _ => ProxyType::Http,
                };
                if let Some(host) = parsed.host_str() {
                    self.proxy_host = host.to_string();
                }
                if let Some(port) = parsed.port() {
                    self.proxy_port = port.to_string();
                }
            }
        } else if self.proxy_type == ProxyType::Direct && self.proxy_mode == ProxyMode::System {
            self.proxy_type = ProxyType::System;
        }
        if self.proxy_host.trim().is_empty() {
            self.proxy_host = default_proxy_host();
        }
        if self.proxy_port.trim().is_empty() {
            self.proxy_port = default_proxy_port();
        }
    }
}

/// Apply proxy configuration to the current process's environment variables.
pub fn apply_proxy_env(settings: &AppSettings) {
    unsafe {
        match settings.proxy_mode {
            ProxyMode::Custom if !settings.proxy_url.trim().is_empty() => {
                let url = settings.proxy_url.trim();
                std::env::set_var("HTTP_PROXY", url);
                std::env::set_var("HTTPS_PROXY", url);
                if url.starts_with("socks") {
                    std::env::set_var("ALL_PROXY", url);
                }
                std::env::remove_var("AITOOLPLUS_PROXY_MODE");
            }
            ProxyMode::Direct => {
                std::env::remove_var("HTTP_PROXY");
                std::env::remove_var("HTTPS_PROXY");
                std::env::remove_var("ALL_PROXY");
                std::env::set_var("AITOOLPLUS_PROXY_MODE", "direct");
            }
            _ => {
                std::env::remove_var("AITOOLPLUS_PROXY_MODE");
            }
        }
    }
}

/// Test the connectivity of a proxy URL (or direct connection) by attempting to reach reliable endpoints.
pub fn test_proxy_connectivity(proxy_url: &str) -> Result<u128, String> {
    let trimmed = proxy_url.trim();

    let mut builder = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(5))
        .timeout_read(std::time::Duration::from_secs(6));

    if !trimmed.is_empty() && trimmed != "direct" {
        let proxy = ureq::Proxy::new(trimmed)
            .map_err(|e| format!("代理配置格式错误: {e}"))?;
        builder = builder.proxy(proxy);
    }

    let agent = builder.build();

    let start = std::time::Instant::now();
    let res = agent
        .get("https://cloudflare.com/cdn-cgi/trace")
        .set("User-Agent", "aitoolplus/1.0")
        .call();

    match res {
        Ok(resp) if resp.status() == 200 || resp.status() == 204 => {
            Ok(start.elapsed().as_millis())
        }
        Ok(_) => Ok(start.elapsed().as_millis()),
        Err(e) => {
            let fallback_res = agent
                .get("https://www.google.com/generate_204")
                .set("User-Agent", "aitoolplus/1.0")
                .call();
            match fallback_res {
                Ok(_) => Ok(start.elapsed().as_millis()),
                Err(_) => {
                    let err_str = e.to_string();
                    let msg = if err_str.contains("10061")
                        || err_str.to_lowercase().contains("connection refused")
                    {
                        "代理服务器拒绝连接，请检查本地代理客户端是否已开启".to_string()
                    } else if err_str.contains("10060")
                        || err_str.to_lowercase().contains("timed out")
                        || err_str.to_lowercase().contains("timeout")
                    {
                        "连接代理服务器超时，请检查代理服务器地址和端口".to_string()
                    } else if err_str.to_lowercase().contains("socks") || err_str.contains("Proxy")
                    {
                        format!("代理握手失败: {err_str}")
                    } else {
                        err_str
                    };
                    Err(msg)
                }
            }
        }
    }
}

fn default_visible_tools() -> Vec<String> {
    let mut list: Vec<String> = crate::tools::ToolId::ALL
        .into_iter()
        .map(|tool| tool.key().to_string())
        .collect();
    list.push("antigravity".into());
    list
}

fn default_true() -> bool {
    true
}

fn default_update_mirror() -> crate::updater::UpdateMirror {
    if system_is_chinese() {
        crate::updater::UpdateMirror::GhProxy
    } else {
        crate::updater::UpdateMirror::Official
    }
}

fn default_backup_interval_hours() -> u32 {
    24
}

fn default_backup_retain_count() -> usize {
    10
}

fn default_backup_interval_days() -> u32 {
    7
}

fn default_backup_max_keep() -> u32 {
    10
}

impl AppSettings {
    pub fn load(path: &Path) -> Self {
        let mut settings = load_or_default::<Self>(path).unwrap_or_default();
        if settings.backup_retain_count == 0 {
            settings.backup_retain_count = 10;
        }
        settings.normalize_proxy();

        let sync_path = path.with_file_name("sync.json");
        let sync = SyncSettings::load(&sync_path);
        settings.backup_type = sync.backup_type;
        settings.webdav = sync.webdav;
        settings.s3 = sync.s3;
        settings
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let mut cloned = self.clone();
        cloned.auto_backup_interval_days = (cloned.backup_interval_hours / 24).max(1);
        cloned.auto_backup_max_keep = cloned.backup_retain_count as u32;
        cloned.sync_proxy();

        // Persist sync settings to isolated sync.json
        let sync_path = path.with_file_name("sync.json");
        let sync = SyncSettings {
            backup_type: self.backup_type,
            webdav: self.webdav.clone(),
            s3: self.s3.clone(),
        };
        sync.save(&sync_path)?;

        // Persist general settings (settings.json), omitting backup_type, webdav, and s3
        save_json_atomic(path, &cloned).map_err(|e| e.to_string())
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

    #[test]
    fn sync_settings_isolated_in_sync_json_and_excluded_from_settings_json() {
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        let sync_path = dir.path().join("sync.json");

        let mut s = AppSettings::default();
        s.backup_type = BackupType::Webdav;
        s.webdav.url = "https://dav.example.com".into();
        s.webdav.username = "user1".into();
        s.webdav.password = "secret_pass_123".into();
        s.s3.secret_access_key = "s3_secret_xyz".into();

        s.save(&settings_path).unwrap();

        // 1. sync.json must exist and hold the credentials (protected)
        assert!(sync_path.is_file());
        let sync_content = fs::read_to_string(&sync_path).unwrap();
        assert!(sync_content.contains("https://dav.example.com"));
        assert!(sync_content.contains("user1"));
        assert!(!sync_content.contains("secret_pass_123")); // DPAPI protected

        // 2. settings.json must NOT contain webdav, s3, or passwords
        let settings_content = fs::read_to_string(&settings_path).unwrap();
        assert!(!settings_content.contains("webdav"));
        assert!(!settings_content.contains("secret_pass_123"));
        assert!(!settings_content.contains("s3_secret_xyz"));
        assert!(!settings_content.contains("https://dav.example.com"));

        // 3. Loading settings restores the isolated sync configuration transparently
        let loaded = AppSettings::load(&settings_path);
        assert_eq!(loaded.backup_type, BackupType::Webdav);
        assert_eq!(loaded.webdav.url, "https://dav.example.com");
        assert_eq!(loaded.webdav.username, "user1");
        assert_eq!(loaded.webdav.password, "secret_pass_123");
        assert_eq!(loaded.s3.secret_access_key, "s3_secret_xyz");
    }

    #[test]
    fn test_proxy_settings_sync_and_normalize() {
        let mut s = AppSettings::default();
        assert_eq!(s.proxy_type, ProxyType::Direct);
        assert_eq!(s.proxy_mode, ProxyMode::Direct);
        assert_eq!(s.proxy_host, "127.0.0.1");
        assert_eq!(s.proxy_port, "7890");

        // Custom SOCKS5
        s.proxy_type = ProxyType::Socks5;
        s.proxy_host = "proxy.example.com".into();
        s.proxy_port = "1080".into();
        s.sync_proxy();
        assert_eq!(s.proxy_mode, ProxyMode::Custom);
        assert_eq!(s.proxy_url, "socks5://proxy.example.com:1080");

        // Back to direct
        s.proxy_type = ProxyType::Direct;
        s.sync_proxy();
        assert_eq!(s.proxy_mode, ProxyMode::Direct);
        assert_eq!(s.proxy_url, "");

        // Legacy proxy_url normalization
        let mut legacy = AppSettings::default();
        legacy.proxy_mode = ProxyMode::Custom;
        legacy.proxy_url = "http://myproxy.local:8080".into();
        legacy.normalize_proxy();
        assert_eq!(legacy.proxy_type, ProxyType::Http);
        assert_eq!(legacy.proxy_host, "myproxy.local");
        assert_eq!(legacy.proxy_port, "8080");
    }

    #[test]
    fn test_proxy_connectivity_direct() {
        // Direct test should either succeed or fail gracefully without panicking
        let res = test_proxy_connectivity("direct");
        println!("Direct connectivity test result: {:?}", res);
    }
}
