use crate::reader::{UsageSource, basename, session_label};
use crate::state::{Platform, UsageRecord};
use chrono::{TimeZone, Utc};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use std::path::{Path, PathBuf};
use tracing::warn;

/// Shared SQLite reader for opencode and MiMo Code (and any future agents that
/// log assistant messages into a local `message` / `session` DB schema).
///
/// Opened read-only; if the DB is missing or unreadable, every method returns
/// empty. Both opencode and MiMo Code use this single implementation — the only
/// difference is the platform enum and default provider id.
pub(crate) struct SqliteMessageReader {
    pub(crate) conn: Option<Connection>,
    /// Highest `message.rowid` consumed so far. Deliberately `rowid`, not
    /// `time_created`: same-millisecond or out-of-order rows would be missed
    /// by a timestamp cursor, and rowid is monotonically assigned by SQLite.
    cursor: i64,
    /// Rowids that were seen with all-zero token counts (opencode inserts an
    /// assistant row, then UPDATEs it in place once the response completes
    /// and totals are known). These sit past `cursor` already, so the normal
    /// `rowid > cursor` query would never see the UPDATE — re-check them each
    /// poll until they parse successfully.
    // ponytail: retried forever if a row is genuinely malformed (never gets
    // real tokens) rather than pruned after N attempts — a handful of stray
    // i64s per platform is not worth extra bookkeeping.
    pending_zero: Vec<i64>,
    platform: Platform,
    /// Provider id used when a message blob does not carry an explicit
    /// `providerID` (e.g., "opencode" or "mimocode").
    default_provider: &'static str,
    db_path: PathBuf,
    /// Inode of `db_path` the last time we (re)connected, so a delete+recreate
    /// (e.g. a migration) can be detected and the stale handle replaced. See
    /// `ensure_connection`.
    last_ino: Option<u64>,
}

/// Unix-only: the connection can only self-heal a delete+recreate on
/// platforms where inode identity is meaningful. Every default data path this
/// app watches is a unix home directory, so this covers the real targets;
// ponytail: on a hypothetical non-unix build, only the "DB created after
// startup" case (conn starts `None`) self-heals — a same-path delete+recreate
// while already connected would not be detected there.
#[cfg(unix)]
fn file_ino(path: &Path) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).ok().map(|m| m.ino())
}
#[cfg(not(unix))]
fn file_ino(_path: &Path) -> Option<u64> {
    None
}

