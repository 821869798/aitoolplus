//! Session history: scan, view, rename, delete, export per tool runtime.
//!
//! Mirrors ai-toolbox `coding/session_manager` semantics:
//! - Sessions come from tool runtime directories, not the DB
//! - `SessionMeta` carries title/project_dir/resume_command; Grok sessions
//!   are directories (`summary.json` + `chat_history.jsonl`) under
//!   `<root>/sessions/<encoded-cwd>/<session-id>/`
//! - Claude Code scans `~/.claude/projects/**/**.jsonl`
//! - export schema: `ai-toolbox.session-export.v2` with providerId
//! - list cache with a 15s TTL, max 16 entries, default limit 200 (full list has no cap)

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use rusqlite::Connection;
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}

// ---------------------------------------------------------------------------
// Directory layout per tool
// ---------------------------------------------------------------------------

fn is_default_home(home: &Path) -> bool {
    let real_home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from);
    real_home.as_deref() == Some(home)
}

fn push_unique_dir(out: &mut Vec<PathBuf>, dir: PathBuf) {
    if !out.iter().any(|existing| existing == &dir) {
        out.push(dir);
    }
}

fn kimi_session_roots(paths: &Paths) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if is_default_home(&paths.home)
        && let Some(share_dir) = std::env::var_os("KIMI_SHARE_DIR")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    {
        push_unique_dir(&mut roots, share_dir.join("sessions"));
    }
    push_unique_dir(&mut roots, paths.home.join(".kimi").join("sessions"));
    push_unique_dir(&mut roots, paths.tool_root(ToolId::Kimi).join("sessions"));
    roots
}

fn opencode_data_roots(paths: &Paths) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if is_default_home(&paths.home) {
        for var in ["XDG_DATA_HOME", "LOCALAPPDATA", "APPDATA"] {
            if let Some(base) = std::env::var_os(var).filter(|v| !v.is_empty()).map(PathBuf::from) {
                push_unique_dir(&mut roots, base.join("opencode"));
            }
        }
    }
    push_unique_dir(
        &mut roots,
        paths.home.join(".local").join("share").join("opencode"),
    );
    push_unique_dir(
        &mut roots,
        paths
            .home
            .join("AppData")
            .join("Local")
            .join("opencode"),
    );
    push_unique_dir(&mut roots, paths.tool_root(ToolId::OpenCode));
    roots
}

fn claude_desktop_session_roots(paths: &Paths) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if is_default_home(&paths.home) {
        if let Some(root) = std::env::var_os("CLAUDE_DESKTOP_ROOT")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
        {
            push_unique_dir(&mut roots, root.join("local-agent-mode-sessions"));
        }
        for var in ["APPDATA", "LOCALAPPDATA"] {
            if let Some(base) = std::env::var_os(var).filter(|v| !v.is_empty()).map(PathBuf::from) {
                push_unique_dir(
                    &mut roots,
                    base.join("Claude").join("local-agent-mode-sessions"),
                );
            }
        }
    }
    for base in [
        paths.home.join("AppData").join("Roaming").join("Claude"),
        paths.home.join("AppData").join("Local").join("Claude"),
        paths
            .home
            .join("Library")
            .join("Application Support")
            .join("Claude"),
        paths.home.join(".config").join("Claude"),
    ] {
        push_unique_dir(&mut roots, base.join("local-agent-mode-sessions"));
    }
    roots
}

pub fn session_root(paths: &Paths, tool: ToolId) -> Option<PathBuf> {
    let home = &paths.home;
    Some(match tool {
        ToolId::ClaudeCode => home.join(".claude").join("projects"),
        ToolId::Codex => home.join(".codex").join("sessions"),
        ToolId::GeminiCli => home.join(".gemini").join("tmp"),
        ToolId::Grok => home.join(".grok").join("sessions"),
        ToolId::Kimi => {
            let roots = kimi_session_roots(paths);
            roots
                .iter()
                .find(|r| r.is_dir())
                .cloned()
                .unwrap_or_else(|| paths.tool_root(ToolId::Kimi).join("sessions"))
        }
        ToolId::OpenCode => {
            let roots = opencode_data_roots(paths);
            roots
                .iter()
                .find(|r| r.is_dir())
                .cloned()
                .unwrap_or_else(|| home.join(".local").join("share").join("opencode"))
        }
        ToolId::OpenClaw => {
            let agents_dir = paths.tool_root(ToolId::OpenClaw).join("agents");
            if agents_dir.is_dir() {
                agents_dir
            } else {
                let legacy = paths.tool_root(ToolId::OpenClaw).join("sessions");
                if legacy.is_dir() { legacy } else { agents_dir }
            }
        }
        ToolId::Pi => paths.tool_root(ToolId::Pi).join("sessions"),
        ToolId::OhMyPi => paths.tool_root(ToolId::OhMyPi).join("sessions"),
        ToolId::Hermes => paths.tool_root(ToolId::Hermes),
        ToolId::Dsh => paths.tool_root(ToolId::Dsh).join("sessions"),
        ToolId::ClaudeDesktop => {
            let roots = claude_desktop_session_roots(paths);
            roots.iter().find(|r| r.is_dir()).cloned()?
        }
        ToolId::Agents => return None,
    })
}

// ---------------------------------------------------------------------------
// Scan (list) per tool format
// ---------------------------------------------------------------------------

pub fn scan_sessions(paths: &Paths, tool: ToolId, limit: usize) -> Vec<SessionMeta> {
    let limit = limit.clamp(1, MAX_SESSION_PATH_LIMIT);
    let mut sessions = scan_sessions_with_limit(paths, tool, limit);
    sessions.sort_by_key(|s| std::cmp::Reverse(s.last_active_at));
    sessions.truncate(limit);
    sessions
}

/// Every session on disk for this tool. No count cap. Same metadata-only scan
/// ai-toolbox uses for its full list.
pub fn scan_all_sessions(paths: &Paths, tool: ToolId) -> Vec<SessionMeta> {
    let mut sessions = scan_sessions_with_limit(paths, tool, usize::MAX);
    sessions.sort_by_key(|s| std::cmp::Reverse(s.last_active_at));
    sessions
}

fn scan_sessions_with_limit(paths: &Paths, tool: ToolId, limit: usize) -> Vec<SessionMeta> {
    match tool {
        ToolId::Grok => session_root(paths, tool)
            .filter(|r| r.is_dir())
            .map(|r| scan_grok(&r))
            .unwrap_or_default(),
        ToolId::Codex => session_root(paths, tool)
            .filter(|r| r.is_dir())
            .map(|r| scan_codex(&r, limit))
            .unwrap_or_default(),
        ToolId::ClaudeCode => session_root(paths, tool)
            .filter(|r| r.is_dir())
            .map(|r| scan_claude_code(&r, limit))
            .unwrap_or_default(),
        ToolId::GeminiCli => session_root(paths, tool)
            .filter(|r| r.is_dir())
            .map(|r| scan_gemini_cli(&r, limit))
            .unwrap_or_default(),
        ToolId::Kimi => scan_kimi_roots(&kimi_session_roots(paths), limit),
        ToolId::OpenCode => scan_opencode(&opencode_data_roots(paths), limit),
        ToolId::OpenClaw => scan_openclaw(paths, limit),
        ToolId::Hermes => scan_hermes(&paths.tool_root(ToolId::Hermes), limit),
        ToolId::Dsh => session_root(paths, tool)
            .filter(|r| r.is_dir())
            .map(|r| scan_dsh(&r, limit))
            .unwrap_or_default(),
        ToolId::ClaudeDesktop => scan_claude_desktop(&claude_desktop_session_roots(paths), limit),
        ToolId::Agents => Vec::new(),
        ToolId::Pi | ToolId::OhMyPi => session_root(paths, tool)
            .filter(|r| r.is_dir())
            .map(|r| scan_jsonl_generic(&r, tool, limit))
            .unwrap_or_default(),
    }
}

/// Directory listing only. Call [`SessionScan::next_batch`] to parse metadata
/// in chunks so a large history can show up before the last file is read.
pub struct SessionScan {
    kind: ScanKind,
    cursor: usize,
}

enum ScanKind {
    Jsonl {
        files: Vec<PathBuf>,
        names: HashMap<String, String>,
        provider: String,
        tool: ToolId,
    },
    Grok {
        dirs: Vec<PathBuf>,
    },
    Ready {
        sessions: Vec<SessionMeta>,
    },
    Empty,
}

impl SessionScan {
    pub fn open(paths: &Paths, tool: ToolId) -> Self {
        match tool {
            ToolId::Grok => {
                let Some(root) = session_root(paths, tool).filter(|r| r.is_dir()) else {
                    return Self { kind: ScanKind::Empty, cursor: 0 };
                };
                Self {
                    kind: ScanKind::Grok { dirs: collect_grok_dirs(&root) },
                    cursor: 0,
                }
            }
            ToolId::ClaudeCode | ToolId::Codex | ToolId::Pi | ToolId::OhMyPi => {
                let Some(root) = session_root(paths, tool).filter(|r| r.is_dir()) else {
                    return Self { kind: ScanKind::Empty, cursor: 0 };
                };
                let mut files = Vec::new();
                collect_jsonl(&root, &mut files);
                files.sort_by_key(|path| std::cmp::Reverse(modified_ms(path)));
                let names = if tool == ToolId::Codex {
                    read_codex_thread_names(&root)
                } else {
                    HashMap::new()
                };
                Self {
                    kind: ScanKind::Jsonl {
                        files,
                        names,
                        provider: tool.key().to_string(),
                        tool,
                    },
                    cursor: 0,
                }
            }
            _ => {
                let mut sessions = scan_sessions_with_limit(paths, tool, usize::MAX);
                sessions.sort_by_key(|s| std::cmp::Reverse(s.last_active_at));
                Self {
                    kind: ScanKind::Ready { sessions },
                    cursor: 0,
                }
            }
        }
    }

    fn len(&self) -> usize {
        match &self.kind {
            ScanKind::Jsonl { files, .. } => files.len(),
            ScanKind::Grok { dirs } => dirs.len(),
            ScanKind::Ready { sessions } => sessions.len(),
            ScanKind::Empty => 0,
        }
    }

    /// `None` when the listing is exhausted. An empty `Vec` means this chunk
    /// had no readable sessions; keep calling.
    pub fn next_batch(&mut self, size: usize) -> Option<Vec<SessionMeta>> {
        let size = size.max(1);
        if self.cursor >= self.len() {
            return None;
        }
        let end = (self.cursor + size).min(self.len());
        let batch = match &self.kind {
            ScanKind::Empty => Vec::new(),
            ScanKind::Ready { sessions } => sessions[self.cursor..end].to_vec(),
            ScanKind::Grok { dirs } => dirs[self.cursor..end]
                .iter()
                .filter_map(|dir| parse_grok_summary(dir, &dir.join("summary.json")))
                .collect(),
            ScanKind::Jsonl { files, names, provider, tool } => files[self.cursor..end]
                .iter()
                .filter_map(|path| {
                    let mut meta = parse_jsonl_meta(path, provider, *tool)?;
                    if let Some(name) = names.get(&meta.session_id) {
                        meta.title = Some(name.clone());
                    }
                    Some(meta)
                })
                .collect(),
        };
        self.cursor = end;
        Some(batch)
    }
}

fn collect_grok_dirs(root: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let Ok(cwd_dirs) = std::fs::read_dir(root) else {
        return dirs;
    };
    for cwd_dir in cwd_dirs.filter_map(|entry| entry.ok()) {
        let cwd_path = cwd_dir.path();
        if !cwd_path.is_dir() {
            continue;
        }
        let Ok(session_dirs) = std::fs::read_dir(&cwd_path) else {
            continue;
        };
        for session_dir in session_dirs.filter_map(|entry| entry.ok()) {
            let dir = session_dir.path();
            if dir.join("summary.json").is_file() {
                dirs.push(dir);
            }
        }
    }
    dirs.sort_by_key(|dir| std::cmp::Reverse(modified_ms(&dir.join("summary.json"))));
    dirs
}

/// Codex: sessions in `~/.codex/sessions/**/*.jsonl` with optional thread_names
/// from `~/.codex/session_index.jsonl`.
fn scan_codex(root: &Path, limit: usize) -> Vec<SessionMeta> {
    let thread_names = read_codex_thread_names(root);
    let mut sessions = scan_jsonl_generic(root, ToolId::Codex, limit);
    for session in &mut sessions {
        if let Some(thread_name) = thread_names.get(&session.session_id) {
            session.title = Some(thread_name.clone());
        }
    }
    sessions
}

fn read_codex_thread_names(root: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let index_file = root
        .parent()
        .map(|p| p.join("session_index.jsonl"))
        .unwrap_or_else(|| root.join("session_index.jsonl"));
    let Ok(content) = std::fs::read_to_string(&index_file) else {
        return map;
    };
    for line in content.lines() {
        let Ok(val) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let id = val.get("id").and_then(Value::as_str).unwrap_or("").trim();
        let name = val
            .get("thread_name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        if !id.is_empty() && !name.is_empty() {
            map.insert(id.to_string(), name.to_string());
        }
    }
    map
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
fn scan_claude_code(root: &Path, limit: usize) -> Vec<SessionMeta> {
    scan_jsonl_generic(root, ToolId::ClaudeCode, limit)
}

/// Kimi: scan `~/.kimi/sessions`, `$KIMI_SHARE_DIR/sessions`, and `~/.kimi-code/sessions`
/// (`state.json` / `summary.json` metadata + `chat_history.jsonl` / `wire.jsonl`).
fn scan_kimi_roots(roots: &[PathBuf], limit: usize) -> Vec<SessionMeta> {
    let mut dirs = Vec::new();
    for root in roots {
        if root.is_dir() {
            collect_kimi_session_dirs(root, 0, &mut dirs);
        }
    }
    let mut by_id: HashMap<String, SessionMeta> = HashMap::new();
    for dir in dirs {
        if let Some(meta) = parse_kimi_session_dir(&dir) {
            let replace = by_id
                .get(&meta.session_id)
                .is_none_or(|old| meta.last_active_at >= old.last_active_at);
            if replace {
                by_id.insert(meta.session_id.clone(), meta);
            }
        }
    }
    let mut out: Vec<SessionMeta> = by_id.into_values().collect();
    out.sort_by_key(|s| std::cmp::Reverse(s.last_active_at));
    if limit != usize::MAX {
        out.truncate(limit);
    }
    out
}

fn collect_kimi_session_dirs(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let markers = [
        "state.json",
        "summary.json",
        "chat_history.jsonl",
        "messages.jsonl",
        "history.jsonl",
        "wire.jsonl",
    ];
    if markers.iter().any(|name| dir.join(name).is_file()) {
        out.push(dir.to_path_buf());
        return;
    }
    if depth >= 3 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            collect_kimi_session_dirs(&path, depth + 1, out);
        }
    }
}

