//! Multi-agent session usage ingestion engine.
//!
//! Synchronizes token usage and cost records from local CLI/IDE session stores into
//! `proxy_request_logs` with deterministic SHA-256 request IDs, cross-tool deduplication,
//! incremental mtime+byte cursors, fork-replay protection, and cache-inclusive billing.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::paths::Paths;
use crate::tools::ToolId;
use crate::usage::SessionSyncResult;

pub(crate) const INPUT_TOKEN_SEMANTICS_FRESH: i64 = 2;
const SESSION_PROXY_DEDUP_WINDOW_SECONDS: i64 = 120;
const MAX_LABEL_BYTES: usize = 256;
const MAX_JSON_FILE_BYTES: u64 = 64 * 1024 * 1024;
const DEFAULT_MAX_DEPTH: usize = 10;

// ---------------------------------------------------------------------------
// Pricing & Cost Calculation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub(crate) struct ModelPrice {
    pub input_per_m: f64,
    pub output_per_m: f64,
    pub cache_read_per_m: f64,
    pub cache_creation_per_m: f64,
}

pub(crate) struct PricingTable {
    exact: HashMap<String, ModelPrice>,
    sorted_keys: Vec<(String, ModelPrice)>,
    app_multipliers: HashMap<String, f64>,
    has_any_proxy_logs: bool,
    pi_parent_cache: std::cell::RefCell<HashMap<PathBuf, HashSet<String>>>,
    codex_parent_cache: std::cell::RefCell<HashMap<PathBuf, Vec<(i64, CodexTokenSig)>>>,
}

impl PricingTable {
    pub(crate) fn load(conn: &Connection) -> Self {
        let mut exact = HashMap::new();
        if let Ok(mut stmt) = conn.prepare(
            "SELECT model_id, input_cost_per_million, output_cost_per_million,
                    cache_read_cost_per_million, cache_creation_cost_per_million
             FROM model_pricing",
        ) {
            if let Ok(rows) = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    ModelPrice {
                        input_per_m: r.get::<_, String>(1)?.parse::<f64>().unwrap_or(0.0),
                        output_per_m: r.get::<_, String>(2)?.parse::<f64>().unwrap_or(0.0),
                        cache_read_per_m: r.get::<_, String>(3)?.parse::<f64>().unwrap_or(0.0),
                        cache_creation_per_m: r.get::<_, String>(4)?.parse::<f64>().unwrap_or(0.0),
                    },
                ))
            }) {
                for (id, price) in rows.flatten() {
                    exact.insert(id.to_ascii_lowercase(), price);
                }
            }
        }

        let mut sorted_keys: Vec<(String, ModelPrice)> =
            exact.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        // Longest keys first for deterministic specificity during prefix/substring fallback
        sorted_keys.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(&b.0)));

        let mut app_multipliers = HashMap::new();
        if let Ok(mut stmt) =
            conn.prepare("SELECT app_type, cost_multiplier FROM app_pricing_config")
        {
            if let Ok(rows) = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?.to_ascii_lowercase(),
                    r.get::<_, String>(1)?.parse::<f64>().unwrap_or(1.0),
                ))
            }) {
                for (app, mult) in rows.flatten() {
                    if mult.is_finite() && mult >= 0.0 {
                        app_multipliers.insert(app, mult);
                    }
                }
            }
        }

        let has_any_proxy_logs: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM proxy_request_logs WHERE COALESCE(data_source, 'proxy') = 'proxy')",
                [],
                |r| r.get(0),
            )
            .unwrap_or(false);

        Self {
            exact,
            sorted_keys,
            app_multipliers,
            has_any_proxy_logs,
            pi_parent_cache: std::cell::RefCell::new(HashMap::new()),
            codex_parent_cache: std::cell::RefCell::new(HashMap::new()),
        }
    }

    pub(crate) fn find_price(&self, model: &str) -> Option<&ModelPrice> {
        let candidates = model_pricing_candidates(model);
        for c in &candidates {
            if let Some(p) = self.exact.get(c) {
                return Some(p);
            }
        }
        // Prefix match (`stored_model` starts with `candidate-` or `candidate` starts with `stored-`)
        for c in &candidates {
            if c.len() < 4 {
                continue;
            }
            for (k, p) in &self.sorted_keys {
                if k.strip_prefix(c.as_str()).is_some_and(|rest| rest.starts_with('-'))
                    || c.strip_prefix(k.as_str()).is_some_and(|rest| rest.starts_with('-'))
                {
                    return Some(p);
                }
            }
        }
        // Deterministic longest substring fallback
        for c in &candidates {
            if c.len() < 4 {
                continue;
            }
            for (k, p) in &self.sorted_keys {
                if k.len() >= 4 && (c.contains(k.as_str()) || k.contains(c.as_str())) {
                    return Some(p);
                }
            }
        }
        None
    }

    pub(crate) fn calculate_costs(
        &self,
        app_type: &str,
        model: &str,
        fresh_input: u32,
        output: u32,
        cache_read: u32,
        cache_creation: u32,
        cache_creation_1h: u32,
        explicit_cost: Option<CostBreakdown>,
    ) -> CostBreakdown {
        if let Some(mut c) = explicit_cost {
            if c.total > 0.0 {
                let mult = self.app_multiplier(app_type);
                if (mult - 1.0).abs() > f64::EPSILON {
                    c.input *= mult;
                    c.output *= mult;
                    c.cache_read *= mult;
                    c.cache_creation *= mult;
                    c.total *= mult;
                }
                return c;
            }
        }
        let Some(p) = self.find_price(model) else {
            return CostBreakdown::default();
        };
        let mult = self.app_multiplier(app_type);
        let input_cost = (fresh_input as f64 * p.input_per_m) / 1_000_000.0;
        let output_cost = (output as f64 * p.output_per_m) / 1_000_000.0;
        let cache_read_cost = (cache_read as f64 * p.cache_read_per_m) / 1_000_000.0;
        let cc_1h = cache_creation_1h.min(cache_creation);
        let cc_5m = cache_creation - cc_1h;
        // 5m cache write uses base `cache_creation_per_m`; 1h Anthropic write is 1.6x (2.0 / 1.25)
        let cache_creation_cost =
            ((cc_5m as f64 + cc_1h as f64 * 1.6) * p.cache_creation_per_m) / 1_000_000.0;
        let base_total = input_cost + output_cost + cache_read_cost + cache_creation_cost;
        CostBreakdown {
            input: input_cost * mult,
            output: output_cost * mult,
            cache_read: cache_read_cost * mult,
            cache_creation: cache_creation_cost * mult,
            total: base_total * mult,
        }
    }

    fn app_multiplier(&self, app_type: &str) -> f64 {
        self.app_multipliers
            .get(&app_type.to_ascii_lowercase())
            .copied()
            .unwrap_or(1.0)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct CostBreakdown {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_creation: f64,
    pub total: f64,
}

fn push_candidate(out: &mut Vec<String>, s: &str) {
    let s = s.trim().to_ascii_lowercase();
    if !s.is_empty() && !out.contains(&s) {
        out.push(s);
    }
}

fn model_pricing_candidates(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return out;
    }

    // Strip routing/tag suffixes such as `[1m]`, `:free`, `:thinking`, `@vertex`
    let base_no_bracket = trimmed
        .split('[')
        .next()
        .unwrap_or(trimmed)
        .split(':')
        .next()
        .unwrap_or(trimmed)
        .split('@')
        .next()
        .unwrap_or(trimmed)
        .trim();

    push_candidate(&mut out, trimmed);
    push_candidate(&mut out, base_no_bracket);

    // Strip provider prefixes (e.g. `openrouter/anthropic/claude-sonnet-4-6` -> `claude-sonnet-4-6`)
    for source in [trimmed, base_no_bracket] {
        let mut rest = source;
        while let Some((_, tail)) = rest.split_once('/') {
            push_candidate(&mut out, tail);
            rest = tail;
        }
    }

    // Strip trailing 8-digit date suffix like `-20250929` or `-2026-02-05`
    let current = out.clone();
    for item in current {
        if let Some((stem, tail)) = item.rsplit_once('-') {
            if tail.len() == 8 && tail.chars().all(|c| c.is_ascii_digit()) {
                push_candidate(&mut out, stem);
            }
        }
    }

    out
}

// ---------------------------------------------------------------------------
// Unified Usage Record & Database Persistence
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub(crate) struct UsageRecord {
    pub request_id: String,
    pub alternate_request_ids: Vec<String>,
    pub dedup_namespace: Option<&'static str>,
    pub semantic_id: Option<String>,
    pub has_entry_id: bool,
    pub provider_id: String,
    pub app_type: String,
    pub model: String,
    pub request_model: String,
    pub fresh_input_tokens: u32,
    pub output_tokens: u32,
    pub cache_read_tokens: u32,
    pub cache_creation_tokens: u32,
    pub cache_creation_1h_tokens: u32,
    pub explicit_cost: Option<CostBreakdown>,
    pub status_code: i64,
    pub error_message: Option<String>,
    pub session_id: Option<String>,
    pub provider_type: &'static str,
    pub data_source: &'static str,
    pub is_streaming: bool,
    pub created_at: i64,
    pub update_if_higher_output: bool,
}

fn insert_or_update_record(
    conn: &Connection,
    pricing: &PricingTable,
    rec: &UsageRecord,
    report: &mut SessionSyncResult,
) -> Result<(), String> {
    let model = clamp_label(&rec.model, "unknown");
    let request_model = clamp_label(&rec.request_model, &model);
    let provider_id = clamp_label(&rec.provider_id, "_session");

    // 1. Check primary request_id & alternate request_ids (cross-tool compatibility with cc-switch/legacy)
    let mut existing: Option<(String, i64)> = conn
        .prepare_cached("SELECT request_id, output_tokens FROM proxy_request_logs WHERE request_id = ?1")
        .and_then(|mut s| {
            s.query_row(params![rec.request_id], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()
        })
        .map_err(|e| format!("查询 request_id 失败: {e}"))?;

    if existing.is_none() {
        for alt in &rec.alternate_request_ids {
            existing = conn
                .prepare_cached("SELECT request_id, output_tokens FROM proxy_request_logs WHERE request_id = ?1")
                .and_then(|mut s| {
                    s.query_row(params![alt], |r| Ok((r.get(0)?, r.get(1)?)))
                        .optional()
                })
                .map_err(|e| format!("查询 alternate request_id 失败: {e}"))?;
            if existing.is_some() {
                break;
            }
        }
    }

    if let Some((stored_id, stored_out)) = existing {
        if rec.update_if_higher_output && (rec.output_tokens as i64) > stored_out {
            let costs = pricing.calculate_costs(
                &rec.app_type,
                &request_model,
                rec.fresh_input_tokens,
                rec.output_tokens,
                rec.cache_read_tokens,
                rec.cache_creation_tokens,
                rec.cache_creation_1h_tokens,
                rec.explicit_cost,
            );
            conn.prepare_cached(
                "UPDATE proxy_request_logs SET
                    input_tokens = ?1, output_tokens = ?2, cache_read_tokens = ?3,
                    cache_creation_tokens = ?4, input_cost_usd = ?5, output_cost_usd = ?6,
                    cache_read_cost_usd = ?7, cache_creation_cost_usd = ?8, total_cost_usd = ?9,
                    input_token_semantics = ?10,
                    session_id = COALESCE(session_id, ?11)
                 WHERE request_id = ?12",
            )
            .and_then(|mut s| {
                s.execute(params![
                    rec.fresh_input_tokens as i64,
                    rec.output_tokens as i64,
                    rec.cache_read_tokens as i64,
                    rec.cache_creation_tokens as i64,
                    format!("{:.6}", costs.input),
                    format!("{:.6}", costs.output),
                    format!("{:.6}", costs.cache_read),
                    format!("{:.6}", costs.cache_creation),
                    format!("{:.6}", costs.total),
                    INPUT_TOKEN_SEMANTICS_FRESH,
                    rec.session_id,
                    stored_id,
                ])
            })
            .map_err(|e| format!("更新增量会话记录失败: {e}"))?;
        }
        report.skipped += 1;
        return Ok(());
    }

    // 2. Semantic dedup check (e.g. forked Pi/OhMyPi sessions)
    // Split into two separate predicates so SQLite uses PK / idx_session_usage_dedup_semantic
    // instead of falling back to a full scan across the namespace via OR.
    if let (Some(ns), Some(sem)) = (rec.dedup_namespace, rec.semantic_id.as_deref()) {
        let seen_by_req: bool = conn
            .prepare_cached(
                "SELECT EXISTS(
                    SELECT 1 FROM session_usage_dedup
                    WHERE data_source = ?1 AND request_id = ?2
                )",
            )
            .and_then(|mut s| s.query_row(params![ns, rec.request_id], |r| r.get(0)))
            .unwrap_or(false);
        let seen_by_sem: bool = if seen_by_req {
            true
        } else {
            conn.prepare_cached(
                "SELECT EXISTS(
                    SELECT 1 FROM session_usage_dedup
                    WHERE data_source = ?1 AND semantic_id = ?2
                )",
            )
            .and_then(|mut s| s.query_row(params![ns, sem], |r| r.get(0)))
            .unwrap_or(false)
        };
        if seen_by_sem {
            report.skipped += 1;
            return Ok(());
        }
    }

    // 3. Cross-source proxy fingerprint dedup (+/- 120s against data_source = 'proxy')
    if pricing.has_any_proxy_logs && has_matching_proxy_log(conn, rec, &model)? {
        report.skipped += 1;
        return Ok(());
    }

    let costs = pricing.calculate_costs(
        &rec.app_type,
        &request_model,
        rec.fresh_input_tokens,
        rec.output_tokens,
        rec.cache_read_tokens,
        rec.cache_creation_tokens,
        rec.cache_creation_1h_tokens,
        rec.explicit_cost,
    );

    let inserted = conn
        .prepare_cached(
            "INSERT OR IGNORE INTO proxy_request_logs (
                request_id, provider_id, app_type, model, request_model,
                input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
                input_token_semantics,
                input_cost_usd, output_cost_usd, cache_read_cost_usd, cache_creation_cost_usd, total_cost_usd,
                latency_ms, status_code, error_message, session_id,
                provider_type, is_streaming, cost_multiplier, created_at, data_source
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, 0, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)",
        )
        .and_then(|mut s| {
            s.execute(params![
                rec.request_id,
                provider_id,
                rec.app_type,
                model,
                request_model,
                rec.fresh_input_tokens as i64,
                rec.output_tokens as i64,
                rec.cache_read_tokens as i64,
                rec.cache_creation_tokens as i64,
                INPUT_TOKEN_SEMANTICS_FRESH,
                format!("{:.6}", costs.input),
                format!("{:.6}", costs.output),
                format!("{:.6}", costs.cache_read),
                format!("{:.6}", costs.cache_creation),
                format!("{:.6}", costs.total),
                rec.status_code,
                rec.error_message,
                rec.session_id,
                rec.provider_type,
                if rec.is_streaming { 1i64 } else { 0i64 },
                format!("{:.4}", pricing.app_multiplier(&rec.app_type)),
                rec.created_at,
                rec.data_source,
            ])
        })
        .map_err(|e| format!("写入使用统计日志失败: {e}"))?;

    if inserted > 0 {
        report.imported += 1;
        if let (Some(ns), Some(sem)) = (rec.dedup_namespace, rec.semantic_id.as_deref()) {
            let _ = conn
                .prepare_cached(
                    "INSERT OR IGNORE INTO session_usage_dedup
                     (data_source, request_id, semantic_id, has_entry_id)
                     VALUES (?1, ?2, ?3, ?4)",
                )
                .and_then(|mut s| {
                    s.execute(params![
                        ns,
                        rec.request_id,
                        sem,
                        if rec.has_entry_id { 1i64 } else { 0i64 },
                    ])
                });
        }
    } else {
        report.skipped += 1;
    }
    Ok(())
}

