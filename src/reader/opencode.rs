use crate::reader::{basename, session_label, UsageSource};
use crate::state::{Platform, UsageRecord};
use chrono::{TimeZone, Utc};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use std::path::PathBuf;
use tracing::warn;

/// Reads per-call usage from opencode's local SQLite DB (`opencode.db`). Each
/// assistant `message` row becomes one `UsageRecord`. Opened read-only; if the
/// DB is missing or unreadable, every method returns empty (opencode absent).
pub struct OpencodeReader {
    conn: Option<Connection>,
    /// Max `message.time_created` (epoch ms) seen so far — the poll cursor.
    cursor: i64,
}

impl OpencodeReader {
    pub fn new(data_dir: PathBuf) -> Self {
        let db_path = data_dir.join("opencode.db");
        // Read-only: we never write opencode's DB. WAL mode permits this
        // concurrent reader to see committed rows (verified against a live
        // install).
        let conn = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY).ok();
        Self { conn, cursor: 0 }
    }

    #[cfg(test)]
    fn from_connection(conn: Connection) -> Self {
        Self {
            conn: Some(conn),
            cursor: 0,
        }
    }

    /// Query assistant messages with `time_created > cursor`, advancing the
    /// cursor to the max timestamp seen.
    fn query_since(&mut self, cursor: i64) -> Vec<UsageRecord> {
        let Some(conn) = self.conn.as_ref() else {
            return Vec::new();
        };
        let mut stmt = match conn.prepare(
            // LEFT JOIN so messages whose session row has been deleted ("orphan"
            // sessions) are still counted — their tokens/cost are real usage.
            // COALESCE maps a missing directory to "", and basename("") returns
            // "unknown", giving a graceful label without special-casing.
            //
            // Strictly > (not >=) is deliberate: re-emitting the boundary row on
            // every poll would double-count in the no-dedup aggregator. The rare
            // trade-off is that two assistant messages written in the exact same
            // millisecond straddling a poll boundary could drop the later one —
            // negligible at second-granularity polling.
            "SELECT m.data, COALESCE(s.directory, '') AS directory, m.session_id, m.time_created \
             FROM message m LEFT JOIN session s ON m.session_id = s.id \
             WHERE json_extract(m.data, '$.role') = 'assistant' \
               AND m.time_created > ?1 \
             ORDER BY m.time_created",
        ) {
            Ok(s) => s,
            Err(e) => {
                warn!("opencode: failed to prepare usage query: {e}");
                return Vec::new();
            }
        };
        let rows = stmt.query_map([cursor], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
            ))
        });
        let mut records = Vec::new();
        let mut max_seen = cursor;
        if let Ok(rows) = rows {
            for (data, directory, session_id, time_created) in rows.flatten() {
                if time_created > max_seen {
                    max_seen = time_created;
                }
                if let Some(rec) =
                    parse_opencode_message(&data, &directory, &session_id, time_created)
                {
                    records.push(rec);
                }
            }
        }
        self.cursor = max_seen;
        records
    }
}

impl UsageSource for OpencodeReader {
    fn platform(&self) -> Platform {
        Platform::OpenCode
    }
    fn scan_all(&mut self) -> Vec<UsageRecord> {
        self.cursor = 0;
        self.query_since(0)
    }
    fn poll_delta(&mut self) -> Vec<UsageRecord> {
        let cursor = self.cursor;
        self.query_since(cursor)
    }
}

