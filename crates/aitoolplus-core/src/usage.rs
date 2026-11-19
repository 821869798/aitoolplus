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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

/// Flattened pricing item parsed from `https://models.dev/api.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelsDevEntry {
    pub key: String,
    pub provider_id: String,
    pub provider_name: String,
    pub raw_model_id: String,
    pub normalized_id: String,
    pub display_name: String,
    pub release_date: String,
    pub pricing: ModelPricingInfo,
}

/// Result summary for a `models.dev` pricing sync run.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelsDevSyncReport {
    pub catalog_models: usize,
    pub selected_models: usize,
    pub added_models: usize,
    pub updated_models: usize,
    pub backfilled_rows: u64,
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
        let _ = instance.backfill_missing_costs();

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

    /// Seed missing built-in model pricing and app defaults without overwriting user edits.
    fn ensure_default_pricing(&self) -> Result<(), String> {
        let conn = self.connect()?;

        let seed_data = [
            // Claude 4.x / 5.x / Fable / Mythos / Legacy
            ("claude-fable-5-2", "Claude Fable 5.2", "8", "40", "0.20", "10"),
            ("claude-fable-5-1", "Claude Fable 5.1", "10", "50", "0.25", "12.50"),
            ("claude-mythos-5-1", "Claude Mythos 5.1", "10", "50", "0.25", "12.50"),
            ("claude-fable-5", "Claude Fable 5", "10", "50", "1.00", "12.50"),
            ("claude-mythos-5", "Claude Mythos 5", "10", "50", "1.00", "12.50"),
            ("claude-opus-5-5", "Claude Opus 5.5", "4", "20", "0.20", "5"),
            ("claude-opus-5.5", "Claude Opus 5.5", "4", "20", "0.20", "5"),
            ("claude-opus-5", "Claude Opus 5", "5", "25", "0.50", "6.25"),
            ("claude-opus-4-8", "Claude Opus 4.8", "5", "25", "0.50", "6.25"),
            ("claude-sonnet-5-5", "Claude Sonnet 5.5", "2", "10", "0.10", "2.50"),
            ("claude-sonnet-5.5", "Claude Sonnet 5.5", "2", "10", "0.10", "2.50"),
            ("claude-haiku-5-5", "Claude Haiku 5.5", "0.10", "0.50", "0.01", "0.125"),
            ("claude-sonnet-5", "Claude Sonnet 5", "2", "10", "0.20", "2.50"),
            ("claude-opus-4-7", "Claude Opus 4.7", "5", "25", "0.50", "6.25"),
            ("claude-opus-4-6", "Claude Opus 4.6", "5", "25", "0.50", "6.25"),
            ("claude-sonnet-4-6", "Claude Sonnet 4.6", "3", "15", "0.30", "3.75"),
            ("claude-opus-4-6-20260206", "Claude Opus 4.6", "5", "25", "0.50", "6.25"),
            ("claude-sonnet-4-6-20260217", "Claude Sonnet 4.6", "3", "15", "0.30", "3.75"),
            ("claude-opus-4-5-20251101", "Claude Opus 4.5", "5", "25", "0.50", "6.25"),
            ("claude-sonnet-4-5-20250929", "Claude Sonnet 4.5", "3", "15", "0.30", "3.75"),
            ("claude-haiku-4-5-20251001", "Claude Haiku 4.5", "1", "5", "0.10", "1.25"),
            ("claude-opus-4-20250514", "Claude Opus 4", "15", "75", "1.50", "18.75"),
            ("claude-opus-4-1-20250805", "Claude Opus 4.1", "15", "75", "1.50", "18.75"),
            ("claude-sonnet-4-20250514", "Claude Sonnet 4", "3", "15", "0.30", "3.75"),
            ("claude-3-5-sonnet-20241022", "Claude 3.5 Sonnet", "3.0", "15.0", "0.30", "3.75"),
            ("claude-3-5-haiku-20241022", "Claude 3.5 Haiku", "0.80", "4.0", "0.08", "1.0"),
            ("claude-3-opus-20240229", "Claude 3 Opus", "15.0", "75.0", "1.50", "18.75"),
            // Gemini 2.0 / 2.5 / 3.x / 4.x (including Antigravity Argon / Carbon / Hydrogen / Neon)
            ("gemini-4-neon", "Gemini 4 Neon", "3.00", "15.00", "0.15", "0"),
            ("gemini-4-argon", "Gemini 4 Argon", "2.00", "10.00", "0.10", "0"),
            ("gemini-4-carbon", "Gemini 4 Carbon", "2.00", "10.00", "0.10", "0"),
            ("gemini-4-hydrogen", "Gemini 4 Hydrogen", "1.50", "7.50", "0.075", "0"),
            ("gemini-3.8-flash", "Gemini 3.8 Flash", "0.75", "3.75", "0.075", "0"),
            ("gemini-3.7-flash", "Gemini 3.7 Flash", "0.75", "3.75", "0.075", "0"),
            ("gemini-3.6-flash", "Gemini 3.6 Flash", "0.75", "3.75", "0.075", "0"),
            ("gemini-3.5-flash", "Gemini 3.5 Flash", "1.50", "9.00", "0.15", "0"),
            ("gemini-3.5-flash-lite", "Gemini 3.5 Flash Lite", "0.30", "2.50", "0.03", "0"),
            ("gemini-3.1-pro-preview", "Gemini 3.1 Pro Preview", "2", "12", "0.20", "0"),
            ("gemini-3.1-flash-lite", "Gemini 3.1 Flash Lite", "0.25", "1.50", "0.025", "0"),
            ("gemini-3.1-flash-lite-preview", "Gemini 3.1 Flash Lite Preview", "0.25", "1.50", "0.025", "0"),
            ("gemini-3-pro-preview", "Gemini 3 Pro Preview", "2", "12", "0.2", "0"),
            ("gemini-3-flash-preview", "Gemini 3 Flash Preview", "0.5", "3", "0.05", "0"),
            ("gemini-2.5-pro", "Gemini 2.5 Pro", "1.25", "10", "0.125", "0"),
            ("gemini-2.5-flash", "Gemini 2.5 Flash", "0.3", "2.5", "0.03", "0"),
            ("gemini-2.5-flash-lite", "Gemini 2.5 Flash Lite", "0.10", "0.40", "0.01", "0"),
            ("gemini-2.0-flash", "Gemini 2.0 Flash", "0.10", "0.40", "0.025", "0"),
            ("gemini-2.0-pro-exp", "Gemini 2.0 Pro", "1.25", "5.0", "0.3125", "0"),
            // OpenAI / GPT-5.x / 6.x
            ("gpt-6.2-sol", "GPT-6.2 Sol", "2", "10", "0.10", "2.50"),
            ("gpt-6.2-astra", "GPT-6.2 Astra", "8", "40", "0.40", "10"),
            ("gpt-6.1-sol", "GPT-6.1 Sol", "2", "10", "0.10", "2.50"),
            ("gpt-6.1-terra", "GPT-6.1 Terra", "1", "5", "0.05", "1.25"),
            ("gpt-6-astra", "GPT-6 Astra", "10", "50", "1", "12.5"),
            ("gpt-6-sol", "GPT-6 Sol", "2", "10", "0.20", "2.50"),
            ("gpt-6-luna", "GPT-6 Luna", "0.10", "0.50", "0.01", "0.125"),
            ("gpt-5.6-sol", "GPT-5.6 Sol", "4", "20", "0.40", "5"),
            ("gpt-5.6-terra", "GPT-5.6 Terra", "2", "12", "0.20", "2.50"),
            ("gpt-5.6-luna", "GPT-5.6 Luna", "0.20", "1.20", "0.02", "0.25"),
            ("gpt-5.6", "GPT-5.6 Sol", "4", "20", "0.40", "5"),
            ("gpt-5.5", "GPT-5.5", "5", "30", "0.50", "0"),
            ("gpt-5.4", "GPT-5.4", "2.50", "15", "0.25", "0"),
            ("gpt-5.4-mini", "GPT-5.4 Mini", "0.75", "4.50", "0.075", "0"),
            ("gpt-5.3-codex", "GPT-5.3 Codex", "1.75", "14", "0.175", "0"),
            ("gpt-5.2", "GPT-5.2", "1.75", "14", "0.175", "0"),
            ("gpt-5.2-codex", "GPT-5.2 Codex", "1.75", "14", "0.175", "0"),
            ("gpt-5.1", "GPT-5.1", "1.25", "10", "0.125", "0"),
            ("gpt-5.1-codex", "GPT-5.1 Codex", "1.25", "10", "0.125", "0"),
            ("gpt-5", "GPT-5", "1.25", "10", "0.125", "0"),
            ("gpt-5-codex", "GPT-5 Codex", "1.25", "10", "0.125", "0"),
            ("gpt-5-mini", "GPT-5 Mini", "0.25", "2", "0.025", "0"),
            ("gpt-4.1", "GPT-4.1", "2", "8", "0.50", "0"),
            ("gpt-4o", "GPT-4o", "2.50", "10.0", "1.25", "0"),
            ("gpt-4o-mini", "GPT-4o Mini", "0.15", "0.60", "0.075", "0"),
            ("o1", "OpenAI o1", "15.0", "60.0", "7.50", "0"),
            ("o1-mini", "OpenAI o1 Mini", "0.55", "2.20", "0.55", "0"),
            ("o3", "OpenAI o3", "2", "8", "0.50", "0"),
            ("o3-mini", "OpenAI o3 Mini", "1.10", "4.40", "0.55", "0"),
            ("o4-mini", "OpenAI o4-mini", "1.10", "4.40", "0.275", "0"),
            // DeepSeek / Grok / Kimi / MiniMax / GLM / Qwen
            ("deepseek-chat", "DeepSeek Chat", "0.44", "1.32", "0.014", "0"),
            ("deepseek-reasoner", "DeepSeek Reasoner", "0.44", "1.32", "0.014", "0"),
            ("deepseek-flash", "DeepSeek V4.1 Flash", "0.3", "1.2", "0.006", "0"),
            ("deepseek-v4.1-flash", "DeepSeek V4.1 Flash", "0.3", "1.2", "0.006", "0"),
            ("deepseek-v4-flash", "DeepSeek V4 Flash", "0.3", "1.2", "0.006", "0"),
            ("deepseek-v4-pro", "DeepSeek V4 Pro", "1.32", "3.96", "0.044", "0"),
            ("deepseek-v3.2", "DeepSeek V3.2", "0.28", "0.42", "0.028", "0"),
            ("grok-2", "Grok 2", "2.0", "10.0", "0", "0"),
            ("grok-3", "Grok 3", "3.0", "15.0", "0.75", "0"),
            ("grok-4", "Grok 4", "3", "15", "0.75", "0"),
            ("grok-4.5", "Grok 4.5", "2", "6", "0.30", "0"),
            ("grok-4.5-build", "Grok 4.5 Build", "2", "6", "0.30", "0"),
            ("grok-4.6", "Grok 4.6", "2", "6", "0.50", "0"),
            ("grok-4.6-build", "Grok 4.6 Build", "2", "6", "0.50", "0"),
            ("grok-4.7", "Grok 4.7", "2", "6", "0.50", "0"),
            ("grok-4.7-build", "Grok 4.7 Build", "2", "6", "0.50", "0"),
            ("kimi-k2.5", "Kimi K2.5", "0.60", "3.00", "0.10", "0"),
            ("kimi-k2.6", "Kimi K2.6", "0.95", "4.00", "0.16", "0"),
            ("kimi-k2.7-code", "Kimi K2.7 Code", "0.95", "4.00", "0.19", "0"),
            ("kimi-k3", "Kimi K3", "3.00", "15.00", "0.30", "3.00"),
            ("k3", "Kimi K3", "3.00", "15.00", "0.30", "3.00"),
            ("kimi-k3.1-pro", "Kimi K3.1 Pro", "2.00", "10.00", "0.20", "2.00"),
            ("k3.1-pro", "Kimi K3.1 Pro", "2.00", "10.00", "0.20", "2.00"),
            ("minimax-m2.5", "MiniMax M2.5", "0.30", "1.20", "0.03", "0.375"),
            ("minimax-m2.7", "MiniMax M2.7", "0.30", "1.20", "0.06", "0.375"),
            ("minimax-m3", "MiniMax M3", "0.30", "1.20", "0.06", "0"),
            ("minimax-m3.1-flash", "MiniMax M3.1 Flash", "0.15", "0.60", "0.015", "0"),
            ("glm-5", "GLM-5", "1", "3.2", "0.2", "0"),
            ("glm-5.3", "GLM-5.3", "1.4", "4.4", "0.26", "0"),
            ("glm-5.3-flash", "GLM-5.3-Flash", "0.15", "0.50", "0.03", "0"),
            ("qwen4-max", "Qwen4 Max", "1.50", "4.50", "0.075", "1.875"),
            ("qwen3.8-max", "Qwen3.8 Max", "2", "6", "0.25", "2.50"),
            ("qwen3-coder-plus", "Qwen3 Coder Plus", "0.65", "3.25", "0.13", "0"),
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

    /// Backfill stored zero-cost usage rows once pricing becomes available.
    pub fn backfill_missing_costs(&self) -> Result<u64, String> {
        let conn = self.connect()?;
        crate::session_usage::backfill_missing_usage_costs_on_conn(&conn, None)
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
        drop(conn);
        let _ = self.ensure_default_pricing();
        let _ = self.backfill_missing_costs();

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
        let _ = crate::session_usage::backfill_missing_usage_costs_on_conn(&conn, Some(&info.model_id));
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

    /// Return distinct model identifiers present in `proxy_request_logs` that currently lack
    /// a matching pricing rule in `model_pricing`.
    pub fn get_unpriced_used_models(&self) -> Result<Vec<String>, String> {
        let conn = self.connect()?;
        get_unpriced_used_models_on_conn(&conn)
    }

    /// Apply a parsed `models.dev` catalog to `model_pricing` (inserting newly released canonical
    /// models and any unpriced models found in local request logs, plus updating 0-cost placeholder
    /// rules without overwriting customized non-zero prices) and backfill historical `$0` rows.
    pub fn sync_from_models_dev_entries(
        &self,
        entries: &[ModelsDevEntry],
    ) -> Result<ModelsDevSyncReport, String> {
        let mut conn = self.connect()?;
        let unpriced_used = get_unpriced_used_models_on_conn(&conn)?;
        let selected = select_models_dev_sync_candidates(entries, &unpriced_used);
        if selected.is_empty() {
            return Ok(ModelsDevSyncReport {
                catalog_models: entries.len(),
                ..Default::default()
            });
        }

        let tx = conn
            .transaction()
            .map_err(|e| format!("开启 models.dev 同步事务失败: {e}"))?;
        let mut added_models = 0usize;
        let mut updated_models = 0usize;

        for item in &selected {
            let p = &item.pricing;
            let existing_costs: Option<(f64, f64)> = tx
                .query_row(
                    "SELECT COALESCE(CAST(input_cost_per_million AS REAL), 0),
                            COALESCE(CAST(output_cost_per_million AS REAL), 0)
                     FROM model_pricing WHERE model_id = ?1",
                    params![p.model_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .ok();

            match existing_costs {
                None => {
                    tx.execute(
                        "INSERT INTO model_pricing (
                            model_id, display_name, input_cost_per_million, output_cost_per_million,
                            cache_read_cost_per_million, cache_creation_cost_per_million
                        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        params![
                            p.model_id,
                            p.display_name,
                            p.input_cost_per_million,
                            p.output_cost_per_million,
                            p.cache_read_cost_per_million,
                            p.cache_creation_cost_per_million,
                        ],
                    )
                    .map_err(|e| format!("写入 models.dev 模型定价失败: {e}"))?;
                    added_models += 1;
                }
                Some((in_cost, out_cost)) if in_cost <= 0.0 && out_cost <= 0.0 => {
                    let new_in = p.input_cost_per_million.parse::<f64>().unwrap_or(0.0);
                    let new_out = p.output_cost_per_million.parse::<f64>().unwrap_or(0.0);
                    if new_in > 0.0 || new_out > 0.0 {
                        tx.execute(
                            "UPDATE model_pricing SET
                                display_name = ?2,
                                input_cost_per_million = ?3,
                                output_cost_per_million = ?4,
                                cache_read_cost_per_million = ?5,
                                cache_creation_cost_per_million = ?6
                             WHERE model_id = ?1",
                            params![
                                p.model_id,
                                p.display_name,
                                p.input_cost_per_million,
                                p.output_cost_per_million,
                                p.cache_read_cost_per_million,
                                p.cache_creation_cost_per_million,
                            ],
                        )
                        .map_err(|e| format!("更新零成本模型定价失败: {e}"))?;
                        updated_models += 1;
                    }
                }
                Some(_) => {}
            }
        }

        tx.commit()
            .map_err(|e| format!("提交 models.dev 同步事务失败: {e}"))?;

        let backfilled_rows = if added_models > 0 || updated_models > 0 {
            crate::session_usage::backfill_missing_usage_costs_on_conn(&conn, None).unwrap_or(0)
        } else {
            0
        };

        Ok(ModelsDevSyncReport {
            catalog_models: entries.len(),
            selected_models: selected.len(),
            added_models,
            updated_models,
            backfilled_rows,
        })
    }

    /// Fetch the latest catalog from `https://models.dev/api.json`, sync new/unpriced models
    /// into `model_pricing`, backfill historical `$0` rows, and return both the sync report
    /// and the full parsed catalog for interactive browsing.
    pub fn sync_from_models_dev(&self) -> Result<(ModelsDevSyncReport, Vec<ModelsDevEntry>), String> {
        let entries = fetch_models_dev_catalog()?;
        let report = self.sync_from_models_dev_entries(&entries)?;
        Ok((report, entries))
    }
}

pub const MODELS_DEV_API_URL: &str = "https://models.dev/api.json";
const MODELS_DEV_MAX_RESPONSE_BYTES: usize = 32 * 1024 * 1024;
const MODELS_DEV_FAMILY_LIMIT: usize = 10;

const NON_TEXT_MODEL_MARKERS: &[&str] = &[
    "audio",
    "deprecated",
    "embedding",
    "image",
    "moderation",
    "realtime",
    "transcribe",
    "tts",
    "video",
];

/// Normalize a `models.dev` model identifier (strips `vendor/`, `:variant`, `@`, `[1m]`).
pub fn normalize_models_dev_model_id(model_id: &str) -> String {
    let after_slash = model_id
        .rsplit_once('/')
        .map(|(_, tail)| tail)
        .unwrap_or(model_id);
    let before_colon = after_slash
        .split_once(':')
        .map(|(head, _)| head)
        .unwrap_or(after_slash);
    let mut normalized = before_colon.trim().replace('@', "-").to_ascii_lowercase();
    if let Some(stripped) = normalized.strip_suffix("[1m]") {
        normalized = stripped.trim().to_string();
    }
    normalized
}

/// Format a per-1M USD price value cleanly (up to 6 decimal places, trimming trailing zeros).
pub fn format_models_dev_price(value: f64) -> String {
    if !value.is_finite() || value <= 0.0 || value >= 1e12 {
        return "0".to_string();
    }
    let formatted = format!("{value:.6}");
    let trimmed = formatted.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

fn is_text_pricing_model(raw_model_id: &str, model_obj: &serde_json::Value) -> bool {
    if model_obj
        .get("status")
        .and_then(|v| v.as_str())
        .map(|s| s.eq_ignore_ascii_case("deprecated"))
        .unwrap_or(false)
    {
        return false;
    }

    if let Some(outputs) = model_obj
        .get("modalities")
        .and_then(|m| m.get("output"))
        .and_then(|o| o.as_array())
    {
        let modalities: Vec<String> = outputs
            .iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.to_ascii_lowercase())
            .collect();
        if !modalities.is_empty()
            && (!modalities.iter().any(|m| m == "text")
                || modalities
                    .iter()
                    .any(|m| matches!(m.as_str(), "audio" | "image" | "video")))
        {
            return false;
        }
    }

    let model_name = model_obj
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let searchable = format!("{raw_model_id} {model_name}").to_ascii_lowercase();
    !NON_TEXT_MODEL_MARKERS
        .iter()
        .any(|marker| searchable.contains(marker))
}

/// Parse a `https://models.dev/api.json` payload into a sorted list of text model pricing entries.
pub fn parse_models_dev_catalog(json: &str) -> Result<Vec<ModelsDevEntry>, String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("解析 models.dev JSON 失败: {e}"))?;
    let providers = root
        .as_object()
        .ok_or_else(|| "models.dev 响应格式无效（顶层应为对象）".to_string())?;

    let mut entries = Vec::new();
    for (provider_id, provider_val) in providers {
        let Some(provider_obj) = provider_val.as_object() else {
            continue;
        };
        let provider_name = provider_obj
            .get("name")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(provider_id.as_str())
            .to_string();
        let Some(models_obj) = provider_obj.get("models").and_then(|v| v.as_object()) else {
            continue;
        };

        for (raw_model_id, model_val) in models_obj {
            if !is_text_pricing_model(raw_model_id, model_val) {
                continue;
            }
            let Some(cost_obj) = model_val.get("cost").and_then(|v| v.as_object()) else {
                continue;
            };
            let input_opt = cost_obj.get("input").and_then(|v| v.as_f64());
            let output_opt = cost_obj.get("output").and_then(|v| v.as_f64());
            if input_opt.is_none() && output_opt.is_none() {
                continue;
            }

            let normalized_id = normalize_models_dev_model_id(raw_model_id);
            if normalized_id.is_empty() {
                continue;
            }

            let display_name = model_val
                .get("name")
                .and_then(|v| v.as_str())
                .filter(|s| !s.trim().is_empty())
                .unwrap_or(raw_model_id.as_str())
                .to_string();
            let release_date = model_val
                .get("release_date")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let input = input_opt.unwrap_or(0.0);
            let output = output_opt.unwrap_or(0.0);
            let cache_read = cost_obj
                .get("cache_read")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let cache_write = cost_obj
                .get("cache_write")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);

            entries.push(ModelsDevEntry {
                key: format!("{provider_id}/{raw_model_id}"),
                provider_id: provider_id.clone(),
                provider_name: provider_name.clone(),
                raw_model_id: raw_model_id.clone(),
                normalized_id: normalized_id.clone(),
                display_name: display_name.clone(),
                release_date,
                pricing: ModelPricingInfo {
                    model_id: normalized_id,
                    display_name,
                    input_cost_per_million: format_models_dev_price(input),
                    output_cost_per_million: format_models_dev_price(output),
                    cache_read_cost_per_million: format_models_dev_price(cache_read),
                    cache_creation_cost_per_million: format_models_dev_price(cache_write),
                },
            });
        }
    }

    entries.sort_by(|a, b| {
        b.release_date
            .cmp(&a.release_date)
            .then_with(|| a.display_name.cmp(&b.display_name))
            .then_with(|| a.key.cmp(&b.key))
    });
    Ok(entries)
}

/// Fetch and parse the public `https://models.dev/api.json` catalog via the global/system proxy agent.
pub fn fetch_models_dev_catalog() -> Result<Vec<ModelsDevEntry>, String> {
    use std::io::Read;

    let agent = crate::skills::skills_http_agent(45);
    let response = agent
        .get(MODELS_DEV_API_URL)
        .set("Accept", "application/json")
        .set("User-Agent", "AIToolPlus-ModelsDevSync")
        .call()
        .map_err(|e| crate::skills::map_skills_request_error(&e, "拉取 models.dev 模型定价"))?;

    let mut buf = Vec::new();
    response
        .into_reader()
        .take((MODELS_DEV_MAX_RESPONSE_BYTES + 1) as u64)
        .read_to_end(&mut buf)
        .map_err(|e| format!("读取 models.dev 响应失败: {e}"))?;

    if buf.len() > MODELS_DEV_MAX_RESPONSE_BYTES {
        return Err(format!(
            "models.dev 响应体积超过上限 ({} bytes)",
            MODELS_DEV_MAX_RESPONSE_BYTES
        ));
    }

    let body = String::from_utf8(buf).map_err(|e| format!("models.dev 响应非 UTF-8: {e}"))?;
    parse_models_dev_catalog(&body)
}

fn is_official_family_match(provider_id: &str, model_id_lower: &str) -> Option<&'static str> {
    match provider_id {
        "anthropic" if model_id_lower.starts_with("claude-") => Some("claude"),
        "openai"
            if model_id_lower.starts_with("gpt-")
                || model_id_lower.starts_with("o1-")
                || model_id_lower.starts_with("o3-")
                || model_id_lower.starts_with("o4-") =>
        {
            Some("gpt")
        }
        "google" if model_id_lower.starts_with("gemini-") => Some("gemini"),
        "xai" if model_id_lower.starts_with("grok-") => Some("grok"),
        "deepseek" if model_id_lower.starts_with("deepseek-") => Some("deepseek"),
        "alibaba" if model_id_lower.starts_with("qwen") => Some("qwen"),
        "xiaomi" if model_id_lower.starts_with("mimo-") => Some("mimo"),
        "longcat" if model_id_lower.starts_with("longcat-") => Some("longcat"),
        "moonshotai" if model_id_lower.starts_with("kimi-") => Some("kimi"),
        "minimax" | "minimax-cn" if model_id_lower.starts_with("minimax-m") => Some("minimax"),
        "zai" | "zhipuai" if model_id_lower.starts_with("glm-") => Some("glm"),
        "mistral" if model_id_lower.starts_with("mistral-") || model_id_lower.starts_with("codestral") => {
            Some("mistral")
        }
        _ => None,
    }
}

