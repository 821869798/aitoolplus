use serde::{Deserialize, Serialize};

/// Supported CLI tool keys for Gateway takeover.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GatewayCliKey {
    Claude,
    Codex,
    Grok,
    Kimi,
}

impl GatewayCliKey {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Grok => "grok",
            Self::Kimi => "kimi",
        }
    }

    pub fn key(&self) -> &'static str {
        self.as_str()
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
            Self::Grok => "Grok CLI",
            Self::Kimi => "Kimi CLI",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "claude" | "claude_code" | "claudecode" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            "grok" | "grok_build" => Some(Self::Grok),
            "kimi" | "kimi_cli" => Some(Self::Kimi),
            _ => None,
        }
    }
}

/// Gateway Proxy Mode for an engaged CLI tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GatewayProxyMode {
    /// Single provider proxy: only forwards to primary provider (P0).
    #[default]
    Single,
    /// Auto failover: P0 fails -> automatically retries on P1, P2...
    Failover,
    /// Multi-provider model aggregation: models prefixed by site (<site>.<model>)
    Aggregate,
}

impl GatewayProxyMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Failover => "failover",
            Self::Aggregate => "aggregate",
        }
    }
}

/// Status of the Gateway HTTP server.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GatewayStatus {
    pub running: bool,
    pub host: String,
    pub port: u16,
    pub uptime_seconds: u64,
    pub active_connections: u32,
    pub total_requests: u64,
    pub success_requests: u64,
    pub failed_requests: u64,
    pub active_targets: Vec<GatewayActiveTarget>,
}

/// Active provider target for a specific CLI tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayActiveTarget {
    pub cli_key: GatewayCliKey,
    pub provider_id: String,
    pub provider_name: String,
    pub mode: GatewayProxyMode,
}

/// Live takeover status of a CLI tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayCliTakeoverStatus {
    pub cli_key: GatewayCliKey,
    pub enabled: bool,
    pub mode: GatewayProxyMode,
    pub primary_provider_id: Option<String>,
    pub primary_provider_name: Option<String>,
    pub provider_priorities: Vec<String>,
    pub error: Option<String>,
}

/// Summary record of an HTTP request through Gateway (stored in SQLite for analytics & list views).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GatewayRequestLogSummary {
    pub id: String,
    pub timestamp: i64,
    pub cli_key: String,
    pub route_name: String,
    pub provider_id: String,
    pub provider_name: String,
    pub model: String,
    pub status_code: u16,
    pub duration_ms: u64,
    pub first_token_ms: Option<u64>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cost: f64,
    pub is_streaming: bool,
    pub failover: bool,
    pub error_category: Option<String>,
}

/// Full request/response detail (stored in JSONL file for deep inspection).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GatewayRequestLogDetail {
    pub id: String,
    pub summary: GatewayRequestLogSummary,
    pub inbound_headers: Vec<(String, String)>,
    pub inbound_body: Option<String>,
    pub upstream_url: String,
    pub upstream_headers: Vec<(String, String)>,
    pub upstream_body: Option<String>,
    pub upstream_status_code: Option<u16>,
    pub upstream_response_body: Option<String>,
    pub client_response_body: Option<String>,
    pub attempts: Vec<GatewayAttemptTrace>,
}

/// Trace of an individual candidate attempt during request / failover.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayAttemptTrace {
    pub provider_id: String,
    pub provider_name: String,
    pub model: String,
    pub status_code: Option<u16>,
    pub duration_ms: u64,
    pub error: Option<String>,
}
