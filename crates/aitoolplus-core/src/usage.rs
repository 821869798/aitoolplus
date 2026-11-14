//! Usage statistics, analytics, and session ingestion engine.
//!
//! Replicates CC-Switch's usage tracking architecture:
//! - SQLite storage (`proxy_request_logs`, `model_pricing`, `usage_daily_rollups`, etc.)
//! - Multi-tool token usage aggregation (Claude, Codex, Gemini, Grok, OpenCode, Pi)
//! - Provider breakdown, model breakdown, hourly/daily trends, and paginated logs.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use chrono::{Local, TimeZone};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::paths::Paths;

// ---------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------

/// Usage summary metrics.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsageSummary {
    pub total_requests: u64,
    pub total_cost: String,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_creation_tokens: u64,
    pub total_cache_read_tokens: u64,
    pub success_rate: f32,
    pub real_total_tokens: u64,
    pub cache_hit_rate: f64,
}

/// Per-app usage breakdown.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSummaryByApp {
    pub app_type: String,
    pub summary: UsageSummary,
}

/// Daily/hourly time series bucket.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DailyStats {
    pub date: String,
    pub request_count: u64,
    pub total_cost: String,
    pub total_tokens: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_creation_tokens: u64,
    pub total_cache_read_tokens: u64,
}

/// Provider breakdown row.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderStats {
    pub provider_id: String,
    pub provider_name: String,
    pub request_count: u64,
    pub total_tokens: u64,
    pub total_cost: String,
    pub success_rate: f32,
    pub avg_latency_ms: u64,
}

/// Model breakdown row.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStats {
    pub model: String,
    pub request_count: u64,
    pub total_tokens: u64,
    pub total_cost: String,
    pub avg_cost_per_request: String,
}

/// Request log filter criteria.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogFilters {
    pub app_type: Option<String>,
    #[serde(default)]
    pub allowed_apps: Option<Vec<String>>,
    pub provider_name: Option<String>,
    pub model: Option<String>,
    pub status_code: Option<u16>,
    pub start_date: Option<i64>,
    pub end_date: Option<i64>,
}

/// Detailed single request log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestLogDetail {
    pub request_id: String,
    pub provider_id: String,
    pub provider_name: Option<String>,
    pub app_type: String,
    pub model: String,
    pub request_model: Option<String>,
    pub cost_multiplier: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_read_tokens: u32,
    pub cache_creation_tokens: u32,
    pub total_cost_usd: String,
    pub is_streaming: bool,
    pub latency_ms: u64,
    pub status_code: u16,
    pub error_message: Option<String>,
    pub created_at: i64,
    pub data_source: Option<String>,
    pub pricing_model: Option<String>,
}

/// Paginated request log response.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PaginatedLogs {
    pub data: Vec<RequestLogDetail>,
    pub total: u32,
    pub page: u32,
    pub page_size: u32,
}

/// Model pricing record.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPricingInfo {
    pub model_id: String,
    pub display_name: String,
    pub input_cost_per_million: String,
    pub output_cost_per_million: String,
    pub cache_read_cost_per_million: String,
    pub cache_creation_cost_per_million: String,
}

/// Global application pricing defaults (default multiplier & pricing model source).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppPricingConfig {
    pub app_type: String,
    pub cost_multiplier: String,
    pub pricing_model_source: String, // "response" | "request"
}

/// Session sync result report.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSyncResult {
    pub imported: u32,
    pub skipped: u32,
    pub files_scanned: u32,
    pub suspected_duplicates: u32,
    pub deferred_files: u32,
    pub errors: Vec<String>,
}

/// CC-Switch usage database migration report.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CcSwitchUsageImportReport {
    pub logs_imported: usize,
    pub pricing_imported: usize,
    pub rollups_imported: usize,
}

// ---------------------------------------------------------------------------
// Database Service
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct UsageDb {
    pub db_path: PathBuf,
}

impl UsageDb {
    /// Open or create the usage database at `paths.app_data/usage.db`.
    /// If `usage.db` does not exist yet, but `~/.cc-switch/cc-switch.db` exists,
    /// automatically copy it as initial seed data.
    pub fn open(paths: &Paths) -> Result<Self, String> {
        let db_path = paths.app_data.join("usage.db");

        if !db_path.exists() {
            if let Some(parent) = db_path.parent() {
                let _ = fs::create_dir_all(parent);
            }

            let cc_switch_db = paths.home.join(".cc-switch").join("cc-switch.db");
            if cc_switch_db.is_file() {
                if let Err(e) = fs::copy(&cc_switch_db, &db_path) {
                    tracing::warn!("Failed to copy cc-switch.db to usage.db: {e}");
                } else {
                    tracing::info!("Seeded usage.db from existing cc-switch.db");
                }
            }
        }

        let instance = Self { db_path };
        instance.init_tables()?;
        instance.ensure_default_pricing()?;

        Ok(instance)
    }

    /// Connect to the SQLite file.
    ///
    /// Reads and the session scan now run on different threads, so a connection
    /// has to wait out a brief write lock instead of failing immediately.
    pub fn connect(&self) -> Result<Connection, String> {
        let conn = Connection::open(&self.db_path)
            .map_err(|e| format!("打开使用统计数据库失败: {e}"))?;
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        Ok(conn)
    }

