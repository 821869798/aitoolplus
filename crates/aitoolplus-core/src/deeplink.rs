//! `aitoolbox://v1/import` provider deep links (cc-switch compatible scope).
//!
//! V1 deliberately supports only Claude Code, Codex and Gemini CLI, matching
//! upstream. Sensitive query values are redacted before logging.

use base64::Engine;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::providers::ProviderRecord;
use crate::tools::ToolId;

pub const SCHEMES: &[&str] = &["aitoolplus", "aitoolbox", "ccswitch"];
pub const PRIMARY_SCHEME: &str = "aitoolplus";
pub const SCHEME: &str = PRIMARY_SCHEME;
pub const VERSION: &str = "v1";
pub const IMPORT_PATH: &str = "/import";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeepLinkImport {
    pub tool: ToolId,
    pub name: String,
    pub category: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub homepage: Option<String>,
    pub notes: Option<String>,
    pub config: Option<String>,
    pub extra: Option<String>,
}

pub fn parse(raw: &str) -> Result<DeepLinkImport, String> {
    let url = Url::parse(raw).map_err(|e| format!("invalid deep link: {e}"))?;
    if !SCHEMES.contains(&url.scheme()) {
        return Err(format!("expected supported scheme ({})", SCHEMES.join(", ")));
    }
    if url.host_str() != Some(VERSION) {
        return Err(format!("unsupported deep-link version; expected {VERSION}"));
    }
    if url.path() != IMPORT_PATH {
        return Err(format!("unsupported path; expected {IMPORT_PATH}"));
    }
    let params: std::collections::BTreeMap<String, String> = url
        .query_pairs()
        .map(|(k, v)| (k.into(), v.into()))
        .collect();
    if params.get("resource").map(String::as_str) != Some("provider") {
        return Err("only provider resources are supported".into());
    }
    if params
        .get("endpoints")
        .is_some_and(|value| !value.trim().is_empty())
    {
        return Err("endpoints parameter is unsupported in v1".into());
    }
    let app = params.get("app").map(String::as_str).unwrap_or("");
    let tool = ToolId::from_key(app)
        .or_else(|| match app.to_ascii_lowercase().as_str() {
            "claude" | "claudecode" | "claude_code" => Some(ToolId::ClaudeCode),
            "codex" => Some(ToolId::Codex),
            "gemini" | "geminicli" | "gemini_cli" => Some(ToolId::GeminiCli),
            "grok" => Some(ToolId::Grok),
            "kimi" => Some(ToolId::Kimi),
            "opencode" | "open_code" => Some(ToolId::OpenCode),
            "openclaw" | "open_claw" => Some(ToolId::OpenClaw),
            "pi" => Some(ToolId::Pi),
            "oh_my_pi" | "omp" | "ohmypi" => Some(ToolId::OhMyPi),
            "claude_desktop" | "claudedesktop" => Some(ToolId::ClaudeDesktop),
            "hermes" => Some(ToolId::Hermes),
            "dsh" | "deepseek_harness" | "deepseek" => Some(ToolId::Dsh),
            _ => None,
        })
        .ok_or_else(|| format!("unsupported deep-link app: {app}"))?;

    let name = required(&params, "name")?;
    let category = normalize_category(params.get("category").map(String::as_str));
    let base_url = optional(&params, "baseUrl")
        .or_else(|| optional(&params, "endpoint"))
        .map(|value| validate_http_url(&value, "baseUrl"))
        .transpose()?;
    let api_key = optional(&params, "apiKey")
        .or_else(|| optional(&params, "key"))
        .or_else(|| optional(&params, "token"));
    let homepage = optional(&params, "homepage")
        .map(|value| validate_http_url(&value, "homepage"))
        .transpose()?;
    let config = optional(&params, "config")
        .map(|value| decode_config(&value, "config"))
        .transpose()?;
    let extra = optional(&params, "extra")
        .map(|value| decode_config(&value, "extra"))
        .transpose()?;
    Ok(DeepLinkImport {
        tool,
        name,
        category,
        api_key,
        base_url,
        model: optional(&params, "model"),
        homepage,
        notes: optional(&params, "notes"),
        config,
        extra,
    })
}