fn has_matching_proxy_log(conn: &Connection, rec: &UsageRecord, model: &str) -> Result<bool, String> {
    conn.prepare_cached(
        "SELECT EXISTS (
            SELECT 1 FROM proxy_request_logs l
            WHERE COALESCE(l.data_source, 'proxy') = 'proxy'
              AND l.app_type = ?1
              AND l.status_code >= 200 AND l.status_code < 300
              AND l.input_tokens = ?3
              AND l.output_tokens = ?4
              AND l.cache_read_tokens = ?5
              AND l.cache_creation_tokens = ?6
              AND l.created_at BETWEEN ?7 - ?8 AND ?7 + ?8
              AND (LOWER(l.model) = LOWER(?2) OR LOWER(l.model) = 'unknown' OR LOWER(?2) = 'unknown')
        )",
    )
    .and_then(|mut s| {
        s.query_row(
            params![
                rec.app_type,
                model,
                rec.fresh_input_tokens as i64,
                rec.output_tokens as i64,
                rec.cache_read_tokens as i64,
                rec.cache_creation_tokens as i64,
                rec.created_at,
                SESSION_PROXY_DEDUP_WINDOW_SECONDS,
            ],
            |r| r.get::<_, bool>(0),
        )
    })
    .map_err(|e| format!("查询 proxy 去重失败: {e}"))
}

// ---------------------------------------------------------------------------
// Cursor Management (handles 19-digit ns, 13-digit ms, 10-digit s + byte size)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct CursorRow {
    last_modified: i64,
    last_line_offset: i64,
    last_byte_offset: Option<i64>,
}

#[derive(Debug, Clone, Copy)]
struct FileStamp {
    mtime_nanos: i64,
    mtime_millis: i64,
    mtime_secs: i64,
    byte_size: i64,
}

impl FileStamp {
    fn from_path(path: &Path) -> Self {
        let meta = match fs::metadata(path) {
            Ok(m) => m,
            Err(_) => {
                return Self {
                    mtime_nanos: 0,
                    mtime_millis: 0,
                    mtime_secs: 0,
                    byte_size: 0,
                }
            }
        };
        let dur = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok());
        let (ns, ms, s) = match dur {
            Some(d) => (d.as_nanos() as i64, d.as_millis() as i64, d.as_secs() as i64),
            None => (0, 0, 0),
        };
        Self {
            mtime_nanos: ns,
            mtime_millis: ms,
            mtime_secs: s,
            byte_size: meta.len() as i64,
        }
    }

    /// For SQLite DB files, include `-wal` and `-shm` sidecars in mtime & size
    /// so WAL appends trigger incremental sync even when the main DB mtime hasn't changed.
    fn from_sqlite_path(path: &Path) -> Self {
        let mut stamp = Self::from_path(path);
        for suffix in ["-wal", "-shm"] {
            let sidecar = PathBuf::from(format!("{}{}", path.display(), suffix));
            if sidecar.exists() {
                let s = Self::from_path(&sidecar);
                stamp.mtime_nanos = stamp.mtime_nanos.max(s.mtime_nanos);
                stamp.mtime_millis = stamp.mtime_millis.max(s.mtime_millis);
                stamp.mtime_secs = stamp.mtime_secs.max(s.mtime_secs);
                stamp.byte_size = stamp.byte_size.saturating_add(s.byte_size);
            }
        }
        stamp
    }

    fn matches(&self, cursor: &CursorRow) -> bool {
        let mtime_match = (self.mtime_nanos > 0 && cursor.last_modified == self.mtime_nanos)
            || (self.mtime_millis > 0 && cursor.last_modified == self.mtime_millis)
            || (self.mtime_secs > 0 && cursor.last_modified == self.mtime_secs);
        if !mtime_match {
            return false;
        }
        // If `last_byte_offset` was recorded, verify byte length too (Windows mtime lag guard).
        match cursor.last_byte_offset {
            Some(saved_bytes) => saved_bytes == self.byte_size,
            None => true,
        }
    }
}

fn load_cursors(conn: &Connection) -> HashMap<String, CursorRow> {
    let mut map = HashMap::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT file_path, last_modified, COALESCE(last_line_offset, 0), last_byte_offset
         FROM session_log_sync",
    ) {
        if let Ok(rows) = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                CursorRow {
                    last_modified: r.get(1)?,
                    last_line_offset: r.get(2)?,
                    last_byte_offset: r.get(3)?,
                },
            ))
        }) {
            for (k, v) in rows.flatten() {
                map.insert(k, v);
            }
        }
    }
    map
}

fn save_cursor(conn: &Connection, key: &str, stamp: FileStamp, line_offset: i64) {
    let now_sec = Local::now().timestamp();
    let _ = conn.prepare_cached(
        "INSERT INTO session_log_sync
            (file_path, last_modified, last_line_offset, last_synced_at, last_byte_offset)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(file_path) DO UPDATE SET
            last_modified = excluded.last_modified,
            last_line_offset = excluded.last_line_offset,
            last_synced_at = excluded.last_synced_at,
            last_byte_offset = excluded.last_byte_offset",
    )
    .and_then(|mut s| {
        s.execute(params![
            key,
            stamp.mtime_millis,
            line_offset,
            now_sec,
            stamp.byte_size
        ])
    });
}

// ---------------------------------------------------------------------------
// Orchestrator
// ---------------------------------------------------------------------------

fn begin_tx_once(conn: &Connection, in_tx: &mut bool) {
    if !*in_tx && conn.execute_batch("BEGIN IMMEDIATE TRANSACTION;").is_ok() {
        *in_tx = true;
    }
}

pub(crate) fn sync_all(conn: &Connection, paths: &Paths) -> Result<SessionSyncResult, String> {
    let mut report = SessionSyncResult::default();
    let pricing = PricingTable::load(conn);
    let cursors = load_cursors(conn);
    let mut in_tx = false;

    // 1. Pi (~/.pi/agent/sessions/**/*.jsonl) & OhMyPi (~/.omp/agent/sessions/**/*.jsonl)
    for (tool, app_type, req_prefix, fallback_provider, dedup_ns) in [
        (ToolId::Pi, "pi", "pi_session", "_pi_session", "pi_session"),
        (
            ToolId::OhMyPi,
            "oh_my_pi",
            "omp_session",
            "_omp_session",
            "omp_session",
        ),
    ] {
        let sessions_dir = paths.tool_root(tool).join("sessions");
        let files = collect_files(
            &sessions_dir,
            &["jsonl"],
            &["context-fold", "subagents", "tool-results", "memory", "subagent-artifacts"],
            true,
        );
        // Process non-fork or older sessions first so parent semantic keys exist when forks run
        let mut sorted = files;
        sorted.sort();
        for path in sorted {
            sync_file_step(
                conn,
                &cursors,
                &mut in_tx,
                &path,
                None,
                &mut report,
                |cursor, stamp, rep| {
                    ingest_pi_compatible(
                        conn,
                        &pricing,
                        &path,
                        app_type,
                        req_prefix,
                        fallback_provider,
                        dedup_ns,
                        cursor,
                        stamp,
                        rep,
                    )
                },
            );
        }
    }

    // 2. Claude Code (~/.claude/projects/**/*.jsonl, including subagents & workflows)
    let claude_projects = paths.home.join(".claude").join("projects");
    for path in collect_files(
        &claude_projects,
        &["jsonl"],
        &["tool-results", "memory"],
        false,
    ) {
        sync_file_step(conn, &cursors, &mut in_tx, &path, None, &mut report, |_, _, rep| {
            ingest_claude_jsonl(
                conn,
                &pricing,
                &path,
                "claude",
                "session",
                Some("claude"),
                "_session",
                "session_log",
                rep,
            )
        });
    }

    // 3. Claude Desktop (projects & local-agent-mode-sessions JSONL)
    for cd_root in claude_desktop_roots(paths) {
        for sub in ["projects", "local-agent-mode-sessions"] {
            let dir = cd_root.join(sub);
            for path in collect_files(&dir, &["jsonl"], &["tool-results"], false) {
                sync_file_step(conn, &cursors, &mut in_tx, &path, None, &mut report, |_, _, rep| {
                    ingest_claude_jsonl(
                        conn,
                        &pricing,
                        &path,
                        "claude_desktop",
                        "claude_desktop",
                        None,
                        "_claude_desktop_session",
                        "claude_desktop_session",
                        rep,
                    )
                });
            }
        }
    }

    // 4. Codex (~/.codex/sessions & ~/.codex/archived_sessions)
    let mut codex_files = Vec::new();
    for sub in ["sessions", "archived_sessions"] {
        codex_files.extend(collect_files(
            &paths.tool_root(ToolId::Codex).join(sub),
            &["jsonl"],
            &[],
            true,
        ));
    }
    codex_files.sort();
    let rollout_index = build_codex_rollout_index(&codex_files);
    for path in &codex_files {
        sync_file_step(conn, &cursors, &mut in_tx, path, None, &mut report, |_, _, rep| {
            ingest_codex_jsonl(conn, &pricing, path, &rollout_index, rep)
        });
    }

    // 5. Antigravity (~/.gemini/antigravity*{,/brain/*/.system_generated/logs/transcript.jsonl})
    let default_agy_model = read_default_agy_model(&paths.home);
    for sub in ["antigravity-cli", "antigravity-ide", "antigravity"] {
        let brain = paths.home.join(".gemini").join(sub).join("brain");
        if let Ok(entries) = fs::read_dir(&brain) {
            for entry in entries.flatten() {
                let logs_dir = entry.path().join(".system_generated").join("logs");
                let primary = logs_dir.join("transcript.jsonl");
                let fallback = logs_dir.join("transcript_full.jsonl");
                let target = if primary.is_file() {
                    primary
                } else if fallback.is_file() {
                    fallback
                } else {
                    continue;
                };
                sync_file_step(
                    conn,
                    &cursors,
                    &mut in_tx,
                    &target,
                    None,
                    &mut report,
                    |_, _, rep| {
                        ingest_antigravity_jsonl(
                            conn,
                            &pricing,
                            &target,
                            &default_agy_model,
                            rep,
                        )
                    },
                );
            }
        }
    }

    // 6. Gemini CLI (~/.gemini/tmp/*/chats/session-*.{json,jsonl})
    let gemini_tmp = paths.home.join(".gemini").join("tmp");
    if let Ok(projects) = fs::read_dir(&gemini_tmp) {
        for proj in projects.flatten() {
            let chats = proj.path().join("chats");
            if !chats.is_dir() {
                continue;
            }
            for path in collect_files(&chats, &["json", "jsonl"], &[], true) {
                let is_session = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("session-"));
                if !is_session {
                    continue;
                }
                sync_file_step(conn, &cursors, &mut in_tx, &path, None, &mut report, |_, _, rep| {
                    ingest_gemini_session(conn, &pricing, &path, rep)
                });
            }
        }
    }

    // 7. OpenCode (SQLite opencode.db V1/V2 + legacy storage/message JSON fallback)
    let mut found_opencode_db = false;
    for db_path in opencode_db_candidates(paths) {
        if db_path.is_file() {
            found_opencode_db = true;
            let stamp = FileStamp::from_sqlite_path(&db_path);
            let key = db_path.to_string_lossy().to_string();
            report.files_scanned += 1;
            if cursors.get(&key).is_some_and(|c| stamp.matches(c)) {
                continue;
            }
            begin_tx_once(conn, &mut in_tx);
            let rows = ingest_opencode_db(conn, &pricing, &db_path, &mut report);
            save_cursor(conn, &key, stamp, rows);
        }
    }
    if !found_opencode_db {
        for dir in opencode_storage_candidates(paths) {
            let msg_dir = dir.join("storage").join("message");
            for path in collect_files(&msg_dir, &["json"], &[], true) {
                sync_file_step(conn, &cursors, &mut in_tx, &path, None, &mut report, |_, _, rep| {
                    ingest_opencode_message_json(conn, &pricing, &path, rep)
                });
            }
        }
    }

    // 8. Grok (~/.grok/{sessions,archived_sessions}/**/updates.jsonl, skip subagents)
    let grok_root = paths.tool_root(ToolId::Grok);
    for sub in ["sessions", "archived_sessions"] {
        for path in collect_files(&grok_root.join(sub), &["jsonl"], &["subagents"], true) {
            if path.file_name().and_then(|n| n.to_str()) == Some("updates.jsonl") {
                sync_file_step(conn, &cursors, &mut in_tx, &path, None, &mut report, |_, _, rep| {
                    ingest_grok_updates(conn, &pricing, &path, rep)
                });
            }
        }
    }

    // 9. Kimi (~/.kimi/sessions/**/wire.jsonl & ~/.kimi-code/{wire.jsonl,trees/*.jsonl,sessions/**/*.jsonl})
    for kimi_dir in [
        paths.home.join(".kimi").join("sessions"),
        paths.tool_root(ToolId::Kimi),
    ] {
        for path in collect_files(&kimi_dir, &["jsonl"], &["subagents"], true) {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let parent_name = path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("");
            let in_trees = parent_name == "trees";
            let in_sessions = path.components().any(|c| c.as_os_str() == "sessions");
            if name == "wire.jsonl" || in_trees || in_sessions {
                sync_file_step(conn, &cursors, &mut in_tx, &path, None, &mut report, |_, _, rep| {
                    ingest_kimi_jsonl(conn, &pricing, &path, rep)
                });
            }
        }
    }

    // 10. OpenClaw (~/.openclaw/agents/*/sessions/*.jsonl & ~/.openclaw/openclaw-agent.sqlite)
    let openclaw_root = paths.tool_root(ToolId::OpenClaw);
    for path in collect_files(&openclaw_root.join("agents"), &["jsonl"], &["subagents"], true) {
        if path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            == Some("sessions")
        {
            sync_file_step(conn, &cursors, &mut in_tx, &path, None, &mut report, |_, _, rep| {
                ingest_openclaw_jsonl(conn, &pricing, &path, rep)
            });
        }
    }
    let openclaw_db = openclaw_root.join("openclaw-agent.sqlite");
    if openclaw_db.is_file() {
        let stamp = FileStamp::from_sqlite_path(&openclaw_db);
        let key = openclaw_db.to_string_lossy().to_string();
        report.files_scanned += 1;
        if !cursors.get(&key).is_some_and(|c| stamp.matches(c)) {
            begin_tx_once(conn, &mut in_tx);
            let rows = ingest_openclaw_sqlite(conn, &pricing, &openclaw_db, &mut report);
            save_cursor(conn, &key, stamp, rows);
        }
    }

    // 11. Hermes (~/.hermes/state.db)
    let hermes_db = paths.tool_root(ToolId::Hermes).join("state.db");
    if hermes_db.is_file() {
        let stamp = FileStamp::from_sqlite_path(&hermes_db);
        let key = hermes_db.to_string_lossy().to_string();
        report.files_scanned += 1;
        if !cursors.get(&key).is_some_and(|c| stamp.matches(c)) {
            begin_tx_once(conn, &mut in_tx);
            let rows = ingest_hermes_db(conn, &pricing, &hermes_db, &mut report);
            save_cursor(conn, &key, stamp, rows);
        }
    }

    // 12. Dsh (~/.dsh/sessions/**/*.jsonl)
    for path in collect_files(
        &paths.tool_root(ToolId::Dsh).join("sessions"),
        &["jsonl"],
        &[],
        true,
    ) {
        sync_file_step(conn, &cursors, &mut in_tx, &path, None, &mut report, |_, _, rep| {
            ingest_dsh_jsonl(conn, &pricing, &path, rep)
        });
    }

    if in_tx {
        let _ = conn.execute_batch("COMMIT;");
    }

    Ok(report)
}

