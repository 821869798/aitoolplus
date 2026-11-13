//! Cross-agent SQLite FTS5 full-text session search index.
//!
//! Features:
//! - Independent cache database `<app_data>/session-index.db` (excluded from backups/sync).
//! - Incremental sync keyed by `(provider_id, source_path)` and file `(modified_ns, len)`.
//! - Trigram FTS5 substring indexing for >= 3-char queries (URLs, code identifiers, CJK)
//!   with exact `instr(normalized, ?)` fallback for 1–2-char queries and multi-keyword AND.
//! - Uniformly supports every agent session parsed via `crate::session::load_messages`,
//!   including Claude Code, Codex, Gemini CLI, OpenCode, Hermes, Pi, Kimi, Grok, Antigravity, etc.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::UNIX_EPOCH;

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::paths::Paths;
use crate::session::{self, SessionMessage, SessionMeta};

const INDEX_FILE: &str = "session-index.db";
/// Bump whenever schema or extraction rules change to trigger clean automatic rebuild.
const INDEX_VERSION: i64 = 1;
/// Reserve lower 24 bits of SQLite rowid for `message_index` (up to 16M messages per session).
const ROW_SHIFT: u32 = 24;
const MAX_MESSAGES_PER_SOURCE: usize = 1 << ROW_SHIFT;
const MAX_ROW_CHARS: usize = 20_000;
const MAX_HIT_ROWS: i64 = 20_000;
pub const DEFAULT_SEARCH_LIMIT: usize = 300;
const MAX_SNIPPETS_PER_SESSION: usize = 3;
const MIN_MATCH_CHARS: usize = 3;
const SNIPPET_BEFORE: usize = 30;
const SNIPPET_AFTER: usize = 90;
const COMPACT_AFTER_CHANGES: usize = 50;
const PROGRESS_EVERY: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentHighlight {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentSnippet {
    pub message_index: usize,
    pub text: String,
    pub highlights: Vec<ContentHighlight>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentHit {
    pub provider_id: String,
    pub source_path: String,
    pub snippets: Vec<ContentSnippet>,
    pub match_count: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StoredFingerprint {
    pub modified_ns: Option<i64>,
    pub len: i64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SyncStats {
    pub total: usize,
    pub reindexed: usize,
    pub removed: usize,
}

static DB_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

pub fn index_db_path(paths: &Paths) -> PathBuf {
    paths.app_data.join(INDEX_FILE)
}

/// Extract searchable user/assistant plain-text rows `(message_index, text)` from parsed messages.
pub fn extract_message_rows(messages: &[SessionMessage]) -> Vec<(usize, String)> {
    messages
        .iter()
        .enumerate()
        .take(MAX_MESSAGES_PER_SOURCE)
        .filter(|(_, msg)| matches!(msg.role.as_str(), "user" | "assistant"))
        .filter_map(|(idx, msg)| {
            let text_from_blocks: String = msg
                .blocks
                .iter()
                .filter(|b| b.kind == "text")
                .filter_map(|b| b.text.as_deref())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("\n");

            let raw = if !text_from_blocks.is_empty() {
                text_from_blocks
            } else if msg.blocks.is_empty()
                || msg
                    .message_type
                    .as_deref()
                    .is_none_or(|t| t == "text" || t.is_empty())
            {
                msg.content.trim().to_string()
            } else {
                String::new()
            };

            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return None;
            }
            Some((idx, truncate_chars(trimmed, MAX_ROW_CHARS)))
        })
        .collect()
}

fn truncate_chars(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((cut, _)) => text[..cut].to_string(),
        None => text.to_string(),
    }
}

/// Build a concise context snippet around the first match in `text`.
pub fn make_highlighted_snippet(text: &str, query: &str) -> (String, Vec<ContentHighlight>) {
    let hay: Vec<char> = text.chars().collect();
    let normalized = text.to_lowercase();
    let tokens: Vec<String> = {
        let needle = query.trim().to_lowercase();
        if !needle.is_empty() && normalized.contains(&needle) {
            vec![needle]
        } else {
            query
                .split_whitespace()
                .map(|w| w.to_lowercase())
                .filter(|w| !w.is_empty())
                .collect()
        }
    };

    let original_positions: Vec<usize> = hay
        .iter()
        .enumerate()
        .flat_map(|(index, c)| std::iter::repeat_n(index, c.to_lowercase().count()))
        .collect();
    let mut matched = vec![false; hay.len()];
    let mut first: Option<(usize, usize)> = None;

    for needle in tokens {
        let needle_chars = needle.chars().count();
        if needle_chars == 0 {
            continue;
        }
        let mut previous_byte = 0;
        let mut normalized_at = 0;
        for (byte, _) in normalized.match_indices(&needle) {
            normalized_at += normalized[previous_byte..byte].chars().count();
            previous_byte = byte;
            let Some(&start) = original_positions.get(normalized_at) else {
                continue;
            };
            let Some(&end_inclusive) =
                original_positions.get(normalized_at + needle_chars - 1)
            else {
                continue;
            };
            let end = (end_inclusive + 1).min(hay.len());
            if first.is_none_or(|(fs, _)| start < fs) {
                first = Some((start, end));
            }
            matched[start..end].fill(true);
        }
    }

    let (start, end) = match first {
        Some((start, end)) => (
            start.saturating_sub(SNIPPET_BEFORE),
            (end + SNIPPET_AFTER).min(hay.len()),
        ),
        None => (0, (SNIPPET_BEFORE + SNIPPET_AFTER).min(hay.len())),
    };

    let mut snippet = String::new();
    let mut highlights: Vec<ContentHighlight> = Vec::new();
    let mut utf16_at = 0usize;
    let mut append = |c: char, highlighted: bool| {
        let end = utf16_at + c.len_utf16();
        snippet.push(c);
        if highlighted {
            if let Some(prev) = highlights.last_mut().filter(|r| r.end == utf16_at) {
                prev.end = end;
            } else {
                highlights.push(ContentHighlight {
                    start: utf16_at,
                    end,
                });
            }
        }
        utf16_at = end;
    };

    if start > 0 {
        append('…', false);
    }
    let mut pending_space = false;
    let mut pending_highlight = false;
    let mut started = false;
    for index in start..end {
        let c = hay[index];
        if c.is_whitespace() {
            pending_space = started;
            pending_highlight |= matched[index];
            continue;
        }
        if pending_space {
            append(' ', pending_highlight);
        }
        pending_space = false;
        pending_highlight = false;
        append(c, matched[index]);
        started = true;
    }
    if end < hay.len() {
        append('…', false);
    }
    (snippet, highlights)
}

fn match_expression(query: &str) -> String {
    format!("\"{}\"", query.replace('"', "\"\""))
}

pub fn source_fingerprint(source_path: &str) -> Option<StoredFingerprint> {
    let raw = Path::new(source_path);
    let target = if raw.is_dir() {
        let grok = raw.join("chat_history.jsonl");
        let ag_full = raw
            .join(".system_generated")
            .join("logs")
            .join("transcript_full.jsonl");
        let ag_std = raw
            .join(".system_generated")
            .join("logs")
            .join("transcript.jsonl");
        if grok.is_file() {
            grok
        } else if ag_full.is_file() {
            ag_full
        } else if ag_std.is_file() {
            ag_std
        } else {
            raw.to_path_buf()
        }
    } else {
        raw.to_path_buf()
    };

    if !target.exists()
        || matches!(
            target.extension().and_then(|e| e.to_str()),
            Some("pb" | "db" | "sqlite" | "sqlite3")
        )
    {
        return None;
    }

    let meta = fs::metadata(&target).ok()?;
    let modified_ns = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos().min(i64::MAX as u128) as i64);
    let len = meta.len().min(i64::MAX as u64) as i64;
    Some(StoredFingerprint { modified_ns, len })
}

pub struct SessionIndex {
    conn: Connection,
}

impl SessionIndex {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        match Self::open_raw(path) {
            Ok(idx) => Ok(idx),
            Err(_) => {
                // Cache DB is derived state; remove corrupt files and rebuild cleanly.
                remove_index_files(path);
                Self::open_raw(path)
            }
        }
    }

    fn open_raw(path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        conn.busy_timeout(std::time::Duration::from_millis(3000))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        Self::init(conn)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> rusqlite::Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> rusqlite::Result<Self> {
        let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version != INDEX_VERSION {
            conn.execute_batch(
                "DROP TABLE IF EXISTS message_text;
                 DROP TABLE IF EXISTS indexed_sources;",
            )?;
        }
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS indexed_sources (
                 id INTEGER PRIMARY KEY,
                 provider_id TEXT NOT NULL,
                 source_path TEXT NOT NULL,
                 modified_ns INTEGER,
                 len INTEGER NOT NULL,
                 UNIQUE (provider_id, source_path)
             );
             CREATE VIRTUAL TABLE IF NOT EXISTS message_text
                 USING fts5(text UNINDEXED, normalized, tokenize = 'trigram case_sensitive 1');",
        )?;
        conn.pragma_update(None, "user_version", INDEX_VERSION)?;
        Ok(Self { conn })
    }

    pub fn is_empty(&self) -> bool {
        self.conn
            .query_row(
                "SELECT NOT EXISTS(SELECT 1 FROM indexed_sources)",
                [],
                |row| row.get(0),
            )
            .unwrap_or(true)
    }

    pub fn stored_fingerprints_for_providers(
        &self,
        providers: &[String],
    ) -> rusqlite::Result<HashMap<(String, String), StoredFingerprint>> {
        let mut map = HashMap::new();
        let allow: HashSet<&str> = providers.iter().map(String::as_str).collect();
        let mut stmt = self.conn.prepare(
            "SELECT provider_id, source_path, modified_ns, len FROM indexed_sources",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                StoredFingerprint {
                    modified_ns: row.get(2)?,
                    len: row.get(3)?,
                },
            ))
        })?;
        for row in rows.flatten() {
            if allow.is_empty() || allow.contains(row.0.as_str()) {
                map.insert((row.0, row.1), row.2);
            }
        }
        Ok(map)
    }

    pub fn replace_source(
        &mut self,
        provider_id: &str,
        source_path: &str,
        fingerprint: StoredFingerprint,
        rows: &[(usize, String)],
    ) -> rusqlite::Result<()> {
        let tx = self.conn.transaction()?;
        let existing: Option<i64> = tx
            .query_row(
                "SELECT id FROM indexed_sources WHERE provider_id = ?1 AND source_path = ?2",
                params![provider_id, source_path],
                |row| row.get(0),
            )
            .optional()?;
        let id = match existing {
            Some(id) => {
                delete_rows(&tx, id)?;
                tx.execute(
                    "UPDATE indexed_sources SET modified_ns = ?1, len = ?2 WHERE id = ?3",
                    params![fingerprint.modified_ns, fingerprint.len, id],
                )?;
                id
            }
            None => {
                tx.execute(
                    "INSERT INTO indexed_sources (provider_id, source_path, modified_ns, len)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![
                        provider_id,
                        source_path,
                        fingerprint.modified_ns,
                        fingerprint.len
                    ],
                )?;
                tx.last_insert_rowid()
            }
        };
        {
            let mut insert = tx.prepare(
                "INSERT INTO message_text (rowid, text, normalized) VALUES (?1, ?2, ?3)",
            )?;
            for (index, text) in rows {
                insert.execute(params![
                    (id << ROW_SHIFT) + *index as i64,
                    text,
                    text.to_lowercase()
                ])?;
            }
        }
        tx.commit()
    }

    pub fn remove_source(&mut self, provider_id: &str, source_path: &str) -> rusqlite::Result<bool> {
        let tx = self.conn.transaction()?;
        let existing: Option<i64> = tx
            .query_row(
                "SELECT id FROM indexed_sources WHERE provider_id = ?1 AND source_path = ?2",
                params![provider_id, source_path],
                |row| row.get(0),
            )
            .optional()?;
        let removed = if let Some(id) = existing {
            delete_rows(&tx, id)?;
            tx.execute("DELETE FROM indexed_sources WHERE id = ?1", params![id])?;
            true
        } else {
            false
        };
        tx.commit()?;
        Ok(removed)
    }

    pub fn retain_provider_sources(
        &mut self,
        providers: &[String],
        keep: &HashSet<(String, String)>,
    ) -> rusqlite::Result<usize> {
        let allow: HashSet<&str> = providers.iter().map(String::as_str).collect();
        let stale: Vec<(String, String)> = {
            let mut stmt = self
                .conn
                .prepare("SELECT provider_id, source_path FROM indexed_sources")?;
            let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;
            rows.filter_map(Result::ok)
                .filter(|(p, _)| allow.contains(p.as_str()))
                .filter(|key| !keep.contains(key))
                .collect()
        };
        let count = stale.len();
        for (provider_id, source_path) in stale {
            self.remove_source(&provider_id, &source_path)?;
        }
        Ok(count)
    }

    pub fn compact(&self) -> rusqlite::Result<()> {
        self.conn
            .execute("INSERT INTO message_text (message_text) VALUES ('optimize')", [])?;
        self.conn.execute_batch("VACUUM;")?;
        self.checkpoint()
    }

    pub fn checkpoint(&self) -> rusqlite::Result<()> {
        self.conn
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
    }

    pub fn search(
        &self,
        query: &str,
        providers: Option<&[String]>,
        limit: usize,
    ) -> rusqlite::Result<Vec<ContentHit>> {
        let query = query.trim();
        if query.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }

        let sources: HashMap<i64, (String, String)> = {
            let mut stmt = self
                .conn
                .prepare("SELECT id, provider_id, source_path FROM indexed_sources")?;
            let rows = stmt.query_map([], |row| Ok((row.get(0)?, (row.get(1)?, row.get(2)?))))?;
            rows.collect::<rusqlite::Result<_>>()?
        };

        let allowed_ids: Option<HashSet<i64>> = providers.map(|list| {
            let set: HashSet<&str> = list.iter().map(String::as_str).collect();
            sources
                .iter()
                .filter(|(_, (pid, _))| set.contains(pid.as_str()))
                .map(|(id, _)| *id)
                .collect()
        });
        if allowed_ids.as_ref().is_some_and(HashSet::is_empty) {
            return Ok(Vec::new());
        }

        let normalized = query.to_lowercase();
        let mut rowids = self.query_rowids(&normalized, allowed_ids.as_ref())?;

        // Multi-keyword fallback: when user enters space-separated words like
        // `github dart_simple_live`, match rows containing every token if the exact phrase had no hits.
        if rowids.is_empty() {
            let tokens: Vec<String> = normalized
                .split_whitespace()
                .map(str::to_string)
                .filter(|t| !t.is_empty())
                .collect();
            if tokens.len() > 1 {
                rowids = self.query_multi_token_rowids(&tokens, allowed_ids.as_ref())?;
            }
        }

        if rowids.is_empty() {
            return Ok(Vec::new());
        }

        let mut order: Vec<i64> = Vec::new();
        let mut best: HashMap<i64, (Vec<i64>, usize)> = HashMap::new();
        for rowid in rowids {
            let source_id = rowid >> ROW_SHIFT;
            if !sources.contains_key(&source_id) {
                continue;
            }
            let (top, count) = best.entry(source_id).or_insert_with(|| {
                order.push(source_id);
                (Vec::new(), 0)
            });
            *count += 1;
            if top.len() < MAX_SNIPPETS_PER_SESSION {
                top.push(rowid);
            }
        }

        let mut text_stmt = self
            .conn
            .prepare("SELECT text FROM message_text WHERE rowid = ?1")?;
        let mut hits = Vec::new();
        for source_id in order.into_iter().take(limit) {
            let (top, match_count) = &best[&source_id];
            let (provider_id, source_path) = &sources[&source_id];
            let snippets = top
                .iter()
                .map(|&rowid| {
                    let text: String = text_stmt.query_row(params![rowid], |row| row.get(0))?;
                    let (snippet_text, highlights) = make_highlighted_snippet(&text, query);
                    Ok(ContentSnippet {
                        message_index: (rowid & ((1 << ROW_SHIFT) - 1)) as usize,
                        text: snippet_text,
                        highlights,
                    })
                })
                .collect::<rusqlite::Result<_>>()?;
            hits.push(ContentHit {
                provider_id: provider_id.clone(),
                source_path: source_path.clone(),
                snippets,
                match_count: *match_count,
            });
        }
        Ok(hits)
    }

    fn query_rowids(
        &self,
        normalized: &str,
        allowed_ids: Option<&HashSet<i64>>,
    ) -> rusqlite::Result<Vec<i64>> {
        let mut out = Vec::new();
        if normalized.chars().count() >= MIN_MATCH_CHARS {
            let mut stmt = self.conn.prepare(
                "SELECT rowid FROM message_text WHERE message_text MATCH ?1 ORDER BY rank LIMIT ?2",
            )?;
            let rows = stmt.query_map(
                params![match_expression(normalized), MAX_HIT_ROWS],
                |row| row.get::<_, i64>(0),
            )?;
            for rowid in rows.flatten() {
                if allowed_ids.is_none_or(|set| set.contains(&(rowid >> ROW_SHIFT))) {
                    out.push(rowid);
                }
            }
        } else {
            let mut stmt = self.conn.prepare(
                "SELECT rowid FROM message_text WHERE instr(normalized, ?1) > 0 ORDER BY rowid DESC LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![normalized, MAX_HIT_ROWS], |row| {
                row.get::<_, i64>(0)
            })?;
            for rowid in rows.flatten() {
                if allowed_ids.is_none_or(|set| set.contains(&(rowid >> ROW_SHIFT))) {
                    out.push(rowid);
                }
            }
        }
        Ok(out)
    }

    fn query_multi_token_rowids(
        &self,
        tokens: &[String],
        allowed_ids: Option<&HashSet<i64>>,
    ) -> rusqlite::Result<Vec<i64>> {
        // Prefer driving with the longest token >= 3 chars using FTS5, then verify remaining tokens.
        let longest_fts = tokens
            .iter()
            .filter(|t| t.chars().count() >= MIN_MATCH_CHARS)
            .max_by_key(|t| t.chars().count())
            .cloned();

        let candidates: Vec<(i64, String)> = if let Some(anchor) = longest_fts {
            let mut stmt = self.conn.prepare(
                "SELECT rowid, normalized FROM message_text WHERE message_text MATCH ?1 ORDER BY rank LIMIT ?2",
            )?;
            let rows = stmt.query_map(
                params![match_expression(&anchor), MAX_HIT_ROWS],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )?;
            rows.flatten().collect()
        } else {
            let mut stmt = self.conn.prepare(
                "SELECT rowid, normalized FROM message_text WHERE instr(normalized, ?1) > 0 ORDER BY rowid DESC LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![tokens[0], MAX_HIT_ROWS], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })?;
            rows.flatten().collect()
        };

        Ok(candidates
            .into_iter()
            .filter(|(rowid, norm)| {
                allowed_ids.is_none_or(|set| set.contains(&(rowid >> ROW_SHIFT)))
                    && tokens.iter().all(|tok| norm.contains(tok.as_str()))
            })
            .map(|(rowid, _)| rowid)
            .collect())
    }
}

