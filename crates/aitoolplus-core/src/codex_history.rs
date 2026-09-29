//! Codex session history unification, migration, and restore engine.
//!
//! Direct parity with cc-switch battle-tested engine:
//! - Codex CLI filters sessions in resume/history by `model_provider` drawer:
//!   - Official subscriptions use built-in `openai` drawer
//!   - Third-party providers use `custom` drawer
//! - Unifying session history:
//!   1. Live TOML injection: routes official ChatGPT OAuth via `custom` provider table
//!      with `requires_openai_auth = true`, `supports_websockets = true`, `wire_api = "responses"`.
//!   2. Live TOML stripping: cleanly removes `model_provider = "custom"` and table when turned off.
//!   3. Stock migration: rewrites `session_meta.payload.model_provider: "openai"` -> `"custom"` in JSONL
//!      and SQLite `threads.model_provider` with online SQLite snapshot backup.
//!   4. Reversible restore: restores strictly based on backup ledgers (`collect_official_ledger`),
//!      never touching sessions created during unification.

use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use toml_edit::DocumentMut;

use crate::paths::Paths;
use crate::tools::ToolId;

pub const OFFICIAL_OPENAI_CODEX_MODEL_PROVIDER_ID: &str = "openai";
pub const UNIFIED_CODEX_MODEL_PROVIDER_ID: &str = "custom";

pub const OFFICIAL_UNIFY_MIGRATION_NAME: &str = "codex-official-history-unify-v1";
pub const OFFICIAL_UNIFY_RESTORE_BACKUP_NAME: &str = "codex-official-history-unify-restore-v1";
pub const CODEX_STATE_DB_FILENAME: &str = "state_5.sqlite";
const CODEX_SQLITE_HOME_ENV: &str = "CODEX_SQLITE_HOME";
const STATE_DB_ID_CHUNK: usize = 500;

/// Serializes Codex official history migration and restore operations across threads.
static CODEX_OFFICIAL_HISTORY_LOCK: Mutex<()> = Mutex::new(());

