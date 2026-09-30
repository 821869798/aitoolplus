use std::sync::Arc;
use std::time::Instant;
use bytes::Bytes;
use reqwest::header::{HeaderName, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde_json::Value;

use crate::gateway::circuit_breaker::CircuitBreakerRegistry;
use crate::gateway::router::{GatewayRouteTarget, UpstreamAuthType};
use crate::gateway::transformer::anthropic::{
    anthropic_to_openai_request, openai_response_to_anthropic,
    transform_openai_chunk_to_anthropic_sse, AnthropicSseState,
};
use crate::gateway::transformer::openai::{
    chat_response_to_responses, responses_to_chat_request,
};
use crate::gateway::transformer::types::AiProtocol;
use crate::gateway::types::GatewayCliKey;

#[derive(Debug)]
pub struct ForwardResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Bytes,
    pub is_streaming: bool,
    pub provider_id: String,
    pub provider_name: String,
    pub model: String,
    pub failover_occurred: bool,
    pub duration_ms: u64,
    pub attempts: Vec<crate::gateway::types::GatewayAttemptTrace>,
}

/// Strips local [1M] or [1m] context marker from model names before sending to upstream API.
/// (Anthropic upstream APIs and proxies reject models with '[1M]' suffix).
pub fn strip_one_m_marker(model: &str) -> &str {
    let trimmed = model.trim();
    let bytes = trimmed.as_bytes();
    for marker in [&b"[1m]"[..], &b"%5b1m%5d"[..]] {
        if bytes.len() >= marker.len()
            && bytes[bytes.len() - marker.len()..].eq_ignore_ascii_case(marker)
        {
            return trimmed[..trimmed.len() - marker.len()].trim();
        }
    }
    trimmed
}

/// Builds the effective target URL for upstream requests, aligning with ai-toolbox & cc-switch:
/// 1. Strips leading /v1 from incoming path if base_url path ends with /v1 (prevents /v1/v1 duplication).
/// 2. If base_url is already a full endpoint (e.g. ends in /v1/messages or /chat/completions), avoids re-appending.
/// 3. Preserves query string parameters (such as `?beta=true`).
pub fn build_target_url(base_url: &str, forwarded_path_and_query: &str) -> String {
    let trimmed_base = base_url.trim().trim_end_matches('/');
    let (path, query) = match forwarded_path_and_query.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (forwarded_path_and_query, None),
    };

    let base_lower = trimmed_base.to_ascii_lowercase();
    let is_full_endpoint = base_lower.ends_with("/v1/messages")
        || base_lower.ends_with("/chat/completions")
        || base_lower.ends_with("/responses");

    let combined = if is_full_endpoint {
        trimmed_base.to_string()
    } else {
        let eff_path = if base_lower.ends_with("/v1") && (path == "/v1" || path.starts_with("/v1/")) {
            path.strip_prefix("/v1").unwrap_or(path)
        } else {
            path
        };
        let formatted = format!("{trimmed_base}/{}", eff_path.trim_start_matches('/'));
        let mut res = formatted;
        while res.contains("/v1/v1") {
            res = res.replace("/v1/v1", "/v1");
        }
        res
    };

    if let Some(q) = query {
        if !q.is_empty() {
            if combined.contains('?') {
                format!("{combined}&{q}")
            } else {
                format!("{combined}?{q}")
            }
        } else {
            combined
        }
    } else {
        combined
    }
}

/// Recursively removes private parameters starting with `_` from the request JSON body,
/// matching cc-switch's `body_filter.rs`.
pub fn filter_private_params(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let filtered: serde_json::Map<String, Value> = map
                .into_iter()
                .filter_map(|(k, v)| {
                    if k.starts_with('_') {
                        None
                    } else {
                        Some((k, filter_private_params(v)))
                    }
                })
                .collect();
            Value::Object(filtered)
        }
        Value::Array(arr) => {
            Value::Array(arr.into_iter().map(filter_private_params).collect())
        }
        other => other,
    }
}

/// DeepSeek's official Anthropic-compatible endpoint treats
/// `thinking: { type: "disabled" }` and effort parameters (`output_config.effort`
/// or `reasoning_effort`) as mutually exclusive, returning HTTP 400:
/// "thinking options type cannot be disabled when reasoning_effort is set".
/// Parity with cc-switch normalize_deepseek_thinking_disabled_strip_effort.
pub fn normalize_deepseek_thinking_effort(body: &mut Value) {
    let thinking_type = body
        .get("thinking")
        .and_then(|t| t.get("type"))
        .and_then(|t| t.as_str());

    if thinking_type == Some("disabled") {
        if let Some(oc) = body.get_mut("output_config").and_then(Value::as_object_mut) {
            oc.remove("effort");
            if oc.is_empty() {
                if let Some(root) = body.as_object_mut() {
                    root.remove("output_config");
                }
            }
        }
        if let Some(root) = body.as_object_mut() {
            root.remove("reasoning_effort");
        }
    }
}

/// Check if an Anthropic error indicates thinking signature or thinking block failure
/// (compatible with cc-switch thinking_rectifier).
pub fn should_rectify_thinking_signature(error_message: Option<&str>) -> bool {
    let Some(msg) = error_message else { return false; };
    let lower = msg.to_lowercase();
    (lower.contains("invalid") && lower.contains("signature"))
        || (lower.contains("thought signature") && (lower.contains("not valid") || lower.contains("invalid")))
        || lower.contains("must start with a thinking block")
        || (lower.contains("expected") && (lower.contains("thinking") || lower.contains("redacted_thinking")) && lower.contains("found") && lower.contains("tool_use"))
        || (lower.contains("signature") && lower.contains("field required"))
        || (lower.contains("signature") && lower.contains("extra inputs are not permitted"))
        || ((lower.contains("thinking") || lower.contains("redacted_thinking")) && lower.contains("cannot be modified"))
        || lower.contains("非法请求")
        || lower.contains("illegal request")
        || lower.contains("invalid request")
}