/// Select canonical recent models per official provider family PLUS any `models.dev` entries
/// that match unpriced models already used in the user's `proxy_request_logs`.
pub fn select_models_dev_sync_candidates(
    entries: &[ModelsDevEntry],
    unpriced_used_models: &[String],
) -> Vec<ModelsDevEntry> {
    let mut selected_by_norm_id: std::collections::BTreeMap<String, ModelsDevEntry> =
        std::collections::BTreeMap::new();
    let mut family_counts: std::collections::HashMap<&'static str, usize> =
        std::collections::HashMap::new();

    // 1. Official provider recent releases per family (entries are already sorted by release_date desc).
    for entry in entries {
        let raw_lower = entry.raw_model_id.to_ascii_lowercase();
        if let Some(family) = is_official_family_match(&entry.provider_id, &raw_lower) {
            let count = family_counts.entry(family).or_insert(0);
            if *count < MODELS_DEV_FAMILY_LIMIT {
                selected_by_norm_id
                    .entry(entry.normalized_id.clone())
                    .or_insert_with(|| entry.clone());
                *count += 1;
            }
        }
    }

    // 2. Any unpriced model present in the user's local usage logs matched against all providers
    //    (preferring official family providers first, then newest release_date).
    if !unpriced_used_models.is_empty() {
        let mut needed_candidates = std::collections::HashSet::new();
        for raw_used in unpriced_used_models {
            let norm = normalize_models_dev_model_id(raw_used);
            if !norm.is_empty() {
                needed_candidates.insert(norm);
            }
            for cand in crate::session_usage::model_pricing_candidates(raw_used) {
                if !cand.is_empty() {
                    needed_candidates.insert(cand);
                }
            }
        }

        // First pass: official providers matching needed candidates
        for entry in entries {
            let raw_lower = entry.raw_model_id.to_ascii_lowercase();
            if is_official_family_match(&entry.provider_id, &raw_lower).is_some()
                && needed_candidates.contains(&entry.normalized_id)
            {
                selected_by_norm_id
                    .entry(entry.normalized_id.clone())
                    .or_insert_with(|| entry.clone());
            }
        }
        // Second pass: any provider in models.dev matching needed candidates
        for entry in entries {
            if needed_candidates.contains(&entry.normalized_id) {
                selected_by_norm_id
                    .entry(entry.normalized_id.clone())
                    .or_insert_with(|| entry.clone());
            }
        }
    }

    selected_by_norm_id.into_values().collect()
}

