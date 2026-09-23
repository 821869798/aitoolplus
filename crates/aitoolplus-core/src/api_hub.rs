//! API Hub: fetch model lists from provider endpoints (`/v1/models`) with
//! graceful degradation, candidate fallback, and typed results for UI rendering.
//!
//! Aligns with cc-switch and ai-toolbox model-fetch architecture.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// One model entry from a provider's model list.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchedModel {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owned_by: Option<String>,
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

/// Known Anthropic protocol compatibility suffixes (sorted by length descending).
pub const KNOWN_COMPAT_SUFFIXES: &[&str] = &[
    "/api/claudecode",
    "/api/anthropic",
    "/apps/anthropic",
    "/api/coding",
    "/claudecode",
    "/anthropic",
    "/step_plan",
    "/coding",
    "/claude",
];

/// Returns true if url ends with `/v{N}` (where N is one or more digits).
pub fn ends_with_version_segment(url: &str) -> bool {
    let last = url.rsplit('/').next().unwrap_or("");
    last.strip_prefix('v')
        .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
}

/// Strips known compatibility subpaths from the end of the base URL.
pub fn strip_compat_suffix(base_url: &str) -> Option<&str> {
    for suffix in KNOWN_COMPAT_SUFFIXES {
        if base_url.ends_with(*suffix) {
            return Some(&base_url[..base_url.len() - suffix.len()]);
        }
    }
    None
}

/// Build candidate endpoints for `/v1/models` retrieval.
/// Aligned with cc-switch `build_models_url_candidates`.
pub fn build_models_url_candidates(
    base_url: &str,
    is_full_url: bool,
    models_url_override: Option<&str>,
) -> Result<Vec<String>, ModelsFetchError> {
    if let Some(raw) = models_url_override {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return Ok(vec![trimmed.to_string()]);
        }
    }

    let trimmed = base_url.trim();
    if trimmed.is_empty() {
        return Err(ModelsFetchError::Unsupported("Base URL is empty".into()));
    }

    let with_scheme = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    let clean = with_scheme.trim_end_matches('/');

    let mut candidates: Vec<String> = Vec::new();

    if is_full_url {
        if let Some(idx) = clean.find("/v1/") {
            candidates.push(format!("{}/v1/models", &clean[..idx]));
        } else if let Some(idx) = clean.rfind('/') {
            let root = &clean[..idx];
            if root.contains("://") && root.len() > root.find("://").unwrap() + 3 {
                candidates.push(format!("{root}/v1/models"));
            }
        }
        if candidates.is_empty() {
            return Err(ModelsFetchError::Unsupported(
                "Cannot derive models endpoint from full URL".into(),
            ));
        }
        return Ok(candidates);
    }

    if ends_with_version_segment(clean) {
        candidates.push(format!("{clean}/models"));
        if !clean.ends_with("/v1") {
            candidates.push(format!("{clean}/v1/models"));
        }
    } else {
        candidates.push(format!("{clean}/v1/models"));
        candidates.push(format!("{clean}/models"));
    }

    if let Some(stripped) = strip_compat_suffix(clean) {
        let root = stripped.trim_end_matches('/');
        if !root.is_empty() && root.contains("://") {
            candidates.push(format!("{root}/v1/models"));
            candidates.push(format!("{root}/models"));
        }
    }

    let mut unique = Vec::with_capacity(candidates.len());
    for url in candidates {
        if !unique.contains(&url) {
            unique.push(url);
        }
    }

    Ok(unique)
}

/// Normalize a single base URL for backward compatibility.
pub fn models_url(base_url: &str) -> Result<String, ModelsFetchError> {
    let candidates = build_models_url_candidates(base_url, false, None)?;
    candidates
        .into_iter()
        .next()
        .ok_or_else(|| ModelsFetchError::Unsupported("empty base_url".into()))
}

