use crate::reader::{UsageSource, basename, session_label};
use crate::state::{Platform, UsageRecord};
use chrono::{TimeZone, Utc};
use rusqlite::{Connection, OpenFlags};
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::warn;

/// Last-known cumulative totals for a session row. Hermes updates rows in
/// place as a session progresses, so we emit token/cost deltas on each poll.
#[derive(Clone, PartialEq)]
struct SessionTotals {
    input: u64,
    output: u64,
    cache_read: u64,
    cache_write: u64,
    cost: f64,
}

struct SessionRow {
    id: String,
    model: String,
    started_at: f64,
    totals: SessionTotals,
    cwd: String,
}

/// Reads per-session usage from hermes-agent's SQLite DB (`state.db`). Each
/// `sessions` row becomes usage records; in-place row updates emit deltas.
pub struct HermesReader {
    conn: Option<Connection>,
    last_seen: HashMap<String, SessionTotals>,
}

impl HermesReader {
    pub fn new(data_dir: PathBuf) -> Self {
        let db_path = data_dir.join("state.db");
        let conn = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY).ok();
        Self {
            conn,
            last_seen: HashMap::new(),
        }
    }

    #[cfg(test)]
    fn from_connection(conn: Connection) -> Self {
        Self {
            conn: Some(conn),
            last_seen: HashMap::new(),
        }
    }

    fn fetch_all_sessions(&self) -> Vec<SessionRow> {
        let Some(conn) = self.conn.as_ref() else {
            return Vec::new();
        };
        let mut stmt = match conn.prepare(
            "SELECT id, model, started_at, input_tokens, output_tokens, \
             cache_read_tokens, cache_write_tokens, estimated_cost_usd, cwd \
             FROM sessions ORDER BY started_at",
        ) {
            Ok(s) => s,
            Err(e) => {
                warn!("hermes: failed to prepare usage query: {e}");
                return Vec::new();
            }
        };
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, f64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, Option<f64>>(7)?,
                row.get::<_, Option<String>>(8)?,
            ))
        });
        let mut sessions = Vec::new();
        if let Ok(rows) = rows {
            for row in rows.flatten() {
                let (id, model, started_at, input, output, cache_read, cache_write, cost, cwd) =
                    row;
                sessions.push(SessionRow {
                    id,
                    model: model.unwrap_or_else(|| "unknown".to_string()),
                    started_at,
                    totals: SessionTotals {
                        input: input.max(0) as u64,
                        output: output.max(0) as u64,
                        cache_read: cache_read.max(0) as u64,
                        cache_write: cache_write.max(0) as u64,
                        cost: cost.unwrap_or(0.0),
                    },
                    cwd: cwd.unwrap_or_default(),
                });
            }
        }
        sessions
    }

    fn sync_records(&mut self) -> Vec<UsageRecord> {
        let sessions = self.fetch_all_sessions();
        let mut records = Vec::new();
        let mut current_ids = Vec::with_capacity(sessions.len());

        for session in sessions {
            current_ids.push(session.id.clone());
            let current = session.totals.clone();
            if current.input == 0
                && current.output == 0
                && current.cache_read == 0
                && current.cache_write == 0
            {
                self.last_seen.insert(session.id.clone(), current);
                continue;
            }

            let (input, output, cache_read, cache_write, cost) =
                if let Some(prev) = self.last_seen.get(&session.id) {
                    (
                        current.input.saturating_sub(prev.input),
                        current.output.saturating_sub(prev.output),
                        current.cache_read.saturating_sub(prev.cache_read),
                        current.cache_write.saturating_sub(prev.cache_write),
                        (current.cost - prev.cost).max(0.0),
                    )
                } else {
                    (
                        current.input,
                        current.output,
                        current.cache_read,
                        current.cache_write,
                        current.cost,
                    )
                };

            self.last_seen.insert(session.id.clone(), current);

            if input == 0 && output == 0 && cache_read == 0 && cache_write == 0 && cost == 0.0 {
                continue;
            }

            let timestamp = match Utc
                .timestamp_millis_opt((session.started_at * 1000.0) as i64)
                .single()
            {
                Some(ts) => ts,
                None => continue,
            };
            let session_label = session_label(&basename(&session.cwd), &session.id);
            records.push(UsageRecord {
                timestamp,
                platform: Platform::Hermes,
                model: session.model,
                session: session_label,
                input_tokens: input,
                output_tokens: output,
                cache_read_tokens: cache_read,
                cache_creation_tokens: cache_write,
                cost_usd: cost,
            });
        }

        self.last_seen
            .retain(|id, _| current_ids.iter().any(|current| current == id));
        records
    }
}

impl UsageSource for HermesReader {
    fn platform(&self) -> Platform {
        Platform::Hermes
    }
    fn scan_all(&mut self) -> Vec<UsageRecord> {
        self.last_seen.clear();
        self.sync_records()
    }
    fn poll_delta(&mut self) -> Vec<UsageRecord> {
        self.sync_records()
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
            rusqlite::params![
                id,
                model,
                started_at,
                input,
                output,
                cache_read,
                cache_write,
                cost,
                cwd
            ],
        )
        .unwrap();
    }

    #[test]
    fn scan_all_parses_sessions_and_skips_empty() {
        let conn = setup();
        insert(
            &conn,
            "s1",
            "claude-3.5-sonnet",
            1000.0,
            100,
            40,
            5,
            2,
            0.5,
            "/Users/me/project",
        );
        insert(
            &conn,
            "s2",
            "claude-3.5-sonnet",
            1500.0,
            0,
            0,
            0,
            0,
            0.0,
            "/Users/me/project",
        );
        insert(
            &conn,
            "s3",
            "gpt-4o",
            2000.0,
            200,
            80,
            0,
            0,
            1.5,
            "/Users/me/other",
        );
        let mut reader = HermesReader::from_connection(conn);

        let records = reader.scan_all();
        assert_eq!(records.len(), 2);

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
        insert(
            &conn,
            "s1",
            "claude-3.5-sonnet",
            1000.0,
            100,
            40,
            5,
            2,
            0.5,
            "/Users/me/project",
        );
        let mut reader = HermesReader::from_connection(conn);

        assert_eq!(reader.scan_all().len(), 1);
        assert_eq!(reader.poll_delta().len(), 0);

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
    fn poll_delta_emits_in_place_session_updates() {
        let conn = setup();
        insert(
            &conn,
            "s1",
            "claude-3.5-sonnet",
            1000.0,
            100,
            40,
            0,
            0,
            0.5,
            "/Users/me/project",
        );
        let mut reader = HermesReader::from_connection(conn);

        assert_eq!(reader.scan_all().len(), 1);

        let conn2 = reader.conn.as_ref().unwrap();
        conn2
            .execute(
                "UPDATE sessions SET input_tokens = 250, output_tokens = 90, estimated_cost_usd = 1.0 \
                 WHERE id = 's1'",
                [],
            )
            .unwrap();

        let delta = reader.poll_delta();
        assert_eq!(delta.len(), 1);
        assert_eq!(delta[0].input_tokens, 150);
        assert_eq!(delta[0].output_tokens, 50);
        assert!((delta[0].cost_usd - 0.5).abs() < 1e-9);
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
