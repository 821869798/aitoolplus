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
        }
    }

    pub fn touch(&mut self) {
        self.updated_at = now_ms();
    }

    pub fn is_enabled_in(&self, tool: ToolId) -> bool {
        self.enabled_tools.iter().any(|k| k == tool.key())
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

/// The single authoritative central repo path.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub central_repo_path: Option<String>,
    #[serde(default)]
    pub preferences: SkillPreferences,
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
        if let Some(existing) = store.skills.iter_mut().find(|s| s.name == name) {
            // refresh central path + hash fields only
            existing.central_path = name.clone();
            existing.status = if dir.join("SKILL.md").is_file() || dir.join("skill.md").is_file() {
                "ok".into()
            } else {
                "error".into()
            };
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
    if !ok {
        // fallback to plain copy
        copy_dir(&source, &target)?;
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
        let target_record = SkillTarget {
            tool: tool.key().to_string(),
            target_path: target.to_string_lossy().to_string(),
            mode: install_mode().into(),
            status: "pending".into(), // removed from tool; would sync if re-enabled
            synced_at: Some(now_ms()),
            error_message: None,
        };
        skill.record_target(&target_record);
    }
    Ok(())
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
// FS helpers
// ---------------------------------------------------------------------------

fn remove_path(path: &Path) -> Result<(), String> {
    let meta = std::fs::symlink_metadata(path);
    match meta {
        Ok(m) if m.is_dir() && !m.file_type().is_symlink() => {
            std::fs::remove_dir_all(path).map_err(|e| e.to_string())
        }
        Ok(_) => {
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
    // std has no junction API; fall back to copy (robust and always correct).
    let _ = source;
    let _ = target;
    false
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
}