fn lock_op() -> std::sync::MutexGuard<'static, ()> {
    CODEX_OFFICIAL_HISTORY_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodexHistoryMigrationOutcome {
    pub source_provider_ids: Vec<String>,
    pub migrated_jsonl_files: usize,
    pub migrated_state_rows: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skipped_reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodexHistoryRestoreOutcome {
    pub restored_jsonl_files: usize,
    pub restored_state_rows: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skipped_reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodexOfficialHistoryUnifyMigration {
    pub completed_at: String,
    pub target_provider_id: String,
    pub migrated_jsonl_files: usize,
    pub migrated_state_rows: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex_config_dir: Option<String>,
}

// ---------------------------------------------------------------------------
// Path & SQLite Helpers
// ---------------------------------------------------------------------------

pub fn canonical_dir_string(dir: &Path) -> String {
    fs::canonicalize(dir)
        .unwrap_or_else(|_| dir.to_path_buf())
        .to_string_lossy()
        .to_string()
}

fn resolve_user_path(raw: &str, home: &Path) -> PathBuf {
    if raw == "~" {
        return home.to_path_buf();
    }
    if let Some(rest) = raw.strip_prefix("~/") {
        return home.join(rest);
    }
    if let Some(rest) = raw.strip_prefix("~\\") {
        return home.join(rest);
    }
    PathBuf::from(raw)
}

fn sqlite_home_from_codex_config(config_text: &str, home: &Path) -> Option<PathBuf> {
    let doc = config_text.parse::<DocumentMut>().ok()?;
    let raw = doc.get("sqlite_home")?.as_str()?.trim();
    if raw.is_empty() {
        return None;
    }
    Some(resolve_user_path(raw, home))
}

fn sqlite_home_from_env(home: &Path) -> Option<PathBuf> {
    let raw = std::env::var(CODEX_SQLITE_HOME_ENV).ok()?;
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    Some(resolve_user_path(raw, home))
}

pub fn codex_state_db_paths(codex_dir: &Path, config_text: &str, home: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let primary = codex_dir.join(CODEX_STATE_DB_FILENAME);
    paths.push(primary);

    if let Some(sqlite_home) = sqlite_home_from_codex_config(config_text, home) {
        let p = sqlite_home.join(CODEX_STATE_DB_FILENAME);
        if !paths.contains(&p) {
            paths.push(p);
        }
    } else if let Some(sqlite_home) = sqlite_home_from_env(home) {
        let p = sqlite_home.join(CODEX_STATE_DB_FILENAME);
        if !paths.contains(&p) {
            paths.push(p);
        }
    }
    paths
}

pub fn collect_jsonl_files(dir: &Path, files: &mut Vec<PathBuf>, depth: u8, max_depth: u8) {
    if depth > max_depth || !dir.is_dir() {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_jsonl_files(&path, files, depth + 1, max_depth);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("jsonl") {
            files.push(path);
        }
    }
}

pub fn collect_files_with_extension(
    dir: &Path,
    extension: &str,
    files: &mut Vec<PathBuf>,
    depth: u8,
    max_depth: u8,
) {
    if depth > max_depth || !dir.is_dir() {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files_with_extension(&path, extension, files, depth + 1, max_depth);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some(extension) {
            files.push(path);
        }
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension(format!(
        "tmp.{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        e.to_string()
    })?;
    Ok(())
}

fn relative_backup_path(path: &Path, root: &Path) -> PathBuf {
    if let Ok(relative) = path.strip_prefix(root) {
        return relative.to_path_buf();
    }

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    let hash = hasher.finish();
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".to_string());
    PathBuf::from("external").join(format!("{hash:016x}-{file_name}"))
}

fn backup_codex_jsonl_file(
    path: &Path,
    codex_dir: &Path,
    backup_root: &Path,
) -> Result<(), String> {
    let backup_path = backup_root
        .join("jsonl")
        .join(relative_backup_path(path, codex_dir));
    if let Some(parent) = backup_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::copy(path, &backup_path).map_err(|e| e.to_string())?;
    Ok(())
}

fn backup_codex_state_db(
    db_path: &Path,
    codex_dir: &Path,
    backup_root: &Path,
    source_conn: &Connection,
) -> Result<(), String> {
    let backup_path = backup_root
        .join("state")
        .join(relative_backup_path(db_path, codex_dir));
    if let Some(parent) = backup_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let mut backup_conn = Connection::open(&backup_path)
        .map_err(|e| format!("创建 Codex state DB 备份失败: {e}"))?;
    let backup = rusqlite::backup::Backup::new(source_conn, &mut backup_conn)
        .map_err(|e| format!("初始化 Codex state DB 备份失败: {e}"))?;
    backup
        .run_to_completion(5, Duration::from_millis(25), None)
        .map_err(|e| format!("写入 Codex state DB 备份失败: {e}"))?;
    Ok(())
}

fn table_exists(conn: &Connection, table: &str) -> Result<bool, String> {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [table],
        |row| row.get::<_, i64>(0),
    )
    .map(|c| c > 0)
    .map_err(|e| e.to_string())
}

fn has_column(conn: &Connection, table: &str, column: &str) -> Result<bool, String> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|e| e.to_string())?;
    let columns: HashSet<String> = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| e.to_string())?
        .flatten()
        .collect();
    Ok(columns.contains(column))
}

fn placeholders(count: usize) -> String {
    std::iter::repeat_n("?", count)
        .collect::<Vec<_>>()
        .join(", ")
}

// ---------------------------------------------------------------------------
// Config Injection & Stripping (Exact cc-switch Parity)
// ---------------------------------------------------------------------------

pub fn codex_unified_official_provider_table() -> toml_edit::Table {
    let mut table = toml_edit::Table::new();
    table.insert("name", toml_edit::value("OpenAI"));
    table.insert("requires_openai_auth", toml_edit::value(true));
    table.insert("supports_websockets", toml_edit::value(true));
    table.insert("wire_api", toml_edit::value("responses"));
    table
}

pub fn table_matches_codex_unified_official_provider(table: &toml_edit::Table) -> bool {
    table.len() == 4
        && table.get("name").and_then(|i| i.as_str()) == Some("OpenAI")
        && table.get("requires_openai_auth").and_then(|i| i.as_bool()) == Some(true)
        && table.get("supports_websockets").and_then(|i| i.as_bool()) == Some(true)
        && table.get("wire_api").and_then(|i| i.as_str()) == Some("responses")
}

