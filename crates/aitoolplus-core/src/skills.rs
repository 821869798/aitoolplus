//! Skills: central-repo management with per-tool sync (mirrors ai-toolbox
//! `coding/skills`).
//!
//! Architecture (from upstream AGENTS.md):
//! - ONE authoritative central repo path (`skill_settings:skills.central_repo_path`,
//!   fallback `app_data_dir/skills`); preferences never act as a second source
//! - Skill records live in the DB; the central repo holds the content
//! - Each skill lists `enabled_tools`; sync writes to each tool's skills dir
//!   via its adapter (path + mode), recording `SkillTarget` sync_details
//! - `status` = content/sync health (ok|error|pending);
//!   `management_enabled` = AI Toolbox's own management flag (distinct)
//! - Supported tools & their install modes: claude_code / codex / pi / opencode
//!   / oh_my_pi (symlink on unix, junction/copy on Windows), kimi (symlink)

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::paths::Paths;
use crate::tools::ToolId;

pub const DEFAULT_CENTRAL_DIR: &str = "skills";
pub const DISABLED_SUFFIX: &str = ".disabled";

// ---------------------------------------------------------------------------
// Types (upstream parity)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    pub id: String,
    pub name: String,
    /// "local" | "import" | "central" | "git"
    #[serde(default = "default_source_type")]
    pub source_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
    /// Central-repo relative path for this skill.
    pub central_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_sync_at: Option<i64>,
    /// ok | error | pending
    #[serde(default = "default_status")]
    pub status: String,
    pub sort_index: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_note: Option<String>,
    /// AI Toolbox management flag (distinct from `status`).
    #[serde(default = "default_true")]
    pub management_enabled: bool,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Tool keys this skill is enabled for.
    #[serde(default)]
    pub enabled_tools: Vec<String>,
    /// Per-tool sync state: { tool: { targetPath, mode, status, syncedAt, errorMessage } }
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_details: Option<Value>,
    /// Optional cached description parsed from SKILL.md frontmatter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional tool key where this skill originally came from ("agents", "claude_code", etc.).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_tool: Option<String>,
}

fn default_source_type() -> String {
    "central".into()
}
fn default_status() -> String {
    "ok".into()
}
fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDocument {
    pub filename: String,
    pub content: String,
    pub truncated: bool,
}

impl Skill {
    pub fn new(name: impl Into<String>, central_path: impl Into<String>) -> Self {
        let now = now_ms();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            source_type: "central".into(),
            source_ref: None,
            central_path: central_path.into(),
            content_hash: None,
            created_at: now,
            updated_at: now,
            last_sync_at: None,
            status: "ok".into(),
            sort_index: 0,
            user_group: None,
            user_note: None,
            management_enabled: true,
            tags: vec![],
            enabled_tools: vec![],
            sync_details: None,
            description: None,
            origin_tool: None,
        }
    }

    pub fn touch(&mut self) {
        self.updated_at = now_ms();
    }

    pub fn is_enabled_in(&self, tool: ToolId) -> bool {
        self.enabled_tools.iter().any(|k| k == tool.key())
    }

    pub fn get_description(&self, repo_path: &Path) -> Option<String> {
        if let Some(desc) = &self.description {
            if !desc.trim().is_empty() {
                return Some(desc.clone());
            }
        }
        read_skill_description(&repo_path.join(&self.central_path))
    }

    fn record_target(&mut self, target: &SkillTarget) {
        let mut map = self
            .sync_details
            .as_ref()
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        map.insert(
            target.tool.clone(),
            serde_json::to_value(target).unwrap_or(Value::Null),
        );
        self.sync_details = Some(Value::Object(map));
        self.touch();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillTarget {
    pub tool: String,
    pub target_path: String,
    /// symlink | junction | copy
    pub mode: String,
    /// ok | error | pending
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub synced_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

/// Preferences (never a second source of truth for the repo path).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillPreferences {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred_tools: Option<Vec<String>>,
    #[serde(default = "default_view_mode")]
    pub default_view_mode: String,
    #[serde(default)]
    pub show_skills_in_tray: bool,
    #[serde(default)]
    pub auto_update_enabled: bool,
}

fn default_view_mode() -> String {
    "flat".into()
}

/// GitHub Skill 仓库配置（对齐 cc-switch）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillRepo {
    pub owner: String,
    pub name: String,
    #[serde(default = "default_main_branch")]
    pub branch: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_main_branch() -> String {
    "main".to_string()
}

pub fn default_skill_repos() -> Vec<SkillRepo> {
    vec![
        SkillRepo {
            owner: "anthropics".to_string(),
            name: "skills".to_string(),
            branch: "main".to_string(),
            enabled: true,
        },
        SkillRepo {
            owner: "anthropics".to_string(),
            name: "claude-plugins-official".to_string(),
            branch: "main".to_string(),
            enabled: true,
        },
        SkillRepo {
            owner: "tencent".to_string(),
            name: "browserskill".to_string(),
            branch: "main".to_string(),
            enabled: true,
        },
        SkillRepo {
            owner: "mattpocock".to_string(),
            name: "skills".to_string(),
            branch: "main".to_string(),
            enabled: true,
        },
        SkillRepo {
            owner: "ComposioHQ".to_string(),
            name: "awesome-claude-skills".to_string(),
            branch: "master".to_string(),
            enabled: true,
        },
        SkillRepo {
            owner: "cexll".to_string(),
            name: "myclaude".to_string(),
            branch: "master".to_string(),
            enabled: true,
        },
        SkillRepo {
            owner: "JimLiu".to_string(),
            name: "baoyu-skills".to_string(),
            branch: "main".to_string(),
            enabled: true,
        },
        SkillRepo {
            owner: "github".to_string(),
            name: "awesome-copilot".to_string(),
            branch: "main".to_string(),
            enabled: true,
        },
        SkillRepo {
            owner: "claude-office-skills".to_string(),
            name: "skills".to_string(),
            branch: "main".to_string(),
            enabled: true,
        },
    ]
}

/// The single authoritative central repo path.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub central_repo_path: Option<String>,
    #[serde(default)]
    pub preferences: SkillPreferences,
    #[serde(default = "default_skill_repos")]
    pub repos: Vec<SkillRepo>,
}

