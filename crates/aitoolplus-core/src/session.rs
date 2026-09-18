//! Session history: scan, view, rename, delete, export per tool runtime.
//!
//! Mirrors ai-toolbox `coding/session_manager` semantics:
//! - Sessions come from tool runtime directories, not the DB
//! - `SessionMeta` carries title/project_dir/resume_command; Grok sessions
//!   are directories (`summary.json` + `chat_history.jsonl`) under
//!   `<root>/sessions/<encoded-cwd>/<session-id>/`
//! - Claude Code scans `~/.claude/projects/**/**.jsonl`
//! - export schema: `ai-toolbox.session-export.v2` with providerId
//! - list cache with a 15s TTL, max 16 entries, default limit 200 (max 500)

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::paths::Paths;
use crate::tools::ToolId;

pub const EXPORT_SCHEMA_NAME: &str = "ai-toolbox.session-export.v2";
pub const EXPORT_SCHEMA_VERSION: u8 = 2;
pub const DEFAULT_SESSION_PATH_LIMIT: usize = 200;
pub const MAX_SESSION_PATH_LIMIT: usize = 500;

// ---------------------------------------------------------------------------
// Types (upstream parity, camelCase serialization)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMeta {
    pub provider_id: String,
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_active_at: Option<i64>,
    pub source_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_command: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMessage {
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ts: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_type: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<SessionMessageBlock>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMessageBlock {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

// ---------------------------------------------------------------------------
// Directory layout per tool
// ---------------------------------------------------------------------------

pub fn session_root(paths: &Paths, tool: ToolId) -> Option<PathBuf> {
    let home = &paths.home;
    Some(match tool {
        ToolId::ClaudeCode => home.join(".claude").join("projects"),
        ToolId::Codex => home.join(".codex").join("sessions"),
        ToolId::GeminiCli => home.join(".gemini").join("tmp"),
        ToolId::Grok => home.join(".grok").join("sessions"),
        ToolId::Kimi => home.join(".kimi-code").join("sessions"),
        ToolId::OpenCode => home
            .join(".local")
            .join("share")
            .join("opencode")
            .join("session"),
        ToolId::OpenClaw => home.join(".openclaw").join("sessions"),
        ToolId::Pi => home.join(".pi").join("agent").join("sessions"),
        ToolId::OhMyPi => home.join(".config").join("oh-my-pi").join("sessions"),
        ToolId::Hermes => home.join(".hermes").join("sessions"),
        ToolId::Dsh => home.join(".dsh").join("sessions"),
        ToolId::ClaudeDesktop => return None,
    })
}

// ---------------------------------------------------------------------------
// Scan (list) per tool format
// ---------------------------------------------------------------------------

pub fn scan_sessions(paths: &Paths, tool: ToolId, limit: usize) -> Vec<SessionMeta> {
    let limit = limit.clamp(1, MAX_SESSION_PATH_LIMIT);
    let Some(root) = session_root(paths, tool) else {
        return vec![];
    };
    if !root.is_dir() {
        return vec![];
    }
    let mut sessions = match tool {
        ToolId::Grok => scan_grok(&root),
        ToolId::ClaudeCode => scan_claude_code(&root),
        ToolId::Kimi => scan_kimi_dirs(&root),
        _ => scan_jsonl_generic(&root, tool),
    };
    sessions.sort_by_key(|s| std::cmp::Reverse(s.last_active_at));
    sessions.truncate(limit);
    sessions
}

/// Grok: `<root>/<encoded-cwd>/<session-id>/summary.json`.
fn scan_grok(root: &Path) -> Vec<SessionMeta> {
    let mut out = vec![];
    let Ok(cwd_dirs) = std::fs::read_dir(root) else {
        return out;
    };
    for cwd_dir in cwd_dirs.filter_map(|e| e.ok()) {
        let cwd_path = cwd_dir.path();
        if !cwd_path.is_dir() {
            continue;
        }
        let Ok(session_dirs) = std::fs::read_dir(&cwd_path) else {
            continue;
        };
        for session_dir in session_dirs.filter_map(|e| e.ok()) {
            let dir = session_dir.path();
            let summary = dir.join("summary.json");
            if !summary.is_file() {
                continue;
            }
            if let Some(meta) = parse_grok_summary(&dir, &summary) {
                out.push(meta);
            }
        }
    }
    out
}

fn parse_grok_summary(session_dir: &Path, summary: &Path) -> Option<SessionMeta> {
    let raw = std::fs::read_to_string(summary).ok()?;
    let value: Value = serde_json::from_str(&raw).ok()?;
    let session_id = value
        .pointer("/info/id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            session_dir
                .file_name()
                .and_then(|n| n.to_str())
                .map(String::from)
        })?;
    let project_dir = value
        .pointer("/info/cwd")
        .and_then(Value::as_str)
        .map(str::to_string);
    Some(SessionMeta {
        provider_id: "grok".into(),
        session_id: session_id.clone(),
        title: value
            .get("generated_title")
            .and_then(Value::as_str)
            .map(str::to_string),
        summary: value
            .get("session_summary")
            .and_then(Value::as_str)
            .map(str::to_string),
        project_dir: project_dir.clone(),
        created_at: value
            .get("created_at")
            .and_then(Value::as_str)
            .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok())
            .map(|v| v.timestamp_millis()),
        last_active_at: value
            .get("last_active_at")
            .or_else(|| value.get("updated_at"))
            .and_then(Value::as_str)
            .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok())
            .map(|v| v.timestamp_millis()),
        source_path: session_dir.to_string_lossy().to_string(),
        resume_command: Some(format!("grok --resume {session_id}")),
    })
}