fn kimi_transcript_in_dir(dir: &Path) -> Option<PathBuf> {
    for name in ["chat_history.jsonl", "messages.jsonl", "history.jsonl", "wire.jsonl"] {
        let p = dir.join(name);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

fn parse_kimi_session_dir(dir: &Path) -> Option<SessionMeta> {
    let transcript = kimi_transcript_in_dir(dir);
    let jsonl_meta = transcript
        .as_deref()
        .and_then(|p| parse_jsonl_meta(p, "kimi", ToolId::Kimi));
    let meta_file = ["state.json", "summary.json"]
        .into_iter()
        .map(|name| dir.join(name))
        .find(|p| p.is_file());
    let mut meta_val: Option<Value> = None;
    if let Some(mf) = meta_file.as_deref()
        && let Ok(raw) = std::fs::read_to_string(mf)
    {
        meta_val = serde_json::from_str::<Value>(&raw).ok();
    }

    if let Some(val) = meta_val {
        let session_id = val
            .get("session_id")
            .or_else(|| val.get("id"))
            .or_else(|| val.pointer("/info/id"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .or_else(|| jsonl_meta.as_ref().map(|m| m.session_id.clone()))
            .or_else(|| {
                dir.file_name()
                    .and_then(|n| n.to_str())
                    .map(str::to_string)
            })?;
        let project_dir = val
            .get("work_dir")
            .or_else(|| val.get("cwd"))
            .or_else(|| val.pointer("/info/cwd"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .or_else(|| jsonl_meta.as_ref().and_then(|m| m.project_dir.clone()));
        let title = val
            .get("title")
            .or_else(|| val.get("generated_title"))
            .or_else(|| val.get("last_prompt"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .or_else(|| jsonl_meta.as_ref().and_then(|m| m.title.clone()));
        let summary = val
            .get("summary")
            .or_else(|| val.get("session_summary"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .or_else(|| title.clone());
        let created_at = val
            .get("created_at")
            .and_then(parse_ts_value)
            .or_else(|| jsonl_meta.as_ref().and_then(|m| m.created_at));
        let last_active_at = val
            .get("updated_at")
            .or_else(|| val.get("last_active_at"))
            .and_then(parse_ts_value)
            .or(created_at)
            .or_else(|| jsonl_meta.as_ref().and_then(|m| m.last_active_at))
            .or_else(|| {
                transcript
                    .as_deref()
                    .map(modified_ms)
                    .or_else(|| Some(modified_ms(dir)))
            });
        let source_path = transcript
            .unwrap_or_else(|| dir.to_path_buf())
            .to_string_lossy()
            .to_string();
        return Some(SessionMeta {
            provider_id: "kimi".into(),
            session_id: session_id.clone(),
            title,
            summary,
            project_dir,
            created_at,
            last_active_at,
            source_path: source_path.clone(),
            resume_command: resume_command(ToolId::Kimi, &session_id, Some(&source_path)),
        });
    }

    jsonl_meta
}

/// OpenClaw: `~/.openclaw/agents/<agentId>/sessions/*.jsonl` (plus legacy `sessions/*.jsonl`).
fn scan_openclaw(paths: &Paths, limit: usize) -> Vec<SessionMeta> {
    let base = paths.tool_root(ToolId::OpenClaw);
    let mut files = Vec::new();
    let agents_dir = base.join("agents");
    if let Ok(entries) = std::fs::read_dir(&agents_dir) {
        for agent_entry in entries.filter_map(|e| e.ok()) {
            let sessions_dir = agent_entry.path().join("sessions");
            if let Ok(session_entries) = std::fs::read_dir(&sessions_dir) {
                for entry in session_entries.filter_map(|e| e.ok()) {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("jsonl")
                        && path.file_name().and_then(|n| n.to_str()) != Some("sessions.json")
                    {
                        files.push(path);
                    }
                }
            }
        }
    }
    let legacy_sessions = base.join("sessions");
    if legacy_sessions.is_dir() {
        collect_jsonl(&legacy_sessions, &mut files);
    }
    files.sort_by_key(|p| std::cmp::Reverse(modified_ms(p)));
    if limit != usize::MAX {
        files.truncate(limit.saturating_mul(3));
    }
    files
        .into_iter()
        .filter_map(|path| parse_jsonl_meta(&path, "openclaw", ToolId::OpenClaw))
        .collect()
}

/// Gemini CLI: `~/.gemini/tmp/<project>/chats/session-*.{json,jsonl}`.
fn scan_gemini_cli(root: &Path, limit: usize) -> Vec<SessionMeta> {
    let mut files = Vec::new();
    collect_gemini_files(root, &mut files);
    files.sort_by_key(|p| std::cmp::Reverse(modified_ms(p)));
    if limit != usize::MAX {
        files.truncate(limit.saturating_mul(3));
    }
    let project_registry = read_gemini_projects_registry(root);
    let mut by_id: HashMap<String, SessionMeta> = HashMap::new();
    for path in files {
        if let Some(meta) = parse_gemini_session(&path, &project_registry) {
            let replace = by_id
                .get(&meta.session_id)
                .is_none_or(|old| meta.last_active_at >= old.last_active_at);
            if replace {
                by_id.insert(meta.session_id.clone(), meta);
            }
        }
    }
    let mut out: Vec<SessionMeta> = by_id.into_values().collect();
    out.sort_by_key(|s| std::cmp::Reverse(s.last_active_at));
    if limit != usize::MAX {
        out.truncate(limit);
    }
    out
}

fn collect_gemini_files(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(project_entries) = std::fs::read_dir(root) else {
        return;
    };
    for project_entry in project_entries.filter_map(|e| e.ok()) {
        let project_dir = project_entry.path();
        if !project_dir.is_dir() {
            continue;
        }
        let chats_dir = project_dir.join("chats");
        if let Ok(chat_entries) = std::fs::read_dir(&chats_dir) {
            for entry in chat_entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                let ext = path.extension().and_then(|e| e.to_str());
                if path.is_file() && matches!(ext, Some("json" | "jsonl")) {
                    out.push(path);
                }
            }
        }
    }
}

fn read_gemini_projects_registry(tmp_root: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Some(gemini_dir) = tmp_root.parent() else {
        return map;
    };
    let Ok(raw) = std::fs::read_to_string(gemini_dir.join("projects.json")) else {
        return map;
    };
    let Ok(val) = serde_json::from_str::<Value>(&raw) else {
        return map;
    };
    if let Some(projects) = val.get("projects").and_then(Value::as_object) {
        for (path_key, alias) in projects {
            if let Some(slug) = alias.as_str() {
                map.insert(slug.to_string(), path_key.clone());
            }
        }
    }
    map
}

fn gemini_project_dir_for_file(
    session_path: &Path,
    project_registry: &HashMap<String, String>,
) -> Option<String> {
    let project_dir = session_path.parent()?.parent()?;
    std::fs::read_to_string(project_dir.join(".project_root"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            project_dir
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|key| project_registry.get(key).cloned())
        })
}

fn parse_gemini_session(
    path: &Path,
    project_registry: &HashMap<String, String>,
) -> Option<SessionMeta> {
    let data = std::fs::read_to_string(path).ok()?;
    let is_jsonl = path.extension().and_then(|e| e.to_str()) == Some("jsonl");
    let conv = parse_gemini_conversation(&data, false, is_jsonl)?;
    if conv.kind.as_deref() == Some("subagent") || conv.messages.is_empty() && !conv.has_chat {
        return None;
    }
    let project_dir = gemini_project_dir_for_file(path, project_registry);
    let title = conv
        .summary
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.chars().take(80).collect::<String>())
        .or_else(|| {
            conv.first_user
                .as_deref()
                .and_then(|t| extract_prompt_title_text(t, 80))
        })
        .or_else(|| project_dir.as_deref().and_then(path_basename));
    let source_str = path.to_string_lossy().to_string();
    Some(SessionMeta {
        provider_id: "gemini_cli".into(),
        session_id: conv.session_id.clone(),
        title: title.clone(),
        summary: title,
        project_dir,
        created_at: conv.start_time,
        last_active_at: conv.last_updated.or(conv.start_time).or(Some(modified_ms(path))),
        source_path: source_str.clone(),
        resume_command: resume_command(ToolId::GeminiCli, &conv.session_id, Some(&source_str)),
    })
}

struct GeminiConversation {
    session_id: String,
    start_time: Option<i64>,
    last_updated: Option<i64>,
    summary: Option<String>,
    kind: Option<String>,
    first_user: Option<String>,
    has_chat: bool,
    messages: Vec<Value>,
}

fn gemini_message_type_is_chat(t: &str) -> bool {
    matches!(t, "user" | "gemini" | "assistant" | "model")
}

fn gemini_first_user_text(messages: &[Value]) -> Option<String> {
    for msg in messages {
        let t = msg.get("type").and_then(Value::as_str).unwrap_or("");
        if t == "user" {
            let raw = msg
                .get("displayContent")
                .or_else(|| msg.get("content"))
                .map(content_text)
                .unwrap_or_default();
            if !raw.trim().is_empty() {
                return Some(raw);
            }
        }
    }
    None
}

fn parse_gemini_conversation(
    data: &str,
    include_messages: bool,
    prefer_jsonl: bool,
) -> Option<GeminiConversation> {
    if !prefer_jsonl
        && let Ok(val) = serde_json::from_str::<Value>(data)
        && let Some(sid) = val.get("sessionId").and_then(Value::as_str)
    {
        let msgs = val
            .get("messages")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let has_chat = msgs.iter().any(|m| {
            gemini_message_type_is_chat(m.get("type").and_then(Value::as_str).unwrap_or(""))
        });
        let first_user = gemini_first_user_text(&msgs);
        return Some(GeminiConversation {
            session_id: sid.to_string(),
            start_time: val.get("startTime").and_then(parse_ts_value),
            last_updated: val.get("lastUpdated").and_then(parse_ts_value),
            summary: val
                .get("summary")
                .and_then(Value::as_str)
                .map(str::to_string),
            kind: val.get("kind").and_then(Value::as_str).map(str::to_string),
            first_user,
            has_chat,
            messages: if include_messages { msgs } else { Vec::new() },
        });
    }

    let mut session_id: Option<String> = None;
    let mut start_time: Option<i64> = None;
    let mut last_updated: Option<i64> = None;
    let mut summary: Option<String> = None;
    let mut kind: Option<String> = None;
    let mut metadata_messages: Option<Vec<Value>> = None;
    let mut messages: Vec<Value> = Vec::new();
    let mut msg_indices: HashMap<String, usize> = HashMap::new();

    let apply_meta = |obj: &serde_json::Map<String, Value>,
                      sid: &mut Option<String>,
                      st: &mut Option<i64>,
                      lu: &mut Option<i64>,
                      sum: &mut Option<String>,
                      knd: &mut Option<String>,
                      mmsgs: &mut Option<Vec<Value>>| {
        if let Some(s) = obj.get("sessionId").and_then(Value::as_str) {
            *sid = Some(s.to_string());
        }
        if let Some(t) = obj.get("startTime").and_then(parse_ts_value) {
            *st = Some(t);
        }
        if let Some(t) = obj.get("lastUpdated").and_then(parse_ts_value) {
            *lu = Some(t);
        }
        if let Some(s) = obj.get("summary").and_then(Value::as_str) {
            *sum = Some(s.to_string());
        }
        if let Some(k) = obj.get("kind").and_then(Value::as_str) {
            *knd = Some(k.to_string());
        }
        if let Some(arr) = obj.get("messages").and_then(Value::as_array) {
            *mmsgs = Some(arr.clone());
        }
    };

    for line in data.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(rec) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        if let Some(rewind_id) = rec.get("$rewindTo").and_then(Value::as_str) {
            if let Some(&pos) = msg_indices.get(rewind_id) {
                messages.truncate(pos);
                msg_indices.retain(|_, &mut idx| idx < pos);
            }
            continue;
        }
        let rec_type = rec.get("type").and_then(Value::as_str).unwrap_or("");
        if gemini_message_type_is_chat(rec_type) || rec_type == "info" || rec_type == "error" {
            if let Some(id) = rec.get("id").and_then(Value::as_str) {
                if let Some(&idx) = msg_indices.get(id) {
                    messages[idx] = rec;
                } else {
                    msg_indices.insert(id.to_string(), messages.len());
                    messages.push(rec);
                }
            } else {
                messages.push(rec);
            }
            continue;
        }
        if let Some(update) = rec.get("$set").and_then(Value::as_object) {
            apply_meta(
                update,
                &mut session_id,
                &mut start_time,
                &mut last_updated,
                &mut summary,
                &mut kind,
                &mut metadata_messages,
            );
            continue;
        }
        if let Some(obj) = rec.as_object() {
            apply_meta(
                obj,
                &mut session_id,
                &mut start_time,
                &mut last_updated,
                &mut summary,
                &mut kind,
                &mut metadata_messages,
            );
        }
    }

    if session_id.is_none()
        && let Ok(val) = serde_json::from_str::<Value>(data)
        && val.get("sessionId").is_some()
    {
        return parse_gemini_conversation(data, include_messages, false);
    }

    let session_id = session_id?;
    let effective = metadata_messages
        .filter(|m| !m.is_empty())
        .unwrap_or(messages);
    let has_chat = effective.iter().any(|m| {
        gemini_message_type_is_chat(m.get("type").and_then(Value::as_str).unwrap_or(""))
    });
    let first_user = gemini_first_user_text(&effective);
    Some(GeminiConversation {
        session_id,
        start_time,
        last_updated,
        summary,
        kind,
        first_user,
        has_chat,
        messages: if include_messages { effective } else { Vec::new() },
    })
}

fn sqlite_table_exists(conn: &Connection, table: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1 LIMIT 1",
        [table],
        |_| Ok(()),
    )
    .is_ok()
}

/// OpenCode: `opencode.db` (V2 `session_v2` / V1 `session`) + legacy `storage/session/**/*.json`.
fn scan_opencode(roots: &[PathBuf], limit: usize) -> Vec<SessionMeta> {
    let mut seen_ids: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for root in roots {
        let db_path = root.join("opencode.db");
        if db_path.is_file() {
            for meta in scan_opencode_sqlite(&db_path, limit) {
                if seen_ids.insert(meta.session_id.clone()) {
                    out.push(meta);
                }
            }
        }
        let storage_root = root.join("storage").join("session");
        if storage_root.is_dir() {
            let mut files = Vec::new();
            collect_json_files(&storage_root, &mut files);
            files.sort_by_key(|p| std::cmp::Reverse(modified_ms(p)));
            for path in files {
                if let Some(meta) = parse_opencode_json_session(&path)
                    && seen_ids.insert(meta.session_id.clone())
                {
                    out.push(meta);
                }
            }
        }
    }
    out.sort_by_key(|s| std::cmp::Reverse(s.last_active_at));
    if limit != usize::MAX {
        out.truncate(limit);
    }
    out
}

fn collect_json_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            collect_json_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("json") {
            out.push(path);
        }
    }
}

fn parse_opencode_json_session(path: &Path) -> Option<SessionMeta> {
    let raw = std::fs::read_to_string(path).ok()?;
    let val: Value = serde_json::from_str(&raw).ok()?;
    if val
        .get("parentID")
        .or_else(|| val.get("parent_id"))
        .and_then(Value::as_str)
        .is_some_and(|p| !p.trim().is_empty())
    {
        return None;
    }
    let session_id = val
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())?
        .to_string();
    let directory = val
        .get("directory")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let title = val
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| directory.as_deref().and_then(path_basename));
    let created_at = val
        .pointer("/time/created")
        .or_else(|| val.get("time_created"))
        .and_then(parse_ts_value);
    let last_active_at = val
        .pointer("/time/updated")
        .or_else(|| val.get("time_updated"))
        .and_then(parse_ts_value)
        .or(created_at)
        .or_else(|| Some(modified_ms(path)));
    let source_path = path.to_string_lossy().to_string();
    Some(SessionMeta {
        provider_id: "opencode".into(),
        session_id: session_id.clone(),
        title: title.clone(),
        summary: title,
        project_dir: directory,
        created_at,
        last_active_at,
        source_path: source_path.clone(),
        resume_command: resume_command(ToolId::OpenCode, &session_id, Some(&source_path)),
    })
}

fn scan_opencode_sqlite(db_path: &Path, limit: usize) -> Vec<SessionMeta> {
    let Ok(conn) = Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) else {
        return Vec::new();
    };
    let is_v2 = sqlite_table_exists(&conn, "session_v2");
    if !is_v2 && !sqlite_table_exists(&conn, "session") {
        return Vec::new();
    }
    let has_limit = limit != usize::MAX;
    let sql = match (is_v2, has_limit) {
        (true, true) => {
            "SELECT id, COALESCE(title, ''), COALESCE(directory, ''), time_created, time_updated FROM session_v2 WHERE parent_id IS NULL ORDER BY time_updated DESC LIMIT ?1"
        }
        (true, false) => {
            "SELECT id, COALESCE(title, ''), COALESCE(directory, ''), time_created, time_updated FROM session_v2 WHERE parent_id IS NULL ORDER BY time_updated DESC"
        }
        (false, true) => {
            "SELECT id, COALESCE(title, ''), COALESCE(directory, ''), time_created, time_updated FROM session ORDER BY time_updated DESC LIMIT ?1"
        }
        (false, false) => {
            "SELECT id, COALESCE(title, ''), COALESCE(directory, ''), time_created, time_updated FROM session ORDER BY time_updated DESC"
        }
    };
    let Ok(mut stmt) = conn.prepare(sql) else {
        return Vec::new();
    };
    let row_fn = |r: &rusqlite::Row<'_>| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, i64>(4)?,
        ))
    };
    let rows = if has_limit {
        stmt.query_map([limit as i64], row_fn)
    } else {
        stmt.query_map([], row_fn)
    };
    let Ok(rows) = rows else {
        return Vec::new();
    };
    let db_disp = db_path.display().to_string();
    let mut out = Vec::new();
    for (session_id, title, directory, created_at, updated_at) in rows.flatten() {
        let project_dir = (!directory.trim().is_empty()).then_some(directory);
        let display_title = if !title.trim().is_empty() {
            Some(title)
        } else {
            project_dir.as_deref().and_then(path_basename)
        };
        let source_path = format!("sqlite:{db_disp}:{session_id}");
        out.push(SessionMeta {
            provider_id: "opencode".into(),
            session_id: session_id.clone(),
            title: display_title.clone(),
            summary: display_title,
            project_dir,
            created_at: Some(created_at),
            last_active_at: Some(updated_at),
            source_path: source_path.clone(),
            resume_command: resume_command(ToolId::OpenCode, &session_id, Some(&source_path)),
        });
    }
    out
}