fn sync_file_step<F>(
    conn: &Connection,
    cursors: &HashMap<String, CursorRow>,
    in_tx: &mut bool,
    path: &Path,
    override_stamp: Option<FileStamp>,
    report: &mut SessionSyncResult,
    mut f: F,
) where
    F: FnMut(Option<CursorRow>, FileStamp, &mut SessionSyncResult) -> i64,
{
    report.files_scanned += 1;
    let stamp = override_stamp.unwrap_or_else(|| FileStamp::from_path(path));
    let key = path.to_string_lossy().to_string();
    let cursor = cursors.get(&key).copied();
    if cursor.is_some_and(|c| stamp.matches(&c)) {
        return;
    }
    begin_tx_once(conn, in_tx);
    let line_offset = f(cursor, stamp, report);
    save_cursor(conn, &key, stamp, line_offset);
}

// ---------------------------------------------------------------------------
// 1. Pi & OhMyPi
// ---------------------------------------------------------------------------

fn hash_field(h: &mut Sha256, bytes: &[u8]) {
    h.update((bytes.len() as u64).to_be_bytes());
    h.update(bytes);
}

fn hash_json(h: &mut Sha256, val: &Value) {
    match val {
        Value::Null => hash_field(h, b"null"),
        Value::Bool(b) => {
            hash_field(h, b"bool");
            hash_field(h, if *b { b"true" } else { b"false" });
        }
        Value::Number(n) => {
            hash_field(h, b"number");
            hash_field(h, n.to_string().as_bytes());
        }
        Value::String(s) => {
            hash_field(h, b"string");
            hash_field(h, s.as_bytes());
        }
        Value::Array(arr) => {
            hash_field(h, b"array");
            h.update((arr.len() as u64).to_be_bytes());
            for item in arr {
                hash_json(h, item);
            }
        }
        Value::Object(map) => {
            hash_field(h, b"object");
            let mut entries: Vec<(&str, &Value)> =
                map.iter().map(|(k, v)| (k.as_str(), v)).collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            h.update((entries.len() as u64).to_be_bytes());
            for (k, v) in entries {
                hash_field(h, k.as_bytes());
                hash_json(h, v);
            }
        }
    }
}

/// Exact bit-for-bit `pi-session-request-v3` SHA-256 request ID (matches cc-switch).
fn pi_request_identity(
    req_prefix: &str,
    kind: &str,
    entry: &Value,
    session_id: &str,
    line_num: i64,
    model: &str,
    created_at: i64,
    inp: u32,
    out: u32,
    cr: u32,
    cw: u32,
) -> (String, String, bool) {
    let entry_id = nonempty_str(entry.get("id"));
    let req_id = if let Some(eid) = entry_id {
        let mut h = Sha256::new();
        let tag = if req_prefix == "pi_session" {
            b"pi-session-request-v3".as_slice()
        } else {
            b"omp-session-request-v1".as_slice()
        };
        hash_field(&mut h, tag);
        hash_field(&mut h, kind.as_bytes());
        hash_field(&mut h, eid.as_bytes());
        hash_json(&mut h, entry.get("timestamp").unwrap_or(&Value::Null));
        format!("{req_prefix}:{:x}", h.finalize())
    } else {
        let mut h = Sha256::new();
        hash_field(&mut h, b"pi-session-line-v1");
        hash_field(&mut h, req_prefix.as_bytes());
        hash_field(&mut h, session_id.as_bytes());
        hash_field(&mut h, &line_num.to_be_bytes());
        hash_field(&mut h, kind.as_bytes());
        format!("{req_prefix}:{:x}", h.finalize())
    };

    let mut sem = Sha256::new();
    hash_field(&mut sem, b"pi-session-semantic-v1");
    hash_field(&mut sem, kind.as_bytes());
    hash_field(&mut sem, model.as_bytes());
    hash_field(&mut sem, &created_at.to_be_bytes());
    hash_field(&mut sem, &inp.to_be_bytes());
    hash_field(&mut sem, &out.to_be_bytes());
    hash_field(&mut sem, &cr.to_be_bytes());
    hash_field(&mut sem, &cw.to_be_bytes());
    let semantic_id = format!("{req_prefix}_semantic:{:x}", sem.finalize());

    (req_id, semantic_id, entry_id.is_some())
}

#[allow(clippy::too_many_arguments)]
fn ingest_pi_compatible(
    conn: &Connection,
    pricing: &PricingTable,
    path: &Path,
    app_type: &str,
    req_prefix: &'static str,
    fallback_provider: &str,
    dedup_ns: &'static str,
    cursor: Option<CursorRow>,
    stamp: FileStamp,
    report: &mut SessionSyncResult,
) -> i64 {
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return 0,
    };

    // Determine if we can seek directly to appended bytes.
    // Wait until header on line 1 is parsed, then seek if previous cursor ended cleanly at newline.
    let seek_offset = cursor
        .and_then(|c| c.last_byte_offset)
        .filter(|&b| b > 0 && b < stamp.byte_size)
        .and_then(|b| {
            let mut probe = [0u8; 1];
            if file.seek(SeekFrom::Start((b - 1) as u64)).is_ok()
                && file.read_exact(&mut probe).is_ok()
                && probe[0] == b'\n'
            {
                Some((b as u64, cursor.map(|c| c.last_line_offset).unwrap_or(0)))
            } else {
                None
            }
        });

    let _ = file.seek(SeekFrom::Start(0));
    let mut reader = BufReader::new(file);
    let mut line_num: i64 = 0;
    let mut session_id: Option<String> = None;
    let mut session_ts: Option<i64> = None;
    let mut parent_cutoffs: HashSet<String> = HashSet::new();
    let mut tracked_model: Option<String> = None;
    let mut tracked_provider: Option<String> = None;
    let file_ts = if stamp.mtime_secs > 0 {
        stamp.mtime_secs
    } else {
        Local::now().timestamp()
    };

    let mut buf = String::new();
    let mut skipped_to_tail = false;

    loop {
        buf.clear();
        let n = match reader.read_line(&mut buf) {
            Ok(n) => n,
            Err(_) => break,
        };
        if n == 0 {
            break;
        }
        line_num += 1;
        let trimmed = buf.trim();
        if trimmed.is_empty() {
            continue;
        }
        let val: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if session_id.is_none() {
            if val.get("type").and_then(Value::as_str) != Some("session") {
                return line_num;
            }
            session_id = nonempty_str(val.get("id")).map(str::to_string);
            let Some(ref sid) = session_id else {
                return line_num;
            };
            session_ts = val.get("timestamp").and_then(parse_any_timestamp);
            if let Some(parent_path_str) = nonempty_str(val.get("parentSession")) {
                if let Some(resolved) = resolve_pi_parent(path, parent_path_str) {
                    let cached = pricing.pi_parent_cache.borrow().get(&resolved).cloned();
                    parent_cutoffs = if let Some(hit) = cached {
                        hit
                    } else {
                        let mut computed = HashSet::new();
                        collect_pi_parent_semantics(&resolved, req_prefix, &mut computed);
                        pricing
                            .pi_parent_cache
                            .borrow_mut()
                            .insert(resolved, computed.clone());
                        computed
                    };
                }
            }
            if let Some((offset, prev_lines)) = seek_offset {
                if !skipped_to_tail {
                    skipped_to_tail = true;
                    // Also check header parentSession only; model_change tracking falls back to msg fields on delta
                    if reader.seek(SeekFrom::Start(offset)).is_ok() {
                        line_num = prev_lines.max(1);
                        let _ = sid;
                        continue;
                    }
                }
            }
            continue;
        }

        let etype = val.get("type").and_then(Value::as_str).unwrap_or("");
        if etype == "model_change" {
            if let Some(m) = nonempty_str(val.get("modelId")).or_else(|| nonempty_str(val.get("model"))) {
                tracked_model = Some(m.to_string());
            }
            if let Some(p) = nonempty_str(val.get("provider")) {
                tracked_provider = Some(p.to_string());
            }
            continue;
        }

        let (kind, usage_val, msg_opt) = match etype {
            "message" => {
                let Some(msg) = val.get("message") else {
                    continue;
                };
                match msg.get("role").and_then(Value::as_str) {
                    Some("assistant") => ("assistant", msg.get("usage"), Some(msg)),
                    Some("toolResult") => {
                        let tool_name = msg
                            .get("toolName")
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        if matches!(tool_name, "task" | "subagent") || msg.get("usage").is_some() {
                            ("tool_result", msg.get("usage"), Some(msg))
                        } else {
                            continue;
                        }
                    }
                    _ => continue,
                }
            }
            "compaction" => ("compaction", val.get("usage"), None),
            "branch_summary" => ("branch_summary", val.get("usage"), None),
            _ => continue,
        };

        let Some(usage) = usage_val else {
            continue;
        };

        let inp = get_u32(usage, &["input", "input_tokens"]);
        let out = get_u32(usage, &["output", "output_tokens"]);
        let cr = get_u32(usage, &["cacheRead", "cache_read_input_tokens"]);
        let cw = get_u32(usage, &["cacheWrite", "cache_creation_input_tokens"]);

        let cost_obj = usage.get("cost");
        let exp_total = cost_obj
            .and_then(|c| c.get("total"))
            .and_then(parse_f64)
            .unwrap_or(0.0);
        let explicit_cost = if exp_total > 0.0 {
            Some(CostBreakdown {
                input: cost_obj.and_then(|c| c.get("input")).and_then(parse_f64).unwrap_or(0.0),
                output: cost_obj.and_then(|c| c.get("output")).and_then(parse_f64).unwrap_or(0.0),
                cache_read: cost_obj.and_then(|c| c.get("cacheRead")).and_then(parse_f64).unwrap_or(0.0),
                cache_creation: cost_obj.and_then(|c| c.get("cacheWrite")).and_then(parse_f64).unwrap_or(0.0),
                total: exp_total,
            })
        } else {
            None
        };

        let stop_reason = msg_opt.and_then(|m| nonempty_str(m.get("stopReason")));
        let (status_code, error_message) = if kind == "assistant" {
            match stop_reason {
                Some("aborted") => (
                    499,
                    Some(
                        msg_opt
                            .and_then(|m| nonempty_str(m.get("errorMessage")))
                            .unwrap_or("Pi request aborted")
                            .chars()
                            .take(4096)
                            .collect(),
                    ),
                ),
                Some("error") => (
                    500,
                    Some(
                        msg_opt
                            .and_then(|m| nonempty_str(m.get("errorMessage")))
                            .unwrap_or("Pi request failed")
                            .chars()
                            .take(4096)
                            .collect(),
                    ),
                ),
                _ => (200, None),
            }
        } else {
            (200, None)
        };

        // Skip zero-usage successful placeholders; retain errored/aborted entries only if usage exists
        if inp == 0 && out == 0 && cr == 0 && cw == 0 && explicit_cost.is_none() && status_code == 200 {
            continue;
        }

        let provider_id = msg_opt
            .and_then(|m| nonempty_str(m.get("provider")))
            .or(tracked_provider.as_deref())
            .unwrap_or(fallback_provider)
            .to_string();
        let model = msg_opt
            .and_then(|m| nonempty_str(m.get("model")))
            .or(tracked_model.as_deref())
            .unwrap_or("unknown")
            .to_string();
        let request_model = msg_opt
            .and_then(|m| {
                nonempty_str(m.get("responseModel"))
                    .or_else(|| m.get("response").and_then(|r| nonempty_str(r.get("model"))))
            })
            .unwrap_or(&model)
            .to_string();

        let created_at = val
            .get("timestamp")
            .and_then(parse_any_timestamp)
            .or_else(|| msg_opt.and_then(|m| m.get("timestamp").and_then(parse_any_timestamp)))
            .or(session_ts)
            .unwrap_or(file_ts);

        let sid = session_id.as_deref().unwrap_or("");
        let (request_id, semantic_id, has_entry_id) = pi_request_identity(
            req_prefix, kind, &val, sid, line_num, &model, created_at, inp, out, cr, cw,
        );

        if parent_cutoffs.contains(&semantic_id) || parent_cutoffs.contains(&request_id) {
            report.skipped += 1;
            continue;
        }

        let rec = UsageRecord {
            request_id,
            alternate_request_ids: Vec::new(),
            dedup_namespace: Some(dedup_ns),
            semantic_id: Some(semantic_id),
            has_entry_id,
            provider_id,
            app_type: app_type.to_string(),
            model,
            request_model,
            fresh_input_tokens: inp,
            output_tokens: out,
            cache_read_tokens: cr,
            cache_creation_tokens: cw,
            cache_creation_1h_tokens: 0,
            explicit_cost,
            status_code,
            error_message,
            session_id: session_id.clone(),
            provider_type: req_prefix,
            data_source: req_prefix,
            is_streaming: true,
            created_at,
            update_if_higher_output: false,
        };
        let _ = insert_or_update_record(conn, pricing, &rec, report);
    }

    line_num
}

fn resolve_pi_parent(child: &Path, raw_parent: &str) -> Option<PathBuf> {
    let p = PathBuf::from(raw_parent);
    if p.is_file() {
        return Some(p);
    }
    let fname = p.file_name()?;
    let same_dir = child.parent()?.join(fname);
    if same_dir.is_file() {
        return Some(same_dir);
    }
    None
}

fn collect_pi_parent_semantics(parent: &Path, req_prefix: &str, out: &mut HashSet<String>) {
    let Ok(file) = File::open(parent) else {
        return;
    };
    let reader = BufReader::new(file);
    let mut line_num: i64 = 0;
    let mut session_id = String::new();
    let mut session_ts: Option<i64> = None;
    for line in reader.lines().map_while(Result::ok) {
        line_num += 1;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(val) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        let etype = val.get("type").and_then(Value::as_str).unwrap_or("");
        if line_num == 1 && etype == "session" {
            session_id = nonempty_str(val.get("id")).unwrap_or("").to_string();
            session_ts = val.get("timestamp").and_then(parse_any_timestamp);
            continue;
        }
        let (kind, usage_val, msg_opt) = match etype {
            "message" => {
                let Some(msg) = val.get("message") else {
                    continue;
                };
                match msg.get("role").and_then(Value::as_str) {
                    Some("assistant") => ("assistant", msg.get("usage"), Some(msg)),
                    Some("toolResult") => ("tool_result", msg.get("usage"), Some(msg)),
                    _ => continue,
                }
            }
            "compaction" => ("compaction", val.get("usage"), None),
            "branch_summary" => ("branch_summary", val.get("usage"), None),
            _ => continue,
        };
        let Some(usage) = usage_val else {
            continue;
        };
        let inp = get_u32(usage, &["input", "input_tokens"]);
        let out_t = get_u32(usage, &["output", "output_tokens"]);
        let cr = get_u32(usage, &["cacheRead", "cache_read_input_tokens"]);
        let cw = get_u32(usage, &["cacheWrite", "cache_creation_input_tokens"]);
        let model = msg_opt
            .and_then(|m| nonempty_str(m.get("model")))
            .unwrap_or("unknown");
        let ts = val
            .get("timestamp")
            .and_then(parse_any_timestamp)
            .or(session_ts)
            .unwrap_or(0);
        let (rid, sid_hash, _) = pi_request_identity(
            req_prefix, kind, &val, &session_id, line_num, model, ts, inp, out_t, cr, cw,
        );
        out.insert(rid);
        out.insert(sid_hash);
    }
}

