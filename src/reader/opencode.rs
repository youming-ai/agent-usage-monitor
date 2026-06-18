use super::UsageSource;
use super::sqlite_message_reader::SqliteMessageReader;
use crate::state::{Platform, UsageRecord};
use std::path::PathBuf;

/// Reads per-call usage from opencode's local SQLite DB (`opencode.db`).
/// Delegates to the shared `SqliteMessageReader`.
pub struct OpencodeReader {
    inner: SqliteMessageReader,
    pub(crate) db_path: PathBuf,
}

impl OpencodeReader {
    pub fn new(data_dir: PathBuf) -> Self {
        let db_path = data_dir.join("opencode.db");
        Self {
            inner: SqliteMessageReader::new(
                data_dir,
                "opencode.db",
                Platform::OpenCode,
                "opencode",
            ),
            db_path,
        }
    }
}

impl UsageSource for OpencodeReader {
    fn platform(&self) -> Platform {
        self.inner.platform()
    }
    fn scan_all(&mut self) -> Vec<UsageRecord> {
        self.inner.scan_all()
    }
    fn poll_delta(&mut self) -> Vec<UsageRecord> {
        self.inner.poll_delta()
    }
    fn get_watch_directories(&self) -> Vec<std::path::PathBuf> {
        vec![self.db_path.clone()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

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
        let mut reader = OpencodeReader {
            inner: SqliteMessageReader::from_connection(conn, Platform::OpenCode, "opencode"),
            db_path: PathBuf::new(),
        };

        let records = reader.scan_all();
        assert_eq!(records.len(), 2);

        let r1 = &records[0];
        assert_eq!(r1.model, "ollama/kimi-k2.6:cloud");
        assert_eq!(r1.input_tokens, 100);
        assert_eq!(r1.output_tokens, 50);
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
        let mut reader = OpencodeReader {
            inner: SqliteMessageReader::from_connection(conn, Platform::OpenCode, "opencode"),
            db_path: PathBuf::new(),
        };

        assert_eq!(reader.scan_all().len(), 1);
        assert_eq!(reader.poll_delta().len(), 0);

        // Access the inner connection directly for test mutation.
        let inner_conn = reader.inner.conn.as_ref().unwrap();
        inner_conn
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
        conn.execute(
            "INSERT INTO message VALUES ('m_orphan', 'ses_missing', 3000, ?1)",
            rusqlite::params![ASSISTANT_2],
        )
        .unwrap();
        let mut reader = OpencodeReader {
            inner: SqliteMessageReader::from_connection(conn, Platform::OpenCode, "opencode"),
            db_path: PathBuf::new(),
        };
        let records = reader.scan_all();
        assert_eq!(records.len(), 1);
        assert!(
            records[0].session.starts_with("unknown"),
            "got {}",
            records[0].session
        );
    }
}
