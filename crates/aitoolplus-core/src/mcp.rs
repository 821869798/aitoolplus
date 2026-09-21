//! MCP (Model Context Protocol) server management + multi-tool sync.
//!
//! Mirrors ai-toolbox `coding/mcp` semantics:
//! - Each server carries its own `enabled_tools` list (not a global sync
//!   target set) and per-tool `sync_details`.
//! - Sync writes one server into a tool's config, preserving unrelated keys
//!   and other servers; remove deletes only that server.
//! - `command_normalize` handles Windows `cmd /c` wrapping for stdio
//!   runners; storage always keeps the unwrapped form.
//! - Format configs convert between the unified schema and tool-specific
//!   shapes (OpenCode local/remote + command array, Gemini httpUrl, …).
//! - Grok TOML uses its own schema (no `type`, npm commands unwrapped,
//!   `enabled` flag, optional timeouts/bearer).

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::paths::Paths;
use crate::tools::ToolId;

// ---------------------------------------------------------------------------
// Types (mirror upstream `types.rs`)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum McpServerType {
    Stdio,
    Http,
    Sse,
}

impl McpServerType {
    pub fn as_str(self) -> &'static str {
        match self {
            McpServerType::Stdio => "stdio",
            McpServerType::Http => "http",
            McpServerType::Sse => "sse",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServer {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub server_type: McpServerType,
    /// Unified config: stdio `{command, args, env}`; http/sse `{url, headers}`.
    #[serde(default)]
    pub server_config: Value,
    /// Tool keys this server is enabled in.
    #[serde(default)]
    pub enabled_tools: Vec<String>,
    /// Per-tool sync state: `{"tool": {status, synced_at, error_message}}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_details: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_note: Option<String>,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<i64>,
    #[serde(default)]
    pub sort_index: i32,
    #[serde(default = "default_true")]
    pub management_enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

fn default_true() -> bool {
    true
}

impl McpServer {
    pub fn new(name: impl Into<String>, server_type: McpServerType, config: Value) -> Self {
        let now = now_ms();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            server_type,
            server_config: config,
            enabled_tools: vec![],
            sync_details: None,
            description: None,
            user_group: None,
            user_note: None,
            favorite: false,
            tags: vec![],
            timeout: None,
            sort_index: 0,
            management_enabled: true,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn touch(&mut self) {
        self.updated_at = now_ms();
    }

    /// Public wrapper so the UI can record per-tool sync results.
    pub fn record_sync_pub(&mut self, tool: ToolId, ok: bool, error: Option<String>) {
        self.record_sync(tool, ok, error);
    }

    pub fn is_enabled_in(&self, tool: ToolId) -> bool {
        self.enabled_tools.iter().any(|k| k == tool.key())
    }

    fn record_sync(&mut self, tool: ToolId, ok: bool, error: Option<String>) {
        let mut details = self
            .sync_details
            .clone()
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        details.insert(
            tool.key().to_string(),
            serde_json::json!({
                "status": if ok { "ok" } else { "error" },
                "synced_at": now_ms(),
                "error_message": error,
            }),
        );
        self.sync_details = Some(Value::Object(details));
        self.touch();
    }
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpStore {
    #[serde(default)]
    pub servers: Vec<McpServer>,
    #[serde(default)]
    pub groups: Vec<McpGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpGroup {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default)]
    pub sort_index: i32,
    pub created_at: i64,
    pub updated_at: i64,
}

// ---------------------------------------------------------------------------
// Store operations
// ---------------------------------------------------------------------------

pub fn list(store: &McpStore) -> Vec<McpServer> {
    let mut v = store.servers.clone();
    v.sort_by(|a, b| {
        b.favorite
            .cmp(&a.favorite)
            .then(a.sort_index.cmp(&b.sort_index))
            .then(a.created_at.cmp(&b.created_at))
    });
    v
}

pub fn get<'a>(store: &'a McpStore, id: &str) -> Option<&'a McpServer> {
    store.servers.iter().find(|s| s.id == id)
}

pub fn upsert(store: &mut McpStore, mut server: McpServer) {
    server.touch();
    if let Some(existing) = store.servers.iter_mut().find(|s| s.id == server.id) {
        *existing = server;
    } else {
        server.sort_index = store
            .servers
            .iter()
            .map(|s| s.sort_index)
            .max()
            .unwrap_or(-1)
            + 1;
        store.servers.push(server);
    }
}

pub fn delete(store: &mut McpStore, id: &str) -> bool {
    let before = store.servers.len();
    store.servers.retain(|s| s.id != id);
    before != store.servers.len()
}

/// Toggle a server's enabled state for one tool; returns the new state.
pub fn toggle_favorite(store: &mut McpStore, id: &str) -> Option<bool> {
    let server = store.servers.iter_mut().find(|server| server.id == id)?;
    server.favorite = !server.favorite;
    server.touch();
    Some(server.favorite)
}

pub fn toggle_tool(store: &mut McpStore, id: &str, tool: ToolId) -> Option<bool> {
    let server = store.servers.iter_mut().find(|s| s.id == id)?;
    let key = tool.key().to_string();
    let enabled = !server.is_enabled_in(tool);
    if enabled {
        if !server.enabled_tools.contains(&key) {
            server.enabled_tools.push(key);
        }
    } else {
        server.enabled_tools.retain(|k| k != &key);
    }
    server.touch();
    Some(enabled)
}

pub fn set_management_enabled(store: &mut McpStore, id: &str, enabled: bool) -> bool {
    if let Some(server) = store.servers.iter_mut().find(|s| s.id == id) {
        server.management_enabled = enabled;
        server.touch();
        true
    } else {
        false
    }
}

pub fn update_metadata(
    store: &mut McpStore,
    id: &str,
    group: Option<String>,
    note: Option<String>,
) -> bool {
    if let Some(server) = store.servers.iter_mut().find(|s| s.id == id) {
        server.user_group = group.filter(|g| !g.trim().is_empty());
        server.user_note = note.filter(|n| !n.trim().is_empty());
        server.touch();
        true
    } else {
        false
    }
}

pub fn update_tags(store: &mut McpStore, id: &str, tags: Vec<String>) -> bool {
    if let Some(server) = store.servers.iter_mut().find(|s| s.id == id) {
        server.tags = tags;
        server.touch();
        true
    } else {
        false
    }
}

/// Parse MCP servers from a raw JSON snippet (mirrors ai-toolbox `mcpJsonImport.ts`).
/// Supports standard Claude Desktop, Cursor, VSCode shapes:
/// - `{ "mcpServers": { ... } }`
/// - `{ "servers": { ... } }`
/// - `{ "mcp": { "servers": { ... } } }`
/// - direct map `{ "server_name": { ... } }`
/// - single server `{ "name": "...", "command": "..." }`
pub fn parse_mcp_servers_from_json(
    json_str: &str,
) -> Result<Vec<(String, McpServerType, Value)>, String> {
    let value: Value = serde_json::from_str(json_str)
        .map_err(|e| format!("JSON 解析失败: {e}"))?;

    let mut out = Vec::new();

    fn has_server_shape(obj: &serde_json::Map<String, Value>) -> bool {
        obj.contains_key("command")
            || obj.contains_key("url")
            || obj.contains_key("httpUrl")
            || obj.contains_key("serverUrl")
    }

    fn parse_single(name: &str, obj: &serde_json::Map<String, Value>) -> Option<(String, McpServerType, Value)> {
        let server_type = if let Some(t) = obj.get("type").and_then(Value::as_str) {
            match t.to_lowercase().as_str() {
                "stdio" | "local" => McpServerType::Stdio,
                "http" => McpServerType::Http,
                "sse" | "remote" => McpServerType::Sse,
                _ if obj.contains_key("command") => McpServerType::Stdio,
                _ => McpServerType::Http,
            }
        } else if obj.contains_key("command") {
            McpServerType::Stdio
        } else if obj.contains_key("url") || obj.contains_key("httpUrl") || obj.contains_key("serverUrl") {
            McpServerType::Http
        } else {
            McpServerType::Stdio
        };

        let mut config = serde_json::Map::new();
        match server_type {
            McpServerType::Stdio => {
                let command = if let Some(cmd_arr) = obj.get("command").and_then(Value::as_array) {
                    let cmd = cmd_arr.first().and_then(Value::as_str).unwrap_or("").to_string();
                    let args: Vec<Value> = cmd_arr.iter().skip(1).cloned().collect();
                    config.insert("command".into(), Value::String(cmd));
                    config.insert("args".into(), Value::Array(args));
                    true
                } else if let Some(cmd_str) = obj.get("command").and_then(Value::as_str) {
                    config.insert("command".into(), Value::String(cmd_str.to_string()));
                    if let Some(args) = obj.get("args") {
                        config.insert("args".into(), args.clone());
                    } else {
                        config.insert("args".into(), Value::Array(vec![]));
                    }
                    true
                } else {
                    false
                };
                if !command {
                    return None;
                }
                if let Some(env) = obj.get("env").or_else(|| obj.get("environment")) {
                    config.insert("env".into(), env.clone());
                }
            }
            McpServerType::Http | McpServerType::Sse => {
                let url = obj.get("url")
                    .or_else(|| obj.get("httpUrl"))
                    .or_else(|| obj.get("serverUrl"))
                    .and_then(Value::as_str);
                if let Some(u) = url {
                    config.insert("url".into(), Value::String(u.to_string()));
                } else {
                    return None;
                }
                if let Some(headers) = obj.get("headers") {
                    config.insert("headers".into(), headers.clone());
                }
            }
        }

        Some((name.to_string(), server_type, Value::Object(config)))
    }

    if let Some(obj) = value.as_object() {
        if let Some(servers) = obj.get("mcpServers").and_then(Value::as_object) {
            for (name, s_val) in servers {
                if let Some(s_obj) = s_val.as_object() {
                    if let Some(parsed) = parse_single(name, s_obj) {
                        out.push(parsed);
                    }
                }
            }
        } else if let Some(servers) = obj.get("servers").and_then(Value::as_object) {
            for (name, s_val) in servers {
                if let Some(s_obj) = s_val.as_object() {
                    if let Some(parsed) = parse_single(name, s_obj) {
                        out.push(parsed);
                    }
                }
            }
        } else if let Some(servers) = obj.get("mcp").and_then(|m| m.get("servers")).and_then(Value::as_object) {
            for (name, s_val) in servers {
                if let Some(s_obj) = s_val.as_object() {
                    if let Some(parsed) = parse_single(name, s_obj) {
                        out.push(parsed);
                    }
                }
            }
        } else if has_server_shape(obj) {
            let name = obj.get("name")
                .or_else(|| obj.get("id"))
                .and_then(Value::as_str)
                .unwrap_or("imported-mcp-server");
            if let Some(parsed) = parse_single(name, obj) {
                out.push(parsed);
            }
        } else {
            // Assume map of servers
            for (name, s_val) in obj {
                if let Some(s_obj) = s_val.as_object() {
                    if let Some(parsed) = parse_single(name, s_obj) {
                        out.push(parsed);
                    }
                }
            }
        }
    }

    if out.is_empty() {
        return Err("未能识别有效的 MCP 服务器配置，请检查 JSON 格式是否包含 mcpServers 或服务器配置字段".into());
    }

    Ok(out)
}

// ---------------------------------------------------------------------------
// Command normalization (mirror upstream `command_normalize.rs`)
// ---------------------------------------------------------------------------

const WINDOWS_WRAP_COMMANDS: &[&str] = &["npx", "npm", "yarn", "pnpm", "node", "bun", "deno"];

fn needs_wrap(command: &str) -> bool {
    let cmd_name = std::path::Path::new(command)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(command);
    WINDOWS_WRAP_COMMANDS
        .iter()
        .any(|&c| cmd_name.eq_ignore_ascii_case(c))
}

fn is_cmd_wrapped(command: &str, args: &[Value]) -> bool {
    if !command.eq_ignore_ascii_case("cmd") && !command.eq_ignore_ascii_case("cmd.exe") {
        return false;
    }
    args.first()
        .and_then(Value::as_str)
        .map(|s| s.eq_ignore_ascii_case("/c"))
        .unwrap_or(false)
}

/// Remove the `cmd /c` wrapper: DB/import state always keeps raw commands.
pub fn unwrap_cmd_c(config: &Value) -> Value {
    let Some(obj) = config.as_object() else {
        return config.clone();
    };
    let server_type = obj.get("type").and_then(Value::as_str).unwrap_or("stdio");
    if server_type != "stdio" {
        return config.clone();
    }
    let command = obj.get("command").and_then(Value::as_str).unwrap_or("");
    let args = obj
        .get("args")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if !is_cmd_wrapped(command, &args) || args.len() < 2 {
        return config.clone();
    }
    let new_command = args[1].as_str().unwrap_or("");
    let new_args: Vec<Value> = args[2..].to_vec();
    let mut result = obj.clone();
    result.insert("command".into(), Value::String(new_command.into()));
    result.insert("args".into(), Value::Array(new_args));
    Value::Object(result)
}

/// Add the `cmd /c` wrapper when syncing to a Windows-local config.
pub fn wrap_cmd_c_for_target(config: &Value, should_wrap: bool) -> Value {
    if !should_wrap {
        return config.clone();
    }
    let Some(obj) = config.as_object() else {
        return config.clone();
    };
    let server_type = obj.get("type").and_then(Value::as_str).unwrap_or("stdio");
    if server_type != "stdio" {
        return config.clone();
    }
    let command = obj.get("command").and_then(Value::as_str).unwrap_or("");
    let args = obj
        .get("args")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if is_cmd_wrapped(command, &args) || !needs_wrap(command) {
        return config.clone();
    }
    let mut new_args = vec![Value::String("/c".into()), Value::String(command.into())];
    new_args.extend(args);
    let mut result = obj.clone();
    result.insert("command".into(), Value::String("cmd".into()));
    result.insert("args".into(), Value::Array(new_args));
    Value::Object(result)
}

/// Unwrap `cmd /c` from OpenCode's command array form.
pub fn unwrap_cmd_c_opencode_array(arr: &[Value]) -> Vec<Value> {
    if arr.len() < 3 {
        return arr.to_vec();
    }
    let first = arr[0].as_str().unwrap_or("");
    let second = arr[1].as_str().unwrap_or("");
    if (first.eq_ignore_ascii_case("cmd") || first.eq_ignore_ascii_case("cmd.exe"))
        && second.eq_ignore_ascii_case("/c")
    {
        arr[2..].to_vec()
    } else {
        arr.to_vec()
    }
}

/// Wrap `cmd /c` for OpenCode's command array form.
pub fn wrap_cmd_c_opencode_array_for_target(arr: &[Value], should_wrap: bool) -> Vec<Value> {
    if !should_wrap || arr.is_empty() {
        return arr.to_vec();
    }
    let first = arr[0].as_str().unwrap_or("");
    if first.eq_ignore_ascii_case("cmd") || first.eq_ignore_ascii_case("cmd.exe") {
        return arr.to_vec();
    }
    if !needs_wrap(first) {
        return arr.to_vec();
    }
    let mut result = vec![Value::String("cmd".into()), Value::String("/c".into())];
    result.extend(arr.iter().cloned());
    result
}

// ---------------------------------------------------------------------------
// Per-tool format configs (mirror upstream `format_configs.rs`)
// ---------------------------------------------------------------------------

/// Which tools support MCP sync and how.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpFormat {
    /// Claude Code / Pi / OMP / Antigravity-like: JSON `mcpServers` with
    /// standard {command, args, env} / {type, url} shapes.
    JsonStandard { field: &'static str },
    /// OpenCode: JSON `mcp` with local/remote types and command arrays.
    OpenCode,
    /// Gemini CLI: JSON `mcpServers` with httpUrl/url split.
    GeminiLike,
    /// Codex: TOML `[mcp_servers.<name>]` with type/command/http_headers.
    Toml { field: &'static str },
    /// Grok: TOML with its own schema (no type field, `enabled` flag).
    GrokToml,
    /// Kimi: TOML `[mcp_servers.<name>]` (Codex-like).
    KimiToml,
}

pub fn mcp_format(tool: ToolId) -> Option<McpFormat> {
    Some(match tool {
        ToolId::ClaudeCode => McpFormat::JsonStandard {
            field: "mcpServers",
        },
        ToolId::Codex => McpFormat::Toml {
            field: "mcp_servers",
        },
        ToolId::GeminiCli => McpFormat::GeminiLike,
        ToolId::OpenCode => McpFormat::OpenCode,
        ToolId::Grok => McpFormat::GrokToml,
        ToolId::Kimi => McpFormat::KimiToml,
        ToolId::OpenClaw => McpFormat::JsonStandard {
            field: "mcpServers",
        },
        ToolId::Pi => McpFormat::JsonStandard {
            field: "mcpServers",
        },
        ToolId::OhMyPi => McpFormat::JsonStandard {
            field: "mcpServers",
        },
        _ => return None,
    })
}

/// Config file path for a tool's MCP section.
pub fn mcp_config_path(paths: &Paths, tool: ToolId) -> Option<PathBuf> {
    Some(match tool {
        // Claude Code: default root keeps mcpServers in ~/.claude.json
        ToolId::ClaudeCode => paths.home.join(".claude.json"),
        ToolId::Codex => paths.tool_root(tool).join("config.toml"),
        ToolId::GeminiCli => paths.tool_root(tool).join("settings.json"),
        ToolId::OpenCode => {
            // .jsonc takes precedence when present (upstream opencode_path)
            let dir = paths.tool_root(tool);
            let jsonc = dir.join("opencode.jsonc");
            if jsonc.exists() {
                jsonc
            } else {
                dir.join("opencode.json")
            }
        }
        ToolId::Grok => paths.tool_root(tool).join("config.toml"),
        ToolId::Kimi => paths.tool_root(tool).join("config.toml"),
        ToolId::OpenClaw => paths.tool_root(tool).join("openclaw.json"),
        ToolId::Pi => paths.tool_root(tool).join("settings.json"),
        ToolId::OhMyPi => paths.tool_root(tool).join("mcp.json"),
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// Sync: one server, one tool (mirror upstream `config_sync.rs`)
// ---------------------------------------------------------------------------

/// Sync (add/update) one server into one tool's config file.
pub fn sync_server_to_tool(paths: &Paths, server: &McpServer, tool: ToolId) -> Result<(), String> {
    let format =
        mcp_format(tool).ok_or_else(|| format!("{} does not support MCP", tool.name_en()))?;
    let path = mcp_config_path(paths, tool)
        .ok_or_else(|| format!("{} has no MCP config path", tool.name_en()))?;
    let should_wrap = cfg!(windows);

    match format {
        McpFormat::JsonStandard { field } => {
            let entry = json_standard_entry(server, tool, should_wrap)?;
            upsert_json_server(&path, field, &server.name, &entry)
        }
        McpFormat::OpenCode => {
            let entry = opencode_entry(server, should_wrap)?;
            upsert_json_server(&path, "mcp", &server.name, &entry)
        }
        McpFormat::GeminiLike => {
            let entry = gemini_entry(server)?;
            upsert_json_server(&path, "mcpServers", &server.name, &entry)
        }
        McpFormat::Toml { field } => {
            let table = codex_toml_table(server, should_wrap)?;
            upsert_toml_server(&path, field, &server.name, table)
        }
        McpFormat::KimiToml => {
            let table = codex_toml_table(server, should_wrap)?;
            upsert_toml_server(&path, "mcp_servers", &server.name, table)
        }
        McpFormat::GrokToml => {
            let table = grok_toml_table(server, true)?;
            upsert_toml_server(&path, "mcp_servers", &server.name, table)
        }
    }
}

/// Remove one server from one tool's config file.
pub fn remove_server_from_tool(
    paths: &Paths,
    server_name: &str,
    tool: ToolId,
) -> Result<(), String> {
    let format =
        mcp_format(tool).ok_or_else(|| format!("{} does not support MCP", tool.name_en()))?;
    let path = mcp_config_path(paths, tool)
        .ok_or_else(|| format!("{} has no MCP config path", tool.name_en()))?;
    let field = match format {
        McpFormat::JsonStandard { field } => field,
        McpFormat::OpenCode => "mcp",
        McpFormat::GeminiLike => "mcpServers",
        McpFormat::Toml { .. } | McpFormat::KimiToml | McpFormat::GrokToml => "mcp_servers",
    };
    match format {
        McpFormat::JsonStandard { .. } | McpFormat::OpenCode | McpFormat::GeminiLike => {
            remove_json_server(&path, field, server_name)
        }
        McpFormat::Toml { .. } | McpFormat::KimiToml | McpFormat::GrokToml => {
            remove_toml_server(&path, "mcp_servers", server_name)
        }
    }
}

/// Sync every server to every tool it is enabled in; returns per-tool counts.
pub fn sync_all_enabled(paths: &Paths, store: &mut McpStore) -> Vec<(ToolId, usize, usize)> {
    let mut per_tool: Vec<(ToolId, usize, usize)> = vec![];
    let mut syncs: Vec<(String, ToolId, Result<(), String>)> = vec![];

    for server in &store.servers {
        if !server.management_enabled {
            continue;
        }
        let enabled: Vec<ToolId> = server
            .enabled_tools
            .iter()
            .filter_map(|k| ToolId::from_key(k))
            .collect();
        for tool in enabled {
            let result = sync_server_to_tool(paths, server, tool);
            syncs.push((server.id.clone(), tool, result));
        }
        // Servers disabled for a tool must be removed from that tool's file.
        let disabled: Vec<ToolId> = ToolId::ALL
            .iter()
            .copied()
            .filter(|t| mcp_format(*t).is_some() && !server.is_enabled_in(*t))
            .collect();
        for tool in disabled {
            // Only remove when the tool ever saw this server (sync_details
            // recorded a previous sync for it).
            let had = server
                .sync_details
                .as_ref()
                .and_then(|d| d.get(tool.key()))
                .is_some();
            if had {
                let _ = remove_server_from_tool(paths, &server.name, tool);
            }
        }
    }

    for (id, tool, result) in syncs {
        if let Some(server) = store.servers.iter_mut().find(|s| s.id == id) {
            match result {
                Ok(()) => server.record_sync(tool, true, None),
                Err(e) => server.record_sync(tool, false, Some(e)),
            }
        }
    }

    for tool in ToolId::ALL {
        if mcp_format(tool).is_none() {
            continue;
        }
        let ok = store
            .servers
            .iter()
            .filter(|s| s.is_enabled_in(tool) && s.sync_ok_in(tool))
            .count();
        let failed = store
            .servers
            .iter()
            .filter(|s| s.is_enabled_in(tool) && s.sync_failed_in(tool))
            .count();
        if ok + failed > 0 {
            per_tool.push((tool, ok, failed));
        }
    }
    per_tool
}

impl McpServer {
    fn sync_ok_in(&self, tool: ToolId) -> bool {
        self.sync_details
            .as_ref()
            .and_then(|d| d.get(tool.key()))
            .and_then(|s| s.get("status"))
            .and_then(Value::as_str)
            == Some("ok")
    }

    fn sync_failed_in(&self, tool: ToolId) -> bool {
        self.sync_details
            .as_ref()
            .and_then(|d| d.get(tool.key()))
            .and_then(|s| s.get("status"))
            .and_then(Value::as_str)
            == Some("error")
    }
}

// ---------------------------------------------------------------------------
// Entry builders (unified -> tool format)
// ---------------------------------------------------------------------------

/// {type?, command, args, env} with optional cmd /c wrap.
fn json_standard_entry(
    server: &McpServer,
    tool: ToolId,
    should_wrap: bool,
) -> Result<Value, String> {
    let mut config = server.server_config.clone();
    config
        .as_object_mut()
        .ok_or("server_config must be an object")?
        .insert(
            "type".into(),
            Value::String(server.server_type.as_str().into()),
        );

    let mut config = if should_wrap {
        wrap_cmd_c_for_target(&config, true)
    } else {
        config
    };

    // OMP mcp.json entries don't carry a `type` for stdio.
    if tool == ToolId::OhMyPi
        && server.server_type == McpServerType::Stdio
        && let Some(obj) = config.as_object_mut()
    {
        obj.remove("type");
    }
    Ok(config)
}

/// OpenCode: `local`/`remote` + `command: [...]` + `environment` + `enabled`.
fn opencode_entry(server: &McpServer, should_wrap: bool) -> Result<Value, String> {
    match server.server_type {
        McpServerType::Stdio => {
            let command = server
                .server_config
                .get("command")
                .and_then(Value::as_str)
                .ok_or("stdio server requires 'command'")?;
            let args: Vec<Value> = server
                .server_config
                .get("args")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let mut arr = vec![Value::String(command.to_string())];
            arr.extend(args);
            let arr = wrap_cmd_c_opencode_array_for_target(&arr, should_wrap);
            let mut obj = serde_json::Map::new();
            obj.insert("type".into(), Value::String("local".into()));
            obj.insert("command".into(), Value::Array(arr));
            if let Some(env) = server.server_config.get("env").and_then(Value::as_object) {
                obj.insert("environment".into(), Value::Object(env.clone()));
            }
            obj.insert("enabled".into(), Value::Bool(true));
            Ok(Value::Object(obj))
        }
        McpServerType::Http | McpServerType::Sse => {
            let url = server
                .server_config
                .get("url")
                .and_then(Value::as_str)
                .ok_or("remote server requires 'url'")?;
            let mut obj = serde_json::Map::new();
            obj.insert("type".into(), Value::String("remote".into()));
            obj.insert("url".into(), Value::String(url.into()));
            if let Some(headers) = server
                .server_config
                .get("headers")
                .and_then(Value::as_object)
            {
                obj.insert("headers".into(), Value::Object(headers.clone()));
            }
            obj.insert("enabled".into(), Value::Bool(true));
            Ok(Value::Object(obj))
        }
    }
}

/// Gemini CLI: http -> httpUrl, sse -> url, stdio plain.
fn gemini_entry(server: &McpServer) -> Result<Value, String> {
    match server.server_type {
        McpServerType::Stdio => {
            let mut config = server.server_config.clone();
            if let Some(obj) = config.as_object_mut() {
                obj.remove("type");
            }
            Ok(config)
        }
        McpServerType::Http => {
            let url = server
                .server_config
                .get("url")
                .and_then(Value::as_str)
                .ok_or("http server requires 'url'")?;
            let mut obj = serde_json::Map::new();
            obj.insert("httpUrl".into(), Value::String(url.into()));
            Ok(Value::Object(obj))
        }
        McpServerType::Sse => {
            let url = server
                .server_config
                .get("url")
                .and_then(Value::as_str)
                .ok_or("sse server requires 'url'")?;
            let mut obj = serde_json::Map::new();
            obj.insert("url".into(), Value::String(url.into()));
            Ok(Value::Object(obj))
        }
    }
}

/// Codex-style TOML table: type/command/args/env or type/url/http_headers.
fn codex_toml_table(server: &McpServer, should_wrap: bool) -> Result<toml_edit::Table, String> {
    let mut t = toml_edit::Table::new();
    match server.server_type {
        McpServerType::Stdio => {
            let command = server
                .server_config
                .get("command")
                .and_then(Value::as_str)
                .ok_or("stdio server requires 'command'")?;
            let args: Vec<String> = server
                .server_config
                .get("args")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default();

            let wrapped = wrap_cmd_c_for_target(
                &serde_json::json!({"type": "stdio", "command": command, "args": args}),
                should_wrap,
            );
            let final_command = wrapped
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or(command);
            let final_args: Vec<String> = wrapped
                .get("args")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(String::from)
                        .collect()
                })
                .unwrap_or(args);

            t.insert("type", toml_edit::value("stdio"));
            t.insert("command", toml_edit::value(final_command));
            if !final_args.is_empty() {
                let arr = toml_edit::Array::from_iter(final_args);
                t.insert("args", toml_edit::value(arr));
            }
            if let Some(env) = server.server_config.get("env").and_then(Value::as_object) {
                let mut env_tbl = toml_edit::Table::new();
                for (k, v) in env {
                    if let Some(s) = v.as_str() {
                        env_tbl.insert(k, toml_edit::value(s));
                    }
                }
                if !env_tbl.is_empty() {
                    t.insert("env", toml_edit::Item::Table(env_tbl));
                }
            }
        }
        McpServerType::Http | McpServerType::Sse => {
            let url = server
                .server_config
                .get("url")
                .and_then(Value::as_str)
                .ok_or("remote server requires 'url'")?;
            t.insert("type", toml_edit::value(server.server_type.as_str()));
            t.insert("url", toml_edit::value(url));
            if let Some(headers) = server
                .server_config
                .get("headers")
                .and_then(Value::as_object)
            {
                let mut h = toml_edit::Table::new();
                for (k, v) in headers {
                    if let Some(s) = v.as_str() {
                        h.insert(k, toml_edit::value(s));
                    }
                }
                if !h.is_empty() {
                    t.insert("http_headers", toml_edit::Item::Table(h));
                }
            }
        }
    }
    for optional in ["startup_timeout_sec", "tool_timeout_sec"] {
        if let Some(v) = server.server_config.get(optional).filter(|v| !v.is_null())
            && let Ok(item) = json_to_toml_item(v)
        {
            t.insert(optional, item);
        }
    }
    Ok(t)
}

/// Grok TOML table: no `type`, npm commands stay unwrapped, `enabled` flag.
fn grok_toml_table(server: &McpServer, enabled: bool) -> Result<toml_edit::Table, String> {
    let mut t = toml_edit::Table::new();
    match server.server_type {
        McpServerType::Stdio => {
            let command = server
                .server_config
                .get("command")
                .and_then(Value::as_str)
                .ok_or("stdio server requires 'command'")?;
            t.insert("command", toml_edit::value(command));
            if let Some(args) = server.server_config.get("args").and_then(Value::as_array) {
                let arr = toml_edit::Array::from_iter(args.iter().filter_map(Value::as_str));
                t.insert("args", toml_edit::value(arr));
            }
            if let Some(env) = server.server_config.get("env").and_then(Value::as_object) {
                let mut env_tbl = toml_edit::Table::new();
                for (k, v) in env {
                    if let Some(s) = v.as_str() {
                        env_tbl.insert(k, toml_edit::value(s));
                    }
                }
                if !env_tbl.is_empty() {
                    t.insert("env", toml_edit::Item::Table(env_tbl));
                }
            }
        }
        McpServerType::Http | McpServerType::Sse => {
            let url = server
                .server_config
                .get("url")
                .and_then(Value::as_str)
                .ok_or("remote server requires 'url'")?;
            t.insert("url", toml_edit::value(url));
            if let Some(headers) = server
                .server_config
                .get("headers")
                .and_then(Value::as_object)
            {
                let mut h = toml_edit::Table::new();
                for (k, v) in headers {
                    if let Some(s) = v.as_str() {
                        h.insert(k, toml_edit::value(s));
                    }
                }
                if !h.is_empty() {
                    t.insert("headers", toml_edit::Item::Table(h));
                }
            }
            if let Some(b) = server
                .server_config
                .get("bearer_token_env_var")
                .and_then(Value::as_str)
            {
                t.insert("bearer_token_env_var", toml_edit::value(b));
            }
        }
    }
    for optional in [
        "cwd",
        "startup_timeout_sec",
        "tool_timeout_sec",
        "tool_timeouts",
    ] {
        if let Some(v) = server.server_config.get(optional).filter(|v| !v.is_null())
            && let Ok(item) = json_to_toml_item(v)
        {
            t.insert(optional, item);
        }
    }
    t.insert("enabled", toml_edit::value(enabled));
    Ok(t)
}

fn json_to_toml_item(value: &Value) -> Result<toml_edit::Item, String> {
    let wrapper = serde_json::json!({ "holder": value });
    let serialized = toml::to_string(&wrapper).map_err(|e| e.to_string())?;
    let mut doc: toml_edit::DocumentMut = serialized
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| e.to_string())?;
    doc.remove("holder")
        .ok_or_else(|| "failed to build TOML field".to_string())
}

// ---------------------------------------------------------------------------
// File IO (JSON via json5-tolerant parse; TOML via toml_edit)
// ---------------------------------------------------------------------------

/// Parse a JSON/JSONC file body to a Value; missing/empty -> `{}`.
fn parse_jsonc(raw: &str) -> Result<Value, String> {
    if raw.trim().is_empty() {
        return Ok(Value::Object(Default::default()));
    }
    // Strip comments for JSONC tolerance (line + block), like upstream json5.
    let cleaned = strip_jsonc_comments(raw);
    serde_json::from_str(&cleaned).map_err(|e| format!("failed to parse config: {e}"))
}

/// Minimal JSONC comment stripping (line comments, block comments, keeping
/// them inside string literals intact).
fn strip_jsonc_comments(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_string = false;
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            if c == '\\' {
                if let Some(&n) = chars.peek() {
                    out.push(n);
                    chars.next();
                }
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                out.push(c);
            }
            '/' if chars.peek() == Some(&'/') => {
                for c2 in chars.by_ref() {
                    if c2 == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next(); // consume '*'
                let mut closed = false;
                while let Some(c2) = chars.next() {
                    if c2 == '*' && chars.peek() == Some(&'/') {
                        chars.next();
                        closed = true;
                        break;
                    }
                }
                let _ = closed;
            }
            _ => out.push(c),
        }
    }
    out
}

/// Get or create an object at a possibly-nested path like `mcp.servers`.
fn ensure_json_object_path<'a>(root: &'a mut Value, path: &str) -> Result<&'a mut Value, String> {
    let mut current = root;
    for part in path.split('.') {
        if !current.is_object() {
            return Err(format!("config path {part} is not an object"));
        }
        let obj = current.as_object_mut().unwrap();
        current = obj
            .entry(part.to_string())
            .or_insert_with(|| Value::Object(Default::default()));
    }
    Ok(current)
}