// ---------------------------------------------------------------------------
// 2. Claude Code & Claude Desktop
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn ingest_claude_jsonl(
    conn: &Connection,
    pricing: &PricingTable,
    path: &Path,
    app_type: &str,
    req_prefix: &str,
    alt_prefix: Option<&str>,
    provider_id: &str,
    data_source: &'static str,
    report: &mut SessionSyncResult,
) -> i64 {
    let Ok(file) = File::open(path) else {
        return 0;
    };
    let reader = BufReader::new(file);

    struct AggMsg {
        key: String,
        alt_key: Option<String>,
        model: String,
        session_id: Option<String>,
        inp: u32,
        out: u32,
        cr: u32,
        cc: u32,
        cc_1h: u32,
        created_at: i64,
        has_stop_reason: bool,
    }

    let mut order: Vec<String> = Vec::new();
    let mut by_key: HashMap<String, AggMsg> = HashMap::new();
    let mut line_num: i64 = 0;

    for line in reader.lines().map_while(Result::ok) {
        line_num += 1;
        if !line.contains("\"assistant\"") || !line.contains("\"usage\"") {
            continue;
        }
        let Ok(val) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if val.get("type").and_then(Value::as_str) != Some("assistant") {
            continue;
        }
        if val.get("isApiErrorMessage").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let Some(msg) = val.get("message") else {
            continue;
        };
        let model = nonempty_str(msg.get("model")).unwrap_or("");
        if model.is_empty() || model == "<synthetic>" {
            continue;
        }
        let Some(usage) = msg.get("usage") else {
            continue;
        };

        let inp = get_u32(usage, &["input_tokens"]);
        let out = get_u32(usage, &["output_tokens"]);
        let cr = get_u32(usage, &["cache_read_input_tokens"]);
        let cc = get_u32(usage, &["cache_creation_input_tokens"]);
        let cc_1h = usage
            .get("cache_creation")
            .and_then(|c| c.get("ephemeral_1h_input_tokens"))
            .and_then(Value::as_u64)
            .unwrap_or(0)
            .min(u32::MAX as u64) as u32;

        if inp == 0 && out == 0 && cr == 0 && cc == 0 {
            continue;
        }

        let raw_msg_id = nonempty_str(msg.get("id"));
        let key = if let Some(mid) = raw_msg_id {
            format!("{req_prefix}:{mid}")
        } else {
            format!("{req_prefix}:{}", sha256_str(line.as_bytes()))
        };
        let alt_key = raw_msg_id.and_then(|mid| alt_prefix.map(|ap| format!("{ap}:{mid}")));

        let session_id = nonempty_str(val.get("sessionId")).map(str::to_string);
        let created_at = val
            .get("timestamp")
            .and_then(parse_any_timestamp)
            .unwrap_or_else(|| Local::now().timestamp());
        let has_stop = msg.get("stop_reason").is_some_and(|v| !v.is_null());

        if let Some(existing) = by_key.get_mut(&key) {
            // Prefer final chunk (`stop_reason` set) or strictly higher `output_tokens`
            let replace = (!existing.has_stop_reason && has_stop)
                || (has_stop == existing.has_stop_reason && out >= existing.out);
            if replace {
                existing.inp = inp;
                existing.out = out;
                existing.cr = cr;
                existing.cc = cc;
                existing.cc_1h = cc_1h;
                existing.model = model.to_string();
                existing.has_stop_reason = has_stop;
                existing.created_at = created_at;
                if existing.session_id.is_none() {
                    existing.session_id = session_id;
                }
            }
        } else {
            order.push(key.clone());
            by_key.insert(
                key.clone(),
                AggMsg {
                    key,
                    alt_key,
                    model: model.to_string(),
                    session_id,
                    inp,
                    out,
                    cr,
                    cc,
                    cc_1h,
                    created_at,
                    has_stop_reason: has_stop,
                },
            );
        }
    }

    for k in order {
        let Some(m) = by_key.remove(&k) else {
            continue;
        };
        let rec = UsageRecord {
            request_id: m.key,
            alternate_request_ids: m.alt_key.into_iter().collect(),
            dedup_namespace: None,
            semantic_id: None,
            has_entry_id: true,
            provider_id: provider_id.to_string(),
            app_type: app_type.to_string(),
            model: m.model.clone(),
            request_model: m.model,
            fresh_input_tokens: m.inp,
            output_tokens: m.out,
            cache_read_tokens: m.cr,
            cache_creation_tokens: m.cc,
            cache_creation_1h_tokens: m.cc_1h,
            explicit_cost: None,
            status_code: 200,
            error_message: None,
            session_id: m.session_id,
            provider_type: data_source,
            data_source,
            is_streaming: true,
            created_at: m.created_at,
            update_if_higher_output: true,
        };
        let _ = insert_or_update_record(conn, pricing, &rec, report);
    }

    line_num
}

fn claude_desktop_roots(paths: &Paths) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    roots.push(paths.tool_root(ToolId::ClaudeDesktop));
    if let Ok(appdata) = std::env::var("APPDATA") {
        let p = PathBuf::from(appdata).join("Claude");
        if !roots.contains(&p) {
            roots.push(p);
        }
    }
    #[cfg(target_os = "macos")]
    {
        let p = paths
            .home
            .join("Library")
            .join("Application Support")
            .join("Claude");
        if !roots.contains(&p) {
            roots.push(p);
        }
    }
    let cfg = paths.home.join(".config").join("Claude");
    if !roots.contains(&cfg) {
        roots.push(cfg);
    }
    roots
}

// ---------------------------------------------------------------------------
// 3. Codex
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CodexSnap {
    input: u32,
    cached_input: u32,
    cache_write_input: u32,
    output: u32,
    reasoning_output: u32,
}

impl CodexSnap {
    fn from_value(v: &Value) -> Option<Self> {
        let obj = v.as_object()?;
        if obj.is_empty() {
            return None;
        }
        Some(Self {
            input: get_u32(v, &["input_tokens", "prompt_tokens"]),
            cached_input: get_u32(v, &["cached_input_tokens"]),
            cache_write_input: get_u32(v, &["cache_write_input_tokens"]),
            output: get_u32(v, &["output_tokens", "completion_tokens"]),
            reasoning_output: get_u32(v, &["reasoning_output_tokens"]),
        })
    }

    fn sub_sat(self, prev: Self) -> Self {
        Self {
            input: self.input.saturating_sub(prev.input),
            cached_input: self.cached_input.saturating_sub(prev.cached_input),
            cache_write_input: self.cache_write_input.saturating_sub(prev.cache_write_input),
            output: self.output.saturating_sub(prev.output),
            reasoning_output: self.reasoning_output.saturating_sub(prev.reasoning_output),
        }
    }

