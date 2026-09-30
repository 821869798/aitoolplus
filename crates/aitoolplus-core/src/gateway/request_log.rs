use std::fs;
use std::path::PathBuf;
use rusqlite::{params, Connection};
use crate::gateway::types::{GatewayRequestLogDetail, GatewayRequestLogSummary};
use crate::paths::Paths;

pub struct RequestLogStore;

impl RequestLogStore {
    fn db_path(paths: &Paths) -> PathBuf {
        paths.gateway_dir().join("gateway.db")
    }

    fn init_db(conn: &Connection) -> Result<(), String> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS gateway_request_logs (
                id TEXT PRIMARY KEY,
                timestamp INTEGER NOT NULL,
                cli_key TEXT NOT NULL,
                route_name TEXT NOT NULL,
                provider_id TEXT NOT NULL,
                provider_name TEXT NOT NULL,
                model TEXT NOT NULL,
                status_code INTEGER NOT NULL,
                duration_ms INTEGER NOT NULL,
                first_token_ms INTEGER,
                input_tokens INTEGER NOT NULL,
                output_tokens INTEGER NOT NULL,
                total_tokens INTEGER NOT NULL,
                cost REAL NOT NULL,
                is_streaming INTEGER NOT NULL,
                failover INTEGER NOT NULL,
                error_category TEXT
            )",
            [],
        ).map_err(|e| format!("Failed to create gateway_request_logs table: {e}"))?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_gateway_request_logs_ts ON gateway_request_logs(timestamp DESC)",
            [],
        ).map_err(|e| format!("Failed to create index: {e}"))?;

        Ok(())
    }

    fn open_conn(paths: &Paths) -> Result<Connection, String> {
        let p = Self::db_path(paths);
        if let Some(parent) = p.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let conn = Connection::open(&p).map_err(|e| format!("Failed to open gateway.db: {e}"))?;
        Self::init_db(&conn)?;
        Ok(conn)
    }

    /// Record a completed request log: writes summary to SQLite and detail JSONL to disk.
    pub fn record(paths: &Paths, detail: &GatewayRequestLogDetail) -> Result<(), String> {
        let conn = Self::open_conn(paths)?;
        let s = &detail.summary;

        conn.execute(
            "INSERT INTO gateway_request_logs (
                id, timestamp, cli_key, route_name, provider_id, provider_name,
                model, status_code, duration_ms, first_token_ms, input_tokens,
                output_tokens, total_tokens, cost, is_streaming, failover, error_category
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            params![
                s.id,
                s.timestamp,
                s.cli_key,
                s.route_name,
                s.provider_id,
                s.provider_name,
                s.model,
                s.status_code,
                s.duration_ms,
                s.first_token_ms,
                s.input_tokens,
                s.output_tokens,
                s.total_tokens,
                s.cost,
                if s.is_streaming { 1 } else { 0 },
                if s.failover { 1 } else { 0 },
                s.error_category,
            ],
        ).map_err(|e| format!("Failed to insert request log summary: {e}"))?;

        // Write detail JSONL file
        let logs_dir = paths.gateway_logs_dir();
        let _ = fs::create_dir_all(&logs_dir);
        let detail_path = logs_dir.join(format!("{}.json", s.id));
        if let Ok(json) = serde_json::to_string_pretty(detail) {
            let _ = fs::write(detail_path, json);
        }

        Ok(())
    }

    /// List recent request log summaries ordered by timestamp DESC.
    pub fn list(paths: &Paths, limit: usize, offset: usize) -> Result<Vec<GatewayRequestLogSummary>, String> {
        let conn = Self::open_conn(paths)?;
        let mut stmt = conn.prepare(
            "SELECT id, timestamp, cli_key, route_name, provider_id, provider_name,
                    model, status_code, duration_ms, first_token_ms, input_tokens,
                    output_tokens, total_tokens, cost, is_streaming, failover, error_category
             FROM gateway_request_logs
             ORDER BY timestamp DESC
             LIMIT ?1 OFFSET ?2"
        ).map_err(|e| format!("Failed to prepare query: {e}"))?;

        let rows = stmt.query_map(params![limit as i64, offset as i64], |row| {
            let is_streaming_int: i32 = row.get(14)?;
            let failover_int: i32 = row.get(15)?;
            Ok(GatewayRequestLogSummary {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                cli_key: row.get(2)?,
                route_name: row.get(3)?,
                provider_id: row.get(4)?,
                provider_name: row.get(5)?,
                model: row.get(6)?,
                status_code: row.get(7)?,
                duration_ms: row.get(8)?,
                first_token_ms: row.get(9)?,
                input_tokens: row.get(10)?,
                output_tokens: row.get(11)?,
                total_tokens: row.get(12)?,
                cost: row.get(13)?,
                is_streaming: is_streaming_int != 0,
                failover: failover_int != 0,
                error_category: row.get(16)?,
            })
        }).map_err(|e| format!("Failed to query rows: {e}"))?;

        let mut results = Vec::new();
        for r in rows {
            if let Ok(item) = r {
                results.push(item);
            }
        }
        Ok(results)
    }

    /// Get request detail by ID.
    pub fn get_detail(paths: &Paths, id: &str) -> Result<Option<GatewayRequestLogDetail>, String> {
        let detail_path = paths.gateway_logs_dir().join(format!("{id}.json"));
        if detail_path.exists() {
            if let Ok(content) = fs::read_to_string(detail_path) {
                if let Ok(detail) = serde_json::from_str::<GatewayRequestLogDetail>(&content) {
                    return Ok(Some(detail));
                }
            }
        }
        Ok(None)
    }

    /// Clear all request logs.
    pub fn clear(paths: &Paths) -> Result<(), String> {
        let conn = Self::open_conn(paths)?;
        conn.execute("DELETE FROM gateway_request_logs", [])
            .map_err(|e| format!("Failed to clear table: {e}"))?;

        let logs_dir = paths.gateway_logs_dir();
        if logs_dir.exists() {
            let _ = fs::remove_dir_all(&logs_dir);
            let _ = fs::create_dir_all(&logs_dir);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_record_and_list_logs() {
        let temp = tempdir().unwrap();
        let paths = Paths::new(temp.path(), temp.path().join(".aitoolplus"));

        let mut detail = GatewayRequestLogDetail::default();
        detail.id = "req_123".into();
        detail.summary.id = "req_123".into();
        detail.summary.timestamp = 1700000000;
        detail.summary.cli_key = "claude".into();
        detail.summary.provider_name = "DeepSeek".into();
        detail.summary.status_code = 200;

        RequestLogStore::record(&paths, &detail).unwrap();

        let list = RequestLogStore::list(&paths, 10, 0).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "req_123");
        assert_eq!(list[0].provider_name, "DeepSeek");

        let fetched = RequestLogStore::get_detail(&paths, "req_123").unwrap();
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().summary.provider_name, "DeepSeek");
    }
}