impl Default for SkillSettings {
    fn default() -> Self {
        Self {
            central_repo_path: None,
            preferences: SkillPreferences::default(),
            repos: default_skill_repos(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillsStore {
    #[serde(default)]
    pub skills: Vec<Skill>,
    #[serde(default)]
    pub groups: Vec<SkillGroupRecord>,
    #[serde(default)]
    pub settings: SkillSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillGroupRecord {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub sort_index: i32,
    pub created_at: i64,
    pub updated_at: i64,
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

// ---------------------------------------------------------------------------
pub fn parse_skill_md_frontmatter(path: &Path) -> (Option<String>, Option<String>) {
    let Ok(text) = std::fs::read_to_string(path) else {
        return (None, None);
    };
    parse_frontmatter_text(&text)
}

pub fn parse_frontmatter_text(text: &str) -> (Option<String>, Option<String>) {
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return (None, None);
    }

    let mut name = None;
    let mut description = None;
    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed == "---" {
            break;
        }
        if let Some(value) = trimmed.strip_prefix("name:") {
            let v = normalize_scalar(value);
            if !v.is_empty() {
                name = Some(v);
            }
        } else if let Some(value) = trimmed.strip_prefix("description:") {
            let v = normalize_scalar(value);
            if is_block_scalar_indicator(&v) {
                if let Some(block) = collect_block_scalar(&mut lines) {
                    if !block.is_empty() {
                        description = Some(block);
                    }
                }
            } else if !v.is_empty() {
                description = Some(v);
            }
        }
    }
    (name, description)
}

fn normalize_scalar(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_string()
}

fn is_block_scalar_indicator(value: &str) -> bool {
    let v = value.trim();
    v.starts_with('|') || v.starts_with('>')
}

fn collect_block_scalar<'a>(lines: &mut std::str::Lines<'a>) -> Option<String> {
    let mut content_lines: Vec<String> = Vec::new();
    for line in lines.by_ref() {
        let trimmed = line.trim();
        if trimmed == "---" {
            break;
        }
        if !line.starts_with(|c: char| c.is_whitespace()) && !trimmed.is_empty() {
            break;
        }
        content_lines.push(trimmed.to_string());
    }
    while content_lines.last().map(|s| s.is_empty()).unwrap_or(false) {
        content_lines.pop();
    }
    if content_lines.iter().all(|s| s.is_empty()) {
        return None;
    }
    Some(content_lines.join("\n").trim().to_string())
}

pub fn read_skill_description(skill_dir: &Path) -> Option<String> {
    let candidates = ["SKILL.md", "skill.md", "README.md"];
    for name in candidates {
        let file = skill_dir.join(name);
        if file.is_file() {
            if let Some(desc) = parse_skill_md_frontmatter(&file).1 {
                return Some(desc);
            }
        }
    }
    None
}

const SKILL_DOCUMENT_READ_CAP_BYTES: usize = 128 * 1024;

pub fn read_skill_documents(skill_dir: &Path) -> Vec<SkillDocument> {
    let candidates = ["SKILL.md", "skill.md", "README.md", "readme.md"];
    let mut docs = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for name in candidates {
        let file_path = skill_dir.join(name);
        if !file_path.is_file() {
            continue;
        }
        let canonical_name = if name.eq_ignore_ascii_case("skill.md") {
            "SKILL.md".to_string()
        } else {
            "README.md".to_string()
        };
        if seen.contains(&canonical_name) {
            continue;
        }
        seen.insert(canonical_name.clone());

        if let Ok(bytes) = std::fs::read(&file_path) {
            let truncated = bytes.len() > SKILL_DOCUMENT_READ_CAP_BYTES;
            let slice = if truncated {
                &bytes[..SKILL_DOCUMENT_READ_CAP_BYTES]
            } else {
                &bytes[..]
            };
            docs.push(SkillDocument {
                filename: canonical_name,
                content: String::from_utf8_lossy(slice).into_owned(),
                truncated,
            });
        }
    }
    docs
}

pub fn set_management_enabled(store: &mut SkillsStore, id: &str, enabled: bool) -> bool {
    if let Some(skill) = store.skills.iter_mut().find(|s| s.id == id) {
        skill.management_enabled = enabled;
        skill.touch();
        return true;
    }
    false
}

pub fn update_metadata(
    store: &mut SkillsStore,
    id: &str,
    user_group: Option<String>,
    user_note: Option<String>,
    tags: Option<Vec<String>>,
) -> bool {
    if let Some(skill) = store.skills.iter_mut().find(|s| s.id == id) {
        if let Some(group) = user_group {
            skill.user_group = if group.trim().is_empty() { None } else { Some(group.trim().to_string()) };
        }
        if let Some(note) = user_note {
            skill.user_note = if note.trim().is_empty() { None } else { Some(note.trim().to_string()) };
        }
        if let Some(t) = tags {
            skill.tags = t;
        }
        skill.touch();
        return true;
    }
    false
}

// ---------------------------------------------------------------------------
// Central repo resolution (single source of truth)
// ---------------------------------------------------------------------------

pub fn central_repo_path(settings: &SkillSettings, paths: &Paths) -> PathBuf {
    settings
        .central_repo_path
        .as_deref()
        .filter(|p| !p.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| paths.app_data.join(DEFAULT_CENTRAL_DIR))
}

/// Enumerate central-repo skills as records (idempotent by name).
pub fn scan_central(store: &mut SkillsStore, paths: &Paths) -> usize {
    let repo = central_repo_path(&store.settings, paths);
    let mut added = 0;
    let mut next = store
        .skills
        .iter()
        .map(|s| s.sort_index)
        .max()
        .unwrap_or(-1);

    let entries = match std::fs::read_dir(&repo) {
        Ok(e) => e,
        Err(_) => return 0,
    };
    let mut names: Vec<(String, PathBuf)> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .filter_map(|p| {
            let name = p
                .file_name()
                .and_then(|n| n.to_str())?
                .trim_end_matches(DISABLED_SUFFIX)
                .to_string();
            Some((name, p))
        })
        .collect();
    names.sort();

    for (name, dir) in names {
        let desc = read_skill_description(&dir);
        if let Some(existing) = store.skills.iter_mut().find(|s| s.name == name) {
            // refresh central path + hash fields only
            existing.central_path = name.clone();
            existing.status = if dir.join("SKILL.md").is_file() || dir.join("skill.md").is_file() {
                "ok".into()
            } else {
                "error".into()
            };
            if existing.description.is_none() || existing.description.as_deref() == Some("") {
                existing.description = desc;
            }
            existing.touch();
            continue;
        }
        next += 1;
        let mut skill = Skill::new(name.clone(), name.clone());
        skill.sort_index = next;
        skill.status = if dir.join("SKILL.md").is_file() || dir.join("skill.md").is_file() {
            "ok".into()
        } else {
            "error".into()
        };
        skill.description = desc;
        store.skills.push(skill);
        added += 1;
    }
    added
}

// ---------------------------------------------------------------------------
// Tool adapters (which tools support skills, where, how)
// ---------------------------------------------------------------------------

/// Skills-capable tools and their install dirs.
pub fn skills_tools() -> &'static [ToolId] {
    &[
        ToolId::ClaudeCode,
        ToolId::Agents,
        ToolId::Codex,
        ToolId::Pi,
        ToolId::OpenCode,
        ToolId::OhMyPi,
        ToolId::Kimi,
    ]
}

pub fn tool_skills_dir(paths: &Paths, tool: ToolId) -> Option<PathBuf> {
    let root = paths.tool_root(tool);
    Some(match tool {
        ToolId::ClaudeCode => root.join("skills"),
        ToolId::Agents => root.join("skills"),
        ToolId::Codex => root.join("skills"),
        ToolId::Pi => root.join("skills"),
        ToolId::OpenCode => root.join("skill"),
        ToolId::OhMyPi => root.join("skills"),
        ToolId::Kimi => root.join("skills"),
        _ => return None,
    })
}

fn install_mode() -> &'static str {
    if cfg!(windows) { "junction" } else { "symlink" }
}

/// Install (link or copy) the central skill into a tool's dir.
pub fn sync_skill_to_tool(
    settings: &SkillSettings,
    paths: &Paths,
    skill: &mut Skill,
    tool: ToolId,
) -> Result<(), String> {
    let repo = central_repo_path(settings, paths);
    let source = repo.join(&skill.central_path);
    let Some(target_root) = tool_skills_dir(paths, tool) else {
        return Err(format!("{} has no skills directory", tool.name_en()));
    };
    std::fs::create_dir_all(&target_root).map_err(|e| e.to_string())?;
    let target = target_root.join(&skill.central_path);

    // remove previous install (any mode)
    remove_path(&target)?;

    let is_agents = tool == ToolId::Agents;
    let (mode, ok) = if is_agents {
        // Universal Agent (~/.agents/skills) must always be a plain directory copy, never junction/symlink!
        ("copy", copy_dir(&source, &target).is_ok())
    } else {
        let mode = install_mode();
        let ok = match mode {
            #[cfg(unix)]
            "symlink" => {
                let rel = make_relative(&target, &source);
                std::os::unix::fs::symlink(&rel, &target).is_ok()
            }
            #[cfg(windows)]
            "junction" => junction_or_copy(&source, &target),
            _ => copy_dir(&source, &target).is_ok(),
        };
        let ok = if !ok {
            copy_dir(&source, &target).is_ok()
        } else {
            true
        };
        (mode, ok)
    };
    if !ok {
        return Err(format!("Failed to sync skill to {}", target.display()));
    }

    skill.enabled_tools.retain(|k| k != tool.key());
    skill.enabled_tools.push(tool.key().to_string());
    skill.last_sync_at = Some(now_ms());

    let target_record = SkillTarget {
        tool: tool.key().to_string(),
        target_path: target.to_string_lossy().to_string(),
        mode: mode.into(),
        status: "ok".into(),
        synced_at: Some(now_ms()),
        error_message: None,
    };
    skill.record_target(&target_record);
    Ok(())
}

/// Remove the skill from a tool's dir (disable path).
pub fn remove_skill_from_tool(
    _settings: &SkillSettings,
    paths: &Paths,
    skill: &mut Skill,
    tool: ToolId,
) -> Result<(), String> {
    let Some(target_root) = tool_skills_dir(paths, tool) else {
        return Ok(());
    };
    let target = target_root.join(&skill.central_path);
    let existed = target.exists() || target.symlink_metadata().is_ok();
    remove_path(&target)?;

    skill.enabled_tools.retain(|k| k != tool.key());
    if existed {
        let is_agents = tool == ToolId::Agents;
        let mode = if is_agents { "copy" } else { install_mode() };
        let target_record = SkillTarget {
            tool: tool.key().to_string(),
            target_path: target.to_string_lossy().to_string(),
            mode: mode.into(),
            status: "pending".into(), // removed from tool; would sync if re-enabled
            synced_at: Some(now_ms()),
            error_message: None,
        };
        skill.record_target(&target_record);
    }
    Ok(())
}

/// Remove the skill link/copy from a tool's dir without clearing `enabled_tools`.
pub fn unlink_skill_from_tool(
    paths: &Paths,
    skill: &Skill,
    tool: ToolId,
) -> Result<(), String> {
    let Some(target_root) = tool_skills_dir(paths, tool) else {
        return Ok(());
    };
    let target = target_root.join(&skill.central_path);
    remove_path(&target)
}

/// Sync every skill to every enabled tool; returns per-tool (ok, failed).
pub fn sync_all(store: &mut SkillsStore, paths: &Paths) -> Vec<(ToolId, usize, usize)> {
    let ids: Vec<String> = store
        .skills
        .iter()
        .filter(|s| s.management_enabled)
        .map(|s| s.id.clone())
        .collect();
    for id in ids {
        let settings = store.settings.clone();
        let _ = &settings;
        for tool in skills_tools().iter().copied() {
            let (enabled, previously) = {
                let skill = store
                    .skills
                    .iter()
                    .find(|s| s.id == id)
                    .expect("skill present");
                let enabled = skill.is_enabled_in(tool);
                let previously = skill
                    .sync_details
                    .as_ref()
                    .and_then(Value::as_object)
                    .and_then(|m| m.get(tool.key()))
                    .and_then(|t| t.get("status"))
                    .and_then(Value::as_str)
                    .is_some_and(|s| s == "ok");
                (enabled, previously)
            };
            if enabled {
                if let Some(skill) = store.skills.iter_mut().find(|s| s.id == id) {
                    let _ = sync_skill_to_tool(&settings, paths, skill, tool);
                }
            } else if previously && let Some(skill) = store.skills.iter_mut().find(|s| s.id == id) {
                let _ = remove_skill_from_tool(&settings, paths, skill, tool);
            }
        }
    }

    let mut report = vec![];
    for tool in skills_tools().iter().copied() {
        let mut ok = 0;
        let mut failed = 0;
        for skill in &store.skills {
            if !skill.is_enabled_in(tool) || !skill.management_enabled {
                continue;
            }
            match skill
                .sync_details
                .as_ref()
                .and_then(Value::as_object)
                .and_then(|m| m.get(tool.key()))
            {
                Some(t) if t.get("status") == Some(&Value::String("ok".into())) => ok += 1,
                Some(_) => failed += 1,
                None => failed += 1,
            }
        }
        if ok + failed > 0 {
            report.push((tool, ok, failed));
        }
    }
    report
}

// ---------------------------------------------------------------------------
// Store CRUD
// ---------------------------------------------------------------------------

pub fn list(store: &SkillsStore) -> Vec<Skill> {
    let mut v = store.skills.clone();
    v.sort_by_key(|s| (s.sort_index, s.created_at));
    v
}

pub fn upsert(store: &mut SkillsStore, mut skill: Skill) {
    skill.touch();
    if let Some(existing) = store.skills.iter_mut().find(|s| s.id == skill.id) {
        *existing = skill;
    } else {
        skill.sort_index = store
            .skills
            .iter()
            .map(|s| s.sort_index)
            .max()
            .unwrap_or(-1)
            + 1;
        store.skills.push(skill);
    }
}

pub fn delete(store: &mut SkillsStore, id: &str) -> bool {
    let before = store.skills.len();
    store.skills.retain(|s| s.id != id);
    before != store.skills.len()
}

pub fn toggle_tool(store: &mut SkillsStore, id: &str, tool: ToolId) -> Option<bool> {
    let skill = store.skills.iter_mut().find(|s| s.id == id)?;
    let key = tool.key().to_string();
    let enabled = !skill.is_enabled_in(tool);
    if enabled {
        if !skill.enabled_tools.contains(&key) {
            skill.enabled_tools.push(key);
        }
    } else {
        skill.enabled_tools.retain(|k| k != &key);
    }
    skill.touch();
    Some(enabled)
}

/// Install an external skill dir into the central repo.
pub fn install_into_central(
    settings: &SkillSettings,
    paths: &Paths,
    source: &Path,
) -> Result<String, String> {
    let repo = central_repo_path(settings, paths);
    let name = source
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("source has no directory name")?
        .trim_end_matches(DISABLED_SUFFIX)
        .to_string();
    let dst = repo.join(&name);
    if dst.exists() {
        remove_path(&dst)?;
    }
    copy_dir(source, &dst)?;
    Ok(name)
}

// ---------------------------------------------------------------------------
// Candidate scanning & Auto-import
// ---------------------------------------------------------------------------

pub fn candidate_skill_dirs(paths: &Paths) -> Vec<PathBuf> {
    let mut dirs = vec![
        paths.home.join(".agents").join("skills"),
        paths.home.join(".claude").join("skills"),
        paths.home.join(".codex").join("skills"),
        paths.home.join(".pi").join("agent").join("skills"),
        paths.home.join(".config").join("oh-my-pi").join("skills"),
        paths.home.join(".config").join("opencode").join("skill"),
        paths.home.join(".kimi-code").join("skills"),
        paths.home.join(".cc-switch").join("skills"),
    ];
    for tool in skills_tools().iter().copied() {
        if let Some(tool_dir) = tool_skills_dir(paths, tool) {
            if !dirs.contains(&tool_dir) {
                dirs.push(tool_dir);
            }
        }
    }
    dirs
}

/// Automatically scan all known tool skill directories and import any unmanaged
/// skills into the central repository without requiring manual directory selection.
pub fn scan_and_import_existing(
    paths: &Paths,
    store: &mut SkillsStore,
) -> Result<Vec<String>, String> {
    let mut imported = Vec::new();
    let repo = central_repo_path(&store.settings, paths);
    std::fs::create_dir_all(&repo).map_err(|e| e.to_string())?;

    for candidate in candidate_skill_dirs(paths) {
        let Ok(entries) = std::fs::read_dir(&candidate) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            // Check if directory has SKILL.md or skill.md
            if !path.join("SKILL.md").is_file() && !path.join("skill.md").is_file() {
                continue;
            }
            let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let name = file_name.trim_end_matches(DISABLED_SUFFIX).to_string();
            if name.is_empty() || name.starts_with('.') {
                continue;
            }

            // Check if already in central repo
            let central_dest = repo.join(&name);
            let in_central = central_dest.exists()
                && (central_dest.join("SKILL.md").is_file()
                    || central_dest.join("skill.md").is_file());
            let in_store = store.skills.iter().any(|s| s.name == name);

            if !in_central {
                if let Ok(installed_name) = install_into_central(&store.settings, paths, &path) {
                    imported.push(installed_name);
                }
            } else if !in_store {
                imported.push(name);
            }
        }
    }

