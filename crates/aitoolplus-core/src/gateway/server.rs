use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use serde_json::json;

use crate::gateway::circuit_breaker::CircuitBreakerRegistry;
use crate::gateway::forwarder::GatewayForwarder;
use crate::gateway::request_log::RequestLogStore;
use crate::gateway::router::GatewayRouter;
use crate::gateway::settings::GatewaySettings;
use crate::gateway::types::{
    GatewayCliKey, GatewayProxyMode, GatewayRequestLogDetail, GatewayRequestLogSummary,
    GatewayStatus,
};
use crate::paths::Paths;

pub struct GatewayServerState {
    pub running: AtomicBool,
    pub port: u16,
    pub host: String,
    pub started_at: Instant,
    pub total_requests: AtomicU64,
    pub success_requests: AtomicU64,
    pub failed_requests: AtomicU64,
}

pub struct GatewayServerHandle {
    pub state: Arc<GatewayServerState>,
    pub shutdown_tx: broadcast::Sender<()>,
    pub _runtime_thread: Option<std::thread::JoinHandle<()>>,
}

impl GatewayServerHandle {
    pub fn stop(&self) {
        let _ = self.shutdown_tx.send(());
        self.state.running.store(false, Ordering::SeqCst);
    }

    pub fn status(&self) -> GatewayStatus {
        let uptime = if self.state.running.load(Ordering::SeqCst) {
            self.state.started_at.elapsed().as_secs()
        } else {
            0
        };
        GatewayStatus {
            running: self.state.running.load(Ordering::SeqCst),
            host: self.state.host.clone(),
            port: self.state.port,
            uptime_seconds: uptime,
            active_connections: 0,
            total_requests: self.state.total_requests.load(Ordering::SeqCst),
            success_requests: self.state.success_requests.load(Ordering::SeqCst),
            failed_requests: self.state.failed_requests.load(Ordering::SeqCst),
            active_targets: vec![],
        }
    }
}

impl Drop for GatewayServerHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

pub struct GatewayServer;

impl GatewayServer {
    /// Start the Gateway server synchronously by spawning a dedicated Tokio runtime thread.
    /// This works reliably anywhere, including non-Tokio environments like GPUI threads.
    pub fn start_sync(
        paths: Paths,
        settings: GatewaySettings,
    ) -> Result<GatewayServerHandle, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        let thread_handle = std::thread::Builder::new()
            .name("aitoolplus-gateway".into())
            .spawn(move || {
                let rt = match tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt,
                    Err(e) => {
                        let _ = tx.send(Err(format!("Failed to build Tokio runtime: {e}")));
                        return;
                    }
                };

                let handle_res = rt.block_on(async {
                    Self::start(paths, settings).await
                });

                match handle_res {
                    Ok(handle) => {
                        let shutdown_tx = handle.shutdown_tx.clone();
                        let state = handle.state.clone();
                        let mut rx_shutdown = shutdown_tx.subscribe();
                        let _ = tx.send(Ok((state, shutdown_tx)));

                        // Keep runtime alive until shutdown
                        rt.block_on(async move {
                            let _ = rx_shutdown.recv().await;
                        });
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e));
                    }
                }
            })
            .map_err(|e| format!("Failed to spawn gateway thread: {e}"))?;

        let (state, shutdown_tx) = rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .map_err(|e| format!("Gateway startup timed out: {e}"))??;

        Ok(GatewayServerHandle {
            state,
            shutdown_tx,
            _runtime_thread: Some(thread_handle),
        })
    }

    /// Start the Gateway server in an existing Tokio context.
    pub async fn start(
        paths: Paths,
        settings: GatewaySettings,
    ) -> Result<GatewayServerHandle, String> {
        let start_port = settings.port;
        let host = settings.host.clone();
        let max_port = if settings.port_auto_select {
            start_port + 50
        } else {
            start_port
        };

        let mut bound_listener = None;
        let mut actual_port = start_port;

        for p in start_port..=max_port {
            let addr = format!("{}:{}", host, p);
            match TcpListener::bind(&addr).await {
                Ok(l) => {
                    bound_listener = Some(l);
                    actual_port = p;
                    break;
                }
                Err(_) => {
                    if !settings.port_auto_select {
                        break;
                    }
                }
            }
        }

        let listener = bound_listener.ok_or_else(|| {
            format!("Failed to bind Gateway HTTP server on {}:{}", host, start_port)
        })?;

        tracing::info!("Gateway HTTP server bound on {}:{}", host, actual_port);

        let (shutdown_tx, mut shutdown_rx) = broadcast::channel::<()>(1);
        let cb_registry = Arc::new(CircuitBreakerRegistry::default());
        let forwarder = Arc::new(GatewayForwarder::new(cb_registry));

        let state = Arc::new(GatewayServerState {
            running: AtomicBool::new(true),
            port: actual_port,
            host: host.clone(),
            started_at: Instant::now(),
            total_requests: AtomicU64::new(0),
            success_requests: AtomicU64::new(0),
            failed_requests: AtomicU64::new(0),
        });

        let server_state = state.clone();
        let paths_clone = paths.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        tracing::info!("Gateway server received shutdown signal");
                        break;
                    }
                    accept_res = listener.accept() => {
                        match accept_res {
                            Ok((stream, peer_addr)) => {
                                let fwd = forwarder.clone();
                                let p = paths_clone.clone();
                                let s = server_state.clone();
                                tokio::spawn(async move {
                                    if let Err(e) = handle_http_stream(stream, peer_addr, fwd, p, s).await {
                                        tracing::warn!("Gateway handle_http_stream error: {e}");
                                    }
                                });
                            }
                            Err(e) => {
                                tracing::warn!("Gateway accept error: {e}");
                                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                            }
                        }
                    }
                }
            }
            server_state.running.store(false, Ordering::SeqCst);
        });

        Ok(GatewayServerHandle {
            state,
            shutdown_tx,
            _runtime_thread: None,
        })
    }
}