/// Strip thinking/redacted_thinking blocks and signatures from Anthropic request body in-place.
pub fn rectify_anthropic_request(body: &mut Value) -> bool {
    let mut modified = false;

    if let Some(messages) = body.get_mut("messages").and_then(|m| m.as_array_mut()) {
        for msg in messages.iter_mut() {
            if let Some(content) = msg.get_mut("content").and_then(|c| c.as_array_mut()) {
                let mut new_content = Vec::with_capacity(content.len());
                for block in content.iter() {
                    let block_type = block.get("type").and_then(|t| t.as_str());
                    if block_type == Some("thinking") || block_type == Some("redacted_thinking") {
                        modified = true;
                        continue;
                    }
                    if block.get("signature").is_some() {
                        let mut block_clone = block.clone();
                        if let Some(obj) = block_clone.as_object_mut() {
                            obj.remove("signature");
                            modified = true;
                            new_content.push(Value::Object(obj.clone()));
                            continue;
                        }
                    }
                    new_content.push(block.clone());
                }
                if modified {
                    *content = new_content;
                }
            }
        }
    }

    if let Some(thinking) = body.get("thinking") {
        if thinking.get("type").and_then(|t| t.as_str()) == Some("enabled") {
            if let Some(messages) = body.get("messages").and_then(|m| m.as_array()) {
                let last_assistant = messages
                    .iter()
                    .rev()
                    .find(|m| m.get("role").and_then(|r| r.as_str()) == Some("assistant"));
                let starts_with_thinking = last_assistant
                    .and_then(|m| m.get("content").and_then(|c| c.as_array()))
                    .and_then(|c| c.first())
                    .and_then(|b| b.get("type").and_then(|t| t.as_str()))
                    .map(|t| t == "thinking" || t == "redacted_thinking")
                    .unwrap_or(false);
                if !starts_with_thinking {
                    if let Some(obj) = body.as_object_mut() {
                        obj.remove("thinking");
                        modified = true;
                    }
                }
            }
        }
    }

    modified
}

pub struct GatewayForwarder {
    client: reqwest::Client,
    circuit_breaker: Arc<CircuitBreakerRegistry>,
}

impl GatewayForwarder {
    pub fn new(circuit_breaker: Arc<CircuitBreakerRegistry>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .unwrap_or_default();
        Self {
            client,
            circuit_breaker,
        }
    }

    pub fn is_retryable_status(status: u16) -> bool {
        // Any HTTP 4xx (client/auth/model/rate errors on that upstream)
        // or 5xx (server errors/timeouts) are eligible for failover to backup providers.
        status >= 400
    }

    /// Forward a request through the failover candidate queue.
    pub async fn forward_with_failover(
        &self,
        cli_key: GatewayCliKey,
        path_and_query: &str,
        inbound_headers: &[(String, String)],
        inbound_body: &[u8],
        candidates: &[GatewayRouteTarget],
        failover_enabled: bool,
    ) -> Result<ForwardResponse, String> {
        if candidates.is_empty() {
            return Err("No healthy upstream providers available in Gateway queue".to_string());
        }

        let start_time = Instant::now();
        let mut last_error = String::from("No candidates attempted");
        let mut failover_occurred = false;
        let mut attempts = Vec::new();

        let all_candidates_open = candidates.iter().all(|c| !self.circuit_breaker.can_request(cli_key.as_str(), &c.provider_id));
        let bypass_circuit_breaker = !failover_enabled || candidates.len() == 1 || all_candidates_open;

        for (idx, target) in candidates.iter().enumerate() {
            // When failover is disabled, only attempt the primary provider (idx == 0)
            if !failover_enabled && idx > 0 {
                break;
            }

            let attempt_start = Instant::now();
            if idx > 0 {
                failover_occurred = true;
            }

            if !self.circuit_breaker.can_request(cli_key.as_str(), &target.provider_id) && !bypass_circuit_breaker {
                tracing::warn!(
                    "Gateway circuit breaker open for provider {}, skipping",
                    target.provider_name
                );
                attempts.push(crate::gateway::types::GatewayAttemptTrace {
                    provider_id: target.provider_id.clone(),
                    provider_name: target.provider_name.clone(),
                    model: target.default_model.clone().unwrap_or_else(|| "default".into()),
                    status_code: None,
                    duration_ms: 0,
                    error: Some("熔断器开启 (Circuit breaker open)".into()),
                });
                if !failover_enabled {
                    return Err(format!("主提供商 {} 处于熔断保护状态，且故障转移已停用", target.provider_name));
                }
                continue;
            }

            match self.dispatch_to_target(cli_key, path_and_query, inbound_headers, inbound_body, target).await {
                Ok(resp) => {
                    let attempt_dur = attempt_start.elapsed().as_millis() as u64;
                    if Self::is_retryable_status(resp.status) {
                        self.circuit_breaker.record_failure(cli_key.as_str(), &target.provider_id);
                        let err_msg = format!("HTTP {}", resp.status);
                        attempts.push(crate::gateway::types::GatewayAttemptTrace {
                            provider_id: target.provider_id.clone(),
                            provider_name: target.provider_name.clone(),
                            model: resp.model.clone(),
                            status_code: Some(resp.status),
                            duration_ms: attempt_dur,
                            error: Some(err_msg.clone()),
                        });
                        if failover_enabled && idx + 1 < candidates.len() {
                            tracing::warn!(
                                "Gateway upstream {} returned error status {}, failing over to next candidate (candidate {}/{})",
                                target.provider_name,
                                resp.status,
                                idx + 1,
                                candidates.len()
                            );
                            last_error = format!("{} returned status {}", target.provider_name, resp.status);
                            continue;
                        }
                    } else {
                        self.circuit_breaker.record_success(cli_key.as_str(), &target.provider_id);
                        attempts.push(crate::gateway::types::GatewayAttemptTrace {
                            provider_id: target.provider_id.clone(),
                            provider_name: target.provider_name.clone(),
                            model: resp.model.clone(),
                            status_code: Some(resp.status),
                            duration_ms: attempt_dur,
                            error: None,
                        });
                    }

                    return Ok(ForwardResponse {
                        status: resp.status,
                        headers: resp.headers,
                        body: resp.body,
                        is_streaming: resp.is_streaming,
                        provider_id: target.provider_id.clone(),
                        provider_name: target.provider_name.clone(),
                        model: resp.model,
                        failover_occurred,
                        duration_ms: start_time.elapsed().as_millis() as u64,
                        attempts,
                    });
                }
                Err(err) => {
                    let attempt_dur = attempt_start.elapsed().as_millis() as u64;
                    tracing::warn!(
                        "Gateway upstream {} failed with error: {}, failing over",
                        target.provider_name,
                        err
                    );
                    self.circuit_breaker.record_failure(cli_key.as_str(), &target.provider_id);
                    attempts.push(crate::gateway::types::GatewayAttemptTrace {
                        provider_id: target.provider_id.clone(),
                        provider_name: target.provider_name.clone(),
                        model: target.default_model.clone().unwrap_or_else(|| "default".into()),
                        status_code: None,
                        duration_ms: attempt_dur,
                        error: Some(err.clone()),
                    });
                    if !failover_enabled {
                        return Err(format!("主提供商 {} 请求失败且故障转移已停用: {err}", target.provider_name));
                    }
                    last_error = err;
                }
            }
        }

        Err(format!("All gateway upstream providers failed: {last_error}"))
    }