fn delete_rows(conn: &Connection, source_id: i64) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM message_text WHERE rowid >= ?1 AND rowid < ?2",
        params![source_id << ROW_SHIFT, (source_id + 1) << ROW_SHIFT],
    )?;
    Ok(())
}

fn remove_index_files(path: &Path) {
    for suffix in ["", "-wal", "-shm"] {
        let mut p = path.as_os_str().to_owned();
        p.push(suffix);
        let _ = fs::remove_file(Path::new(&p));
    }
}

type PendingWrite<'a> = (&'a SessionMeta, StoredFingerprint, Vec<(usize, String)>);

/// Incrementally synchronize FTS5 message index for `sessions` belonging to `scope_providers`.
pub fn sync_sessions_with_progress<F>(
    paths: &Paths,
    scope_providers: &[String],
    sessions: &[SessionMeta],
    mut on_progress: F,
) -> Result<SyncStats, String>
where
    F: FnMut(usize, usize),
{
    let db_path = index_db_path(paths);
    let mut dedup: HashMap<(String, String), &SessionMeta> = HashMap::new();
    for s in sessions {
        dedup
            .entry((s.provider_id.clone(), s.source_path.clone()))
            .or_insert(s);
    }
    let unique_sessions: Vec<&SessionMeta> = dedup.into_values().collect();
    let total = unique_sessions.len();
    on_progress(0, total);

    let (initially_empty, mut stored_fps, removed) = {
        let _guard = DB_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut index = SessionIndex::open(&db_path).map_err(|e| e.to_string())?;
        let keep: HashSet<(String, String)> = unique_sessions
            .iter()
            .map(|s| (s.provider_id.clone(), s.source_path.clone()))
            .collect();
        let removed = index
            .retain_provider_sources(scope_providers, &keep)
            .map_err(|e| e.to_string())?;
        let empty = index.is_empty();
        let fps = index
            .stored_fingerprints_for_providers(scope_providers)
            .map_err(|e| e.to_string())?;
        (empty, fps, removed)
    };

    let mut reindexed = 0usize;
    let mut pending_writes: Vec<PendingWrite<'_>> = Vec::new();

    for (idx, meta) in unique_sessions.iter().enumerate() {
        let Some(fp) = source_fingerprint(&meta.source_path) else {
            if (idx + 1) % PROGRESS_EVERY == 0 {
                on_progress(idx + 1, total);
            }
            continue;
        };
        let key = (meta.provider_id.clone(), meta.source_path.clone());
        if stored_fps.get(&key).copied() == Some(fp) {
            if (idx + 1) % PROGRESS_EVERY == 0 {
                on_progress(idx + 1, total);
            }
            continue;
        }
        stored_fps.insert(key, fp);
        if let Ok(messages) = session::load_messages(paths, meta) {
            let rows = extract_message_rows(&messages);
            pending_writes.push((meta, fp, rows));
        }

        if pending_writes.len() >= 10 {
            reindexed += flush_pending(&db_path, &mut pending_writes)?;
        }
        if (idx + 1) % PROGRESS_EVERY == 0 {
            on_progress(idx + 1, total);
        }
    }

    if !pending_writes.is_empty() {
        reindexed += flush_pending(&db_path, &mut pending_writes)?;
    }

    if reindexed > 0 || removed > 0 {
        let _guard = DB_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        if let Ok(index) = SessionIndex::open(&db_path) {
            if initially_empty && reindexed >= COMPACT_AFTER_CHANGES {
                let _ = index.compact();
            } else {
                let _ = index.checkpoint();
            }
        }
    }

    on_progress(total, total);
    Ok(SyncStats {
        total,
        reindexed,
        removed,
    })
}