pub fn into_provider(request: DeepLinkImport) -> Result<ProviderRecord, String> {
    let mut record = ProviderRecord::new(request.name.clone(), request.category);
    record.website_url = request.homepage;
    record.notes = request.notes;
    let settings = match request.tool {
        ToolId::ClaudeCode => {
            if let Some(config) = request.config {
                serde_json::from_str(&config)
                    .map_err(|e| format!("decoded Claude config is invalid JSON: {e}"))?
            } else {
                let mut env = serde_json::Map::new();
                if let Some(key) = request.api_key {
                    env.insert(
                        "ANTHROPIC_AUTH_TOKEN".into(),
                        serde_json::Value::String(key),
                    );
                }
                if let Some(url) = request.base_url {
                    env.insert("ANTHROPIC_BASE_URL".into(), serde_json::Value::String(url));
                }
                if let Some(model) = request.model {
                    env.insert("ANTHROPIC_MODEL".into(), serde_json::Value::String(model));
                }
                serde_json::json!({"env": env})
            }
        }
        ToolId::Codex => {
            if let Some(config) = request.config {
                // decoded Codex config is raw TOML
                config
                    .parse::<toml_edit::DocumentMut>()
                    .map_err(|e| format!("decoded Codex config is invalid TOML: {e}"))?;
                serde_json::json!({"toml": config})
            } else {
                let base = request
                    .base_url
                    .ok_or("Codex deep link requires baseUrl or endpoint or config")?;
                let model = request.model.unwrap_or_else(|| "gpt-5-codex".into());
                let key = request.api_key.unwrap_or_default().replace('"', "\\\"");
                serde_json::json!({"toml": format!(
                    "model_provider = \"imported\"\nmodel = \"{model}\"\n\n[model_providers.imported]\nname = \"Imported\"\nbase_url = \"{base}\"\nwire_api = \"responses\"\napi_key = \"{key}\"\n"
                )})
            }
        }
        ToolId::GeminiCli => {
            if let Some(config) = request.config {
                serde_json::from_str(&config)
                    .map_err(|e| format!("decoded Gemini config is invalid JSON: {e}"))?
            } else {
                let mut env = serde_json::Map::new();
                if let Some(key) = request.api_key {
                    env.insert("GEMINI_API_KEY".into(), serde_json::Value::String(key));
                }
                if let Some(url) = request.base_url {
                    env.insert(
                        "GOOGLE_GEMINI_BASE_URL".into(),
                        serde_json::Value::String(url),
                    );
                }
                if let Some(model) = request.model {
                    env.insert("GEMINI_MODEL".into(), serde_json::Value::String(model));
                }
                serde_json::json!({
                    "env": env,
                    "security": {"auth": {"selectedType": "gemini-api-key"}}
                })
            }
        }
        ToolId::Grok => {
            serde_json::json!({
                "base_url": request.base_url.unwrap_or_else(|| "https://api.x.ai/v1".into()),
                "api_key": request.api_key.unwrap_or_default(),
                "model": request.model.unwrap_or_else(|| "grok-4".into()),
            })
        }
        ToolId::Kimi => {
            serde_json::json!({
                "base_url": request.base_url.unwrap_or_else(|| "https://api.moonshot.cn/v1".into()),
                "api_key": request.api_key.unwrap_or_default(),
                "model": request.model.unwrap_or_else(|| "kimi-k2".into()),
            })
        }
        ToolId::OpenCode => {
            serde_json::json!({
                "provider": "custom",
                "base_url": request.base_url.unwrap_or_else(|| "https://api.example.com/v1".into()),
                "api_key": request.api_key.unwrap_or_default(),
                "model": request.model.unwrap_or_else(|| "claude-sonnet-4".into()),
            })
        }
        ToolId::OpenClaw => {
            serde_json::json!({
                "base_url": request.base_url.unwrap_or_else(|| "https://api.example.com/v1".into()),
                "api_key": request.api_key.unwrap_or_default(),
                "model": request.model.unwrap_or_else(|| "gpt-4o".into()),
            })
        }
        ToolId::Pi => {
            let model_name = request.model.unwrap_or_else(|| "default-model".into());
            serde_json::json!({
                "name": request.name,
                "baseUrl": request.base_url.unwrap_or_else(|| "https://api.example.com/v1".into()),
                "apiKey": request.api_key.unwrap_or_default(),
                "models": [{"id": model_name.clone(), "name": model_name}]
            })
        }
        ToolId::OhMyPi => {
            serde_json::json!({
                "name": request.name,
                "baseUrl": request.base_url.unwrap_or_else(|| "https://api.example.com/v1".into()),
            })
        }
        ToolId::ClaudeDesktop => {
            serde_json::json!({
                "inferenceGatewayBaseUrl": request.base_url.unwrap_or_else(|| "https://api.example.com".into()),
                "inferenceModels": [request.model.unwrap_or_else(|| "default-model".into())]
            })
        }
        ToolId::Hermes => {
            serde_json::json!({
                "name": request.name,
                "base_url": request.base_url.unwrap_or_else(|| "https://api.example.com/v1".into()),
                "api_key": request.api_key.unwrap_or_default(),
                "model": request.model.unwrap_or_else(|| "model-1".into()),
            })
        }
        ToolId::Dsh => {
            serde_json::json!({
                "name": request.name,
                "baseUrl": request.base_url.unwrap_or_else(|| "https://api.example.com/v1".into()),
                "apiKeyEnv": "API_KEY",
                "apiKey": request.api_key.unwrap_or_default(),
                "defaultModel": request.model.unwrap_or_else(|| "model-1".into()),
            })
        }
        ToolId::Agents => {
            serde_json::json!({})
        }
    };
    record.settings_config = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    Ok(record)
}