    /// Initialize SQLite schema if missing.
    fn init_tables(&self) -> Result<(), String> {
        let conn = self.connect()?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS proxy_request_logs (
                request_id TEXT PRIMARY KEY,
                provider_id TEXT NOT NULL,
                app_type TEXT NOT NULL,
                model TEXT NOT NULL,
                request_model TEXT,
                pricing_model TEXT,
                input_tokens INTEGER NOT NULL DEFAULT 0,
                output_tokens INTEGER NOT NULL DEFAULT 0,
                cache_read_tokens INTEGER NOT NULL DEFAULT 0,
                cache_creation_tokens INTEGER NOT NULL DEFAULT 0,
                input_token_semantics INTEGER NOT NULL DEFAULT 0,
                input_cost_usd TEXT NOT NULL DEFAULT '0',
                output_cost_usd TEXT NOT NULL DEFAULT '0',
                cache_read_cost_usd TEXT NOT NULL DEFAULT '0',
                cache_creation_cost_usd TEXT NOT NULL DEFAULT '0',
                total_cost_usd TEXT NOT NULL DEFAULT '0',
                latency_ms INTEGER NOT NULL DEFAULT 0,
                first_token_ms INTEGER,
                duration_ms INTEGER,
                status_code INTEGER NOT NULL DEFAULT 200,
                error_message TEXT,
                session_id TEXT,
                provider_type TEXT,
                is_streaming INTEGER NOT NULL DEFAULT 0,
                cost_multiplier TEXT NOT NULL DEFAULT '1.0',
                created_at INTEGER NOT NULL,
                data_source TEXT NOT NULL DEFAULT 'proxy'
            )",
            [],
        )
        .map_err(|e| format!("创建 proxy_request_logs 失败: {e}"))?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS model_pricing (
                model_id TEXT PRIMARY KEY,
                display_name TEXT NOT NULL,
                input_cost_per_million TEXT NOT NULL,
                output_cost_per_million TEXT NOT NULL,
                cache_read_cost_per_million TEXT NOT NULL DEFAULT '0',
                cache_creation_cost_per_million TEXT NOT NULL DEFAULT '0'
            )",
            [],
        )
        .map_err(|e| format!("创建 model_pricing 失败: {e}"))?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS app_pricing_config (
                app_type TEXT PRIMARY KEY,
                cost_multiplier TEXT NOT NULL DEFAULT '1.0',
                pricing_model_source TEXT NOT NULL DEFAULT 'response'
            )",
            [],
        )
        .map_err(|e| format!("创建 app_pricing_config 失败: {e}"))?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS usage_daily_rollups (
                date TEXT NOT NULL,
                provider_id TEXT NOT NULL,
                app_type TEXT NOT NULL,
                model TEXT NOT NULL,
                request_count INTEGER NOT NULL DEFAULT 0,
                success_count INTEGER NOT NULL DEFAULT 0,
                input_tokens INTEGER NOT NULL DEFAULT 0,
                output_tokens INTEGER NOT NULL DEFAULT 0,
                cache_read_tokens INTEGER NOT NULL DEFAULT 0,
                cache_creation_tokens INTEGER NOT NULL DEFAULT 0,
                total_cost_usd TEXT NOT NULL DEFAULT '0',
                avg_latency_ms INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (date, provider_id, app_type, model)
            )",
            [],
        )
        .map_err(|e| format!("创建 usage_daily_rollups 失败: {e}"))?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS session_log_sync (
                file_path TEXT PRIMARY KEY,
                last_modified INTEGER NOT NULL,
                last_line_offset INTEGER NOT NULL DEFAULT 0,
                last_synced_at INTEGER NOT NULL,
                last_byte_offset INTEGER,
                last_tail_fingerprint INTEGER
            )",
            [],
        )
        .map_err(|e| format!("创建 session_log_sync 失败: {e}"))?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS session_usage_dedup (
                data_source TEXT NOT NULL,
                request_id TEXT NOT NULL,
                semantic_id TEXT NOT NULL,
                has_entry_id INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (data_source, request_id)
            )",
            [],
        )
        .map_err(|e| format!("创建 session_usage_dedup 失败: {e}"))?;

        let _ = conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_session_usage_dedup_semantic ON session_usage_dedup(data_source, semantic_id)",
            [],
        );

        let _ = conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_request_logs_session ON proxy_request_logs(session_id)",
            [],
        );

        let _ = conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_request_logs_dedup_cols ON proxy_request_logs(app_type, data_source, input_tokens, output_tokens, cache_read_tokens, created_at, cache_creation_tokens)",
            [],
        );

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_request_logs_created_at ON proxy_request_logs(created_at)",
            [],
        )
        .map_err(|e| format!("创建索引失败: {e}"))?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_request_logs_app_created_at ON proxy_request_logs(app_type, created_at DESC)",
            [],
        )
        .map_err(|e| format!("创建索引失败: {e}"))?;

        Ok(())
    }

    /// Seed default model pricing rows if the table is empty.
    fn ensure_default_pricing(&self) -> Result<(), String> {
        let conn = self.connect()?;
        let count: i64 = conn
            .query_row("SELECT count(*) FROM model_pricing", [], |r| r.get(0))
            .unwrap_or(0);
        if count > 0 {
            return Ok(());
        }

        let seed_data = [
            ("claude-3-5-sonnet-20241022", "Claude 3.5 Sonnet", "3.0", "15.0", "0.30", "3.75"),
            ("claude-3-5-haiku-20241022", "Claude 3.5 Haiku", "0.80", "4.0", "0.08", "1.0"),
            ("claude-3-opus-20240229", "Claude 3 Opus", "15.0", "75.0", "1.50", "18.75"),
            ("claude-sonnet-4-6", "Claude Sonnet 4.6", "3.0", "15.0", "0.30", "3.75"),
            ("claude-opus-4-6", "Claude Opus 4.6", "5.0", "25.0", "0.50", "6.25"),
            ("gpt-4o", "GPT-4o", "2.50", "10.0", "1.25", "0"),
            ("gpt-4o-mini", "GPT-4o Mini", "0.15", "0.60", "0.075", "0"),
            ("o1", "OpenAI o1", "15.0", "60.0", "7.50", "0"),
            ("o1-mini", "OpenAI o1 Mini", "3.0", "12.0", "1.50", "0"),
            ("o3-mini", "OpenAI o3 Mini", "1.10", "4.40", "0.55", "0"),
            ("gemini-2.0-flash", "Gemini 2.0 Flash", "0.10", "0.40", "0.025", "0"),
            ("gemini-2.0-pro-exp", "Gemini 2.0 Pro", "1.25", "5.0", "0.3125", "0"),
            ("gemini-3.8-flash", "Gemini 3.8 Flash", "0.75", "3.75", "0.075", "0"),
            ("gemini-4-argon", "Gemini 4 Argon", "2.00", "10.00", "0.10", "0"),
            ("claude-sonnet-5-5", "Claude Sonnet 5.5", "2.0", "10.0", "0.10", "2.50"),
            ("claude-sonnet-5.5", "Claude Sonnet 5.5", "2.0", "10.0", "0.10", "2.50"),
            ("claude-opus-5-5", "Claude Opus 5.5", "4.0", "20.0", "0.20", "5.0"),
            ("claude-opus-5.5", "Claude Opus 5.5", "4.0", "20.0", "0.20", "5.0"),
            ("deepseek-chat", "DeepSeek V3", "0.14", "0.28", "0.014", "0"),
            ("deepseek-reasoner", "DeepSeek R1", "0.55", "2.19", "0.14", "0"),
            ("grok-2", "Grok 2", "2.0", "10.0", "0", "0"),
            ("grok-3", "Grok 3", "3.0", "15.0", "0.30", "0"),
            ("qwen-max", "Qwen Max", "2.40", "9.60", "0.60", "0"),
            ("qwen-plus", "Qwen Plus", "0.40", "1.20", "0.10", "0"),
        ];

        let mut stmt = conn
            .prepare(
                "INSERT OR IGNORE INTO model_pricing (model_id, display_name, input_cost_per_million, output_cost_per_million, cache_read_cost_per_million, cache_creation_cost_per_million)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .map_err(|e| format!("准备插入 pricing 失败: {e}"))?;

        for (id, name, inp, out, cr, cc) in seed_data {
            let _ = stmt.execute(params![id, name, inp, out, cr, cc]);
        }

        // Seed default app pricing defaults
        let app_seeds = [
            ("claude", "1.0", "response"),
            ("codex", "1.0", "response"),
            ("agy", "1.0", "response"),
            ("gemini", "1.0", "response"),
            ("grok", "1.0", "response"),
            ("opencode", "1.0", "response"),
            ("pi", "1.0", "response"),
            ("oh_my_pi", "1.0", "response"),
            ("kimi", "1.0", "response"),
            ("openclaw", "1.0", "response"),
            ("hermes", "1.0", "response"),
            ("dsh", "1.0", "response"),
            ("claude_desktop", "1.0", "response"),
        ];
        if let Ok(mut app_stmt) = conn.prepare(
            "INSERT OR IGNORE INTO app_pricing_config (app_type, cost_multiplier, pricing_model_source) VALUES (?1, ?2, ?3)",
        ) {
            for (app, mult, src) in app_seeds {
                let _ = app_stmt.execute(params![app, mult, src]);
            }
        }

        Ok(())
    }

    /// Import all data from CC-Switch database into this usage database using ATTACH DATABASE.
    pub fn import_from_cc_switch(&self, cc_switch_db_path: &Path) -> Result<CcSwitchUsageImportReport, String> {
        if !cc_switch_db_path.is_file() {
            return Err(format!("CC-Switch 数据库不存在: {}", cc_switch_db_path.display()));
        }

        let mtime_stamp = Self::cc_switch_db_mtime(cc_switch_db_path);
        let conn = self.connect()?;
        let attach_path = cc_switch_db_path.to_string_lossy().replace('\\', "/");

        conn.execute(&format!("ATTACH DATABASE '{}' AS src;", attach_path), [])
            .map_err(|e| format!("附加 CC-Switch 数据库失败: {e}"))?;

        let logs_count: usize = conn
            .execute(
                "INSERT OR IGNORE INTO main.proxy_request_logs SELECT * FROM src.proxy_request_logs;",
                [],
            )
            .unwrap_or(0);

        let pricing_count: usize = conn
            .execute(
                "INSERT OR REPLACE INTO main.model_pricing SELECT * FROM src.model_pricing;",
                [],
            )
            .unwrap_or(0);

        let rollups_count: usize = conn
            .execute(
                "INSERT OR REPLACE INTO main.usage_daily_rollups SELECT * FROM src.usage_daily_rollups;",
                [],
            )
            .unwrap_or(0);

        // Use INSERT OR IGNORE so CC-Switch's 10-digit second cursors never overwrite
        // aitoolplus's 13-digit millisecond cursors and trigger full session re-scans.
        let _ = conn.execute("INSERT OR IGNORE INTO main.session_log_sync SELECT * FROM src.session_log_sync;", []);
        let _ = conn.execute("DETACH DATABASE src;", []);

        if mtime_stamp > 0 {
            if let Ok(mut guard) = Self::cc_switch_mtime_cache().lock() {
                guard.insert(cc_switch_db_path.to_path_buf(), mtime_stamp);
            }
        }

        Ok(CcSwitchUsageImportReport {
            logs_imported: logs_count,
            pricing_imported: pricing_count,
            rollups_imported: rollups_count,
        })
    }

    /// Incrementally import from CC-Switch database only if its file (or WAL) mtime has changed
    /// since the last import in this process.
    pub fn import_from_cc_switch_if_changed(
        &self,
        cc_switch_db_path: &Path,
    ) -> Result<Option<CcSwitchUsageImportReport>, String> {
        if !cc_switch_db_path.is_file() {
            return Ok(None);
        }
        let current_mtime = Self::cc_switch_db_mtime(cc_switch_db_path);
        if current_mtime > 0 {
            if let Ok(guard) = Self::cc_switch_mtime_cache().lock() {
                if guard.get(cc_switch_db_path).copied() == Some(current_mtime) {
                    return Ok(None);
                }
            }
        }
        self.import_from_cc_switch(cc_switch_db_path).map(Some)
    }

    fn cc_switch_mtime_cache() -> &'static std::sync::Mutex<HashMap<PathBuf, u128>> {
        static CACHE: std::sync::OnceLock<std::sync::Mutex<HashMap<PathBuf, u128>>> =
            std::sync::OnceLock::new();
        CACHE.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
    }

    fn cc_switch_db_mtime(cc_switch_db_path: &Path) -> u128 {
        let file_mtime = |p: &Path| -> u128 {
            p.metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis())
                .unwrap_or(0)
        };
        let main_mtime = file_mtime(cc_switch_db_path);
        let wal_path = PathBuf::from(format!("{}-wal", cc_switch_db_path.display()));
        let wal_mtime = file_mtime(&wal_path);
        main_mtime.max(wal_mtime)
    }

    // -----------------------------------------------------------------------
    // Reporting & Aggregation Queries
    // -----------------------------------------------------------------------

    /// Get overall usage summary metrics.
    pub fn get_usage_summary(
        &self,
        start_date: Option<i64>,
        end_date: Option<i64>,
        app_type: Option<&str>,
        provider_name: Option<&str>,
        model: Option<&str>,
    ) -> Result<UsageSummary, String> {
        self.get_usage_summary_scoped(start_date, end_date, app_type, None, provider_name, model)
    }

    /// Get overall usage summary metrics, restricted to `allowed_apps` when present.
    pub fn get_usage_summary_scoped(
        &self,
        start_date: Option<i64>,
        end_date: Option<i64>,
        app_type: Option<&str>,
        allowed_apps: Option<&[String]>,
        provider_name: Option<&str>,
        model: Option<&str>,
    ) -> Result<UsageSummary, String> {
        let conn = self.connect()?;

        let (where_clause, params_vec) = build_usage_where(
            start_date,
            end_date,
            app_type,
            allowed_apps,
            provider_name,
            model,
            None,
        );

        let sql = format!(
            "SELECT
                COUNT(*),
                COALESCE(SUM(CAST(total_cost_usd AS REAL)), 0),
                COALESCE(SUM(input_tokens), 0),
                COALESCE(SUM(output_tokens), 0),
                COALESCE(SUM(cache_creation_tokens), 0),
                COALESCE(SUM(cache_read_tokens), 0),
                COALESCE(SUM(CASE WHEN status_code >= 200 AND status_code < 300 THEN 1 ELSE 0 END), 0)
             FROM proxy_request_logs
             {where_clause}"
        );

        let param_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();

        let summary = conn
            .query_row(&sql, param_refs.as_slice(), |row| {
                let total_requests: i64 = row.get(0)?;
                let total_cost: f64 = row.get(1)?;
                let total_input_tokens: i64 = row.get(2)?;
                let total_output_tokens: i64 = row.get(3)?;
                let total_cache_creation_tokens: i64 = row.get(4)?;
                let total_cache_read_tokens: i64 = row.get(5)?;
                let success_count: i64 = row.get(6)?;

                let success_rate = if total_requests > 0 {
                    (success_count as f32 / total_requests as f32) * 100.0
                } else {
                    0.0
                };

                let real_total = (total_input_tokens + total_output_tokens + total_cache_creation_tokens + total_cache_read_tokens) as u64;
                let cacheable = (total_input_tokens + total_cache_creation_tokens + total_cache_read_tokens) as f64;
                let cache_hit_rate = if cacheable > 0.0 {
                    (total_cache_read_tokens as f64) / cacheable
                } else {
                    0.0
                };

                Ok(UsageSummary {
                    total_requests: total_requests as u64,
                    total_cost: format!("{total_cost:.6}"),
                    total_input_tokens: total_input_tokens as u64,
                    total_output_tokens: total_output_tokens as u64,
                    total_cache_creation_tokens: total_cache_creation_tokens as u64,
                    total_cache_read_tokens: total_cache_read_tokens as u64,
                    success_rate,
                    real_total_tokens: real_total,
                    cache_hit_rate,
                })
            })
            .map_err(|e| format!("查询使用量汇总失败: {e}"))?;

        Ok(summary)
    }

    /// Get usage summary grouped by application type.
    pub fn get_usage_summary_by_app(
        &self,
        start_date: Option<i64>,
        end_date: Option<i64>,
        provider_name: Option<&str>,
        model: Option<&str>,
    ) -> Result<Vec<UsageSummaryByApp>, String> {
        self.get_usage_summary_by_app_scoped(start_date, end_date, None, provider_name, model)
    }

    /// Get usage summary grouped by application type, restricted to `allowed_apps` when present.
    pub fn get_usage_summary_by_app_scoped(
        &self,
        start_date: Option<i64>,
        end_date: Option<i64>,
        allowed_apps: Option<&[String]>,
        provider_name: Option<&str>,
        model: Option<&str>,
    ) -> Result<Vec<UsageSummaryByApp>, String> {
        let conn = self.connect()?;

        let (where_clause, params_vec) = build_usage_where(
            start_date,
            end_date,
            None,
            allowed_apps,
            provider_name,
            model,
            None,
        );

        let sql = format!(
            "SELECT
                app_type,
                COUNT(*),
                COALESCE(SUM(CAST(total_cost_usd AS REAL)), 0),
                COALESCE(SUM(input_tokens), 0),
                COALESCE(SUM(output_tokens), 0),
                COALESCE(SUM(cache_creation_tokens), 0),
                COALESCE(SUM(cache_read_tokens), 0),
                COALESCE(SUM(CASE WHEN status_code >= 200 AND status_code < 300 THEN 1 ELSE 0 END), 0)
             FROM proxy_request_logs
             {where_clause}
             GROUP BY app_type
             ORDER BY SUM(input_tokens + output_tokens + cache_read_tokens) DESC"
        );

        let param_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        let mut stmt = conn.prepare(&sql).map_err(|e| format!("准备查询失败: {e}"))?;

        let rows = stmt
            .query_map(param_refs.as_slice(), |row| {
                let app_type: String = row.get(0)?;
                let total_requests: i64 = row.get(1)?;
                let total_cost: f64 = row.get(2)?;
                let total_input_tokens: i64 = row.get(3)?;
                let total_output_tokens: i64 = row.get(4)?;
                let total_cache_creation_tokens: i64 = row.get(5)?;
                let total_cache_read_tokens: i64 = row.get(6)?;
                let success_count: i64 = row.get(7)?;

                let success_rate = if total_requests > 0 {
                    (success_count as f32 / total_requests as f32) * 100.0
                } else {
                    0.0
                };
                let real_total = (total_input_tokens + total_output_tokens + total_cache_creation_tokens + total_cache_read_tokens) as u64;
                let cacheable = (total_input_tokens + total_cache_creation_tokens + total_cache_read_tokens) as f64;
                let cache_hit_rate = if cacheable > 0.0 {
                    (total_cache_read_tokens as f64) / cacheable
                } else {
                    0.0
                };

                Ok(UsageSummaryByApp {
                    app_type,
                    summary: UsageSummary {
                        total_requests: total_requests as u64,
                        total_cost: format!("{total_cost:.6}"),
                        total_input_tokens: total_input_tokens as u64,
                        total_output_tokens: total_output_tokens as u64,
                        total_cache_creation_tokens: total_cache_creation_tokens as u64,
                        total_cache_read_tokens: total_cache_read_tokens as u64,
                        success_rate,
                        real_total_tokens: real_total,
                        cache_hit_rate,
                    },
                })
            })
            .map_err(|e| format!("执行查询失败: {e}"))?;

        let mut list = Vec::new();
        for r in rows {
            if let Ok(item) = r {
                list.push(item);
            }
        }
        Ok(list)
    }

    /// Get daily or hourly usage trends.
    pub fn get_daily_trends(
        &self,
        start_date: Option<i64>,
        end_date: Option<i64>,
        app_type: Option<&str>,
        provider_name: Option<&str>,
        model: Option<&str>,
    ) -> Result<Vec<DailyStats>, String> {
        self.get_daily_trends_scoped(start_date, end_date, app_type, None, provider_name, model)
    }

    /// Get daily or hourly usage trends, restricted to `allowed_apps` when present.
    pub fn get_daily_trends_scoped(
        &self,
        start_date: Option<i64>,
        end_date: Option<i64>,
        app_type: Option<&str>,
        allowed_apps: Option<&[String]>,
        provider_name: Option<&str>,
        model: Option<&str>,
    ) -> Result<Vec<DailyStats>, String> {
        let conn = self.connect()?;

        let now_ts = Local::now().timestamp();
        let end_ts = end_date.unwrap_or(now_ts);
        let start_ts = match start_date {
            Some(s) => s,
            None => {
                let (min_where, min_params) = build_usage_where(
                    None,
                    end_date,
                    app_type,
                    allowed_apps,
                    provider_name,
                    model,
                    None,
                );
                let min_sql = format!("SELECT MIN(created_at) FROM proxy_request_logs {min_where}");
                let min_refs: Vec<&dyn rusqlite::ToSql> =
                    min_params.iter().map(|p| p.as_ref()).collect();
                let min_ts: Option<i64> = conn
                    .query_row(&min_sql, min_refs.as_slice(), |row| row.get(0))
                    .ok()
                    .flatten();
                min_ts.unwrap_or_else(|| end_ts - 30 * 86400)
            }
        };
        let duration = end_ts - start_ts;

        let is_hourly = duration <= 24 * 3600;

        let (where_clause, params_vec) = build_usage_where(
            Some(start_ts),
            Some(end_ts),
            app_type,
            allowed_apps,
            provider_name,
            model,
            None,
        );

        if is_hourly {
            let bucket_seconds: i64 = 3600;
            let bucket_count = if duration <= 0 {
                1
            } else {
                ((duration + bucket_seconds - 1) / bucket_seconds).max(1)
            };

            let sql = format!(
                "SELECT
                    strftime('%Y-%m-%dT%H:00:00', created_at, 'unixepoch', 'localtime') as hour_str,
                    COUNT(*),
                    COALESCE(SUM(CAST(total_cost_usd AS REAL)), 0),
                    COALESCE(SUM(input_tokens + output_tokens + cache_creation_tokens + cache_read_tokens), 0),
                    COALESCE(SUM(input_tokens), 0),
                    COALESCE(SUM(output_tokens), 0),
                    COALESCE(SUM(cache_creation_tokens), 0),
                    COALESCE(SUM(cache_read_tokens), 0)
                 FROM proxy_request_logs
                 {where_clause}
                 GROUP BY hour_str
                 ORDER BY hour_str ASC"
            );

            let param_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
            let mut stmt = conn.prepare(&sql).map_err(|e| format!("准备趋势查询失败: {e}"))?;

            let mut map: std::collections::HashMap<String, DailyStats> = std::collections::HashMap::new();
            let rows = stmt.query_map(param_refs.as_slice(), |row| {
                let date: String = row.get(0)?;
                let request_count: i64 = row.get(1)?;
                let total_cost: f64 = row.get(2)?;
                let total_tokens: i64 = row.get(3)?;
                let total_input_tokens: i64 = row.get(4)?;
                let total_output_tokens: i64 = row.get(5)?;
                let total_cache_creation_tokens: i64 = row.get(6)?;
                let total_cache_read_tokens: i64 = row.get(7)?;

                Ok((date, DailyStats {
                    date: String::new(),
                    request_count: request_count as u64,
                    total_cost: format!("{total_cost:.6}"),
                    total_tokens: total_tokens as u64,
                    total_input_tokens: total_input_tokens as u64,
                    total_output_tokens: total_output_tokens as u64,
                    total_cache_creation_tokens: total_cache_creation_tokens as u64,
                    total_cache_read_tokens: total_cache_read_tokens as u64,
                }))
            }).map_err(|e| format!("执行趋势查询失败: {e}"))?;

            for r in rows {
                if let Ok((d, stat)) = r {
                    map.insert(d, stat);
                }
            }

            let mut list = Vec::with_capacity(bucket_count as usize);
            for i in 0..bucket_count {
                let bucket_ts = start_ts + i * bucket_seconds;
                let date = if let Some(dt) = Local.timestamp_opt(bucket_ts, 0).single() {
                    dt.format("%Y-%m-%dT%H:00:00").to_string()
                } else {
                    format!("{bucket_ts}")
                };
                if let Some(mut stat) = map.remove(&date) {
                    stat.date = date;
                    list.push(stat);
                } else {
                    list.push(DailyStats {
                        date,
                        request_count: 0,
                        total_cost: "0.000000".to_string(),
                        total_tokens: 0,
                        total_input_tokens: 0,
                        total_output_tokens: 0,
                        total_cache_creation_tokens: 0,
                        total_cache_read_tokens: 0,
                    });
                }
            }
            Ok(list)
        } else {
            let start_day = if let Some(dt) = Local.timestamp_opt(start_ts, 0).single() {
                dt.date_naive()
            } else {
                Local::now().date_naive()
            };
            let end_day = if let Some(dt) = Local.timestamp_opt(end_ts, 0).single() {
                dt.date_naive()
            } else {
                Local::now().date_naive()
            };
            let day_count = (end_day.signed_duration_since(start_day).num_days() + 1).max(1) as usize;

            let sql = format!(
                "SELECT
                    strftime('%Y-%m-%d', created_at, 'unixepoch', 'localtime') as day_str,
                    COUNT(*),
                    COALESCE(SUM(CAST(total_cost_usd AS REAL)), 0),
                    COALESCE(SUM(input_tokens + output_tokens + cache_creation_tokens + cache_read_tokens), 0),
                    COALESCE(SUM(input_tokens), 0),
                    COALESCE(SUM(output_tokens), 0),
                    COALESCE(SUM(cache_creation_tokens), 0),
                    COALESCE(SUM(cache_read_tokens), 0)
                 FROM proxy_request_logs
                 {where_clause}
                 GROUP BY day_str
                 ORDER BY day_str ASC"
            );

            let param_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
            let mut stmt = conn.prepare(&sql).map_err(|e| format!("准备趋势查询失败: {e}"))?;

            let mut map: std::collections::HashMap<String, DailyStats> = std::collections::HashMap::new();
            let rows = stmt.query_map(param_refs.as_slice(), |row| {
                let day_str: String = row.get(0)?;
                let request_count: i64 = row.get(1)?;
                let total_cost: f64 = row.get(2)?;
                let total_tokens: i64 = row.get(3)?;
                let total_input_tokens: i64 = row.get(4)?;
                let total_output_tokens: i64 = row.get(5)?;
                let total_cache_creation_tokens: i64 = row.get(6)?;
                let total_cache_read_tokens: i64 = row.get(7)?;

                Ok((day_str, DailyStats {
                    date: String::new(),
                    request_count: request_count as u64,
                    total_cost: format!("{total_cost:.6}"),
                    total_tokens: total_tokens as u64,
                    total_input_tokens: total_input_tokens as u64,
                    total_output_tokens: total_output_tokens as u64,
                    total_cache_creation_tokens: total_cache_creation_tokens as u64,
                    total_cache_read_tokens: total_cache_read_tokens as u64,
                }))
            }).map_err(|e| format!("执行趋势查询失败: {e}"))?;

            for r in rows {
                if let Ok((d, stat)) = r {
                    map.insert(d, stat);
                }
            }

            let mut list = Vec::with_capacity(day_count);
            let mut cur = start_day;
            while cur <= end_day {
                let date = cur.format("%Y-%m-%d").to_string();
                if let Some(mut stat) = map.remove(&date) {
                    stat.date = date;
                    list.push(stat);
                } else {
                    list.push(DailyStats {
                        date,
                        request_count: 0,
                        total_cost: "0.000000".to_string(),
                        total_tokens: 0,
                        total_input_tokens: 0,
                        total_output_tokens: 0,
                        total_cache_creation_tokens: 0,
                        total_cache_read_tokens: 0,
                    });
                }
                if cur == end_day { break; }
                cur = cur.succ_opt().unwrap_or(cur);
            }
            Ok(list)
        }
    }

    /// Get usage statistics grouped by provider.
    pub fn get_provider_stats(
        &self,
        start_date: Option<i64>,
        end_date: Option<i64>,
        app_type: Option<&str>,
        provider_name: Option<&str>,
        model: Option<&str>,
    ) -> Result<Vec<ProviderStats>, String> {
        self.get_provider_stats_scoped(start_date, end_date, app_type, None, provider_name, model)
    }

    /// Get usage statistics grouped by provider, restricted to `allowed_apps` when present.
    pub fn get_provider_stats_scoped(
        &self,
        start_date: Option<i64>,
        end_date: Option<i64>,
        app_type: Option<&str>,
        allowed_apps: Option<&[String]>,
        provider_name: Option<&str>,
        model: Option<&str>,
    ) -> Result<Vec<ProviderStats>, String> {
        let conn = self.connect()?;

        let (where_clause, params_vec) = build_usage_where(
            start_date,
            end_date,
            app_type,
            allowed_apps,
            provider_name,
            model,
            None,
        );

        let sql = format!(
            "SELECT
                provider_id,
                COUNT(*),
                COALESCE(SUM(input_tokens + output_tokens + cache_read_tokens), 0),
                COALESCE(SUM(CAST(total_cost_usd AS REAL)), 0),
                COALESCE(SUM(CASE WHEN status_code >= 200 AND status_code < 300 THEN 1 ELSE 0 END), 0),
                COALESCE(AVG(latency_ms), 0)
             FROM proxy_request_logs
             {where_clause}
             GROUP BY provider_id
             ORDER BY COALESCE(SUM(CAST(total_cost_usd AS REAL)), 0) DESC"
        );

        let param_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        let mut stmt = conn.prepare(&sql).map_err(|e| format!("准备 provider 统计失败: {e}"))?;

        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            let provider_id: String = row.get(0)?;
            let request_count: i64 = row.get(1)?;
            let total_tokens: i64 = row.get(2)?;
            let total_cost: f64 = row.get(3)?;
            let success_count: i64 = row.get(4)?;
            let avg_latency: f64 = row.get(5)?;

            let success_rate = if request_count > 0 {
                (success_count as f32 / request_count as f32) * 100.0
            } else {
                0.0
            };

            let readable_name = format_provider_display_name(&provider_id);

            Ok(ProviderStats {
                provider_id,
                provider_name: readable_name,
                request_count: request_count as u64,
                total_tokens: total_tokens as u64,
                total_cost: format!("{total_cost:.6}"),
                success_rate,
                avg_latency_ms: avg_latency as u64,
            })
        }).map_err(|e| format!("执行 provider 统计失败: {e}"))?;

        let mut list = Vec::new();
        for r in rows {
            if let Ok(item) = r {
                list.push(item);
            }
        }
        Ok(list)
    }

    /// Get usage statistics grouped by model.
    pub fn get_model_stats(
        &self,
        start_date: Option<i64>,
        end_date: Option<i64>,
        app_type: Option<&str>,
        provider_name: Option<&str>,
        model: Option<&str>,
    ) -> Result<Vec<ModelStats>, String> {
        self.get_model_stats_scoped(start_date, end_date, app_type, None, provider_name, model)
    }

    /// Get usage statistics grouped by model, restricted to `allowed_apps` when present.
    pub fn get_model_stats_scoped(
        &self,
        start_date: Option<i64>,
        end_date: Option<i64>,
        app_type: Option<&str>,
        allowed_apps: Option<&[String]>,
        provider_name: Option<&str>,
        model: Option<&str>,
    ) -> Result<Vec<ModelStats>, String> {
        let conn = self.connect()?;

        let (where_clause, params_vec) = build_usage_where(
            start_date,
            end_date,
            app_type,
            allowed_apps,
            provider_name,
            model,
            None,
        );

        let sql = format!(
            "SELECT
                COALESCE(NULLIF(pricing_model, ''), model) as effective_model,
                COUNT(*),
                COALESCE(SUM(input_tokens + output_tokens + cache_read_tokens), 0),
                COALESCE(SUM(CAST(total_cost_usd AS REAL)), 0)
             FROM proxy_request_logs
             {where_clause}
             GROUP BY effective_model
             ORDER BY COALESCE(SUM(CAST(total_cost_usd AS REAL)), 0) DESC"
        );

        let param_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        let mut stmt = conn.prepare(&sql).map_err(|e| format!("准备 model 统计失败: {e}"))?;

        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            let model_name: String = row.get(0)?;
            let request_count: i64 = row.get(1)?;
            let total_tokens: i64 = row.get(2)?;
            let total_cost: f64 = row.get(3)?;

            let avg_cost = if request_count > 0 {
                total_cost / request_count as f64
            } else {
                0.0
            };

            Ok(ModelStats {
                model: model_name,
                request_count: request_count as u64,
                total_tokens: total_tokens as u64,
                total_cost: format!("{total_cost:.6}"),
                avg_cost_per_request: format!("{avg_cost:.6}"),
            })
        }).map_err(|e| format!("执行 model 统计失败: {e}"))?;

        let mut list = Vec::new();
        for r in rows {
            if let Ok(item) = r {
                list.push(item);
            }
        }
        Ok(list)
    }

    /// Get paginated request logs.
    pub fn get_request_logs(
        &self,
        filters: &LogFilters,
        page: u32,
        page_size: u32,
    ) -> Result<PaginatedLogs, String> {
        let conn = self.connect()?;

        let (where_clause, params_vec) = build_usage_where(
            filters.start_date,
            filters.end_date,
            filters.app_type.as_deref(),
            filters.allowed_apps.as_deref(),
            filters.provider_name.as_deref(),
            filters.model.as_deref(),
            filters.status_code,
        );

        // Total count
        let count_sql = format!("SELECT COUNT(*) FROM proxy_request_logs {where_clause}");
        let count_params: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        let total: u32 = conn
            .query_row(&count_sql, count_params.as_slice(), |r| r.get(0))
            .unwrap_or(0);

        let offset = page * page_size;
        let query_sql = format!(
            "SELECT
                request_id, provider_id, app_type, model, request_model,
                cost_multiplier, input_tokens, output_tokens, cache_read_tokens,
                cache_creation_tokens, total_cost_usd, is_streaming, latency_ms,
                status_code, error_message, created_at, data_source, pricing_model
             FROM proxy_request_logs
             {where_clause}
             ORDER BY created_at DESC
             LIMIT ? OFFSET ?"
        );

        let mut query_params = params_vec;
        query_params.push(Box::new(page_size as i64));
        query_params.push(Box::new(offset as i64));
        let query_refs: Vec<&dyn rusqlite::ToSql> = query_params.iter().map(|p| p.as_ref()).collect();

        let mut stmt = conn.prepare(&query_sql).map_err(|e| format!("准备日志列表失败: {e}"))?;
        let rows = stmt.query_map(query_refs.as_slice(), |row| {
            let provider_id: String = row.get(1)?;
            let readable_name = format_provider_display_name(&provider_id);

            Ok(RequestLogDetail {
                request_id: row.get(0)?,
                provider_id,
                provider_name: Some(readable_name),
                app_type: row.get(2)?,
                model: row.get(3)?,
                request_model: row.get(4)?,
                cost_multiplier: row.get(5)?,
                input_tokens: row.get::<_, i64>(6)? as u32,
                output_tokens: row.get::<_, i64>(7)? as u32,
                cache_read_tokens: row.get::<_, i64>(8)? as u32,
                cache_creation_tokens: row.get::<_, i64>(9)? as u32,
                total_cost_usd: row.get(10)?,
                is_streaming: row.get::<_, i64>(11)? != 0,
                latency_ms: row.get::<_, i64>(12)? as u64,
                status_code: row.get::<_, i64>(13)? as u16,
                error_message: row.get(14)?,
                created_at: row.get(15)?,
                data_source: row.get(16)?,
                pricing_model: row.get(17)?,
            })
        }).map_err(|e| format!("查询日志失败: {e}"))?;

        let mut data = Vec::new();
        for r in rows {
            if let Ok(item) = r {
                data.push(item);
            }
        }

        Ok(PaginatedLogs {
            data,
            total,
            page,
            page_size,
        })
    }

    /// List all configured model pricing entries.
    pub fn get_model_pricing(&self) -> Result<Vec<ModelPricingInfo>, String> {
        let conn = self.connect()?;
        let mut stmt = conn
            .prepare(
                "SELECT model_id, display_name, input_cost_per_million, output_cost_per_million,
                        cache_read_cost_per_million, cache_creation_cost_per_million
                 FROM model_pricing
                 ORDER BY display_name ASC",
            )
            .map_err(|e| format!("查询定价表失败: {e}"))?;

        let rows = stmt.query_map([], |row| {
            Ok(ModelPricingInfo {
                model_id: row.get(0)?,
                display_name: row.get(1)?,
                input_cost_per_million: row.get(2)?,
                output_cost_per_million: row.get(3)?,
                cache_read_cost_per_million: row.get(4)?,
                cache_creation_cost_per_million: row.get(5)?,
            })
        }).map_err(|e| format!("读取定价行失败: {e}"))?;

        let mut list = Vec::new();
        for r in rows {
            if let Ok(item) = r {
                list.push(item);
            }
        }
        Ok(list)
    }

    /// Insert or update a model pricing entry.
    pub fn update_model_pricing(&self, info: &ModelPricingInfo) -> Result<(), String> {
        let conn = self.connect()?;
        conn.execute(
            "INSERT INTO model_pricing (model_id, display_name, input_cost_per_million, output_cost_per_million, cache_read_cost_per_million, cache_creation_cost_per_million)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(model_id) DO UPDATE SET
                display_name = excluded.display_name,
                input_cost_per_million = excluded.input_cost_per_million,
                output_cost_per_million = excluded.output_cost_per_million,
                cache_read_cost_per_million = excluded.cache_read_cost_per_million,
                cache_creation_cost_per_million = excluded.cache_creation_cost_per_million",
            params![
                info.model_id,
                info.display_name,
                info.input_cost_per_million,
                info.output_cost_per_million,
                info.cache_read_cost_per_million,
                info.cache_creation_cost_per_million,
            ],
        ).map_err(|e| format!("更新模型定价失败: {e}"))?;
        Ok(())
    }

    /// Delete a model pricing entry.
    pub fn delete_model_pricing(&self, model_id: &str) -> Result<(), String> {
        let conn = self.connect()?;
        conn.execute("DELETE FROM model_pricing WHERE model_id = ?1", params![model_id])
            .map_err(|e| format!("删除模型定价失败: {e}"))?;
        Ok(())
    }

    /// Get all application pricing defaults (multiplier & source).
    pub fn get_app_pricing_configs(&self) -> Result<Vec<AppPricingConfig>, String> {
        let conn = self.connect()?;
        let mut stmt = conn
            .prepare("SELECT app_type, cost_multiplier, pricing_model_source FROM app_pricing_config ORDER BY app_type ASC")
            .map_err(|e| format!("查询 app_pricing_config 失败: {e}"))?;

        let rows = stmt.query_map([], |row| {
            Ok(AppPricingConfig {
                app_type: row.get(0)?,
                cost_multiplier: row.get(1)?,
                pricing_model_source: row.get(2)?,
            })
        }).map_err(|e| format!("读取 app_pricing_config 行失败: {e}"))?;

        let mut list = Vec::new();
        for r in rows.flatten() {
            list.push(r);
        }
        // Ensure core apps are present
        let existing: std::collections::HashSet<String> = list.iter().map(|c| c.app_type.clone()).collect();
        for expected in ["claude", "codex", "agy", "grok"] {
            if !existing.contains(expected) {
                list.push(AppPricingConfig {
                    app_type: expected.to_string(),
                    cost_multiplier: "1.0".to_string(),
                    pricing_model_source: "response".to_string(),
                });
            }
        }
        Ok(list)
    }

    /// Insert or update an application pricing config entry.
    pub fn set_app_pricing_config(&self, app_type: &str, multiplier: &str, source: &str) -> Result<(), String> {
        let conn = self.connect()?;
        conn.execute(
            "INSERT INTO app_pricing_config (app_type, cost_multiplier, pricing_model_source)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(app_type) DO UPDATE SET
                cost_multiplier = excluded.cost_multiplier,
                pricing_model_source = excluded.pricing_model_source",
            params![app_type, multiplier, source],
        ).map_err(|e| format!("保存 app_pricing_config 失败: {e}"))?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Session Ingestion
    // -----------------------------------------------------------------------

    /// Scan local session files for all supported tools and record new usage.
    pub fn sync_session_usage(&self, paths: &Paths) -> Result<SessionSyncResult, String> {
        let conn = self.connect()?;
        crate::session_usage::sync_all(&conn, paths)
    }
}