fn upsert_json_server(
    path: &Path,
    field: &str,
    server_name: &str,
    entry: &Value,
) -> Result<(), String> {
    let mut config: Value = if path.exists() {
        let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
        parse_jsonc(&raw)?
    } else {
        Value::Object(Default::default())
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let servers = ensure_json_object_path(&mut config, field)?;
    servers
        .as_object_mut()
        .ok_or_else(|| format!("{field} is not a JSON object"))?
        .insert(server_name.to_string(), entry.clone());
    // OMP schema stamp: keep top-level $schema when we create the file.
    if path.ends_with("mcp.json")
        && !config.as_object().unwrap().contains_key("$schema")
        && config.as_object().map(|o| o.is_empty()).unwrap_or(false)
    {
        let obj = config.as_object_mut().unwrap();
        obj.insert(
            "$schema".into(),
            Value::String("https://raw.githubusercontent.com/can1357/oh-my-pi/main/packages/coding-agent/src/config/mcp-schema.json".into()),
        );
    }
    let out = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    fs::write(path, out).map_err(|e| e.to_string())
}

fn remove_json_server(path: &Path, field: &str, server_name: &str) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
    if raw.trim().is_empty() {
        return Ok(());
    }
    let mut config: Value = parse_jsonc(&raw)?;
    if let Ok(servers) = ensure_json_object_path(&mut config, field)
        && let Some(obj) = servers.as_object_mut()
    {
        obj.remove(server_name);
    }
    let out = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    fs::write(path, out).map_err(|e| e.to_string())
}

