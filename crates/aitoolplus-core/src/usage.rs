//! Usage statistics, analytics, and session ingestion engine.
//!
//! Replicates CC-Switch's usage tracking architecture:
//! - SQLite storage (`proxy_request_logs`, `model_pricing`, `usage_daily_rollups`, etc.)
//! - Multi-tool token usage aggregation (Claude, Codex, Gemini, Grok, OpenCode, Pi)
//! - Provider breakdown, model breakdown, hourly/daily trends, and paginated logs.

use std::collections::HashMap;
use std::fs;
use std::io::BufRead;
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

        let cc_switch_db = paths.home.join(".cc-switch").join("cc-switch.db");
        if cc_switch_db.is_file() {
            let _ = instance.import_from_cc_switch(&cc_switch_db);
        }

        Ok(instance)
    }

    /// Connect to the SQLite file.
    pub fn connect(&self) -> Result<Connection, String> {
        Connection::open(&self.db_path).map_err(|e| format!("打开使用统计数据库失败: {e}"))
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
            ("gemini", "1.0", "response"),
            ("grok", "1.0", "response"),
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

        let _ = conn.execute("INSERT OR REPLACE INTO main.session_log_sync SELECT * FROM src.session_log_sync;", []);
        let _ = conn.execute("DETACH DATABASE src;", []);

        Ok(CcSwitchUsageImportReport {
            logs_imported: logs_count,
            pricing_imported: pricing_count,
            rollups_imported: rollups_count,
        })
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
        let conn = self.connect()?;

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
        if let Some(at) = app_type {
            if at != "all" {
                conditions.push("app_type = ?".to_string());
                params_vec.push(Box::new(at.to_string()));
            }
        }
        if let Some(p) = provider_name {
            conditions.push("provider_id = ?".to_string());
            params_vec.push(Box::new(p.to_string()));
        }
        if let Some(m) = model {
            conditions.push("model = ?".to_string());
            params_vec.push(Box::new(m.to_string()));
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

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
        let conn = self.connect()?;

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
        if let Some(p) = provider_name {
            conditions.push("provider_id = ?".to_string());
            params_vec.push(Box::new(p.to_string()));
        }
        if let Some(m) = model {
            conditions.push("model = ?".to_string());
            params_vec.push(Box::new(m.to_string()));
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

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
        let conn = self.connect()?;

        let now_ts = Local::now().timestamp();
        let end_ts = end_date.unwrap_or(now_ts);
        let start_ts = match start_date {
            Some(s) => s,
            None => {
                let min_ts: Option<i64> = conn
                    .query_row("SELECT MIN(created_at) FROM proxy_request_logs", [], |row| row.get(0))
                    .ok()
                    .flatten();
                min_ts.unwrap_or_else(|| end_ts - 30 * 86400)
            }
        };
        let duration = end_ts - start_ts;

        let is_hourly = duration <= 24 * 3600;

        let mut conditions = vec!["created_at >= ?".to_string(), "created_at <= ?".to_string()];
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(start_ts), Box::new(end_ts)];

        if let Some(at) = app_type {
            if at != "all" {
                conditions.push("app_type = ?".to_string());
                params_vec.push(Box::new(at.to_string()));
            }
        }
        if let Some(p) = provider_name {
            conditions.push("provider_id = ?".to_string());
            params_vec.push(Box::new(p.to_string()));
        }
        if let Some(m) = model {
            conditions.push("model = ?".to_string());
            params_vec.push(Box::new(m.to_string()));
        }

        let where_clause = format!("WHERE {}", conditions.join(" AND "));

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
        let conn = self.connect()?;

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
        if let Some(at) = app_type {
            if at != "all" {
                conditions.push("app_type = ?".to_string());
                params_vec.push(Box::new(at.to_string()));
            }
        }
        if let Some(p) = provider_name {
            conditions.push("provider_id = ?".to_string());
            params_vec.push(Box::new(p.to_string()));
        }
        if let Some(m) = model {
            conditions.push("model = ?".to_string());
            params_vec.push(Box::new(m.to_string()));
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

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

            let readable_name = match provider_id.as_str() {
                "_session" => "Claude (Session)".to_string(),
                "_codex_session" => "Codex (Session)".to_string(),
                "_gemini_session" => "Gemini (Session)".to_string(),
                "_grok_session" => "Grok Build (Session)".to_string(),
                "_opencode_session" => "OpenCode (Session)".to_string(),
                "_pi_session" => "Pi (Session)".to_string(),
                other => other.to_string(),
            };

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
        let conn = self.connect()?;

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
        if let Some(at) = app_type {
            if at != "all" {
                conditions.push("app_type = ?".to_string());
                params_vec.push(Box::new(at.to_string()));
            }
        }
        if let Some(p) = provider_name {
            conditions.push("provider_id = ?".to_string());
            params_vec.push(Box::new(p.to_string()));
        }
        if let Some(m) = model {
            conditions.push("model = ?".to_string());
            params_vec.push(Box::new(m.to_string()));
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

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

        let mut conditions = Vec::new();
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(ref at) = filters.app_type {
            if at != "all" {
                conditions.push("app_type = ?".to_string());
                params_vec.push(Box::new(at.clone()));
            }
        }
        if let Some(ref p) = filters.provider_name {
            conditions.push("provider_id = ?".to_string());
            params_vec.push(Box::new(p.clone()));
        }
        if let Some(ref m) = filters.model {
            conditions.push("model = ?".to_string());
            params_vec.push(Box::new(m.clone()));
        }
        if let Some(status) = filters.status_code {
            conditions.push("status_code = ?".to_string());
            params_vec.push(Box::new(status as i64));
        }
        if let Some(start) = filters.start_date {
            conditions.push("created_at >= ?".to_string());
            params_vec.push(Box::new(start));
        }
        if let Some(end) = filters.end_date {
            conditions.push("created_at <= ?".to_string());
            params_vec.push(Box::new(end));
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

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
            let readable_name = match provider_id.as_str() {
                "_session" => "Claude (Session)".to_string(),
                "_codex_session" => "Codex (Session)".to_string(),
                "_gemini_session" => "Gemini (Session)".to_string(),
                "_grok_session" => "Grok Build (Session)".to_string(),
                "_opencode_session" => "OpenCode (Session)".to_string(),
                "_pi_session" => "Pi (Session)".to_string(),
                other => other.to_string(),
            };

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
        for expected in ["claude", "codex", "gemini", "grok"] {
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
        let mut report = SessionSyncResult::default();
        let conn = self.connect()?;

        // Fetch pricing lookup table
        let mut pricing_map: HashMap<String, (f64, f64, f64, f64)> = HashMap::new();
        if let Ok(mut stmt) = conn.prepare("SELECT model_id, input_cost_per_million, output_cost_per_million, cache_read_cost_per_million, cache_creation_cost_per_million FROM model_pricing") {
            if let Ok(rows) = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?.parse::<f64>().unwrap_or(0.0),
                    r.get::<_, String>(2)?.parse::<f64>().unwrap_or(0.0),
                    r.get::<_, String>(3)?.parse::<f64>().unwrap_or(0.0),
                    r.get::<_, String>(4)?.parse::<f64>().unwrap_or(0.0),
                ))
            }) {
                for r in rows.flatten() {
                    pricing_map.insert(r.0.to_lowercase(), (r.1, r.2, r.3, r.4));
                }
            }
        }

        // Helper to calculate cost
        let calc_cost = |model: &str, inp: u32, out: u32, cr: u32, cc: u32| -> String {
            let m_lower = model.to_lowercase();
            let prices = pricing_map.get(&m_lower).or_else(|| {
                pricing_map.iter().find(|(k, _)| m_lower.contains(k.as_str())).map(|(_, v)| v)
            });
            if let Some((pi, po, pcr, pcc)) = prices {
                let cost = (inp as f64 * pi + out as f64 * po + cr as f64 * pcr + cc as f64 * pcc) / 1_000_000.0;
                format!("{cost:.6}")
            } else {
                "0.000000".to_string()
            }
        };

        // Pre-fetch session_log_sync cursors for fast mtime check (avoids re-parsing unchanged files)
        let mut cursors: HashMap<String, i64> = HashMap::new();
        if let Ok(mut stmt) = conn.prepare("SELECT file_path, last_modified FROM session_log_sync") {
            if let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))) {
                for r in rows.flatten() {
                    cursors.insert(r.0, r.1);
                }
            }
        }

        let mut sync_file = |p: &Path, ingest_fn: &dyn Fn(&Connection, &Path, &dyn Fn(&str, u32, u32, u32, u32) -> String, &mut SessionSyncResult)| {
            report.files_scanned += 1;
            let mtime = p.metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            let path_str = p.to_string_lossy().to_string();
            if let Some(&last_mod) = cursors.get(&path_str) {
                if last_mod == mtime && mtime > 0 {
                    return;
                }
            }
            ingest_fn(&conn, p, &calc_cost, &mut report);
            let now_sec = chrono::Local::now().timestamp();
            let _ = conn.execute(
                "INSERT INTO session_log_sync (file_path, last_modified, last_line_offset, last_synced_at)
                 VALUES (?1, ?2, 0, ?3)
                 ON CONFLICT(file_path) DO UPDATE SET
                     last_modified = excluded.last_modified,
                     last_synced_at = excluded.last_synced_at",
                rusqlite::params![path_str, mtime, now_sec],
            );
        };

        // 1. Claude Code (~/.claude/projects/*/*.jsonl)
        let claude_projects = paths.home.join(".claude").join("projects");
        if claude_projects.is_dir() {
            if let Ok(entries) = fs::read_dir(&claude_projects) {
                for proj in entries.flatten() {
                    if proj.path().is_dir() {
                        if let Ok(files) = fs::read_dir(proj.path()) {
                            for f in files.flatten() {
                                let p = f.path();
                                if p.extension().map_or(false, |ext| ext == "jsonl") {
                                    sync_file(&p, &Self::ingest_claude_jsonl);
                                }
                            }
                        }
                    }
                }
            }
        }

        // 2. Codex (~/.codex/sessions/YYYY/MM/DD/*.jsonl + ~/.codex/archived_sessions/*.jsonl)
        let codex_sessions = paths.home.join(".codex").join("sessions");
        if codex_sessions.is_dir() {
            let mut codex_files = Vec::new();
            Self::collect_files_recursive(&codex_sessions, "jsonl", 0, 4, &mut codex_files);
            for p in codex_files {
                sync_file(&p, &Self::ingest_codex_jsonl);
            }
        }
        let codex_archived = paths.home.join(".codex").join("archived_sessions");
        if codex_archived.is_dir() {
            if let Ok(files) = fs::read_dir(&codex_archived) {
                for f in files.flatten() {
                    let p = f.path();
                    if p.extension().map_or(false, |ext| ext == "jsonl") {
                        sync_file(&p, &Self::ingest_codex_jsonl);
                    }
                }
            }
        }

        // 3. Pi (~/.pi/agent/sessions/*.jsonl)
        let pi_sessions = paths.home.join(".pi").join("agent").join("sessions");
        if pi_sessions.is_dir() {
            if let Ok(files) = fs::read_dir(&pi_sessions) {
                for f in files.flatten() {
                    let p = f.path();
                    if p.extension().map_or(false, |ext| ext == "jsonl") {
                        sync_file(&p, &Self::ingest_pi_jsonl);
                    }
                }
            }
        }

        // 4. Gemini CLI (~/.gemini/tmp/*/chats/session-*.json)
        let gemini_tmp = paths.home.join(".gemini").join("tmp");
        if gemini_tmp.is_dir() {
            let mut gemini_files = Vec::new();
            Self::collect_files_recursive(&gemini_tmp, "json", 0, 4, &mut gemini_files);
            for p in gemini_files {
                if p.file_name().map_or(false, |n| n.to_string_lossy().starts_with("session-")) {
                    sync_file(&p, &Self::ingest_gemini_json);
                }
            }
        }

        Ok(report)
    }

    fn collect_files_recursive(dir: &Path, ext: &str, current_depth: u32, max_depth: u32, files: &mut Vec<PathBuf>) {
        if current_depth > max_depth {
            return;
        }
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    Self::collect_files_recursive(&path, ext, current_depth + 1, max_depth, files);
                } else if path.extension().map_or(false, |e| e == ext) {
                    files.push(path);
                }
            }
        }
    }

    fn ingest_gemini_json(
        conn: &Connection,
        path: &Path,
        calc_cost: &dyn Fn(&str, u32, u32, u32, u32) -> String,
        report: &mut SessionSyncResult,
    ) {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return,
        };
        let val: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => return,
        };

        if let Some(messages) = val.get("messages").and_then(|m| m.as_array()) {
            for msg in messages {
                if let Some(tokens) = msg.get("tokens") {
                    let inp = tokens.get("input").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                    let out = tokens.get("output").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                    let cr = tokens.get("cached").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                    let thoughts = tokens.get("thoughts").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                    let total_out = out.saturating_add(thoughts);

                    if inp == 0 && total_out == 0 && cr == 0 {
                        continue;
                    }

                    let model = msg.get("model").and_then(|v| v.as_str()).unwrap_or("gemini-2.5-pro");
                    let msg_id = msg.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    let req_id = if !msg_id.is_empty() {
                        format!("gemini:{msg_id}")
                    } else {
                        format!("gemini:{:x}", md5_hash(&serde_json::to_string(msg).unwrap_or_default()))
                    };

                    let created_at = msg.get("timestamp")
                        .and_then(|v| v.as_str())
                        .and_then(|ts| chrono::DateTime::parse_from_rfc3339(ts).ok())
                        .map(|dt| dt.timestamp())
                        .unwrap_or_else(|| Local::now().timestamp());

                    let total_cost = calc_cost(model, inp, total_out, cr, 0);

                    let inserted = conn.execute(
                        "INSERT OR IGNORE INTO proxy_request_logs (
                            request_id, provider_id, app_type, model, input_tokens, output_tokens,
                            cache_read_tokens, cache_creation_tokens, total_cost_usd, latency_ms,
                            status_code, created_at, data_source
                        ) VALUES (?1, '_gemini_session', 'gemini', ?2, ?3, ?4, ?5, 0, ?6, 0, 200, ?7, 'gemini_session')",
                        params![req_id, model, inp, total_out, cr, total_cost, created_at],
                    );

                    if let Ok(n) = inserted {
                        if n > 0 {
                            report.imported += 1;
                        } else {
                            report.skipped += 1;
                        }
                    }
                }
            }
        }
    }

    fn ingest_claude_jsonl(
        conn: &Connection,
        path: &Path,
        calc_cost: &dyn Fn(&str, u32, u32, u32, u32) -> String,
        report: &mut SessionSyncResult,
    ) {
        let file = match fs::File::open(path) {
            Ok(f) => f,
            Err(_) => return,
        };
        let reader = std::io::BufReader::new(file);

        for line_res in reader.lines() {
            let line = match line_res {
                Ok(l) => l,
                Err(_) => continue,
            };
            if !line.contains("\"assistant\"") || !line.contains("\"usage\"") {
                continue;
            }

            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) {
                if val.get("type").and_then(|v| v.as_str()) == Some("assistant") {
                    if let Some(msg) = val.get("message") {
                        let msg_id = msg.get("id").and_then(|v| v.as_str()).unwrap_or("");
                        let model = msg.get("model").and_then(|v| v.as_str()).unwrap_or("claude-3-5-sonnet");
                        if let Some(usage) = msg.get("usage") {
                            let inp = usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                            let out = usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                            let cr = usage.get("cache_read_input_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                            let cc = usage.get("cache_creation_input_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

                            let req_id = if !msg_id.is_empty() {
                                format!("claude:{msg_id}")
                            } else {
                                format!("claude:{:x}", md5_hash(&line))
                            };

                            let created_at = val.get("timestamp")
                                .and_then(|v| v.as_str())
                                .and_then(|ts| chrono::DateTime::parse_from_rfc3339(ts).ok())
                                .map(|dt| dt.timestamp())
                                .unwrap_or_else(|| Local::now().timestamp());

                            let total_cost = calc_cost(model, inp, out, cr, cc);

                            let inserted = conn.execute(
                                "INSERT OR IGNORE INTO proxy_request_logs (
                                    request_id, provider_id, app_type, model, input_tokens, output_tokens,
                                    cache_read_tokens, cache_creation_tokens, total_cost_usd, latency_ms,
                                    status_code, created_at, data_source
                                ) VALUES (?1, '_session', 'claude', ?2, ?3, ?4, ?5, ?6, ?7, 0, 200, ?8, 'session_log')",
                                params![req_id, model, inp, out, cr, cc, total_cost, created_at],
                            );

                            if let Ok(n) = inserted {
                                if n > 0 {
                                    report.imported += 1;
                                } else {
                                    report.skipped += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn ingest_codex_jsonl(
        conn: &Connection,
        path: &Path,
        calc_cost: &dyn Fn(&str, u32, u32, u32, u32) -> String,
        report: &mut SessionSyncResult,
    ) {
        let file = match fs::File::open(path) {
            Ok(f) => f,
            Err(_) => return,
        };
        let reader = std::io::BufReader::new(file);

        let mut current_model = "gpt-5-codex".to_string();

        for line_res in reader.lines() {
            let line = match line_res {
                Ok(l) => l,
                Err(_) => continue,
            };
            if line.is_empty() {
                continue;
            }

            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) {
                let entry_type = val.get("type").and_then(|v| v.as_str()).unwrap_or("");

                // 1. Turn context: track active model
                if entry_type == "turn_context" {
                    if let Some(payload) = val.get("payload") {
                        if let Some(m) = payload.get("model").and_then(|v| v.as_str()) {
                            if !m.is_empty() {
                                current_model = m.to_string();
                            }
                        }
                    }
                    continue;
                }

                // 2. Event message with token_count
                if entry_type == "event_msg" {
                    if let Some(payload) = val.get("payload") {
                        if payload.get("type").and_then(|v| v.as_str()) == Some("token_count") {
                            if let Some(info) = payload.get("info") {
                                let usage_obj = info.get("last_token_usage").or_else(|| info.get("total_token_usage"));
                                if let Some(usage) = usage_obj {
                                    let inp = usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                                    let out = usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                                    let cr = usage.get("cached_input_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

                                    if inp == 0 && out == 0 && cr == 0 {
                                        continue;
                                    }

                                    let req_id = format!("codex:{:x}", md5_hash(&line));
                                    let created_at = val.get("timestamp")
                                        .and_then(|v| v.as_str())
                                        .and_then(|ts| chrono::DateTime::parse_from_rfc3339(ts).ok())
                                        .map(|dt| dt.timestamp())
                                        .unwrap_or_else(|| Local::now().timestamp());

                                    let total_cost = calc_cost(&current_model, inp, out, cr, 0);

                                    let inserted = conn.execute(
                                        "INSERT OR IGNORE INTO proxy_request_logs (
                                            request_id, provider_id, app_type, model, input_tokens, output_tokens,
                                            cache_read_tokens, cache_creation_tokens, total_cost_usd, latency_ms,
                                            status_code, created_at, data_source
                                        ) VALUES (?1, '_codex_session', 'codex', ?2, ?3, ?4, ?5, 0, ?6, 0, 200, ?7, 'codex_session')",
                                        params![req_id, current_model, inp, out, cr, total_cost, created_at],
                                    );

                                    if let Ok(n) = inserted {
                                        if n > 0 {
                                            report.imported += 1;
                                        } else {
                                            report.skipped += 1;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    continue;
                }

                // 3. Fallback to generic usage if present
                if let Some(usage) = val.get("usage") {
                    let inp = usage.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                    let out = usage.get("completion_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                    let cr = usage.get("prompt_tokens_details")
                        .and_then(|d| d.get("cached_tokens"))
                        .and_then(|v| v.as_u64()).unwrap_or(0) as u32;

                    let model = val.get("model").and_then(|v| v.as_str()).unwrap_or(&current_model);
                    let req_id = format!("codex:{:x}", md5_hash(&line));
                    let created_at = val.get("created_at")
                        .and_then(|v| v.as_i64())
                        .unwrap_or_else(|| Local::now().timestamp());

                    let total_cost = calc_cost(model, inp, out, cr, 0);

                    let inserted = conn.execute(
                        "INSERT OR IGNORE INTO proxy_request_logs (
                            request_id, provider_id, app_type, model, input_tokens, output_tokens,
                            cache_read_tokens, cache_creation_tokens, total_cost_usd, latency_ms,
                            status_code, created_at, data_source
                        ) VALUES (?1, '_codex_session', 'codex', ?2, ?3, ?4, ?5, 0, ?6, 0, 200, ?7, 'codex_session')",
                        params![req_id, model, inp, out, cr, total_cost, created_at],
                    );

                    if let Ok(n) = inserted {
                        if n > 0 {
                            report.imported += 1;
                        } else {
                            report.skipped += 1;
                        }
                    }
                }
            }
        }
    }

    fn ingest_pi_jsonl(
        conn: &Connection,
        path: &Path,
        calc_cost: &dyn Fn(&str, u32, u32, u32, u32) -> String,
        report: &mut SessionSyncResult,
    ) {
        let file = match fs::File::open(path) {
            Ok(f) => f,
            Err(_) => return,
        };
        let reader = std::io::BufReader::new(file);

        for line_res in reader.lines() {
            let line = match line_res {
                Ok(l) => l,
                Err(_) => continue,
            };
            if !line.contains("\"usage\"") {
                continue;
            }

            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) {
                if let Some(msg) = val.get("message").or(Some(&val)) {
                    if let Some(usage) = msg.get("usage") {
                        let inp = usage.get("input").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                        let out = usage.get("output").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                        let cr = usage.get("cacheRead").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                        let cc = usage.get("cacheWrite").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

                        let model = msg.get("model").and_then(|v| v.as_str()).unwrap_or("pi-default");
                        let req_id = format!("pi:{:x}", md5_hash(&line));
                        let created_at = val.get("timestamp")
                            .and_then(|v| v.as_i64())
                            .unwrap_or_else(|| Local::now().timestamp());

                        let total_cost = calc_cost(model, inp, out, cr, cc);

                        let inserted = conn.execute(
                            "INSERT OR IGNORE INTO proxy_request_logs (
                                request_id, provider_id, app_type, model, input_tokens, output_tokens,
                                cache_read_tokens, cache_creation_tokens, total_cost_usd, latency_ms,
                                status_code, created_at, data_source
                            ) VALUES (?1, '_pi_session', 'pi', ?2, ?3, ?4, ?5, ?6, ?7, 0, 200, ?8, 'session_log')",
                            params![req_id, model, inp, out, cr, cc, total_cost, created_at],
                        );

                        if let Ok(n) = inserted {
                            if n > 0 {
                                report.imported += 1;
                            } else {
                                report.skipped += 1;
                            }
                        }
                    }
                }
            }
        }
    }
}

fn md5_hash(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
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

        // 2. Setup gemini session
        let gemini_chats = home.join(".gemini").join("tmp").join("proj-1").join("chats");
        std::fs::create_dir_all(&gemini_chats).unwrap();
        let gemini_file = gemini_chats.join("session-2026-01-01.json");
        let gemini_content = r#"{
            "sessionId": "gem-1",
            "messages": [
                {
                    "id": "msg-1",
                    "timestamp": "2026-01-01T10:00:00Z",
                    "model": "gemini-2.5-flash",
                    "tokens": {"input": 200, "output": 80, "cached": 50, "thoughts": 10}
                }
            ]
        }"#;
        std::fs::write(&gemini_file, gemini_content).unwrap();

        // First sync: should discover nested codex and gemini
        let rep1 = db.sync_session_usage(&paths).unwrap();
        assert_eq!(rep1.files_scanned, 2, "Should scan both files");
        assert_eq!(rep1.imported, 2, "Should import both records");

        // Second sync: both files are unchanged, should skip parsing via session_log_sync cursor
        let rep2 = db.sync_session_usage(&paths).unwrap();
        assert_eq!(rep2.files_scanned, 2, "Should check both files");
        assert_eq!(rep2.imported, 0, "Unchanged files should not be re-imported");
    }
}

