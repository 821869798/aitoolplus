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
        ToolId::ClaudeDesktop | ToolId::Agents => return None,
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
        ToolId::Codex => scan_codex(&root, limit),
        ToolId::ClaudeCode => scan_claude_code(&root, limit),
        ToolId::Kimi => scan_kimi_dirs(&root, limit),
        _ => scan_jsonl_generic(&root, tool, limit),
    };
    sessions.sort_by_key(|s| std::cmp::Reverse(s.last_active_at));
    sessions.truncate(limit);
    sessions
}

/// Every session on disk for this tool. No count cap. Same metadata-only scan
/// ai-toolbox uses for its full list.
pub fn scan_all_sessions(paths: &Paths, tool: ToolId) -> Vec<SessionMeta> {
    let Some(root) = session_root(paths, tool) else {
        return vec![];
    };
    if !root.is_dir() {
        return vec![];
    }
    let mut sessions = match tool {
        ToolId::Grok => scan_grok(&root),
        ToolId::Codex => scan_codex(&root, usize::MAX),
        ToolId::ClaudeCode => scan_claude_code(&root, usize::MAX),
        ToolId::Kimi => scan_kimi_dirs(&root, usize::MAX),
        _ => scan_jsonl_generic(&root, tool, usize::MAX),
    };
    sessions.sort_by_key(|s| std::cmp::Reverse(s.last_active_at));
    sessions
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
    Empty,
}

impl SessionScan {
    pub fn open(paths: &Paths, tool: ToolId) -> Self {
        let Some(root) = session_root(paths, tool).filter(|root| root.is_dir()) else {
            return Self { kind: ScanKind::Empty, cursor: 0 };
        };
        if tool == ToolId::Grok {
            return Self { kind: ScanKind::Grok { dirs: collect_grok_dirs(&root) }, cursor: 0 };
        }
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

    fn len(&self) -> usize {
        match &self.kind {
            ScanKind::Jsonl { files, .. } => files.len(),
            ScanKind::Grok { dirs } => dirs.len(),
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

/// Kimi: session directories with metadata; fall back to jsonl scan.
fn scan_kimi_dirs(root: &Path, limit: usize) -> Vec<SessionMeta> {
    scan_jsonl_generic(root, ToolId::Kimi, limit)
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
            });

        if let Some(role) = role {
            if role.eq_ignore_ascii_case("user") || role.eq_ignore_ascii_case("assistant") {
                message_count += 1;
            }
            if first_user.is_none() && role.eq_ignore_ascii_case("user") {
                let text = extract_text(&value);
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

fn resume_command(tool: ToolId, session_id: &str, source_path: Option<&str>) -> Option<String> {
    Some(match tool {
        ToolId::ClaudeCode => format!("claude --resume {session_id}"),
        ToolId::Codex => format!("codex resume {session_id}"),
        ToolId::GeminiCli => return None,
        ToolId::Grok => format!("grok --resume {session_id}"),
        ToolId::Kimi => format!("kimi --resume {session_id}"),
        ToolId::OpenCode => format!("opencode --continue {session_id}"),
        ToolId::OpenClaw => format!("openclaw resume {session_id}"),
        ToolId::Pi => {
            if let Some(sp) = source_path {
                format!("pi --session \"{sp}\"")
            } else {
                format!("pi --continue {session_id}")
            }
        }
        ToolId::OhMyPi => {
            if let Some(sp) = source_path {
                format!("omp --session \"{sp}\"")
            } else {
                format!("omp --continue {session_id}")
            }
        }
        ToolId::Hermes => return None,
        ToolId::Dsh => return None,
        ToolId::ClaudeDesktop | ToolId::Agents => return None,
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
    use std::io::BufRead;
    let file = std::fs::File::open(path)
        .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    let reader = std::io::BufReader::new(file);

    let mut out: Vec<SessionMessage> = vec![];
    let mut tool_use_map: HashMap<String, (usize, usize)> = HashMap::new();

    for line in reader.lines().filter_map(|l| l.ok()) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };

        // 1. Filter out internal noise / meta events
        if let Some(type_str) = value.get("type").and_then(Value::as_str) {
            match type_str {
                "mode" | "permission-mode" | "atis-latch" | "file-history-snapshot"
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
}