/// Injects unified official `custom` provider route into Codex `config.toml`.
/// Only injects when no explicit model_provider is set (or matches standard openai seed).
pub fn inject_codex_unified_session_bucket(config_text: &str) -> Result<String, String> {
    let mut doc = if config_text.trim().is_empty() {
        toml_edit::DocumentMut::new()
    } else {
        config_text
            .parse::<DocumentMut>()
            .map_err(|e| format!("Invalid Codex config.toml: {e}"))?
    };

    if let Some(curr) = doc.get("model_provider").and_then(|v| v.as_str()) {
        if curr != OFFICIAL_OPENAI_CODEX_MODEL_PROVIDER_ID
            && curr != UNIFIED_CODEX_MODEL_PROVIDER_ID
        {
            // Do not override user's explicit custom provider
            return Ok(config_text.to_string());
        }
    }

    let existing_custom_conflicts = doc
        .get("model_providers")
        .and_then(|i| i.as_table())
        .and_then(|p| p.get(UNIFIED_CODEX_MODEL_PROVIDER_ID))
        .and_then(|i| i.as_table())
        .is_some_and(|t| !table_matches_codex_unified_official_provider(t));

    if existing_custom_conflicts {
        // User has conflicting custom provider table (e.g. third-party base_url).
        // Refuse injection to avoid routing official auth to unknown backend.
        return Ok(config_text.to_string());
    }

    doc["model_provider"] = toml_edit::value(UNIFIED_CODEX_MODEL_PROVIDER_ID);

    if doc.get("model_providers").is_none() {
        let mut parent = toml_edit::Table::new();
        parent.set_implicit(true);
        doc["model_providers"] = toml_edit::Item::Table(parent);
    }
    if let Some(providers) = doc["model_providers"].as_table_mut() {
        providers.remove(OFFICIAL_OPENAI_CODEX_MODEL_PROVIDER_ID);
        if !providers.contains_key(UNIFIED_CODEX_MODEL_PROVIDER_ID) {
            providers.insert(
                UNIFIED_CODEX_MODEL_PROVIDER_ID,
                toml_edit::Item::Table(codex_unified_official_provider_table()),
            );
        }
    }

    Ok(doc.to_string())
}

/// Strips the injected unified official provider from config text, removing
/// `model_provider` completely (per cc-switch parity, since official Codex
/// CLI uses implicit openai default).
pub fn strip_codex_unified_session_bucket(config_text: &str) -> Result<String, String> {
    if !config_text.contains("model_provider") {
        return Ok(config_text.to_string());
    }
    let mut doc = config_text
        .parse::<DocumentMut>()
        .map_err(|e| format!("Invalid Codex config.toml: {e}"))?;

    if doc.get("model_provider").and_then(|i| i.as_str()) != Some(UNIFIED_CODEX_MODEL_PROVIDER_ID)
    {
        return Ok(config_text.to_string());
    }

    let matches_injected = doc
        .get("model_providers")
        .and_then(|i| i.as_table())
        .and_then(|p| p.get(UNIFIED_CODEX_MODEL_PROVIDER_ID))
        .and_then(|i| i.as_table())
        .is_some_and(table_matches_codex_unified_official_provider);

    if !matches_injected {
        return Ok(config_text.to_string());
    }

    doc.as_table_mut().remove("model_provider");
    let providers_empty = doc["model_providers"]
        .as_table_mut()
        .map(|providers| {
            providers.remove(UNIFIED_CODEX_MODEL_PROVIDER_ID);
            providers.is_empty()
        })
        .unwrap_or(false);
    if providers_empty {
        doc.as_table_mut().remove("model_providers");
    }

    Ok(doc.to_string())
}

// ---------------------------------------------------------------------------
// Migration Engine: JSONL & SQLite
// ---------------------------------------------------------------------------

fn ensure_codex_session_file_unchanged(
    path: &Path,
    modified_before: Option<SystemTime>,
    len_before: u64,
) -> Result<(), String> {
    let meta = fs::metadata(path).map_err(|e| e.to_string())?;
    if meta.modified().ok() != modified_before || meta.len() != len_before {
        return Err(format!(
            "Codex session file changed concurrently during migration: {}",
            path.display()
        ));
    }
    Ok(())
}

fn rewrite_codex_session_file_lines(
    path: &Path,
    codex_dir: &Path,
    backup_root: &Path,
    rewrite_line: impl Fn(&str) -> Option<String>,
) -> Result<bool, String> {
    let meta = fs::metadata(path).map_err(|e| e.to_string())?;
    let modified_before = meta.modified().ok();
    let len_before = meta.len();
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;

    let mut rewritten = String::with_capacity(content.len());
    let mut changed = false;

    for segment in content.split_inclusive('\n') {
        let (line, newline) = segment
            .strip_suffix('\n')
            .map(|l| (l, "\n"))
            .unwrap_or((segment, ""));
        if let Some(next_line) = rewrite_line(line) {
            rewritten.push_str(&next_line);
            changed = true;
        } else {
            rewritten.push_str(line);
        }
        rewritten.push_str(newline);
    }

    if !changed {
        return Ok(false);
    }

    ensure_codex_session_file_unchanged(path, modified_before, len_before)?;
    backup_codex_jsonl_file(path, codex_dir, backup_root)?;
    ensure_codex_session_file_unchanged(path, modified_before, len_before)?;
    write_atomic(path, rewritten.as_bytes())?;
    Ok(true)
}