/// Claude Code: `~/.claude/projects/<encoded>/*.jsonl` (+ optional index).
fn scan_claude_code(root: &Path) -> Vec<SessionMeta> {
    scan_jsonl_generic(root, ToolId::ClaudeCode)
}

/// Kimi: session directories with metadata; fall back to jsonl scan.
fn scan_kimi_dirs(root: &Path) -> Vec<SessionMeta> {
    scan_jsonl_generic(root, ToolId::Kimi)
}

/// Generic JSONL scan for tools whose sessions are line-delimited JSON
/// files (codex/pi/omp/gemini/opencode/openclaw/hermes/dsh).
fn scan_jsonl_generic(root: &Path, tool: ToolId) -> Vec<SessionMeta> {
    let mut files = vec![];
    collect_jsonl(root, &mut files);
    files.sort_by_key(|p| std::cmp::Reverse(modified_ms(p)));
    let provider = tool.key().to_string();
    files
        .into_iter()
        .filter_map(|path| parse_jsonl_meta(&path, &provider, tool))
        .collect()
}

fn collect_jsonl(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            collect_jsonl(&path, out);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("jsonl"))
        {
            out.push(path);
        }
    }
}

fn modified_ms(path: &Path) -> i64 {
    path.metadata()
        .and_then(|m| m.modified())
        .map(|t| {
            let dt: chrono::DateTime<chrono::Local> = t.into();
            dt.timestamp_millis()
        })
        .unwrap_or(0)
}

fn parse_jsonl_meta(path: &Path, provider: &str, tool: ToolId) -> Option<SessionMeta> {
    // Read only head+tail lines to build cheap metadata (upstream style).
    let raw = std::fs::read_to_string(path).ok()?;
    let lines: Vec<&str> = raw.lines().collect();
    if lines.is_empty() {
        return None;
    }

    let mut first_user: Option<String> = None;
    let mut last_ts: Option<i64> = None;
    let mut first_ts: Option<i64> = None;
    let mut message_count = 0usize;

    let inspect = |line: &str| -> Option<()> {
        let value: Value = serde_json::from_str(line).ok()?;
        let role = value
            .get("type")
            .and_then(Value::as_str)
            .or_else(|| value.get("role").and_then(Value::as_str))?;
        if role == "user" || role == "assistant" {
            message_count += 1;
        }
        if first_user.is_none() && role == "user" {
            first_user = Some(extract_text(&value).chars().take(120).collect());
        }
        if let Some(ts) = value
            .get("timestamp")
            .or_else(|| value.get("ts"))
            .or_else(|| value.get("created_at"))
            .and_then(Value::as_i64)
        {
            if first_ts.is_none() {
                first_ts = Some(ts);
            }
            last_ts = Some(ts);
        }
        Some(())
    };

    // head 40 + tail 20 lines
    let head = lines.iter().take(40);
    let tail_start = lines.len().saturating_sub(20);
    let tail = lines[tail_start.min(lines.len())..].iter();
    let mut inspect = inspect;
    let _ = &mut inspect;
    for line in head.chain(tail) {
        inspect(line);
    }

    if message_count == 0 {
        return None;
    }

    let session_id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("session")
        .to_string();
    let resume = resume_command(tool, &session_id);
    Some(SessionMeta {
        provider_id: provider.to_string(),
        session_id: session_id.clone(),
        title: first_user.clone(),
        summary: first_user,
        project_dir: None,
        created_at: first_ts,
        last_active_at: last_ts.or(Some(modified_ms(path))),
        source_path: path.to_string_lossy().to_string(),
        resume_command: resume,
    })
}

