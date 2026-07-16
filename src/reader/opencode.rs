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
        assert_eq!(crate::state::resolve(r1.model), "ollama/kimi-k2.6:cloud");
        assert_eq!(r1.input_tokens, 100);
        assert_eq!(r1.output_tokens, 50);
        assert_eq!(r1.cache_read_tokens, 5);
        assert_eq!(r1.cache_creation_tokens, 2);
        assert_eq!(r1.cost_usd, 0.0);
        assert_eq!(crate::state::resolve(r1.session), "myproject ses_abc");
        assert_eq!(r1.platform, Platform::OpenCode);

        let r2 = &records[1];
        assert_eq!(crate::state::resolve(r2.model), "opencode-go/minimax-m3");
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
        assert_eq!(
            crate::state::resolve(delta[0].model),
            "opencode-go/minimax-m3"
        );
    }

    #[test]
    fn missing_db_yields_empty() {
        let mut reader = OpencodeReader::new(PathBuf::from("/nonexistent/path"));
        assert!(reader.scan_all().is_empty());
        assert!(reader.poll_delta().is_empty());
    }

    fn init_disk_db(path: &std::path::Path) {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(
            "CREATE TABLE session (id TEXT PRIMARY KEY, directory TEXT NOT NULL);
             CREATE TABLE message (id TEXT PRIMARY KEY, session_id TEXT NOT NULL, time_created INTEGER NOT NULL, data TEXT NOT NULL);
             INSERT INTO session VALUES ('ses_abc', '/Users/me/myproject');",
        )
        .unwrap();
    }

    #[test]
    fn db_created_after_reader_start_is_picked_up() {
        // C6 regression: if opencode.db doesn't exist yet when the reader is
        // constructed, the connection is None forever unless the reader
        // self-heals once the file appears.
        let dir = tempfile::tempdir().unwrap();
        let mut reader = OpencodeReader::new(dir.path().to_path_buf());
        assert!(reader.scan_all().is_empty(), "db not created yet");

        let db_path = dir.path().join("opencode.db");
        init_disk_db(&db_path);
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute(
                "INSERT INTO message VALUES ('m1', 'ses_abc', 1000, ?1)",
                rusqlite::params![ASSISTANT_1],
            )
            .unwrap();
        }

        let records = reader.poll_delta();
        assert_eq!(
            records.len(),
            1,
            "reader must self-heal once the db appears, not stay permanently empty"
        );
    }

    #[test]
    fn db_deleted_and_recreated_is_picked_up() {
        // C6 regression: a migration that deletes and recreates opencode.db
        // at the same path must not leave the reader holding a stale handle
        // to the deleted file forever.
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("opencode.db");
        init_disk_db(&db_path);
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute(
                "INSERT INTO message VALUES ('m1', 'ses_abc', 1000, ?1)",
                rusqlite::params![ASSISTANT_1],
            )
            .unwrap();
        }

        let mut reader = OpencodeReader::new(dir.path().to_path_buf());
        assert_eq!(reader.scan_all().len(), 1);

        // Simulate a migration: delete then recreate the db file at the same path.
        std::fs::remove_file(&db_path).unwrap();
        init_disk_db(&db_path);
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute(
                "INSERT INTO message VALUES ('m2', 'ses_abc', 2000, ?1)",
                rusqlite::params![ASSISTANT_2],
            )
            .unwrap();
        }

        let records = reader.poll_delta();
        assert_eq!(
            records.len(),
            1,
            "reader must reconnect after the db is deleted and recreated"
        );
        assert_eq!(
            crate::state::resolve(records[0].model),
            "opencode-go/minimax-m3"
        );
    }

    #[test]
    fn zero_token_row_is_reread_once_updated_in_place() {
        // C7 regression: opencode inserts an assistant row, then UPDATEs it
        // with real token counts once the response completes. A time_created
        // cursor advances past the row on first sight (zero tokens => no
        // record), so the later UPDATE would never be re-read. The rowid
        // cursor plus zero-token retry set must catch it.
        let conn = setup();
        insert(&conn, "m1", 1000, ASSISTANT_NO_TOKENS);
        let mut reader = OpencodeReader {
            inner: SqliteMessageReader::from_connection(conn, Platform::OpenCode, "opencode"),
            db_path: PathBuf::new(),
        };
        assert!(
            reader.scan_all().is_empty(),
            "zero-token row should not emit yet"
        );

        let inner_conn = reader.inner.conn.as_ref().unwrap();
        inner_conn
            .execute(
                "UPDATE message SET data = ?1 WHERE id = 'm1'",
                rusqlite::params![ASSISTANT_1],
            )
            .unwrap();

        let delta = reader.poll_delta();
        assert_eq!(
            delta.len(),
            1,
            "row updated in place with real tokens must be re-read"
        );
        assert_eq!(delta[0].input_tokens, 100);
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
            crate::state::resolve(records[0].session).starts_with("unknown"),
            "got {}",
            crate::state::resolve(records[0].session)
        );
    }
}