    if !imported.is_empty() {
        scan_central(store, paths);
    }

    // Automatically recognize tool installation status:
    // If a skill currently exists on disk in a tool's skills directory,
    // ensure that tool is marked as enabled.
    // For .agents: MUST be a real directory copy (never junction/link); if link, convert to real dir!
    // If not existing on disk in .agents, ensure "agents" is removed from enabled_tools.
    // For other tools: convert plain directories to junctions/links.
    let settings = store.settings.clone();
    let central_repo = central_repo_path(&settings, paths);
    for skill in store.skills.iter_mut() {
        let source = central_repo.join(&skill.central_path);
        for tool in skills_tools().iter().copied() {
            if let Some(tool_dir) = tool_skills_dir(paths, tool) {
                let target = tool_dir.join(&skill.central_path);
                let exists = target.exists() || target.symlink_metadata().is_ok();
                let key = tool.key().to_string();

                if tool == ToolId::Agents {
                    if exists {
                        if skill.origin_tool.is_none() {
                            skill.origin_tool = Some(key.clone());
                            skill.touch();
                        }
                        if !skill.enabled_tools.contains(&key) {
                            skill.enabled_tools.push(key.clone());
                            skill.touch();
                        }

                        let is_link = target
                            .symlink_metadata()
                            .ok()
                            .map(|m| m.file_type().is_symlink())
                            .unwrap_or(false);

                        if is_link {
                            // Convert junction/symlink to real directory copy
                            let _ = remove_path(&target);
                            if source.exists() {
                                let _ = copy_dir(&source, &target);
                            }
                        }

                        let target_record = SkillTarget {
                            tool: key,
                            target_path: target.to_string_lossy().to_string(),
                            mode: "copy".into(),
                            status: "ok".into(),
                            synced_at: Some(now_ms()),
                            error_message: None,
                        };
                        skill.record_target(&target_record);
                    } else {
                        // Directory does not exist in .agents/skills: Universal Agent is off
                        if skill.enabled_tools.contains(&key) {
                            skill.enabled_tools.retain(|k| k != &key);
                            skill.touch();
                        }
                        if skill
                            .sync_details
                            .as_ref()
                            .and_then(|v| v.as_object())
                            .and_then(|m| m.get(&key))
                            .is_some()
                        {
                            let target_record = SkillTarget {
                                tool: key,
                                target_path: target.to_string_lossy().to_string(),
                                mode: "copy".into(),
                                status: "pending".into(),
                                synced_at: Some(now_ms()),
                                error_message: None,
                            };
                            skill.record_target(&target_record);
                        }
                    }
                } else {
                    if exists {
                        let is_link = target
                            .symlink_metadata()
                            .ok()
                            .map(|m| m.file_type().is_symlink())
                            .unwrap_or(false);

                        if skill.origin_tool.is_none() {
                            skill.origin_tool = Some(key.clone());
                            skill.touch();
                        }
                        if !skill.enabled_tools.contains(&key) {
                            skill.enabled_tools.push(key.clone());
                            skill.touch();
                        }

                        if !is_link {
                            let _ = sync_skill_to_tool(&settings, paths, skill, tool);
                        } else {
                            let is_ok = skill
                                .sync_details
                                .as_ref()
                                .and_then(|v| v.as_object())
                                .and_then(|m| m.get(&key))
                                .and_then(|t| t.get("status"))
                                .and_then(|s| s.as_str())
                                == Some("ok");
                            if !is_ok {
                                let target_record = SkillTarget {
                                    tool: key,
                                    target_path: target.to_string_lossy().to_string(),
                                    mode: install_mode().into(),
                                    status: "ok".into(),
                                    synced_at: Some(now_ms()),
                                    error_message: None,
                                };
                                skill.record_target(&target_record);
                            }
                        }
                    }
                }
            }
        }
    }