    fn is_zero(self) -> bool {
        self.input == 0
            && self.cached_input == 0
            && self.cache_write_input == 0
            && self.output == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CodexTokenSig {
    total: Option<CodexSnap>,
    last: Option<CodexSnap>,
}

struct CodexTokenEvent {
    event_index: u32,
    delta: CodexSnap,
    sig: CodexTokenSig,
    model: String,
    created_at: i64,
}

fn build_codex_rollout_index(files: &[PathBuf]) -> HashMap<String, Vec<PathBuf>> {
    let mut index: HashMap<String, Vec<PathBuf>> = HashMap::new();
    for f in files {
        if let Some(uuid) = extract_trailing_uuid(f) {
            index.entry(uuid).or_default().push(f.clone());
        }
    }
    index
}

fn extract_trailing_uuid(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    // Codex names rollouts `rollout-2026-02-02T10-00-00-<uuid>.jsonl` (36-char UUID)
    if stem.len() >= 36 {
        let candidate = &stem[stem.len() - 36..];
        let parts: Vec<&str> = candidate.split('-').collect();
        if parts.len() == 5
            && parts[0].len() == 8
            && parts[1].len() == 4
            && parts[2].len() == 4
            && parts[3].len() == 4
            && parts[4].len() == 12
            && candidate.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
        {
            return Some(candidate.to_ascii_lowercase());
        }
    }
    None
}

fn ingest_codex_jsonl(
    conn: &Connection,
    pricing: &PricingTable,
    path: &Path,
    rollout_index: &HashMap<String, Vec<PathBuf>>,
    report: &mut SessionSyncResult,
) -> i64 {
    let Ok(file) = File::open(path) else {
        return 0;
    };
    let reader = BufReader::new(file);

    let mut current_model = "gpt-5-codex".to_string();
    let mut session_id: Option<String> = None;
    let mut forked_from_id: Option<String> = None;
    let mut root_ts: Option<i64> = None;
    let mut prev_total: Option<CodexSnap> = None;
    let mut last_sig: Option<CodexTokenSig> = None;
    let mut events: Vec<CodexTokenEvent> = Vec::new();
    let mut fallback_records: Vec<UsageRecord> = Vec::new();
    let mut line_num: i64 = 0;

    for line in reader.lines().map_while(Result::ok) {
        line_num += 1;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(val) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        let entry_type = val.get("type").and_then(Value::as_str).unwrap_or("");
        let created_at = val
            .get("timestamp")
            .and_then(parse_any_timestamp)
            .or_else(|| val.get("created_at").and_then(parse_any_timestamp))
            .unwrap_or_else(|| Local::now().timestamp());

        if entry_type == "session_meta" {
            if let Some(payload) = val.get("payload") {
                if session_id.is_none() {
                    session_id = nonempty_str(payload.get("id")).map(str::to_string);
                    forked_from_id =
                        nonempty_str(payload.get("forked_from_id")).map(|s| s.to_ascii_lowercase());
                    root_ts = val
                        .get("timestamp")
                        .and_then(parse_any_timestamp)
                        .or_else(|| payload.get("timestamp").and_then(parse_any_timestamp));
                }
            }
            continue;
        }

        if entry_type == "turn_context" {
            if let Some(m) = val
                .get("payload")
                .and_then(|p| nonempty_str(p.get("model")))
            {
                current_model = m.to_string();
            }
            continue;
        }

        if entry_type == "event_msg" {
            if let Some(payload) = val.get("payload") {
                if let Some(m) = nonempty_str(payload.get("model")) {
                    current_model = m.to_string();
                }
                if payload.get("type").and_then(Value::as_str) == Some("token_count") {
                    if let Some(info) = payload.get("info").filter(|i| !i.is_null()) {
                        if let Some(m) = nonempty_str(info.get("model"))
                            .or_else(|| nonempty_str(info.get("model_name")))
                        {
                            current_model = m.to_string();
                        }
                        let total_opt = info
                            .get("total_token_usage")
                            .and_then(CodexSnap::from_value);
                        let last_opt = info
                            .get("last_token_usage")
                            .and_then(CodexSnap::from_value);

                        if total_opt.is_none() && last_opt.is_none() {
                            continue;
                        }
                        let sig = CodexTokenSig {
                            total: total_opt,
                            last: last_opt,
                        };
                        if last_sig == Some(sig) {
                            continue;
                        }
                        last_sig = Some(sig);

                        let delta = match (total_opt, last_opt) {
                            (Some(tot), Some(last)) => {
                                let d = match prev_total {
                                    Some(prev)
                                        if tot.input >= prev.input
                                            && tot.output >= prev.output =>
                                    {
                                        tot.sub_sat(prev)
                                    }
                                    Some(_) => last,
                                    None => {
                                        if !last.is_zero()
                                            && last.input <= tot.input
                                            && last.output <= tot.output
                                        {
                                            last
                                        } else {
                                            tot
                                        }
                                    }
                                };
                                prev_total = Some(tot);
                                d
                            }
                            (Some(tot), None) => {
                                let d = match prev_total {
                                    Some(prev)
                                        if tot.input >= prev.input
                                            && tot.output >= prev.output =>
                                    {
                                        tot.sub_sat(prev)
                                    }
                                    _ => tot,
                                };
                                prev_total = Some(tot);
                                d
                            }
                            (None, Some(last)) => last,
                            (None, None) => continue,
                        };

                        if delta.is_zero() {
                            continue;
                        }
                        let event_index = (events.len() + 1) as u32;
                        events.push(CodexTokenEvent {
                            event_index,
                            delta,
                            sig,
                            model: current_model.clone(),
                            created_at,
                        });
                    }
                }
            }
            continue;
        }

        // Fallback generic OpenAI-style usage JSON line
        if let Some(usage) = val.get("usage") {
            let raw_inp = get_u32(usage, &["prompt_tokens", "input_tokens"]);
            let out = get_u32(usage, &["completion_tokens", "output_tokens"]);
            let cr = usage
                .get("prompt_tokens_details")
                .and_then(|d| d.get("cached_tokens"))
                .and_then(Value::as_u64)
                .map(|n| n.min(u32::MAX as u64) as u32)
                .unwrap_or_else(|| get_u32(usage, &["cached_input_tokens"]));
            if raw_inp == 0 && out == 0 && cr == 0 {
                continue;
            }
            let fresh_inp = raw_inp.saturating_sub(cr);
            let model = nonempty_str(val.get("model"))
                .unwrap_or(&current_model)
                .to_string();
            fallback_records.push(UsageRecord {
                request_id: format!("codex_session:{}", sha256_str(line.as_bytes())),
                alternate_request_ids: Vec::new(),
                dedup_namespace: None,
                semantic_id: None,
                has_entry_id: false,
                provider_id: "_codex_session".to_string(),
                app_type: "codex".to_string(),
                model: model.clone(),
                request_model: model,
                fresh_input_tokens: fresh_inp,
                output_tokens: out,
                cache_read_tokens: cr,
                cache_creation_tokens: 0,
                cache_creation_1h_tokens: 0,
                explicit_cost: None,
                status_code: 200,
                error_message: None,
                session_id: session_id.clone(),
                provider_type: "codex_session",
                data_source: "codex_session",
                is_streaming: true,
                created_at,
                update_if_higher_output: false,
            });
        }
    }

    if events.is_empty() {
        for rec in fallback_records {
            let _ = insert_or_update_record(conn, pricing, &rec, report);
        }
        return line_num;
    }

    // Determine thread ID matching cc-switch (`codex_session:thread-v1:<root_thread_id>:<idx>`)
    let root_thread_id = extract_trailing_uuid(path)
        .or_else(|| session_id.as_deref().map(|s| s.to_ascii_lowercase()))
        .unwrap_or_else(|| sha256_str(path.to_string_lossy().as_bytes()));

    // Fork replay exclusion: skip leading prefix copied from parent rollout before fork cutoff
    let replay_prefix = if let (Some(parent_id), Some(cutoff)) = (forked_from_id.as_deref(), root_ts) {
        if let Some(parent_candidates) = rollout_index.get(parent_id) {
            if let Some(parent_path) = parent_candidates.first() {
                let timeline = {
                    let cached = pricing.codex_parent_cache.borrow().get(parent_path).cloned();
                    if let Some(hit) = cached {
                        hit
                    } else {
                        let parsed = collect_codex_parent_timeline(parent_path);
                        pricing
                            .codex_parent_cache
                            .borrow_mut()
                            .insert(parent_path.clone(), parsed.clone());
                        parsed
                    }
                };
                let parent_sigs: Vec<CodexTokenSig> = timeline
                    .into_iter()
                    .filter(|(ts, _)| *ts <= cutoff)
                    .map(|(_, sig)| sig)
                    .collect();
                matching_codex_replay_prefix(&events, &parent_sigs)
            } else {
                0
            }
        } else {
            0
        }
    } else {
        0
    };

    for (idx, ev) in events.into_iter().enumerate() {
        if idx < replay_prefix {
            report.skipped += 1;
            continue;
        }
        // Codex's `input_tokens` includes `cached_input_tokens` + `cache_write_input_tokens`
        let fresh_input = ev
            .delta
            .input
            .saturating_sub(ev.delta.cached_input)
            .saturating_sub(ev.delta.cache_write_input);

        let rec = UsageRecord {
            request_id: format!(
                "codex_session:thread-v1:{}:{}",
                root_thread_id, ev.event_index
            ),
            alternate_request_ids: Vec::new(),
            dedup_namespace: None,
            semantic_id: None,
            has_entry_id: true,
            provider_id: "_codex_session".to_string(),
            app_type: "codex".to_string(),
            model: ev.model.clone(),
            request_model: ev.model,
            fresh_input_tokens: fresh_input,
            output_tokens: ev.delta.output,
            cache_read_tokens: ev.delta.cached_input,
            cache_creation_tokens: ev.delta.cache_write_input,
            cache_creation_1h_tokens: 0,
            explicit_cost: None,
            status_code: 200,
            error_message: None,
            session_id: session_id.clone().or_else(|| Some(root_thread_id.clone())),
            provider_type: "codex_session",
            data_source: "codex_session",
            is_streaming: true,
            created_at: ev.created_at,
            update_if_higher_output: false,
        };
        let _ = insert_or_update_record(conn, pricing, &rec, report);
    }

    line_num
}

fn collect_codex_parent_timeline(parent_path: &Path) -> Vec<(i64, CodexTokenSig)> {
    let Ok(file) = File::open(parent_path) else {
        return Vec::new();
    };
    let mut sigs = Vec::new();
    let mut last_sig: Option<CodexTokenSig> = None;
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        let Ok(val) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let Some(ts) = val.get("timestamp").and_then(parse_any_timestamp) else {
            continue;
        };
        if val.get("type").and_then(Value::as_str) != Some("event_msg") {
            continue;
        }
        let Some(payload) = val.get("payload") else {
            continue;
        };
        if payload.get("type").and_then(Value::as_str) != Some("token_count") {
            continue;
        }
        let Some(info) = payload.get("info").filter(|i| !i.is_null()) else {
            continue;
        };
        let sig = CodexTokenSig {
            total: info.get("total_token_usage").and_then(CodexSnap::from_value),
            last: info.get("last_token_usage").and_then(CodexSnap::from_value),
        };
        if sig.total.is_none() && sig.last.is_none() {
            continue;
        }
        if last_sig == Some(sig) {
            continue;
        }
        last_sig = Some(sig);
        sigs.push((ts, sig));
    }
    sigs
}

fn matching_codex_replay_prefix(child: &[CodexTokenEvent], parent: &[CodexTokenSig]) -> usize {
    let mut offset = 0usize;
    let mut matched = 0usize;
    for ev in child {
        let Some(pos) = parent[offset..].iter().position(|s| *s == ev.sig) else {
            break;
        };
        offset += pos + 1;
        matched += 1;
    }
    matched
}

// ---------------------------------------------------------------------------
// 4. Antigravity (`agy`)
// ---------------------------------------------------------------------------

fn read_default_agy_model(home: &Path) -> String {
    for sub in ["antigravity-cli", "antigravity-ide", "antigravity"] {
        let settings_path = home.join(".gemini").join(sub).join("settings.json");
        if let Ok(content) = fs::read_to_string(&settings_path) {
            if let Ok(val) = serde_json::from_str::<Value>(&content) {
                if let Some(m) = nonempty_str(val.get("model")) {
                    return m.to_string();
                }
            }
        }
    }
    "Gemini 4 Argon (Medium)".to_string()
}

fn normalize_agy_model_slug(display_model: &str) -> String {
    display_model
        .trim()
        .to_ascii_lowercase()
        .replace(['(', ')'], "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
}

fn ingest_antigravity_jsonl(
    conn: &Connection,
    pricing: &PricingTable,
    path: &Path,
    default_display_model: &str,
    report: &mut SessionSyncResult,
) -> i64 {
    let Ok(file) = File::open(path) else {
        return 0;
    };
    let cid = path
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let mut current_request_model = default_display_model.to_string();
    let mut current_model_slug = normalize_agy_model_slug(&current_request_model);
    let mut line_num: i64 = 0;

    for line in BufReader::new(file).lines().map_while(Result::ok) {
        line_num += 1;
        if line.is_empty() {
            continue;
        }
        let has_model_change = line.contains("Model Selection");
        let has_tokens = line.contains("\"input_tokens\"") || line.contains("\"output_tokens\"");
        if !has_model_change && !has_tokens {
            continue;
        }
        let Ok(val) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if has_model_change {
            if let Some(content) = val.get("content").and_then(Value::as_str) {
                if let Some(idx) = content.find("Model Selection` from ") {
                    let after = &content[idx + "Model Selection` from ".len()..];
                    if let Some(to_idx) = after.find(" to ") {
                        let rest = &after[to_idx + 4..];
                        let new_model = rest
                            .split(". No need to comment")
                            .next()
                            .unwrap_or(rest)
                            .trim_end_matches('.')
                            .trim();
                        if !new_model.is_empty() {
                            current_request_model = new_model.to_string();
                            current_model_slug = normalize_agy_model_slug(&current_request_model);
                        }
                    }
                }
            }
        }
        if has_tokens {
            let inp = get_u32(&val, &["input_tokens"]);
            let out = get_u32(&val, &["output_tokens"]);
            let cr = get_u32(&val, &["cache_read_tokens"]);
            let cc = get_u32(&val, &["cache_creation_tokens"]);
            if inp == 0 && out == 0 && cr == 0 && cc == 0 {
                continue;
            }
            let step_idx = val.get("step_index").and_then(Value::as_u64).unwrap_or(0);
            let req_id = if step_idx > 0 {
                format!("agy:{cid}:{step_idx}")
            } else {
                format!("agy:{cid}:{}", sha256_str(line.as_bytes()))
            };
            let created_at = val
                .get("created_at")
                .and_then(parse_any_timestamp)
                .unwrap_or_else(|| Local::now().timestamp());

            let rec = UsageRecord {
                request_id: req_id,
                alternate_request_ids: Vec::new(),
                dedup_namespace: None,
                semantic_id: None,
                has_entry_id: step_idx > 0,
                provider_id: "_agy_session".to_string(),
                app_type: "agy".to_string(),
                model: current_model_slug.clone(),
                request_model: current_request_model.clone(),
                fresh_input_tokens: inp,
                output_tokens: out,
                cache_read_tokens: cr,
                cache_creation_tokens: cc,
                cache_creation_1h_tokens: 0,
                explicit_cost: None,
                status_code: 200,
                error_message: None,
                session_id: Some(cid.clone()),
                provider_type: "agy_session",
                data_source: "agy_session",
                is_streaming: true,
                created_at,
                update_if_higher_output: false,
            };
            let _ = insert_or_update_record(conn, pricing, &rec, report);
        }
    }

    line_num
}

// ---------------------------------------------------------------------------
// 5. Gemini CLI (.json & .jsonl)
// ---------------------------------------------------------------------------

fn ingest_gemini_session(
    conn: &Connection,
    pricing: &PricingTable,
    path: &Path,
    report: &mut SessionSyncResult,
) -> i64 {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let meta_len = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    if meta_len > MAX_JSON_FILE_BYTES {
        return 0;
    }

    let mut session_id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("gemini")
        .to_string();
    let mut messages: Vec<Value> = Vec::new();
    let mut line_num: i64 = 0;

    if ext == "json" {
        let Ok(raw) = fs::read_to_string(path) else {
            return 0;
        };
        let Ok(doc) = serde_json::from_str::<Value>(&raw) else {
            return 0;
        };
        if let Some(sid) = nonempty_str(doc.get("sessionId")) {
            session_id = sid.to_string();
        }
        if let Some(arr) = doc.get("messages").and_then(Value::as_array) {
            messages = arr.clone();
            line_num = messages.len() as i64;
        }
    } else if ext == "jsonl" {
        let Ok(file) = File::open(path) else {
            return 0;
        };
        let mut by_id: HashMap<String, Value> = HashMap::new();
        let mut order: Vec<String> = Vec::new();
        for line in BufReader::new(file).lines().map_while(Result::ok) {
            line_num += 1;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let Ok(v) = serde_json::from_str::<Value>(trimmed) else {
                continue;
            };
            if let Some(sid) = nonempty_str(v.get("sessionId")) {
                session_id = sid.to_string();
            }
            // Gemini CLI 0.36+ emits replay records: top-level `{id, type: "gemini", tokens, ...}`
            // or `$set`/patch events or `session_metadata`. Track message objects with `type == "gemini"`.
            let target = v.get("message").unwrap_or(&v);
            if target.get("type").and_then(Value::as_str) == Some("gemini")
                || target.get("role").and_then(Value::as_str) == Some("model")
            {
                let key = nonempty_str(target.get("id"))
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("line-{line_num}"));
                if !by_id.contains_key(&key) {
                    order.push(key.clone());
                }
                by_id.insert(key, target.clone());
            }
        }
        for k in order {
            if let Some(m) = by_id.remove(&k) {
                messages.push(m);
            }
        }
    }

    for (idx, msg) in messages.iter().enumerate() {
        let mtype = msg
            .get("type")
            .and_then(Value::as_str)
            .or_else(|| msg.get("role").and_then(Value::as_str))
            .unwrap_or("");
        if !matches!(mtype, "gemini" | "model" | "assistant") {
            continue;
        }
        let Some(tokens) = msg.get("tokens").or_else(|| msg.get("usageMetadata")) else {
            continue;
        };
        let raw_inp = get_u32(tokens, &["input", "promptTokenCount"]);
        let cached = get_u32(tokens, &["cached", "cachedContentTokenCount"]);
        let out_main = get_u32(tokens, &["output", "candidatesTokenCount"]);
        let thoughts = get_u32(tokens, &["thoughts", "thoughtsTokenCount"]);
        let tool_t = get_u32(tokens, &["tool", "toolUsePromptTokenCount"]);
        let out = out_main.saturating_add(thoughts).saturating_add(tool_t);
        if raw_inp == 0 && out == 0 && cached == 0 {
            continue;
        }
        let fresh_inp = raw_inp.saturating_sub(cached);
        let msg_id = nonempty_str(msg.get("id"))
            .map(str::to_string)
            .unwrap_or_else(|| format!("msg-{idx}"));
        let model = nonempty_str(msg.get("model"))
            .unwrap_or("gemini-2.5-pro")
            .to_string();
        let created_at = msg
            .get("timestamp")
            .and_then(parse_any_timestamp)
            .unwrap_or_else(|| Local::now().timestamp());

        let rec = UsageRecord {
            request_id: format!("gemini_session:{session_id}:{msg_id}"),
            alternate_request_ids: vec![format!("gemini:{msg_id}")],
            dedup_namespace: None,
            semantic_id: None,
            has_entry_id: true,
            provider_id: "_gemini_session".to_string(),
            app_type: "gemini".to_string(),
            model: model.clone(),
            request_model: model,
            fresh_input_tokens: fresh_inp,
            output_tokens: out,
            cache_read_tokens: cached,
            cache_creation_tokens: 0,
            cache_creation_1h_tokens: 0,
            explicit_cost: None,
            status_code: 200,
            error_message: None,
            session_id: Some(session_id.clone()),
            provider_type: "gemini_session",
            data_source: "gemini_session",
            is_streaming: true,
            created_at,
            update_if_higher_output: true,
        };
        let _ = insert_or_update_record(conn, pricing, &rec, report);
    }

    line_num
}

// ---------------------------------------------------------------------------
// 6. OpenCode (SQLite V1/V2 + JSON storage fallback)
// ---------------------------------------------------------------------------

fn opencode_storage_candidates(paths: &Paths) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        if !xdg.trim().is_empty() {
            dirs.push(PathBuf::from(xdg).join("opencode"));
        }
    }
    dirs.push(paths.home.join(".local").join("share").join("opencode"));
    if let Ok(local_appdata) = std::env::var("LOCALAPPDATA") {
        dirs.push(PathBuf::from(&local_appdata).join("opencode"));
        dirs.push(PathBuf::from(&local_appdata).join("OpenCode"));
    }
    if let Ok(appdata) = std::env::var("APPDATA") {
        dirs.push(PathBuf::from(appdata).join("opencode"));
    }
    dirs.push(paths.tool_root(ToolId::OpenCode));
    let mut unique = Vec::new();
    for d in dirs {
        if !unique.contains(&d) {
            unique.push(d);
        }
    }
    unique
}

fn opencode_db_candidates(paths: &Paths) -> Vec<PathBuf> {
    opencode_storage_candidates(paths)
        .into_iter()
        .map(|d| d.join("opencode.db"))
        .collect()
}

fn ingest_opencode_db(
    conn: &Connection,
    pricing: &PricingTable,
    db_path: &Path,
    report: &mut SessionSyncResult,
) -> i64 {
    let Ok(oc_conn) = open_readonly_sqlite(db_path) else {
        return 0;
    };
    let mut rows_seen: i64 = 0;

    // Strategy A: V1 `message` table (`id`, `session_id`, `time_created`, `data`)
    if table_has_columns(&oc_conn, "message", &["id", "session_id", "data"]) {
        if let Ok(mut stmt) = oc_conn
            .prepare("SELECT id, session_id, time_created, data FROM message ORDER BY rowid ASC")
        {
            if let Ok(rows) = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<i64>>(2)?,
                    r.get::<_, String>(3)?,
                ))
            }) {
                for row in rows.flatten() {
                    rows_seen += 1;
                    let (msg_id, sid, t_created, data_str) = row;
                    if let Ok(data) = serde_json::from_str::<Value>(&data_str) {
                        ingest_opencode_payload(
                            conn,
                            pricing,
                            &msg_id,
                            &sid,
                            t_created,
                            &data,
                            report,
                        );
                    }
                }
            }
        }
    }

    // Strategy B: V2 `session_message` (or `session_v2` aggregate / JSON data)
    if rows_seen == 0 && table_has_columns(&oc_conn, "session_message", &["id", "session_id"]) {
        let cols = table_columns(&oc_conn, "session_message");
        if cols.contains("data") {
            if let Ok(mut stmt) = oc_conn.prepare(
                "SELECT id, session_id, time_created, data FROM session_message ORDER BY rowid ASC",
            ) {
                if let Ok(rows) = stmt.query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, Option<i64>>(2)?,
                        r.get::<_, String>(3)?,
                    ))
                }) {
                    for row in rows.flatten() {
                        rows_seen += 1;
                        let (msg_id, sid, t_created, data_str) = row;
                        if let Ok(data) = serde_json::from_str::<Value>(&data_str) {
                            ingest_opencode_payload(
                                conn,
                                pricing,
                                &msg_id,
                                &sid,
                                t_created,
                                &data,
                                report,
                            );
                        }
                    }
                }
            }
        }
    }

    rows_seen
}

fn ingest_opencode_message_json(
    conn: &Connection,
    pricing: &PricingTable,
    path: &Path,
    report: &mut SessionSyncResult,
) -> i64 {
    let Ok(raw) = fs::read_to_string(path) else {
        return 0;
    };
    let Ok(data) = serde_json::from_str::<Value>(&raw) else {
        return 0;
    };
    let msg_id = nonempty_str(data.get("id"))
        .or_else(|| path.file_stem().and_then(|s| s.to_str()))
        .unwrap_or("opencode-msg")
        .to_string();
    let sid = nonempty_str(data.get("sessionID"))
        .or_else(|| nonempty_str(data.get("session_id")))
        .or_else(|| {
            path.parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
        })
        .unwrap_or("opencode-session")
        .to_string();
    ingest_opencode_payload(conn, pricing, &msg_id, &sid, None, &data, report);
    1
}

