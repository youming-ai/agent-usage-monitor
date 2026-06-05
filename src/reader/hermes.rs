use crate::reader::{basename, session_label, UsageSource};
use crate::state::{Platform, UsageRecord};
use chrono::{TimeZone, Utc};
use rusqlite::{Connection, OpenFlags};
use std::path::PathBuf;
use tracing::warn;

/// Reads per-session usage from hermes-agent's SQLite DB (`state.db`). Each
/// `sessions` row becomes one `UsageRecord`. Opened read-only; if the DB is
/// missing or unreadable, every method returns empty (hermes absent).
pub struct HermesReader {
    conn: Option<Connection>,
    /// Max `started_at` (epoch seconds as REAL) seen so far — the poll cursor.
    cursor: f64,
}

impl HermesReader {
    pub fn new(data_dir: PathBuf) -> Self {
        let db_path = data_dir.join("state.db");
        let conn = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY).ok();
        Self { conn, cursor: 0.0 }
    }

    #[cfg(test)]
    fn from_connection(conn: Connection) -> Self {
        Self {
            conn: Some(conn),
            cursor: 0.0,
        }
    }

    /// Query sessions with `started_at > cursor`, advancing the cursor to the
    /// max timestamp seen.
    fn query_since(&mut self, cursor: f64) -> Vec<UsageRecord> {
        let Some(conn) = self.conn.as_ref() else {
            return Vec::new();
        };
        let mut stmt = match conn.prepare(
            "SELECT id, model, started_at, input_tokens, output_tokens, \
             cache_read_tokens, cache_write_tokens, estimated_cost_usd, cwd \
             FROM sessions WHERE started_at > ?1 ORDER BY started_at",
        ) {
            Ok(s) => s,
            Err(e) => {
                warn!("hermes: failed to prepare usage query: {e}");
                return Vec::new();
            }
        };
        let rows = stmt.query_map([cursor], |row| {
            Ok((
                row.get::<_, String>(0)?,       // id
                row.get::<_, Option<String>>(1)?, // model
                row.get::<_, f64>(2)?,          // started_at
                row.get::<_, i64>(3)?,          // input_tokens
                row.get::<_, i64>(4)?,          // output_tokens
                row.get::<_, i64>(5)?,          // cache_read_tokens
                row.get::<_, i64>(6)?,          // cache_write_tokens
                row.get::<_, Option<f64>>(7)?,  // estimated_cost_usd
                row.get::<_, Option<String>>(8)?, // cwd
            ))
        });
        let mut records = Vec::new();
        let mut max_seen = cursor;
        if let Ok(rows) = rows {
            for row in rows.flatten() {
                let (id, model, started_at, input, output, cache_read, cache_write, cost, cwd) =
                    row;
                if started_at > max_seen {
                    max_seen = started_at;
                }
                // Skip rows with no token usage.
                if input == 0 && output == 0 && cache_read == 0 && cache_write == 0 {
                    continue;
                }
                let timestamp = Utc.timestamp_millis_opt((started_at * 1000.0) as i64).single();
                let Some(timestamp) = timestamp else {
                    continue;
                };
                let model = model.unwrap_or_else(|| "unknown".to_string());
                let cwd = cwd.unwrap_or_default();
                let session = session_label(&basename(&cwd), &id);
                records.push(UsageRecord {
                    timestamp,
                    platform: Platform::Hermes,
                    model,
                    session,
                    input_tokens: input as u64,
                    output_tokens: output as u64,
                    cache_read_tokens: cache_read as u64,
                    cache_creation_tokens: cache_write as u64,
                    cost_usd: cost.unwrap_or(0.0),
                });
            }
        }
        self.cursor = max_seen;
        records
    }
}

impl UsageSource for HermesReader {
    fn platform(&self) -> Platform {
        Platform::Hermes
    }
    fn scan_all(&mut self) -> Vec<UsageRecord> {
        self.cursor = 0.0;
        self.query_since(0.0)
    }
    fn poll_delta(&mut self) -> Vec<UsageRecord> {
        let cursor = self.cursor;
        self.query_since(cursor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE sessions (
                id TEXT PRIMARY KEY,
                model TEXT,
                started_at REAL NOT NULL,
                ended_at REAL,
                input_tokens INTEGER DEFAULT 0,
                output_tokens INTEGER DEFAULT 0,
                cache_read_tokens INTEGER DEFAULT 0,
                cache_write_tokens INTEGER DEFAULT 0,
                estimated_cost_usd REAL,
                cwd TEXT
            );",
        )
        .unwrap();
        conn
    }