fn rewrite_codex_session_meta_line(
    line: &str,
    source_provider_ids: &HashSet<String>,
) -> Option<String> {
    if !line.contains("\"session_meta\"") || !line.contains("\"model_provider\"") {
        return None;
    }
    let mut value: Value = serde_json::from_str(line).ok()?;
    if value.get("type").and_then(Value::as_str) != Some("session_meta") {
        return None;
    }
    let payload = value.get_mut("payload")?.as_object_mut()?;
    let current_provider = payload.get("model_provider")?.as_str()?;
    if !source_provider_ids.contains(current_provider) {
        return None;
    }
    payload.insert(
        "model_provider".to_string(),
        Value::String(UNIFIED_CODEX_MODEL_PROVIDER_ID.to_string()),
    );
    serde_json::to_string(&value).ok()
}

fn migrate_codex_jsonl_files(
    codex_dir: &Path,
    source_provider_ids: &HashSet<String>,
    backup_root: &Path,
) -> Result<usize, String> {
    let mut files = Vec::new();
    collect_jsonl_files(&codex_dir.join("sessions"), &mut files, 0, 8);
    collect_jsonl_files(&codex_dir.join("archived_sessions"), &mut files, 0, 4);

    let mut count = 0;
    for file_path in files {
        if rewrite_codex_session_file_lines(&file_path, codex_dir, backup_root, |line| {
            rewrite_codex_session_meta_line(line, source_provider_ids)
        })? {
            count += 1;
        }
    }
    Ok(count)
}

fn migrate_codex_state_dbs(
    codex_dir: &Path,
    source_provider_ids: &BTreeSet<String>,
    backup_root: &Path,
    config_text: &str,
    home: &Path,
) -> Result<usize, String> {
    let mut total_rows = 0;
    for db_path in codex_state_db_paths(codex_dir, config_text, home) {
        total_rows += migrate_codex_state_db_provider_bucket(
            &db_path,
            codex_dir,
            source_provider_ids,
            backup_root,
        )?;
    }
    Ok(total_rows)
}

fn migrate_codex_state_db_provider_bucket(
    db_path: &Path,
    codex_dir: &Path,
    source_provider_ids: &BTreeSet<String>,
    backup_root: &Path,
) -> Result<usize, String> {
    if !db_path.exists() || source_provider_ids.is_empty() {
        return Ok(0);
    }

    let mut conn = Connection::open(db_path)
        .map_err(|e| format!("打开 Codex state DB 失败: {e}"))?;
    conn.busy_timeout(Duration::from_secs(5))
        .map_err(|e| format!("设置 Codex state DB busy_timeout 失败: {e}"))?;

    let has_threads = table_exists(&conn, "threads")?
        && has_column(&conn, "threads", "model_provider")?;
    if !has_threads {
        return Ok(0);
    }

    let ph = placeholders(source_provider_ids.len());
    let count_sql = format!("SELECT COUNT(*) FROM threads WHERE model_provider IN ({ph})");
    let matching_rows: i64 = conn
        .query_row(
            &count_sql,
            rusqlite::params_from_iter(source_provider_ids.iter()),
            |row| row.get(0),
        )
        .map_err(|e| format!("统计 Codex state DB 待迁移行失败: {e}"))?;
    if matching_rows == 0 {
        return Ok(0);
    }

    backup_codex_state_db(db_path, codex_dir, backup_root, &conn)?;

    let update_sql =
        format!("UPDATE threads SET model_provider = ? WHERE model_provider IN ({ph})");
    let mut values = Vec::with_capacity(source_provider_ids.len() + 1);
    values.push(UNIFIED_CODEX_MODEL_PROVIDER_ID.to_string());
    values.extend(source_provider_ids.iter().cloned());
    let tx = conn
        .transaction()
        .map_err(|e| format!("开启 Codex state DB 迁移事务失败: {e}"))?;
    let changed = tx
        .execute(&update_sql, rusqlite::params_from_iter(values.iter()))
        .map_err(|e| format!("迁移 Codex state DB provider 失败: {e}"))?;
    tx.commit()
        .map_err(|e| format!("提交 Codex state DB 迁移事务失败: {e}"))?;
    Ok(changed)
}