fn ingest_opencode_payload(
    conn: &Connection,
    pricing: &PricingTable,
    msg_id: &str,
    session_id: &str,
    fallback_created: Option<i64>,
    data: &Value,
    report: &mut SessionSyncResult,
) {
    if data.get("role").and_then(Value::as_str) != Some("assistant") {
        return;
    }
    let Some(tokens) = data.get("tokens") else {
        return;
    };
    // Skip incomplete in-flight OpenCode assistant messages unless completed or compaction finished
    let completed = data
        .get("time")
        .and_then(|t| t.get("completed"))
        .and_then(parse_any_timestamp);
    if completed.is_none()
        && data.get("finish").is_none()
        && data.get("mode").and_then(Value::as_str) != Some("compaction")
    {
        return;
    }

    let inp = get_u32(tokens, &["input"]);
    let out_base = get_u32(tokens, &["output"]);
    let reasoning = get_u32(tokens, &["reasoning"]);
    let out = out_base.saturating_add(reasoning);
    let cache = tokens.get("cache");
    let cr = cache.map_or(0, |c| get_u32(c, &["read"]));
    let cw = cache.map_or(0, |c| get_u32(c, &["write"]));
    let cost_val = data.get("cost").and_then(parse_f64).unwrap_or(0.0);

    if inp == 0 && out == 0 && cr == 0 && cw == 0 && cost_val <= 0.0 {
        return;
    }

    let model = nonempty_str(data.get("modelID"))
        .or_else(|| data.get("model").and_then(|m| nonempty_str(m.get("modelID"))))
        .unwrap_or("unknown")
        .to_string();
    let provider_id = nonempty_str(data.get("providerID"))
        .or_else(|| data.get("model").and_then(|m| nonempty_str(m.get("providerID"))))
        .unwrap_or("_opencode_session")
        .to_string();
    let created_at = completed
        .or_else(|| {
            data.get("time")
                .and_then(|t| t.get("created"))
                .and_then(parse_any_timestamp)
        })
        .or_else(|| fallback_created.map(normalize_unix_ts))
        .unwrap_or_else(|| Local::now().timestamp());

    let explicit_cost = if cost_val > 0.0 {
        Some(CostBreakdown {
            input: 0.0,
            output: 0.0,
            cache_read: 0.0,
            cache_creation: 0.0,
            total: cost_val,
        })
    } else {
        None
    };

    let rec = UsageRecord {
        request_id: format!("opencode_session:{session_id}:{msg_id}"),
        alternate_request_ids: vec![format!("opencode:{msg_id}")],
        dedup_namespace: None,
        semantic_id: None,
        has_entry_id: true,
        provider_id,
        app_type: "opencode".to_string(),
        model: model.clone(),
        request_model: model,
        fresh_input_tokens: inp,
        output_tokens: out,
        cache_read_tokens: cr,
        cache_creation_tokens: cw,
        cache_creation_1h_tokens: 0,
        explicit_cost,
        status_code: 200,
        error_message: None,
        session_id: Some(session_id.to_string()),
        provider_type: "opencode_session",
        data_source: "opencode_session",
        is_streaming: true,
        created_at,
        update_if_higher_output: false,
    };
    let _ = insert_or_update_record(conn, pricing, &rec, report);
}

// ---------------------------------------------------------------------------
// 7. Grok (`updates.jsonl`)
// ---------------------------------------------------------------------------

fn ingest_grok_updates(
    conn: &Connection,
    pricing: &PricingTable,
    path: &Path,
    report: &mut SessionSyncResult,
) -> i64 {
    let Ok(file) = File::open(path) else {
        return 0;
    };
    let session_id = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("grok")
        .to_string();
    let mut line_num: i64 = 0;

    for line in BufReader::new(file).lines().map_while(Result::ok) {
        line_num += 1;
        if !line.contains("turn_completed") {
            continue;
        }
        let Ok(val) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let update = val.get("update").unwrap_or(&val);
        if update.get("type").and_then(Value::as_str) != Some("turn_completed") {
            continue;
        }
        let created_at = val
            .get("timestamp")
            .and_then(parse_any_timestamp)
            .or_else(|| update.get("timestamp").and_then(parse_any_timestamp))
            .unwrap_or_else(|| Local::now().timestamp());
        let turn_id = nonempty_str(update.get("turnId"))
            .or_else(|| nonempty_str(val.get("id")))
            .map(str::to_string)
            .unwrap_or_else(|| format!("line-{line_num}"));

        if let Some(model_map) = update.get("modelUsage").and_then(Value::as_object) {
            for (model_name, usage) in model_map {
                let raw_inp = get_u32(usage, &["inputTokens", "input_tokens"]);
                let cached = get_u32(usage, &["cachedInputTokens", "cached_input_tokens"]);
                let out = get_u32(usage, &["outputTokens", "output_tokens"])
                    .saturating_add(get_u32(usage, &["reasoningTokens", "reasoning_tokens"]));
                let fresh_inp = raw_inp.saturating_sub(cached);
                let ticks = usage
                    .get("costUsdTicks")
                    .and_then(parse_f64)
                    .unwrap_or(0.0);
                let explicit_cost = if ticks > 0.0 {
                    Some(CostBreakdown {
                        input: 0.0,
                        output: 0.0,
                        cache_read: 0.0,
                        cache_creation: 0.0,
                        total: ticks / 10_000_000_000.0,
                    })
                } else {
                    None
                };
                if fresh_inp == 0 && cached == 0 && out == 0 && explicit_cost.is_none() {
                    continue;
                }
                let rec = UsageRecord {
                    request_id: format!("grok_session:{session_id}:{turn_id}:{model_name}"),
                    alternate_request_ids: Vec::new(),
                    dedup_namespace: None,
                    semantic_id: None,
                    has_entry_id: true,
                    provider_id: "_grok_session".to_string(),
                    app_type: "grok".to_string(),
                    model: model_name.clone(),
                    request_model: model_name.clone(),
                    fresh_input_tokens: fresh_inp,
                    output_tokens: out,
                    cache_read_tokens: cached,
                    cache_creation_tokens: 0,
                    cache_creation_1h_tokens: 0,
                    explicit_cost,
                    status_code: 200,
                    error_message: None,
                    session_id: Some(session_id.clone()),
                    provider_type: "grok_session",
                    data_source: "grok_session",
                    is_streaming: true,
                    created_at,
                    update_if_higher_output: false,
                };
                let _ = insert_or_update_record(conn, pricing, &rec, report);
            }
        }
    }

    line_num
}

// ---------------------------------------------------------------------------
// 8. Kimi (`wire.jsonl` / `trees/*.jsonl`)
// ---------------------------------------------------------------------------

fn ingest_kimi_jsonl(
    conn: &Connection,
    pricing: &PricingTable,
    path: &Path,
    report: &mut SessionSyncResult,
) -> i64 {
    let Ok(file) = File::open(path) else {
        return 0;
    };
    let session_id = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("kimi")
        .to_string();
    let mut line_num: i64 = 0;
    let mut current_model = "kimi-k2.5".to_string();

    for line in BufReader::new(file).lines().map_while(Result::ok) {
        line_num += 1;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(val) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        let msg = val.get("message").unwrap_or(&val);
        if let Some(m) = nonempty_str(msg.get("model"))
            .or_else(|| nonempty_str(val.get("model")))
            .or_else(|| val.get("payload").and_then(|p| nonempty_str(p.get("model"))))
        {
            current_model = m.to_string();
        }
        let token_usage = msg
            .get("token_usage")
            .or_else(|| msg.get("usage"))
            .or_else(|| {
                val.get("payload")
                    .and_then(|p| p.get("token_usage").or_else(|| p.get("usage")))
            });
        let Some(tu) = token_usage else {
            continue;
        };
        let raw_inp = get_u32(tu, &["input_other", "input_tokens", "input", "prompt_tokens"]);
        let cr = get_u32(
            tu,
            &[
                "input_cache_read",
                "cached_tokens",
                "cache_read_input_tokens",
                "cacheRead",
            ],
        );
        let cw = get_u32(
            tu,
            &[
                "input_cache_creation",
                "cache_creation_input_tokens",
                "cacheWrite",
            ],
        );
        let out = get_u32(tu, &["output", "output_tokens", "completion_tokens"]);
        if raw_inp == 0 && cr == 0 && cw == 0 && out == 0 {
            continue;
        }
        let created_at = val
            .get("timestamp")
            .and_then(parse_any_timestamp)
            .unwrap_or_else(|| Local::now().timestamp());
        let eid = nonempty_str(val.get("message_id"))
            .or_else(|| val.get("payload").and_then(|p| nonempty_str(p.get("message_id"))))
            .or_else(|| nonempty_str(val.get("id")))
            .map(str::to_string)
            .unwrap_or_else(|| sha256_str(trimmed.as_bytes()));

        let rec = UsageRecord {
            request_id: format!("kimi_session:{session_id}:{eid}"),
            alternate_request_ids: Vec::new(),
            dedup_namespace: None,
            semantic_id: None,
            has_entry_id: true,
            provider_id: "_kimi_session".to_string(),
            app_type: "kimi".to_string(),
            model: current_model.clone(),
            request_model: current_model.clone(),
            fresh_input_tokens: raw_inp,
            output_tokens: out,
            cache_read_tokens: cr,
            cache_creation_tokens: cw,
            cache_creation_1h_tokens: 0,
            explicit_cost: None,
            status_code: 200,
            error_message: None,
            session_id: Some(session_id.clone()),
            provider_type: "kimi_session",
            data_source: "kimi_session",
            is_streaming: true,
            created_at,
            update_if_higher_output: false,
        };
        let _ = insert_or_update_record(conn, pricing, &rec, report);
    }

    line_num
}

// ---------------------------------------------------------------------------
// 9. OpenClaw (`agents/*/sessions/*.jsonl` & `openclaw-agent.sqlite`)
// ---------------------------------------------------------------------------

fn ingest_openclaw_jsonl(
    conn: &Connection,
    pricing: &PricingTable,
    path: &Path,
    report: &mut SessionSyncResult,
) -> i64 {
    let Ok(file) = File::open(path) else {
        return 0;
    };
    let mut session_id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("openclaw")
        .to_string();
    let mut line_num: i64 = 0;

    for line in BufReader::new(file).lines().map_while(Result::ok) {
        line_num += 1;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(val) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        if val.get("type").and_then(Value::as_str) == Some("session") {
            if let Some(id) = nonempty_str(val.get("id")) {
                session_id = id.to_string();
            }
            continue;
        }
        let msg = val.get("message").unwrap_or(&val);
        let role = msg.get("role").and_then(Value::as_str).unwrap_or("");
        if role != "assistant" && val.get("type").and_then(Value::as_str) != Some("assistant") {
            continue;
        }
        let Some(usage) = msg.get("usage").or_else(|| val.get("usage")) else {
            continue;
        };
        let inp = get_u32(usage, &["input", "input_tokens", "prompt_tokens"]);
        let out = get_u32(usage, &["output", "output_tokens", "completion_tokens"]);
        let cr = get_u32(usage, &["cacheRead", "cache_read_input_tokens", "cached_tokens"]);
        let cw = get_u32(usage, &["cacheWrite", "cache_creation_input_tokens"]);
        let cost_total = usage
            .get("cost")
            .and_then(|c| c.get("total"))
            .and_then(parse_f64)
            .unwrap_or(0.0);
        if inp == 0 && out == 0 && cr == 0 && cw == 0 && cost_total <= 0.0 {
            continue;
        }
        let model = nonempty_str(msg.get("model"))
            .or_else(|| nonempty_str(val.get("model")))
            .unwrap_or("unknown")
            .to_string();
        let provider = nonempty_str(msg.get("provider"))
            .unwrap_or("_openclaw_session")
            .to_string();
        let created_at = val
            .get("timestamp")
            .and_then(parse_any_timestamp)
            .or_else(|| msg.get("timestamp").and_then(parse_any_timestamp))
            .unwrap_or_else(|| Local::now().timestamp());
        let eid = nonempty_str(val.get("id"))
            .or_else(|| nonempty_str(msg.get("id")))
            .map(str::to_string)
            .unwrap_or_else(|| format!("line-{line_num}"));

        let rec = UsageRecord {
            request_id: format!("openclaw_session:{session_id}:{eid}"),
            alternate_request_ids: Vec::new(),
            dedup_namespace: None,
            semantic_id: None,
            has_entry_id: true,
            provider_id: provider,
            app_type: "openclaw".to_string(),
            model: model.clone(),
            request_model: model,
            fresh_input_tokens: inp,
            output_tokens: out,
            cache_read_tokens: cr,
            cache_creation_tokens: cw,
            cache_creation_1h_tokens: 0,
            explicit_cost: if cost_total > 0.0 {
                Some(CostBreakdown {
                    total: cost_total,
                    ..CostBreakdown::default()
                })
            } else {
                None
            },
            status_code: 200,
            error_message: None,
            session_id: Some(session_id.clone()),
            provider_type: "openclaw_session",
            data_source: "openclaw_session",
            is_streaming: true,
            created_at,
            update_if_higher_output: false,
        };
        let _ = insert_or_update_record(conn, pricing, &rec, report);
    }

    line_num
}

fn ingest_openclaw_sqlite(
    conn: &Connection,
    pricing: &PricingTable,
    db_path: &Path,
    report: &mut SessionSyncResult,
) -> i64 {
    let Ok(oc_conn) = open_readonly_sqlite(db_path) else {
        return 0;
    };
    let mut rows_seen: i64 = 0;
    if table_has_columns(&oc_conn, "messages", &["id", "session_id"]) {
        let cols = table_columns(&oc_conn, "messages");
        if cols.contains("usage_json") || cols.contains("usage") {
            let usage_col = if cols.contains("usage_json") {
                "usage_json"
            } else {
                "usage"
            };
            let model_col = if cols.contains("model") { "model" } else { "'unknown'" };
            let ts_col = if cols.contains("created_at") {
                "created_at"
            } else {
                "NULL"
            };
            let sql = format!(
                "SELECT id, session_id, {model_col}, {usage_col}, {ts_col} FROM messages WHERE {usage_col} IS NOT NULL"
            );
            if let Ok(mut stmt) = oc_conn.prepare(&sql) {
                if let Ok(rows) = stmt.query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, Option<String>>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, Option<ValueOrText>>(4)?,
                    ))
                }) {
                    for (id, sid, model, usage_str, ts_val) in rows.flatten() {
                        rows_seen += 1;
                        let Ok(usage) = serde_json::from_str::<Value>(&usage_str) else {
                            continue;
                        };
                        let inp = get_u32(&usage, &["input", "input_tokens"]);
                        let out = get_u32(&usage, &["output", "output_tokens"]);
                        let cr = get_u32(&usage, &["cacheRead", "cache_read_tokens"]);
                        let cw = get_u32(&usage, &["cacheWrite", "cache_creation_tokens"]);
                        if inp == 0 && out == 0 && cr == 0 && cw == 0 {
                            continue;
                        }
                        let m = model.unwrap_or_else(|| "unknown".to_string());
                        let rec = UsageRecord {
                            request_id: format!("openclaw_db:{sid}:{id}"),
                            alternate_request_ids: Vec::new(),
                            dedup_namespace: None,
                            semantic_id: None,
                            has_entry_id: true,
                            provider_id: "_openclaw_session".to_string(),
                            app_type: "openclaw".to_string(),
                            model: m.clone(),
                            request_model: m,
                            fresh_input_tokens: inp,
                            output_tokens: out,
                            cache_read_tokens: cr,
                            cache_creation_tokens: cw,
                            cache_creation_1h_tokens: 0,
                            explicit_cost: None,
                            status_code: 200,
                            error_message: None,
                            session_id: Some(sid),
                            provider_type: "openclaw_session",
                            data_source: "openclaw_session",
                            is_streaming: false,
                            created_at: ts_val
                                .map(|v| v.to_ts())
                                .unwrap_or_else(|| Local::now().timestamp()),
                            update_if_higher_output: false,
                        };
                        let _ = insert_or_update_record(conn, pricing, &rec, report);
                    }
                }
            }
        }
    }
    rows_seen
}

// ---------------------------------------------------------------------------
// 10. Hermes (`~/.hermes/state.db`)
// ---------------------------------------------------------------------------