pub fn import_into_store(
    raw: &str,
    store: &mut crate::store::Store,
) -> Result<(ToolId, String), String> {
    let request = parse(raw)?;
    let tool = request.tool;
    let provider = into_provider(request)?;
    let name = provider.name.clone();
    let section = store.tool_mut(tool);
    // deduplicate by name + settings; a later explicit import refreshes it.
    if let Some(existing) = section.providers.iter_mut().find(|item| item.name == name) {
        existing.settings_config = provider.settings_config;
        existing.category = provider.category;
        existing.notes = provider.notes;
        existing.website_url = provider.website_url;
        existing.touch();
    } else {
        section.providers.push(provider);
    }
    Ok((tool, name))
}

pub fn redact(raw: &str) -> String {
    let Ok(mut url) = Url::parse(raw) else {
        return "<unparseable deep-link URL>".into();
    };
    let sensitive = ["apiKey", "config", "extra"];
    let pairs: Vec<(String, String)> = url
        .query_pairs()
        .map(|(key, value)| {
            let value = if sensitive.contains(&key.as_ref()) {
                "<redacted>".into()
            } else {
                value.into_owned()
            };
            (key.into_owned(), value)
        })
        .collect();
    url.set_query(None);
    {
        let mut query = url.query_pairs_mut();
        for (key, value) in pairs {
            query.append_pair(&key, &value);
        }
    }
    url.to_string()
}

fn required(
    params: &std::collections::BTreeMap<String, String>,
    key: &str,
) -> Result<String, String> {
    optional(params, key).ok_or_else(|| format!("missing required parameter: {key}"))
}