fn get_unpriced_used_models_on_conn(conn: &Connection) -> Result<Vec<String>, String> {
    let pricing_table = crate::session_usage::PricingTable::load(conn);
    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT model, request_model, pricing_model
             FROM proxy_request_logs
             WHERE COALESCE(CAST(total_cost_usd AS REAL), 0) <= 0",
        )
        .map_err(|e| format!("查询未定价模型失败: {e}"))?;

    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })
        .map_err(|e| format!("读取未定价模型失败: {e}"))?;

    let mut unpriced = std::collections::BTreeSet::new();
    for r in rows.flatten() {
        for candidate in [Some(r.0.as_str()), r.1.as_deref(), r.2.as_deref()]
            .into_iter()
            .flatten()
        {
            let trimmed = candidate.trim();
            if trimmed.is_empty()
                || trimmed.starts_with('<')
                || trimmed.eq_ignore_ascii_case("unknown")
                || trimmed.eq_ignore_ascii_case("auto")
                || trimmed.eq_ignore_ascii_case("default")
            {
                continue;
            }
            if pricing_table.find_price(trimmed).is_none() {
                unpriced.insert(trimmed.to_string());
            }
        }
    }
    Ok(unpriced.into_iter().collect())
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

    #[test]
    fn test_models_dev_catalog_sync_and_backfill() {
        let sample_json = r#"{
            "anthropic": {
                "name": "Anthropic",
                "models": {
                    "claude-sonnet-6-0[1m]": {
                        "name": "Claude Sonnet 6.0",
                        "release_date": "2027-03-01",
                        "modalities": { "input": ["text"], "output": ["text"] },
                        "cost": { "input": 3.5, "output": 17.5, "cache_read": 0.35, "cache_write": 4.375 }
                    },
                    "claude-old-deprecated": {
                        "name": "Claude Old",
                        "status": "deprecated",
                        "cost": { "input": 10.0, "output": 30.0 }
                    }
                }
            },
            "openai": {
                "name": "OpenAI",
                "models": {
                    "text-embedding-4": {
                        "name": "Text Embedding 4",
                        "cost": { "input": 0.02, "output": 0.0 }
                    },
                    "gpt-audio-next": {
                        "name": "GPT Audio Next",
                        "modalities": { "input": ["text"], "output": ["text", "audio"] },
                        "cost": { "input": 5.0, "output": 20.0 }
                    }
                }
            },
            "openrouter": {
                "name": "OpenRouter",
                "models": {
                    "stepfun/step-4-pro:thinking": {
                        "name": "Step 4 Pro",
                        "release_date": "2027-02-15",
                        "cost": { "input": 1.2, "output": 4.8, "cache_read": 0.12 }
                    }
                }
            }
        }"#;

        let catalog = parse_models_dev_catalog(sample_json).expect("parse models.dev json");
        assert_eq!(catalog.len(), 2, "Should filter deprecated, embedding, and audio output models");
        assert_eq!(catalog[0].normalized_id, "claude-sonnet-6-0");
        assert_eq!(catalog[0].pricing.input_cost_per_million, "3.5");
        assert_eq!(catalog[1].normalized_id, "step-4-pro");

        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().join("home"), tmp.path().join("appdata"));
        let db = UsageDb::open(&paths).expect("open usage db");
        let now = chrono::Utc::now().timestamp();
        {
            let conn = db.connect().unwrap();
            for (req_id, model) in [
                ("req-future-claude", "claude-sonnet-6-0-20270301"),
                ("req-future-step", "stepfun/step-4-pro"),
            ] {
                conn.execute(
                    "INSERT INTO proxy_request_logs (
                        request_id, provider_id, app_type, model, input_tokens, output_tokens,
                        cache_read_tokens, cache_creation_tokens, total_cost_usd, latency_ms,
                        status_code, created_at, data_source
                    ) VALUES (?1, '_session', 'claude', ?2, 1000000, 1000000, 0, 0, '0', 100, 200, ?3, 'session')",
                    params![req_id, model, now],
                )
                .unwrap();
            }
        }

        let unpriced_before = db.get_unpriced_used_models().unwrap();
        assert!(unpriced_before.contains(&"claude-sonnet-6-0-20270301".to_string()));
        assert!(unpriced_before.contains(&"stepfun/step-4-pro".to_string()));

        let report = db.sync_from_models_dev_entries(&catalog).expect("sync models.dev");
        assert_eq!(report.added_models, 2);
        assert_eq!(report.backfilled_rows, 2);
        assert!(db.get_unpriced_used_models().unwrap().is_empty());

        let summary = db.get_usage_summary(None, None, None, None, None).unwrap();
        let total_cost: f64 = summary.total_cost.parse().unwrap_or(0.0);
        // (3.5 + 17.5) + (1.2 + 4.8) = 21.0 + 6.0 = 27.0
        assert!(
            (total_cost - 27.0).abs() < 1e-4,
            "Expected historical $0 logs for future models to backfill to $27.0, got {total_cost}"
        );
    }
}

