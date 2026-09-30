use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const DEFAULT_GATEWAY_HOST: &str = "127.0.0.1";
pub const DEFAULT_GATEWAY_PORT: u16 = 15721;
pub const DEFAULT_STREAMING_IDLE_TIMEOUT_SECS: u64 = 45;
pub const DEFAULT_NON_STREAMING_TIMEOUT_SECS: u64 = 60;

/// App-specific gateway settings (e.g. streaming timeouts or custom headers).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayAppConfig {
    #[serde(default = "default_streaming_idle_timeout")]
    pub streaming_idle_timeout_secs: u64,
    #[serde(default = "default_non_streaming_timeout")]
    pub non_streaming_timeout_secs: u64,
}

fn default_streaming_idle_timeout() -> u64 {
    DEFAULT_STREAMING_IDLE_TIMEOUT_SECS
}

fn default_non_streaming_timeout() -> u64 {
    DEFAULT_NON_STREAMING_TIMEOUT_SECS
}

impl Default for GatewayAppConfig {
    fn default() -> Self {
        Self {
            streaming_idle_timeout_secs: DEFAULT_STREAMING_IDLE_TIMEOUT_SECS,
            non_streaming_timeout_secs: DEFAULT_NON_STREAMING_TIMEOUT_SECS,
        }
    }
}

/// Global settings for the Gateway.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewaySettings {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_true")]
    pub port_auto_select: bool,
    #[serde(default = "default_false")]
    pub enabled_on_startup: bool,
    #[serde(default = "default_true")]
    pub request_log_enabled: bool,
    #[serde(default = "default_true")]
    pub request_log_body_enabled: bool,
    #[serde(default = "default_streaming_idle_timeout")]
    pub streaming_idle_timeout_secs: u64,
    #[serde(default = "default_non_streaming_timeout")]
    pub non_streaming_timeout_secs: u64,
    /// Master toggle for automatic failover across candidate providers.
    #[serde(default = "default_false")]
    pub failover_enabled: bool,
    #[serde(default)]
    pub app_configs: HashMap<String, GatewayAppConfig>,
    /// Priority lists of provider IDs for failover mode per CLI.
    #[serde(default)]
    pub failover_priorities: HashMap<String, Vec<String>>,
    /// Persistent CLI takeover desired states (parity with cc-switch proxy_config.enabled per app).
    /// Used by `stop_with_restore_keep_state` on exit and `restore_proxy_state_on_startup` on boot.
    #[serde(default)]
    pub takeover_apps: HashMap<String, GatewayCliTakeoverSetting>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayCliTakeoverSetting {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default)]
    pub mode: super::types::GatewayProxyMode,
    #[serde(default)]
    pub primary_provider_id: Option<String>,
}

fn default_host() -> String {
    DEFAULT_GATEWAY_HOST.to_string()
}

fn default_port() -> u16 {
    DEFAULT_GATEWAY_PORT
}

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

impl Default for GatewaySettings {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            port_auto_select: true,
            enabled_on_startup: false,
            request_log_enabled: true,
            request_log_body_enabled: true,
            streaming_idle_timeout_secs: DEFAULT_STREAMING_IDLE_TIMEOUT_SECS,
            non_streaming_timeout_secs: DEFAULT_NON_STREAMING_TIMEOUT_SECS,
            failover_enabled: false,
            app_configs: HashMap::new(),
            failover_priorities: HashMap::new(),
            takeover_apps: HashMap::new(),
        }
    }
}

impl GatewaySettings {
    pub fn effective_app_config(&self, app_key: &str) -> GatewayAppConfig {
        self.app_configs
            .get(app_key)
            .cloned()
            .unwrap_or_else(|| GatewayAppConfig {
                streaming_idle_timeout_secs: self.streaming_idle_timeout_secs,
                non_streaming_timeout_secs: self.non_streaming_timeout_secs,
            })
    }

    pub fn load(paths: &crate::Paths) -> Self {
        let p = paths.gateway_dir().join("settings.json");
        if p.exists() {
            if let Ok(bytes) = std::fs::read(&p) {
                if let Ok(s) = serde_json::from_slice::<GatewaySettings>(&bytes) {
                    return s;
                }
            }
        }
        GatewaySettings::default()
    }

    pub fn save(&self, paths: &crate::Paths) -> std::io::Result<()> {
        let dir = paths.gateway_dir();
        std::fs::create_dir_all(&dir)?;
        let p = dir.join("settings.json");
        let data = serde_json::to_vec_pretty(self).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(p, data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gateway_settings_failover_defaults_to_false() {
        let def = GatewaySettings::default();
        assert!(!def.failover_enabled, "Default failover_enabled must be false");

        let from_json: GatewaySettings = serde_json::from_str("{}").unwrap();
        assert!(!from_json.failover_enabled, "Deserialized from empty JSON must default to false");
    }
}

