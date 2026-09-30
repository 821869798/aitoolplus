use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::gateway::transformer::types::AiProtocol;
use crate::gateway::types::{GatewayCliKey, GatewayProxyMode};
use crate::paths::Paths;
use crate::providers::ProviderRecord;
use crate::tools::ToolId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpstreamAuthType {
    Bearer,
    AnthropicApiKey,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayRouteTarget {
    pub provider_id: String,
    pub provider_name: String,
    pub base_url: String,
    pub api_key: String,
    pub auth_type: UpstreamAuthType,
    pub default_model: Option<String>,
    pub opus_model: Option<String>,
    pub sonnet_model: Option<String>,
    pub haiku_model: Option<String>,
    pub fable_model: Option<String>,
    pub target_protocol: AiProtocol,
    pub custom_headers: Vec<(String, String)>,
    pub model_rewrites: Vec<(String, String)>,
    pub timeout_seconds: u64,
}

impl GatewayRouteTarget {
    pub fn from_provider(record: &ProviderRecord, cli_key: GatewayCliKey) -> Option<Self> {
        let settings = serde_json::from_str::<Value>(&record.settings_config).ok().unwrap_or(Value::Null);

        let tool_id = cli_to_tool_id(cli_key);
        let (resolved_url, resolved_key) = record.resolve_credentials(tool_id);

        let base_url = if !resolved_url.is_empty() {
            resolved_url
        } else {
            match cli_key {
                GatewayCliKey::Claude => "https://api.anthropic.com".to_string(),
                GatewayCliKey::Codex => "https://api.openai.com/v1".to_string(),
                _ => "".to_string(),
            }
        };

        let api_key = resolved_key;

        let mut opus_model = None;
        let mut sonnet_model = None;
        let mut haiku_model = None;
        let mut fable_model = None;
        let mut default_model = None;

        if cli_key == GatewayCliKey::Claude {
            if let Some(env) = settings.get("env") {
                opus_model = env.get("ANTHROPIC_DEFAULT_OPUS_MODEL")
                    .or_else(|| env.get("ANTHROPIC_DEFAULT_OPUS_MODEL_NAME"))
                    .or_else(|| settings.get("opusModel"))
                    .and_then(Value::as_str)
                    .map(|s| crate::gateway::forwarder::strip_one_m_marker(s).to_string());

                sonnet_model = env.get("ANTHROPIC_DEFAULT_SONNET_MODEL")
                    .or_else(|| env.get("ANTHROPIC_DEFAULT_SONNET_MODEL_NAME"))
                    .or_else(|| settings.get("sonnetModel"))
                    .and_then(Value::as_str)
                    .map(|s| crate::gateway::forwarder::strip_one_m_marker(s).to_string());

                haiku_model = env.get("ANTHROPIC_DEFAULT_HAIKU_MODEL")
                    .or_else(|| env.get("ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME"))
                    .or_else(|| settings.get("haikuModel"))
                    .and_then(Value::as_str)
                    .map(|s| crate::gateway::forwarder::strip_one_m_marker(s).to_string());

                fable_model = env.get("ANTHROPIC_DEFAULT_FABLE_MODEL")
                    .or_else(|| env.get("ANTHROPIC_DEFAULT_FABLE_MODEL_NAME"))
                    .or_else(|| settings.get("fableModel"))
                    .and_then(Value::as_str)
                    .map(|s| crate::gateway::forwarder::strip_one_m_marker(s).to_string());

                default_model = env.get("ANTHROPIC_MODEL")
                    .or_else(|| settings.get("model"))
                    .and_then(Value::as_str)
                    .map(|s| crate::gateway::forwarder::strip_one_m_marker(s).to_string());
            }
            if default_model.is_none() {
                default_model = settings.get("model")
                    .and_then(Value::as_str)
                    .map(|s| crate::gateway::forwarder::strip_one_m_marker(s).to_string());
            }
        } else {
            default_model = settings.get("model")
                .and_then(Value::as_str)
                .map(|s| crate::gateway::forwarder::strip_one_m_marker(s).to_string());
        }

        let default_model = default_model
            .or_else(|| opus_model.clone())
            .or_else(|| sonnet_model.clone())
            .or_else(|| haiku_model.clone())
            .or_else(|| {
                if base_url.contains("deepseek") {
                    Some("deepseek-chat".to_string())
                } else {
                    None
                }
            });

        // Determine upstream authentication header type
        // Aligned with cc-switch (infer_anthropic_auth_strategy) and ai-toolbox (ProviderAuthStrategy)
        let auth_type = if cli_key == GatewayCliKey::Claude {
            if let Some(env) = settings.get("env") {
                if env.get("ANTHROPIC_AUTH_TOKEN").and_then(Value::as_str).filter(|s| !s.trim().is_empty()).is_some() {
                    UpstreamAuthType::Bearer
                } else if env.get("ANTHROPIC_API_KEY").and_then(Value::as_str).filter(|s| !s.trim().is_empty()).is_some() {
                    UpstreamAuthType::AnthropicApiKey
                } else if api_key.starts_with("sk-ant-") {
                    UpstreamAuthType::AnthropicApiKey
                } else {
                    UpstreamAuthType::Bearer
                }
            } else if api_key.starts_with("sk-ant-") {
                UpstreamAuthType::AnthropicApiKey
            } else {
                UpstreamAuthType::Bearer
            }
        } else {
            UpstreamAuthType::Bearer
        };

        // Determine target protocol
        let target_protocol = if let Some(meta) = &record.meta {
            if let Some(format) = meta.get("apiFormat").or_else(|| meta.get("api_format")).and_then(Value::as_str) {
                AiProtocol::from_api_format(format).unwrap_or_else(|| default_protocol_for_cli(cli_key))
            } else if base_url.contains("/anthropic") {
                AiProtocol::AnthropicMessages
            } else if base_url.contains("v1/chat") || base_url.contains("chat/completions") {
                AiProtocol::OpenAiChat
            } else {
                default_protocol_for_cli(cli_key)
            }
        } else if base_url.contains("/anthropic") {
            AiProtocol::AnthropicMessages
        } else if base_url.contains("v1/chat") || base_url.contains("chat/completions") {
            AiProtocol::OpenAiChat
        } else {
            default_protocol_for_cli(cli_key)
        };

        // Extract custom headers
        let mut custom_headers = Vec::new();
        if let Some(meta) = &record.meta {
            if let Some(headers) = meta.get("customHeaders").or_else(|| meta.get("custom_headers")).and_then(Value::as_array) {
                for h in headers {
                    if let (Some(k), Some(v)) = (h.get("name").and_then(Value::as_str), h.get("value").and_then(Value::as_str)) {
                        custom_headers.push((k.to_string(), v.to_string()));
                    }
                }
            }
        }

        // Extract model rewrites
        let mut model_rewrites = Vec::new();
        if let Some(meta) = &record.meta {
            if let Some(rewrites) = meta.get("modelRewrites").or_else(|| meta.get("model_rewrites")).and_then(Value::as_array) {
                for r in rewrites {
                    if let (Some(from), Some(to)) = (r.get("from").and_then(Value::as_str), r.get("to").and_then(Value::as_str)) {
                        model_rewrites.push((from.to_string(), to.to_string()));
                    }
                }
            }
        }

        Some(Self {
            provider_id: record.id.clone(),
            provider_name: record.name.clone(),
            base_url,
            api_key,
            auth_type,
            default_model,
            opus_model,
            sonnet_model,
            haiku_model,
            fable_model,
            target_protocol,
            custom_headers,
            model_rewrites,
            timeout_seconds: 60,
        })
    }
}

pub fn default_protocol_for_cli(cli: GatewayCliKey) -> AiProtocol {
    match cli {
        GatewayCliKey::Claude => AiProtocol::AnthropicMessages,
        GatewayCliKey::Codex => AiProtocol::OpenAiResponses,
        GatewayCliKey::Grok | GatewayCliKey::Kimi => AiProtocol::OpenAiChat,
    }
}

pub fn cli_to_tool_id(cli: GatewayCliKey) -> ToolId {
    match cli {
        GatewayCliKey::Claude => ToolId::ClaudeCode,
        GatewayCliKey::Codex => ToolId::Codex,
        GatewayCliKey::Grok => ToolId::Grok,
        GatewayCliKey::Kimi => ToolId::Kimi,
    }
}

pub struct GatewayRouter;

impl GatewayRouter {
    /// Resolve candidate route targets for a CLI tool, in prioritized failover order.
    pub fn resolve_candidates(
        paths: &Paths,
        cli_key: GatewayCliKey,
        mode: GatewayProxyMode,
        explicit_primary_id: Option<&str>,
    ) -> Vec<GatewayRouteTarget> {
        let tool_id = cli_to_tool_id(cli_key);
        let store_handle = crate::store::StoreHandle::open(paths).ok();
        let tool_store = store_handle
            .as_ref()
            .map(|h| h.store().tool(tool_id))
            .unwrap_or_default();

        let mut available: Vec<ProviderRecord> = tool_store
            .providers
            .into_iter()
            .filter(|p| !p.is_disabled)
            .collect();

        if available.is_empty() {
            return Vec::new();
        }

        // Sort: explicit primary or is_applied first, then by sort_index
        available.sort_by(|a, b| {
            let a_is_primary = if let Some(pid) = explicit_primary_id {
                a.id == pid
            } else {
                a.is_applied
            };
            let b_is_primary = if let Some(pid) = explicit_primary_id {
                b.id == pid
            } else {
                b.is_applied
            };

            match (a_is_primary, b_is_primary) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.sort_index.cmp(&b.sort_index),
            }
        });

        if mode == GatewayProxyMode::Single {
            available.truncate(1);
        }

        available
            .iter()
            .filter_map(|r| GatewayRouteTarget::from_provider(r, cli_key))
            .collect()
    }
}