async fn handle_http_stream(
    mut stream: TcpStream,
    _peer: SocketAddr,
    forwarder: Arc<GatewayForwarder>,
    paths: Paths,
    server_state: Arc<GatewayServerState>,
) -> Result<(), String> {
    let mut buf = vec![0u8; 8192];
    let mut header_bytes = Vec::new();

    // 1. Read HTTP headers
    loop {
        let n = stream.read(&mut buf).await.map_err(|e| e.to_string())?;
        if n == 0 {
            return Ok(());
        }
        header_bytes.extend_from_slice(&buf[..n]);

        if let Some(pos) = find_header_end(&header_bytes) {
            let header_slice = &header_bytes[..pos];
            let remaining_body = &header_bytes[pos + 4..];

            let header_str = String::from_utf8_lossy(header_slice);
            let mut lines = header_str.lines();
            let req_line = lines.next().unwrap_or("");
            let mut parts = req_line.split_whitespace();
            let _method = parts.next().unwrap_or("GET");
            let path = parts.next().unwrap_or("/");

            let mut headers = Vec::new();
            let mut content_length: usize = 0;
            for line in lines {
                if let Some((k, v)) = line.split_once(':') {
                    let k_trim = k.trim().to_string();
                    let v_trim = v.trim().to_string();
                    if k_trim.eq_ignore_ascii_case("content-length") {
                        content_length = v_trim.parse().unwrap_or(0);
                    }
                    headers.push((k_trim, v_trim));
                }
            }

            // 2. Read full body if Content-Length > remaining_body.len()
            let mut body = remaining_body.to_vec();
            while body.len() < content_length {
                let n = stream.read(&mut buf).await.map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                body.extend_from_slice(&buf[..n]);
            }

            // 3. Handle Route
            server_state.total_requests.fetch_add(1, Ordering::SeqCst);

            if path == "/health" || path == "/status" {
                let resp = json!({
                    "status": "ok",
                    "running": true,
                    "port": server_state.port,
                    "uptime_seconds": server_state.started_at.elapsed().as_secs(),
                });
                write_json_response(&mut stream, 200, &resp).await?;
                return Ok(());
            }

            let clean_path = path.split('?').next().unwrap_or(path);

            // Answer Claude Code connection-warming probes locally (GET/HEAD /api/hello, /hello)
            // Parity with ai-toolbox (upstream.rs:858) and cc-switch
            if clean_path == "/api/hello"
                || clean_path == "/hello"
                || clean_path.ends_with("/api/hello")
                || clean_path.ends_with("/hello")
            {
                let resp = json!({"ok": true});
                write_json_response(&mut stream, 200, &resp).await?;
                return Ok(());
            }

            // Answer Codex / OpenAI models reachability probes locally
            // Parity with cc-switch (handlers.rs:78) and ai-toolbox
            if clean_path == "/models"
                || clean_path == "/v1/models"
                || clean_path.ends_with("/models")
                || clean_path.ends_with("/v1/models")
            {
                let resp = json!({
                    "object": "list",
                    "data": []
                });
                write_json_response(&mut stream, 200, &resp).await?;
                return Ok(());
            }

            // Match CLI Key and target route
            let (cli_key, target_path) = match_cli_from_path(path);
            let manifest = crate::gateway::cli_proxy::manifest::CliProxyManifest::read(&paths, cli_key);
            let (mode, primary_id) = manifest.as_ref().map(|m| (m.mode, m.primary_provider_id.as_str())).unwrap_or((GatewayProxyMode::Failover, ""));

            let candidates = GatewayRouter::resolve_candidates(
                &paths,
                cli_key,
                mode,
                if primary_id.is_empty() { None } else { Some(primary_id) },
            );

            if candidates.is_empty() {
                server_state.failed_requests.fetch_add(1, Ordering::SeqCst);
                let err_resp = json!({
                    "error": {
                        "type": "gateway_no_providers",
                        "message": format!("No active providers configured for {:?}", cli_key)
                    }
                });
                write_json_response(&mut stream, 502, &err_resp).await?;
                return Ok(());
            }

            let mut final_body = body.clone();
            let mut final_candidates = candidates;

            if mode == GatewayProxyMode::Aggregate {
                if let Ok(mut json_val) = serde_json::from_slice::<serde_json::Value>(&final_body) {
                    if let Some(model_str) = json_val.get("model").and_then(|v| v.as_str()) {
                        if let Some((prefix, actual_model)) = model_str.split_once('.') {
                            let mut matched_id = None;
                            for c in &final_candidates {
                                let c_prefix = crate::gateway::cli_proxy::codex::sanitize_prefix(&c.provider_name);
                                let c_id_prefix = crate::gateway::cli_proxy::codex::sanitize_prefix(&c.provider_id);
                                if prefix.eq_ignore_ascii_case(&c_prefix)
                                    || prefix.eq_ignore_ascii_case(&c_id_prefix)
                                    || prefix.eq_ignore_ascii_case(&c.provider_id)
                                {
                                    matched_id = Some(c.provider_id.clone());
                                    break;
                                }
                            }
                            if let Some(target_id) = matched_id {
                                json_val["model"] = serde_json::Value::String(actual_model.to_string());
                                if let Ok(new_bytes) = serde_json::to_vec(&json_val) {
                                    final_body = new_bytes;
                                    let new_len_str = final_body.len().to_string();
                                    for (k, v) in headers.iter_mut() {
                                        if k.eq_ignore_ascii_case("content-length") {
                                            *v = new_len_str.clone();
                                        }
                                    }
                                }
                                let mut reordered = Vec::new();
                                if let Some(target) = final_candidates.iter().find(|c| c.provider_id == target_id) {
                                    reordered.push(target.clone());
                                }
                                for c in &final_candidates {
                                    if c.provider_id != target_id {
                                        reordered.push(c.clone());
                                    }
                                }
                                final_candidates = reordered;
                            }
                        }
                    }
                }
            }

            let req_id = uuid::Uuid::new_v4().to_string();
            let gw_settings = crate::gateway::GatewaySettings::load(&paths);
            let failover_enabled = gw_settings.failover_enabled;

            let forward_result = forwarder
                .forward_with_failover(cli_key, &target_path, &headers, &final_body, &final_candidates, failover_enabled)
                .await;

            match forward_result {
                Ok(resp) => {
                    if resp.status < 400 {
                        server_state.success_requests.fetch_add(1, Ordering::SeqCst);
                    } else {
                        server_state.failed_requests.fetch_add(1, Ordering::SeqCst);
                    }

                    // Extract token counts if available
                    let (in_tok, out_tok) = extract_tokens(&resp.body);

                    // Write HTTP response to client stream
                    write_raw_response(&mut stream, resp.status, &resp.headers, &resp.body).await?;

                    // Record observability log
                    let detail = GatewayRequestLogDetail {
                        id: req_id.clone(),
                        summary: GatewayRequestLogSummary {
                            id: req_id,
                            timestamp: chrono::Utc::now().timestamp(),
                            cli_key: cli_key.as_str().to_string(),
                            route_name: path.to_string(),
                            provider_id: resp.provider_id,
                            provider_name: resp.provider_name,
                            model: resp.model,
                            status_code: resp.status,
                            duration_ms: resp.duration_ms,
                            first_token_ms: None,
                            input_tokens: in_tok,
                            output_tokens: out_tok,
                            total_tokens: in_tok + out_tok,
                            cost: 0.0,
                            is_streaming: resp.is_streaming,
                            failover: resp.failover_occurred,
                            error_category: if resp.status >= 400 { Some("upstream_error".into()) } else { None },
                        },
                        inbound_headers: headers,
                        inbound_body: Some(String::from_utf8_lossy(&body).to_string()),
                        upstream_url: target_path.clone(),
                        upstream_headers: resp.headers.clone(),
                        upstream_body: Some(String::from_utf8_lossy(&body).to_string()),
                        upstream_status_code: Some(resp.status),
                        upstream_response_body: Some(String::from_utf8_lossy(&resp.body).to_string()),
                        client_response_body: Some(String::from_utf8_lossy(&resp.body).to_string()),
                        attempts: resp.attempts,
                    };
                    let _ = RequestLogStore::record(&paths, &detail);
                }
                Err(err) => {
                    server_state.failed_requests.fetch_add(1, Ordering::SeqCst);
                    let err_resp = json!({
                        "error": {
                            "type": "gateway_forward_error",
                            "message": err
                        }
                    });
                    let failed_detail = GatewayRequestLogDetail {
                        id: req_id.clone(),
                        summary: GatewayRequestLogSummary {
                            id: req_id,
                            timestamp: chrono::Utc::now().timestamp(),
                            cli_key: cli_key.as_str().to_string(),
                            route_name: path.to_string(),
                            provider_id: "all_failed".to_string(),
                            provider_name: "候选提供商均失败".to_string(),
                            model: "-".to_string(),
                            status_code: 502,
                            duration_ms: 0,
                            first_token_ms: None,
                            input_tokens: 0,
                            output_tokens: 0,
                            total_tokens: 0,
                            cost: 0.0,
                            is_streaming: false,
                            failover: true,
                            error_category: Some("gateway_forward_error".into()),
                        },
                        inbound_headers: headers,
                        inbound_body: Some(String::from_utf8_lossy(&body).to_string()),
                        upstream_url: target_path.clone(),
                        upstream_headers: vec![],
                        upstream_body: Some(String::from_utf8_lossy(&body).to_string()),
                        upstream_status_code: Some(502),
                        upstream_response_body: Some(err.clone()),
                        client_response_body: Some(err_resp.to_string()),
                        attempts: vec![],
                    };
                    let _ = RequestLogStore::record(&paths, &failed_detail);
                    write_json_response(&mut stream, 502, &err_resp).await?;
                }
            }

            return Ok(());
        }
    }
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    for i in 0..bytes.len().saturating_sub(3) {
        if &bytes[i..i + 4] == b"\r\n\r\n" {
            return Some(i);
        }
    }
    None
}