/// Build authorization and custom headers for model discovery.
pub fn build_auth_headers(
    api_key: &str,
    api_format: Option<&str>,
    custom_headers: Option<&BTreeMap<String, String>>,
) -> Vec<(String, String)> {
    let mut headers = Vec::new();
    let key = api_key.trim();
    if !key.is_empty() {
        // ALWAYS include standard Bearer authorization.
        // /v1/models is an OpenAI-standard route. Almost all reverse proxies and gateways
        // (including AnyRouter, OneAPI, NewAPI, PackHub, OpenRouter, DeepSeek) strictly require
        // `Authorization: Bearer <key>` to query /v1/models, even when proxying Claude or Codex.
        headers.push(("Authorization".to_string(), format!("Bearer {key}")));

        // In addition, supply protocol-specific headers (e.g. Anthropic, Gemini) so native
        // or protocol-strict endpoints also succeed.
        match api_format {
            Some("anthropic-messages") | Some("anthropic") => {
                headers.push(("x-api-key".to_string(), key.to_string()));
                headers.push(("anthropic-version".to_string(), "2023-06-01".to_string()));
            }
            Some("google-generative-ai") | Some("gemini") | Some("gemini_native") => {
                headers.push(("x-goog-api-key".to_string(), key.to_string()));
            }
            _ => {
                headers.push(("x-api-key".to_string(), key.to_string()));
            }
        }
    }
    if let Some(custom) = custom_headers {
        for (k, v) in custom {
            let k_trimmed = k.trim();
            if !k_trimmed.is_empty() {
                if let Some(pos) = headers.iter().position(|(hk, _)| hk.eq_ignore_ascii_case(k_trimmed)) {
                    headers[pos] = (k_trimmed.to_string(), v.trim().to_string());
                } else {
                    headers.push((k_trimmed.to_string(), v.trim().to_string()));
                }
            }
        }
    }
    headers
}

/// Parse custom headers from raw string format (JSON object or comma/newline separated `Key: Value`).
pub fn parse_custom_headers(raw: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return map;
    }

    // Try parsing as JSON object
    if trimmed.starts_with('{') && trimmed.ends_with('}')
        && let Ok(json_map) = serde_json::from_str::<BTreeMap<String, Value>>(trimmed)
    {
        for (k, v) in json_map {
            let val_str = match v {
                Value::String(s) => s,
                other => other.to_string(),
            };
            map.insert(k, val_str);
        }
        return map;
    }

    // Parse as newline/comma separated key-value pairs
    for line in trimmed.split(['\n', ',']) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((k, v)) = line.split_once(':') {
            let k = k.trim();
            let v = v.trim();
            if !k.is_empty() {
                map.insert(k.to_string(), v.to_string());
            }
        }
    }

    map
}

/// Parse an OpenAI-style `/v1/models` response into entries.
/// Accepts `{ "data": [...] }`, `[...]`, or `{ "models": [...] }` (Google Gemini).
pub fn parse_openai_models(body: &Value) -> Vec<FetchedModel> {
    let arr = if let Some(a) = body.get("data").and_then(Value::as_array) {
        a
    } else if let Some(a) = body.as_array() {
        a
    } else if let Some(a) = body.get("models").and_then(Value::as_array) {
        a
    } else {
        return vec![];
    };
    arr.iter()
        .filter_map(|m| {
            let id = m.get("id").or_else(|| m.get("name")).and_then(Value::as_str)?;
            let clean_id = id.strip_prefix("models/").unwrap_or(id);
            let owned_by = m
                .get("owned_by")
                .or_else(|| m.get("ownedBy"))
                .and_then(Value::as_str)
                .map(String::from);
            Some(FetchedModel {
                id: clean_id.to_string(),
                owned_by,
                display_name: m
                    .get("display_name")
                    .or_else(|| m.get("displayName"))
                    .or_else(|| m.get("name"))
                    .and_then(Value::as_str)
                    .map(String::from),
                context_length: m
                    .get("context_length")
                    .or_else(|| m.get("context_length_tokens"))
                    .or_else(|| m.get("input_token_limit"))
                    .and_then(Value::as_u64),
                input_price: m.get("input_price").and_then(Value::as_f64),
                output_price: m.get("output_price").and_then(Value::as_f64),
                tool_support: m.get("tool_support").and_then(Value::as_bool),
                vision_support: m.get("vision_support").and_then(Value::as_bool),
            })
        })
        .collect()
}