    /// Resolves the effective model name for a specific upstream target:
    /// 1. Prioritizes explicit `target.model_rewrites` rules (exact match, prefix wildcard `*`, or catch-all `*`).
    /// 2. For Claude Code: maps requested model family/tier (opus, sonnet, haiku, fable) to the target's configured tier model,
    ///    following cc-switch & ai-toolbox semantics.
    ///    If target has no matching tier but specifies a default_model, falls back to default_model.
    /// 3. If target protocol is OpenAiChat (protocol conversion), falls back to target's default_model.
    /// 4. Strips local `[1M]` context marker before sending to upstream.
    pub fn resolve_target_model(
        &self,
        cli_key: GatewayCliKey,
        original_model: Option<&str>,
        target: &GatewayRouteTarget,
    ) -> Option<String> {
        if let Some(req_model) = original_model {
            let req_clean = strip_one_m_marker(req_model);

            // 1. Check explicit model rewrites on target
            for (from, to) in &target.model_rewrites {
                let from_clean = strip_one_m_marker(from);
                if from == "*" || from == req_model || from_clean == req_clean {
                    return Some(strip_one_m_marker(to).to_string());
                }
                if let Some(prefix) = from.strip_suffix('*') {
                    if req_clean.starts_with(prefix) {
                        return Some(strip_one_m_marker(to).to_string());
                    }
                }
            }

            // 2. Claude Code model tier mapping (following cc-switch & ai-toolbox):
            if cli_key == GatewayCliKey::Claude {
                let req_lower = req_clean.to_ascii_lowercase();

                // Fable tier: fable -> fable_model -> opus_model -> default_model
                if req_lower.contains("fable") {
                    if let Some(ref m) = target.fable_model {
                        return Some(strip_one_m_marker(m).to_string());
                    }
                    if let Some(ref m) = target.opus_model {
                        return Some(strip_one_m_marker(m).to_string());
                    }
                    if let Some(ref m) = target.default_model {
                        return Some(strip_one_m_marker(m).to_string());
                    }
                }

                // Opus tier: opus -> opus_model -> default_model
                if req_lower.contains("opus") {
                    if let Some(ref m) = target.opus_model {
                        return Some(strip_one_m_marker(m).to_string());
                    }
                    if let Some(ref m) = target.default_model {
                        return Some(strip_one_m_marker(m).to_string());
                    }
                }

                // Sonnet tier: sonnet -> sonnet_model -> default_model
                if req_lower.contains("sonnet") {
                    if let Some(ref m) = target.sonnet_model {
                        return Some(strip_one_m_marker(m).to_string());
                    }
                    if let Some(ref m) = target.default_model {
                        return Some(strip_one_m_marker(m).to_string());
                    }
                }

                // Haiku tier: haiku -> haiku_model -> default_model
                if req_lower.contains("haiku") {
                    if let Some(ref m) = target.haiku_model {
                        return Some(strip_one_m_marker(m).to_string());
                    }
                    if let Some(ref m) = target.default_model {
                        return Some(strip_one_m_marker(m).to_string());
                    }
                }

                // If target does not specify that tier, fall back to target's default_model if present
                if let Some(ref def) = target.default_model {
                    if !def.is_empty() {
                        return Some(strip_one_m_marker(def).to_string());
                    }
                }

                return Some(req_clean.to_string());
            }

            // 3. Protocol conversion fallback:
            if target.target_protocol == AiProtocol::OpenAiChat {
                if let Some(ref def) = target.default_model {
                    if !def.is_empty() {
                        return Some(strip_one_m_marker(def).to_string());
                    }
                }
            }

            return Some(req_clean.to_string());
        }

        // 4. Fallback to default_model if original_model is None
        target.default_model.as_deref()
            .or(target.sonnet_model.as_deref())
            .or(target.opus_model.as_deref())
            .or(target.haiku_model.as_deref())
            .map(|s| strip_one_m_marker(s).to_string())
    }