/// Map a sidebar `visible_tools` key to its canonical `proxy_request_logs.app_type`.
pub fn sidebar_key_to_usage_app_type(key: &str) -> Option<&'static str> {
    match key.trim().to_ascii_lowercase().as_str() {
        "claude_code" | "claudecode" | "claude" => Some("claude"),
        "claude_desktop" | "claudedesktop" => Some("claude_desktop"),
        "codex" => Some("codex"),
        "antigravity" | "agy" => Some("agy"),
        "gemini_cli" | "gemini" | "geminicli" => Some("gemini"),
        "grok" | "grokbuild" => Some("grok"),
        "kimi" => Some("kimi"),
        "opencode" | "open_code" => Some("opencode"),
        "openclaw" | "open_claw" => Some("openclaw"),
        "pi" => Some("pi"),
        "oh_my_pi" | "omp" | "ohmypi" => Some("oh_my_pi"),
        "hermes" => Some("hermes"),
        "dsh" | "deepseek_harness" => Some("dsh"),
        _ => None,
    }
}

/// Resolve the allowed `app_type` list in Usage stats from `AppSettings.visible_tools`.
/// Mirrors sidebar semantics: when `visible_tools` is empty, all default sidebar tools apply.
pub fn usage_app_types_for_visible_tools(visible_tools: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let push_unique = |out: &mut Vec<String>, app: &str| {
        if !out.iter().any(|existing| existing == app) {
            out.push(app.to_string());
        }
    };
    if visible_tools.is_empty() {
        for tool in crate::tools::ToolId::ALL {
            if let Some(app) = sidebar_key_to_usage_app_type(tool.key()) {
                push_unique(&mut out, app);
            }
        }
        push_unique(&mut out, "agy");
        return out;
    }
    for key in visible_tools {
        if let Some(app) = sidebar_key_to_usage_app_type(key) {
            push_unique(&mut out, app);
        }
    }
    out
}