fn ingest_hermes_db(
    conn: &Connection,
    pricing: &PricingTable,
    db_path: &Path,
    report: &mut SessionSyncResult,
) -> i64 {
    let Ok(h_conn) = open_readonly_sqlite(db_path) else {
        return 0;
    };
    let cols = table_columns(&h_conn, "sessions");
    if cols.is_empty() {
        return 0;
    }

    let sel = |name: &str, default_expr: &str| -> String {
        if cols.contains(name) {
            name.to_string()
        } else {
            default_expr.to_string()
        }
    };

    let sql = format!(
        "SELECT id, {}, {}, {}, {}, {}, {}, {} FROM sessions",
        sel("model", "'hermes-default'"),
        sel("input_tokens", "0"),
        sel("output_tokens", "0"),
        sel("cache_read_tokens", "0"),
        sel("cache_write_tokens", "0"),
        sel("estimated_cost_usd", "0.0"),
        sel("started_at", "NULL")
    );

    let mut rows_seen: i64 = 0;
    if let Ok(mut stmt) = h_conn.prepare(&sql) {
        if let Ok(rows) = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<i64>>(2)?,
                r.get::<_, Option<i64>>(3)?,
                r.get::<_, Option<i64>>(4)?,
                r.get::<_, Option<i64>>(5)?,
                r.get::<_, Option<f64>>(6)?,
                r.get::<_, Option<ValueOrText>>(7)?,
            ))
        }) {
            for (sid, model, inp, out, cr, cw, cost_usd, ts_val) in rows.flatten() {
                rows_seen += 1;
                let inp_u = inp.unwrap_or(0).max(0) as u32;
                let out_u = out.unwrap_or(0).max(0) as u32;
                let cr_u = cr.unwrap_or(0).max(0) as u32;
                let cw_u = cw.unwrap_or(0).max(0) as u32;
                let cost = cost_usd.unwrap_or(0.0);
                if inp_u == 0 && out_u == 0 && cr_u == 0 && cw_u == 0 && cost <= 0.0 {
                    continue;
                }
                let m = model
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| "hermes-default".to_string());
                let rec = UsageRecord {
                    request_id: format!("hermes_session:{sid}"),
                    alternate_request_ids: Vec::new(),
                    dedup_namespace: None,
                    semantic_id: None,
                    has_entry_id: true,
                    provider_id: "_hermes_session".to_string(),
                    app_type: "hermes".to_string(),
                    model: m.clone(),
                    request_model: m,
                    fresh_input_tokens: inp_u,
                    output_tokens: out_u,
                    cache_read_tokens: cr_u,
                    cache_creation_tokens: cw_u,
                    cache_creation_1h_tokens: 0,
                    explicit_cost: if cost > 0.0 {
                        Some(CostBreakdown {
                            total: cost,
                            ..CostBreakdown::default()
                        })
                    } else {
                        None
                    },
                    status_code: 200,
                    error_message: None,
                    session_id: Some(sid),
                    provider_type: "hermes_session",
                    data_source: "hermes_session",
                    is_streaming: false,
                    created_at: ts_val
                        .map(|v| v.to_ts())
                        .unwrap_or_else(|| Local::now().timestamp()),
                    // Re-sync existing Hermes session rows when active session token counters rise
                    update_if_higher_output: true,
                };
                let _ = insert_or_update_record(conn, pricing, &rec, report);
            }
        }
    }

    rows_seen
}

// ---------------------------------------------------------------------------
// 11. Dsh (`~/.dsh/sessions/**/*.jsonl`)
// ---------------------------------------------------------------------------

fn ingest_dsh_jsonl(
    conn: &Connection,
    pricing: &PricingTable,
    path: &Path,
    report: &mut SessionSyncResult,
) -> i64 {
    let Ok(file) = File::open(path) else {
        return 0;
    };
    let session_id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("dsh")
        .to_string();
    let mut line_num: i64 = 0;

    for line in BufReader::new(file).lines().map_while(Result::ok) {
        line_num += 1;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(val) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        let Some(usage) = val
            .get("usage")
            .or_else(|| val.get("message").and_then(|m| m.get("usage")))
        else {
            continue;
        };
        let inp = get_u32(usage, &["prompt_tokens", "input_tokens", "input"]);
        let out = get_u32(usage, &["completion_tokens", "output_tokens", "output"]);
        let cr = get_u32(
            usage,
            &["prompt_cache_hit_tokens", "cache_read_input_tokens", "cacheRead"],
        );
        let cw = get_u32(
            usage,
            &["prompt_cache_miss_tokens", "cache_creation_input_tokens", "cacheWrite"],
        );
        if inp == 0 && out == 0 && cr == 0 {
            continue;
        }
        let model = nonempty_str(val.get("model"))
            .or_else(|| val.get("message").and_then(|m| nonempty_str(m.get("model"))))
            .unwrap_or("deepseek-chat")
            .to_string();
        let created_at = val
            .get("timestamp")
            .and_then(parse_any_timestamp)
            .or_else(|| val.get("created_at").and_then(parse_any_timestamp))
            .unwrap_or_else(|| Local::now().timestamp());
        let eid = nonempty_str(val.get("id"))
            .map(str::to_string)
            .unwrap_or_else(|| format!("line-{line_num}"));

        let rec = UsageRecord {
            request_id: format!("dsh_session:{session_id}:{eid}"),
            alternate_request_ids: Vec::new(),
            dedup_namespace: None,
            semantic_id: None,
            has_entry_id: true,
            provider_id: "_dsh_session".to_string(),
            app_type: "dsh".to_string(),
            model: model.clone(),
            request_model: model,
            fresh_input_tokens: inp,
            output_tokens: out,
            cache_read_tokens: cr,
            cache_creation_tokens: cw,
            cache_creation_1h_tokens: 0,
            explicit_cost: None,
            status_code: 200,
            error_message: None,
            session_id: Some(session_id.clone()),
            provider_type: "dsh_session",
            data_source: "dsh_session",
            is_streaming: true,
            created_at,
            update_if_higher_output: false,
        };
        let _ = insert_or_update_record(conn, pricing, &rec, report);
    }

    line_num
}

// ---------------------------------------------------------------------------
// Generic Shared Utilities
// ---------------------------------------------------------------------------

fn collect_files(
    dir: &Path,
    exts: &[&str],
    skip_dirs: &[&str],
    skip_hidden: bool,
) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if dir.is_dir() {
        walk_recursive(dir, exts, skip_dirs, skip_hidden, 0, &mut out);
    }
    out
}

fn walk_recursive(
    dir: &Path,
    exts: &[&str],
    skip_dirs: &[&str],
    skip_hidden: bool,
    depth: usize,
    out: &mut Vec<PathBuf>,
) {
    if depth > DEFAULT_MAX_DEPTH {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // Skip symlinks to guard against cycles or escapes
        if fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
            continue;
        }
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if skip_hidden && name.starts_with('.') {
                continue;
            }
            if skip_dirs.iter().any(|s| *s == name) {
                continue;
            }
            walk_recursive(&path, exts, skip_dirs, skip_hidden, depth + 1, out);
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if exts.iter().any(|allowed| *allowed == ext) {
                out.push(path);
            }
        }
    }
}

pub(crate) fn parse_any_timestamp(val: &Value) -> Option<i64> {
    match val {
        Value::String(s) => {
            let s = s.trim();
            if s.is_empty() {
                return None;
            }
            if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
                return Some(dt.timestamp());
            }
            if let Ok(n) = s.parse::<f64>() {
                return Some(normalize_unix_f64(n));
            }
            None
        }
        Value::Number(n) => n.as_i64().map(normalize_unix_ts).or_else(|| n.as_f64().map(normalize_unix_f64)),
        _ => None,
    }
}

fn normalize_unix_ts(raw: i64) -> i64 {
    if raw.abs() >= 100_000_000_000_000 {
        raw / 1_000_000_000 // nanoseconds -> seconds
    } else if raw.abs() >= 100_000_000_000 {
        raw / 1_000 // milliseconds -> seconds
    } else {
        raw
    }
}

fn normalize_unix_f64(raw: f64) -> i64 {
    if raw.abs() >= 1e14 {
        (raw / 1e9) as i64
    } else if raw.abs() >= 1e11 {
        (raw / 1e3) as i64
    } else {
        raw as i64
    }
}

fn nonempty_str(v: Option<&Value>) -> Option<&str> {
    v.and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn clamp_label(raw: &str, fallback: &str) -> String {
    let s = raw.trim();
    let s = if s.is_empty() { fallback } else { s };
    if s.len() <= MAX_LABEL_BYTES {
        return s.to_string();
    }
    let mut end = MAX_LABEL_BYTES;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

fn get_u32(obj: &Value, keys: &[&str]) -> u32 {
    for k in keys {
        if let Some(v) = obj.get(*k) {
            if let Some(u) = v.as_u64() {
                return u.min(u32::MAX as u64) as u32;
            }
            if let Some(i) = v.as_i64() {
                if i >= 0 {
                    return (i as u64).min(u32::MAX as u64) as u32;
                }
            }
        }
    }
    0
}

fn parse_f64(v: &Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.trim().parse::<f64>().ok()))
        .filter(|n| n.is_finite() && *n >= 0.0)
}

fn sha256_str(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn open_readonly_sqlite(path: &Path) -> Result<Connection, rusqlite::Error> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    let _ = conn.busy_timeout(std::time::Duration::from_secs(2));
    Ok(conn)
}

fn table_columns(conn: &Connection, table: &str) -> HashSet<String> {
    let mut set = HashSet::new();
    let sql = format!("PRAGMA table_info({table})");
    if let Ok(mut stmt) = conn.prepare(&sql) {
        if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(1)) {
            for col in rows.flatten() {
                set.insert(col);
            }
        }
    }
    set
}

fn table_has_columns(conn: &Connection, table: &str, required: &[&str]) -> bool {
    let cols = table_columns(conn, table);
    required.iter().all(|c| cols.contains(*c))
}

enum ValueOrText {
    Int(i64),
    Real(f64),
    Text(String),
}

impl ValueOrText {
    fn to_ts(&self) -> i64 {
        match self {
            ValueOrText::Int(i) => normalize_unix_ts(*i),
            ValueOrText::Real(f) => normalize_unix_f64(*f),
            ValueOrText::Text(s) => DateTime::parse_from_rfc3339(s.trim())
                .map(|dt| dt.timestamp())
                .or_else(|_| s.trim().parse::<f64>().map(normalize_unix_f64))
                .unwrap_or_else(|_| Local::now().timestamp()),
        }
    }
}

impl rusqlite::types::FromSql for ValueOrText {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        match value {
            rusqlite::types::ValueRef::Integer(i) => Ok(ValueOrText::Int(i)),
            rusqlite::types::ValueRef::Real(f) => Ok(ValueOrText::Real(f)),
            rusqlite::types::ValueRef::Text(bytes) => Ok(ValueOrText::Text(
                String::from_utf8_lossy(bytes).into_owned(),
            )),
            _ => Err(rusqlite::types::FromSqlError::InvalidType),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::UsageDb;
    use tempfile::tempdir;