fn resume_command(tool: ToolId, session_id: &str) -> Option<String> {
    Some(match tool {
        ToolId::ClaudeCode => format!("claude --resume {session_id}"),
        ToolId::Codex => format!("codex resume {session_id}"),
        ToolId::GeminiCli => return None,
        ToolId::Grok => format!("grok --resume {session_id}"),
        ToolId::Kimi => format!("kimi --resume {session_id}"),
        ToolId::OpenCode => format!("opencode --continue {session_id}"),
        ToolId::OpenClaw => format!("openclaw resume {session_id}"),
        ToolId::Pi => format!("pi --continue {session_id}"),
        ToolId::OhMyPi => format!("omp --continue {session_id}"),
        ToolId::Hermes => return None,
        ToolId::Dsh => return None,
        ToolId::ClaudeDesktop => return None,
    })
}

// ---------------------------------------------------------------------------
// Message loading
// ---------------------------------------------------------------------------

pub fn load_messages(_paths: &Paths, meta: &SessionMeta) -> Result<Vec<SessionMessage>, String> {
    let source = PathBuf::from(&meta.source_path);
    if source.is_dir() {
        // Grok-style directory sessions.
        let history = source.join("chat_history.jsonl");
        if !history.is_file() {
            return Ok(vec![]);
        }
        return load_jsonl_messages(&history);
    }
    load_jsonl_messages(&source)
}

fn load_jsonl_messages(path: &Path) -> Result<Vec<SessionMessage>, String> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    let mut out = vec![];
    for line in raw.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(role) = value
            .get("type")
            .and_then(Value::as_str)
            .or_else(|| value.get("role").and_then(Value::as_str))
        else {
            continue;
        };
        let content = extract_text(&value);
        let ts = value
            .get("timestamp")
            .or_else(|| value.get("ts"))
            .or_else(|| value.get("created_at"))
            .and_then(Value::as_i64);
        out.push(SessionMessage {
            role: role.to_string(),
            content,
            ts,
            id: value.get("id").and_then(Value::as_str).map(String::from),
            message_type: value.get("type").and_then(Value::as_str).map(String::from),
            blocks: vec![],
            model: value
                .get("model")
                .or_else(|| value.pointer("/message/model"))
                .and_then(Value::as_str)
                .map(String::from),
        });
    }
    Ok(out)
}

/// Extract human-readable text from a message envelope (upstream
/// `utils::extract_text` semantics: string, text parts, tool results).
pub fn extract_text(v: &Value) -> String {
    if let Some(msg) = v.get("message")
        && let Some(content) = msg.get("content")
    {
        return content_text(content);
    }
    if let Some(content) = v.get("content") {
        return content_text(content);
    }
    if let Some(text) = v.get("text").and_then(Value::as_str) {
        return text.to_string();
    }
    if let Some(text) = v.pointer("/tool_result/content") {
        return content_text(text);
    }
    String::new()
}

fn content_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(|i| {
                i.get("text")
                    .and_then(Value::as_str)
                    .or_else(|| i.get("content").and_then(Value::as_str))
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Object(obj) => obj
            .get("text")
            .and_then(Value::as_str)
            .map(String::from)
            .unwrap_or_default(),
        _ => String::new(),
    }
}