    imported.sort();
    imported.dedup();
    Ok(imported)
}

/// Check and update all Git-backed skills and refresh catalog metadata.
/// Returns (updated_count, failed_count).
pub fn update_all_skills(store: &mut SkillsStore, paths: &Paths) -> (usize, usize) {
    let mut updated = 0;
    let mut failed = 0;

    let git_ids: Vec<String> = store
        .skills
        .iter()
        .filter(|s| s.source_type == "git" || s.source_ref.as_deref().is_some_and(|r| r.ends_with(".git") || r.starts_with("http")))
        .map(|s| s.id.clone())
        .collect();

    for id in git_ids {
        match crate::skill_git::update(store, paths, &id) {
            Ok(()) => updated += 1,
            Err(_) => failed += 1,
        }
    }

    scan_central(store, paths);
    (updated, failed)
}

/// Restore all skills across all enabled tools from Junction/Symlink back to standalone plain directories,
/// freeing them from central repo dependency. Returns (restored_count, error_count).
pub fn restore_all_to_plain(store: &mut SkillsStore, paths: &Paths) -> (usize, usize) {
    let mut restored = 0;
    let mut errors = 0;
    let central_repo = central_repo_path(&store.settings, paths);

    for skill in &mut store.skills {
        let source = central_repo.join(&skill.central_path);
        if !source.exists() {
            continue;
        }

        for tool in skills_tools().iter().copied() {
            if !skill.is_enabled_in(tool) {
                continue;
            }
            let Some(target_root) = tool_skills_dir(paths, tool) else {
                continue;
            };
            let target = target_root.join(&skill.central_path);
            let is_link = target
                .symlink_metadata()
                .ok()
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(false);

            if is_link || target.exists() {
                // Remove junction/symlink and replace with real directory copy
                let _ = remove_path(&target);
                match copy_dir(&source, &target) {
                    Ok(_) => {
                        restored += 1;
                        let target_record = SkillTarget {
                            tool: tool.key().to_string(),
                            target_path: target.to_string_lossy().to_string(),
                            mode: "copy".into(),
                            status: "ok".into(),
                            synced_at: Some(now_ms()),
                            error_message: None,
                        };
                        skill.record_target(&target_record);
                    }
                    Err(_) => {
                        errors += 1;
                    }
                }
            }
        }
    }
    (restored, errors)
}

// ---------------------------------------------------------------------------
// Skill Store (skills.sh & Curated Catalog)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreSkillItem {
    pub id: String,
    pub name: String,
    pub skill_id: String,
    pub source: String,
    #[serde(default)]
    pub installs: u64,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub repo_owner: String,
    #[serde(default)]
    pub repo_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_branch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreSearchResult {
    pub count: usize,
    pub query: String,
    pub skills: Vec<StoreSkillItem>,
}

pub fn add_skill_repo(settings: &mut SkillSettings, repo: SkillRepo) {
    if !settings.repos.iter().any(|r| r.owner.eq_ignore_ascii_case(&repo.owner) && r.name.eq_ignore_ascii_case(&repo.name)) {
        settings.repos.push(repo);
    }
}

pub fn remove_skill_repo(settings: &mut SkillSettings, owner: &str, name: &str) {
    settings.repos.retain(|r| !(r.owner.eq_ignore_ascii_case(owner) && r.name.eq_ignore_ascii_case(name)));
}

pub fn discover_repo_skills(
    store: &SkillsStore,
    filter_repo: Option<&str>,
    filter_status: Option<&str>,
    query: Option<&str>,
) -> Vec<StoreSkillItem> {
    let mut all = curated_skills();
    for repo in &store.settings.repos {
        if !all.iter().any(|s| s.repo_owner.eq_ignore_ascii_case(&repo.owner) && s.repo_name.eq_ignore_ascii_case(&repo.name)) {
            all.push(StoreSkillItem {
                id: format!("{}/{}:{}", repo.owner, repo.name, repo.name),
                name: repo.name.clone(),
                skill_id: repo.name.clone(),
                source: format!("{}/{}", repo.owner, repo.name),
                installs: 100,
                description: format!("来自用户配置仓库 {}/{} (分支: {})", repo.owner, repo.name, repo.branch),
                repo_owner: repo.owner.clone(),
                repo_name: repo.name.clone(),
                repo_branch: Some(repo.branch.clone()),
            });
        }
    }

    // Filter by repo
    if let Some(r) = filter_repo {
        if !r.is_empty() && r != "all" {
            all.retain(|s| s.source.eq_ignore_ascii_case(r) || format!("{}/{}", s.repo_owner, s.repo_name).eq_ignore_ascii_case(r));
        }
    }

    // Filter by installation status
    if let Some(st) = filter_status {
        if st == "installed" {
            all.retain(|item| {
                store.skills.iter().any(|s| {
                    s.name.eq_ignore_ascii_case(&item.skill_id)
                        || s.name.eq_ignore_ascii_case(&item.name)
                        || s.central_path.eq_ignore_ascii_case(&item.skill_id)
                })
            });
        } else if st == "uninstalled" {
            all.retain(|item| {
                !store.skills.iter().any(|s| {
                    s.name.eq_ignore_ascii_case(&item.skill_id)
                        || s.name.eq_ignore_ascii_case(&item.name)
                        || s.central_path.eq_ignore_ascii_case(&item.skill_id)
                })
            });
        }
    }

    // Filter by search query
    if let Some(q) = query {
        let q_clean = q.trim().to_lowercase();
        if !q_clean.is_empty() {
            all.retain(|s| {
                s.name.to_lowercase().contains(&q_clean)
                    || s.skill_id.to_lowercase().contains(&q_clean)
                    || s.source.to_lowercase().contains(&q_clean)
                    || s.description.to_lowercase().contains(&q_clean)
            });
        }
    }

    all
}