/// Extract the api key + base URL from a provider record's settings JSON.
pub fn provider_endpoint(settings: &Value) -> Option<(String, String)> {
    // Codex stores a TOML projection in {"toml":"..."} or {"config":"..."}.
    if let Some(raw) = settings
        .get("toml")
        .or_else(|| settings.get("config"))
        .and_then(Value::as_str)
        && let Ok(doc) = raw.parse::<toml_edit::DocumentMut>()
    {
        let selector = doc
            .get("model_provider")
            .and_then(|value| value.as_str())
            .unwrap_or("custom");
        if let Some(table) = doc
            .get("model_providers")
            .and_then(|providers| providers.get(selector))
        {
            if let Some(base) = table.get("base_url").and_then(|value| value.as_str()) {
                let mut key = table
                    .get("api_key")
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .to_string();
                if key.is_empty() {
                    if let Some(auth_key) = settings
                        .pointer("/auth/OPENAI_API_KEY")
                        .or_else(|| settings.pointer("/auth/api_key"))
                        .or_else(|| settings.pointer("/auth/token"))
                        .and_then(Value::as_str)
                    {
                        key = auth_key.to_string();
                    }
                }
                return Some((base.into(), key));
            }
        }
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

    // camelCase/snake_case, Claude env shapes, and Gemini env shapes.
    let base = settings
        .get("baseUrl")
        .or_else(|| settings.get("base_url"))
        .or_else(|| settings.get("baseURL"))
        .or_else(|| settings.pointer("/env/ANTHROPIC_BASE_URL"))
        .or_else(|| settings.pointer("/env/GOOGLE_GEMINI_BASE_URL"))
        .or_else(|| settings.pointer("/env/GEMINI_BASE_URL"))
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
        .or_else(|| {
            settings
                .pointer("/env/GEMINI_API_KEY")
                .and_then(Value::as_str)
        })
        .or_else(|| {
            settings
                .pointer("/env/GOOGLE_API_KEY")
                .and_then(Value::as_str)
        })
        .or_else(|| {
            settings
                .pointer("/auth/OPENAI_API_KEY")
                .and_then(Value::as_str)
        })
        .unwrap_or("");
    Some((base.to_string(), key.to_string()))
}

/// Build the Authorization header value for a models request.
pub fn auth_header(api_key: &str) -> String {
    format!("Bearer {api_key}")
}

/// Live fetch: GET provider models with candidate URLs, headers, and fallback.
pub fn fetch_models_advanced(
    base_url: &str,
    api_key: &str,
    api_format: Option<&str>,
    custom_headers: Option<&BTreeMap<String, String>>,
    models_url_override: Option<&str>,
) -> Result<ModelsFetchResult, ModelsFetchError> {
    let candidates = build_models_url_candidates(base_url, false, models_url_override)?;
    let headers = build_auth_headers(api_key, api_format, custom_headers);

    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(15))
        .build();

    let mut had_auth_error = false;
    let mut last_auth_detail: Option<String> = None;
    let mut last_err = String::new();

    let has_custom_ua = headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("user-agent"));
    let default_ua = if !has_custom_ua && (base_url.contains("agentrouter") || api_format.map_or(false, |f| f.contains("anthropic"))) {
        "claude-cli/2.1.219 (external, sdk-cli)"
    } else {
        "aitoolplus/1.0"
    };

    for url in &candidates {
        let mut request = agent
            .get(url)
            .set("Accept", "application/json");

        if !has_custom_ua {
            request = request.set("User-Agent", default_ua);
        }

        for (k, v) in &headers {
            request = request.set(k, v);
        }

        match request.call() {
            Ok(response) => {
                let body: Value = response
                    .into_json()
                    .map_err(|e| ModelsFetchError::Parse(e.to_string()))?;
                let mut models = parse_openai_models(&body);
                if !models.is_empty() {
                    models.sort_by_key(|a| a.id.to_lowercase());
                    return Ok(ModelsFetchResult {
                        models,
                        raw: Some(body),
                    });
                }
            }
            Err(ureq::Error::Status(status, response)) if status == 401 || status == 403 => {
                had_auth_error = true;
                let mut is_unauthorized_client = false;
                if let Ok(val) = response.into_json::<Value>() {
                    if let Some(msg) = val
                        .pointer("/error/message")
                        .or_else(|| val.pointer("/message"))
                        .and_then(Value::as_str)
                    {
                        if msg.to_ascii_lowercase().contains("unauthorized client")
                            || msg.to_ascii_lowercase().contains("unauthorized_client")
                        {
                            is_unauthorized_client = true;
                        }
                        last_auth_detail = Some(msg.trim().to_string());
                    }
                }

                // If blocked by client fingerprinting (e.g. AgentRouter WAF), auto-retry with Claude CLI headers!
                if is_unauthorized_client {
                    let mut retry_req = agent
                        .get(url)
                        .set("Accept", "application/json")
                        .set("User-Agent", "claude-cli/2.1.219 (external, sdk-cli)")
                        .set("anthropic-version", "2023-06-01")
                        .set("x-app", "cli");
                    for (k, v) in &headers {
                        if !k.eq_ignore_ascii_case("user-agent") {
                            retry_req = retry_req.set(k, v);
                        }
                    }
                    if let Ok(retry_resp) = retry_req.call() {
                        if let Ok(body) = retry_resp.into_json::<Value>() {
                            let mut models = parse_openai_models(&body);
                            if !models.is_empty() {
                                models.sort_by_key(|a| a.id.to_lowercase());
                                return Ok(ModelsFetchResult {
                                    models,
                                    raw: Some(body),
                                });
                            }
                        }
                    }
                }

                if last_auth_detail.is_none() {
                    last_err = format!("Candidate {url} returned {status}");
                }
                continue;
            }
            Err(ureq::Error::Status(404, _) | ureq::Error::Status(405, _)) => {
                last_err = format!("Candidate {url} returned 404/405");
                continue;
            }
            Err(other) => {
                last_err = other.to_string();
                continue;
            }
        }
    }

    if had_auth_error {
        if let Some(msg) = last_auth_detail {
            if msg.to_ascii_lowercase().contains("unauthorized client")
                || msg.to_ascii_lowercase().contains("unauthorized_client")
            {
                Err(ModelsFetchError::Unsupported(format!(
                    "服务商网关拦截: {msg}（该服务商限制了客户端类型，不支持自动拉取模型列表，请直接在添加模型中手动输入模型名称）"
                )))
            } else {
                Err(ModelsFetchError::Unsupported(format!(
                    "认证失败 (401/403): {msg}"
                )))
            }
        } else {
            Err(ModelsFetchError::Auth)
        }
    } else if !last_err.is_empty() {
        Err(ModelsFetchError::Network(last_err))
    } else {
        Err(ModelsFetchError::Unsupported(
            "No models returned from provider endpoints".into(),
        ))
    }
}