fn write_backup_generation_meta(backup_root: &Path, codex_dir_key: &str) -> Result<(), String> {
    if !backup_root.exists() {
        return Ok(());
    }
    let payload = serde_json::json!({ "codexConfigDir": codex_dir_key });
    let bytes = serde_json::to_vec_pretty(&payload).map_err(|e| e.to_string())?;
    write_atomic(&backup_root.join("meta.json"), &bytes)
}

/// Executes the migration of official Codex history into the shared `custom` bucket.
pub fn migrate_codex_official_history_unify(
    paths: &Paths,
) -> Result<CodexHistoryMigrationOutcome, String> {
    let _guard = lock_op();
    let codex_dir = paths.tool_root(ToolId::Codex);
    let codex_dir_key = canonical_dir_string(&codex_dir);

    let config_file = codex_dir.join("config.toml");
    let config_text = if config_file.exists() {
        fs::read_to_string(&config_file).unwrap_or_default()
    } else {
        String::new()
    };

    let source_ids: HashSet<String> =
        [OFFICIAL_OPENAI_CODEX_MODEL_PROVIDER_ID.to_string()].into();
    let source_btree: BTreeSet<String> =
        [OFFICIAL_OPENAI_CODEX_MODEL_PROVIDER_ID.to_string()].into();

    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    let backup_root = paths
        .app_data
        .join("backups")
        .join(OFFICIAL_UNIFY_MIGRATION_NAME)
        .join(&timestamp);

    let migrated_jsonl_files = migrate_codex_jsonl_files(&codex_dir, &source_ids, &backup_root)?;
    let migrated_state_rows = migrate_codex_state_dbs(
        &codex_dir,
        &source_btree,
        &backup_root,
        &config_text,
        &paths.home,
    )?;

    if backup_root.exists() {
        write_backup_generation_meta(&backup_root, &codex_dir_key)?;
    }

    Ok(CodexHistoryMigrationOutcome {
        source_provider_ids: source_ids.into_iter().collect(),
        migrated_jsonl_files,
        migrated_state_rows,
        skipped_reason: None,
    })
}

// ---------------------------------------------------------------------------
// Restore Engine: Based on Backup Ledger (Exact cc-switch Parity)
// ---------------------------------------------------------------------------

fn backup_generation_matches_dir(generation: &Path, codex_dir_key: &str) -> bool {
    let Ok(text) = fs::read_to_string(generation.join("meta.json")) else {
        return true;
    };
    serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|val| {
            val.get("codexConfigDir")
                .and_then(Value::as_str)
                .map(|dir| dir == codex_dir_key)
        })
        .unwrap_or(true)
}