pub fn curated_skills() -> Vec<StoreSkillItem> {
    vec![
        StoreSkillItem {
            id: "ponytail-ai/ponytail:ponytail".into(),
            name: "ponytail".into(),
            skill_id: "ponytail".into(),
            source: "ponytail-ai/ponytail".into(),
            installs: 89000,
            description: "极简开发原则：抵制过度工程，以最简短、高效的代码实现目标".into(),
            repo_owner: "ponytail-ai".into(),
            repo_name: "ponytail".into(),
            repo_branch: Some("main".into()),
        },
        StoreSkillItem {
            id: "gpui-kit/skills:gpui-kit".into(),
            name: "gpui-kit".into(),
            skill_id: "gpui-kit".into(),
            source: "gpui-kit/skills".into(),
            installs: 52000,
            description: "GPUI Kit 原生桌面应用框架：设计规范、交互状态、色彩与组件体系".into(),
            repo_owner: "gpui-kit".into(),
            repo_name: "skills".into(),
            repo_branch: Some("main".into()),
        },
        StoreSkillItem {
            id: "github/awesome-copilot:git-commit".into(),
            name: "git-commit".into(),
            skill_id: "git-commit".into(),
            source: "github/awesome-copilot".into(),
            installs: 45375,
            description: "遵循 Conventional Commits 规范的 Git 提交信息自动生成助手".into(),
            repo_owner: "github".into(),
            repo_name: "awesome-copilot".into(),
            repo_branch: Some("main".into()),
        },
        StoreSkillItem {
            id: "mattpocock/skills:git-guardrails-claude-code".into(),
            name: "git-guardrails-claude-code".into(),
            skill_id: "git-guardrails-claude-code".into(),
            source: "mattpocock/skills".into(),
            installs: 378465,
            description: "Claude Code Git 安全防护栏，防止意外覆盖或丢失工作区变更".into(),
            repo_owner: "mattpocock".into(),
            repo_name: "skills".into(),
            repo_branch: Some("main".into()),
        },
        StoreSkillItem {
            id: "skills/code-reviewer:code-reviewer".into(),
            name: "code-reviewer".into(),
            skill_id: "code-reviewer".into(),
            source: "skills/code-reviewer".into(),
            installs: 28000,
            description: "代码审查专家：检查边界条件、并发安全与性能损耗".into(),
            repo_owner: "skills".into(),
            repo_name: "code-reviewer".into(),
            repo_branch: Some("main".into()),
        },
        StoreSkillItem {
            id: "anthropics/skills:frontend-design".into(),
            name: "frontend-design".into(),
            skill_id: "frontend-design".into(),
            source: "anthropics/skills".into(),
            installs: 96000,
            description: "前端界面与视觉美学指导：色彩层级、排版网格与优雅过渡".into(),
            repo_owner: "anthropics".into(),
            repo_name: "skills".into(),
            repo_branch: Some("main".into()),
        },
        StoreSkillItem {
            id: "anthropics/skills:pdf-extraction".into(),
            name: "pdf-extraction".into(),
            skill_id: "pdf-extraction".into(),
            source: "anthropics/skills".into(),
            installs: 51000,
            description: "PDF 与复杂扫描文档结构化智能提取与表格表格解析工具".into(),
            repo_owner: "anthropics".into(),
            repo_name: "skills".into(),
            repo_branch: Some("main".into()),
        },
        StoreSkillItem {
            id: "anthropics/skills:mcp-builder".into(),
            name: "mcp-builder".into(),
            skill_id: "mcp-builder".into(),
            source: "anthropics/skills".into(),
            installs: 73000,
            description: "一键生成标准 Model Context Protocol (MCP) 服务器脚手架代码".into(),
            repo_owner: "anthropics".into(),
            repo_name: "skills".into(),
            repo_branch: Some("main".into()),
        },
        StoreSkillItem {
            id: "anthropics/skills:notion".into(),
            name: "notion".into(),
            skill_id: "notion".into(),
            source: "anthropics/skills".into(),
            installs: 41000,
            description: "Notion 知识库、页面与数据库双向读写与整理工具".into(),
            repo_owner: "anthropics".into(),
            repo_name: "skills".into(),
            repo_branch: Some("main".into()),
        },
        StoreSkillItem {
            id: "ComposioHQ/awesome-claude-skills:deep-research".into(),
            name: "deep-research".into(),
            skill_id: "deep-research".into(),
            source: "ComposioHQ/awesome-claude-skills".into(),
            installs: 64000,
            description: "深度联网多源研究与结构化信息综合萃取专家".into(),
            repo_owner: "ComposioHQ".into(),
            repo_name: "awesome-claude-skills".into(),
            repo_branch: Some("master".into()),
        },
        StoreSkillItem {
            id: "ComposioHQ/awesome-claude-skills:browser-use".into(),
            name: "browser-use".into(),
            skill_id: "browser-use".into(),
            source: "ComposioHQ/awesome-claude-skills".into(),
            installs: 82000,
            description: "网页自动化浏览、表单点击与端到端自动化操作技能".into(),
            repo_owner: "ComposioHQ".into(),
            repo_name: "awesome-claude-skills".into(),
            repo_branch: Some("master".into()),
        },
        StoreSkillItem {
            id: "github/awesome-copilot:github-issues".into(),
            name: "github-issues".into(),
            skill_id: "github-issues".into(),
            source: "github/awesome-copilot".into(),
            installs: 15795,
            description: "GitHub Issues 和 Pull Request 自动化检索与管理工作流".into(),
            repo_owner: "github".into(),
            repo_name: "awesome-copilot".into(),
            repo_branch: Some("main".into()),
        },
        StoreSkillItem {
            id: "cexll/myclaude:rust-analyzer".into(),
            name: "rust-analyzer".into(),
            skill_id: "rust-analyzer".into(),
            source: "cexll/myclaude".into(),
            installs: 32000,
            description: "Rust 语言最佳工程实践、Cargo 诊断与静态分析助手".into(),
            repo_owner: "cexll".into(),
            repo_name: "myclaude".into(),
            repo_branch: Some("master".into()),
        },
        StoreSkillItem {
            id: "JimLiu/baoyu-skills:baoyu-comic".into(),
            name: "baoyu-comic".into(),
            skill_id: "baoyu-comic".into(),
            source: "JimLiu/baoyu-skills".into(),
            installs: 39000,
            description: "宝玉漫画风插图与图解生成工作流，支持多格分镜与概念可视化".into(),
            repo_owner: "JimLiu".into(),
            repo_name: "baoyu-skills".into(),
            repo_branch: Some("main".into()),
        },
        StoreSkillItem {
            id: "JimLiu/baoyu-skills:baoyu-slide".into(),
            name: "baoyu-slide".into(),
            skill_id: "baoyu-slide".into(),
            source: "JimLiu/baoyu-skills".into(),
            installs: 34000,
            description: "宝玉演示文稿 Slide 结构大纲设计与专业幻灯片排版建议".into(),
            repo_owner: "JimLiu".into(),
            repo_name: "baoyu-skills".into(),
            repo_branch: Some("main".into()),
        },
        StoreSkillItem {
            id: "tencent/browserskill:browser-skill".into(),
            name: "browser-skill".into(),
            skill_id: "browser-skill".into(),
            source: "tencent/browserskill".into(),
            installs: 18200,
            description: "腾讯官方智能浏览器技能：自动化网页交互、DOM操作与数据提取".into(),
            repo_owner: "tencent".into(),
            repo_name: "browserskill".into(),
            repo_branch: Some("main".into()),
        },
        StoreSkillItem {
            id: "claude-office-skills/skills:excel-automation".into(),
            name: "excel-automation".into(),
            skill_id: "excel-automation".into(),
            source: "claude-office-skills/skills".into(),
            installs: 14977,
            description: "Excel 与电子表格高级公式、数据清洗透视与自动化脚本生成".into(),
            repo_owner: "claude-office-skills".into(),
            repo_name: "skills".into(),
            repo_branch: Some("main".into()),
        },
    ]
}

#[derive(Debug, Deserialize)]
struct SkillsShApiItem {
    pub id: String,
    #[serde(rename = "skillId")]
    pub skill_id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub installs: u64,
    pub source: String,
}

#[derive(Debug, Deserialize)]
struct SkillsShApiResponse {
    pub count: usize,
    pub query: String,
    #[serde(default)]
    pub skills: Vec<SkillsShApiItem>,
}

fn format_proxy(raw: &str) -> String {
    let raw = raw.trim();
    if raw.starts_with("http://") || raw.starts_with("https://") || raw.starts_with("socks5://") || raw.starts_with("socks5h://") {
        raw.to_string()
    } else if let Some(rest) = raw.strip_prefix("socks://") {
        format!("socks5://{rest}")
    } else {
        format!("http://{raw}")
    }
}

#[cfg(target_os = "windows")]
fn get_windows_system_proxy() -> Option<String> {
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RegGetValueW, REG_ROUTINE_FLAGS};

    let subkey: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings\0".encode_utf16().collect();
    let enable_name: Vec<u16> = "ProxyEnable\0".encode_utf16().collect();
    let mut proxy_enable: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;

    let res = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            PCWSTR(enable_name.as_ptr()),
            REG_ROUTINE_FLAGS(2), // RRF_RT_REG_DWORD
            None,
            Some(&mut proxy_enable as *mut u32 as _),
            Some(&mut size),
        )
    };
    if res != windows::Win32::Foundation::ERROR_SUCCESS || proxy_enable == 0 {
        return None;
    }

    let server_name: Vec<u16> = "ProxyServer\0".encode_utf16().collect();
    let mut buffer = [0u16; 512];
    let mut buf_size = (buffer.len() * 2) as u32;

    let res = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            PCWSTR(server_name.as_ptr()),
            REG_ROUTINE_FLAGS(1), // RRF_RT_REG_SZ
            None,
            Some(buffer.as_mut_ptr() as _),
            Some(&mut buf_size),
        )
    };
    if res != windows::Win32::Foundation::ERROR_SUCCESS {
        return None;
    }

    let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    let server = String::from_utf16_lossy(&buffer[..len]);
    let server = server.trim();
    if server.is_empty() {
        return None;
    }

    if server.contains('=') {
        for part in server.split(';') {
            let part = part.trim();
            if let Some(rest) = part.strip_prefix("https=") {
                return Some(format_proxy(rest));
            }
        }
        for part in server.split(';') {
            let part = part.trim();
            if let Some(rest) = part.strip_prefix("http=") {
                return Some(format_proxy(rest));
            }
        }
        None
    } else {
        Some(format_proxy(&server))
    }
}

pub fn resolve_proxy() -> Option<String> {
    if std::env::var("AITOOLPLUS_PROXY_MODE").as_deref() == Ok("direct") {
        return None;
    }

    if let Ok(env_proxy) = std::env::var("ALL_PROXY")
        .or_else(|_| std::env::var("all_proxy"))
        .or_else(|_| std::env::var("HTTPS_PROXY"))
        .or_else(|_| std::env::var("https_proxy"))
        .or_else(|_| std::env::var("HTTP_PROXY"))
        .or_else(|_| std::env::var("http_proxy"))
    {
        let p = env_proxy.trim();
        if !p.is_empty() {
            return Some(format_proxy(p));
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(win_proxy) = get_windows_system_proxy() {
            return Some(win_proxy);
        }
    }

    None
}

pub fn skills_http_agent(timeout_secs: u64) -> ureq::Agent {
    let mut builder = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(10))
        .timeout_read(std::time::Duration::from_secs(timeout_secs))
        .timeout_write(std::time::Duration::from_secs(10));

    if let Some(proxy_str) = resolve_proxy() {
        tracing::info!("使用代理请求技能: {}", proxy_str);
        if let Ok(proxy) = ureq::Proxy::new(&proxy_str) {
            builder = builder.proxy(proxy);
        }
    }

    builder.build()
}