/// Parse one assistant `message.data` JSON blob into a `UsageRecord`. Returns
/// `None` for rows without token usage (errors, aborted calls).
fn parse_opencode_message(
    data: &str,
    directory: &str,
    session_id: &str,
    time_created_ms: i64,
) -> Option<UsageRecord> {
    let v: Value = serde_json::from_str(data).ok()?;
    let tokens = v.get("tokens")?;
    let u64_at = |obj: &Value, key: &str| obj.get(key).and_then(|x| x.as_u64()).unwrap_or(0);

    let input = u64_at(tokens, "input");
    let output = u64_at(tokens, "output");
    let reasoning = u64_at(tokens, "reasoning");
    let (cache_read, cache_write) = match tokens.get("cache") {
        Some(cache) => (u64_at(cache, "read"), u64_at(cache, "write")),
        None => (0, 0),
    };

    // Skip no-op rows, mirroring the Claude parser.
    if input == 0 && output == 0 && reasoning == 0 && cache_read == 0 && cache_write == 0 {
        return None;
    }

    let model_id = v.get("modelID").and_then(|x| x.as_str()).unwrap_or("unknown");
    let provider_id = v
        .get("providerID")
        .and_then(|x| x.as_str())
        .unwrap_or("opencode");
    let model = format!("{provider_id}/{model_id}");
    let cost = v.get("cost").and_then(|x| x.as_f64()).unwrap_or(0.0);
    let timestamp = Utc.timestamp_millis_opt(time_created_ms).single()?;
    let session = session_label(&basename(directory), session_id);

    Some(UsageRecord {
        timestamp,
        platform: Platform::OpenCode,
        model,
        session,
        input_tokens: input,
        // opencode tracks reasoning separately; fold it into output (it is
        // generated/billed as output) so the OUTPUT column reflects all of it.
        output_tokens: output + reasoning,
        cache_read_tokens: cache_read,
        cache_creation_tokens: cache_write,
        cost_usd: cost,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ASSISTANT_1: &str = r#"{"role":"assistant","modelID":"kimi-k2.6:cloud","providerID":"ollama","cost":0.0,"tokens":{"input":100,"output":40,"reasoning":10,"cache":{"read":5,"write":2}},"time":{"created":1000}}"#;
    const ASSISTANT_2: &str = r#"{"role":"assistant","modelID":"minimax-m3","providerID":"opencode-go","cost":1.5,"tokens":{"input":200,"output":80,"reasoning":0,"cache":{"read":0,"write":0}},"time":{"created":2000}}"#;
    const USER_MSG: &str = r#"{"role":"user","time":{"created":1500}}"#;
    const ASSISTANT_NO_TOKENS: &str = r#"{"role":"assistant","modelID":"x","providerID":"y","cost":0.0,"tokens":{"input":0,"output":0,"reasoning":0,"cache":{"read":0,"write":0}},"time":{"created":1700}}"#;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE session (id TEXT PRIMARY KEY, directory TEXT NOT NULL);
             CREATE TABLE message (id TEXT PRIMARY KEY, session_id TEXT NOT NULL, time_created INTEGER NOT NULL, data TEXT NOT NULL);
             INSERT INTO session VALUES ('ses_abc', '/Users/me/myproject');",
        )
        .unwrap();
        conn
    }

    fn insert(conn: &Connection, id: &str, t: i64, data: &str) {
        conn.execute(
            "INSERT INTO message VALUES (?1, 'ses_abc', ?2, ?3)",
            rusqlite::params![id, t, data],
        )
        .unwrap();
    }

    #[test]
    fn scan_all_parses_assistant_messages_and_skips_others() {
        let conn = setup();
        insert(&conn, "m1", 1000, ASSISTANT_1);
        insert(&conn, "m2", 1500, USER_MSG);
        insert(&conn, "m3", 1700, ASSISTANT_NO_TOKENS);
        insert(&conn, "m4", 2000, ASSISTANT_2);
        let mut reader = OpencodeReader::from_connection(conn);

        let records = reader.scan_all();
        assert_eq!(records.len(), 2); // user + tokenless rows skipped

        let r1 = &records[0];
        assert_eq!(r1.model, "ollama/kimi-k2.6:cloud");
        assert_eq!(r1.input_tokens, 100);
        assert_eq!(r1.output_tokens, 50); // 40 output + 10 reasoning
        assert_eq!(r1.cache_read_tokens, 5);
        assert_eq!(r1.cache_creation_tokens, 2);
        assert_eq!(r1.cost_usd, 0.0);
        assert_eq!(r1.session, "myproject ses_abc");
        assert_eq!(r1.platform, Platform::OpenCode);

        let r2 = &records[1];
        assert_eq!(r2.model, "opencode-go/minimax-m3");
        assert_eq!(r2.cost_usd, 1.5);
    }

    #[test]
    fn poll_delta_returns_only_new_rows() {
        let conn = setup();
        insert(&conn, "m1", 1000, ASSISTANT_1);
        let mut reader = OpencodeReader::from_connection(conn);

        assert_eq!(reader.scan_all().len(), 1);
        assert_eq!(reader.poll_delta().len(), 0); // nothing new

        // A newer assistant message arrives.
        let conn2 = reader.conn.as_ref().unwrap();
        conn2
            .execute(
                "INSERT INTO message VALUES ('m2', 'ses_abc', 2000, ?1)",
                rusqlite::params![ASSISTANT_2],
            )
            .unwrap();

        let delta = reader.poll_delta();
        assert_eq!(delta.len(), 1);
        assert_eq!(delta[0].model, "opencode-go/minimax-m3");
    }

    #[test]
    fn missing_db_yields_empty() {
        let mut reader = OpencodeReader::new(PathBuf::from("/nonexistent/path"));
        assert!(reader.scan_all().is_empty());
        assert!(reader.poll_delta().is_empty());
    }

    #[test]
    fn counts_messages_with_missing_session() {
        let conn = setup();
        // Assistant message whose session_id has no matching session row.
        conn.execute(
            "INSERT INTO message VALUES ('m_orphan', 'ses_missing', 3000, ?1)",
            rusqlite::params![ASSISTANT_2],
        )
        .unwrap();
        let mut reader = OpencodeReader::from_connection(conn);
        let records = reader.scan_all();
        assert_eq!(records.len(), 1);
        assert!(
            records[0].session.starts_with("unknown"),
            "got {}",
            records[0].session
        );
    }
}