fn flush_pending(
    db_path: &Path,
    pending: &mut Vec<PendingWrite<'_>>,
) -> Result<usize, String> {
    let _guard = DB_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut index = SessionIndex::open(db_path).map_err(|e| e.to_string())?;
    let mut count = 0usize;
    for (meta, fp, rows) in pending.drain(..) {
        index
            .replace_source(&meta.provider_id, &meta.source_path, fp, &rows)
            .map_err(|e| e.to_string())?;
        count += 1;
    }
    Ok(count)
}

/// Query full-text session index across `scope_providers` (or all providers if `None`).
pub fn search_sessions_content(
    paths: &Paths,
    scope_providers: Option<&[String]>,
    query: &str,
    limit: usize,
) -> Result<Vec<ContentHit>, String> {
    let db_path = index_db_path(paths);
    if !db_path.is_file() || query.trim().is_empty() {
        return Ok(Vec::new());
    }
    let _guard = DB_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let index = SessionIndex::open(&db_path).map_err(|e| e.to_string())?;
    index
        .search(query, scope_providers, limit)
        .map_err(|e| e.to_string())
}

/// Immediately forget deleted session sources from the FTS index.
pub fn forget_indexed_sources(paths: &Paths, items: &[(String, String)]) {
    if items.is_empty() {
        return;
    }
    let db_path = index_db_path(paths);
    if !db_path.is_file() {
        return;
    }
    let _guard = DB_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if let Ok(mut index) = SessionIndex::open(&db_path) {
        let mut dirty = false;
        for (provider_id, source_path) in items {
            if let Ok(true) = index.remove_source(provider_id, source_path) {
                dirty = true;
            }
        }
        if dirty {
            let _ = index.checkpoint();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::antigravity::scan_antigravity_sessions;

    #[test]
    fn test_fts_url_short_cjk_and_multitoken_search() {
        let mut index = SessionIndex::open_in_memory().unwrap();
        let fp = StoredFingerprint {
            modified_ns: Some(12345),
            len: 500,
        };
        index
            .replace_source(
                "antigravity:app",
                "/tmp/agy/sess-1/transcript_full.jsonl",
                fp,
                &[
                    (0, "帮我看看这个仓库 https://github.com/June6699/dart_simple_live 的播放器实现".into()),
                    (1, "我来检查 Go 和 Rust 的性能差异".into()),
                ],
            )
            .unwrap();
        index
            .replace_source(
                "claude",
                "/tmp/claude/sess-2.jsonl",
                fp,
                &[(0, "Refactoring provider catalog validation logic".into())],
            )
            .unwrap();

        // Exact URL hit (>= 3 chars trigram)
        let hits = index
            .search(
                "https://github.com/June6699/dart_simple_live",
                Some(&["antigravity:app".into(), "antigravity:cli".into()]),
                50,
            )
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].source_path, "/tmp/agy/sess-1/transcript_full.jsonl");
        assert!(hits[0].snippets[0].text.contains("dart_simple_live"));

        // Case-insensitive substring hit
        let ci_hits = index
            .search("DART_SIMPLE_LIVE", None, 50)
            .unwrap();
        assert_eq!(ci_hits.len(), 1);

        // Short (< 3 char) query fallback
        let short_hits = index.search("go", None, 50).unwrap();
        assert_eq!(short_hits.len(), 1);
        assert_eq!(short_hits[0].provider_id, "antigravity:app");

        // Multi-keyword query across same message
        let multi_hits = index
            .search("github dart_simple_live", None, 50)
            .unwrap();
        assert_eq!(multi_hits.len(), 1);

        // Provider filter excludes unmatched providers
        let filtered = index
            .search("dart_simple_live", Some(&["claude".into()]), 50)
            .unwrap();
        assert!(filtered.is_empty());
    }

    #[test]
    fn test_sync_and_search_across_antigravity_and_claude_sessions() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let app_data = temp.path().join("app_data");
        let paths = Paths::new(home.to_path_buf(), app_data);

        // 1. Mock ~/.gemini/antigravity/brain/<cid>/.system_generated/logs/transcript_full.jsonl
        let ag_logs = home
            .join(".gemini")
            .join("antigravity")
            .join("brain")
            .join("ag-conv-1")
            .join(".system_generated")
            .join("logs");
        fs::create_dir_all(&ag_logs).unwrap();
        let ag_transcript = ag_logs.join("transcript_full.jsonl");
        fs::write(
            &ag_transcript,
            concat!(
                r#"{"step_index":0,"type":"USER_INPUT","created_at":"2026-10-01T10:00:00Z","content":"<USER_REQUEST>\n请帮我分析一下这个项目\n</USER_REQUEST>"}"#,
                "\n",
                r#"{"step_index":1,"type":"PLANNER_RESPONSE","created_at":"2026-10-01T10:01:00Z","content":"已定位仓库 https://github.com/June6699/dart_simple_live 中的播放器模块。"}"#,
                "\n"
            ),
        )
        .unwrap();

        // 2. Mock ~/.claude/projects/demo/claude-1.jsonl
        let claude_dir = home.join(".claude").join("projects").join("demo");
        fs::create_dir_all(&claude_dir).unwrap();
        let claude_file = claude_dir.join("claude-1.jsonl");
        fs::write(
            &claude_file,
            concat!(
                r#"{"type":"user","timestamp":"2026-10-02T10:00:00Z","message":{"role":"user","content":"Hello Claude"}}"#,
                "\n",
                r#"{"type":"assistant","timestamp":"2026-10-02T10:01:00Z","message":{"role":"assistant","content":[{"type":"text","text":"Checking https://github.com/June6699/dart_simple_live issue details."}]}}"#,
                "\n"
            ),
        )
        .unwrap();

        let ag_sessions: Vec<SessionMeta> = scan_antigravity_sessions(home, usize::MAX)
            .into_iter()
            .map(Into::into)
            .collect();
        assert_eq!(ag_sessions.len(), 1);
        assert!(ag_sessions[0].source_path.ends_with("transcript_full.jsonl"));

        let stats_ag = sync_sessions_with_progress(
            &paths,
            &["antigravity:app".into(), "antigravity:cli".into()],
            &ag_sessions,
            |_, _| {},
        )
        .unwrap();
        assert_eq!(stats_ag.reindexed, 1);

        // Second sync with unmodified files must skip parsing
        let stats_ag2 = sync_sessions_with_progress(
            &paths,
            &["antigravity:app".into(), "antigravity:cli".into()],
            &ag_sessions,
            |_, _| {},
        )
        .unwrap();
        assert_eq!(stats_ag2.reindexed, 0);

        let claude_sessions = session::scan_all_sessions(&paths, crate::tools::ToolId::ClaudeCode);
        assert_eq!(claude_sessions.len(), 1);
        let claude_key = crate::tools::ToolId::ClaudeCode.key().to_string();
        let stats_cc = sync_sessions_with_progress(
            &paths,
            std::slice::from_ref(&claude_key),
            &claude_sessions,
            |_, _| {},
        )
        .unwrap();
        assert_eq!(stats_cc.reindexed, 1);

        // Search Antigravity only
        let ag_hits = search_sessions_content(
            &paths,
            Some(&["antigravity:app".into(), "antigravity:cli".into()]),
            "https://github.com/June6699/dart_simple_live",
            50,
        )
        .unwrap();
        assert_eq!(ag_hits.len(), 1);
        assert_eq!(ag_hits[0].source_path, ag_sessions[0].source_path);

        // Search Claude only
        let cc_hits = search_sessions_content(
            &paths,
            Some(std::slice::from_ref(&claude_key)),
            "https://github.com/June6699/dart_simple_live",
            50,
        )
        .unwrap();
        assert_eq!(cc_hits.len(), 1);
        assert_eq!(cc_hits[0].source_path, claude_sessions[0].source_path);

        // Forget Antigravity source verifies removal without affecting Claude
        forget_indexed_sources(
            &paths,
            &[(ag_sessions[0].provider_id.clone(), ag_sessions[0].source_path.clone())],
        );
        let ag_after = search_sessions_content(
            &paths,
            Some(&["antigravity:app".into(), "antigravity:cli".into()]),
            "dart_simple_live",
            50,
        )
        .unwrap();
        assert!(ag_after.is_empty());
        let cc_after = search_sessions_content(
            &paths,
            Some(std::slice::from_ref(&claude_key)),
            "dart_simple_live",
            50,
        )
        .unwrap();
        assert_eq!(cc_after.len(), 1);
    }
}