pub fn map_skills_request_error(e: &ureq::Error, action: &str) -> String {
    let err_str = e.to_string();
    let is_timeout = err_str.contains("10060")
        || err_str.to_lowercase().contains("timed out")
        || err_str.to_lowercase().contains("timeout");
    let is_conn_refused = err_str.contains("10061") || err_str.to_lowercase().contains("connection refused");
    let is_reset = err_str.contains("10054") || err_str.to_lowercase().contains("connection reset");

    if is_timeout {
        format!(
            "{action}超时：连接 skills.sh 技能库超时（境外网络）。\n请开启代理软件/科学上网，或在【系统设置 - 网络与代理】中配置代理。"
        )
    } else if is_conn_refused {
        format!(
            "{action}被拒绝：请检查本地代理软件是否已启动并在运行。"
        )
    } else if is_reset {
        format!(
            "{action}被重置：网络连接不稳定或被防火墙拦截，请尝试开启或更换代理节点。"
        )
    } else {
        format!("{action}失败: {err_str}")
    }
}

pub fn search_skills_store(
    query: &str,
    limit: usize,
    offset: usize,
) -> Result<StoreSearchResult, String> {
    let q = query.trim();
    if q.is_empty() {
        let curated = curated_skills();
        let total = curated.len();
        let paged: Vec<StoreSkillItem> = curated
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect();
        return Ok(StoreSearchResult {
            count: total,
            query: String::new(),
            skills: paged,
        });
    }

    let mut url =
        url::Url::parse("https://skills.sh/api/search").map_err(|e| e.to_string())?;
    url.query_pairs_mut()
        .append_pair("q", q)
        .append_pair("limit", &limit.to_string())
        .append_pair("offset", &offset.to_string());

    let agent = skills_http_agent(15);
    let resp: SkillsShApiResponse = agent
        .get(url.as_str())
        .call()
        .map_err(|e| map_skills_request_error(&e, "skills.sh 搜索"))?
        .into_json()
        .map_err(|e| format!("解析 skills.sh 响应失败: {e}"))?;

    let skills: Vec<StoreSkillItem> = resp
        .skills
        .into_iter()
        .filter_map(|s| {
            let parts: Vec<&str> = s.source.splitn(2, '/').collect();
            if parts.len() != 2 {
                return None;
            }
            let (owner, repo) = (parts[0].to_string(), parts[1].to_string());
            let skill_id = s.skill_id.unwrap_or_else(|| s.name.clone());
            Some(StoreSkillItem {
                id: s.id,
                name: s.name,
                skill_id,
                source: s.source,
                installs: s.installs,
                description: format!("来自 GitHub 仓库 {}/{}", owner, repo),
                repo_owner: owner,
                repo_name: repo,
                repo_branch: None,
            })
        })
        .collect();

    // 智能关键词相关度重排序（解决 skills.sh 语义检索噪音、精准命中优先）
    let query_lower = q.to_lowercase();
    let query_tokens: Vec<&str> = query_lower.split_whitespace().collect();

    let mut scored_skills: Vec<(StoreSkillItem, i32)> = skills
        .into_iter()
        .map(|item| {
            let id_lower = item.id.to_lowercase();
            let name_lower = item.name.to_lowercase();
            let source_lower = item.source.to_lowercase();
            let skill_id_lower = item.skill_id.to_lowercase();

            let mut score = 0i32;
            let mut matched_count = 0;

            for token in &query_tokens {
                let in_id = id_lower.contains(token);
                let in_name = name_lower.contains(token);
                let in_source = source_lower.contains(token);
                let in_skill_id = skill_id_lower.contains(token);

                if in_id || in_name || in_source || in_skill_id {
                    matched_count += 1;
                    score += 100;
                    if in_name || in_skill_id {
                        score += 80;
                    }
                    if in_source {
                        score += 60;
                    }
                }
            }

            // 搜索所有 token 均匹配（例如 tencent + browser 均存在）给予巨额置顶加分
            if matched_count == query_tokens.len() && !query_tokens.is_empty() {
                score += 1000;
            }

            // 完整短语匹配加分
            if id_lower.contains(&query_lower) || name_lower.contains(&query_lower) || source_lower.contains(&query_lower) {
                score += 500;
            }

            // 安装量热度轻微加权作为平局加分
            score += ((item.installs as f64 + 1.0).log10() * 5.0) as i32;

            (item, score)
        })
        .collect();

    scored_skills.sort_by(|a, b| b.1.cmp(&a.1));
    let ranked_skills = scored_skills.into_iter().map(|(s, _)| s).collect();

    Ok(StoreSearchResult {
        count: resp.count,
        query: resp.query,
        skills: ranked_skills,
    })
}

pub fn install_from_store(
    paths: &Paths,
    store: &mut SkillsStore,
    item: &StoreSkillItem,
) -> Result<String, String> {
    use std::io::Read;

    let repo_dir = central_repo_path(&store.settings, paths);
    std::fs::create_dir_all(&repo_dir).map_err(|e| e.to_string())?;

    let install_name = if item.skill_id.is_empty() {
        &item.name
    } else {
        &item.skill_id
    };

    // 1. Try downloading zip from GitHub:
    let branches = ["main", "master"];
    let mut downloaded_zip: Option<Vec<u8>> = None;
    let agent = skills_http_agent(30);
    for branch in branches {
        let zip_url = format!(
            "https://github.com/{}/{}/archive/refs/heads/{}.zip",
            item.repo_owner, item.repo_name, branch
        );
        let resp = agent.get(&zip_url).call();
        if let Ok(res) = resp {
            let mut bytes = Vec::new();
            if res.into_reader().read_to_end(&mut bytes).is_ok() && !bytes.is_empty() {
                downloaded_zip = Some(bytes);
                break;
            }
        }
    }

    if let Some(zip_bytes) = downloaded_zip {
        let reader = std::io::Cursor::new(zip_bytes);
        if let Ok(mut archive) = zip::ZipArchive::new(reader) {
            let temp_dir = std::env::temp_dir().join(format!("aitoolplus-skill-{}", uuid::Uuid::new_v4()));
            let _ = std::fs::create_dir_all(&temp_dir);

            for i in 0..archive.len() {
                if let Ok(mut file) = archive.by_index(i) {
                    let Some(enclosed_name) = file.enclosed_name().map(|p| p.to_path_buf()) else {
                        continue;
                    };
                    let outpath = temp_dir.join(enclosed_name);
                    if file.is_dir() {
                        let _ = std::fs::create_dir_all(&outpath);
                    } else {
                        if let Some(p) = outpath.parent() {
                            let _ = std::fs::create_dir_all(p);
                        }
                        if let Ok(mut outfile) = std::fs::File::create(&outpath) {
                            let _ = std::io::copy(&mut file, &mut outfile);
                        }
                    }
                }
            }

            fn find_skill(dir: &Path, target: &str, depth: usize) -> Option<PathBuf> {
                if depth > 4 {
                    return None;
                }
                let entries = std::fs::read_dir(dir).ok()?;
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_dir() {
                        if p.file_name().and_then(|n| n.to_str()) == Some(target)
                            && (p.join("SKILL.md").is_file() || p.join("skill.md").is_file())
                        {
                            return Some(p);
                        }
                        if let Some(found) = find_skill(&p, target, depth + 1) {
                            return Some(found);
                        }
                    }
                }
                None
            }

            let mut target_source = find_skill(&temp_dir, install_name, 0);

            if target_source.is_none() {
                fn find_any_skill(dir: &Path, depth: usize) -> Option<PathBuf> {
                    if depth > 4 {
                        return None;
                    }
                    let entries = std::fs::read_dir(dir).ok()?;
                    for entry in entries.flatten() {
                        let p = entry.path();
                        if p.is_dir() {
                            if p.join("SKILL.md").is_file() || p.join("skill.md").is_file() {
                                return Some(p);
                            }
                            if let Some(found) = find_any_skill(&p, depth + 1) {
                                return Some(found);
                            }
                        }
                    }
                    None
                }
                target_source = find_any_skill(&temp_dir, 0);
            }

            if let Some(src) = target_source {
                let dest = repo_dir.join(install_name);
                if dest.exists() {
                    let _ = remove_path(&dest);
                }
                let copy_res = copy_dir(&src, &dest);
                let _ = std::fs::remove_dir_all(&temp_dir);
                copy_res?;

                let mut skill = Skill::new(install_name, install_name);
                skill.source_type = "store".into();
                skill.source_ref = Some(format!("https://github.com/{}/{}", item.repo_owner, item.repo_name));
                skill.user_note = Some(item.description.clone());
                upsert(store, skill);
                scan_central(store, paths);
                return Ok(install_name.to_string());
            }
            let _ = std::fs::remove_dir_all(&temp_dir);
        }
    }

    // Fallback: use git clone if available
    let git_url = format!("https://github.com/{}/{}.git", item.repo_owner, item.repo_name);
    match crate::skill_git::install(store, paths, &git_url, Some(install_name)) {
        Ok(name) => Ok(name),
        Err(e) => Err(format!("从 GitHub 下载或克隆失败: {e}")),
    }
}