// ---------------------------------------------------------------------------
// Mutations: delete, rename (via title metadata file), export
// ---------------------------------------------------------------------------

/// Delete the session (file or whole directory for Grok-style sessions).
pub fn delete_session(meta: &SessionMeta) -> Result<(), String> {
    let path = PathBuf::from(&meta.source_path);
    if path.is_dir() {
        std::fs::remove_dir_all(&path).map_err(|e| e.to_string())
    } else if path.is_file() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())
    } else {
        Ok(())
    }
}

/// Rename: write a `title` into the sidecar `<file>.title` (toolbox-owned
/// metadata that doesn't disturb tool runtimes); the list scan merges it.
pub fn rename_session(meta: &SessionMeta, title: &str) -> Result<(), String> {
    let path = PathBuf::from(&meta.source_path);
    let sidecar = if path.is_dir() {
        path.join("toolbox-title.txt")
    } else {
        path.with_extension("toolbox-title.txt")
    };
    std::fs::write(&sidecar, title).map_err(|e| e.to_string())
}

/// Read the toolbox-owned title sidecar if present.
pub fn sidecar_title(meta: &SessionMeta) -> Option<String> {
    let path = PathBuf::from(&meta.source_path);
    let sidecar = if path.is_dir() {
        path.join("toolbox-title.txt")
    } else {
        path.with_extension("toolbox-title.txt")
    };
    std::fs::read_to_string(sidecar)
        .ok()
        .filter(|s| !s.is_empty())
}

/// Export one session as the v2 schema document.
pub fn export_session(paths: &Paths, meta: &SessionMeta) -> Result<String, String> {
    let messages = load_messages(paths, meta)?;
    let doc = serde_json::json!({
        "schema": EXPORT_SCHEMA_NAME,
        "version": EXPORT_SCHEMA_VERSION,
        "providerId": meta.provider_id,
        "sessionId": meta.session_id,
        "title": meta.title,
        "sourcePath": meta.source_path,
        "messages": messages,
    });
    serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())
}

/// Export one session as clean formatted Markdown.
pub fn export_session_markdown(paths: &Paths, meta: &SessionMeta) -> Result<String, String> {
    let messages = load_messages(paths, meta)?;
    let mut md = String::new();
    let title = meta.title.as_deref().unwrap_or(&meta.session_id);
    md.push_str(&format!("# 会话记录: {title}\n\n"));
    md.push_str(&format!("- **工具**: {}\n", meta.provider_id));
    md.push_str(&format!("- **会话 ID**: `{}`\n", meta.session_id));
    if let Some(dir) = &meta.project_dir {
        md.push_str(&format!("- **项目路径**: `{dir}`\n"));
    }
    if let Some(summary) = &meta.summary {
        md.push_str(&format!("- **摘要**: {summary}\n"));
    }
    md.push_str("\n---\n\n");

    for m in messages {
        let role_title = match m.role.as_str() {
            "user" => "👤 用户 (User)",
            "assistant" => "🤖 助手 (Assistant)",
            "system" => "⚙️ 系统 (System)",
            _ => "💬 消息 (Message)",
        };
        md.push_str(&format!("### {role_title}\n\n"));
        if !m.content.trim().is_empty() {
            md.push_str(m.content.trim());
            md.push_str("\n\n");
        }
        for b in &m.blocks {
            if b.kind == "thinking" {
                if let Some(txt) = &b.text {
                    let trimmed = txt.trim();
                    if !trimmed.is_empty() {
                        md.push_str("> 🧠 **思考过程**:\n");
                        for line in trimmed.lines() {
                            md.push_str(&format!("> {line}\n"));
                        }
                        md.push('\n');
                    }
                }
            } else if b.kind == "tool_call" {
                let name = b.tool_name.as_deref().or(b.title.as_deref()).unwrap_or("tool");
                md.push_str(&format!("🔧 **工具调用**: `{name}`\n\n"));
                if let Some(txt) = &b.text {
                    md.push_str("```json\n");
                    md.push_str(txt);
                    md.push_str("\n```\n\n");
                }
            }
        }
        md.push_str("---\n\n");
    }

    Ok(md)
}