/// Hermes: `<root>/state.db` + `<root>/profiles/*/state.db` (`sessions` table)
/// merged with `<root>/sessions/*.jsonl` + `profiles/*/sessions/*.jsonl`.
fn scan_hermes(root: &Path, limit: usize) -> Vec<SessionMeta> {
    if !root.is_dir() {
        return Vec::new();
    }
    let mut db_paths = Vec::new();
    let mut session_dirs = Vec::new();
    let root_db = root.join("state.db");
    if root_db.is_file() {
        db_paths.push(root_db);
    }
    let root_sess = root.join("sessions");
    if root_sess.is_dir() {
        session_dirs.push(root_sess);
    }
    if let Ok(entries) = std::fs::read_dir(root.join("profiles")) {
        for entry in entries.filter_map(|e| e.ok()) {
            let p = entry.path();
            if p.is_dir() {
                let db = p.join("state.db");
                if db.is_file() {
                    db_paths.push(db);
                }
                let sd = p.join("sessions");
                if sd.is_dir() {
                    session_dirs.push(sd);
                }
            }
        }
    }

    let mut seen_ids: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for db_path in db_paths {
        for meta in scan_hermes_sqlite(&db_path) {
            if seen_ids.insert(meta.session_id.clone()) {
                out.push(meta);
            }
        }
    }
    for sdir in session_dirs {
        for meta in scan_jsonl_generic(&sdir, ToolId::Hermes, usize::MAX) {
            if seen_ids.insert(meta.session_id.clone()) {
                out.push(meta);
            }
        }
    }
    out.sort_by_key(|s| std::cmp::Reverse(s.last_active_at));
    if limit != usize::MAX {
        out.truncate(limit);
    }
    out
}

fn sqlite_col_as_ts(row: &rusqlite::Row<'_>, idx: usize) -> Option<i64> {
    let val = row.get_ref(idx).ok()?;
    match val {
        rusqlite::types::ValueRef::Integer(n) => {
            Some(if n < 1_000_000_000_000 { n * 1000 } else { n })
        }
        rusqlite::types::ValueRef::Real(f) => {
            let n = f as i64;
            Some(if n < 1_000_000_000_000 { n * 1000 } else { n })
        }
        rusqlite::types::ValueRef::Text(bytes) => {
            let s = std::str::from_utf8(bytes).ok()?.trim();
            if let Ok(n) = s.parse::<i64>() {
                Some(if n < 1_000_000_000_000 { n * 1000 } else { n })
            } else {
                chrono::DateTime::parse_from_rfc3339(s)
                    .ok()
                    .map(|dt| dt.timestamp_millis())
            }
        }
        _ => None,
    }
}

fn scan_hermes_sqlite(db_path: &Path) -> Vec<SessionMeta> {
    let Ok(conn) = Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) else {
        return Vec::new();
    };
    if !sqlite_table_exists(&conn, "sessions") {
        return Vec::new();
    }
    let mut cols = HashSet::new();
    if let Ok(mut info) = conn.prepare("PRAGMA table_info(sessions)")
        && let Ok(rows) = info.query_map([], |r| r.get::<_, String>(1))
    {
        for c in rows.flatten() {
            cols.insert(c);
        }
    }
    let has = |name: &str| cols.contains(name);
    if !has("id") {
        return Vec::new();
    }
    let title_col = if has("title") { "COALESCE(title, '')" } else { "''" };
    let cwd_col = if has("cwd") {
        "COALESCE(cwd, '')"
    } else if has("directory") {
        "COALESCE(directory, '')"
    } else {
        "''"
    };
    let start_col = if has("started_at") {
        "started_at"
    } else if has("created_at") {
        "created_at"
    } else {
        "NULL"
    };
    let end_col = if has("ended_at") {
        "ended_at"
    } else if has("updated_at") {
        "updated_at"
    } else {
        "NULL"
    };
    let sql = format!(
        "SELECT id, {title_col}, {cwd_col}, {start_col}, {end_col} FROM sessions ORDER BY rowid DESC LIMIT 500"
    );
    let Ok(mut stmt) = conn.prepare(&sql) else {
        return Vec::new();
    };
    let Ok(rows) = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            sqlite_col_as_ts(r, 3),
            sqlite_col_as_ts(r, 4),
        ))
    }) else {
        return Vec::new();
    };
    let db_disp = db_path.display().to_string();
    let mut out = Vec::new();
    for (session_id, title, cwd, started_at, ended_at) in rows.flatten() {
        let project_dir = (!cwd.trim().is_empty()).then_some(cwd);
        let display_title = if !title.trim().is_empty() {
            Some(title.chars().take(80).collect::<String>())
        } else {
            project_dir.as_deref().and_then(path_basename)
        };
        let source_path = format!("sqlite:{db_disp}:{session_id}");
        out.push(SessionMeta {
            provider_id: "hermes".into(),
            session_id,
            title: display_title.clone(),
            summary: display_title,
            project_dir,
            created_at: started_at,
            last_active_at: ended_at.or(started_at),
            source_path,
            resume_command: None,
        });
    }
    out
}

/// Dsh: `<root>/sessions/<id>/session[.vN].jsonl`, keeping highest generation per dir.
fn dsh_generation_num(path: &Path) -> Option<u32> {
    let name = path.file_name()?.to_str()?;
    if name == "session.jsonl" {
        return Some(1);
    }
    let rest = name.strip_prefix("session.v")?.strip_suffix(".jsonl")?;
    if rest.is_empty() || rest.starts_with('0') {
        return None;
    }
    rest.parse().ok()
}

fn select_dsh_session_files(root: &Path) -> Vec<PathBuf> {
    let mut all = Vec::new();
    collect_jsonl(root, &mut all);
    let mut best: HashMap<PathBuf, PathBuf> = HashMap::new();
    let mut fallback = Vec::new();
    for path in all {
        let dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        if let Some(generation) = dsh_generation_num(&path) {
            let replace = best
                .get(&dir)
                .is_none_or(|old| generation > dsh_generation_num(old).unwrap_or(0));
            if replace {
                best.insert(dir, path);
            }
        } else {
            fallback.push(path);
        }
    }
    let mut out: Vec<PathBuf> = best.values().cloned().collect();
    for p in fallback {
        if !best.contains_key(p.parent().unwrap_or(Path::new("."))) {
            out.push(p);
        }
    }
    out
}

fn scan_dsh(root: &Path, limit: usize) -> Vec<SessionMeta> {
    let mut files = select_dsh_session_files(root);
    files.sort_by_key(|p| std::cmp::Reverse(modified_ms(p)));
    if limit != usize::MAX {
        files.truncate(limit.saturating_mul(3));
    }
    files
        .into_iter()
        .filter_map(|p| parse_dsh_session(&p))
        .collect()
}

fn parse_dsh_session(path: &Path) -> Option<SessionMeta> {
    let (head, tail) = read_head_tail_lines(path, 80, 20).ok()?;
    let mut session_id: Option<String> = None;
    let mut created_at: Option<i64> = None;
    let mut cwd: Option<String> = None;
    let mut first_user: Option<String> = None;
    let mut title_event: Option<String> = None;
    let mut is_subagent = false;
    let mut has_turn = false;
    let mut last_ts: Option<i64> = None;

    for raw in head.iter().chain(tail.iter()) {
        let Ok(val) = serde_json::from_str::<Value>(raw) else {
            continue;
        };
        if let Some(ts) = val.get("time").and_then(parse_ts_value) {
            last_ts = Some(last_ts.map_or(ts, |m| m.max(ts)));
        }
        let ev = val.get("type").and_then(Value::as_str).unwrap_or("");
        if ev == "session" {
            if session_id.is_none() {
                session_id = val.get("id").and_then(Value::as_str).map(str::to_string);
            }
            if created_at.is_none() {
                created_at = val.get("createdAt").and_then(parse_ts_value);
            }
            if cwd.is_none() {
                cwd = val
                    .get("cwd")
                    .and_then(Value::as_str)
                    .filter(|s| !s.trim().is_empty())
                    .map(str::to_string);
            }
            is_subagent = val.get("origin").and_then(Value::as_str) == Some("subagent");
        } else if ev == "turn/start" {
            has_turn = true;
        } else if ev == "session/title" {
            if let Some(t) = val
                .pointer("/data/title")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                title_event = Some(t.chars().take(80).collect());
            }
        } else if ev == "user/message" {
            let append_ok = match val.get("surfaceOp") {
                None => true,
                Some(Value::String(op)) => op == "append",
                _ => false,
            };
            if append_ok
                && val.pointer("/data/source/kind").and_then(Value::as_str) == Some("user")
            {
                has_turn = true;
                if first_user.is_none()
                    && let Some(content) = val.pointer("/data/content")
                {
                    let txt = content_text(content);
                    if !txt.trim().is_empty() {
                        first_user = extract_prompt_title_text(&txt, 80);
                    }
                }
            }
        }
    }

    let session_id = session_id?;
    if is_subagent || !has_turn {
        return None;
    }
    let mtime = modified_ms(path);
    let created = created_at.or(Some(mtime));
    let last_active = last_ts.unwrap_or(mtime).max(created.unwrap_or(0));
    let display_title = title_event
        .or_else(|| first_user.clone())
        .or_else(|| cwd.as_deref().and_then(path_basename));
    Some(SessionMeta {
        provider_id: "dsh".into(),
        session_id,
        title: display_title.clone(),
        summary: first_user.or(display_title),
        project_dir: cwd,
        created_at: created,
        last_active_at: Some(last_active),
        source_path: path.to_string_lossy().to_string(),
        resume_command: None,
    })
}

/// Claude Desktop: `<root>/local-agent-mode-sessions/**/local_*.json` + `.claude/projects/**/*.jsonl`.
fn scan_claude_desktop(roots: &[PathBuf], limit: usize) -> Vec<SessionMeta> {
    let mut meta_jsons = Vec::new();
    for root in roots {
        if root.is_dir() {
            collect_claude_desktop_metas(root, &mut meta_jsons);
        }
    }
    let mut out = Vec::new();
    for meta_path in meta_jsons {
        if let Some(meta) = parse_claude_desktop_session(&meta_path) {
            out.push(meta);
        }
    }
    out.sort_by_key(|s| std::cmp::Reverse(s.last_active_at));
    if limit != usize::MAX {
        out.truncate(limit);
    }
    out
}

fn collect_claude_desktop_metas(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            collect_claude_desktop_metas(&path, out);
        } else if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("local_") && n.ends_with(".json"))
        {
            out.push(path);
        }
    }
}

