use crate::reader::{UsageSource, basename, session_label};
use crate::state::{Platform, UsageRecord};
use chrono::{TimeZone, Utc};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use std::path::PathBuf;
use tracing::warn;

/// Shared SQLite reader for opencode and MiMo Code (and any future agents that
/// log assistant messages into a local `message` / `session` DB schema).
///
/// Opened read-only; if the DB is missing or unreadable, every method returns
/// empty. Both opencode and MiMo Code use this single implementation — the only
/// difference is the platform enum and default provider id.
pub(crate) struct SqliteMessageReader {
    pub(crate) conn: Option<Connection>,
    cursor: i64,
    platform: Platform,
    /// Provider id used when a message blob does not carry an explicit
    /// `providerID` (e.g., "opencode" or "mimocode").
    default_provider: &'static str,
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
        Self {
            conn,
            cursor: 0,
            platform,
            default_provider,
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
            platform,
            default_provider,
        }
    }

    fn query_since(&mut self, cursor: i64) -> Vec<UsageRecord> {
        let Some(conn) = self.conn.as_ref() else {
            return Vec::new();
        };
        let mut stmt = match conn.prepare(
            "SELECT m.data, COALESCE(s.directory, '') AS directory, m.session_id, m.time_created \
             FROM message m LEFT JOIN session s ON m.session_id = s.id \
             WHERE json_extract(m.data, '$.role') = 'assistant' \
               AND m.time_created > ?1 \
             ORDER BY m.time_created",
        ) {
            Ok(s) => s,
            Err(e) => {
                warn!("{:?}: failed to prepare usage query: {e}", self.platform);
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
                if let Some(rec) = parse_message(
                    &data,
                    &directory,
                    &session_id,
                    time_created,
                    self.platform,
                    self.default_provider,
                ) {
                    records.push(rec);
                }
            }
        }
        self.cursor = max_seen;
        records
    }
}

impl UsageSource for SqliteMessageReader {
    fn platform(&self) -> Platform {
        self.platform
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

fn parse_message(
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
        model,
        session,
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