/// 从本地 ZIP 压缩包安装 Skill
/// 1. 解压 ZIP 到临时目录
/// 2. 递归扫描包含 SKILL.md 或 skill.md 的技能目录
/// 3. 复制到中央仓库并写入 SkillsStore
/// 4. 默认启用并在通用 Agent (~/.agents/skills) 中建立软链/Junction
pub fn install_from_zip(
    paths: &Paths,
    store: &mut SkillsStore,
    zip_path: &Path,
) -> Result<Vec<String>, String> {
    if !zip_path.exists() {
        return Err(format!("指定的 ZIP 文件不存在: {}", zip_path.display()));
    }

    let file = std::fs::File::open(zip_path).map_err(|e| format!("打开 ZIP 文件失败: {e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("解析 ZIP 压缩包失败: {e}"))?;

    let temp_dir = std::env::temp_dir().join(format!("aitoolplus-zip-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).map_err(|e| e.to_string())?;

    for i in 0..archive.len() {
        if let Ok(mut file) = archive.by_index(i) {
            let Some(enclosed_name) = file.enclosed_name().map(|p| p.to_path_buf()) else {
                continue;
            };
            let outpath = temp_dir.join(enclosed_name);
            if file.is_dir() {
                let _ = std::fs::create_dir_all(&outpath);
            } else {
                if let Some(p) = outpath.parent() {
                    let _ = std::fs::create_dir_all(p);
                }
                if let Ok(mut outfile) = std::fs::File::create(&outpath) {
                    let _ = std::io::copy(&mut file, &mut outfile);
                }
            }
        }
    }

    fn scan_skill_dirs(dir: &Path, depth: usize, acc: &mut Vec<PathBuf>) {
        if depth > 5 {
            return;
        }
        if dir.join("SKILL.md").is_file() || dir.join("skill.md").is_file() {
            acc.push(dir.to_path_buf());
            return;
        }
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    scan_skill_dirs(&p, depth + 1, acc);
                }
            }
        }
    }

    let mut found_skill_dirs = Vec::new();
    scan_skill_dirs(&temp_dir, 0, &mut found_skill_dirs);

    if found_skill_dirs.is_empty() {
        let _ = std::fs::remove_dir_all(&temp_dir);
        return Err("ZIP 压缩包中未找到包含 SKILL.md 的有效技能目录".to_string());
    }

    let zip_stem = zip_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("skill")
        .to_string();

    let repo_dir = central_repo_path(&store.settings, paths);
    std::fs::create_dir_all(&repo_dir).map_err(|e| e.to_string())?;

    let mut installed_names = Vec::new();

    for skill_dir in found_skill_dirs {
        let raw_name = if skill_dir == temp_dir {
            zip_stem.clone()
        } else {
            skill_dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&zip_stem)
                .to_string()
        };

        let install_name = raw_name
            .trim()
            .trim_end_matches(DISABLED_SUFFIX)
            .replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "-");
        if install_name.is_empty() {
            continue;
        }

        let dst = repo_dir.join(&install_name);
        if dst.exists() {
            let _ = remove_path(&dst);
        }
        copy_dir(&skill_dir, &dst)?;

        let desc = read_skill_description(&dst);

        let mut entry = Skill::new(&install_name, &install_name);
        entry.source_type = "zip".to_string();
        entry.source_ref = Some(zip_path.to_string_lossy().to_string());
        entry.description = desc;
        entry.origin_tool = Some(ToolId::Agents.key().to_string());
        entry.enabled_tools = vec![ToolId::Agents.key().to_string()];
        entry.tags = vec!["ZIP导入".to_string()];

        let _ = sync_skill_to_tool(&store.settings, paths, &mut entry, ToolId::Agents);
        upsert(store, entry);
        installed_names.push(install_name);
    }

    scan_central(store, paths);
    let _ = std::fs::remove_dir_all(&temp_dir);

    if installed_names.is_empty() {
        return Err("未成功安装任何有效技能".to_string());
    }

    Ok(installed_names)
}

// ---------------------------------------------------------------------------
// FS helpers
// ---------------------------------------------------------------------------

fn remove_dir_all_force(path: &Path) -> std::io::Result<()> {
    if let Err(_e) = std::fs::remove_dir_all(path) {
        fn make_writable(p: &Path) {
            if let Ok(entries) = std::fs::read_dir(p) {
                for entry in entries.flatten() {
                    let ep = entry.path();
                    if ep.is_dir() {
                        make_writable(&ep);
                    } else if let Ok(metadata) = ep.metadata() {
                        let mut perms = metadata.permissions();
                        if perms.readonly() {
                            perms.set_readonly(false);
                            let _ = std::fs::set_permissions(&ep, perms);
                        }
                    }
                }
            }
            if let Ok(metadata) = p.metadata() {
                let mut perms = metadata.permissions();
                if perms.readonly() {
                    perms.set_readonly(false);
                    let _ = std::fs::set_permissions(p, perms);
                }
            }
        }
        make_writable(path);
        std::fs::remove_dir_all(path)?;
    }
    Ok(())
}

fn remove_path(path: &Path) -> Result<(), String> {
    let meta = std::fs::symlink_metadata(path);
    match meta {
        Ok(m) if m.is_dir() && !m.file_type().is_symlink() => {
            remove_dir_all_force(path).map_err(|e| e.to_string())
        }
        Ok(_) => {
            let _ = std::fs::remove_dir(path);
            let _ = std::fs::remove_file(path);
            let _ = std::fs::remove_dir_all(path);
            Ok(())
        }
        Err(_) => Ok(()),
    }
}