fn collect_official_session_ids_from_backup(path: &Path, session_ids: &mut HashSet<String>) {
    let Ok(content) = fs::read_to_string(path) else {
        return;
    };
    for line in content.lines() {
        if !line.contains("\"session_meta\"") || !line.contains("\"model_provider\"") {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if value.get("type").and_then(Value::as_str) != Some("session_meta") {
            continue;
        }
        let Some(payload) = value.get("payload") else {
            continue;
        };
        if payload.get("model_provider").and_then(Value::as_str)
            != Some(OFFICIAL_OPENAI_CODEX_MODEL_PROVIDER_ID)
        {
            continue;
        }
        if let Some(session_id) = payload.get("id").and_then(Value::as_str) {
            session_ids.insert(session_id.to_string());
        }
    }
}

fn collect_official_thread_ids_from_backup(db_path: &Path, thread_ids: &mut BTreeSet<String>) {
    let Ok(conn) = Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
    else {
        return;
    };
    let Ok(mut stmt) = conn.prepare("SELECT id FROM threads WHERE model_provider = ?1") else {
        return;
    };
    let Ok(rows) = stmt.query_map([OFFICIAL_OPENAI_CODEX_MODEL_PROVIDER_ID], |row| {
        row.get::<_, String>(0)
    }) else {
        return;
    };
    for thread_id in rows.flatten() {
        thread_ids.insert(thread_id);
    }
}

pub fn collect_official_ledger(
    ledger_parent: &Path,
    codex_dir_key: &str,
) -> (HashSet<String>, BTreeSet<String>) {
    let mut session_ids = HashSet::new();
    let mut thread_ids = BTreeSet::new();

    let Ok(entries) = fs::read_dir(ledger_parent) else {
        return (session_ids, thread_ids);
    };

    for entry in entries.flatten() {
        let generation = entry.path();
        if !generation.is_dir() {
            continue;
        }
        if !backup_generation_matches_dir(&generation, codex_dir_key) {
            continue;
        }
        let mut backup_files = Vec::new();
        collect_jsonl_files(&generation.join("jsonl"), &mut backup_files, 0, 10);
        for backup_file in backup_files {
            collect_official_session_ids_from_backup(&backup_file, &mut session_ids);
        }

        let mut backup_dbs = Vec::new();
        collect_files_with_extension(&generation.join("state"), "sqlite", &mut backup_dbs, 0, 4);
        for backup_db in backup_dbs {
            collect_official_thread_ids_from_backup(&backup_db, &mut thread_ids);
        }
    }

    (session_ids, thread_ids)
}

fn rewrite_codex_session_meta_line_for_restore(
    line: &str,
    official_session_ids: &HashSet<String>,
) -> Option<String> {
    if !line.contains("\"session_meta\"") || !line.contains("\"model_provider\"") {
        return None;
    }
    let mut value: Value = serde_json::from_str(line).ok()?;
    if value.get("type").and_then(Value::as_str) != Some("session_meta") {
        return None;
    }
    let payload = value.get_mut("payload")?.as_object_mut()?;
    if payload.get("model_provider")?.as_str()? != UNIFIED_CODEX_MODEL_PROVIDER_ID {
        return None;
    }
    let session_id = payload.get("id")?.as_str()?;
    if !official_session_ids.contains(session_id) {
        return None;
    }
    payload.insert(
        "model_provider".to_string(),
        Value::String(OFFICIAL_OPENAI_CODEX_MODEL_PROVIDER_ID.to_string()),
    );
    serde_json::to_string(&value).ok()
}

fn restore_codex_state_db_official_threads(
    db_path: &Path,
    codex_dir: &Path,
    official_thread_ids: &BTreeSet<String>,
    backup_root: &Path,
) -> Result<usize, String> {
    if !db_path.exists() || official_thread_ids.is_empty() {
        return Ok(0);
    }

    let mut conn = Connection::open(db_path)
        .map_err(|e| format!("打开 Codex state DB 失败: {e}"))?;
    conn.busy_timeout(Duration::from_secs(5))
        .map_err(|e| format!("设置 Codex state DB busy_timeout 失败: {e}"))?;

    let has_threads = table_exists(&conn, "threads")?
        && has_column(&conn, "threads", "model_provider")?;
    if !has_threads {
        return Ok(0);
    }

    let ids: Vec<&String> = official_thread_ids.iter().collect();
    let mut matching_rows: i64 = 0;
    for chunk in ids.chunks(STATE_DB_ID_CHUNK) {
        let ph = placeholders(chunk.len());
        let count_sql = format!(
            "SELECT COUNT(*) FROM threads WHERE model_provider = ? AND id IN ({ph})"
        );
        let mut values = Vec::with_capacity(chunk.len() + 1);
        values.push(UNIFIED_CODEX_MODEL_PROVIDER_ID.to_string());
        values.extend(chunk.iter().map(|id| (*id).clone()));
        let count: i64 = conn
            .query_row(&count_sql, rusqlite::params_from_iter(values.iter()), |row| {
                row.get(0)
            })
            .map_err(|e| format!("统计 Codex state DB 待还原行失败: {e}"))?;
        matching_rows += count;
    }
    if matching_rows == 0 {
        return Ok(0);
    }

    backup_codex_state_db(db_path, codex_dir, backup_root, &conn)?;

    let tx = conn
        .transaction()
        .map_err(|e| format!("开启 Codex state DB 还原事务失败: {e}"))?;
    let mut changed = 0;
    for chunk in ids.chunks(STATE_DB_ID_CHUNK) {
        let ph = placeholders(chunk.len());
        let update_sql = format!(
            "UPDATE threads SET model_provider = ? WHERE model_provider = ? AND id IN ({ph})"
        );
        let mut values = Vec::with_capacity(chunk.len() + 2);
        values.push(OFFICIAL_OPENAI_CODEX_MODEL_PROVIDER_ID.to_string());
        values.push(UNIFIED_CODEX_MODEL_PROVIDER_ID.to_string());
        values.extend(chunk.iter().map(|id| (*id).clone()));
        changed += tx
            .execute(&update_sql, rusqlite::params_from_iter(values.iter()))
            .map_err(|e| format!("还原 Codex state DB provider 失败: {e}"))?;
    }
    tx.commit()
        .map_err(|e| format!("提交 Codex state DB 还原事务失败: {e}"))?;
    Ok(changed)
}

pub fn has_codex_official_history_unify_backup(paths: &Paths) -> bool {
    let ledger_parent = paths
        .app_data
        .join("backups")
        .join(OFFICIAL_UNIFY_MIGRATION_NAME);
    let codex_dir = paths.tool_root(ToolId::Codex);
    let codex_dir_key = canonical_dir_string(&codex_dir);

    let Ok(entries) = fs::read_dir(ledger_parent) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let p = entry.path();
        p.is_dir() && backup_generation_matches_dir(&p, &codex_dir_key)
    })
}

