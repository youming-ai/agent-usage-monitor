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
        files_read: 0,
        files_edited: 0,
        files_added: 0,
        files_deleted: 0,
        terminal_commands: 0,
        lines_read: 0,
        lines_edited: 0,
    })
}