fn copy_dir(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let ty = entry.file_type().map_err(|e| e.to_string())?;
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir(&entry.path(), &to)?;
        } else {
            std::fs::copy(entry.path(), &to).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn make_relative(target: &Path, source: &Path) -> PathBuf {
    // link sits in <tool>/skills/<name>; repo may be anywhere.
    let _ = target;
    source.to_path_buf()
}

#[cfg(windows)]
fn junction_or_copy(source: &Path, target: &Path) -> bool {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    // Remove any existing target file/directory/junction first
    let _ = remove_path(target);

    // Try creating an NTFS Directory Junction via cmd /c mklink /J (requires no admin privileges)
    let status = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(target)
        .arg(source)
        .creation_flags(CREATE_NO_WINDOW)
        .status();

    match status {
        Ok(s) if s.success() && (target.exists() || target.symlink_metadata().is_ok()) => true,
        _ => copy_dir(source, target).is_ok(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, Paths, SkillsStore) {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path().join("home"), dir.path().join("data"));
        let store = SkillsStore::default();
        // central repo with two skills
        let repo = central_repo_path(&store.settings, &paths);
        for name in ["alpha-skill", "beta-skill"] {
            let dir_path = repo.join(name);
            std::fs::create_dir_all(&dir_path).unwrap();
            std::fs::write(dir_path.join("SKILL.md"), format!("# {name}\n")).unwrap();
        }
        (dir, paths, store)
    }

    #[test]
    fn scan_central_discovers_and_refreshes() {
        let (_dir, paths, mut store) = setup();
        assert_eq!(scan_central(&mut store, &paths), 2);
        assert_eq!(scan_central(&mut store, &paths), 0); // idempotent
        let alpha = store
            .skills
            .iter()
            .find(|s| s.name == "alpha-skill")
            .unwrap();
        assert_eq!(alpha.status, "ok");
        assert_eq!(alpha.central_path, "alpha-skill");
    }

    #[test]
    fn central_repo_path_setting_wins() {
        let (_dir, paths, mut store) = setup();
        store.settings.central_repo_path = Some("/custom/repo".into());
        assert_eq!(
            central_repo_path(&store.settings, &paths),
            PathBuf::from("/custom/repo")
        );
        store.settings.central_repo_path = Some("".into());
        assert_eq!(
            central_repo_path(&store.settings, &paths),
            paths.app_data.join("skills")
        );
    }

    #[test]
    fn sync_and_remove_lifecycle_with_targets() {
        let (_dir, paths, mut store) = setup();
        scan_central(&mut store, &paths);
        let id = store.skills[0].id.clone();

        // enable for claude_code + codex
        assert_eq!(toggle_tool(&mut store, &id, ToolId::ClaudeCode), Some(true));
        assert_eq!(toggle_tool(&mut store, &id, ToolId::Codex), Some(true));

        let report = sync_all(&mut store, &paths);
        let claude = report
            .iter()
            .find(|(t, _, _)| *t == ToolId::ClaudeCode)
            .unwrap();
        assert_eq!(claude.1, 1, "expected one ok sync");
        assert_eq!(claude.2, 0);

        // tool dir contains the skill
        let installed = paths
            .home
            .join(".claude")
            .join("skills")
            .join("alpha-skill");
        assert!(installed.join("SKILL.md").exists());
        let codex_installed = paths.home.join(".codex").join("skills").join("alpha-skill");
        assert!(codex_installed.join("SKILL.md").exists());

        // sync_details recorded per tool
        let skill = store.skills.iter().find(|s| s.id == id).unwrap();
        let details = skill.sync_details.as_ref().unwrap();
        assert_eq!(details["claude_code"]["status"], "ok");
        assert!(
            details["claude_code"]["targetPath"]
                .as_str()
                .is_some_and(|p| p.contains("alpha-skill"))
        );

        // disable + sync removes from the tool
        assert_eq!(
            toggle_tool(&mut store, &id, ToolId::ClaudeCode),
            Some(false)
        );
        let _ = sync_all(&mut store, &paths);
        assert!(
            !paths
                .home
                .join(".claude")
                .join("skills")
                .join("alpha-skill")
                .exists(),
            "disabled skill not removed"
        );
        // codex install untouched
        assert!(codex_installed.join("SKILL.md").exists());
    }

    #[test]
    fn kimi_opencode_omp_dirs_supported() {
        let (_dir, paths, _store) = setup();
        assert_eq!(
            tool_skills_dir(&paths, ToolId::Kimi).unwrap(),
            paths.home.join(".kimi-code").join("skills")
        );
        assert_eq!(
            tool_skills_dir(&paths, ToolId::OpenCode).unwrap(),
            paths.home.join(".config").join("opencode").join("skill")
        );
        assert_eq!(
            tool_skills_dir(&paths, ToolId::OhMyPi).unwrap(),
            paths.home.join(".config").join("oh-my-pi").join("skills")
        );
    }

    #[test]
    fn install_into_central_registers_on_next_scan() {
        let (dir, paths, mut store) = setup();
        scan_central(&mut store, &paths);
        let ext = dir.path().join("external").join("new-skill");
        std::fs::create_dir_all(&ext).unwrap();
        std::fs::write(ext.join("SKILL.md"), "# new\n").unwrap();
        let name = install_into_central(&store.settings, &paths, &ext).unwrap();
        assert_eq!(name, "new-skill");
        assert_eq!(scan_central(&mut store, &paths), 1);
        assert!(store.skills.iter().any(|s| s.name == "new-skill"));
    }

    #[test]
    fn management_disabled_skips_sync() {
        let (_dir, paths, mut store) = setup();
        scan_central(&mut store, &paths);
        store.skills[0].management_enabled = false;
        store.skills[0].enabled_tools.push("claude_code".into());
        let report = sync_all(&mut store, &paths);
        assert!(report.is_empty());
        assert!(
            !paths
                .home
                .join(".claude")
                .join("skills")
                .join("alpha-skill")
                .exists()
        );
    }

    #[test]
    fn scan_and_import_existing_auto_enables_tool() {
        let (_dir, paths, mut store) = setup();
        let agents_skills = paths.home.join(".agents").join("skills").join("agents-skill");
        std::fs::create_dir_all(&agents_skills).unwrap();
        std::fs::write(agents_skills.join("SKILL.md"), "# agents-skill\n").unwrap();

        let imported = scan_and_import_existing(&paths, &mut store).unwrap();
        assert!(imported.contains(&"agents-skill".to_string()));

        let skill = store.skills.iter().find(|s| s.name == "agents-skill").unwrap();
        assert!(skill.is_enabled_in(ToolId::Agents));

        // Must remain a real directory, NOT a link!
        let is_link = agents_skills
            .symlink_metadata()
            .ok()
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false);
        assert!(!is_link, ".agents/skills must be a normal directory copy, not a link");
    }

    #[test]
    fn test_agents_tool_lifecycle() {
        let (_dir, paths, mut store) = setup();
        scan_central(&mut store, &paths);
        let id = store.skills[0].id.clone();

        // 1. Enable for Agents
        assert_eq!(toggle_tool(&mut store, &id, ToolId::Agents), Some(true));
        let skill = store.skills.iter_mut().find(|s| s.id == id).unwrap();
        sync_skill_to_tool(&store.settings, &paths, skill, ToolId::Agents).unwrap();

        let target = paths.home.join(".agents").join("skills").join("alpha-skill");
        assert!(target.join("SKILL.md").exists());
        let is_link = target
            .symlink_metadata()
            .ok()
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false);
        assert!(!is_link, "Agents skill must be a real directory copy, never a link/junction");

        let skill = store.skills.iter().find(|s| s.id == id).unwrap();
        assert!(skill.is_enabled_in(ToolId::Agents));
        let details = skill.sync_details.as_ref().unwrap();
        assert_eq!(details["agents"]["mode"], "copy");
        assert_eq!(details["agents"]["status"], "ok");

        // 2. Disable Agents: deletes target on disk and removes from enabled_tools
        assert_eq!(toggle_tool(&mut store, &id, ToolId::Agents), Some(false));
        let skill = store.skills.iter_mut().find(|s| s.id == id).unwrap();
        remove_skill_from_tool(&store.settings, &paths, skill, ToolId::Agents).unwrap();

        assert!(!target.exists(), "Target in .agents/skills must be deleted when disabled");
        assert!(!skill.is_enabled_in(ToolId::Agents));

        // 3. Scan again when target does not exist: stays disabled
        let _ = scan_and_import_existing(&paths, &mut store).unwrap();
        let skill = store.skills.iter().find(|s| s.id == id).unwrap();
        assert!(!skill.is_enabled_in(ToolId::Agents));
    }

    #[cfg(windows)]
    #[test]
    fn test_junction_creation_and_removal() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("source_skill");
        let dst = dir.path().join("target_skill");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("SKILL.md"), "# test\n").unwrap();

        assert!(junction_or_copy(&src, &dst));
        assert!(dst.join("SKILL.md").is_file());

        // Test remove_path does not delete source
        remove_path(&dst).unwrap();
        assert!(!dst.exists());
        assert!(src.join("SKILL.md").is_file());
    }

    #[test]
    fn test_restore_all_to_plain() {
        let (_dir, paths, mut store) = setup();
        let agents_skills = paths.home.join(".agents").join("skills").join("agents-skill");
        std::fs::create_dir_all(&agents_skills).unwrap();
        std::fs::write(agents_skills.join("SKILL.md"), "# agents-skill\n").unwrap();

        let _ = scan_and_import_existing(&paths, &mut store).unwrap();
        let skill = store.skills.iter().find(|s| s.name == "agents-skill").unwrap();
        assert_eq!(skill.origin_tool, Some("agents".to_string()));

        let (restored, errors) = restore_all_to_plain(&mut store, &paths);
        assert!(restored > 0);
        assert_eq!(errors, 0);

        // Verify that ~/.agents/skills/agents-skill is a regular directory and contains SKILL.md
        let target = paths.home.join(".agents").join("skills").join("agents-skill");
        assert!(target.join("SKILL.md").is_file());
        let is_link = target
            .symlink_metadata()
            .ok()
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false);
        assert!(!is_link, "Restored skill directory must not be a symlink/junction");
    }

    #[test]
    fn test_discover_repo_skills_and_repo_management() {
        let (_dir, _paths, mut store) = setup();
        assert!(!store.settings.repos.is_empty());

        let all = discover_repo_skills(&store, None, None, None);
        assert!(!all.is_empty());

        // Filter by repo
        let anthropic_skills = discover_repo_skills(&store, Some("anthropics/skills"), None, None);
        assert!(anthropic_skills.iter().all(|s| s.repo_owner == "anthropics"));

        // Add custom repo
        add_skill_repo(&mut store.settings, SkillRepo {
            owner: "my-org".into(),
            name: "my-custom-skills".into(),
            branch: "main".into(),
            enabled: true,
        });
        let with_custom = discover_repo_skills(&store, Some("my-org/my-custom-skills"), None, None);
        assert_eq!(with_custom.len(), 1);
        assert_eq!(with_custom[0].name, "my-custom-skills");

        // Remove custom repo
        remove_skill_repo(&mut store.settings, "my-org", "my-custom-skills");
        let after_remove = discover_repo_skills(&store, Some("my-org/my-custom-skills"), None, None);
        assert!(after_remove.is_empty());
    }

    #[test]
    #[ignore]
    fn test_online_search_tencent_browser() {
        let res = search_skills_store("tencent browser", 20, 0).expect("search skills store");
        println!("Total count: {}", res.count);
        for (i, skill) in res.skills.iter().enumerate() {
            println!("#{}: id={}, name={}, source={}", i + 1, skill.id, skill.name, skill.source);
        }
        assert!(!res.skills.is_empty());
        assert!(res.skills[0].source.to_lowercase().contains("tencent") || res.skills[0].name.to_lowercase().contains("tencent"));
    }

    #[test]
    fn test_install_from_zip() {
        use std::io::Write;
        let (_dir, paths, mut store) = setup();

        let temp_dir = tempfile::tempdir().unwrap();
        let zip_path = temp_dir.path().join("my-test-skill.zip");
        let zip_file = std::fs::File::create(&zip_path).unwrap();
        let mut zip_writer = zip::ZipWriter::new(zip_file);

        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        zip_writer.start_file("my-test-skill/SKILL.md", options).unwrap();
        zip_writer.write_all(b"# My Test Skill\nDescription of my test skill.\n").unwrap();
        zip_writer.finish().unwrap();

        let installed = install_from_zip(&paths, &mut store, &zip_path).unwrap();
        assert_eq!(installed, vec!["my-test-skill".to_string()]);

        let skill = store.skills.iter().find(|s| s.name == "my-test-skill").unwrap();
        assert_eq!(skill.source_type, "zip");
        assert!(skill.tags.contains(&"ZIP导入".to_string()));
        assert!(paths.home.join(".agents").join("skills").join("my-test-skill").join("SKILL.md").exists());
    }
}