fn optional(params: &std::collections::BTreeMap<String, String>, key: &str) -> Option<String> {
    params
        .get(key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn normalize_category(value: Option<&str>) -> String {
    match value.unwrap_or("").trim() {
        "official" => "official",
        "aggregator" | "third_party" => "custom",
        "omo" | "custom" => "custom",
        _ => "custom",
    }
    .into()
}

fn validate_http_url(value: &str, field: &str) -> Result<String, String> {
    let parsed = Url::parse(value).map_err(|e| format!("invalid {field}: {e}"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(format!("{field} must use http or https"));
    }
    Ok(value.into())
}

fn decode_config(value: &str, field: &str) -> Result<String, String> {
    let engines = [
        &base64::engine::general_purpose::STANDARD,
        &base64::engine::general_purpose::URL_SAFE,
        &base64::engine::general_purpose::STANDARD_NO_PAD,
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
    ];
    for engine in engines {
        if let Ok(bytes) = engine.decode(value)
            && let Ok(text) = String::from_utf8(bytes)
        {
            return Ok(text);
        }
    }
    Err(format!("invalid base64 in {field}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_build_multi_scheme() {
        let claude = parse("aitoolbox://v1/import?resource=provider&app=claude&name=Relay&category=custom&apiKey=secret&baseUrl=https%3A%2F%2Fa.example&model=m1").unwrap();
        let provider = into_provider(claude).unwrap();
        assert_eq!(
            provider.settings()["env"]["ANTHROPIC_BASE_URL"],
            "https://a.example"
        );
        assert_eq!(provider.settings()["env"]["ANTHROPIC_AUTH_TOKEN"], "secret");

        let codex = parse("aitoolplus://v1/import?resource=provider&app=codex&name=C&baseUrl=https%3A%2F%2Fc.example%2Fv1&apiKey=k&model=gpt-5").unwrap();
        let provider = into_provider(codex).unwrap();
        assert!(
            provider.settings()["toml"]
                .as_str()
                .unwrap()
                .contains("base_url = \"https://c.example/v1\"")
        );

        let pi = parse("aitoolplus://v1/import?resource=provider&app=pi&name=PiTest&endpoint=https%3A%2F%2Fpi.example%2Fv1&apiKey=pi-sec&model=pi-3").unwrap();
        let provider = into_provider(pi).unwrap();
        assert_eq!(provider.settings()["baseUrl"], "https://pi.example/v1");
        assert_eq!(provider.settings()["apiKey"], "pi-sec");

        let grok = parse("ccswitch://v1/import?resource=provider&app=grok&name=GrokX&endpoint=https%3A%2F%2Fapi.x.ai%2Fv1&apiKey=x-sec").unwrap();
        let provider = into_provider(grok).unwrap();
        assert_eq!(provider.settings()["base_url"], "https://api.x.ai/v1");
        assert_eq!(provider.settings()["api_key"], "x-sec");
    }

    #[test]
    fn config_base64_and_validation() {
        let encoded =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"env":{"A":"B"}}"#);
        let request = parse(&format!(
            "aitoolplus://v1/import?resource=provider&app=claude&name=X&config={encoded}"
        ))
        .unwrap();
        assert_eq!(request.config.as_deref(), Some(r#"{"env":{"A":"B"}}"#));
        assert!(parse("aitoolplus://v2/import?resource=provider&app=codex&name=X").is_err());
        assert!(parse("aitoolplus://v1/import?resource=mcp&app=codex&name=X").is_err());
        assert!(parse("aitoolplus://v1/import?resource=provider&app=unknown_app&name=X").is_err());
        assert!(
            parse("aitoolplus://v1/import?resource=provider&app=codex&name=X&baseUrl=ftp%3A%2F%2Fx")
                .is_err()
        );
    }

    #[test]
    fn redaction_and_store_dedup() {
        let raw = "aitoolplus://v1/import?resource=provider&app=claude&name=X&apiKey=secret&baseUrl=https%3A%2F%2Fx.example";
        let redacted = redact(raw);
        assert!(!redacted.contains("secret"));
        assert!(redacted.contains("%3Credacted%3E") || redacted.contains("%3credacted%3e"));
        let mut store = crate::store::Store::new();
        import_into_store(raw, &mut store).unwrap();
        import_into_store(raw, &mut store).unwrap();
        assert_eq!(store.tool(ToolId::ClaudeCode).providers.len(), 1);
    }
}