    async fn dispatch_to_target(
        &self,
        cli_key: GatewayCliKey,
        path: &str,
        inbound_headers: &[(String, String)],
        body_bytes: &[u8],
        target: &GatewayRouteTarget,
    ) -> Result<ForwardResponse, String> {
        let start_time = Instant::now();
        let mut client_json: Value = serde_json::from_slice(body_bytes).unwrap_or(Value::Null);

        // Filter private parameters starting with `_` (matching cc-switch body_filter.rs)
        if client_json.is_object() {
            client_json = filter_private_params(client_json);
            if target.base_url.contains("deepseek") {
                normalize_deepseek_thinking_effort(&mut client_json);
            }
        }

        let original_model = client_json.get("model").and_then(Value::as_str).map(str::to_string);
        let effective_model = self.resolve_target_model(cli_key, original_model.as_deref(), target);

        if let Some(ref eff_model) = effective_model {
            if client_json.is_object() {
                client_json["model"] = Value::String(strip_one_m_marker(eff_model).to_string());
            }
        } else if let Some(ref orig) = original_model {
            if client_json.is_object() {
                client_json["model"] = Value::String(strip_one_m_marker(orig).to_string());
            }
        }

        // Protocol transformation:
        // Claude client -> OpenAI Chat upstream
        if cli_key == GatewayCliKey::Claude && target.target_protocol == AiProtocol::OpenAiChat {
            return self.dispatch_claude_to_openai(client_json, target, effective_model).await;
        }

        // Codex client -> OpenAI Chat upstream
        if cli_key == GatewayCliKey::Codex && target.target_protocol == AiProtocol::OpenAiChat {
            return self.dispatch_codex_to_openai_chat(client_json, target, effective_model).await;
        }

        // Native / pass-through forwarding:
        let is_rewritten = effective_model.is_some() && effective_model != original_model;
        let outbound_body = if is_rewritten || client_json.is_object() {
            serde_json::to_vec(&client_json).unwrap_or_else(|_| body_bytes.to_vec())
        } else {
            body_bytes.to_vec()
        };

        let url = build_target_url(&target.base_url, path);
        let mut req_builder = self.client.post(&url);

        // Copy inbound headers (updating content-length if model was rewritten)
        let new_content_len = outbound_body.len().to_string();
        for (k, v) in inbound_headers {
            let lower = k.to_ascii_lowercase();
            // Never forward host, connection hop-by-hop headers, accept-encoding, or client-side takeover auth tokens to upstream!
            if lower == "host"
                || lower == "authorization"
                || lower == "x-api-key"
                || lower == "x-goog-api-key"
                || lower == "connection"
                || lower == "keep-alive"
                || lower == "proxy-connection"
                || lower == "transfer-encoding"
                || lower == "upgrade"
                || lower == "accept-encoding"
            {
                continue;
            }
            if lower == "content-length" {
                req_builder = req_builder.header("content-length", &new_content_len);
                continue;
            }
            if let (Ok(h_name), Ok(h_val)) = (HeaderName::from_bytes(k.as_bytes()), HeaderValue::from_str(v)) {
                req_builder = req_builder.header(h_name, h_val);
            }
        }

        // Force Accept-Encoding: identity so upstream returns uncompressed plain text/SSE (matching ai-toolbox)
        req_builder = req_builder.header("accept-encoding", "identity");

        // Set Auth header for upstream target according to target.auth_type
        // Parity with cc-switch (claude.rs:814) and ai-toolbox (upstream.rs:10444):
        // NEVER send both x-api-key and Authorization simultaneously!
        if !target.api_key.is_empty() {
            if cli_key == GatewayCliKey::Claude {
                match target.auth_type {
                    UpstreamAuthType::AnthropicApiKey => {
                        req_builder = req_builder.header("x-api-key", &target.api_key);
                    }
                    UpstreamAuthType::Bearer => {
                        req_builder = req_builder.header("Authorization", format!("Bearer {}", target.api_key));
                    }
                }
            } else {
                req_builder = req_builder.header("Authorization", format!("Bearer {}", target.api_key));
            }
        }

        // Set custom headers
        for (k, v) in &target.custom_headers {
            if let (Ok(h_name), Ok(h_val)) = (HeaderName::from_bytes(k.as_bytes()), HeaderValue::from_str(v)) {
                req_builder = req_builder.header(h_name, h_val);
            }
        }

        let resp = req_builder
            .body(outbound_body)
            .send()
            .await
            .map_err(|e| format!("Request to upstream failed: {e}"))?;

        let status = resp.status().as_u16();
        let mut out_headers = Vec::new();
        for (k, v) in resp.headers() {
            if let Ok(val_str) = v.to_str() {
                out_headers.push((k.as_str().to_string(), val_str.to_string()));
            }
        }
        let is_streaming = out_headers
            .iter()
            .any(|(k, v)| k.eq_ignore_ascii_case("content-type") && v.contains("text/event-stream"));

        let bytes = resp.bytes().await.map_err(|e| format!("Failed to read upstream response body: {e}"))?;

        // Check if thinking signature rectifier should trigger for Anthropic requests (matching cc-switch)
        if status == 400 && cli_key == GatewayCliKey::Claude {
            let err_text = String::from_utf8_lossy(&bytes);
            if should_rectify_thinking_signature(Some(&err_text)) {
                let mut retry_json = client_json.clone();
                if rectify_anthropic_request(&mut retry_json) {
                    tracing::info!(
                        "Gateway thinking signature rectifier triggered for {}, retrying request...",
                        target.provider_name
                    );
                    if let Ok(retry_bytes) = serde_json::to_vec(&retry_json) {
                        let mut retry_builder = self.client.post(&url);
                        let retry_len = retry_bytes.len().to_string();
                        for (k, v) in inbound_headers {
                            let lower = k.to_ascii_lowercase();
                            if lower == "host"
                                || lower == "authorization"
                                || lower == "x-api-key"
                                || lower == "x-goog-api-key"
                                || lower == "connection"
                                || lower == "keep-alive"
                                || lower == "proxy-connection"
                                || lower == "transfer-encoding"
                                || lower == "upgrade"
                                || lower == "accept-encoding"
                            {
                                continue;
                            }
                            if lower == "content-length" {
                                retry_builder = retry_builder.header("content-length", &retry_len);
                                continue;
                            }
                            if let (Ok(h_name), Ok(h_val)) = (HeaderName::from_bytes(k.as_bytes()), HeaderValue::from_str(v)) {
                                retry_builder = retry_builder.header(h_name, h_val);
                            }
                        }
                        retry_builder = retry_builder.header("accept-encoding", "identity");
                        if !target.api_key.is_empty() {
                            retry_builder = retry_builder
                                .header("x-api-key", &target.api_key)
                                .header("Authorization", format!("Bearer {}", target.api_key));
                        }
                        for (k, v) in &target.custom_headers {
                            if let (Ok(h_name), Ok(h_val)) = (HeaderName::from_bytes(k.as_bytes()), HeaderValue::from_str(v)) {
                                retry_builder = retry_builder.header(h_name, h_val);
                            }
                        }
                        if let Ok(retry_resp) = retry_builder.body(retry_bytes).send().await {
                            let r_status = retry_resp.status().as_u16();
                            let mut r_headers = Vec::new();
                            for (k, v) in retry_resp.headers() {
                                if let Ok(val_str) = v.to_str() {
                                    r_headers.push((k.as_str().to_string(), val_str.to_string()));
                                }
                            }
                            let r_streaming = r_headers.iter().any(|(k, v)| k.eq_ignore_ascii_case("content-type") && v.contains("text/event-stream"));
                            if let Ok(r_bytes) = retry_resp.bytes().await {
                                if r_status < 400 {
                                    return Ok(ForwardResponse {
                                        status: r_status,
                                        headers: r_headers,
                                        body: r_bytes,
                                        is_streaming: r_streaming,
                                        provider_id: target.provider_id.clone(),
                                        provider_name: target.provider_name.clone(),
                                        model: effective_model.unwrap_or_else(|| target.default_model.clone().unwrap_or_else(|| "default".into())),
                                        failover_occurred: false,
                                        duration_ms: start_time.elapsed().as_millis() as u64,
                                        attempts: vec![],
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(ForwardResponse {
            status,
            headers: out_headers,
            body: bytes,
            is_streaming,
            provider_id: target.provider_id.clone(),
            provider_name: target.provider_name.clone(),
            model: effective_model.unwrap_or_else(|| target.default_model.clone().unwrap_or_else(|| "default".into())),
            failover_occurred: false,
            duration_ms: start_time.elapsed().as_millis() as u64,
            attempts: vec![],
        })
    }

    async fn dispatch_claude_to_openai(
        &self,
        claude_req: Value,
        target: &GatewayRouteTarget,
        effective_model: Option<String>,
    ) -> Result<ForwardResponse, String> {
        let start_time = Instant::now();
        let openai_req = anthropic_to_openai_request(&claude_req)?;
        let is_stream = openai_req.get("stream").and_then(Value::as_bool).unwrap_or(false);

        let base = target.base_url.trim_end_matches('/');
        let url = format!("{base}/chat/completions");

        let mut req_builder = self.client.post(&url)
            .header(CONTENT_TYPE, "application/json")
            .header("accept-encoding", "identity")
            .header(AUTHORIZATION, format!("Bearer {}", target.api_key));

        for (k, v) in &target.custom_headers {
            if let (Ok(hn), Ok(hv)) = (HeaderName::from_bytes(k.as_bytes()), HeaderValue::from_str(v)) {
                req_builder = req_builder.header(hn, hv);
            }
        }

        let resp = req_builder
            .json(&openai_req)
            .send()
            .await
            .map_err(|e| format!("Upstream OpenAI request error: {e}"))?;

        let status = resp.status().as_u16();
        let model_name = effective_model
            .or_else(|| openai_req.get("model").and_then(Value::as_str).map(str::to_string))
            .unwrap_or_else(|| target.default_model.clone().unwrap_or_default());

        if !resp.status().is_success() {
            let err_bytes = resp.bytes().await.unwrap_or_default();
            return Ok(ForwardResponse {
                status,
                headers: vec![("content-type".to_string(), "application/json".to_string())],
                body: err_bytes,
                is_streaming: false,
                provider_id: target.provider_id.clone(),
                provider_name: target.provider_name.clone(),
                model: model_name,
                failover_occurred: false,
                duration_ms: start_time.elapsed().as_millis() as u64,
                attempts: vec![],
            });
        }

        if is_stream {
            let text = resp.text().await.map_err(|e| format!("Failed to read stream text: {e}"))?;
            let mut state = AnthropicSseState::default();
            let mut out_sse = String::new();

            for line in text.lines() {
                let events = transform_openai_chunk_to_anthropic_sse(line, &mut state);
                for event in events {
                    out_sse.push_str(&event);
                }
            }

            Ok(ForwardResponse {
                status: 200,
                headers: vec![
                    ("content-type".to_string(), "text/event-stream; charset=utf-8".to_string()),
                    ("cache-control".to_string(), "no-cache".to_string()),
                ],
                body: Bytes::from(out_sse),
                is_streaming: true,
                provider_id: target.provider_id.clone(),
                provider_name: target.provider_name.clone(),
                model: model_name,
                failover_occurred: false,
                duration_ms: start_time.elapsed().as_millis() as u64,
                attempts: vec![],
            })
        } else {
            let resp_json: Value = resp.json().await.map_err(|e| format!("Failed to parse OpenAI json: {e}"))?;
            let anthropic_json = openai_response_to_anthropic(&resp_json);
            let body_bytes = serde_json::to_vec(&anthropic_json).unwrap_or_default();

            Ok(ForwardResponse {
                status: 200,
                headers: vec![("content-type".to_string(), "application/json".to_string())],
                body: Bytes::from(body_bytes),
                is_streaming: false,
                provider_id: target.provider_id.clone(),
                provider_name: target.provider_name.clone(),
                model: model_name,
                failover_occurred: false,
                duration_ms: start_time.elapsed().as_millis() as u64,
                attempts: vec![],
            })
        }
    }

    async fn dispatch_codex_to_openai_chat(
        &self,
        codex_req: Value,
        target: &GatewayRouteTarget,
        effective_model: Option<String>,
    ) -> Result<ForwardResponse, String> {
        let start_time = Instant::now();
        let chat_req = responses_to_chat_request(&codex_req)?;
        let base = target.base_url.trim_end_matches('/');
        let url = format!("{base}/chat/completions");

        let mut req_builder = self.client.post(&url)
            .header(CONTENT_TYPE, "application/json")
            .header("accept-encoding", "identity")
            .header(AUTHORIZATION, format!("Bearer {}", target.api_key));

        for (k, v) in &target.custom_headers {
            if let (Ok(hn), Ok(hv)) = (HeaderName::from_bytes(k.as_bytes()), HeaderValue::from_str(v)) {
                req_builder = req_builder.header(hn, hv);
            }
        }

        let resp = req_builder
            .json(&chat_req)
            .send()
            .await
            .map_err(|e| format!("Upstream OpenAI request error: {e}"))?;

        let status = resp.status().as_u16();
        let model_name = effective_model
            .or_else(|| chat_req.get("model").and_then(Value::as_str).map(str::to_string))
            .unwrap_or_else(|| target.default_model.clone().unwrap_or_default());

        if !resp.status().is_success() {
            let err_bytes = resp.bytes().await.unwrap_or_default();
            return Ok(ForwardResponse {
                status,
                headers: vec![("content-type".to_string(), "application/json".to_string())],
                body: err_bytes,
                is_streaming: false,
                provider_id: target.provider_id.clone(),
                provider_name: target.provider_name.clone(),
                model: model_name,
                failover_occurred: false,
                duration_ms: start_time.elapsed().as_millis() as u64,
                attempts: vec![],
            });
        }

        let chat_resp: Value = resp.json().await.map_err(|e| format!("Failed to parse OpenAI json: {e}"))?;
        let responses_json = chat_response_to_responses(&chat_resp);
        let body_bytes = serde_json::to_vec(&responses_json).unwrap_or_default();

        Ok(ForwardResponse {
            status: 200,
            headers: vec![("content-type".to_string(), "application/json".to_string())],
            body: Bytes::from(body_bytes),
            is_streaming: false,
            provider_id: target.provider_id.clone(),
            provider_name: target.provider_name.clone(),
            model: model_name,
            failover_occurred: false,
            duration_ms: start_time.elapsed().as_millis() as u64,
            attempts: vec![],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_target_model_exact_rewrite() {
        let forwarder = GatewayForwarder::new(Arc::new(CircuitBreakerRegistry::default()));
        let target = GatewayRouteTarget {
            provider_id: "prov-1".into(),
            provider_name: "Test Prov".into(),
            base_url: "https://api.example.com".into(),
            api_key: "sk-test".into(),
            auth_type: UpstreamAuthType::Bearer,
            default_model: None,
            opus_model: None,
            sonnet_model: None,
            haiku_model: None,
            fable_model: None,
            target_protocol: AiProtocol::AnthropicMessages,
            custom_headers: vec![],
            model_rewrites: vec![
                ("claude-3-7-sonnet-20250219".into(), "claude-3-7-sonnet".into()),
                ("claude-3-5-sonnet-20241022".into(), "anthropic/claude-3.5-sonnet".into()),
            ],
            timeout_seconds: 60,
        };

        // Exact match
        assert_eq!(
            forwarder.resolve_target_model(GatewayCliKey::Claude, Some("claude-3-7-sonnet-20250219"), &target),
            Some("claude-3-7-sonnet".into())
        );
        assert_eq!(
            forwarder.resolve_target_model(GatewayCliKey::Claude, Some("claude-3-5-sonnet-20241022"), &target),
            Some("anthropic/claude-3.5-sonnet".into())
        );
        // Non-matched keeps original
        assert_eq!(
            forwarder.resolve_target_model(GatewayCliKey::Claude, Some("claude-3-5-haiku-20241022"), &target),
            Some("claude-3-5-haiku-20241022".into())
        );
    }

    #[test]
    fn test_resolve_target_model_claude_tier_mapping() {
        let forwarder = GatewayForwarder::new(Arc::new(CircuitBreakerRegistry::default()));
        let target = GatewayRouteTarget {
            provider_id: "prov-ds".into(),
            provider_name: "DeepSeek".into(),
            base_url: "https://api.deepseek.com/anthropic".into(),
            api_key: "sk-ds".into(),
            auth_type: UpstreamAuthType::Bearer,
            default_model: Some("deepseek-v4-pro".into()),
            opus_model: Some("deepseek-flash".into()),
            sonnet_model: Some("deepseek-flash".into()),
            haiku_model: Some("deepseek-flash".into()),
            fable_model: None,
            target_protocol: AiProtocol::AnthropicMessages,
            custom_headers: vec![],
            model_rewrites: vec![],
            timeout_seconds: 60,
        };

        // Claude Code requests Opus tier model -> mapped to deepseek-flash
        assert_eq!(
            forwarder.resolve_target_model(GatewayCliKey::Claude, Some("claude-opus-5-5"), &target),
            Some("deepseek-flash".into())
        );
        assert_eq!(
            forwarder.resolve_target_model(GatewayCliKey::Claude, Some("claude-3-opus-20240229"), &target),
            Some("deepseek-flash".into())
        );
        // Claude Code requests Sonnet tier model -> mapped to deepseek-flash
        assert_eq!(
            forwarder.resolve_target_model(GatewayCliKey::Claude, Some("claude-3-5-sonnet-20241022"), &target),
            Some("deepseek-flash".into())
        );
        // Claude Code requests unknown custom model -> falls back to default_model (deepseek-v4-pro)
        assert_eq!(
            forwarder.resolve_target_model(GatewayCliKey::Claude, Some("custom-reasoning-model"), &target),
            Some("deepseek-v4-pro".into())
        );
    }

    #[test]
    fn test_resolve_target_model_wildcard_rewrite() {
        let forwarder = GatewayForwarder::new(Arc::new(CircuitBreakerRegistry::default()));
        let target = GatewayRouteTarget {
            provider_id: "prov-2".into(),
            provider_name: "DeepSeek Upstream".into(),
            base_url: "https://api.deepseek.com".into(),
            api_key: "sk-ds".into(),
            auth_type: UpstreamAuthType::Bearer,
            default_model: Some("deepseek-chat".into()),
            opus_model: None,
            sonnet_model: None,
            haiku_model: None,
            fable_model: None,
            target_protocol: AiProtocol::OpenAiChat,
            custom_headers: vec![],
            model_rewrites: vec![
                ("claude-*".into(), "deepseek-chat".into()),
            ],
            timeout_seconds: 60,
        };

        // Prefix match
        assert_eq!(
            forwarder.resolve_target_model(GatewayCliKey::Claude, Some("claude-3-7-sonnet-20250219"), &target),
            Some("deepseek-chat".into())
        );
        assert_eq!(
            forwarder.resolve_target_model(GatewayCliKey::Claude, Some("claude-opus-4"), &target),
            Some("deepseek-chat".into())
        );
    }

    #[test]
    fn test_resolve_target_model_protocol_conversion_default_fallback() {
        let forwarder = GatewayForwarder::new(Arc::new(CircuitBreakerRegistry::default()));
        let target = GatewayRouteTarget {
            provider_id: "prov-3".into(),
            provider_name: "OpenAI Compatible".into(),
            base_url: "https://api.openai.com/v1".into(),
            api_key: "sk-oa".into(),
            auth_type: UpstreamAuthType::Bearer,
            default_model: Some("gpt-4o".into()),
            opus_model: None,
            sonnet_model: None,
            haiku_model: None,
            fable_model: None,
            target_protocol: AiProtocol::OpenAiChat,
            custom_headers: vec![],
            model_rewrites: vec![], // No explicit rewrites
            timeout_seconds: 60,
        };

        // When Claude calls an OpenAiChat target without explicit rewrites, falls back to default_model!
        assert_eq!(
            forwarder.resolve_target_model(GatewayCliKey::Claude, Some("claude-3-7-sonnet-20250219"), &target),
            Some("gpt-4o".into())
        );
    }

    #[test]
    fn test_is_retryable_status_covers_all_upstream_errors() {
        // Success codes must NOT retry
        assert!(!GatewayForwarder::is_retryable_status(200));
        assert!(!GatewayForwarder::is_retryable_status(201));
        assert!(!GatewayForwarder::is_retryable_status(204));
        assert!(!GatewayForwarder::is_retryable_status(302));

        // Upstream auth, quota, model not found, rate limit, and server errors MUST failover
        assert!(GatewayForwarder::is_retryable_status(400)); // bad request / invalid model
        assert!(GatewayForwarder::is_retryable_status(401)); // unauthorized / invalid key / expired
        assert!(GatewayForwarder::is_retryable_status(402)); // payment required / out of quota
        assert!(GatewayForwarder::is_retryable_status(403)); // forbidden / IP block / banned
        assert!(GatewayForwarder::is_retryable_status(404)); // not found / route or model missing
        assert!(GatewayForwarder::is_retryable_status(408)); // timeout
        assert!(GatewayForwarder::is_retryable_status(422)); // unprocessable entity
        assert!(GatewayForwarder::is_retryable_status(429)); // rate limit
        assert!(GatewayForwarder::is_retryable_status(500)); // internal server error
        assert!(GatewayForwarder::is_retryable_status(502)); // bad gateway
        assert!(GatewayForwarder::is_retryable_status(503)); // service unavailable
        assert!(GatewayForwarder::is_retryable_status(504)); // gateway timeout
    }

    #[tokio::test]
    async fn test_failover_disabled_stops_at_primary() {
        let forwarder = GatewayForwarder::new(Arc::new(CircuitBreakerRegistry::default()));
        // Two candidates pointing to unroutable ports
        let candidates = vec![
            GatewayRouteTarget {
                provider_id: "prov-p0".into(),
                provider_name: "Primary P0".into(),
                base_url: "http://127.0.0.1:59998".into(), // Will fail to connect
                api_key: "sk-p0".into(),
                auth_type: UpstreamAuthType::Bearer,
                default_model: Some("model-p0".into()),
                opus_model: None,
                sonnet_model: None,
                haiku_model: None,
                fable_model: None,
                target_protocol: AiProtocol::AnthropicMessages,
                custom_headers: vec![],
                model_rewrites: vec![],
                timeout_seconds: 1,
            },
            GatewayRouteTarget {
                provider_id: "prov-p1".into(),
                provider_name: "Backup P1".into(),
                base_url: "http://127.0.0.1:59999".into(),
                api_key: "sk-p1".into(),
                auth_type: UpstreamAuthType::Bearer,
                default_model: Some("model-p1".into()),
                opus_model: None,
                sonnet_model: None,
                haiku_model: None,
                fable_model: None,
                target_protocol: AiProtocol::AnthropicMessages,
                custom_headers: vec![],
                model_rewrites: vec![],
                timeout_seconds: 1,
            },
        ];

        // When failover_enabled is FALSE, dispatch should fail immediately on P0 without trying P1
        let res = forwarder.forward_with_failover(
            GatewayCliKey::Claude,
            "/v1/messages",
            &[],
            b"{}",
            &candidates,
            false, // failover_enabled = false
        ).await;

        assert!(res.is_err());
        let err_msg = res.unwrap_err();
        assert!(err_msg.contains("主提供商 Primary P0 请求失败且故障转移已停用"), "Err was: {err_msg}");
    }

    #[test]
    fn test_strip_one_m_marker_edge_cases() {
        assert_eq!(strip_one_m_marker("claude-opus-5-5[1M]"), "claude-opus-5-5");
        assert_eq!(strip_one_m_marker("claude-opus-5-5 [1M]"), "claude-opus-5-5");
        assert_eq!(strip_one_m_marker("claude-opus-5-5[1m]"), "claude-opus-5-5");
        assert_eq!(strip_one_m_marker("claude-opus-5-5%5b1m%5d"), "claude-opus-5-5");
        assert_eq!(strip_one_m_marker("claude-opus-5-5%5B1M%5D"), "claude-opus-5-5");

        // Models ending in 1, M, or other characters must NOT be truncated
        assert_eq!(strip_one_m_marker("deepseek-v1"), "deepseek-v1");
        assert_eq!(strip_one_m_marker("deepseek-chat-1"), "deepseek-chat-1");
        assert_eq!(strip_one_m_marker("gemini-2.5-flash"), "gemini-2.5-flash");
    }

    #[test]
    fn test_thinking_rectifier() {
        assert!(should_rectify_thinking_signature(Some("Invalid 'signature' in 'thinking' block")));
        assert!(should_rectify_thinking_signature(Some("must start with a thinking block")));
        assert!(should_rectify_thinking_signature(Some("Thought signature is not valid")));
        assert!(should_rectify_thinking_signature(Some("Expected `thinking` or `redacted_thinking`, but found `tool_use`")));
        assert!(!should_rectify_thinking_signature(Some("Rate limit exceeded")));

        let mut body = serde_json::json!({
            "model": "claude-sonnet-5",
            "thinking": { "type": "enabled", "budget_tokens": 10000 },
            "messages": [
                {
                    "role": "user",
                    "content": "Hello"
                },
                {
                    "role": "assistant",
                    "content": [
                        { "type": "thinking", "thinking": "Internal thought", "signature": "sig123" },
                        { "type": "text", "text": "Hi there" }
                    ]
                }
            ]
        });

        let modified = rectify_anthropic_request(&mut body);
        assert!(modified);

        let assistant_blocks = body["messages"][1]["content"].as_array().unwrap();
        assert_eq!(assistant_blocks.len(), 1);
        assert_eq!(assistant_blocks[0]["type"], "text");
        assert_eq!(assistant_blocks[0]["text"], "Hi there");
    }

    #[test]
    fn test_deepseek_candidate_model_resolution() {
        let cb_registry = std::sync::Arc::new(crate::gateway::circuit_breaker::CircuitBreakerRegistry::default());
        let forwarder = GatewayForwarder::new(cb_registry);

        let target = GatewayRouteTarget {
            provider_id: "68521e3d-8bea-4a91-9b1b-3b42fc8b076d".to_string(),
            provider_name: "DeepSeek".to_string(),
            base_url: "https://api.deepseek.com/anthropic".to_string(),
            api_key: "sk-mock-deepseek-key-for-unit-testing".to_string(),
            auth_type: UpstreamAuthType::Bearer,
            default_model: Some("deepseek-v4-pro".to_string()),
            opus_model: Some("deepseek-flash".to_string()),
            sonnet_model: Some("deepseek-flash".to_string()),
            haiku_model: Some("deepseek-flash".to_string()),
            fable_model: None,
            target_protocol: AiProtocol::AnthropicMessages,
            custom_headers: vec![],
            model_rewrites: vec![],
            timeout_seconds: 60,
        };

        // Opus tier request -> maps to deepseek-flash
        let resolved = forwarder.resolve_target_model(GatewayCliKey::Claude, Some("claude-opus-5-5"), &target);
        assert_eq!(resolved, Some("deepseek-flash".to_string()));

        // Opus tier with [1M] -> maps to deepseek-flash
        let resolved_1m = forwarder.resolve_target_model(GatewayCliKey::Claude, Some("claude-opus-5-5[1M]"), &target);
        assert_eq!(resolved_1m, Some("deepseek-flash".to_string()));
    }
}