/// Backward-compatible live fetch.
pub fn fetch_models(base_url: &str, api_key: &str) -> Result<ModelsFetchResult, ModelsFetchError> {
    fetch_models_advanced(base_url, api_key, None, None, None)
}

/// Connectivity test used by provider cards and the batch-test action.
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
    fn test_build_models_url_candidates_standard() {
        let candidates =
            build_models_url_candidates("https://api.openai.com/v1", false, None).unwrap();
        assert_eq!(candidates[0], "https://api.openai.com/v1/models");

        let candidates =
            build_models_url_candidates("https://api.deepseek.com", false, None).unwrap();
        assert!(candidates.contains(&"https://api.deepseek.com/v1/models".to_string()));
    }

    #[test]
    fn test_build_models_url_candidates_versioned() {
        // e.g. ZhiPu GLM Coding Plan
        let candidates =
            build_models_url_candidates("https://open.bigmodel.cn/api/coding/paas/v4", false, None)
                .unwrap();
        assert_eq!(
            candidates[0],
            "https://open.bigmodel.cn/api/coding/paas/v4/models"
        );
        assert!(candidates.contains(&"https://open.bigmodel.cn/api/coding/paas/v4/v1/models".to_string()));
    }

    #[test]
    fn test_build_models_url_candidates_compat_suffix() {
        let candidates =
            build_models_url_candidates("https://api.example.com/api/anthropic", false, None)
                .unwrap();
        assert!(candidates.contains(&"https://api.example.com/v1/models".to_string()));
        assert!(candidates.contains(&"https://api.example.com/models".to_string()));
    }

    #[test]
    fn test_build_models_url_candidates_override() {
        let candidates = build_models_url_candidates(
            "https://api.example.com",
            false,
            Some("https://custom.endpoint/models"),
        )
        .unwrap();
        assert_eq!(candidates, vec!["https://custom.endpoint/models"]);
    }

    #[test]
    fn test_build_auth_headers() {
        let anthropic = build_auth_headers("sk-ant", Some("anthropic"), None);
        assert!(anthropic.contains(&("Authorization".to_string(), "Bearer sk-ant".to_string())));
        assert!(anthropic.contains(&("x-api-key".to_string(), "sk-ant".to_string())));
        assert!(anthropic.contains(&("anthropic-version".to_string(), "2023-06-01".to_string())));

        let gemini = build_auth_headers("gem-key", Some("gemini"), None);
        assert!(gemini.contains(&("Authorization".to_string(), "Bearer gem-key".to_string())));
        assert!(gemini.contains(&("x-goog-api-key".to_string(), "gem-key".to_string())));

        let default_auth = build_auth_headers("sk-open", None, None);
        assert!(default_auth.contains(&("Authorization".to_string(), "Bearer sk-open".to_string())));
        assert!(default_auth.contains(&("x-api-key".to_string(), "sk-open".to_string())));
    }

    #[test]
    fn test_parse_custom_headers() {
        let raw = "X-Custom: Value1\nAuthorization-Alt: Token2";
        let map = parse_custom_headers(raw);
        assert_eq!(map.get("X-Custom").map(|s| s.as_str()), Some("Value1"));
        assert_eq!(
            map.get("Authorization-Alt").map(|s| s.as_str()),
            Some("Token2")
        );

        let json_raw = r#"{"X-Foo": "Bar", "X-Num": 123}"#;
        let map = parse_custom_headers(json_raw);
        assert_eq!(map.get("X-Foo").map(|s| s.as_str()), Some("Bar"));
    }

    #[test]
    fn test_parses_openai_models_with_owned_by() {
        let body = json!({
            "object": "list",
            "data": [
                {
                    "id": "claude-3-7-sonnet",
                    "owned_by": "anthropic",
                    "display_name": "Claude 3.7 Sonnet"
                },
                {
                    "id": "deepseek-chat",
                    "owned_by": "deepseek"
                }
            ]
        });
        let models = parse_openai_models(&body);
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "claude-3-7-sonnet");
        assert_eq!(models[0].owned_by.as_deref(), Some("anthropic"));
        assert_eq!(models[1].id, "deepseek-chat");
        assert_eq!(models[1].owned_by.as_deref(), Some("deepseek"));
    }

    #[test]
    #[ignore]
    fn test_anyrouter_claude_and_codex_fetch() {
        let base_url = "https://anyrouter.top";
        let api_key = "sk-Lcs8bYuZUPSNhvO9lWE4wfriQlgCxkqmYfzlThiTi8iR7bTU";
        // Claude Code fetch simulation (with api_format: "anthropic")
        let claude_res = fetch_models_advanced(base_url, api_key, Some("anthropic"), None, None);
        assert!(claude_res.is_ok(), "Claude AnyRouter fetch failed: {:?}", claude_res.err());
        let claude_models = claude_res.unwrap().models;
        assert!(!claude_models.is_empty());
        assert!(claude_models.iter().any(|m| m.id.starts_with("claude-")));

        // Codex fetch simulation (base_url: "https://anyrouter.top/v1", api_format: "openai_responses")
        let codex_res = fetch_models_advanced("https://anyrouter.top/v1", api_key, Some("openai_responses"), None, None);
        assert!(codex_res.is_ok(), "Codex AnyRouter fetch failed: {:?}", codex_res.err());
        let codex_models = codex_res.unwrap().models;
        assert!(!codex_models.is_empty());
    }

    #[test]
    #[ignore]
    fn test_agentrouter_fetch() {
        let base_url = "https://agentrouter.org";
        let api_key = "sk-Iw3m9qsWSiGBr22ziZBvKGjmqZRmNmM1pHHUvsxedlHOLfbI";
        let res = fetch_models_advanced(base_url, api_key, Some("anthropic-messages"), None, None);
        assert!(res.is_ok(), "AgentRouter fetch failed: {:?}", res.err());
        let models = res.unwrap().models;
        assert!(!models.is_empty(), "Models should not be empty");
        assert!(models.iter().any(|m| m.id == "claude-opus-5"), "Should contain claude-opus-5");
    }
}