fn match_cli_from_path(path: &str) -> (GatewayCliKey, String) {
    if let Some(rest) = path.strip_prefix("/anthropic") {
        (GatewayCliKey::Claude, if rest.is_empty() { "/v1/messages".to_string() } else { rest.to_string() })
    } else if let Some(rest) = path.strip_prefix("/openai") {
        (GatewayCliKey::Codex, if rest.is_empty() { "/v1/chat/completions".to_string() } else { rest.to_string() })
    } else if path.contains("messages") {
        (GatewayCliKey::Claude, path.to_string())
    } else {
        (GatewayCliKey::Codex, path.to_string())
    }
}

fn extract_tokens(body: &[u8]) -> (u64, u64) {
    if let Ok(val) = serde_json::from_slice::<serde_json::Value>(body) {
        if let Some(usage) = val.get("usage") {
            let prompt = usage.get("prompt_tokens")
                .or_else(|| usage.get("input_tokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let completion = usage.get("completion_tokens")
                .or_else(|| usage.get("output_tokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            return (prompt, completion);
        }
    }
    (0, 0)
}

async fn write_json_response(
    stream: &mut TcpStream,
    status: u16,
    value: &serde_json::Value,
) -> Result<(), String> {
    let body_str = serde_json::to_string(value).unwrap_or_default();
    let resp = format!(
        "HTTP/1.1 {} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status,
        body_str.len(),
        body_str
    );
    stream.write_all(resp.as_bytes()).await.map_err(|e| e.to_string())
}

async fn write_raw_response(
    stream: &mut TcpStream,
    status: u16,
    headers: &[(String, String)],
    body: &[u8],
) -> Result<(), String> {
    let mut header_lines = format!("HTTP/1.1 {} OK\r\n", status);

    for (k, v) in headers {
        let lower = k.to_ascii_lowercase();
        if lower == "content-length"
            || lower == "content-encoding"
            || lower == "transfer-encoding"
            || lower == "connection"
            || lower == "keep-alive"
            || lower == "proxy-connection"
            || lower == "upgrade"
        {
            continue;
        }
        header_lines.push_str(&format!("{}: {}\r\n", k, v));
    }

    header_lines.push_str(&format!("Content-Length: {}\r\n", body.len()));
    header_lines.push_str("Connection: close\r\n\r\n");

    stream.write_all(header_lines.as_bytes()).await.map_err(|e| e.to_string())?;
    stream.write_all(body).await.map_err(|e| e.to_string())?;
    stream.flush().await.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_live_gateway_server() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("home");
        let app_data = temp.path().join("app_data");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&app_data).unwrap();

        let paths = Paths::new(home, app_data);

        let mut settings = GatewaySettings::default();
        settings.port = 15729;
        settings.port_auto_select = true;

        let handle = GatewayServer::start(paths.clone(), settings).await.expect("start server");
        let status = handle.status();
        assert!(status.running);
        assert_eq!(status.port, 15729);

        // Test /health endpoint
        let client = reqwest::Client::new();
        let health_url = format!("http://127.0.0.1:{}/health", status.port);
        let resp = client.get(&health_url).send().await.expect("send /health");
        assert_eq!(resp.status(), 200);
        let health_json: serde_json::Value = resp.json().await.expect("parse /health json");
        assert_eq!(health_json["status"], "ok");

        // Test /status endpoint
        let status_url = format!("http://127.0.0.1:{}/status", status.port);
        let resp = client.get(&status_url).send().await.expect("send /status");
        assert_eq!(resp.status(), 200);
        let status_json: serde_json::Value = resp.json().await.expect("parse /status json");
        assert_eq!(status_json["running"], true);
        assert_eq!(status_json["port"], 15729);

        // Test stop
        handle.stop();
        let status_after = handle.status();
        assert!(!status_after.running);
    }

    #[tokio::test]
    async fn test_live_gateway_failover_and_request_logging() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("home");
        let app_data = temp.path().join("app_data");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&app_data).unwrap();

        let paths = Paths::new(home, app_data);

        // 1. Start mock server 1 (P0: returns 500)
        let listener_p0 = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr_p0 = listener_p0.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener_p0.accept().await {
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf).await;
                let resp = "HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\nContent-Length: 26\r\nConnection: close\r\n\r\n{\"error\":\"mock 500 error\"}";
                let _ = stream.write_all(resp.as_bytes()).await;
                let _ = stream.flush().await;
            }
        });

        // 2. Start mock server 2 (P1: returns 200 with OpenAI format chat completion)
        let listener_p1 = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr_p1 = listener_p1.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener_p1.accept().await {
                let mut buf = [0u8; 2048];
                let _ = stream.read(&mut buf).await;
                let body = json!({
                    "id": "chatcmpl-mock",
                    "choices": [{
                        "message": {
                            "role": "assistant",
                            "content": "hello from backup provider!"
                        }
                    }],
                    "usage": {
                        "prompt_tokens": 12,
                        "completion_tokens": 8
                    }
                }).to_string();
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(resp.as_bytes()).await;
                let _ = stream.flush().await;
            }
        });

        // 3. Populate store with P0 and P1 under ClaudeCode tool
        let mut store = crate::store::StoreHandle::open(&paths).unwrap();
        let _ = store.update(|s| {
            let section = s.tool_mut(crate::tools::ToolId::ClaudeCode);
            section.providers = vec![
                crate::providers::ProviderRecord {
                    id: "p0".into(),
                    name: "Primary P0".into(),
                    category: "custom".into(),
                    settings_config: format!("{{\"base_url\":\"http://{}\",\"api_key\":\"sk-test\"}}", addr_p0),
                    is_applied: true,
                    is_disabled: false,
                    sort_index: 0,
                    notes: None,
                    website_url: None,
                    meta: None,
                    created_at: "2026-01-01T00:00:00Z".into(),
                    updated_at: "2026-01-01T00:00:00Z".into(),
                },
                crate::providers::ProviderRecord {
                    id: "p1".into(),
                    name: "Backup P1".into(),
                    category: "custom".into(),
                    settings_config: format!("{{\"base_url\":\"http://{}\",\"api_key\":\"sk-test\"}}", addr_p1),
                    is_applied: false,
                    is_disabled: false,
                    sort_index: 1,
                    notes: None,
                    website_url: None,
                    meta: Some(json!({
                        "apiFormat": "openai"
                    })),
                    created_at: "2026-01-01T00:00:00Z".into(),
                    updated_at: "2026-01-01T00:00:00Z".into(),
                },
            ];
        });

        // 4. Start Gateway Server
        let mut settings = GatewaySettings::default();
        settings.port = 15730;
        settings.port_auto_select = true;
        settings.failover_enabled = true;
        settings.request_log_enabled = true;
        settings.request_log_body_enabled = true;
        settings.save(&paths).unwrap();

        let handle = GatewayServer::start(paths.clone(), settings).await.unwrap();

        // 5. Send an Anthropic Messages request to the gateway
        let client = reqwest::Client::new();
        let req_body = json!({
            "model": "claude-3-7-sonnet",
            "messages": [
                {"role": "user", "content": "hi"}
            ]
        });

        let resp = client
            .post(format!("http://127.0.0.1:{}/anthropic/v1/messages", handle.status().port))
            .header("x-api-key", "test-key")
            .json(&req_body)
            .send()
            .await
            .expect("send request to gateway");

        let status = resp.status();
        assert_eq!(status, 200);
        let resp_json: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(resp_json["content"][0]["text"], "hello from backup provider!");

        // 6. Verify Request Log Store
        let logs = RequestLogStore::list(&paths, 10, 0).unwrap();
        assert_eq!(logs.len(), 1);
        let summary = &logs[0];
        assert_eq!(summary.status_code, 200);
        assert_eq!(summary.input_tokens, 12);
        assert_eq!(summary.output_tokens, 8);

        // Verify Detail
        let detail = RequestLogStore::get_detail(&paths, &summary.id).unwrap().unwrap();
        assert!(detail.inbound_body.unwrap().contains("claude-3-7-sonnet"));
        assert!(detail.client_response_body.unwrap().contains("hello from backup provider!"));

        handle.stop();
    }

    #[tokio::test]
    async fn test_live_gateway_failover_on_401_unauthorized() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("home");
        let app_data = temp.path().join("app_data");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&app_data).unwrap();

        let paths = Paths::new(home, app_data);

        // 1. Mock P0: returns 401 Unauthorized (invalid key or expired anyrouter account)
        let listener_p0 = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr_p0 = listener_p0.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener_p0.accept().await {
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf).await;
                let err_body = "{\"error\":{\"type\":\"authentication_error\",\"message\":\"invalid api key\"}}";
                let resp = format!(
                    "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    err_body.len(),
                    err_body
                );
                let _ = stream.write_all(resp.as_bytes()).await;
                let _ = stream.flush().await;
            }
        });

        // 2. Mock P1: returns 200 OK (DeepSeek)
        let listener_p1 = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr_p1 = listener_p1.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener_p1.accept().await {
                let mut buf = [0u8; 2048];
                let _ = stream.read(&mut buf).await;
                let body = json!({
                    "id": "chatcmpl-ds",
                    "object": "chat.completion",
                    "created": 1234567,
                    "model": "deepseek-chat",
                    "choices": [{
                        "index": 0,
                        "finish_reason": "stop",
                        "message": {
                            "role": "assistant",
                            "content": "hello from deepseek failover!"
                        }
                    }],
                    "usage": {
                        "prompt_tokens": 10,
                        "completion_tokens": 5
                    }
                }).to_string();
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(resp.as_bytes()).await;
                let _ = stream.flush().await;
            }
        });

        // 3. Populate store with P0 (anyrouter) and P1 (deepseek) under ClaudeCode tool
        let mut store = crate::store::StoreHandle::open(&paths).unwrap();
        let _ = store.update(|s| {
            let section = s.tool_mut(crate::tools::ToolId::ClaudeCode);
            section.providers = vec![
                crate::providers::ProviderRecord {
                    id: "anyrouter".into(),
                    name: "AnyRouter P0".into(),
                    category: "custom".into(),
                    settings_config: format!("{{\"base_url\":\"http://{}\",\"api_key\":\"bad-key\"}}", addr_p0),
                    is_applied: true,
                    is_disabled: false,
                    sort_index: 0,
                    notes: None,
                    website_url: None,
                    meta: None,
                    created_at: "2026-01-01T00:00:00Z".into(),
                    updated_at: "2026-01-01T00:00:00Z".into(),
                },
                crate::providers::ProviderRecord {
                    id: "deepseek".into(),
                    name: "DeepSeek P1".into(),
                    category: "custom".into(),
                    settings_config: format!("{{\"base_url\":\"http://{}\",\"api_key\":\"ds-key\"}}", addr_p1),
                    is_applied: false,
                    is_disabled: false,
                    sort_index: 1,
                    notes: None,
                    website_url: None,
                    meta: Some(json!({
                        "apiFormat": "openai"
                    })),
                    created_at: "2026-01-01T00:00:00Z".into(),
                    updated_at: "2026-01-01T00:00:00Z".into(),
                },
            ];
        });

        // 4. Start Gateway Server
        let mut settings = GatewaySettings::default();
        settings.port = 15731;
        settings.port_auto_select = true;
        settings.failover_enabled = true;
        settings.save(&paths).unwrap();

        let handle = GatewayServer::start(paths.clone(), settings).await.unwrap();

        // 5. Send an Anthropic request
        let client = reqwest::Client::new();
        let req_body = json!({
            "model": "claude-3-7-sonnet",
            "messages": [
                {"role": "user", "content": "hi"}
            ]
        });

        let resp = client
            .post(format!("http://127.0.0.1:{}/anthropic/v1/messages", handle.status().port))
            .header("x-api-key", "test-key")
            .json(&req_body)
            .send()
            .await
            .expect("send request to gateway");

        let status = resp.status();
        let body_str = resp.text().await.unwrap();
        assert_eq!(status, 200);
        let resp_json: serde_json::Value = serde_json::from_str(&body_str).unwrap();
        assert_eq!(resp_json["content"][0]["text"], "hello from deepseek failover!");

        handle.stop();
    }
}

