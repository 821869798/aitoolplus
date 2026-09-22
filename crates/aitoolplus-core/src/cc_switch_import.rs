//! CC-Switch database importer.
//!
//! Imports provider configurations from `~/.cc-switch/cc-switch.db` into AI ToolPlus.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use rusqlite::Connection;
use serde_json::Value;

use crate::paths::Paths;
use crate::providers::ProviderRecord;
use crate::store::Store;
use crate::tools::ToolId;

#[derive(Debug, Clone, Default)]
pub struct CcSwitchImportReport {
    pub total_found: usize,
    pub imported_count: usize,
    pub updated_count: usize,
    pub per_tool: HashMap<ToolId, usize>,
}

/// Detect the default ~/.cc-switch/cc-switch.db path if it exists.
pub fn default_cc_switch_db_path(paths: &Paths) -> PathBuf {
    paths.home.join(".cc-switch").join("cc-switch.db")
}

pub fn detect_cc_switch_db(paths: &Paths) -> Option<PathBuf> {
    let p = default_cc_switch_db_path(paths);
    if p.is_file() {
        Some(p)
    } else {
        None
    }
}

pub fn import_from_cc_switch(
    paths: &Paths,
    store: &mut Store,
    custom_db_path: Option<&Path>,
) -> Result<CcSwitchImportReport, String> {
    let db_path = custom_db_path
        .map(Path::to_path_buf)
        .or_else(|| detect_cc_switch_db(paths))
        .ok_or_else(|| "未找到 cc-switch.db 数据库文件 (cc-switch.db not found)".to_string())?;

    if !db_path.exists() {
        return Err(format!("文件不存在: {}", db_path.display()));
    }

    let conn = Connection::open(&db_path)
        .map_err(|e| format!("打开 cc-switch.db 失败: {e}"))?;

    let mut stmt = conn
        .prepare("SELECT rowid, id, app_type, name, settings_config, website_url, category, notes, meta, is_current FROM providers ORDER BY rowid ASC")
        .map_err(|e| format!("查询 providers 失败: {e}"))?;

    struct RowData {
        _rowid: i64,
        id: String,
        app_type: String,
        name: String,
        settings_config: Option<String>,
        website_url: Option<String>,
        category: Option<String>,
        notes: Option<String>,
        meta: Option<String>,
        is_current: Option<i32>,
    }

    let rows = stmt
        .query_map([], |row| {
            Ok(RowData {
                _rowid: row.get(0)?,
                id: row.get(1)?,
                app_type: row.get(2)?,
                name: row.get(3)?,
                settings_config: row.get(4)?,
                website_url: row.get(5)?,
                category: row.get(6)?,
                notes: row.get(7)?,
                meta: row.get(8)?,
                is_current: row.get(9)?,
            })
        })
        .map_err(|e| format!("读取行失败: {e}"))?;

    let mut report = CcSwitchImportReport::default();
    let mut touched_tools = std::collections::HashSet::new();

    for row in rows {
        let r = match row {
            Ok(r) => r,
            Err(_) => continue,
        };
        report.total_found += 1;

        let tool_id = match r.app_type.as_str() {
            "claude" => ToolId::ClaudeCode,
            "claude-desktop" => ToolId::ClaudeDesktop,
            "codex" => ToolId::Codex,
            "gemini" => ToolId::GeminiCli,
            "opencode" => ToolId::OpenCode,
            "pi" => ToolId::Pi,
            "grok" | "grokbuild" => ToolId::Grok,
            "openclaw" => ToolId::OpenClaw,
            "hermes" => ToolId::Hermes,
            "kimi" => ToolId::Kimi,
            _ => continue,
        };

        let is_current = r.is_current.unwrap_or(0) == 1;
        touched_tools.insert(tool_id);

        let target_id = if tool_id == ToolId::Pi && !r.id.starts_with("pi:") {
            format!("pi:{}", r.id)
        } else {
            r.id.clone()
        };

        let mut settings_config = r.settings_config.unwrap_or_else(|| "{}".to_string());
        if tool_id == ToolId::Pi {
            if let Ok(mut val) = serde_json::from_str::<Value>(&settings_config) {
                if let Some(obj) = val.as_object_mut() {
                    if !obj.contains_key("_providerKey") {
                        obj.insert("_providerKey".into(), Value::String(r.id.clone()));
                        settings_config = serde_json::to_string_pretty(&val).unwrap_or(settings_config);
                    }
                }
            }
        } else if tool_id == ToolId::Codex {
            if let Ok(mut val) = serde_json::from_str::<Value>(&settings_config) {
                if let Some(obj) = val.as_object_mut() {
                    // Mirror config to toml so both representations are available
                    if let Some(cfg_str) = obj.get("config").and_then(Value::as_str) {
                        if !obj.contains_key("toml") {
                            obj.insert("toml".into(), Value::String(cfg_str.to_string()));
                            settings_config = serde_json::to_string_pretty(&val).unwrap_or(settings_config);
                        }
                    }
                }
            }
        }

        let meta_val: Option<Value> = r.meta.as_deref().and_then(|m| serde_json::from_str(m).ok());
        let category = r.category.unwrap_or_else(|| "custom".to_string());

        let section = store.tool_mut(tool_id);

        // For single-provider tools: when an active provider is imported from CC-Switch,
        // clean up temporary live:* fallback records and unmark existing applied providers.
        if tool_id != ToolId::Pi && is_current {
            section.providers.retain(|p| !p.id.starts_with("live:"));
            for p in section.providers.iter_mut() {
                p.is_applied = false;
            }
        }

        let existing = section
            .providers
            .iter_mut()
            .find(|p| !p.id.starts_with("live:") && (p.id == target_id || p.name == r.name));

        if let Some(p) = existing {
            if settings_config != "{}" && !settings_config.trim().is_empty() {
                p.settings_config = settings_config;
            }
            p.name = r.name;
            p.category = category;
            if r.website_url.is_some() {
                p.website_url = r.website_url;
            }
            if meta_val.is_some() {
                p.meta = meta_val;
            }
            if r.notes.is_some() {
                p.notes = r.notes;
            }
            if tool_id == ToolId::Pi {
                p.is_applied = true;
            } else if is_current {
                p.is_applied = true;
            }
            p.touch();
            report.updated_count += 1;
        } else {
            let max_idx = section.providers.iter().map(|p| p.sort_index).max().unwrap_or(-1);
            let mut record = ProviderRecord::new(&r.name, &category);
            record.id = target_id;
            record.sort_index = max_idx + 1;
            record.settings_config = settings_config;
            record.website_url = r.website_url;
            record.meta = meta_val;
            record.notes = r.notes.or_else(|| {
                if tool_id == ToolId::Pi {
                    Some(format!("Pi runtime provider: {}", r.id))
                } else {
                    None
                }
            });
            record.is_applied = is_current;
            section.providers.push(record);
            report.imported_count += 1;
            *report.per_tool.entry(tool_id).or_insert(0) += 1;
        }
    }

    // Post-import sanitation:
    // 1. For all single-provider tools: if real providers exist from CC-Switch,
    //    remove stale temporary live:* fallback records.
    // 2. Enforce that strictly at most ONE provider per tool has is_applied == true.
    for &tool_id in &touched_tools {
        if tool_id == ToolId::Pi {
            continue;
        }
        let section = store.tool_mut(tool_id);
        let has_real = section.providers.iter().any(|p| !p.id.starts_with("live:"));
        if has_real {
            section.providers.retain(|p| !p.id.starts_with("live:"));
        }
        let mut seen_applied = false;
        for p in section.providers.iter_mut() {
            if p.is_applied {
                if seen_applied {
                    p.is_applied = false;
                } else {
                    seen_applied = true;
                }
            }
        }
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_cc_switch_import_flow() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("cc-switch.db");
        let conn = Connection::open(&db_path).unwrap();

        conn.execute(
            "CREATE TABLE providers (
                id TEXT PRIMARY KEY,
                app_type TEXT NOT NULL,
                name TEXT NOT NULL,
                settings_config TEXT,
                website_url TEXT,
                category TEXT,
                notes TEXT,
                meta TEXT,
                is_current INTEGER
            )",
            [],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO providers (id, app_type, name, settings_config, is_current) VALUES
            ('p1', 'pi', 'tokenrouter1', '{\"apiKey\":\"sk-1\"}', 0),
            ('c1', 'claude', 'DeepSeek', '{\"env\":{}}', 0)",
            [],
        )
        .unwrap();

        let paths = Paths::new(dir.path().join("home"), dir.path().join("appdata"));
        let mut store = Store::new();

        let report = import_from_cc_switch(&paths, &mut store, Some(&db_path)).unwrap();
        assert_eq!(report.imported_count, 2);
        assert_eq!(report.total_found, 2);

        let pi_provs = &store.tool(ToolId::Pi).providers;
        assert!(pi_provs.iter().any(|p| p.name == "tokenrouter1"));

        let claude_provs = &store.tool(ToolId::ClaudeCode).providers;
        assert!(claude_provs.iter().any(|p| p.name == "DeepSeek"));
    }

    #[test]
    fn test_cc_switch_import_single_applied_dedup() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("cc-switch.db");
        let conn = Connection::open(&db_path).unwrap();

        conn.execute(
            "CREATE TABLE providers (
                id TEXT PRIMARY KEY,
                app_type TEXT NOT NULL,
                name TEXT NOT NULL,
                settings_config TEXT,
                website_url TEXT,
                category TEXT,
                notes TEXT,
                meta TEXT,
                is_current INTEGER
            )",
            [],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO providers (id, app_type, name, settings_config, is_current) VALUES
            ('claude-anyrouter', 'claude', 'anyrouter', '{\"env\":{\"ANTHROPIC_BASE_URL\":\"https://anyrouter.top\"}}', 1),
            ('claude-deepseek', 'claude', 'DeepSeek', '{\"env\":{}}', 0)",
            [],
        )
        .unwrap();

        let paths = Paths::new(dir.path().join("home"), dir.path().join("appdata"));
        let mut store = Store::new();

        // Simulate initial state where aitoolplus bootstrap had created `live:claude_code` with is_applied = true
        let mut live_rec = ProviderRecord::new("anyrouter.top", "custom");
        live_rec.id = "live:claude_code".to_string();
        live_rec.is_applied = true;
        store.tool_mut(ToolId::ClaudeCode).providers.push(live_rec);

        // Also simulate an official provider present
        let mut official = ProviderRecord::new("Claude Official", "official");
        official.id = "claude-official".to_string();
        official.is_applied = false;
        store.tool_mut(ToolId::ClaudeCode).providers.push(official);

        let report = import_from_cc_switch(&paths, &mut store, Some(&db_path)).unwrap();
        assert_eq!(report.imported_count, 2);

        let claude_provs = &store.tool(ToolId::ClaudeCode).providers;
        // Verify live:claude_code was removed
        assert!(!claude_provs.iter().any(|p| p.id.starts_with("live:")));

        // Verify exactly one provider is applied and it is `anyrouter`
        let applied: Vec<_> = claude_provs.iter().filter(|p| p.is_applied).collect();
        assert_eq!(applied.len(), 1);
        assert_eq!(applied[0].name, "anyrouter");
        assert_eq!(applied[0].id, "claude-anyrouter");
    }

    #[test]
    fn test_import_live_cc_switch_db_if_exists() {
        let home = std::env::var("USERPROFILE").unwrap_or_else(|_| ".".into());
        let db_path = PathBuf::from(home).join(".cc-switch").join("cc-switch.db");
        if !db_path.exists() {
            return;
        }
        let dir = tempdir().unwrap();
        let paths = Paths::new(dir.path().join("home"), dir.path().join("appdata"));
        let mut store = Store::new();

        let report = import_from_cc_switch(&paths, &mut store, Some(&db_path)).unwrap();
        println!("Report: {:#?}", report);
        for (tool, count) in &report.per_tool {
            println!("Tool {:?}: {} providers", tool, count);
        }
        for p in &store.tool(ToolId::Codex).providers {
            let (u, k) = p.resolve_credentials(ToolId::Codex);
            println!("Codex provider: id={}, name={}, is_applied={}, url={}, has_key={}", p.id, p.name, p.is_applied, u, !k.is_empty());
            if p.category != "official" {
                assert!(!u.is_empty(), "Codex provider {} missing url", p.name);
                assert!(!k.is_empty(), "Codex provider {} missing key", p.name);
            }
        }
        for p in &store.tool(ToolId::OpenCode).providers {
            let (u, k) = p.resolve_credentials(ToolId::OpenCode);
            println!("OpenCode provider: id={}, name={}, is_applied={}, url={}, has_key={}", p.id, p.name, p.is_applied, u, !k.is_empty());
            assert!(!u.is_empty(), "OpenCode provider {} missing url", p.name);
        }
        for p in &store.tool(ToolId::ClaudeCode).providers {
            let (u, k) = p.resolve_credentials(ToolId::ClaudeCode);
            println!("Claude provider: id={}, name={}, is_applied={}, url={}, has_key={}", p.id, p.name, p.is_applied, u, !k.is_empty());
            if p.category != "official" {
                assert!(!u.is_empty(), "Claude provider {} missing url", p.name);
                assert!(!k.is_empty(), "Claude provider {} missing key", p.name);
            }
        }
        for p in &store.tool(ToolId::Pi).providers {
            let (u, k) = p.resolve_credentials(ToolId::Pi);
            println!("Pi provider: id={}, name={}, is_applied={}, url={}, has_key={}", p.id, p.name, p.is_applied, u, !k.is_empty());
            assert!(!u.is_empty(), "Pi provider {} missing url", p.name);
            assert!(!k.is_empty(), "Pi provider {} missing key", p.name);
        }
    }

    #[test]
    #[ignore]
    fn test_reimport_into_user_store_json_live() {
        let home = std::env::var("USERPROFILE").unwrap_or_else(|_| ".".into());
        let db_path = PathBuf::from(&home).join(".cc-switch").join("cc-switch.db");
        let store_path = PathBuf::from(&home).join(".aitoolplus").join("store.json");
        if !db_path.exists() || !store_path.exists() {
            return;
        }
        let paths = Paths::system();
        let mut store_handle = match crate::store::StoreHandle::open(&paths) {
            Ok(h) => h,
            Err(_) => return,
        };
        let report = import_from_cc_switch(&paths, store_handle.store_mut(), Some(&db_path)).unwrap();
        println!("Reimported into live store: {:?}", report);
        store_handle.save().unwrap();
    }
}