fn append_app_filter(
    conditions: &mut Vec<String>,
    params_vec: &mut Vec<Box<dyn rusqlite::ToSql>>,
    app_type: Option<&str>,
    allowed_apps: Option<&[String]>,
) {
    let selected = app_type.filter(|at| *at != "all");
    match (selected, allowed_apps) {
        (Some(at), Some(allowed)) => {
            if allowed.iter().any(|a| a == at) {
                conditions.push("app_type = ?".to_string());
                params_vec.push(Box::new(at.to_string()));
            } else {
                conditions.push("1 = 0".to_string());
            }
        }
        (Some(at), None) => {
            conditions.push("app_type = ?".to_string());
            params_vec.push(Box::new(at.to_string()));
        }
        (None, Some(allowed)) => {
            if allowed.is_empty() {
                conditions.push("1 = 0".to_string());
            } else {
                let placeholders = vec!["?"; allowed.len()].join(", ");
                conditions.push(format!("app_type IN ({placeholders})"));
                for app in allowed {
                    params_vec.push(Box::new(app.clone()));
                }
            }
        }
        (None, None) => {}
    }
}

fn build_usage_where(
    start_date: Option<i64>,
    end_date: Option<i64>,
    app_type: Option<&str>,
    allowed_apps: Option<&[String]>,
    provider_name: Option<&str>,
    model: Option<&str>,
    status_code: Option<u16>,
) -> (String, Vec<Box<dyn rusqlite::ToSql>>) {
    let mut conditions = Vec::new();
    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(start) = start_date {
        conditions.push("created_at >= ?".to_string());
        params_vec.push(Box::new(start));
    }
    if let Some(end) = end_date {
        conditions.push("created_at <= ?".to_string());
        params_vec.push(Box::new(end));
    }
    append_app_filter(&mut conditions, &mut params_vec, app_type, allowed_apps);
    if let Some(p) = provider_name {
        conditions.push("provider_id = ?".to_string());
        params_vec.push(Box::new(p.to_string()));
    }
    if let Some(m) = model {
        conditions.push("model = ?".to_string());
        params_vec.push(Box::new(m.to_string()));
    }
    if let Some(status) = status_code {
        conditions.push("status_code = ?".to_string());
        params_vec.push(Box::new(status as i64));
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };
    (where_clause, params_vec)
}