    #[test]
    fn test_pi_and_omp_sessions_rfc3339_fork_dedup_and_incremental_append() {
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        let app_data = tmp.path().join("appdata");
        let paths = Paths::new(home.clone(), app_data);
        let db = UsageDb::open(&paths).unwrap();

        let pi_dir = home.join(".pi").join("agent").join("sessions").join("proj-alpha");
        let fold_dir = pi_dir.join("context-fold");
        fs::create_dir_all(&fold_dir).unwrap();

        let parent_file = pi_dir.join("001-parent.jsonl");
        let parent_lines = concat!(
            r#"{"type":"session","version":1,"id":"pi-sess-1","timestamp":"2026-11-14T10:00:00Z","cwd":"D:/work"}"#, "\n",
            r#"{"type":"message","id":"entry-a","timestamp":"2026-11-14T10:01:00Z","message":{"role":"assistant","provider":"openrouter","model":"anthropic/claude-sonnet-4-6","responseModel":"claude-sonnet-4-6-20260205","stopReason":"stop","usage":{"input":1000,"output":250,"cacheRead":400,"cacheWrite":100,"cost":{"input":0.003,"output":0.00375,"cacheRead":0.00012,"cacheWrite":0.000375,"total":0.007245}}}}"#, "\n",
            r#"{"type":"message","id":"entry-b","timestamp":"2026-11-14T10:02:00Z","message":{"role":"toolResult","toolName":"task","provider":"openrouter","model":"gpt-4o","usage":{"input":500,"output":50,"cacheRead":0,"cacheWrite":0}}}"#, "\n",
            r#"{"type":"compaction","id":"entry-c","timestamp":1794650600000,"usage":{"input":200,"output":40,"cacheRead":0,"cacheWrite":0}}"#, "\n",
            r#"{"type":"branch_summary","id":"entry-d","timestamp":"2026-11-14T10:04:00Z","usage":{"input":150,"output":30,"cacheRead":0,"cacheWrite":0}}"#, "\n",
        );
        fs::write(&parent_file, parent_lines).unwrap();

        // Ignored helper subdirectory
        fs::write(fold_dir.join("ignored.jsonl"), parent_lines).unwrap();

        // Forked Pi session repeating entry-a even with a rewritten entry id!
        let fork_file = pi_dir.join("002-fork.jsonl");
        let fork_lines = format!(
            "{}\n{}\n{}\n",
            r#"{"type":"session","version":1,"id":"pi-sess-fork","parentSession":"001-parent.jsonl","timestamp":"2026-11-14T10:05:00Z"}"#,
            r#"{"type":"message","id":"rewritten-entry-a","timestamp":"2026-11-14T10:01:00Z","message":{"role":"assistant","provider":"openrouter","model":"anthropic/claude-sonnet-4-6","stopReason":"stop","usage":{"input":1000,"output":250,"cacheRead":400,"cacheWrite":100,"cost":{"total":0.007245}}}}"#,
            r#"{"type":"message","id":"entry-new-on-fork","timestamp":"2026-11-14T10:06:00Z","message":{"role":"assistant","provider":"openrouter","model":"claude-sonnet-4-6","stopReason":"aborted","errorMessage":"user cancel","usage":{"input":300,"output":20,"cacheRead":0,"cacheWrite":0}}}"#,
        );
        fs::write(&fork_file, fork_lines).unwrap();

        // OhMyPi session
        let omp_dir = home.join(".omp").join("agent").join("sessions").join("proj-beta");
        fs::create_dir_all(&omp_dir).unwrap();
        fs::write(
            omp_dir.join("omp-1.jsonl"),
            concat!(
                r#"{"type":"session","id":"omp-sess-1","timestamp":"2026-11-14T11:00:00Z"}"#, "\n",
                r#"{"type":"message","id":"omp-e1","timestamp":"2026-11-14T11:00:10Z","message":{"role":"assistant","provider":"openai","model":"gpt-4o","usage":{"input":100,"output":20,"cacheRead":0,"cacheWrite":0}}}"#, "\n",
            ),
        )
        .unwrap();

        let rep = db.sync_session_usage(&paths).unwrap();
        assert_eq!(rep.imported, 6, "Expected 4 Pi parent + 1 Pi fork new + 1 OhMyPi = 6");
        assert!(rep.skipped >= 1, "Forked repeat entry should be skipped");

        let conn = db.connect().unwrap();
        let pi_sum = db
            .get_usage_summary(None, None, Some("pi"), None, None)
            .unwrap();
        assert_eq!(pi_sum.total_requests, 5);
        // Verify RFC3339 timestamp parsed cleanly (not fallback current date)
        let ts: i64 = conn
            .query_row(
                "SELECT created_at FROM proxy_request_logs WHERE app_type='pi' AND model='anthropic/claude-sonnet-4-6'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(ts, 1794650460, "2026-11-14T10:01:00Z == 1794650460");

        // Verify explicit cost honored & aborted recorded with 499
        let aborted_status: i64 = conn
            .query_row(
                "SELECT status_code FROM proxy_request_logs WHERE app_type='pi' AND error_message='user cancel'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(aborted_status, 499);

        // Incremental append to parent_file even keeping same timestamp or new line
        let mut f = std::fs::OpenOptions::new().append(true).open(&parent_file).unwrap();
        use std::io::Write;
        writeln!(
            f,
            "{}",
            r#"{"type":"message","id":"entry-e","timestamp":"2026-11-14T10:07:00Z","message":{"role":"assistant","provider":"openrouter","model":"gpt-4o","usage":{"input":400,"output":80,"cacheRead":0,"cacheWrite":0}}}"#
        )
        .unwrap();
        drop(f);

        let rep2 = db.sync_session_usage(&paths).unwrap();
        assert_eq!(rep2.imported, 1, "Appended entry should import incrementally");
    }

    #[test]
    fn test_claude_streaming_subagents_and_legacy_dedup() {
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        let app_data = tmp.path().join("appdata");
        let paths = Paths::new(home.clone(), app_data);
        let db = UsageDb::open(&paths).unwrap();

        // Pre-seed an older `claude:msg-legacy` row (simulating older aitoolplus version)
        {
            let conn = db.connect().unwrap();
            conn.execute(
                "INSERT INTO proxy_request_logs (request_id, provider_id, app_type, model, input_tokens, output_tokens, total_cost_usd, status_code, created_at, data_source)
                 VALUES ('claude:msg-legacy', '_session', 'claude', 'claude-sonnet-4-6', 100, 10, '0.000450', 200, 1794650000, 'session_log')",
                [],
            )
            .unwrap();
        }

        let proj = home.join(".claude").join("projects").join("proj-x");
        let wf = proj.join("sess-1").join("subagents").join("workflows").join("wf_01");
        fs::create_dir_all(&wf).unwrap();

        fs::write(
            proj.join("sess-1.jsonl"),
            concat!(
                // Legacy partial row upgraded from 10 -> 90 output tokens, without creating duplicate `session:msg-legacy`
                r#"{"type":"assistant","sessionId":"sess-1","timestamp":"2026-11-14T09:53:20Z","message":{"id":"msg-legacy","model":"claude-sonnet-4-6","stop_reason":"end_turn","usage":{"input_tokens":100,"output_tokens":90,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}}"#, "\n",
                // Skipped synthetic & API error
                r#"{"type":"assistant","sessionId":"sess-1","timestamp":"2026-11-14T09:54:00Z","message":{"id":"msg-syn","model":"<synthetic>","usage":{"input_tokens":10,"output_tokens":10}}}"#, "\n",
                // Streaming partial followed by final chunk with 1h cache creation
                r#"{"type":"assistant","sessionId":"sess-1","timestamp":"2026-11-14T09:55:00Z","message":{"id":"msg-stream","model":"claude-sonnet-4-6","stop_reason":null,"usage":{"input_tokens":1000,"output_tokens":5,"cache_read_input_tokens":0,"cache_creation_input_tokens":1000}}}"#, "\n",
                r#"{"type":"assistant","sessionId":"sess-1","timestamp":"2026-11-14T09:55:05Z","message":{"id":"msg-stream","model":"claude-sonnet-4-6","stop_reason":"end_turn","usage":{"input_tokens":1000,"output_tokens":200,"cache_read_input_tokens":500,"cache_creation_input_tokens":1000,"cache_creation":{"ephemeral_1h_input_tokens":1000}}}}"#, "\n",
            ),
        )
        .unwrap();

        // Workflow subagent log
        fs::write(
            wf.join("agent-1.jsonl"),
            concat!(
                r#"{"type":"assistant","sessionId":"sess-1","timestamp":"2026-11-14T09:56:00Z","message":{"id":"msg-sub","model":"claude-sonnet-4-6","stop_reason":"end_turn","usage":{"input_tokens":200,"output_tokens":40,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}}"#, "\n",
            ),
        )
        .unwrap();

        let rep = db.sync_session_usage(&paths).unwrap();
        assert_eq!(rep.imported, 2, "msg-stream + msg-sub imported; msg-legacy updated in place");

        let conn = db.connect().unwrap();
        let claude_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM proxy_request_logs WHERE app_type='claude'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(claude_count, 3, "No duplicate row for claude:msg-legacy vs session:msg-legacy");

        let updated_out: i64 = conn
            .query_row("SELECT output_tokens FROM proxy_request_logs WHERE request_id='claude:msg-legacy'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(updated_out, 90);

        // Verify 1h cache creation tier (1.6x * 3.75 = 6.00 per M):
        // inp: 1000*3/1M=0.003, out: 200*15/1M=0.003, cr: 500*0.3/1M=0.00015, cc_1h: 1000*3.75*1.6/1M=0.006 -> 0.012150
        let stream_cost: String = conn
            .query_row("SELECT total_cost_usd FROM proxy_request_logs WHERE request_id='session:msg-stream'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(stream_cost, "0.012150");
    }

    #[test]
    fn test_codex_delta_cached_subtraction_and_fork_replay() {
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        let app_data = tmp.path().join("appdata");
        let paths = Paths::new(home.clone(), app_data);
        let db = UsageDb::open(&paths).unwrap();

        let parent_uuid = "01a05c2c-d4b7-7e72-97a3-a4c70ec59855";
        let child_uuid = "01a05c2c-d4b7-7e72-97a3-a4c70ec59999";
        let codex_dir = home.join(".codex").join("sessions").join("2026").join("11").join("14");
        fs::create_dir_all(&codex_dir).unwrap();

        let parent_path = codex_dir.join(format!("rollout-2026-11-14T10-00-00-{parent_uuid}.jsonl"));
        fs::write(
            &parent_path,
            concat!(
                r#"{"timestamp":"2026-11-14T10:00:00Z","type":"session_meta","payload":{"id":"01a05c2c-d4b7-7e72-97a3-a4c70ec59855"}}"#, "\n",
                r#"{"timestamp":"2026-11-14T10:00:01Z","type":"turn_context","payload":{"model":"gpt-4o"}}"#, "\n",
                // Turn 1: total input=1000 (cached=400 -> fresh=600), output=100
                r#"{"timestamp":"2026-11-14T10:00:05Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1000,"cached_input_tokens":400,"output_tokens":100}}}}"#, "\n",
                // Duplicate UI snapshot with identical totals -> must be skipped
                r#"{"timestamp":"2026-11-14T10:00:06Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1000,"cached_input_tokens":400,"output_tokens":100}}}}"#, "\n",
                // Turn 2: total input=1800 (delta input=800, delta cached=300 -> fresh=500), delta output=50
                r#"{"timestamp":"2026-11-14T10:00:10Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1800,"cached_input_tokens":700,"output_tokens":150}}}}"#, "\n",
            ),
        )
        .unwrap();

        // Child rollout forked at 10:00:08 -> inherits turn 1 replay, plus adds own turn
        let child_path = codex_dir.join(format!("rollout-2026-11-14T10-00-08-{child_uuid}.jsonl"));
        fs::write(
            &child_path,
            concat!(
                r#"{"timestamp":"2026-11-14T10:00:08Z","type":"session_meta","payload":{"id":"01a05c2c-d4b7-7e72-97a3-a4c70ec59999","forked_from_id":"01a05c2c-d4b7-7e72-97a3-a4c70ec59855"}}"#, "\n",
                r#"{"timestamp":"2026-11-14T10:00:08Z","type":"turn_context","payload":{"model":"gpt-4o"}}"#, "\n",
                r#"{"timestamp":"2026-11-14T10:00:08Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1000,"cached_input_tokens":400,"output_tokens":100}}}}"#, "\n",
                r#"{"timestamp":"2026-11-14T10:00:20Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1400,"cached_input_tokens":600,"output_tokens":120}}}}"#, "\n",
            ),
        )
        .unwrap();

        let rep = db.sync_session_usage(&paths).unwrap();
        assert_eq!(rep.imported, 3, "2 parent turns + 1 non-replayed child turn");

        let sum = db.get_usage_summary(None, None, Some("codex"), None, None).unwrap();
        // Turn 1 fresh=600, Turn 2 fresh=500, Child turn fresh=200 => total fresh=1300; cached=400+300+200=900; output=100+50+20=170
        assert_eq!(sum.total_input_tokens, 1300);
        assert_eq!(sum.total_cache_read_tokens, 900);
        assert_eq!(sum.total_output_tokens, 170);
    }

    #[test]
    fn test_all_remaining_agents_ingestion_and_dedup() {
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        let app_data = tmp.path().join("appdata");
        let paths = Paths::new(home.clone(), app_data);
        let db = UsageDb::open(&paths).unwrap();

        // Pre-seed legacy Gemini row
        {
            let conn = db.connect().unwrap();
            conn.execute(
                "INSERT INTO proxy_request_logs (request_id, provider_id, app_type, model, input_tokens, output_tokens, cache_read_tokens, total_cost_usd, status_code, created_at, data_source)
                 VALUES ('gemini:gem-legacy', '_gemini_session', 'gemini', 'gemini-2.0-flash', 100, 10, 0, '0.000014', 200, 1794650000, 'gemini_session')",
                [],
            )
            .unwrap();
        }

        // 1. Gemini CLI (.json + .jsonl)
        let chats = home.join(".gemini").join("tmp").join("proj-g").join("chats");
        fs::create_dir_all(&chats).unwrap();
        fs::write(
            chats.join("session-1.json"),
            r#"{"sessionId":"gem-s1","messages":[{"id":"gem-legacy","type":"gemini","model":"gemini-2.0-flash","timestamp":"2026-11-14T08:00:00Z","tokens":{"input":100,"output":5,"thoughts":5,"cached":0}},{"id":"gem-2","type":"gemini","model":"gemini-2.0-flash","timestamp":"2026-11-14T08:01:00Z","tokens":{"input":500,"output":40,"thoughts":60,"cached":200}}]}"#,
        )
        .unwrap();
        fs::write(
            chats.join("session-2.jsonl"),
            r#"{"sessionId":"gem-s2","id":"gem-3","type":"gemini","model":"gemini-2.0-flash","timestamp":"2026-11-14T08:02:00Z","tokens":{"input":300,"output":20,"thoughts":10,"cached":100}}"#,
        )
        .unwrap();

        // 2. OpenCode SQLite DB
        let oc_dir = home.join(".local").join("share").join("opencode");
        fs::create_dir_all(&oc_dir).unwrap();
        let oc_db = oc_dir.join("opencode.db");
        {
            let oc_conn = Connection::open(&oc_db).unwrap();
            oc_conn
                .execute(
                    "CREATE TABLE message (id TEXT PRIMARY KEY, session_id TEXT, time_created INTEGER, data TEXT)",
                    [],
                )
                .unwrap();
            oc_conn
                .execute(
                    "INSERT INTO message VALUES ('oc-m1', 'oc-s1', 1794651000000, ?)",
                    params![r#"{"role":"assistant","providerID":"deepseek","modelID":"deepseek-chat","cost":0.0025,"time":{"created":1794651000000,"completed":1794651005000},"tokens":{"input":500,"output":80,"reasoning":20,"cache":{"read":100,"write":0}}}"#],
                )
                .unwrap();
        }

        // 3. Grok updates.jsonl (plus skipped subagents dir)
        let grok_sess = home.join(".grok").join("sessions").join("grok-s1");
        let grok_sub = grok_sess.join("subagents");
        fs::create_dir_all(&grok_sub).unwrap();
        let grok_line = r#"{"type":"_x.ai/session/update","timestamp":"2026-11-14T08:10:00Z","update":{"type":"turn_completed","turnId":"t1","modelUsage":{"grok-3":{"inputTokens":1000,"cachedInputTokens":300,"outputTokens":100,"reasoningTokens":20,"costUsdTicks":12300000}}}}"#;
        fs::write(grok_sess.join("updates.jsonl"), format!("{grok_line}\n")).unwrap();
        fs::write(grok_sub.join("updates.jsonl"), format!("{grok_line}\n")).unwrap();

        // 4. Kimi
        let kimi_sess = home.join(".kimi").join("sessions").join("kimi-s1");
        fs::create_dir_all(&kimi_sess).unwrap();
        fs::write(
            kimi_sess.join("wire.jsonl"),
            r#"{"id":"k1","timestamp":"2026-11-14T08:15:00Z","model":"kimi-k2.5","token_usage":{"input_other":400,"output":50,"input_cache_read":100,"input_cache_creation":0}}"#,
        )
        .unwrap();

        // 5. OpenClaw
        let claw_sess = home.join(".openclaw").join("agents").join("main").join("sessions");
        fs::create_dir_all(&claw_sess).unwrap();
        fs::write(
            claw_sess.join("claw-1.jsonl"),
            concat!(
                r#"{"type":"session","id":"claw-s1"}"#, "\n",
                r#"{"id":"c1","timestamp":"2026-11-14T08:20:00Z","message":{"role":"assistant","provider":"openai","model":"gpt-4o","usage":{"input":200,"output":40,"cacheRead":0,"cacheWrite":0,"cost":{"total":0.001}}}}"#, "\n",
            ),
        )
        .unwrap();

        // 6. Hermes state.db
        let hermes_dir = home.join(".hermes");
        fs::create_dir_all(&hermes_dir).unwrap();
        {
            let h_conn = Connection::open(hermes_dir.join("state.db")).unwrap();
            h_conn
                .execute(
                    "CREATE TABLE sessions (id TEXT PRIMARY KEY, model TEXT, input_tokens INTEGER, output_tokens INTEGER, cache_read_tokens INTEGER, cache_write_tokens INTEGER, estimated_cost_usd REAL, started_at INTEGER)",
                    [],
                )
                .unwrap();
            h_conn
                .execute(
                    "INSERT INTO sessions VALUES ('h-s1', 'deepseek-chat', 600, 120, 50, 0, 0.0015, 1794652000)",
                    [],
                )
                .unwrap();
        }

        // 7. Dsh
        let dsh_dir = home.join(".dsh").join("sessions");
        fs::create_dir_all(&dsh_dir).unwrap();
        fs::write(
            dsh_dir.join("dsh-1.jsonl"),
            r#"{"id":"d1","model":"deepseek-chat","timestamp":"2026-11-14T08:30:00Z","usage":{"prompt_tokens":300,"completion_tokens":60,"prompt_cache_hit_tokens":40}}"#,
        )
        .unwrap();

        let rep = db.sync_session_usage(&paths).unwrap();
        assert_eq!(rep.imported, 8, "2 Gemini + 1 OpenCode + 1 Grok + 1 Kimi + 1 OpenClaw + 1 Hermes + 1 Dsh = 8");

        let gem = db.get_usage_summary(None, None, Some("gemini"), None, None).unwrap();
        // Pre-seeded 100 fresh + gem-2 (500-200=300 fresh, out=100, cr=200) + gem-3 (300-100=200 fresh, out=30, cr=100)
        assert_eq!(gem.total_requests, 3);
        assert_eq!(gem.total_input_tokens, 600);
        assert_eq!(gem.total_output_tokens, 140);
        assert_eq!(gem.total_cache_read_tokens, 300);

        let oc = db.get_usage_summary(None, None, Some("opencode"), None, None).unwrap();
        assert_eq!(oc.total_requests, 1);
        assert_eq!(oc.total_cost, "0.002500");

        let grok = db.get_usage_summary(None, None, Some("grok"), None, None).unwrap();
        assert_eq!(grok.total_requests, 1);
        assert_eq!(grok.total_input_tokens, 700);
        assert_eq!(grok.total_output_tokens, 120);
        assert_eq!(grok.total_cost, "0.001230");

        for app in ["kimi", "openclaw", "hermes", "dsh"] {
            let s = db.get_usage_summary(None, None, Some(app), None, None).unwrap();
            assert_eq!(s.total_requests, 1, "Expected 1 row for {app}");
        }
    }
}