impl SqliteMessageReader {
    pub fn new(
        data_dir: PathBuf,
        db_name: &str,
        platform: Platform,
        default_provider: &'static str,
    ) -> Self {
        let db_path = data_dir.join(db_name);
        let conn = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY).ok();
        let last_ino = file_ino(&db_path);
        Self {
            conn,
            cursor: 0,
            pending_zero: Vec::new(),
            platform,
            default_provider,
            db_path,
            last_ino,
        }
    }

    #[cfg(test)]
    pub(crate) fn from_connection(
        conn: Connection,
        platform: Platform,
        default_provider: &'static str,
    ) -> Self {
        Self {
            conn: Some(conn),
            cursor: 0,
            pending_zero: Vec::new(),
            platform,
            default_provider,
            db_path: PathBuf::new(),
            last_ino: None,
        }
    }

    /// Open (or reopen) the connection if it's missing, or if `db_path` now
    /// points at a different file than the one we last connected to (a
    /// delete+recreate, e.g. on migration). Without this, a DB created after
    /// `aum` started — or replaced while running — stays permanently
    /// unreadable until restart.
    fn ensure_connection(&mut self) {
        let current_ino = file_ino(&self.db_path);
        let changed = current_ino != self.last_ino;
        // Distinguish "first appearance" (last_ino was never set — nothing to
        // reset) from a genuine recreate (we previously had a real inode, and
        // now have a different one).
        let recreated = changed && current_ino.is_some() && self.last_ino.is_some();
        if self.conn.is_none() || (changed && current_ino.is_some()) {
            self.conn =
                Connection::open_with_flags(&self.db_path, OpenFlags::SQLITE_OPEN_READ_ONLY).ok();
            if recreated {
                // `cursor`/`pending_zero` are rowids from the PREVIOUS file's
                // rowid sequence. A recreated db (new file, new table) starts
                // its own sequence from scratch, so keeping the old cursor
                // would skip rows in the new db that happen to have a lower
                // rowid than the old cursor value — silent data loss right
                // after the very reconnect meant to fix data loss.
                self.cursor = 0;
                self.pending_zero.clear();
            }
        }
        self.last_ino = current_ino;
    }

    fn query_since(&mut self) -> Vec<UsageRecord> {
        // Must run before reading `self.cursor` below: a detected db
        // recreation resets `self.cursor` to 0 (the new db's rowid sequence
        // starts fresh), and a caller-supplied cursor snapshotted before this
        // call would silently ignore that reset.
        self.ensure_connection();
        let cursor = self.cursor;
        let Some(conn) = self.conn.as_ref() else {
            return Vec::new();
        };

        // Re-check rowids that were all-zero tokens last time, in addition to
        // whatever is newly past the cursor (see `pending_zero`).
        let pending = std::mem::take(&mut self.pending_zero);
        let pending_clause = if pending.is_empty() {
            String::new()
        } else {
            let ids: Vec<String> = pending.iter().map(i64::to_string).collect();
            format!(" OR m.rowid IN ({})", ids.join(","))
        };

        let sql = format!(
            "SELECT m.rowid, m.id, m.data, COALESCE(s.directory, '') AS directory, m.session_id, m.time_created \
             FROM message m LEFT JOIN session s ON m.session_id = s.id \
             WHERE json_extract(m.data, '$.role') = 'assistant' \
               AND (m.rowid > ?1{pending_clause}) \
             ORDER BY m.rowid"
        );
        let mut stmt = match conn.prepare(&sql) {
            Ok(s) => s,
            Err(e) => {
                warn!("{:?}: failed to prepare usage query: {e}", self.platform);
                return Vec::new();
            }
        };
        let rows = stmt.query_map([cursor], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i64>(5)?,
            ))
        });
        let mut records = Vec::new();
        let mut max_seen = cursor;
        let mut still_zero = Vec::new();
        if let Ok(rows) = rows {
            for (rowid, id, data, directory, session_id, time_created) in rows.flatten() {
                if rowid > max_seen {
                    max_seen = rowid;
                }
                match parse_message(
                    &id,
                    &data,
                    &directory,
                    &session_id,
                    time_created,
                    self.platform,
                    self.default_provider,
                ) {
                    Some(rec) => records.push(rec),
                    // Either still all-zero (pending an UPDATE) or otherwise
                    // unparseable; either way, cheap to re-check next poll.
                    None => still_zero.push(rowid),
                }
            }
        }
        self.cursor = max_seen;
        self.pending_zero = still_zero;
        records
    }
}

impl UsageSource for SqliteMessageReader {
    fn platform(&self) -> Platform {
        self.platform
    }
    fn scan_all(&mut self) -> Vec<UsageRecord> {
        self.cursor = 0;
        self.pending_zero.clear();
        self.query_since()
    }
    fn poll_delta(&mut self) -> Vec<UsageRecord> {
        self.query_since()
    }
}

fn parse_message(
    id: &str,
    data: &str,
    directory: &str,
    session_id: &str,
    time_created_ms: i64,
    platform: Platform,
    default_provider: &str,
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

    if input == 0 && output == 0 && reasoning == 0 && cache_read == 0 && cache_write == 0 {
        return None;
    }

    let model_id = v
        .get("modelID")
        .and_then(|x| x.as_str())
        .unwrap_or("unknown");
    let provider_id = v
        .get("providerID")
        .and_then(|x| x.as_str())
        .unwrap_or(default_provider);
    let model = format!("{provider_id}/{model_id}");
    let cost = v.get("cost").and_then(|x| x.as_f64()).unwrap_or(0.0);
    let timestamp = Utc.timestamp_millis_opt(time_created_ms).single()?;
    let session = session_label(&basename(directory), session_id);

    Some(UsageRecord {
        timestamp,
        platform,
        model: crate::state::intern(&model),
        session: crate::state::intern(&session),
        id: crate::state::intern(id),
        input_tokens: input,
        output_tokens: output + reasoning,
        cache_read_tokens: cache_read,
        cache_creation_tokens: cache_write,
        cost_usd: cost,
    })
}