fn parse_claude_desktop_session(meta_path: &Path) -> Option<SessionMeta> {
    let raw = std::fs::read_to_string(meta_path).ok()?;
    let val: Value = serde_json::from_str(&raw).ok()?;
    let cli_sid = val
        .get("cliSessionId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let local_sid = val
        .get("sessionId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let session_id = cli_sid
        .or(local_sid)
        .map(str::to_string)
        .or_else(|| meta_path.file_stem().and_then(|n| n.to_str()).map(str::to_string))?;

    let projects_root = meta_path.with_extension("").join(".claude").join("projects");
    let mut jsonls = Vec::new();
    if projects_root.is_dir() {
        collect_jsonl(&projects_root, &mut jsonls);
    }
    if jsonls.is_empty() {
        return None;
    }
    let transcript = if let Some(cid) = cli_sid {
        let expected = format!("{cid}.jsonl");
        jsonls
            .iter()
            .find(|p| p.file_name().and_then(|n| n.to_str()) == Some(expected.as_str()))
            .cloned()
            .or_else(|| (jsonls.len() == 1).then(|| jsonls.remove(0)))?
    } else if jsonls.len() == 1 {
        jsonls.remove(0)
    } else {
        return None;
    };

    let title = val
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let initial = val
        .get("initialMessage")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let project_dir = val
        .get("resolvedFolderKinds")
        .and_then(Value::as_array)
        .and_then(|arr| arr.first())
        .and_then(|item| item.get("display"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            val.get("userSelectedFolders")
                .and_then(Value::as_array)
                .and_then(|arr| arr.first())
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
        .or_else(|| {
            let cwd = val.get("cwd").and_then(Value::as_str)?.trim();
            if cwd.is_empty() {
                return None;
            }
            let p = Path::new(cwd);
            let dir = if p.file_name().and_then(|n| n.to_str()) == Some("outputs") {
                p.parent().unwrap_or(p)
            } else {
                p
            };
            Some(dir.to_string_lossy().to_string())
        });

    let created_at = val.get("createdAt").and_then(parse_ts_value);
    let last_active_at = val
        .get("lastActivityAt")
        .and_then(parse_ts_value)
        .or_else(|| Some(modified_ms(&transcript)));

    Some(SessionMeta {
        provider_id: "claude_desktop".into(),
        session_id,
        title: title.or_else(|| initial.clone()),
        summary: initial,
        project_dir,
        created_at,
        last_active_at,
        source_path: transcript.to_string_lossy().to_string(),
        resume_command: None,
    })
}

/// Generic JSONL scan for tools whose sessions are line-delimited JSON
/// files (codex/pi/omp/gemini/opencode/openclaw/hermes/dsh).
fn scan_jsonl_generic(root: &Path, tool: ToolId, limit: usize) -> Vec<SessionMeta> {
    let mut files = vec![];
    collect_jsonl(root, &mut files);
    files.sort_by_key(|p| std::cmp::Reverse(modified_ms(p)));
    if limit != usize::MAX {
        files.truncate(limit.saturating_mul(3));
    }
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
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.eq_ignore_ascii_case("subagents")
                || name.eq_ignore_ascii_case("tool-results")
                || name.eq_ignore_ascii_case("memory")
                || name.eq_ignore_ascii_case("context-fold")
                || name.eq_ignore_ascii_case("subagent-artifacts")
            {
                continue;
            }
            collect_jsonl(&path, out);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("jsonl") {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with("agent-") {
                continue;
            }
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

fn strip_xml_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        if c == '<' {
            in_tag = true;
        } else if c == '>' {
            in_tag = false;
        } else if !in_tag {
            out.push(c);
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PromptWrapperBlock {
    Instructions,
    Permissions,
    Skills,
    Environment,
    UserAction,
    CollaborationMode,
    BracketedPrompt,
    SkillTag,
}

fn is_bracketed_prompt_wrapper_start(line: &str) -> bool {
    line.starts_with("[Assistant Rules")
        || line == "[Available Skills]"
        || line == "[Available Tools]"
}

fn is_bracketed_prompt_section_header(line: &str) -> bool {
    line.len() >= 3 && line.len() <= 120 && line.starts_with('[') && line.ends_with(']')
}

fn detect_prompt_wrapper_start(line: &str) -> Option<PromptWrapperBlock> {
    if line.starts_with("# AGENTS.md instructions") || line == "<INSTRUCTIONS>" {
        return Some(PromptWrapperBlock::Instructions);
    }
    if line.starts_with("<skill") {
        return Some(PromptWrapperBlock::SkillTag);
    }
    match line {
        "<permissions instructions>" => Some(PromptWrapperBlock::Permissions),
        "<skills_instructions>" => Some(PromptWrapperBlock::Skills),
        "<environment_context>" => Some(PromptWrapperBlock::Environment),
        "<user_action>" => Some(PromptWrapperBlock::UserAction),
        "<collaboration_mode>" => Some(PromptWrapperBlock::CollaborationMode),
        _ => None,
    }
}

fn is_prompt_wrapper_end(line: &str, wrapper: PromptWrapperBlock) -> bool {
    match wrapper {
        PromptWrapperBlock::Instructions => line == "</INSTRUCTIONS>",
        PromptWrapperBlock::Permissions => line == "</permissions instructions>",
        PromptWrapperBlock::Skills => line == "</skills_instructions>",
        PromptWrapperBlock::Environment => line == "</environment_context>",
        PromptWrapperBlock::UserAction => line == "</user_action>",
        PromptWrapperBlock::CollaborationMode => line == "</collaboration_mode>",
        PromptWrapperBlock::BracketedPrompt => false,
        PromptWrapperBlock::SkillTag => line.starts_with("</skill>"),
    }
}

fn strip_user_request_marker(line: &str) -> Option<&str> {
    if line == "[User Request]" {
        return Some("");
    }
    let rest = line.strip_prefix("[User Request]")?.trim_start();
    Some(rest.strip_prefix(':').unwrap_or(rest).trim_start())
}

fn extract_wrapped_user_request_text(text: &str) -> Option<String> {
    let mut is_user_request_block = false;
    let mut lines = Vec::new();

    for raw_line in text.lines() {
        let line = raw_line.trim();

        if let Some(rest) = strip_user_request_marker(line) {
            is_user_request_block = true;
            if !rest.is_empty() {
                lines.push(rest.to_string());
            }
            continue;
        }

        if !is_user_request_block {
            continue;
        }

        if is_bracketed_prompt_section_header(line) {
            break;
        }

        lines.push(raw_line.trim_end().to_string());
    }

    let value = lines.join("\n");
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn is_prompt_title_noise_line(line: &str) -> bool {
    if line.starts_with('/')
        || line.starts_with("Based on this message")
        || line.starts_with("References are relative to")
    {
        return true;
    }
    if line.starts_with('<') && line.ends_with('>') {
        return true;
    }
    let lowercase = line.to_lowercase();
    matches!(
        lowercase.as_str(),
        "hi" | "hello" | "hey" | "在吗" | "在么" | "在不在" | "你好" | "您好" | "嗨" | "test"
    )
}

fn extract_prompt_title_text(text: &str, max_chars: usize) -> Option<String> {
    let unwrapped = extract_wrapped_user_request_text(text);
    let target = unwrapped.as_deref().unwrap_or(text);
    let mut active_wrapper: Option<PromptWrapperBlock> = None;

    for raw_line in target.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        if is_bracketed_prompt_wrapper_start(line) {
            active_wrapper = Some(PromptWrapperBlock::BracketedPrompt);
            continue;
        }

        if let Some(wrapper) = active_wrapper {
            if is_prompt_wrapper_end(line, wrapper) {
                active_wrapper = None;
            }
            continue;
        }

        if let Some(wrapper) = detect_prompt_wrapper_start(line) {
            if !is_prompt_wrapper_end(line, wrapper) {
                active_wrapper = Some(wrapper);
            }
            continue;
        }

        if is_prompt_title_noise_line(line) {
            continue;
        }

        let collapsed = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if collapsed.is_empty() {
            continue;
        }

        let chars: Vec<char> = collapsed.chars().collect();
        return if chars.len() <= max_chars {
            Some(collapsed)
        } else {
            let truncated: String = chars.into_iter().take(max_chars).collect();
            Some(format!("{truncated}..."))
        };
    }

    None
}

fn path_basename(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let normalized = trimmed.trim_end_matches(['/', '\\']);
    let last = normalized
        .split(['/', '\\'])
        .next_back()
        .filter(|segment| !segment.is_empty())?;
    Some(last.to_string())
}

fn clean_session_title(raw: &str) -> String {
    let text = raw.trim();
    if let Some(start) = text.find("<command-args>") {
        if let Some(end) = text[start..].find("</command-args>") {
            let inner = &text[start + "<command-args>".len()..start + end];
            let inner_trimmed = inner.trim();
            if !inner_trimmed.is_empty() {
                return inner_trimmed.chars().take(120).collect();
            }
        }
    }
    if let Some(start) = text.find("<command-name>") {
        if let Some(end) = text[start..].find("</command-name>") {
            let cmd = text[start + "<command-name>".len()..start + end].trim();
            let stripped = strip_xml_tags(text);
            if !stripped.is_empty() {
                return stripped.chars().take(120).collect();
            }
            return cmd.chars().take(120).collect();
        }
    }
    let stripped = strip_xml_tags(text);
    if !stripped.is_empty() {
        stripped.chars().take(120).collect()
    } else {
        text.chars().take(120).collect()
    }
}

pub fn clean_antigravity_user_content(raw: &str) -> String {
    let text = raw.trim();
    if let Some(start) = text.find("<USER_REQUEST>") {
        if let Some(end) = text[start..].find("</USER_REQUEST>") {
            return text[start + "<USER_REQUEST>".len()..start + end].trim().to_string();
        }
    }
    text.to_string()
}

fn parse_ts_value(v: &Value) -> Option<i64> {
    if let Some(num) = v.as_i64() {
        return Some(if num > 1_000_000_000_000 {
            num
        } else {
            num * 1000
        });
    }
    if let Some(num) = v.as_f64() {
        let num = num as i64;
        return Some(if num > 1_000_000_000_000 {
            num
        } else {
            num * 1000
        });
    }
    if let Some(s) = v.as_str() {
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
            return Some(dt.timestamp_millis());
        }
    }
    None
}

fn read_head_tail_lines(
    path: &Path,
    head_n: usize,
    tail_n: usize,
) -> std::io::Result<(Vec<String>, Vec<String>)> {
    use std::fs::File;
    use std::io::{BufRead, BufReader, Seek, SeekFrom};

    let file = File::open(path)?;
    let file_len = file.metadata()?.len();

    if file_len < 32_768 {
        let reader = BufReader::new(file);
        let all_lines: Vec<String> = reader.lines().map_while(Result::ok).collect();
        let head = all_lines.iter().take(head_n).cloned().collect();
        let skip = all_lines.len().saturating_sub(tail_n);
        let tail = all_lines.into_iter().skip(skip).collect();
        return Ok((head, tail));
    }

    let reader = BufReader::new(file);
    let head: Vec<String> = reader.lines().take(head_n).map_while(Result::ok).collect();

    let seek_pos = file_len.saturating_sub(32_768);
    let mut tail_file = File::open(path)?;
    tail_file.seek(SeekFrom::Start(seek_pos))?;
    let tail_reader = BufReader::new(tail_file);
    let all_tail: Vec<String> = tail_reader.lines().map_while(Result::ok).collect();

    let skip_first = if seek_pos > 0 { 1 } else { 0 };
    let usable_tail: Vec<String> = all_tail.into_iter().skip(skip_first).collect();
    let skip = usable_tail.len().saturating_sub(tail_n);
    let tail = usable_tail.into_iter().skip(skip).collect();

    Ok((head, tail))
}

fn parse_jsonl_meta(path: &Path, provider: &str, tool: ToolId) -> Option<SessionMeta> {
    let (head, tail) = read_head_tail_lines(path, 80, 80).ok()?;

    let mut first_user: Option<String> = None;
    let mut latest_session_name: Option<String> = None;
    let mut last_ts: Option<i64> = None;
    let mut first_ts: Option<i64> = None;
    let mut message_count = 0usize;
    let mut project_dir: Option<String> = None;
    let mut session_id_from_file: Option<String> = None;

    for line in head.iter().chain(tail.iter()) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };

        let entry_type = value.get("type").and_then(Value::as_str).unwrap_or("");
        if entry_type == "session" || entry_type == "session_meta" {
            let id = value
                .get("id")
                .or_else(|| value.pointer("/payload/id"))
                .or_else(|| value.get("sessionId"))
                .and_then(Value::as_str);
            if let Some(id) = id {
                if !id.trim().is_empty() {
                    session_id_from_file = Some(id.to_string());
                }
            }
            let cwd = value
                .get("cwd")
                .or_else(|| value.pointer("/payload/cwd"))
                .and_then(Value::as_str);
            if let Some(cwd) = cwd {
                if !cwd.trim().is_empty() {
                    project_dir = Some(cwd.to_string());
                }
            }
            if let Some(ts) = value.get("timestamp").and_then(parse_ts_value) {
                if first_ts.is_none() {
                    first_ts = Some(ts);
                }
            }
        } else if entry_type == "session_info" {
            if let Some(name) = value.get("name").and_then(Value::as_str) {
                let trimmed_name = name.trim();
                if !trimmed_name.is_empty() {
                    latest_session_name = Some(trimmed_name.to_string());
                }
            }
        }

        if let Some(slug) = value.get("slug").and_then(Value::as_str) {
            let trimmed = slug.trim();
            if !trimmed.is_empty() && latest_session_name.is_none() {
                latest_session_name = Some(trimmed.to_string());
            }
        }

        if project_dir.is_none() {
            if let Some(cwd) = value.get("cwd").and_then(Value::as_str) {
                if !cwd.trim().is_empty() {
                    project_dir = Some(cwd.to_string());
                }
            } else if let Some(cwd) = value.pointer("/attachment/snapshot/workingDirectory").and_then(Value::as_str) {
                if !cwd.trim().is_empty() {
                    project_dir = Some(cwd.to_string());
                }
            }
        }

        let role = value
            .pointer("/message/role")
            .and_then(Value::as_str)
            .or_else(|| value.get("role").and_then(Value::as_str))
            .or_else(|| value.pointer("/payload/role").and_then(Value::as_str))
            .or_else(|| {
                if entry_type == "user" || entry_type == "assistant" {
                    Some(entry_type)
                } else {
                    None
                }
            })
            .or_else(|| {
                if tool == ToolId::Kimi {
                    match value.pointer("/message/type").and_then(Value::as_str) {
                        Some("TurnBegin") => Some("user"),
                        Some("ContentPart") => Some("assistant"),
                        _ => None,
                    }
                } else {
                    None
                }
            });

        if let Some(role) = role {
            if role.eq_ignore_ascii_case("user") || role.eq_ignore_ascii_case("assistant") {
                message_count += 1;
            }
            if first_user.is_none() && role.eq_ignore_ascii_case("user") {
                let text = if tool == ToolId::Kimi
                    && value.pointer("/message/type").and_then(Value::as_str) == Some("TurnBegin")
                {
                    value
                        .pointer("/message/payload/user_input")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string()
                } else {
                    extract_text(&value)
                };
                let prompt_title = extract_prompt_title_text(&text, 120);
                let cleaned = prompt_title.unwrap_or_else(|| clean_session_title(&text));
                if !cleaned.is_empty() {
                    first_user = Some(cleaned);
                }
            }
        }

        if let Some(ts) = value
            .get("timestamp")
            .or_else(|| value.get("ts"))
            .or_else(|| value.get("created_at"))
            .or_else(|| value.get("time"))
            .and_then(parse_ts_value)
        {
            if first_ts.is_none() {
                first_ts = Some(ts);
            }
            if entry_type != "session_info" {
                last_ts = Some(ts);
            }
        }
    }

    if message_count == 0 && first_user.is_none() && latest_session_name.is_none() {
        return None;
    }

    let session_id = session_id_from_file.unwrap_or_else(|| {
        if tool == ToolId::Kimi
            && path.file_name().and_then(|n| n.to_str()) == Some("wire.jsonl")
        {
            if let Some(parent_id) = path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .filter(|s| !s.is_empty())
            {
                return parent_id.to_string();
            }
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("session");
        if stem.len() >= 36 {
            let tail = &stem[stem.len() - 36..];
            if tail.chars().filter(|c| *c == '-').count() == 4 {
                return tail.to_string();
            }
        }
        if let Some(pos) = stem.find('_') {
            let suffix = &stem[pos + 1..];
            if suffix.len() >= 32 {
                return suffix.to_string();
            }
        }
        stem.to_string()
    });

    let display_title = latest_session_name
        .or(first_user)
        .or_else(|| project_dir.as_deref().and_then(path_basename));
    let resume = resume_command(tool, &session_id, path.to_str());

    Some(SessionMeta {
        provider_id: provider.to_string(),
        session_id,
        title: display_title.clone(),
        summary: display_title,
        project_dir,
        created_at: first_ts,
        last_active_at: last_ts.or(Some(modified_ms(path))),
        source_path: path.to_string_lossy().to_string(),
        resume_command: resume,
    })
}

fn quote_cli_arg(value: &str) -> String {
    let escaped = value.replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn resume_command(tool: ToolId, session_id: &str, source_path: Option<&str>) -> Option<String> {
    Some(match tool {
        ToolId::ClaudeCode => format!("claude --resume {session_id}"),
        ToolId::Codex => format!("codex resume {session_id}"),
        ToolId::GeminiCli => format!("gemini --resume {session_id}"),
        ToolId::Grok => format!("grok --resume {session_id}"),
        ToolId::Kimi => format!("kimi --session {session_id}"),
        ToolId::OpenCode => format!("opencode -s {session_id}"),
        ToolId::OpenClaw => return None,
        ToolId::Pi => {
            let target = source_path
                .map(quote_cli_arg)
                .unwrap_or_else(|| session_id.to_string());
            format!("pi --session {target}")
        }
        ToolId::OhMyPi => {
            let target = source_path
                .map(quote_cli_arg)
                .unwrap_or_else(|| session_id.to_string());
            format!("omp --resume {target}")
        }
        ToolId::Hermes | ToolId::Dsh | ToolId::ClaudeDesktop | ToolId::Agents => return None,
    })
}

// ---------------------------------------------------------------------------
// Message loading
// ---------------------------------------------------------------------------

fn parse_sqlite_source(source: &str) -> Option<(PathBuf, String)> {
    let rest = source.strip_prefix("sqlite:")?;
    let sep = rest.rfind(':')?;
    let db_path = PathBuf::from(&rest[..sep]);
    let session_id = rest[sep + 1..].to_string();
    if db_path.as_os_str().is_empty() || session_id.is_empty() {
        return None;
    }
    Some((db_path, session_id))
}

pub fn load_messages(_paths: &Paths, meta: &SessionMeta) -> Result<Vec<SessionMessage>, String> {
    if meta.source_path.starts_with("sqlite:") {
        return load_sqlite_messages(&meta.source_path, &meta.provider_id);
    }
    let source = PathBuf::from(&meta.source_path);
    if source.is_dir() {
        for name in ["chat_history.jsonl", "messages.jsonl", "history.jsonl", "wire.jsonl"] {
            let candidate = source.join(name);
            if candidate.is_file() {
                return load_jsonl_messages(&candidate);
            }
        }
        return Ok(vec![]);
    }
    if !source.is_file() {
        return Ok(vec![]);
    }
    if meta.provider_id == "gemini_cli" {
        return load_gemini_messages(&source);
    }
    if meta.provider_id == "dsh" {
        return load_dsh_messages(&source);
    }
    if meta.provider_id == "opencode" && source.extension().and_then(|e| e.to_str()) == Some("json")
    {
        return load_opencode_json_messages(&source, &meta.session_id);
    }
    if matches!(
        source.extension().and_then(|e| e.to_str()),
        Some("pb" | "db" | "sqlite" | "sqlite3")
    ) {
        return Ok(vec![]);
    }
    load_jsonl_messages(&source)
}

fn load_gemini_messages(path: &Path) -> Result<Vec<SessionMessage>, String> {
    let data = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    let is_jsonl = path.extension().and_then(|e| e.to_str()) == Some("jsonl");
    let Some(conv) = parse_gemini_conversation(&data, true, is_jsonl) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for msg in conv.messages {
        let t = msg.get("type").and_then(Value::as_str).unwrap_or("");
        let role = match t {
            "user" => "user",
            "gemini" | "assistant" | "model" => "assistant",
            "info" | "error" => "system",
            _ => continue,
        };
        let ts = msg.get("timestamp").and_then(parse_ts_value);
        let id = msg.get("id").and_then(Value::as_str).map(str::to_string);
        let model = msg.get("model").and_then(Value::as_str).map(str::to_string);
        let mut blocks = Vec::new();
        let mut text_parts = Vec::new();
        if let Some(thoughts) = msg.get("thoughts").and_then(Value::as_array) {
            for th in thoughts {
                let desc = th
                    .get("description")
                    .or_else(|| th.get("subject"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim();
                if !desc.is_empty() {
                    blocks.push(SessionMessageBlock {
                        kind: "thinking".into(),
                        text: Some(desc.to_string()),
                        title: Some("Thinking".into()),
                        ..Default::default()
                    });
                }
            }
        }
        let body = msg
            .get("displayContent")
            .or_else(|| msg.get("content"))
            .map(content_text)
            .unwrap_or_default();
        if !body.trim().is_empty() {
            text_parts.push(body.clone());
            blocks.push(SessionMessageBlock {
                kind: "text".into(),
                text: Some(body),
                ..Default::default()
            });
        }
        if let Some(tcs) = msg.get("toolCalls").and_then(Value::as_array) {
            for tc in tcs {
                let name = tc
                    .get("displayName")
                    .or_else(|| tc.get("name"))
                    .and_then(Value::as_str)
                    .unwrap_or("tool");
                let args = tc
                    .get("args")
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                let output = tc
                    .get("resultDisplay")
                    .or_else(|| tc.get("result"))
                    .and_then(|v| {
                        v.as_str()
                            .map(str::to_string)
                            .or_else(|| Some(v.to_string()))
                    });
                blocks.push(SessionMessageBlock {
                    kind: "tool_call".into(),
                    tool_name: Some(name.to_string()),
                    title: Some(format!("Call {name}")),
                    text: Some(args),
                    output,
                    status: tc.get("status").and_then(Value::as_str).map(str::to_string),
                    ..Default::default()
                });
            }
        }
        if blocks.is_empty() && text_parts.is_empty() {
            continue;
        }
        out.push(SessionMessage {
            role: role.to_string(),
            content: text_parts.join("\n"),
            ts,
            id,
            message_type: Some("text".into()),
            blocks,
            model,
        });
    }
    Ok(out)
}

fn load_dsh_messages(path: &Path) -> Result<Vec<SessionMessage>, String> {
    use std::io::BufRead;
    let file = std::fs::File::open(path)
        .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    let mut out: Vec<SessionMessage> = Vec::new();
    let mut pending_tools: HashMap<String, (usize, usize)> = HashMap::new();

    for line in std::io::BufReader::new(file).lines().map_while(Result::ok) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(val) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        let ev = val.get("type").and_then(Value::as_str).unwrap_or("");
        let ts = val.get("time").and_then(parse_ts_value);
        let append_ok = match val.get("surfaceOp") {
            None => true,
            Some(Value::String(op)) => op == "append",
            _ => false,
        };
        match ev {
            "user/message" if append_ok => {
                if val.pointer("/data/source/kind").and_then(Value::as_str) != Some("user") {
                    continue;
                }
                let text = val
                    .pointer("/data/content")
                    .map(content_text)
                    .unwrap_or_default();
                if text.trim().is_empty() {
                    continue;
                }
                out.push(SessionMessage {
                    role: "user".into(),
                    content: text.clone(),
                    ts,
                    id: None,
                    message_type: Some("text".into()),
                    blocks: vec![SessionMessageBlock {
                        kind: "text".into(),
                        text: Some(text),
                        ..Default::default()
                    }],
                    model: None,
                });
            }
            "assistant/message" if append_ok => {
                let Some(data) = val.get("data") else {
                    continue;
                };
                let msg = data
                    .get("message")
                    .filter(|v| v.is_object())
                    .unwrap_or(data);
                let mut blocks = Vec::new();
                let mut text_parts = Vec::new();
                if let Some(items) = msg.get("content").and_then(Value::as_array) {
                    for item in items {
                        let kind = item.get("type").and_then(Value::as_str).unwrap_or("");
                        let txt = item.get("text").and_then(Value::as_str).unwrap_or("");
                        if txt.trim().is_empty() {
                            continue;
                        }
                        if kind == "reasoning" {
                            blocks.push(SessionMessageBlock {
                                kind: "thinking".into(),
                                text: Some(txt.to_string()),
                                title: Some("Thinking".into()),
                                ..Default::default()
                            });
                        } else if kind == "text" {
                            text_parts.push(txt.to_string());
                            blocks.push(SessionMessageBlock {
                                kind: "text".into(),
                                text: Some(txt.to_string()),
                                ..Default::default()
                            });
                        }
                    }
                }
                if blocks.is_empty() {
                    continue;
                }
                out.push(SessionMessage {
                    role: "assistant".into(),
                    content: text_parts.join("\n"),
                    ts,
                    id: None,
                    message_type: Some("text".into()),
                    blocks,
                    model: msg
                        .pointer("/source/model")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                });
            }
            "tool/call" => {
                let Some(data) = val.get("data") else {
                    continue;
                };
                let name = data.get("name").and_then(Value::as_str).unwrap_or("tool");
                let call_id = data.get("callId").and_then(Value::as_str).unwrap_or("");
                let args = data
                    .get("arguments")
                    .and_then(|a| {
                        a.as_str()
                            .map(str::to_string)
                            .or_else(|| Some(a.to_string()))
                    })
                    .unwrap_or_default();
                let m_idx = out.len();
                if !call_id.is_empty() {
                    pending_tools.insert(call_id.to_string(), (m_idx, 0));
                }
                out.push(SessionMessage {
                    role: "assistant".into(),
                    content: format!("Called {name}: {args}"),
                    ts,
                    id: (!call_id.is_empty()).then(|| call_id.to_string()),
                    message_type: Some("tool_call".into()),
                    blocks: vec![SessionMessageBlock {
                        kind: "tool_call".into(),
                        tool_name: Some(name.to_string()),
                        title: Some(format!("Call {name}")),
                        text: Some(args),
                        ..Default::default()
                    }],
                    model: None,
                });
            }
            "tool/result" => {
                let Some(data) = val.get("data") else {
                    continue;
                };
                let call_id = data.get("callId").and_then(Value::as_str).unwrap_or("");
                let is_err = data.get("isError").and_then(Value::as_bool).unwrap_or(false);
                let output = data
                    .get("output")
                    .or_else(|| data.get("result"))
                    .map(|v| {
                        v.as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| v.to_string())
                    })
                    .unwrap_or_default();
                if let Some(&(m_idx, b_idx)) = pending_tools.get(call_id)
                    && let Some(blk) = out.get_mut(m_idx).and_then(|m| m.blocks.get_mut(b_idx))
                {
                    blk.output = Some(output);
                    blk.is_error = Some(is_err);
                    blk.status = Some(if is_err { "failed".into() } else { "completed".into() });
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

fn load_sqlite_messages(source: &str, provider_id: &str) -> Result<Vec<SessionMessage>, String> {
    let (db_path, session_id) = parse_sqlite_source(source)
        .ok_or_else(|| format!("Invalid SQLite source reference: {source}"))?;
    if !db_path.is_file() {
        return Ok(Vec::new());
    }
    let conn = Connection::open_with_flags(
        &db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("Failed to open SQLite database {}: {e}", db_path.display()))?;

    if provider_id == "hermes" || (sqlite_table_exists(&conn, "messages") && !sqlite_table_exists(&conn, "session_message")) {
        if !sqlite_table_exists(&conn, "messages") {
            return Ok(Vec::new());
        }
        let mut stmt = conn
            .prepare("SELECT role, content, timestamp FROM messages WHERE session_id = ?1 ORDER BY timestamp ASC")
            .map_err(|e| format!("Failed to query Hermes messages: {e}"))?;
        let rows = stmt
            .query_map([session_id.as_str()], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    sqlite_col_as_ts(r, 2),
                ))
            })
            .map_err(|e| format!("Failed to read Hermes messages: {e}"))?;
        let mut out = Vec::new();
        for (role, content, ts) in rows.flatten() {
            if content.trim().is_empty() {
                continue;
            }
            out.push(SessionMessage {
                role,
                content: content.clone(),
                ts,
                id: None,
                message_type: Some("text".into()),
                blocks: vec![SessionMessageBlock {
                    kind: "text".into(),
                    text: Some(content),
                    ..Default::default()
                }],
                model: None,
            });
        }
        return Ok(out);
    }

    // OpenCode V2 (`session_message`) first, fallback V1 (`message` + `part`)
    if sqlite_table_exists(&conn, "session_message") {
        let mut stmt = conn
            .prepare("SELECT id, type, time_created, data FROM session_message WHERE session_id = ?1 ORDER BY seq ASC")
            .map_err(|e| format!("Failed to query OpenCode V2 messages: {e}"))?;
        let rows = stmt
            .query_map([session_id.as_str()], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })
            .map_err(|e| format!("Failed to read OpenCode V2 messages: {e}"))?;
        let mut out = Vec::new();
        for (id, msg_type, ts, raw) in rows.flatten() {
            let Ok(val) = serde_json::from_str::<Value>(&raw) else {
                continue;
            };
            let mut blocks = Vec::new();
            let mut text_parts = Vec::new();
            let role = match msg_type.as_str() {
                "user" => {
                    if let Some(txt) = val.get("text").and_then(Value::as_str).filter(|s| !s.trim().is_empty()) {
                        text_parts.push(txt.to_string());
                        blocks.push(SessionMessageBlock {
                            kind: "text".into(),
                            text: Some(txt.to_string()),
                            ..Default::default()
                        });
                    }
                    "user"
                }
                "assistant" => {
                    if let Some(parts) = val.get("content").and_then(Value::as_array) {
                        for p in parts {
                            let pt = p.get("type").and_then(Value::as_str).unwrap_or("");
                            match pt {
                                "text" => {
                                    if let Some(t) = p.get("text").and_then(Value::as_str).filter(|s| !s.trim().is_empty()) {
                                        text_parts.push(t.to_string());
                                        blocks.push(SessionMessageBlock {
                                            kind: "text".into(),
                                            text: Some(t.to_string()),
                                            ..Default::default()
                                        });
                                    }
                                }
                                "reasoning" => {
                                    if let Some(t) = p.get("text").and_then(Value::as_str).filter(|s| !s.trim().is_empty()) {
                                        blocks.push(SessionMessageBlock {
                                            kind: "thinking".into(),
                                            text: Some(t.to_string()),
                                            title: Some("Thinking".into()),
                                            ..Default::default()
                                        });
                                    }
                                }
                                "tool" => {
                                    let name = p
                                        .get("name")
                                        .or_else(|| p.get("tool"))
                                        .and_then(Value::as_str)
                                        .unwrap_or("tool");
                                    let inp = p
                                        .pointer("/state/input")
                                        .map(|v| v.to_string())
                                        .unwrap_or_default();
                                    let output = p
                                        .pointer("/state/output")
                                        .and_then(Value::as_str)
                                        .map(str::to_string);
                                    blocks.push(SessionMessageBlock {
                                        kind: "tool_call".into(),
                                        tool_name: Some(name.to_string()),
                                        title: Some(format!("Call {name}")),
                                        text: Some(inp),
                                        output,
                                        ..Default::default()
                                    });
                                }
                                _ => {}
                            }
                        }
                    }
                    "assistant"
                }
                "compaction" => {
                    if let Some(sum) = val.get("summary").and_then(Value::as_str).filter(|s| !s.trim().is_empty()) {
                        let txt = format!("[Summary] {sum}");
                        text_parts.push(txt.clone());
                        blocks.push(SessionMessageBlock {
                            kind: "text".into(),
                            text: Some(txt),
                            ..Default::default()
                        });
                    }
                    "system"
                }
                "system" | "synthetic" | "skill" | "shell" => {
                    if let Some(txt) = val.get("text").or_else(|| val.get("command")).and_then(Value::as_str).filter(|s| !s.trim().is_empty()) {
                        text_parts.push(txt.to_string());
                        blocks.push(SessionMessageBlock {
                            kind: "text".into(),
                            text: Some(txt.to_string()),
                            ..Default::default()
                        });
                    }
                    "system"
                }
                _ => continue,
            };
            if blocks.is_empty() {
                continue;
            }
            let model = val
                .pointer("/model/id")
                .or_else(|| val.get("model"))
                .and_then(Value::as_str)
                .map(str::to_string);
            out.push(SessionMessage {
                role: role.into(),
                content: text_parts.join("\n"),
                ts: Some(ts),
                id: Some(id),
                message_type: Some(msg_type),
                blocks,
                model,
            });
        }
        if !out.is_empty() {
            return Ok(out);
        }
    }

    // OpenCode V1 (`message` + `part`)
    if !sqlite_table_exists(&conn, "message") {
        return Ok(Vec::new());
    }
    let mut parts_by_msg: HashMap<String, Vec<Value>> = HashMap::new();
    if sqlite_table_exists(&conn, "part")
        && let Ok(mut pstmt) = conn.prepare(
            "SELECT message_id, data FROM part WHERE session_id = ?1 ORDER BY time_created ASC",
        )
        && let Ok(prows) = pstmt.query_map([session_id.as_str()], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
    {
        for (mid, raw) in prows.flatten() {
            if let Ok(pv) = serde_json::from_str::<Value>(&raw) {
                parts_by_msg.entry(mid).or_default().push(pv);
            }
        }
    }
    let mut mstmt = conn
        .prepare("SELECT id, time_created, data FROM message WHERE session_id = ?1 ORDER BY time_created ASC")
        .map_err(|e| format!("Failed to query OpenCode messages: {e}"))?;
    let mrows = mstmt
        .query_map([session_id.as_str()], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?))
        })
        .map_err(|e| format!("Failed to read OpenCode messages: {e}"))?;
    let mut out = Vec::new();
    for (mid, ts, raw) in mrows.flatten() {
        let Ok(val) = serde_json::from_str::<Value>(&raw) else {
            continue;
        };
        let role = val.get("role").and_then(Value::as_str).unwrap_or("unknown");
        let model = val
            .get("modelID")
            .or_else(|| val.get("model"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let mut blocks = Vec::new();
        let mut text_parts = Vec::new();
        if let Some(parts) = parts_by_msg.get(&mid) {
            for p in parts {
                match p.get("type").and_then(Value::as_str).unwrap_or("") {
                    "text" => {
                        if let Some(t) = p.get("text").and_then(Value::as_str).filter(|s| !s.trim().is_empty()) {
                            text_parts.push(t.to_string());
                            blocks.push(SessionMessageBlock {
                                kind: "text".into(),
                                text: Some(t.to_string()),
                                ..Default::default()
                            });
                        }
                    }
                    "reasoning" => {
                        if let Some(t) = p.get("text").and_then(Value::as_str).filter(|s| !s.trim().is_empty()) {
                            blocks.push(SessionMessageBlock {
                                kind: "thinking".into(),
                                text: Some(t.to_string()),
                                title: Some("Thinking".into()),
                                ..Default::default()
                            });
                        }
                    }
                    "tool" => {
                        let name = p.get("tool").and_then(Value::as_str).unwrap_or("tool");
                        let inp = p
                            .pointer("/state/input")
                            .map(|v| v.to_string())
                            .unwrap_or_default();
                        let output = p
                            .pointer("/state/output")
                            .and_then(Value::as_str)
                            .map(str::to_string);
                        blocks.push(SessionMessageBlock {
                            kind: "tool_call".into(),
                            tool_name: Some(name.to_string()),
                            title: Some(format!("Call {name}")),
                            text: Some(inp),
                            output,
                            ..Default::default()
                        });
                    }
                    _ => {}
                }
            }
        }
        if blocks.is_empty() {
            continue;
        }
        out.push(SessionMessage {
            role: role.to_string(),
            content: text_parts.join("\n"),
            ts: Some(ts),
            id: Some(mid),
            message_type: Some("text".into()),
            blocks,
            model,
        });
    }
    Ok(out)
}

fn load_opencode_json_messages(
    session_file: &Path,
    session_id: &str,
) -> Result<Vec<SessionMessage>, String> {
    // storage/session/<proj>/<ses_xxx.json> -> storage/message/<ses_xxx>/*.json
    let storage_root = session_file
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent);
    let Some(msg_dir) = storage_root.map(|r| r.join("message").join(session_id)) else {
        return Ok(Vec::new());
    };
    if !msg_dir.is_dir() {
        return Ok(Vec::new());
    }
    let part_root = storage_root.map(|r| r.join("part"));
    let mut msg_files = Vec::new();
    collect_json_files(&msg_dir, &mut msg_files);
    let mut parsed_msgs: Vec<(i64, Value)> = Vec::new();
    for mf in msg_files {
        if let Ok(raw) = std::fs::read_to_string(&mf)
            && let Ok(val) = serde_json::from_str::<Value>(&raw)
        {
            let created = val
                .pointer("/time/created")
                .and_then(parse_ts_value)
                .unwrap_or(0);
            parsed_msgs.push((created, val));
        }
    }
    parsed_msgs.sort_by_key(|(ts, _)| *ts);
    let mut out = Vec::new();
    for (ts, val) in parsed_msgs {
        let Some(mid) = val.get("id").and_then(Value::as_str) else {
            continue;
        };
        let role = val.get("role").and_then(Value::as_str).unwrap_or("unknown");
        let model = val
            .get("modelID")
            .or_else(|| val.get("model"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let mut blocks = Vec::new();
        let mut text_parts = Vec::new();
        if let Some(pdir) = part_root.as_ref().map(|r| r.join(mid))
            && pdir.is_dir()
        {
            let mut pfiles = Vec::new();
            collect_json_files(&pdir, &mut pfiles);
            pfiles.sort();
            for pf in pfiles {
                if let Ok(praw) = std::fs::read_to_string(&pf)
                    && let Ok(pv) = serde_json::from_str::<Value>(&praw)
                    && pv.get("type").and_then(Value::as_str) == Some("text")
                    && let Some(t) = pv.get("text").and_then(Value::as_str).filter(|s| !s.trim().is_empty())
                {
                    text_parts.push(t.to_string());
                    blocks.push(SessionMessageBlock {
                        kind: "text".into(),
                        text: Some(t.to_string()),
                        ..Default::default()
                    });
                }
            }
        }
        if blocks.is_empty() {
            continue;
        }
        out.push(SessionMessage {
            role: role.to_string(),
            content: text_parts.join("\n"),
            ts: Some(ts),
            id: Some(mid.to_string()),
            message_type: Some("text".into()),
            blocks,
            model,
        });
    }
    Ok(out)
}

fn load_jsonl_messages(path: &Path) -> Result<Vec<SessionMessage>, String> {
    use std::io::BufRead;
    let file = std::fs::File::open(path)
        .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    let reader = std::io::BufReader::new(file);

    let is_ag_transcript = matches!(
        path.file_name().and_then(|n| n.to_str()),
        Some("transcript_full.jsonl" | "transcript.jsonl")
    );

    let values: Vec<Value> = if is_ag_transcript {
        // Antigravity transcripts can rewrite the same `step_index` as a step progresses.
        let mut by_step: std::collections::BTreeMap<u64, Value> = std::collections::BTreeMap::new();
        let mut unindexed: Vec<Value> = Vec::new();
        for line in reader.lines().map_while(Result::ok) {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
                continue;
            };
            if let Some(step_idx) = value.get("step_index").and_then(Value::as_u64) {
                by_step.insert(step_idx, value);
            } else {
                unindexed.push(value);
            }
        }
        let mut merged: Vec<Value> = by_step.into_values().collect();
        merged.extend(unindexed);
        merged
    } else {
        reader
            .lines()
            .map_while(Result::ok)
            .filter_map(|line| {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    serde_json::from_str::<Value>(trimmed).ok()
                }
            })
            .collect()
    };

    let mut out: Vec<SessionMessage> = vec![];
    let mut tool_use_map: HashMap<String, (usize, usize)> = HashMap::new();

    for value in values {

        // 1. Filter out internal noise / meta events
        if let Some(type_str) = value.get("type").and_then(Value::as_str) {
            match type_str {
                "CHECKPOINT" | "mode" | "permission-mode" | "atis-latch" | "file-history-snapshot"
                | "attachment" | "last-prompt" | "turn_context" | "session_meta"
                | "session" | "session_info" => {
                    continue;
                }
                _ => {}
            }
        }

        // 2. Extract timestamp
        let ts = value
            .get("timestamp")
            .or_else(|| value.get("ts"))
            .or_else(|| value.get("created_at"))
            .and_then(|v| {
                if let Some(num) = v.as_i64() {
                    Some(num)
                } else if let Some(s) = v.as_str() {
                    chrono::DateTime::parse_from_rfc3339(s)
                        .ok()
                        .map(|dt| dt.timestamp_millis())
                } else {
                    None
                }
            });

        // 2b. Antigravity USER_INPUT
        if value.get("type").and_then(Value::as_str) == Some("USER_INPUT") {
            let content_raw = value.get("content").and_then(Value::as_str).unwrap_or("");
            let clean_text = clean_antigravity_user_content(content_raw);
            if !clean_text.is_empty() {
                out.push(SessionMessage {
                    role: "user".to_string(),
                    content: clean_text.clone(),
                    ts,
                    id: None,
                    message_type: Some("text".into()),
                    blocks: vec![SessionMessageBlock {
                        kind: "text".into(),
                        text: Some(clean_text),
                        ..Default::default()
                    }],
                    model: None,
                });
            }
            continue;
        }

        // 2c. Antigravity PLANNER_RESPONSE
        if value.get("type").and_then(Value::as_str) == Some("PLANNER_RESPONSE") {
            let mut blocks = vec![];
            let content = value.get("content").and_then(Value::as_str).unwrap_or("").to_string();

            if let Some(thinking) = value.get("thinking").and_then(Value::as_str) {
                if !thinking.trim().is_empty() {
                    blocks.push(SessionMessageBlock {
                        kind: "thinking".into(),
                        text: Some(thinking.to_string()),
                        title: Some("Thinking".into()),
                        ..Default::default()
                    });
                }
            }

            if !content.trim().is_empty() {
                blocks.push(SessionMessageBlock {
                    kind: "text".into(),
                    text: Some(content.clone()),
                    ..Default::default()
                });
            }

            if let Some(tool_calls) = value.get("tool_calls").and_then(Value::as_array) {
                for tc in tool_calls {
                    let name = tc.get("name").and_then(Value::as_str).unwrap_or("tool");
                    let args = tc.get("args").map(|a| a.to_string()).unwrap_or_default();
                    let is_cmd = name.eq_ignore_ascii_case("run_command")
                        || name.eq_ignore_ascii_case("bash")
                        || name.eq_ignore_ascii_case("exec");
                    blocks.push(SessionMessageBlock {
                        kind: if is_cmd { "command".into() } else { "tool_call".into() },
                        text: Some(args.clone()),
                        command: if is_cmd { Some(args) } else { None },
                        tool_name: Some(name.to_string()),
                        title: Some(format!("Call {name}")),
                        ..Default::default()
                    });
                }
            }

            if !blocks.is_empty() || !content.is_empty() {
                let msg_type = if blocks.iter().all(|b| b.kind == "thinking") {
                    "thinking"
                } else {
                    "text"
                };
                out.push(SessionMessage {
                    role: "assistant".to_string(),
                    content,
                    ts,
                    id: None,
                    message_type: Some(msg_type.into()),
                    blocks,
                    model: None,
                });
            }
            continue;
        }

        // 2d. Antigravity GENERIC (Tool execution results)
        if value.get("type").and_then(Value::as_str) == Some("GENERIC") {
            let content = value.get("content").and_then(Value::as_str).unwrap_or("");
            let mut attached = false;
            if let Some(last_msg) = out.last_mut() {
                if last_msg.role == "assistant" {
                    if let Some(blk) = last_msg
                        .blocks
                        .iter_mut()
                        .find(|b| (b.kind == "command" || b.kind == "tool_call") && b.output.is_none())
                    {
                        blk.output = Some(content.to_string());
                        blk.status = Some("completed".into());
                        attached = true;
                    }
                }
            }
            if !attached && !content.trim().is_empty() {
                out.push(SessionMessage {
                    role: "tool".to_string(),
                    content: content.to_string(),
                    ts,
                    id: None,
                    message_type: Some("tool_call".into()),
                    blocks: vec![SessionMessageBlock {
                        kind: "tool_result".into(),
                        text: Some(content.to_string()),
                        output: Some(content.to_string()),
                        status: Some("completed".into()),
                        ..Default::default()
                    }],
                    model: None,
                });
            }
            continue;
        }

        // 2e. Antigravity SYSTEM_MESSAGE
        if value.get("type").and_then(Value::as_str) == Some("SYSTEM_MESSAGE") {
            let content = value.get("content").and_then(Value::as_str).unwrap_or("");
            if !content.trim().is_empty() {
                out.push(SessionMessage {
                    role: "system".to_string(),
                    content: content.to_string(),
                    ts,
                    id: None,
                    message_type: Some("text".into()),
                    blocks: vec![SessionMessageBlock {
                        kind: "text".into(),
                        text: Some(content.to_string()),
                        ..Default::default()
                    }],
                    model: None,
                });
            }
            continue;
        }

        // 3. Codex "response_item"
        if value.get("type").and_then(Value::as_str) == Some("response_item") {
            if let Some(payload) = value.get("payload") {
                let p_type = payload.get("type").and_then(Value::as_str).unwrap_or("");
                if p_type == "message" {
                    let role = payload.get("role").and_then(Value::as_str).unwrap_or("user");
                    let model = payload.get("model").and_then(Value::as_str).map(String::from);
                    let mut blocks = vec![];
                    let mut content = String::new();

                    if let Some(reasoning) = payload.get("reasoning_content").and_then(Value::as_str) {
                        if !reasoning.trim().is_empty() {
                            blocks.push(SessionMessageBlock {
                                kind: "thinking".into(),
                                text: Some(reasoning.to_string()),
                                title: Some("Thinking".into()),
                                ..Default::default()
                            });
                        }
                    }

                    if let Some(content_val) = payload.get("content") {
                        if let Some(s) = content_val.as_str() {
                            content.push_str(s);
                            blocks.push(SessionMessageBlock {
                                kind: "text".into(),
                                text: Some(s.to_string()),
                                ..Default::default()
                            });
                        } else if let Some(arr) = content_val.as_array() {
                            for item in arr {
                                let item_type = item.get("type").and_then(Value::as_str).unwrap_or("");
                                if item_type == "thinking" || item_type == "reasoning" {
                                    if let Some(t) = item.get("thinking").or_else(|| item.get("text")).and_then(Value::as_str) {
                                        blocks.push(SessionMessageBlock {
                                            kind: "thinking".into(),
                                            text: Some(t.to_string()),
                                            title: Some("Thinking".into()),
                                            ..Default::default()
                                        });
                                    }
                                } else if item_type == "text" || item_type == "input_text" {
                                    if let Some(t) = item.get("text").and_then(Value::as_str) {
                                        if !content.is_empty() {
                                            content.push('\n');
                                        }
                                        content.push_str(t);
                                        blocks.push(SessionMessageBlock {
                                            kind: "text".into(),
                                            text: Some(t.to_string()),
                                            ..Default::default()
                                        });
                                    }
                                }
                            }
                        }
                    }

                    if !blocks.is_empty() || !content.is_empty() {
                        let msg_type = if blocks.iter().any(|b| b.kind == "thinking") && blocks.iter().all(|b| b.kind == "thinking") {
                            "thinking"
                        } else {
                            "text"
                        };
                        out.push(SessionMessage {
                            role: role.to_string(),
                            content,
                            ts,
                            id: None,
                            message_type: Some(msg_type.to_string()),
                            blocks,
                            model,
                        });
                    }
                    continue;
                } else if p_type == "function_call" {
                    let fn_name = payload.get("name").and_then(Value::as_str).unwrap_or("tool");
                    let call_id = payload.get("call_id").and_then(Value::as_str).unwrap_or("");
                    let args = payload.get("arguments").and_then(Value::as_str).unwrap_or("");
                    let is_cmd = fn_name.eq_ignore_ascii_case("bash")
                        || fn_name.eq_ignore_ascii_case("exec")
                        || fn_name.eq_ignore_ascii_case("shell")
                        || fn_name.eq_ignore_ascii_case("command");

                    let block = SessionMessageBlock {
                        kind: if is_cmd { "command".into() } else { "tool_call".into() },
                        text: Some(args.to_string()),
                        command: if is_cmd { Some(args.to_string()) } else { None },
                        tool_name: Some(fn_name.to_string()),
                        title: Some(format!("Call {fn_name}")),
                        ..Default::default()
                    };

                    let msg_idx = out.len();
                    if !call_id.is_empty() {
                        tool_use_map.insert(call_id.to_string(), (msg_idx, 0));
                    }

                    out.push(SessionMessage {
                        role: "assistant".to_string(),
                        content: format!("Called {fn_name}: {args}"),
                        ts,
                        id: if call_id.is_empty() { None } else { Some(call_id.to_string()) },
                        message_type: Some(if is_cmd { "command".into() } else { "tool_call".into() }),
                        blocks: vec![block],
                        model: None,
                    });
                    continue;
                } else if p_type == "function_call_output" {
                    let call_id = payload.get("call_id").and_then(Value::as_str).unwrap_or("");
                    let output = payload.get("output").and_then(Value::as_str).unwrap_or("");
                    if let Some(&(m_idx, b_idx)) = tool_use_map.get(call_id) {
                        if let Some(msg) = out.get_mut(m_idx) {
                            if let Some(blk) = msg.blocks.get_mut(b_idx) {
                                blk.output = Some(output.to_string());
                                blk.status = Some("completed".into());
                                continue;
                            }
                        }
                    }
                    out.push(SessionMessage {
                        role: "tool".to_string(),
                        content: output.to_string(),
                        ts,
                        id: Some(call_id.to_string()),
                        message_type: Some("tool_call".into()),
                        blocks: vec![SessionMessageBlock {
                            kind: "tool_result".into(),
                            text: Some(output.to_string()),
                            output: Some(output.to_string()),
                            status: Some("completed".into()),
                            ..Default::default()
                        }],
                        model: None,
                    });
                    continue;
                }
            }
        }

        // 3b. Kimi wire.jsonl wrapper (`{"message":{"type":"TurnBegin"|"ContentPart", ...}}`)
        if let Some(wire_type) = value.pointer("/message/type").and_then(Value::as_str) {
            match wire_type {
                "TurnBegin" => {
                    if let Some(txt) = value
                        .pointer("/message/payload/user_input")
                        .and_then(Value::as_str)
                        .filter(|s| !s.trim().is_empty())
                    {
                        out.push(SessionMessage {
                            role: "user".into(),
                            content: txt.to_string(),
                            ts,
                            id: None,
                            message_type: Some("text".into()),
                            blocks: vec![SessionMessageBlock {
                                kind: "text".into(),
                                text: Some(txt.to_string()),
                                ..Default::default()
                            }],
                            model: None,
                        });
                    }
                    continue;
                }
                "ContentPart" => {
                    let payload = value.pointer("/message/payload");
                    let part_type = payload
                        .and_then(|p| p.get("type"))
                        .and_then(Value::as_str)
                        .unwrap_or("text");
                    let txt = payload
                        .and_then(|p| p.get("text").or_else(|| p.get("think")))
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    if !txt.trim().is_empty() {
                        let is_think = part_type == "think";
                        out.push(SessionMessage {
                            role: "assistant".into(),
                            content: if is_think { String::new() } else { txt.to_string() },
                            ts,
                            id: None,
                            message_type: Some(if is_think { "thinking".into() } else { "text".into() }),
                            blocks: vec![SessionMessageBlock {
                                kind: if is_think { "thinking".into() } else { "text".into() },
                                text: Some(txt.to_string()),
                                title: is_think.then(|| "Thinking".into()),
                                ..Default::default()
                            }],
                            model: None,
                        });
                    }
                    continue;
                }
                "StatusUpdate" | "StepBegin" | "CompactionBegin" | "CompactionEnd" => continue,
                _ => {}
            }
        }

        // 4. Claude Code / Pi / Generic JSONL parsing
        let role = value
            .pointer("/message/role")
            .and_then(Value::as_str)
            .or_else(|| value.get("role").and_then(Value::as_str))
            .or_else(|| {
                let t = value.get("type").and_then(Value::as_str)?;
                if t == "user" || t == "assistant" || t == "tool" {
                    Some(t)
                } else {
                    None
                }
            })
            .unwrap_or("");

        // Handle Pi toolResult message
        if role.eq_ignore_ascii_case("toolResult") {
            let tool_call_id = value
                .pointer("/message/toolCallId")
                .or_else(|| value.get("tool_call_id"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let tool_name = value
                .pointer("/message/toolName")
                .or_else(|| value.get("tool_name"))
                .and_then(Value::as_str)
                .unwrap_or("tool");
            let is_err = value
                .pointer("/message/isError")
                .or_else(|| value.get("is_error"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let res_content = value
                .pointer("/message/content")
                .map(content_text)
                .unwrap_or_else(|| extract_text(&value));

            let mut attached = false;
            if !tool_call_id.is_empty() {
                if let Some(&(m_idx, b_idx)) = tool_use_map.get(tool_call_id) {
                    if let Some(msg) = out.get_mut(m_idx) {
                        if let Some(blk) = msg.blocks.get_mut(b_idx) {
                            blk.output = Some(res_content.clone());
                            blk.is_error = Some(is_err);
                            blk.status = Some(if is_err { "failed".into() } else { "completed".into() });
                            attached = true;
                        }
                    }
                }
            }

            if !attached {
                out.push(SessionMessage {
                    role: "tool".to_string(),
                    content: res_content.clone(),
                    ts,
                    id: if tool_call_id.is_empty() { None } else { Some(tool_call_id.to_string()) },
                    message_type: Some("tool_call".into()),
                    blocks: vec![SessionMessageBlock {
                        kind: "tool_result".into(),
                        text: Some(res_content.clone()),
                        output: Some(res_content),
                        tool_name: Some(tool_name.to_string()),
                        is_error: Some(is_err),
                        status: Some(if is_err { "failed".into() } else { "completed".into() }),
                        ..Default::default()
                    }],
                    model: None,
                });
            }
            continue;
        }

        let model = value
            .get("model")
            .or_else(|| value.pointer("/message/model"))
            .and_then(Value::as_str)
            .map(String::from);

        let msg_id = value
            .get("uuid")
            .or_else(|| value.get("id"))
            .or_else(|| value.pointer("/message/id"))
            .and_then(Value::as_str)
            .map(String::from);

        let mut blocks: Vec<SessionMessageBlock> = vec![];
        let mut content = String::new();

        let content_arr = value
            .pointer("/message/content")
            .or_else(|| value.get("content"))
            .and_then(Value::as_array);

        if let Some(items) = content_arr {
            let mut is_pure_tool_result = true;
            for item in items {
                let item_type = item.get("type").and_then(Value::as_str).unwrap_or("");
                match item_type {
                    "thinking" => {
                        is_pure_tool_result = false;
                        if let Some(th) = item.get("thinking").and_then(Value::as_str) {
                            blocks.push(SessionMessageBlock {
                                kind: "thinking".into(),
                                text: Some(th.to_string()),
                                title: Some("Thinking".into()),
                                ..Default::default()
                            });
                        }
                    }
                    "text" => {
                        is_pure_tool_result = false;
                        if let Some(txt) = item.get("text").and_then(Value::as_str) {
                            if !content.is_empty() {
                                content.push('\n');
                            }
                            content.push_str(txt);
                            blocks.push(SessionMessageBlock {
                                kind: "text".into(),
                                text: Some(txt.to_string()),
                                ..Default::default()
                            });
                        }
                    }
                    "tool_use" | "toolCall" => {
                        is_pure_tool_result = false;
                        let tool_name = item.get("name").and_then(Value::as_str).unwrap_or("tool");
                        let tool_id = item.get("id").and_then(Value::as_str).unwrap_or("");
                        let input = item.get("input").or_else(|| item.get("arguments"));
                        let is_cmd = tool_name.eq_ignore_ascii_case("bash")
                            || tool_name.eq_ignore_ascii_case("command")
                            || tool_name.eq_ignore_ascii_case("terminal");

                        let mut cmd_str = None;
                        let mut desc_str = None;
                        let mut param_summary = String::new();

                        if let Some(inp) = input {
                            if let Some(cmd) = inp.get("command").and_then(Value::as_str) {
                                cmd_str = Some(cmd.to_string());
                            }
                            if let Some(desc) = inp.get("description").and_then(Value::as_str) {
                                desc_str = Some(desc.to_string());
                            }
                            if !is_cmd {
                                if let Some(path) = inp.get("file_path").or_else(|| inp.get("path")).and_then(Value::as_str) {
                                    param_summary = path.to_string();
                                } else if let Some(pattern) = inp.get("pattern").and_then(Value::as_str) {
                                    param_summary = format!("pattern: {pattern}");
                                } else if let Some(q) = inp.get("query").and_then(Value::as_str) {
                                    param_summary = q.to_string();
                                } else if let Ok(s) = serde_json::to_string(inp) {
                                    param_summary = s;
                                }
                            }
                        }

                        let block = SessionMessageBlock {
                            kind: if is_cmd { "command".into() } else { "tool_call".into() },
                            text: if is_cmd { cmd_str.clone() } else { Some(param_summary.clone()) },
                            command: cmd_str,
                            tool_name: Some(tool_name.to_string()),
                            title: desc_str.or_else(|| if !param_summary.is_empty() { Some(param_summary) } else { None }),
                            ..Default::default()
                        };

                        let msg_idx = out.len();
                        let blk_idx = blocks.len();
                        if !tool_id.is_empty() {
                            tool_use_map.insert(tool_id.to_string(), (msg_idx, blk_idx));
                        }
                        blocks.push(block);
                    }
                    "tool_result" => {
                        let tool_use_id = item.get("tool_use_id").and_then(Value::as_str).unwrap_or("");
                        let res_content = extract_text(item);
                        let is_err = item.get("is_error").and_then(Value::as_bool).unwrap_or(false);

                        let mut stdout = value.pointer("/toolUseResult/stdout").and_then(Value::as_str).map(String::from);
                        let stderr = value.pointer("/toolUseResult/stderr").and_then(Value::as_str).map(String::from);
                        if stdout.is_none() && !res_content.is_empty() {
                            stdout = Some(res_content.clone());
                        }

                        let mut attached = false;
                        if let Some(&(m_idx, b_idx)) = tool_use_map.get(tool_use_id) {
                            if let Some(msg) = out.get_mut(m_idx) {
                                if let Some(blk) = msg.blocks.get_mut(b_idx) {
                                    let full_output = match (&stdout, &stderr) {
                                        (Some(o), Some(e)) if !e.is_empty() => format!("{o}\n[stderr]: {e}"),
                                        (Some(o), _) => o.clone(),
                                        (None, Some(e)) => format!("[stderr]: {e}"),
                                        _ => res_content.clone(),
                                    };
                                    blk.output = Some(full_output);
                                    blk.is_error = Some(is_err);
                                    blk.status = Some(if is_err { "failed".into() } else { "completed".into() });
                                    attached = true;
                                }
                            }
                        }

                        if !attached {
                            blocks.push(SessionMessageBlock {
                                kind: "tool_result".into(),
                                text: stdout.or(Some(res_content)),
                                is_error: Some(is_err),
                                status: Some(if is_err { "failed".into() } else { "completed".into() }),
                                ..Default::default()
                            });
                        }
                    }
                    _ => {}
                }
            }

            if is_pure_tool_result && blocks.is_empty() {
                continue;
            }
        } else {
            let text = extract_text(&value);
            if !text.trim().is_empty() {
                content = text.clone();
                blocks.push(SessionMessageBlock {
                    kind: "text".into(),
                    text: Some(text),
                    ..Default::default()
                });
            }
        }

        if blocks.is_empty() && content.trim().is_empty() {
            continue;
        }

        let message_type = if blocks.iter().any(|b| b.kind == "command") {
            "command"
        } else if blocks.iter().any(|b| b.kind == "tool_call" || b.kind == "tool_result") {
            "tool_call"
        } else if blocks.iter().any(|b| b.kind == "thinking") && blocks.iter().all(|b| b.kind == "thinking") {
            "thinking"
        } else {
            "text"
        };

        let final_role = if role == "assistant" {
            "assistant"
        } else if role == "user" {
            "user"
        } else if role == "system" {
            "system"
        } else if role == "tool" {
            "tool"
        } else if blocks.iter().any(|b| b.kind == "command" || b.kind == "tool_call") {
            "assistant"
        } else {
            "user"
        };

        out.push(SessionMessage {
            role: final_role.to_string(),
            content,
            ts,
            id: msg_id,
            message_type: Some(message_type.to_string()),
            blocks,
            model,
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

/// Delete the session (file or whole directory for Grok-style sessions),
/// along with any toolbox-owned title sidecar file.
pub fn delete_session(meta: &SessionMeta) -> Result<(), String> {
    if let Some((db_path, session_id)) = parse_sqlite_source(&meta.source_path) {
        if !db_path.is_file() {
            return Ok(());
        }
        let mut conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        for sql in [
            "DELETE FROM part WHERE session_id = ?1",
            "DELETE FROM session_message WHERE session_id = ?1",
            "DELETE FROM message WHERE session_id = ?1",
            "DELETE FROM messages WHERE session_id = ?1",
            "DELETE FROM session_share WHERE session_id = ?1",
            "DELETE FROM session WHERE id = ?1",
            "DELETE FROM session_v2 WHERE id = ?1",
            "DELETE FROM sessions WHERE id = ?1",
        ] {
            let _ = tx.execute(sql, [session_id.as_str()]);
        }
        tx.commit().map_err(|e| e.to_string())?;
        return Ok(());
    }
    let path = PathBuf::from(&meta.source_path);
    if path.is_dir() {
        std::fs::remove_dir_all(&path).map_err(|e| e.to_string())
    } else if path.is_file() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        let sidecar = path.with_extension("toolbox-title.txt");
        if sidecar.is_file() {
            let _ = std::fs::remove_file(&sidecar);
        }
        Ok(())
    } else {
        Ok(())
    }
}

/// Filter sessions whose last active timestamp (falling back to creation time)
/// is older than `older_than_days` relative to `now_ms`.
///
/// Sessions without any known timestamp are preserved to avoid accidental deletion.
pub fn filter_stale_sessions(
    sessions: &[SessionMeta],
    older_than_days: u32,
    now_ms: i64,
) -> Vec<SessionMeta> {
    let cutoff_ms = now_ms - (older_than_days as i64).saturating_mul(86_400_000);
    sessions
        .iter()
        .filter(|s| {
            s.last_active_at
                .or(s.created_at)
                .is_some_and(|ts| ts < cutoff_ms)
        })
        .cloned()
        .collect()
}

/// Delete multiple sessions in a batch and invalidate the session cache.
///
/// Returns `(deleted_count, failed_count)`.
pub fn delete_sessions_batch(sessions: &[SessionMeta]) -> (usize, usize) {
    let mut deleted = 0usize;
    let mut failed = 0usize;
    for meta in sessions {
        match delete_session(meta) {
            Ok(()) => deleted += 1,
            Err(_) => failed += 1,
        }
    }
    if deleted > 0 {
        invalidate_cache();
    }
    (deleted, failed)
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

pub fn cached_all_if_fresh(tool: ToolId) -> Option<Vec<SessionMeta>> {
    let key = format!("{}:all", tool.key());
    let guard = CACHE.lock().ok()?;
    let entry = guard.as_ref()?.get(&key)?;
    (entry.created.elapsed().as_secs() < 15).then(|| entry.sessions.clone())
}

pub fn store_full_list_cache(tool: ToolId, sessions: Vec<SessionMeta>) {
    let Ok(mut guard) = CACHE.lock() else {
        return;
    };
    let map = guard.get_or_insert_with(HashMap::new);
    map.insert(
        format!("{}:all", tool.key()),
        CacheEntry {
            created: std::time::Instant::now(),
            sessions,
        },
    );
    while map.len() > 16 {
        if let Some(oldest) = map
            .iter()
            .min_by_key(|(_, entry)| entry.created)
            .map(|(key, _)| key.clone())
        {
            map.remove(&oldest);
        } else {
            break;
        }
    }
}

pub fn cached_scan_all(paths: &Paths, tool: ToolId) -> Vec<SessionMeta> {
    if let Some(hit) = cached_all_if_fresh(tool) {
        return hit;
    }
    let sessions = scan_all_sessions(paths, tool);
    store_full_list_cache(tool, sessions.clone());
    sessions
}

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
        invalidate_cache();
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

        let sessions = scan_sessions(&paths, ToolId::ClaudeCode, 50);
        assert_eq!(sessions.len(), 1);
        let mut scan = SessionScan::open(&paths, ToolId::ClaudeCode);
        let first = scan.next_batch(1).unwrap();
        assert_eq!(first.len(), 1);
        assert!(scan.next_batch(1).is_none());
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

    #[test]
    fn test_rich_session_parsing() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("rich_session.jsonl");
        let content = r#"
{"type":"mode","mode":"normal"}
{"type":"assistant","message":{"model":"claude-3-7-sonnet","role":"assistant","content":[{"type":"thinking","thinking":"Analyzing the codebase..."},{"type":"text","text":"I will run git status."},{"type":"tool_use","id":"tool_1","name":"Bash","input":{"command":"git status","description":"Check git status"}}]}}
{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"tool_1","content":"On branch main\nnothing to commit","is_error":false}]},"toolUseResult":{"stdout":"On branch main\nnothing to commit","stderr":""}}
{"type":"assistant","message":{"model":"claude-3-7-sonnet","role":"assistant","content":[{"type":"text","text":"Working tree is clean."}]}}
"#;
        std::fs::write(&file, content).unwrap();
        let meta = SessionMeta {
            provider_id: "claude_code".into(),
            session_id: "test".into(),
            title: None,
            summary: None,
            project_dir: None,
            created_at: None,
            last_active_at: None,
            source_path: file.to_string_lossy().to_string(),
            resume_command: None,
        };
        let paths = paths_with(dir.path());
        let messages = load_messages(&paths, &meta).unwrap();
        // Mode line was skipped!
        // Tool result was attached to the command block!
        assert_eq!(messages.len(), 2);
        
        let msg0 = &messages[0];
        assert_eq!(msg0.role, "assistant");
        assert_eq!(msg0.blocks.len(), 3);
        assert_eq!(msg0.blocks[0].kind, "thinking");
        assert_eq!(msg0.blocks[0].text.as_deref(), Some("Analyzing the codebase..."));
        assert_eq!(msg0.blocks[1].kind, "text");
        assert_eq!(msg0.blocks[2].kind, "command");
        assert_eq!(msg0.blocks[2].command.as_deref(), Some("git status"));
        assert_eq!(msg0.blocks[2].output.as_deref(), Some("On branch main\nnothing to commit"));
        assert_eq!(msg0.blocks[2].status.as_deref(), Some("completed"));

        let msg1 = &messages[1];
        assert_eq!(msg1.role, "assistant");
        assert_eq!(msg1.content, "Working tree is clean.");
    }

    #[test]
    fn filter_stale_sessions_respects_cutoff_and_unknown_timestamps() {
        let now_ms = 1_700_000_000_000_i64;
        let day_ms = 86_400_000_i64;
        let mk = |id: &str, last_active_at: Option<i64>, created_at: Option<i64>| SessionMeta {
            provider_id: "codex".into(),
            session_id: id.into(),
            title: Some(id.into()),
            summary: None,
            project_dir: None,
            created_at,
            last_active_at,
            source_path: format!("/tmp/{id}.jsonl"),
            resume_command: None,
        };

        let sessions = vec![
            mk("old-91d", Some(now_ms - 91 * day_ms), None),
            mk("exact-90d", Some(now_ms - 90 * day_ms), None),
            mk("fresh-89d", Some(now_ms - 89 * day_ms), None),
            mk("created-100d", None, Some(now_ms - 100 * day_ms)),
            mk("unknown-ts", None, None),
        ];

        let stale = filter_stale_sessions(&sessions, 90, now_ms);
        let ids: Vec<&str> = stale.iter().map(|s| s.session_id.as_str()).collect();
        assert_eq!(ids, vec!["old-91d", "created-100d"]);
    }

    #[test]
    fn delete_sessions_batch_removes_files_and_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let file1 = dir.path().join("s1.jsonl");
        let file2 = dir.path().join("s2.jsonl");
        std::fs::write(&file1, "{}\n").unwrap();
        std::fs::write(&file2, "{}\n").unwrap();

        let mk = |id: &str, p: &std::path::Path| SessionMeta {
            provider_id: "claude_code".into(),
            session_id: id.into(),
            title: None,
            summary: None,
            project_dir: None,
            created_at: None,
            last_active_at: Some(1_000),
            source_path: p.to_string_lossy().to_string(),
            resume_command: None,
        };

        let m1 = mk("s1", &file1);
        rename_session(&m1, "Custom Title").unwrap();
        let sidecar = file1.with_extension("toolbox-title.txt");
        assert!(sidecar.exists());

        let m2 = mk("s2", &file2);
        let (deleted, failed) = delete_sessions_batch(&[m1, m2]);
        assert_eq!((deleted, failed), (2, 0));
        assert!(!file1.exists());
        assert!(!sidecar.exists());
        assert!(!file2.exists());
    }

    #[test]
    fn resume_commands_match_cli_contracts_across_all_tools() {
        assert_eq!(
            resume_command(ToolId::ClaudeCode, "cid-1", None).as_deref(),
            Some("claude --resume cid-1")
        );
        assert_eq!(
            resume_command(ToolId::Codex, "codex-1", None).as_deref(),
            Some("codex resume codex-1")
        );
        assert_eq!(
            resume_command(ToolId::GeminiCli, "gem-1", None).as_deref(),
            Some("gemini --resume gem-1")
        );
        assert_eq!(
            resume_command(ToolId::Grok, "grok-1", None).as_deref(),
            Some("grok --resume grok-1")
        );
        assert_eq!(
            resume_command(ToolId::Kimi, "kimi-1", None).as_deref(),
            Some("kimi --session kimi-1")
        );
        assert_eq!(
            resume_command(ToolId::OpenCode, "ses_abc", None).as_deref(),
            Some("opencode -s ses_abc")
        );
        assert_eq!(resume_command(ToolId::OpenClaw, "claw-1", None), None);
        assert_eq!(
            resume_command(ToolId::Pi, "pi-1", Some("/tmp/s.jsonl")).as_deref(),
            Some("pi --session \"/tmp/s.jsonl\"")
        );
        assert_eq!(
            resume_command(ToolId::Pi, "pi-1", None).as_deref(),
            Some("pi --session pi-1")
        );
        assert_eq!(
            resume_command(ToolId::OhMyPi, "omp-1", Some("/tmp/s.jsonl")).as_deref(),
            Some("omp --resume \"/tmp/s.jsonl\"")
        );
        assert_eq!(
            resume_command(ToolId::OhMyPi, "omp-1", None).as_deref(),
            Some("omp --resume omp-1")
        );
        assert_eq!(resume_command(ToolId::Hermes, "h-1", None), None);
        assert_eq!(resume_command(ToolId::Dsh, "d-1", None), None);
        assert_eq!(resume_command(ToolId::ClaudeDesktop, "cd-1", None), None);
        assert_eq!(resume_command(ToolId::Agents, "a-1", None), None);

        // Kimi wire.jsonl layout uses parent directory name as session ID when JSONL omits id.
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_with(dir.path());
        let kimi_sess = paths
            .home
            .join(".kimi-code")
            .join("sessions")
            .join("proj-hash")
            .join("kimi-uuid-99");
        std::fs::create_dir_all(&kimi_sess).unwrap();
        std::fs::write(
            kimi_sess.join("wire.jsonl"),
            "{\"type\":\"user\",\"content\":\"hello kimi\"}\n",
        )
        .unwrap();
        let kimi_list = scan_sessions(&paths, ToolId::Kimi, 20);
        assert_eq!(kimi_list.len(), 1);
        assert_eq!(kimi_list[0].session_id, "kimi-uuid-99");
        assert_eq!(
            kimi_list[0].resume_command.as_deref(),
            Some("kimi --session kimi-uuid-99")
        );
    }

    #[test]
    fn scan_and_load_updated_agents_formats() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_with(dir.path());

        // 1. OpenClaw under ~/.openclaw/agents/<agentId>/sessions/*.jsonl
        let claw_dir = paths.home.join(".openclaw").join("agents").join("main").join("sessions");
        std::fs::create_dir_all(&claw_dir).unwrap();
        std::fs::write(
            claw_dir.join("claw-1.jsonl"),
            "{\"type\":\"session\",\"id\":\"claw-1\",\"cwd\":\"/proj/claw\",\"timestamp\":1760000000000}\n{\"type\":\"message\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"text\",\"text\":\"check openclaw\"}]}}\n",
        )
        .unwrap();
        let claw_sessions = scan_sessions(&paths, ToolId::OpenClaw, 20);
        assert_eq!(claw_sessions.len(), 1);
        assert_eq!(claw_sessions[0].session_id, "claw-1");
        assert_eq!(claw_sessions[0].resume_command, None);
        assert_eq!(load_messages(&paths, &claw_sessions[0]).unwrap().len(), 1);

        // 2. Gemini CLI: legacy .json AND current .jsonl under ~/.gemini/tmp/<proj>/chats/
        let gem_proj = paths.home.join(".gemini").join("tmp").join("projhash");
        let gem_chats = gem_proj.join("chats");
        std::fs::create_dir_all(&gem_chats).unwrap();
        std::fs::write(gem_proj.join(".project_root"), "/work/gemini-proj\n").unwrap();
        std::fs::write(
            gem_chats.join("session-2026-05-01T10-00-aaaa1111.json"),
            r#"{"sessionId":"gem-json-1","startTime":"2026-05-01T10:00:00Z","lastUpdated":"2026-05-01T10:05:00Z","messages":[{"type":"user","content":"gemini json hello"},{"type":"gemini","content":"gemini json reply"}]}"#,
        )
        .unwrap();
        std::fs::write(
            gem_chats.join("session-2026-05-02T10-00-bbbb2222.jsonl"),
            "{\"sessionId\":\"gem-jsonl-2\",\"startTime\":\"2026-05-02T10:00:00Z\",\"kind\":\"main\"}\n{\"id\":\"m1\",\"type\":\"user\",\"content\":[{\"text\":\"gemini jsonl hi\"}],\"timestamp\":\"2026-05-02T10:01:00Z\"}\n{\"$set\":{\"lastUpdated\":\"2026-05-02T10:02:00Z\"}}\n",
        )
        .unwrap();
        let gem_sessions = scan_sessions(&paths, ToolId::GeminiCli, 20);
        assert_eq!(gem_sessions.len(), 2);
        let gem_j = gem_sessions.iter().find(|s| s.session_id == "gem-jsonl-2").unwrap();
        assert_eq!(gem_j.resume_command.as_deref(), Some("gemini --resume gem-jsonl-2"));
        assert_eq!(gem_j.project_dir.as_deref(), Some("/work/gemini-proj"));
        assert_eq!(load_messages(&paths, gem_j).unwrap().len(), 1);

        // 3. OpenCode SQLite V2 (`session_v2` + `session_message`)
        let oc_dir = paths.home.join(".local").join("share").join("opencode");
        std::fs::create_dir_all(&oc_dir).unwrap();
        let oc_db = oc_dir.join("opencode.db");
        {
            let conn = Connection::open(&oc_db).unwrap();
            conn.execute_batch(
                "CREATE TABLE session_v2 (id TEXT PRIMARY KEY, parent_id TEXT, title TEXT, directory TEXT, time_created INTEGER, time_updated INTEGER);
                 CREATE TABLE session_message (id TEXT PRIMARY KEY, session_id TEXT, seq INTEGER, type TEXT, time_created INTEGER, data TEXT);
                 INSERT INTO session_v2 VALUES ('ses_v2_main', NULL, 'V2 Task', '/work/oc', 1760000001000, 1760000005000);
                 INSERT INTO session_v2 VALUES ('ses_v2_sub', 'ses_v2_main', 'Sub', '/work/oc', 1760000002000, 1760000003000);
                 INSERT INTO session_message VALUES ('m1', 'ses_v2_main', 1, 'user', 1760000001000, '{\"text\":\"hello v2\"}');
                 INSERT INTO session_message VALUES ('m2', 'ses_v2_main', 2, 'assistant', 1760000002000, '{\"model\":{\"id\":\"claude-sonnet-4\"},\"content\":[{\"type\":\"text\",\"text\":\"reply v2\"}]}');",
            )
            .unwrap();
        }
        let oc_sessions = scan_sessions(&paths, ToolId::OpenCode, 20);
        assert_eq!(oc_sessions.len(), 1);
        assert_eq!(oc_sessions[0].session_id, "ses_v2_main");
        assert_eq!(oc_sessions[0].resume_command.as_deref(), Some("opencode -s ses_v2_main"));
        let oc_msgs = load_messages(&paths, &oc_sessions[0]).unwrap();
        assert_eq!(oc_msgs.len(), 2);
        assert_eq!(oc_msgs[1].model.as_deref(), Some("claude-sonnet-4"));

        // 4. Hermes `state.db`
        let hermes_dir = paths.home.join(".hermes");
        std::fs::create_dir_all(&hermes_dir).unwrap();
        {
            let conn = Connection::open(hermes_dir.join("state.db")).unwrap();
            conn.execute_batch(
                "CREATE TABLE sessions (id TEXT PRIMARY KEY, title TEXT, cwd TEXT, started_at INTEGER, ended_at INTEGER);
                 CREATE TABLE messages (session_id TEXT, role TEXT, content TEXT, timestamp INTEGER);
                 INSERT INTO sessions VALUES ('hermes-1', 'Hermes Check', '/work/hermes', 1760000010, 1760000020);
                 INSERT INTO messages VALUES ('hermes-1', 'user', 'hi hermes', 1760000011);",
            )
            .unwrap();
        }
        let hermes_sessions = scan_sessions(&paths, ToolId::Hermes, 20);
        assert_eq!(hermes_sessions.len(), 1);
        assert_eq!(hermes_sessions[0].session_id, "hermes-1");
        assert_eq!(load_messages(&paths, &hermes_sessions[0]).unwrap().len(), 1);

        // 5. Dsh highest generation (`session.v2.jsonl`) & real event types
        let dsh_sess = paths.home.join(".dsh").join("sessions").join("dsh-1");
        std::fs::create_dir_all(&dsh_sess).unwrap();
        std::fs::write(
            dsh_sess.join("session.jsonl"),
            "{\"type\":\"session\",\"id\":\"dsh-1\",\"cwd\":\"/work/dsh\",\"createdAt\":1760000000000}\n{\"type\":\"turn/start\"}\n",
        )
        .unwrap();
        std::fs::write(
            dsh_sess.join("session.v2.jsonl"),
            "{\"type\":\"session\",\"id\":\"dsh-1\",\"cwd\":\"/work/dsh\",\"createdAt\":1760000000000}\n{\"type\":\"turn/start\"}\n{\"type\":\"user/message\",\"surfaceOp\":\"append\",\"time\":1760000001000,\"data\":{\"source\":{\"kind\":\"user\"},\"content\":[{\"type\":\"text\",\"text\":\"hello dsh v2\"}]}}\n{\"type\":\"assistant/message\",\"surfaceOp\":\"append\",\"time\":1760000002000,\"data\":{\"message\":{\"source\":{\"model\":\"gpt-5\"},\"content\":[{\"type\":\"text\",\"text\":\"dsh answer\"}]}}}\n",
        )
        .unwrap();
        let dsh_sessions = scan_sessions(&paths, ToolId::Dsh, 20);
        assert_eq!(dsh_sessions.len(), 1);
        assert!(dsh_sessions[0].source_path.ends_with("session.v2.jsonl"));
        assert_eq!(dsh_sessions[0].title.as_deref(), Some("hello dsh v2"));
        assert_eq!(load_messages(&paths, &dsh_sessions[0]).unwrap().len(), 2);

        // 6. Kimi `~/.kimi/sessions/<hash>/<sid>` with `state.json` + `wire.jsonl` TurnBegin
        let kimi_dir = paths.home.join(".kimi").join("sessions").join("h1").join("kimi-state-1");
        std::fs::create_dir_all(&kimi_dir).unwrap();
        std::fs::write(
            kimi_dir.join("state.json"),
            r#"{"session_id":"kimi-state-1","work_dir":"/work/kimi","title":"Kimi Task","created_at":1760000000}"#,
        )
        .unwrap();
        std::fs::write(
            kimi_dir.join("wire.jsonl"),
            "{\"timestamp\":1760000001,\"message\":{\"type\":\"TurnBegin\",\"payload\":{\"user_input\":\"kimi prompt\"}}}\n{\"timestamp\":1760000002,\"message\":{\"type\":\"ContentPart\",\"payload\":{\"type\":\"text\",\"text\":\"kimi reply\"}}}\n",
        )
        .unwrap();
        let kimi_sessions = scan_sessions(&paths, ToolId::Kimi, 20);
        assert!(kimi_sessions.iter().any(|s| s.session_id == "kimi-state-1"));
        let km = kimi_sessions.iter().find(|s| s.session_id == "kimi-state-1").unwrap();
        assert_eq!(load_messages(&paths, km).unwrap().len(), 2);

        // 7. Claude Desktop `local_*.json` + `.claude/projects/...`
        let cd_root = paths
            .home
            .join(".config")
            .join("Claude")
            .join("local-agent-mode-sessions")
            .join("org1")
            .join("user1");
        let cd_proj = cd_root.join("local_cd1").join(".claude").join("projects").join("p");
        std::fs::create_dir_all(&cd_proj).unwrap();
        std::fs::write(
            cd_root.join("local_cd1.json"),
            r#"{"cliSessionId":"cd-cli-1","title":"Desktop Session","cwd":"/work/desktop/outputs","createdAt":1760000000000}"#,
        )
        .unwrap();
        std::fs::write(
            cd_proj.join("cd-cli-1.jsonl"),
            "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"hello desktop\"}}\n",
        )
        .unwrap();
        let cd_sessions = scan_sessions(&paths, ToolId::ClaudeDesktop, 20);
        assert_eq!(cd_sessions.len(), 1);
        assert_eq!(cd_sessions[0].session_id, "cd-cli-1");
        assert_eq!(cd_sessions[0].project_dir.as_deref(), Some("/work/desktop"));
        assert_eq!(cd_sessions[0].resume_command, None);
        assert_eq!(load_messages(&paths, &cd_sessions[0]).unwrap().len(), 1);

        // Verify SessionScan::open sees the same updated tools too
        let mut scan = SessionScan::open(&paths, ToolId::OpenCode);
        assert_eq!(scan.next_batch(10).unwrap().len(), 1);
        delete_session(&oc_sessions[0]).unwrap();
        assert!(scan_sessions(&paths, ToolId::OpenCode, 20).is_empty());
    }
}