fn upsert_toml_server(
    path: &Path,
    field: &str,
    server_name: &str,
    table: toml_edit::Table,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut doc: toml_edit::DocumentMut = if path.exists() {
        fs::read_to_string(path)
            .unwrap_or_default()
            .parse()
            .unwrap_or_default()
    } else {
        Default::default()
    };
    if !doc.contains_key(field) {
        doc[field] = toml_edit::table();
    }
    doc[field][server_name] = toml_edit::Item::Table(table);
    fs::write(path, doc.to_string()).map_err(|e| e.to_string())
}

fn remove_toml_server(path: &Path, field: &str, server_name: &str) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let raw = fs::read_to_string(path).unwrap_or_default();
    let Ok(mut doc) = raw.parse::<toml_edit::DocumentMut>() else {
        return Ok(());
    };
    if let Some(servers) = doc.get_mut(field).and_then(|s| s.as_table_mut()) {
        servers.remove(server_name);
    }
    fs::write(path, doc.to_string()).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Discovery: scan tool configs for existing servers (import into store)
// ---------------------------------------------------------------------------

/// Scan all supported tool configs and return discovered server entries.
pub fn scan_tool_configs(paths: &Paths) -> Vec<(ToolId, String, McpServerType, Value)> {
    let mut out = vec![];
    for tool in ToolId::ALL {
        let Some(format) = mcp_format(tool) else {
            continue;
        };
        let Some(path) = mcp_config_path(paths, tool) else {
            continue;
        };
        if !path.exists() {
            continue;
        }
        let Ok(raw) = fs::read_to_string(&path) else {
            continue;
        };
        let entries: Vec<(String, Value)> = match format {
            McpFormat::JsonStandard { field } => {
                let Ok(v) = parse_jsonc(&raw) else { continue };
                v.get(field)
                    .and_then(Value::as_object)
                    .map(|o| o.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                    .unwrap_or_default()
            }
            McpFormat::GeminiLike => {
                let Ok(v) = parse_jsonc(&raw) else { continue };
                v.get("mcpServers")
                    .and_then(Value::as_object)
                    .map(|o| o.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                    .unwrap_or_default()
            }
            McpFormat::OpenCode => {
                let Ok(v) = parse_jsonc(&raw) else { continue };
                v.get("mcp")
                    .and_then(Value::as_object)
                    .map(|o| o.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                    .unwrap_or_default()
            }
            McpFormat::Toml { field } => {
                let Ok(doc) = raw.parse::<toml_edit::DocumentMut>() else {
                    continue;
                };
                doc.get(field)
                    .and_then(|f| f.as_table())
                    .map(|t| {
                        t.iter()
                            .filter_map(|(k, v)| {
                                v.as_table()
                                    .map(|tbl| (k.to_string(), toml_table_to_json(tbl)))
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            }
            McpFormat::KimiToml | McpFormat::GrokToml => {
                let Ok(doc) = raw.parse::<toml_edit::DocumentMut>() else {
                    continue;
                };
                doc.get("mcp_servers")
                    .and_then(|f| f.as_table())
                    .map(|t| {
                        t.iter()
                            .filter_map(|(k, v)| {
                                v.as_table()
                                    .map(|tbl| (k.to_string(), toml_table_to_json(tbl)))
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            }
        };
        for (name, entry) in entries {
            let (server_type, unified) = normalize_discovered(&entry, format);
            out.push((tool, name, server_type, unified));
        }
    }
    out
}

fn toml_table_to_json(tbl: &toml_edit::Table) -> Value {
    let map = serde_json::Map::new();
    let mut obj = map;
    for (k, item) in tbl.iter() {
        if let Some(v) = item.as_value() {
            if let Ok(j) = toml_value_to_json(v) {
                obj.insert(k.to_string(), j);
            }
        } else if let Some(sub) = item.as_table() {
            obj.insert(k.to_string(), toml_table_to_json(sub));
        }
    }
    Value::Object(obj)
}

fn toml_value_to_json(v: &toml_edit::Value) -> Result<Value, String> {
    let s = v.to_string();
    // toml_edit values print as TOML literals; parse via a wrapper doc.
    let doc = format!("x = {s}");
    let parsed: toml_edit::DocumentMut = doc
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| e.to_string())?;
    if let Some(item) = parsed.get("x")
        && let Some(val) = item.as_value()
    {
        if let Some(s) = val.as_str() {
            return Ok(Value::String(s.into()));
        }
        if let Some(b) = val.as_bool() {
            return Ok(Value::Bool(b));
        }
        if let Some(i) = val.as_integer() {
            return Ok(Value::Number(i.into()));
        }
        if let Some(f) = val.as_float() {
            return Ok(serde_json::Number::from_f64(f)
                .map(Value::Number)
                .unwrap_or(Value::Null));
        }
        if let Some(arr) = val.as_array() {
            let items: Vec<Value> = arr
                .iter()
                .map(|a| toml_value_to_json(a).unwrap_or(Value::Null))
                .collect();
            return Ok(Value::Array(items));
        }
    }
    Ok(Value::String(s))
}

/// Convert a discovered tool-format entry back to the unified schema.
fn normalize_discovered(entry: &Value, format: McpFormat) -> (McpServerType, Value) {
    let mut unified = serde_json::Map::new();
    // Determine type per format.
    let declared = entry.get("type").and_then(Value::as_str);
    let server_type = match format {
        McpFormat::OpenCode => match declared {
            Some("local") => McpServerType::Stdio,
            Some("remote") => {
                // OpenCode http remote
                McpServerType::Http
            }
            _ => McpServerType::Stdio,
        },
        _ => match declared {
            Some("http") => McpServerType::Http,
            Some("sse") => McpServerType::Sse,
            _ => {
                // Infer from URL fields when type is missing.
                if entry.get("httpUrl").is_some() {
                    McpServerType::Http
                } else if entry.get("url").is_some() && entry.get("command").is_none() {
                    McpServerType::Sse
                } else {
                    McpServerType::Stdio
                }
            }
        },
    };

    match server_type {
        McpServerType::Stdio => {
            // OpenCode: command is an array.
            if let Some(arr) = entry.get("command").and_then(Value::as_array) {
                let unwrapped = unwrap_cmd_c_opencode_array(arr);
                if let Some(first) = unwrapped.first().and_then(Value::as_str) {
                    unified.insert("command".into(), Value::String(first.into()));
                    let rest: Vec<Value> = unwrapped[1..].to_vec();
                    if !rest.is_empty() {
                        unified.insert("args".into(), Value::Array(rest));
                    }
                }
                if let Some(env) = entry.get("environment").and_then(Value::as_object) {
                    unified.insert("env".into(), Value::Object(env.clone()));
                }
            } else {
                if let Some(cmd) = entry.get("command").and_then(Value::as_str) {
                    unified.insert("command".into(), Value::String(cmd.into()));
                }
                if let Some(args) = entry.get("args").and_then(Value::as_array) {
                    unified.insert("args".into(), Value::Array(args.clone()));
                }
                if let Some(env) = entry.get("env").and_then(Value::as_object) {
                    unified.insert("env".into(), Value::Object(env.clone()));
                }
                // Grok TOML uses `environment` too? Keep `env` canonical.
                if let Some(env) = entry.get("environment").and_then(Value::as_object) {
                    unified.insert("env".into(), Value::Object(env.clone()));
                }
            }
        }
        McpServerType::Http | McpServerType::Sse => {
            let url = entry
                .get("url")
                .or_else(|| entry.get("httpUrl"))
                .or_else(|| entry.get("serverUrl"))
                .and_then(Value::as_str)
                .map(|s| s.to_string());
            if let Some(url) = url {
                unified.insert("url".into(), Value::String(url));
            }
            let headers = entry
                .get("headers")
                .or_else(|| entry.get("http_headers"))
                .and_then(Value::as_object)
                .cloned();
            if let Some(headers) = headers {
                unified.insert("headers".into(), Value::Object(headers));
            }
        }
    }

    // Unwrap any cmd /c storage form so the DB keeps the portable command.
    let value = unwrap_cmd_c(&Value::Object(unified));
    let mut obj = value.as_object().cloned().unwrap_or_default();
    // Carry optional timeout/bearer fields through.
    for optional in [
        "startup_timeout_sec",
        "tool_timeout_sec",
        "tool_timeouts",
        "bearer_token_env_var",
        "cwd",
    ] {
        if let Some(v) = entry.get(optional).filter(|v| !v.is_null()) {
            obj.insert(optional.to_string(), v.clone());
        }
    }
    (server_type, Value::Object(obj))
}

/// Import discovered entries into the store, enabling them for the tool
/// they came from. Idempotent by name.
pub fn import_discovered(paths: &Paths, store: &mut McpStore) -> (usize, Vec<(ToolId, String)>) {
    let discovered = scan_tool_configs(paths);
    let mut imported = 0;
    let mut sources: Vec<(ToolId, String)> = vec![];
    let existing: std::collections::HashSet<String> =
        store.servers.iter().map(|s| s.name.clone()).collect();

    for (tool, name, server_type, unified) in discovered {
        if existing.contains(&name) {
            // Already managed; ensure it stays enabled for this tool.
            if let Some(server) = store.servers.iter_mut().find(|s| s.name == name) {
                let key = tool.key().to_string();
                if !server.enabled_tools.contains(&key) {
                    server.enabled_tools.push(key);
                    server.touch();
                }
            }
            continue;
        }
        let mut server = McpServer::new(name.clone(), server_type, unified);
        server.enabled_tools.push(tool.key().to_string());
        server.description = Some(format!("imported from {}", tool.name_en()));
        store.servers.push(server);
        imported += 1;
        sources.push((tool, name));
    }
    (imported, sources)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths_with(root: &Path) -> Paths {
        Paths::new(root.join("home"), root.join("data"))
    }

    #[test]
    fn cmd_c_wrap_unwrap_roundtrip() {
        let raw = serde_json::json!({
            "type": "stdio", "command": "npx", "args": ["-y", "@mcp/fs"]
        });
        let wrapped = wrap_cmd_c_for_target(&raw, true);
        assert_eq!(wrapped["command"], "cmd");
        assert_eq!(wrapped["args"][0], "/c");
        assert_eq!(wrapped["args"][1], "npx");
        let unwrapped = unwrap_cmd_c(&wrapped);
        assert_eq!(unwrapped["command"], "npx");
        assert_eq!(unwrapped["args"][0], "-y");
        // non-windows target doesn't wrap
        let plain = wrap_cmd_c_for_target(&raw, false);
        assert_eq!(plain["command"], "npx");
        // commands outside the list never wrap
        let other = serde_json::json!({"type": "stdio", "command": "custom-bin"});
        assert_eq!(wrap_cmd_c_for_target(&other, true)["command"], "custom-bin");
    }

    #[test]
    fn opencode_array_wrap_unwrap() {
        let arr = vec![
            Value::String("npx".into()),
            Value::String("-y".into()),
            Value::String("fs".into()),
        ];
        let wrapped = wrap_cmd_c_opencode_array_for_target(&arr, true);
        assert_eq!(wrapped[0], "cmd");
        assert_eq!(wrapped[1], "/c");
        let unwrapped = unwrap_cmd_c_opencode_array(&wrapped);
        assert_eq!(unwrapped[0], "npx");
        assert_eq!(unwrapped.len(), 3);
    }

    #[test]
    fn sync_claude_json_preserves_other_sections() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_with(dir.path());
        std::fs::create_dir_all(&paths.home).unwrap();
        let cfg = paths.home.join(".claude.json");
        std::fs::write(
            &cfg,
            r#"{"projects": {"a": {"b": 1}}, "mcpServers": {"old": {"command": "x"}}}"#,
        )
        .unwrap();

        let mut server = McpServer::new(
            "fs",
            McpServerType::Stdio,
            serde_json::json!({
                "command": "npx", "args": ["-y", "@mcp/fs"], "env": {"A": "1"}
            }),
        );
        server.enabled_tools.push("claude_code".into());
        sync_server_to_tool(&paths, &server, ToolId::ClaudeCode).unwrap();

        let v: Value = serde_json::from_str(&std::fs::read_to_string(&cfg).unwrap()).unwrap();
        assert_eq!(v["projects"]["a"]["b"], 1, "unrelated section lost");
        assert!(v["mcpServers"]["old"].is_object(), "other server lost");
        let entry = &v["mcpServers"]["fs"];
        if cfg!(windows) {
            assert_eq!(entry["command"], "cmd");
            assert_eq!(entry["args"][0], "/c");
        } else {
            assert_eq!(entry["command"], "npx");
        }
        assert_eq!(entry["env"]["A"], "1");
    }

    #[test]
    fn sync_and_remove_codex_toml() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_with(dir.path());
        std::fs::create_dir_all(&paths.home).unwrap();
        let cfg = paths.home.join(".codex").join("config.toml");
        std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
        std::fs::write(
            &cfg,
            "model = \"gpt-4o\"\n\n[mcp_servers.old]\ncommand = \"x\"\n",
        )
        .unwrap();

        let server = McpServer::new(
            "fs",
            McpServerType::Stdio,
            serde_json::json!({
                "command": "npx", "args": ["-y", "fs"], "env": {"K": "V"}
            }),
        );
        sync_server_to_tool(&paths, &server, ToolId::Codex).unwrap();
        let toml = std::fs::read_to_string(&cfg).unwrap();
        assert!(toml.contains("model = \"gpt-4o\""), "root key lost");
        assert!(toml.contains("[mcp_servers.old]"), "other server lost");
        assert!(toml.contains("[mcp_servers.fs]"));
        assert!(toml.contains("type = \"stdio\""));
        assert!(toml.contains("[mcp_servers.fs.env]"));

        remove_server_from_tool(&paths, "fs", ToolId::Codex).unwrap();
        let toml = std::fs::read_to_string(&cfg).unwrap();
        assert!(!toml.contains("[mcp_servers.fs]"));
        assert!(toml.contains("[mcp_servers.old]"));
        assert!(toml.contains("model = \"gpt-4o\""));
    }

    #[test]
    fn grok_toml_has_enabled_and_no_type() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_with(dir.path());
        let server = McpServer::new(
            "fs",
            McpServerType::Stdio,
            serde_json::json!({
                "command": "npx", "args": ["fs"]
            }),
        );
        sync_server_to_tool(&paths, &server, ToolId::Grok).unwrap();
        let toml = std::fs::read_to_string(paths.home.join(".grok").join("config.toml")).unwrap();
        assert!(toml.contains("[mcp_servers.fs]"));
        assert!(toml.contains("enabled = true"));
        assert!(
            !toml.contains("type = "),
            "grok must not write type: {toml}"
        );
    }

    #[test]
    fn opencode_local_remote_shapes() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_with(dir.path());
        let local = McpServer::new(
            "fs",
            McpServerType::Stdio,
            serde_json::json!({
                "command": "npx", "args": ["-y", "fs"], "env": {"E": "1"}
            }),
        );
        sync_server_to_tool(&paths, &local, ToolId::OpenCode).unwrap();
        let v: Value = serde_json::from_str(
            &std::fs::read_to_string(
                paths
                    .home
                    .join(".config")
                    .join("opencode")
                    .join("opencode.json"),
            )
            .unwrap(),
        )
        .unwrap();
        let entry = &v["mcp"]["fs"];
        assert_eq!(entry["type"], "local");
        assert_eq!(entry["enabled"], true);
        assert_eq!(entry["environment"]["E"], "1");
        if cfg!(windows) {
            assert_eq!(entry["command"][0], "cmd");
        } else {
            assert_eq!(entry["command"][0], "npx");
        }

        let remote = McpServer::new(
            "api",
            McpServerType::Http,
            serde_json::json!({
                "url": "https://mcp.example/fetch", "headers": {"X": "y"}
            }),
        );
        sync_server_to_tool(&paths, &remote, ToolId::OpenCode).unwrap();
        let v: Value = serde_json::from_str(
            &std::fs::read_to_string(
                paths
                    .home
                    .join(".config")
                    .join("opencode")
                    .join("opencode.json"),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(v["mcp"]["api"]["type"], "remote");
        assert_eq!(v["mcp"]["api"]["url"], "https://mcp.example/fetch");
    }

    #[test]
    fn gemini_httpurl_split() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_with(dir.path());
        let http = McpServer::new(
            "h",
            McpServerType::Http,
            serde_json::json!({"url": "https://x/h"}),
        );
        let sse = McpServer::new(
            "s",
            McpServerType::Sse,
            serde_json::json!({"url": "https://x/s"}),
        );
        sync_server_to_tool(&paths, &http, ToolId::GeminiCli).unwrap();
        sync_server_to_tool(&paths, &sse, ToolId::GeminiCli).unwrap();
        let v: Value = serde_json::from_str(
            &std::fs::read_to_string(paths.home.join(".gemini").join("settings.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(v["mcpServers"]["h"]["httpUrl"], "https://x/h");
        assert_eq!(v["mcpServers"]["s"]["url"], "https://x/s");
    }

    #[test]
    fn jsonc_parsing_with_comments() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_with(dir.path());
        std::fs::create_dir_all(&paths.home).unwrap();
        let cfg = paths.home.join(".claude.json");
        std::fs::write(
            &cfg,
            "{\n  // comment\n  \"num\": 1, /* block */\n  \"mcpServers\": {}\n}",
        )
        .unwrap();
        let server = McpServer::new(
            "n",
            McpServerType::Sse,
            serde_json::json!({"url": "https://u"}),
        );
        sync_server_to_tool(&paths, &server, ToolId::ClaudeCode).unwrap();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&cfg).unwrap()).unwrap();
        assert_eq!(v["num"], 1);
        assert_eq!(v["mcpServers"]["n"]["url"], "https://u");
    }

    #[test]
    fn discovery_imports_and_enables() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_with(dir.path());
        std::fs::create_dir_all(&paths.home).unwrap();
        let cfg = paths.home.join(".claude.json");
        std::fs::write(
            &cfg,
            r#"{"mcpServers": {"found": {"type": "http", "url": "https://x/mcp"}}}"#,
        )
        .unwrap();

        let mut store = McpStore::default();
        let (imported, sources) = import_discovered(&paths, &mut store);
        assert_eq!(imported, 1);
        assert_eq!(sources.len(), 1);
        let server = &store.servers[0];
        assert_eq!(server.name, "found");
        assert_eq!(server.server_type, McpServerType::Http);
        assert!(server.enabled_tools.contains(&"claude_code".to_string()));

        // Second scan is idempotent by name.
        let (imported2, _) = import_discovered(&paths, &mut store);
        assert_eq!(imported2, 0);
        assert_eq!(store.servers.len(), 1);
    }

    #[test]
    fn toggle_and_full_sync_flow() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_with(dir.path());
        std::fs::create_dir_all(&paths.home).unwrap();
        let mut store = McpStore::default();
        let mut s = McpServer::new(
            "fs",
            McpServerType::Stdio,
            serde_json::json!({
                "command": "npx", "args": ["-y", "fs"]
            }),
        );
        s.id = "srv".into();
        store.servers.push(s.clone());

        // enable for codex + claude
        assert_eq!(toggle_tool(&mut store, "srv", ToolId::Codex), Some(true));
        assert_eq!(
            toggle_tool(&mut store, "srv", ToolId::ClaudeCode),
            Some(true)
        );
        let report = sync_all_enabled(&paths, &mut store);
        let codex = report.iter().find(|(t, _, _)| *t == ToolId::Codex).unwrap();
        assert_eq!(codex.1, 1, "expected one ok sync");

        // server record carries per-tool sync state
        let server = store.servers.iter().find(|s| s.id == "srv").unwrap();
        assert_eq!(
            server.sync_details.as_ref().unwrap()["codex"]["status"],
            "ok"
        );

        // disabling then syncing removes from the file
        let cfg = paths.home.join(".codex").join("config.toml");
        assert!(
            std::fs::read_to_string(&cfg)
                .unwrap()
                .contains("[mcp_servers.fs]")
        );
        assert_eq!(toggle_tool(&mut store, "srv", ToolId::Codex), Some(false));
        let _ = sync_all_enabled(&paths, &mut store);
        assert!(
            !std::fs::read_to_string(&cfg)
                .unwrap()
                .contains("[mcp_servers.fs]"),
            "disabled server not removed"
        );
    }

    #[test]
    fn test_parse_mcp_servers_from_json() {
        let json = r#"{
            "mcpServers": {
                "sqlite": {
                    "command": "uvx",
                    "args": ["mcp-server-sqlite", "--db-path", "test.db"]
                },
                "github": {
                    "type": "http",
                    "url": "https://api.githubcopilot.com/mcp"
                }
            }
        }"#;
        let servers = parse_mcp_servers_from_json(json).unwrap();
        assert_eq!(servers.len(), 2);
        let sqlite = servers.iter().find(|(name, _, _)| name == "sqlite").unwrap();
        assert_eq!(sqlite.1, McpServerType::Stdio);
        assert_eq!(sqlite.2["command"], "uvx");

        let github = servers.iter().find(|(name, _, _)| name == "github").unwrap();
        assert_eq!(github.1, McpServerType::Http);
        assert_eq!(github.2["url"], "https://api.githubcopilot.com/mcp");
    }
}
