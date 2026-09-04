//! API Hub: fetch model lists from provider endpoints (`/v1/models`) with
//! graceful degradation, plus a typed result the UI can render.
//!
//! Mirrors ai-toolbox `all_api_hub` + per-tool `models_api` semantics in a
//! dependency-light form (no reqwest; std TCP + minimal HTTP/1.1 over TLS is
//! not attempted — callers provide the transport).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One model entry from a provider's model list.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchedModel {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_length: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_price: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_price: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_support: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vision_support: Option<bool>,
}

/// Why a fetch failed, so the UI can show the right hint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ModelsFetchError {
    Unsupported(String),
    Network(String),
    Parse(String),
    Auth,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelsFetchResult {
    pub models: Vec<FetchedModel>,
    pub raw: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectivityResult {
    pub ok: bool,
    pub latency_ms: u128,
    pub status: Option<u16>,
    pub message: String,
    pub models_count: usize,
}

/// Normalize a base URL for a models request: ensure scheme, strip trailing
/// slashes; append `/models` when the base ends with `/v1` (OpenAI style).
pub fn models_url(base_url: &str) -> Result<String, ModelsFetchError> {
    let trimmed = base_url.trim();
    if trimmed.is_empty() {
        return Err(ModelsFetchError::Unsupported("empty base_url".into()));
    }
    let with_scheme = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    let no_trailing = with_scheme.trim_end_matches('/');
    let url = if no_trailing.ends_with("/v1") {
        format!("{no_trailing}/models")
    } else {
        format!("{no_trailing}/v1/models")
    };
    Ok(url)
}

/// Parse an OpenAI-style `/v1/models` response into entries.
pub fn parse_openai_models(body: &Value) -> Vec<FetchedModel> {
    let Some(arr) = body.get("data").and_then(Value::as_array) else {
        return vec![];
    };
    arr.iter()
        .filter_map(|m| {
            let id = m.get("id").and_then(Value::as_str)?;
            Some(FetchedModel {
                id: id.to_string(),
                display_name: m
                    .get("display_name")
                    .or_else(|| m.get("name"))
                    .and_then(Value::as_str)
                    .map(String::from),
                context_length: m
                    .get("context_length")
                    .or_else(|| m.get("context_length_tokens"))
                    .and_then(Value::as_u64),
                input_price: m.get("input_price").and_then(Value::as_f64),
                output_price: m.get("output_price").and_then(Value::as_f64),
                tool_support: m.get("tool_support").and_then(Value::as_bool),
                vision_support: m.get("vision_support").and_then(Value::as_bool),
            })
        })
        .collect()
}

/// Extract the api key + base URL from a provider record's settings JSON,
/// understanding each tool's field names (upstream `all_api_hub` does this
/// per-tool; we centralize the common shapes here).
pub fn provider_endpoint(settings: &Value) -> Option<(String, String)> {
    // Codex stores a TOML projection in {"toml":"..."}.
    if let Some(raw) = settings.get("toml").and_then(Value::as_str)
        && let Ok(doc) = raw.parse::<toml_edit::DocumentMut>()
        && let Some(selector) = doc.get("model_provider").and_then(|value| value.as_str())
        && let Some(table) = doc
            .get("model_providers")
            .and_then(|providers| providers.get(selector))
    {
        let base = table.get("base_url").and_then(|value| value.as_str())?;
        let key = table
            .get("api_key")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        return Some((base.into(), key.into()));
    }

    // OpenCode stores provider entries under provider.<id>.options.
    if let Some(providers) = settings.get("provider").and_then(Value::as_object) {
        for provider in providers.values() {
            if let Some(base) = provider
                .pointer("/options/baseURL")
                .or_else(|| provider.pointer("/options/base_url"))
                .and_then(Value::as_str)
            {
                let key = provider
                    .pointer("/options/apiKey")
                    .or_else(|| provider.pointer("/options/api_key"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                return Some((base.into(), key.into()));
            }
        }
    }

    // camelCase/snake_case and Claude env shapes.
    let base = settings
        .get("baseUrl")
        .or_else(|| settings.get("base_url"))
        .or_else(|| settings.get("baseURL"))
        .or_else(|| settings.pointer("/env/ANTHROPIC_BASE_URL"))
        .or_else(|| settings.pointer("/env/GOOGLE_GEMINI_BASE_URL"))
        .and_then(Value::as_str)?;
    let key = settings
        .get("apiKey")
        .or_else(|| settings.get("api_key"))
        .or_else(|| settings.get("api_keyEnv"))
        .and_then(Value::as_str)
        .or_else(|| {
            settings
                .pointer("/env/ANTHROPIC_AUTH_TOKEN")
                .and_then(Value::as_str)
        })
        .or_else(|| {
            settings
                .pointer("/env/ANTHROPIC_API_KEY")
                .and_then(Value::as_str)
        })
        .unwrap_or("");
    Some((base.to_string(), key.to_string()))
}

/// Build the Authorization header value for a models request.
pub fn auth_header(api_key: &str) -> String {
    format!("Bearer {api_key}")
}

/// Live fetch: GET the provider's models endpoint with a 10s timeout.
/// `base_url` comes from `provider_endpoint`; `api_key` may be empty for
/// open endpoints (the request is simply sent without auth).
pub fn fetch_models(base_url: &str, api_key: &str) -> Result<ModelsFetchResult, ModelsFetchError> {
    let url = models_url(base_url)?;
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(10))
        .build();
    let mut request = agent.get(&url).set("Accept", "application/json");
    if !api_key.trim().is_empty() {
        request = request.set("Authorization", &auth_header(api_key));
    }
    let response = request.call().map_err(|e| match e {
        ureq::Error::Status(401, _) | ureq::Error::Status(403, _) => ModelsFetchError::Auth,
        other => ModelsFetchError::Network(other.to_string()),
    })?;
    let body: Value = response
        .into_json()
        .map_err(|e| ModelsFetchError::Parse(e.to_string()))?;
    let models = parse_openai_models(&body);
    if models.is_empty() {
        return Err(ModelsFetchError::Unsupported(
            "no models in response".into(),
        ));
    }
    Ok(ModelsFetchResult {
        models,
        raw: Some(body),
    })
}

/// Connectivity test used by provider cards and the batch-test action.
/// A successful `/v1/models` response proves URL + authentication and also
/// returns the model count; failures retain bounded diagnostic text.
pub fn test_connectivity(settings: &Value) -> ConnectivityResult {
    let start = std::time::Instant::now();
    let Some((base_url, api_key)) = provider_endpoint(settings) else {
        return ConnectivityResult {
            ok: false,
            latency_ms: 0,
            status: None,
            message: "provider config lacks baseUrl".into(),
            models_count: 0,
        };
    };
    match fetch_models(&base_url, &api_key) {
        Ok(result) => ConnectivityResult {
            ok: true,
            latency_ms: start.elapsed().as_millis(),
            status: Some(200),
            message: "ok".into(),
            models_count: result.models.len(),
        },
        Err(ModelsFetchError::Auth) => ConnectivityResult {
            ok: false,
            latency_ms: start.elapsed().as_millis(),
            status: Some(401),
            message: "authentication failed".into(),
            models_count: 0,
        },
        Err(error) => ConnectivityResult {
            ok: false,
            latency_ms: start.elapsed().as_millis(),
            status: None,
            message: format!("{error:?}").chars().take(300).collect(),
            models_count: 0,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn models_url_shapes() {
        assert_eq!(
            models_url("https://api.example.com/v1").unwrap(),
            "https://api.example.com/v1/models"
        );
        assert_eq!(
            models_url("https://api.example.com").unwrap(),
            "https://api.example.com/v1/models"
        );
        assert_eq!(
            models_url("api.example.com/v1/").unwrap(),
            "https://api.example.com/v1/models"
        );
        assert!(models_url("").is_err());
        assert!(models_url("   ").is_err());
    }

    #[test]
    fn parses_openai_models() {
        let body = json!({
            "object": "list",
            "data": [
                {"id": "m-1", "display_name": "Model One", "context_length": 128000,
                 "input_price": 0.5, "output_price": 1.5, "tool_support": true},
                {"id": "m-2"},
                {"no_id": true}
            ]
        });
        let models = parse_openai_models(&body);
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "m-1");
        assert_eq!(models[0].display_name.as_deref(), Some("Model One"));
        assert_eq!(models[0].context_length, Some(128000));
        assert_eq!(models[0].input_price, Some(0.5));
        assert_eq!(models[0].tool_support, Some(true));
        assert!(models[0].vision_support.is_none());
        assert_eq!(models[1].id, "m-2");
    }

    #[test]
    fn endpoint_extraction_all_field_shapes() {
        // Pi/OMP camelCase
        let v = json!({"baseUrl": "https://a/v1", "apiKey": "k1"});
        assert_eq!(
            provider_endpoint(&v),
            Some(("https://a/v1".into(), "k1".into()))
        );
        // snake_case
        let v = json!({"base_url": "https://b/v1", "api_key": "k2"});
        assert_eq!(
            provider_endpoint(&v),
            Some(("https://b/v1".into(), "k2".into()))
        );
        // Claude env style
        let v = json!({"env": {"ANTHROPIC_BASE_URL": "https://c", "ANTHROPIC_AUTH_TOKEN": "k3"}});
        assert_eq!(
            provider_endpoint(&v),
            Some(("https://c".into(), "k3".into()))
        );

        // Codex TOML projection
        let v = json!({"toml": "model_provider = \"x\"\n[model_providers.x]\nbase_url = \"https://x/v1\"\napi_key = \"kx\"\n"});
        assert_eq!(
            provider_endpoint(&v),
            Some(("https://x/v1".into(), "kx".into()))
        );

        // OpenCode provider map
        let v =
            json!({"provider": {"q": {"options": {"baseURL": "https://q/v1", "apiKey": "kq"}}}});
        assert_eq!(
            provider_endpoint(&v),
            Some(("https://q/v1".into(), "kq".into()))
        );
    }

    #[test]
    fn auth_header_format() {
        assert_eq!(auth_header("sk-1"), "Bearer sk-1");
    }
}