    fn insert(
        conn: &Connection,
        id: &str,
        model: &str,
        started_at: f64,
        input: i64,
        output: i64,
        cache_read: i64,
        cache_write: i64,
        cost: f64,
        cwd: &str,
    ) {
        conn.execute(
            "INSERT INTO sessions (id, model, started_at, input_tokens, output_tokens, \
             cache_read_tokens, cache_write_tokens, estimated_cost_usd, cwd) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![id, model, started_at, input, output, cache_read, cache_write, cost, cwd],
        )
        .unwrap();
    }

    #[test]
    fn scan_all_parses_sessions_and_skips_empty() {
        let conn = setup();
        insert(&conn, "s1", "claude-3.5-sonnet", 1000.0, 100, 40, 5, 2, 0.5, "/Users/me/project");
        // Zero-token session — should be skipped.
        insert(&conn, "s2", "claude-3.5-sonnet", 1500.0, 0, 0, 0, 0, 0.0, "/Users/me/project");
        insert(&conn, "s3", "gpt-4o", 2000.0, 200, 80, 0, 0, 1.5, "/Users/me/other");
        let mut reader = HermesReader::from_connection(conn);

        let records = reader.scan_all();
        assert_eq!(records.len(), 2); // s2 skipped

        let r1 = &records[0];
        assert_eq!(r1.model, "claude-3.5-sonnet");
        assert_eq!(r1.input_tokens, 100);
        assert_eq!(r1.output_tokens, 40);
        assert_eq!(r1.cache_read_tokens, 5);
        assert_eq!(r1.cache_creation_tokens, 2);
        assert_eq!(r1.cost_usd, 0.5);
        assert_eq!(r1.session, "project s1");
        assert_eq!(r1.platform, Platform::Hermes);

        let r2 = &records[1];
        assert_eq!(r2.model, "gpt-4o");
        assert_eq!(r2.cost_usd, 1.5);
        assert_eq!(r2.session, "other s3");
    }

    #[test]
    fn poll_delta_returns_only_new_rows() {
        let conn = setup();
        insert(&conn, "s1", "claude-3.5-sonnet", 1000.0, 100, 40, 5, 2, 0.5, "/Users/me/project");
        let mut reader = HermesReader::from_connection(conn);

        assert_eq!(reader.scan_all().len(), 1);
        assert_eq!(reader.poll_delta().len(), 0); // nothing new

        // A newer session arrives.
        let conn2 = reader.conn.as_ref().unwrap();
        conn2
            .execute(
                "INSERT INTO sessions (id, model, started_at, input_tokens, output_tokens, \
                 cache_read_tokens, cache_write_tokens, estimated_cost_usd, cwd) \
                 VALUES ('s2', 'gpt-4o', 2000.0, 200, 80, 0, 0, 1.5, '/Users/me/other')",
                [],
            )
            .unwrap();

        let delta = reader.poll_delta();
        assert_eq!(delta.len(), 1);
        assert_eq!(delta[0].model, "gpt-4o");
    }

    #[test]
    fn missing_db_yields_empty() {
        let mut reader = HermesReader::new(PathBuf::from("/nonexistent/path"));
        assert!(reader.scan_all().is_empty());
        assert!(reader.poll_delta().is_empty());
    }

    #[test]
    fn session_with_null_model_uses_unknown() {
        let conn = setup();
        conn.execute(
            "INSERT INTO sessions (id, model, started_at, input_tokens, output_tokens, \
             cache_read_tokens, cache_write_tokens, estimated_cost_usd, cwd) \
             VALUES ('s1', NULL, 1000.0, 100, 40, 0, 0, NULL, '/Users/me/project')",
            [],
        )
        .unwrap();
        let mut reader = HermesReader::from_connection(conn);
        let records = reader.scan_all();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].model, "unknown");
        assert_eq!(records[0].cost_usd, 0.0);
    }

    #[test]
    fn session_with_null_cwd_uses_empty_basename() {
        let conn = setup();
        conn.execute(
            "INSERT INTO sessions (id, model, started_at, input_tokens, output_tokens, \
             cache_read_tokens, cache_write_tokens, estimated_cost_usd, cwd) \
             VALUES ('s1', 'claude-3.5-sonnet', 1000.0, 100, 40, 0, 0, 0.5, NULL)",
            [],
        )
        .unwrap();
        let mut reader = HermesReader::from_connection(conn);
        let records = reader.scan_all();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].session, "unknown s1");
    }
}