/// Restores previously migrated official sessions back to `model_provider: "openai"`.
pub fn restore_codex_official_history_from_backups(
    paths: &Paths,
) -> Result<CodexHistoryRestoreOutcome, String> {
    let _guard = lock_op();
    let codex_dir = paths.tool_root(ToolId::Codex);
    let codex_dir_key = canonical_dir_string(&codex_dir);

    let ledger_parent = paths
        .app_data
        .join("backups")
        .join(OFFICIAL_UNIFY_MIGRATION_NAME);
    let (official_session_ids, official_thread_ids) =
        collect_official_ledger(&ledger_parent, &codex_dir_key);

    if official_session_ids.is_empty() && official_thread_ids.is_empty() {
        return Ok(CodexHistoryRestoreOutcome {
            skipped_reason: Some("no_backup_ledger".to_string()),
            ..Default::default()
        });
    }

    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    let restore_backup_root = paths
        .app_data
        .join("backups")
        .join(OFFICIAL_UNIFY_RESTORE_BACKUP_NAME)
        .join(&timestamp);

    let mut files = Vec::new();
    collect_jsonl_files(&codex_dir.join("sessions"), &mut files, 0, 8);
    collect_jsonl_files(&codex_dir.join("archived_sessions"), &mut files, 0, 4);

    let mut restored_jsonl_files = 0;
    for file_path in files {
        if rewrite_codex_session_file_lines(
            &file_path,
            &codex_dir,
            &restore_backup_root,
            |line| rewrite_codex_session_meta_line_for_restore(line, &official_session_ids),
        )? {
            restored_jsonl_files += 1;
        }
    }

    let config_file = codex_dir.join("config.toml");
    let config_text = if config_file.exists() {
        fs::read_to_string(&config_file).unwrap_or_default()
    } else {
        String::new()
    };

    let mut restored_state_rows = 0;
    for db_path in codex_state_db_paths(&codex_dir, &config_text, &paths.home) {
        restored_state_rows += restore_codex_state_db_official_threads(
            &db_path,
            &codex_dir,
            &official_thread_ids,
            &restore_backup_root,
        )?;
    }

    if restored_jsonl_files == 0 && restored_state_rows == 0 {
        return Ok(CodexHistoryRestoreOutcome {
            skipped_reason: Some("nothing_to_restore".to_string()),
            ..Default::default()
        });
    }

    Ok(CodexHistoryRestoreOutcome {
        restored_jsonl_files,
        restored_state_rows,
        skipped_reason: None,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
pub mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_inject_and_strip_codex_unified_session_bucket() {
        let base_config = "";
        let injected = inject_codex_unified_session_bucket(base_config).unwrap();
        assert!(injected.contains("model_provider = \"custom\""));
        assert!(injected.contains("[model_providers.custom]"));
        assert!(injected.contains("requires_openai_auth = true"));
        assert!(injected.contains("supports_websockets = true"));
        assert!(injected.contains("wire_api = \"responses\""));

        // Stripping should cleanly remove custom route
        let stripped = strip_codex_unified_session_bucket(&injected).unwrap();
        assert_eq!(stripped.trim(), "");

        // Stripping must NOT touch third party custom providers that have different fields
        let third_party = r#"model_provider = "custom"

[model_providers.custom]
name = "AIHubMix"
base_url = "https://aihubmix.example/v1"
wire_api = "responses"
"#;
        let untouched = strip_codex_unified_session_bucket(third_party).unwrap();
        assert_eq!(untouched, third_party);
    }

    #[test]
    fn test_conflict_custom_table_refuses_injection() {
        let conflict = r#"[model_providers.custom]
name = "MyCustomRelay"
base_url = "https://custom.relay/v1"
"#;
        let res = inject_codex_unified_session_bucket(conflict).unwrap();
        assert_eq!(res, conflict);
    }

    #[test]
    fn test_migration_and_restore_roundtrip() {
        let dir = tempdir().unwrap();
        let codex_dir = dir.path().join(".codex");
        fs::create_dir_all(codex_dir.join("sessions/2026/06")).unwrap();
        fs::create_dir_all(dir.path().join(".aitoolplus")).unwrap();

        let paths = Paths::new(dir.path(), dir.path().join(".aitoolplus"));

        let session_file = codex_dir.join("sessions/2026/06/session1.jsonl");
        let initial_jsonl = concat!(
            "{\"type\":\"session_meta\",\"payload\":{\"id\":\"s1\",\"model_provider\":\"openai\"}}\n",
            "{\"type\":\"session_meta\",\"payload\":{\"id\":\"s2\",\"model_provider\":\"custom\"}}\n",
            "{\"type\":\"response_item\",\"payload\":{\"text\":\"hello world\"}}\n"
        );
        fs::write(&session_file, initial_jsonl).unwrap();

        let db_path = codex_dir.join(CODEX_STATE_DB_FILENAME);
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, model_provider TEXT NOT NULL);
             INSERT INTO threads (id, model_provider) VALUES ('t1', 'openai'), ('t2', 'custom');"
        ).unwrap();
        drop(conn);

        // 1. Run Migration
        let outcome = migrate_codex_official_history_unify(&paths).unwrap();
        assert_eq!(outcome.migrated_jsonl_files, 1);
        assert_eq!(outcome.migrated_state_rows, 1);

        // Verify session_meta is now custom
        let migrated_text = fs::read_to_string(&session_file).unwrap();
        assert!(migrated_text.contains("{\"type\":\"session_meta\",\"payload\":{\"id\":\"s1\",\"model_provider\":\"custom\"}}"));
        assert!(migrated_text.contains("{\"type\":\"session_meta\",\"payload\":{\"id\":\"s2\",\"model_provider\":\"custom\"}}"));

        // Verify DB is now custom
        let conn = Connection::open(&db_path).unwrap();
        let count_openai: i64 = conn.query_row("SELECT COUNT(*) FROM threads WHERE model_provider = 'openai'", [], |r| r.get(0)).unwrap();
        let count_custom: i64 = conn.query_row("SELECT COUNT(*) FROM threads WHERE model_provider = 'custom'", [], |r| r.get(0)).unwrap();
        assert_eq!(count_openai, 0);
        assert_eq!(count_custom, 2);
        drop(conn);

        // Add a new session during unified mode (s3)
        let new_session = format!(
            "{}{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"s3\",\"model_provider\":\"custom\"}}}}\n",
            migrated_text
        );
        fs::write(&session_file, new_session).unwrap();

        // 2. Run Restore
        let restore_outcome = restore_codex_official_history_from_backups(&paths).unwrap();
        assert_eq!(restore_outcome.restored_jsonl_files, 1);
        assert_eq!(restore_outcome.restored_state_rows, 1);

        // Verify s1 is restored to openai, while s2 and s3 remain custom!
        let restored_text = fs::read_to_string(&session_file).unwrap();
        assert!(restored_text.contains("{\"type\":\"session_meta\",\"payload\":{\"id\":\"s1\",\"model_provider\":\"openai\"}}"));
        assert!(restored_text.contains("{\"type\":\"session_meta\",\"payload\":{\"id\":\"s2\",\"model_provider\":\"custom\"}}"));
        assert!(restored_text.contains("{\"type\":\"session_meta\",\"payload\":{\"id\":\"s3\",\"model_provider\":\"custom\"}}"));

        // Verify DB t1 is restored to openai, t2 remains custom
        let conn = Connection::open(&db_path).unwrap();
        let count_openai: i64 = conn.query_row("SELECT COUNT(*) FROM threads WHERE model_provider = 'openai'", [], |r| r.get(0)).unwrap();
        let count_custom: i64 = conn.query_row("SELECT COUNT(*) FROM threads WHERE model_provider = 'custom'", [], |r| r.get(0)).unwrap();
        assert_eq!(count_openai, 1);
        assert_eq!(count_custom, 1);
    }
}