/// Import an exported v2 session back into a tool's session directory as a
/// plain JSONL file (best effort; runtimes own their real resume state).
pub fn import_session(paths: &Paths, tool: ToolId, doc: &str) -> Result<String, String> {
    let value: Value = serde_json::from_str(doc).map_err(|e| format!("invalid export: {e}"))?;
    if value.get("schema") != Some(&Value::String(EXPORT_SCHEMA_NAME.into())) {
        return Err("unsupported export schema".into());
    }
    let Some(root) = session_root(paths, tool) else {
        return Err(format!("{} has no session directory", tool.name_en()));
    };
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;

    let session_id = value
        .get("sessionId")
        .and_then(Value::as_str)
        .unwrap_or("imported");
    let stamp = chrono::Local::now().format("%Y%m%d%H%M%S%3f");
    let out = root.join(format!("{stamp}-{session_id}.jsonl"));
    let messages = value
        .get("messages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut jsonl = String::new();
    for m in &messages {
        jsonl.push_str(&m.to_string());
        jsonl.push('\n');
    }
    std::fs::write(&out, jsonl).map_err(|e| e.to_string())?;
    Ok(out.to_string_lossy().to_string())
}

// ---------------------------------------------------------------------------
// List cache (15s TTL, 16 entries) — mirrors upstream SESSION_LIST_CACHE
// ---------------------------------------------------------------------------

struct CacheEntry {
    created: std::time::Instant,
    sessions: Vec<SessionMeta>,
}

static CACHE: std::sync::Mutex<Option<HashMap<String, CacheEntry>>> = std::sync::Mutex::new(None);

pub fn cached_scan(paths: &Paths, tool: ToolId, limit: usize) -> Vec<SessionMeta> {
    let key = format!("{}:{limit}", tool.key());
    if let Ok(guard) = CACHE.lock()
        && let Some(map) = guard.as_ref()
        && let Some(entry) = map.get(&key)
        && entry.created.elapsed().as_secs() < 15
    {
        return entry.sessions.clone();
    }
    let sessions = scan_sessions(paths, tool, limit);
    if let Ok(mut guard) = CACHE.lock() {
        let map = guard.get_or_insert_with(HashMap::new);
        map.insert(
            key,
            CacheEntry {
                created: std::time::Instant::now(),
                sessions: sessions.clone(),
            },
        );
        while map.len() > 16 {
            // evict oldest by creation
            if let Some(oldest) = map
                .iter()
                .min_by_key(|(_, e)| e.created)
                .map(|(k, _)| k.clone())
            {
                map.remove(&oldest);
            }
        }
    }
    sessions
}

/// Invalidate the cache (after delete/rename/import).
pub fn invalidate_cache() {
    if let Ok(mut guard) = CACHE.lock() {
        *guard = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths_with(root: &Path) -> Paths {
        Paths::new(root.join("home"), root.join("data"))
    }

    #[test]
    fn grok_directory_sessions_discovered() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_with(dir.path());
        let root = paths.home.join(".grok").join("sessions");
        let session = root
            .join("home-user-proj")
            .join("sess-42")
            .join("summary.json");
        std::fs::create_dir_all(session.parent().unwrap()).unwrap();
        std::fs::write(
            &session,
            r#"{
                "info": {"id": "sess-42", "cwd": "/home/user/proj"},
                "generated_title": "Fix bug",
                "session_summary": "Fixed the parser",
                "created_at": "2026-09-01T10:00:00Z",
                "last_active_at": "2026-09-02T11:30:00Z"
            }"#,
        )
        .unwrap();
        std::fs::write(
            session.parent().unwrap().join("chat_history.jsonl"),
            "{\"type\":\"user\",\"message\":{\"content\":\"hi\"}}\n",
        )
        .unwrap();

        let sessions = cached_scan(&paths, ToolId::Grok, 50);
        assert_eq!(sessions.len(), 1);
        let s = &sessions[0];
        assert_eq!(s.session_id, "sess-42");
        assert_eq!(s.title.as_deref(), Some("Fix bug"));
        assert_eq!(s.project_dir.as_deref(), Some("/home/user/proj"));
        assert_eq!(s.resume_command.as_deref(), Some("grok --resume sess-42"));
        assert!(s.source_path.ends_with("sess-42"));

        // messages from the directory
        let msgs = load_messages(&paths, s).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content, "hi");
    }

    #[test]
    fn claude_jsonl_sessions_with_timestamps() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_with(dir.path());
        let proj = paths
            .home
            .join(".claude")
            .join("projects")
            .join("home-user-x");
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::write(
            proj.join("abc.jsonl"),
            concat!(
                "{\"type\":\"user\",\"message\":{\"content\":\"first question\"},\"timestamp\":1700000000000}\n",
                "{\"type\":\"assistant\",\"message\":{\"content\":\"answer\"},\"timestamp\":1700000001000}\n",
                "{\"type\":\"user\",\"message\":{\"content\":\"second question\"},\"timestamp\":1700000002000}\n",
            ),
        )
        .unwrap();

        let sessions = cached_scan(&paths, ToolId::ClaudeCode, 50);
        assert_eq!(sessions.len(), 1);
        let s = &sessions[0];
        assert_eq!(s.session_id, "abc");
        assert_eq!(s.title.as_deref(), Some("first question"));
        assert_eq!(s.created_at, Some(1700000000000));
        assert_eq!(s.last_active_at, Some(1700000002000));
        assert_eq!(s.resume_command.as_deref(), Some("claude --resume abc"));

        invalidate_cache();
        let msgs = load_messages(&paths, s).unwrap();
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[2].content, "second question");
    }

    #[test]
    fn rename_delete_and_cache_invalidation() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_with(dir.path());
        let proj = paths.home.join(".claude").join("projects").join("p");
        std::fs::create_dir_all(&proj).unwrap();
        let file = proj.join("s1.jsonl");
        std::fs::write(
            &file,
            "{\"type\":\"user\",\"message\":{\"content\":\"q\"}}\n",
        )
        .unwrap();

        let sessions = cached_scan(&paths, ToolId::ClaudeCode, 50);
        assert_eq!(sessions.len(), 1);

        rename_session(&sessions[0], "My Title").unwrap();
        assert_eq!(sidecar_title(&sessions[0]).as_deref(), Some("My Title"));

        invalidate_cache();
        delete_session(&sessions[0]).unwrap();
        assert!(!file.exists());
    }

    #[test]
    fn export_import_roundtrip_v2() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_with(dir.path());
        let proj = paths.home.join(".codex").join("sessions");
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::write(
            proj.join("xyz.jsonl"),
            "{\"type\":\"user\",\"content\":\"hello there\"}\n{\"type\":\"assistant\",\"content\":\"general kenobi\"}\n",
        )
        .unwrap();

        let sessions = cached_scan(&paths, ToolId::Codex, 50);
        assert_eq!(sessions.len(), 1);
        let export = export_session(&paths, &sessions[0]).unwrap();
        let value: Value = serde_json::from_str(&export).unwrap();
        assert_eq!(value["schema"], "ai-toolbox.session-export.v2");
        assert_eq!(value["providerId"], "codex");
        assert_eq!(value["messages"][1]["content"], "general kenobi");

        invalidate_cache();
        let imported = import_session(&paths, ToolId::Codex, &export).unwrap();
        assert!(imported.ends_with(".jsonl"));
        assert!(std::path::Path::new(&imported).exists());
    }

    #[test]
    fn limits_and_empty_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_with(dir.path());
        // no session dirs exist -> empty
        assert!(cached_scan(&paths, ToolId::Pi, 50).is_empty());
        assert!(scan_sessions(&paths, ToolId::ClaudeDesktop, 50).is_empty());

        // limit respected
        let proj = paths.home.join(".claude").join("projects").join("p");
        std::fs::create_dir_all(&proj).unwrap();
        for i in 0..5 {
            std::fs::write(
                proj.join(format!("s{i}.jsonl")),
                format!("{{\"type\":\"user\",\"message\":{{\"content\":\"q{i}\"}}}}\n"),
            )
            .unwrap();
        }
        let sessions = scan_sessions(&paths, ToolId::ClaudeCode, 3);
        assert_eq!(sessions.len(), 3);
    }
}