fn format_provider_display_name(provider_id: &str) -> String {
    match provider_id {
        "_session" => "Claude (Session)".to_string(),
        "_claude_desktop_session" => "Claude Desktop (Session)".to_string(),
        "_codex_session" => "Codex (Session)".to_string(),
        "_agy_session" | "_antigravity_session" => "Antigravity (Session)".to_string(),
        "_gemini_session" => "Gemini (Session)".to_string(),
        "_grok_session" => "Grok (Session)".to_string(),
        "_opencode_session" => "OpenCode (Session)".to_string(),
        "_pi_session" => "Pi (Session)".to_string(),
        "_omp_session" => "OhMyPi (Session)".to_string(),
        "_kimi_session" => "Kimi (Session)".to_string(),
        "_openclaw_session" => "OpenClaw (Session)".to_string(),
        "_hermes_session" => "Hermes (Session)".to_string(),
        "_dsh_session" => "Dsh (Session)".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_usage_db_lifecycle_and_analytics() {
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().join("home"), tmp.path().join("appdata"));

        let db = UsageDb::open(&paths).expect("open usage db");

        // Verify default model pricing was seeded
        let pricing = db.get_model_pricing().expect("get pricing");
        assert!(!pricing.is_empty());
        assert!(pricing.iter().any(|p| p.model_id.contains("claude-3-5-sonnet")));

        // Verify initial empty summary
        let summary = db.get_usage_summary(None, None, None, None, None).expect("summary");
        assert_eq!(summary.total_requests, 0);
        assert_eq!(summary.real_total_tokens, 0);

        // Insert test request log
        {
            let conn = db.connect().expect("connect db");
            conn.execute(
                "INSERT INTO proxy_request_logs (
                    request_id, provider_id, app_type, model, input_tokens, output_tokens,
                    cache_read_tokens, cache_creation_tokens, total_cost_usd, latency_ms,
                    status_code, created_at, data_source
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    "req-1001",
                    "deepseek-provider",
                    "codex",
                    "deepseek-chat",
                    1000,
                    200,
                    500,
                    100,
                    "0.000450",
                    120,
                    200,
                    chrono::Utc::now().timestamp(),
                    "proxy"
                ],
            ).expect("insert log");
        }

        // Test summary with log
        let sum2 = db.get_usage_summary(None, None, None, None, None).expect("summary with log");
        assert_eq!(sum2.total_requests, 1);
        assert_eq!(sum2.total_input_tokens, 1000);
        assert_eq!(sum2.total_output_tokens, 200);
        assert_eq!(sum2.total_cache_read_tokens, 500);
        assert_eq!(sum2.total_cache_creation_tokens, 100);
        // Real tokens = input + output + cache_create + cache_read = 1000 + 200 + 100 + 500 = 1800
        assert_eq!(sum2.real_total_tokens, 1800);

        // App breakdown
        let apps = db.get_usage_summary_by_app(None, None, None, None).expect("apps summary");
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].app_type, "codex");

        // Provider stats
        let provs = db.get_provider_stats(None, None, None, None, None).expect("provider stats");
        assert_eq!(provs.len(), 1);
        assert_eq!(provs[0].provider_name, "deepseek-provider");

        // Model stats
        let models = db.get_model_stats(None, None, None, None, None).expect("model stats");
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].model, "deepseek-chat");

        // Request logs pagination
        let filters = LogFilters::default();
        let logs = db.get_request_logs(&filters, 0, 10).expect("request logs");
        assert_eq!(logs.total, 1);
        assert_eq!(logs.data.len(), 1);
        assert_eq!(logs.data[0].request_id, "req-1001");
        assert_eq!(logs.data[0].app_type, "codex");

        // Daily trends
        let trends = db.get_daily_trends(None, None, None, None, None).expect("trends");
        assert_eq!(trends.len(), 1);
        assert_eq!(trends[0].request_count, 1);
        assert_eq!(trends[0].total_tokens, 1800);

        // Update pricing
        let update_pricing = ModelPricingInfo {
            model_id: "custom-model-*".to_string(),
            display_name: "Custom Model".to_string(),
            input_cost_per_million: "1.5".to_string(),
            output_cost_per_million: "3.0".to_string(),
            cache_read_cost_per_million: "0.2".to_string(),
            cache_creation_cost_per_million: "0.8".to_string(),
        };
        db.update_model_pricing(&update_pricing).expect("save pricing");
        let pricing2 = db.get_model_pricing().expect("get updated pricing");
        assert!(pricing2.iter().any(|p| p.model_id == "custom-model-*"));
    }

    #[test]
    fn test_session_sync_recursive_codex_and_incremental_cursor() {
        let temp_dir = tempfile::tempdir().unwrap();
        let home = temp_dir.path().join("home");
        let app_data = temp_dir.path().join("appdata");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&app_data).unwrap();
        let paths = Paths::new(home.clone(), app_data);

        let db = UsageDb::open(&paths).unwrap();

        // 1. Setup nested codex session
        let codex_dir = home.join(".codex").join("sessions").join("2026").join("02").join("02");
        std::fs::create_dir_all(&codex_dir).unwrap();
        let codex_file = codex_dir.join("rollout-test.jsonl");
        let codex_content = r#"{"timestamp":"2026-02-02T10:00:00Z","type":"session_meta","payload":{"id":"session-123"}}
{"timestamp":"2026-02-02T10:00:01Z","type":"turn_context","payload":{"model":"gpt-5.2-codex"}}
{"timestamp":"2026-02-02T10:00:02Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":100,"output_tokens":50,"cached_input_tokens":20}}}}
"#;
        std::fs::write(&codex_file, codex_content).unwrap();

        // 2. Setup Antigravity session transcript
        let agy_logs = home
            .join(".gemini")
            .join("antigravity-cli")
            .join("brain")
            .join("conv-123")
            .join(".system_generated")
            .join("logs");
        std::fs::create_dir_all(&agy_logs).unwrap();
        let agy_file = agy_logs.join("transcript.jsonl");
        let agy_content = r#"{"step_index":1,"type":"USER_INPUT","content":"The user changed setting `Model Selection` from Gemini 4 Argon (Medium) to Gemini 3.8 Flash (High). No need to comment","created_at":"2026-01-01T09:59:59Z"}
{"step_index":2,"type":"PLANNER_RESPONSE","input_tokens":200,"output_tokens":90,"cache_read_tokens":50,"created_at":"2026-01-01T10:00:00Z"}
"#;
        std::fs::write(&agy_file, agy_content).unwrap();

        // First sync: should discover nested codex and antigravity
        let rep1 = db.sync_session_usage(&paths).unwrap();
        assert_eq!(rep1.files_scanned, 2, "Should scan both files");
        assert_eq!(rep1.imported, 2, "Should import both records");

        // Second sync: both files are unchanged, should skip parsing via session_log_sync cursor
        let rep2 = db.sync_session_usage(&paths).unwrap();
        assert_eq!(rep2.files_scanned, 2, "Should check both files");
        assert_eq!(rep2.imported, 0, "Unchanged files should not be re-imported");

        // Downgrade cursor in DB to 10-digit Unix seconds (simulating cc-switch legacy cursor)
        {
            let conn = db.connect().unwrap();
            conn.execute(
                "UPDATE session_log_sync SET last_modified = last_modified / 1000 WHERE last_modified > 1000000000000",
                [],
            )
            .unwrap();
        }

        // Third sync: should still recognize 10-digit second cursor as matching file mtime
        let rep3 = db.sync_session_usage(&paths).unwrap();
        assert_eq!(rep3.files_scanned, 2);
        assert_eq!(rep3.imported, 0, "10-digit second cursor from cc-switch should match without re-parsing");
    }

    #[test]
    fn test_scoped_usage_queries_and_visible_tools_mapping() {
        let mapped = usage_app_types_for_visible_tools(&[
            "claude_code".into(),
            "codex".into(),
            "pi".into(),
            "antigravity".into(),
            "agents".into(),
        ]);
        assert_eq!(mapped, vec!["claude", "codex", "pi", "agy"]);

        let defaults = usage_app_types_for_visible_tools(&[]);
        assert!(defaults.contains(&"claude".to_string()));
        assert!(defaults.contains(&"agy".to_string()));
        assert!(!defaults.contains(&"gemini".to_string()));

        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().join("home"), tmp.path().join("appdata"));
        let db = UsageDb::open(&paths).expect("open usage db");
        let now = chrono::Utc::now().timestamp();
        {
            let conn = db.connect().unwrap();
            for (id, app, prov, model, ts) in [
                ("r-claude", "claude", "_session", "claude-sonnet", now),
                ("r-pi", "pi", "_pi_session", "claude-sonnet", now),
                ("r-gemini", "gemini", "_gemini_session", "gemini-2.5-pro", now - 5 * 86400),
            ] {
                conn.execute(
                    "INSERT INTO proxy_request_logs (
                        request_id, provider_id, app_type, model, input_tokens, output_tokens,
                        cache_read_tokens, cache_creation_tokens, total_cost_usd, latency_ms,
                        status_code, created_at, data_source
                    ) VALUES (?1, ?2, ?3, ?4, 100, 50, 0, 0, '0.100000', 100, 200, ?5, 'session')",
                    params![id, prov, app, model, ts],
                )
                .unwrap();
            }
        }

        let allowed = vec!["claude".to_string(), "pi".to_string()];
        // "All Apps" scoped to visible tools must exclude hidden gemini
        let sum = db
            .get_usage_summary_scoped(None, None, None, Some(&allowed), None, None)
            .unwrap();
        assert_eq!(sum.total_requests, 2);
        assert_eq!(sum.real_total_tokens, 300);

        // Explicitly querying a hidden app inside scope must return 0
        let hidden = db
            .get_usage_summary_scoped(None, None, Some("gemini"), Some(&allowed), None, None)
            .unwrap();
        assert_eq!(hidden.total_requests, 0);

        let provs = db
            .get_provider_stats_scoped(None, None, None, Some(&allowed), None, None)
            .unwrap();
        assert_eq!(provs.len(), 2);
        assert!(provs.iter().all(|p| p.provider_id != "_gemini_session"));

        let models = db
            .get_model_stats_scoped(None, None, None, Some(&allowed), None, None)
            .unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].model, "claude-sonnet");

        // All-time daily trends MIN(created_at) must not stretch back to hidden gemini timestamp
        let trends = db
            .get_daily_trends_scoped(None, None, None, Some(&allowed), None, None)
            .unwrap();
        assert_eq!(trends.len(), 1);
        assert_eq!(trends[0].request_count, 2);

        let logs = db
            .get_request_logs(
                &LogFilters {
                    allowed_apps: Some(allowed),
                    ..Default::default()
                },
                0,
                10,
            )
            .unwrap();
        assert_eq!(logs.total, 2);
        assert!(logs.data.iter().all(|r| r.app_type != "gemini"));
    }
}