// This one test module covers opencode and MiMo Code both — they are the
// same reader (`SqliteMessageReader`) constructed with different db name /
// platform / provider-id args (see `platforms.rs`), so exercising it once
// covers both; the two used to each carry an identical copy of this suite
// via a thin `OpencodeReader`/`MimoCodeReader` wrapper whose only job was to
// bolt an (unused-after this refactor) `db_path` onto this same type.
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
        let mut reader = SqliteMessageReader::from_connection(conn, Platform::OpenCode, "opencode");

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
        let mut reader = SqliteMessageReader::from_connection(conn, Platform::OpenCode, "opencode");

        assert_eq!(reader.scan_all().len(), 1);
        assert_eq!(reader.poll_delta().len(), 0);

        // Access the inner connection directly for test mutation.
        let inner_conn = reader.conn.as_ref().unwrap();
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
        let mut reader = SqliteMessageReader::new(
            PathBuf::from("/nonexistent/path"),
            "opencode.db",
            Platform::OpenCode,
            "opencode",
        );
        assert!(reader.scan_all().is_empty());
        assert!(reader.poll_delta().is_empty());
    }

    fn init_disk_db(path: &Path) {
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
        // C6 regression: if the db doesn't exist yet when the reader is
        // constructed, the connection is None forever unless the reader
        // self-heals once the file appears.
        let dir = tempfile::tempdir().unwrap();
        let mut reader = SqliteMessageReader::new(
            dir.path().to_path_buf(),
            "opencode.db",
            Platform::OpenCode,
            "opencode",
        );
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
        // C6 regression: a migration that deletes and recreates the db file
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

        let mut reader = SqliteMessageReader::new(
            dir.path().to_path_buf(),
            "opencode.db",
            Platform::OpenCode,
            "opencode",
        );
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
        let mut reader = SqliteMessageReader::from_connection(conn, Platform::OpenCode, "opencode");
        assert!(
            reader.scan_all().is_empty(),
            "zero-token row should not emit yet"
        );

        let inner_conn = reader.conn.as_ref().unwrap();
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
        let mut reader = SqliteMessageReader::from_connection(conn, Platform::OpenCode, "opencode");
        let records = reader.scan_all();
        assert_eq!(records.len(), 1);
        assert!(
            crate::state::resolve(records[0].session).starts_with("unknown"),
            "got {}",
            crate::state::resolve(records[0].session)
        );
    }

    #[test]
    fn mimo_code_platform_and_provider_wiring_works() {
        // Sanity check that the reader works correctly for MiMo Code too —
        // same code path as opencode above, just different constructor args
        // (see `platforms.rs`), which is all that used to distinguish the
        // now-deleted `MimoCodeReader` wrapper from `OpencodeReader`.
        let conn = setup();
        const MIMO_ASSISTANT: &str = r#"{"role":"assistant","modelID":"mimo-v2.5-pro","providerID":"xiaomi","cost":0.0,"tokens":{"input":100,"output":40,"reasoning":10,"cache":{"read":5,"write":2}},"time":{"created":1000}}"#;
        insert(&conn, "m1", 1000, MIMO_ASSISTANT);
        let mut reader = SqliteMessageReader::from_connection(conn, Platform::MimoCode, "mimocode");

        let records = reader.scan_all();
        assert_eq!(records.len(), 1);
        assert_eq!(
            crate::state::resolve(records[0].model),
            "xiaomi/mimo-v2.5-pro"
        );
        assert_eq!(records[0].platform, Platform::MimoCode);
    }
}
