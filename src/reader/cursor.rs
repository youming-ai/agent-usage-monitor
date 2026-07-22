use crate::reader::pricing;
use crate::reader::{UsageSource, find_recursive, is_under_dir_named, is_uuid, session_label};
use crate::state::{Platform, UsageRecord};
use chrono::{TimeZone, Utc};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};

const CHARS_PER_TOKEN: f64 = 4.0;

/// Tracks turn assembly while scanning Composer 2 JSONL transcripts.
#[derive(Clone, Default)]
struct TranscriptTurnState {
    project: String,
    cwd: String,
    conversation_id: String,
    model: String,
    finalized_turns: u64,
    current_user_chars: usize,
    current_assistant_chars: usize,
    /// How much of `current_assistant_chars` has already been emitted as a
    /// record. A turn's assistant reply can arrive as several separate JSONL
    /// lines (streamed chunks) across multiple poll cycles; flushing only the
    /// *growth* since the last flush (instead of finalizing the whole turn on
    /// the first chunk) means later chunks are never silently dropped.
    counted_assistant_chars: usize,
    /// Whether this turn's (non-growing) input/user tokens have already been
    /// billed in an earlier flush.
    input_counted: bool,
    has_user: bool,
    has_assistant: bool,
    /// Captured from a "timestamp" field on a user/assistant line if present in the JSONL.
    /// Allows historical transcripts to carry real event times instead of observation time.
    current_turn_timestamp: Option<chrono::DateTime<chrono::Utc>>,
}

pub struct CursorReader {
    data_dir: PathBuf,
    extra_chats_dir: PathBuf,
    transcript_positions: HashMap<PathBuf, u64>,
    transcript_state: HashMap<PathBuf, TranscriptTurnState>,
    store_seen_keys: HashMap<PathBuf, HashSet<String>>,
    conversation_models: HashMap<String, String>,
}

impl CursorReader {
    pub fn new(data_dir: PathBuf) -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let extra_chats_dir = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"))
            .join("cursor/chats");
        Self {
            data_dir,
            extra_chats_dir,
            transcript_positions: HashMap::new(),
            transcript_state: HashMap::new(),
            store_seen_keys: HashMap::new(),
            conversation_models: HashMap::new(),
        }
    }

    fn reload_conversation_models(&mut self) {
        let db_path = self.data_dir.join("ai-tracking/ai-code-tracking.db");
        let Ok(conn) = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        else {
            return;
        };
        let Ok(mut stmt) = conn.prepare(
            "SELECT conversationId, model FROM conversation_summaries WHERE model IS NOT NULL",
        ) else {
            return;
        };
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        });
        if let Ok(rows) = rows {
            for row in rows.flatten() {
                self.conversation_models.insert(row.0, row.1);
            }
        }
    }

    fn find_transcript_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        let projects = self.data_dir.join("projects");
        if projects.exists() {
            find_recursive(&projects, &mut files, &|p| {
                p.extension().map(|e| e == "jsonl").unwrap_or(false)
                    && is_under_dir_named(p, "agent-transcripts")
            });
        }
        files
    }

    fn find_store_db_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        for root in [self.data_dir.join("chats"), self.extra_chats_dir.clone()] {
            if root.exists() {
                find_recursive(&root, &mut files, &|p| {
                    p.file_name().is_some_and(|n| n == "store.db")
                });
            }
        }
        files
    }

    fn scan_all_inner(&mut self, from_start: bool) -> Vec<UsageRecord> {
        self.reload_conversation_models();

        if from_start {
            self.transcript_positions.clear();
            self.transcript_state.clear();
            self.store_seen_keys.clear();
        }

        let transcript_files = self.find_transcript_files();
        let store_files = self.find_store_db_files();
        let current_transcripts: HashSet<PathBuf> = transcript_files.iter().cloned().collect();
        let current_stores: HashSet<PathBuf> = store_files.iter().cloned().collect();

        self.transcript_positions
            .retain(|p, _| current_transcripts.contains(p));
        self.transcript_state
            .retain(|p, _| current_transcripts.contains(p));
        self.store_seen_keys
            .retain(|p, _| current_stores.contains(p));

        let mut records = Vec::new();
        for file in transcript_files {
            let offset = if from_start {
                0
            } else {
                self.transcript_positions.get(&file).copied().unwrap_or(0)
            };
            let mut st = self
                .transcript_state
                .get(&file)
                .cloned()
                .unwrap_or_else(|| transcript_meta_for(&file, &self.conversation_models));
            let (entries, bytes_read) = read_transcript_from_offset(&file, offset, &mut st);
            let mut entries = entries;

            if offset == 0 {
                // Full (re)read of the transcript file (historical scan_all or after
                // truncation). For data without embedded per-line timestamps, stamp the
                // batch with the file's mtime as a reasonable historical proxy.
                // But *only* override timestamps that look like they came from Utc::now()
                // inside finalize (recent relative to this scan). If the line carried an
                // explicit "timestamp", keep the precise value.
                // Incremental delta appends (offset > 0) keep their now()/captured ts.
                let scan_now = Utc::now();
                if let Ok(md) = std::fs::metadata(&file)
                    && let Ok(modified) = md.modified()
                    && let Ok(dur) = modified.duration_since(std::time::UNIX_EPOCH)
                    && let Some(file_ts) = Utc.timestamp_opt(dur.as_secs() as i64, 0).single()
                {
                    for r in &mut entries {
                        let age_secs = scan_now
                            .signed_duration_since(r.timestamp)
                            .num_seconds()
                            .abs();
                        if age_secs < 300 {
                            r.timestamp = file_ts;
                        }
                    }
                }
            }

            self.transcript_positions.insert(file.clone(), bytes_read);
            self.transcript_state.insert(file, st);
            records.extend(entries);
        }

        for file in store_files {
            records.extend(self.read_store_delta(&file));
        }

        records.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        records
    }

    fn read_store_delta(&mut self, path: &Path) -> Vec<UsageRecord> {
        let conn = match Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let table_exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='blobs')",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if !table_exists {
            return Vec::new();
        }

        // Always scan from rowid 0 — dedup is handled by `store_seen_keys`
        // (HashSet), which survives store.db replacement/rotation (unlike a
        // rowid resume cursor, which was the root cause of silent data loss
        // after store.db recreation: a fresh db's rowids restart at 1, so a
        let session_id = extract_store_session_id(path);
        let project = extract_store_project(path);
        let cwd = decode_cursor_project_cwd(&project.raw);
        let session = session_label(&project.display, &session_id);

        let mut stmt = match conn
            .prepare("SELECT rowid, key, value FROM blobs WHERE rowid > 0 ORDER BY rowid")
        {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        });
        let Ok(rows) = rows else {
            return Vec::new();
        };

        let seen = self.store_seen_keys.entry(path.to_path_buf()).or_default();
        let mut records = Vec::new();

        for row in rows.flatten() {
            let (_rowid, key, value) = row;
            if let Some(rec) = parse_store_blob(&value, &key, &session, &session_id, &cwd, seen) {
                records.push(rec);
            }
        }

        records
    }
}

impl UsageSource for CursorReader {
    fn platform(&self) -> Platform {
        Platform::Cursor
    }

    fn scan_all(&mut self) -> Vec<UsageRecord> {
        self.scan_all_inner(true)
    }

    fn poll_delta(&mut self) -> Vec<UsageRecord> {
        self.scan_all_inner(false)
    }
}

/// A Cursor "project" identifier. `display` is the human-friendly label shown
/// in the sessions list; `raw` is the original hyphen-encoded folder segment
/// used to reconstruct the working directory. Bundled so the two strings can't
/// be transposed at a call site.
struct CursorProject {
    display: String,
    raw: String,
}

impl CursorProject {
    fn unknown() -> Self {
        Self {
            display: "cursor".to_string(),
            raw: String::new(),
        }
    }

    fn from_encoded(raw: &str) -> Self {
        Self {
            display: prettify_project_id(raw),
            raw: raw.to_string(),
        }
    }
}

fn transcript_meta_for(path: &Path, models: &HashMap<String, String>) -> TranscriptTurnState {
    let (project, conversation_id) = extract_transcript_paths(path);
    let cwd = decode_cursor_project_cwd(&project.raw);
    let model = models
        .get(&conversation_id)
        .cloned()
        .unwrap_or_else(|| "cursor-auto".to_string());
    TranscriptTurnState {
        project: project.display,
        cwd,
        conversation_id,
        model,
        ..Default::default()
    }
}

fn extract_transcript_paths(path: &Path) -> (CursorProject, String) {
    let mut project = CursorProject::unknown();
    let mut conversation_id = String::new();

    for ancestor in path.ancestors() {
        if let Some(name) = ancestor.file_name().and_then(|n| n.to_str()) {
            if name == "agent-transcripts" {
                if let Some(proj) = ancestor
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                {
                    project = CursorProject::from_encoded(proj);
                }
                break;
            }
            if conversation_id.is_empty() && is_uuid(name) {
                conversation_id = name.to_string();
            }
        }
    }

    if conversation_id.is_empty() {
        conversation_id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();
    }

    (project, conversation_id)
}

fn extract_store_session_id(path: &Path) -> String {
    path.parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string()
}

fn extract_store_project(path: &Path) -> CursorProject {
    path.ancestors()
        .find_map(|a| {
            let name = a.file_name()?.to_str()?;
            if name == "chats" {
                None
            } else if a
                .parent()
                .is_some_and(|p| p.file_name().is_some_and(|n| n == "chats"))
            {
                Some(CursorProject::from_encoded(name))
            } else {
                None
            }
        })
        .unwrap_or_else(CursorProject::unknown)
}

fn prettify_project_id(raw: &str) -> String {
    let stripped = raw.strip_prefix("-Users-").unwrap_or(raw);
    stripped
        .split('-')
        .rfind(|s| !s.is_empty())
        .unwrap_or(stripped)
        .to_string()
}
fn decode_cursor_project_cwd(raw: &str) -> String {
    if raw.is_empty() {
        return String::new();
    }
    let stripped = raw.strip_prefix('-').unwrap_or(raw);
    let parts: Vec<&str> = stripped.split('-').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        return String::new();
    }

    let mut current = PathBuf::from("/");
    let mut idx = 0;
    while idx < parts.len() {
        let mut matched = false;
        // Try longest candidate first (greedy match for hyphenated folder names)
        for len in (1..=(parts.len() - idx)).rev() {
            let candidate_name = parts[idx..idx + len].join("-");
            let candidate_path = current.join(&candidate_name);
            if candidate_path.is_dir() {
                current = candidate_path;
                idx += len;
                matched = true;
                break;
            }
        }
        if !matched {
            return String::new();
        }
    }

    current.to_str().unwrap_or("").to_string()
}

fn read_transcript_from_offset(
    path: &Path,
    skip_bytes: u64,
    st: &mut TranscriptTurnState,
) -> (Vec<UsageRecord>, u64) {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return (Vec::new(), skip_bytes),
    };

    let file_len = file.metadata().map(|m| m.len()).unwrap_or(0);
    if skip_bytes > file_len {
        // File was truncated/rewritten. The in-progress turn's counters
        // (counted_assistant_chars, input_counted, ...) are stale baselines
        // from content that may no longer exist at those char offsets in the
        // rewritten file — keep them around and deltas would come out wrong
        // or get suppressed entirely. Reset the turn-in-progress bookkeeping
        // (but keep the file-level metadata: project/conversation_id/model).
        st.reset_current_turn();
        return read_transcript_from_offset(path, 0, st);
    }

    let mut reader = BufReader::new(file);
    if skip_bytes > 0 && reader.seek(SeekFrom::Start(skip_bytes)).is_err() {
        return read_transcript_from_offset(path, 0, st);
    }

    let mut records = Vec::new();
    let mut offset = skip_bytes;

    while let Some((line, bytes)) = crate::reader::read_next_line(&mut reader) {
        if let Some(rec) = parse_transcript_line(line.trim_end_matches(['\r', '\n']), st) {
            records.push(rec);
        }
        offset += bytes;
    }

    if let Some(rec) = flush_transcript_progress(st) {
        records.push(rec);
    }

    (records, offset)
}

fn parse_transcript_line(line: &str, st: &mut TranscriptTurnState) -> Option<UsageRecord> {
    let v: Value = serde_json::from_str(line).ok()?;
    let role = v.get("role").and_then(|r| r.as_str())?;

    // Capture embedded timestamp if the transcript line carries one (consistent with
    // grok/codex/claude/pi/etc readers). This is used in finalize for the turn.
    if let Some(ts) = v.get("timestamp").and_then(|t| t.as_str()).and_then(|s| {
        chrono::DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .ok()
    }) {
        st.current_turn_timestamp = Some(ts);
    }

    match role {
        "user" => {
            // A new user line unambiguously ends the previous turn — flush
            // whatever assistant growth is still pending for it, then start
            // fresh for this one.
            let record = flush_transcript_progress(st);
            let chars = content_char_count(v.get("message"));
            st.reset_current_turn();
            st.current_user_chars = chars;
            st.has_user = chars > 0;
            record
        }
        "assistant" => {
            st.current_assistant_chars += content_char_count(v.get("message"));
            st.has_assistant = st.current_assistant_chars > 0;
            None
        }
        _ => None,
    }
}

/// Emit a record for assistant growth since the last flush of this turn, if
/// any. Unlike a one-shot "finalize", this does **not** end the turn: a reply
/// can arrive as several JSONL lines across multiple poll cycles (streamed
/// chunks), and this is called at the end of every incremental read. Flushing
/// only the delta — rather than finalizing (and resetting `has_user`) on the
/// very first chunk — means later chunks of the same still-open turn are
/// never silently dropped. The turn is only fully reset when a genuine new
/// "user" line starts the next one (see `parse_transcript_line`).
fn flush_transcript_progress(st: &mut TranscriptTurnState) -> Option<UsageRecord> {
    if !st.has_user || !st.has_assistant {
        return None;
    }

    let input_chars = if st.input_counted {
        0
    } else {
        st.current_user_chars
    };
    let assistant_delta_chars = st
        .current_assistant_chars
        .saturating_sub(st.counted_assistant_chars);

    let input = estimate_tokens(input_chars);
    let output = estimate_tokens(assistant_delta_chars);
    if input == 0 && output == 0 {
        return None;
    }

    let ts = st.current_turn_timestamp.unwrap_or_else(Utc::now);
    st.finalized_turns += 1;
    st.input_counted = true;
    st.counted_assistant_chars = st.current_assistant_chars;

    let model = normalize_cursor_model(&st.model);
    let cost = pricing::calculate_cost(&model, input, output, 0, 0);
    let record_id = format!("{}:{}", st.conversation_id, st.finalized_turns);

    Some(UsageRecord {
        timestamp: ts,
        platform: Platform::Cursor,
        model: crate::state::intern(&model),
        session: crate::state::intern(&session_label(&st.project, &st.conversation_id)),
        session_id: crate::state::intern(&st.conversation_id),
        // Cursor's "project" is a display label, not a filesystem path.
        cwd: crate::state::intern(&st.cwd),
        title: crate::state::intern(""),
        id: crate::state::intern(&record_id),
        input_tokens: input,
        output_tokens: output,
        cache_read_tokens: 0,
        cache_creation_tokens: 0,
        cost_usd: cost,
    })
}

impl TranscriptTurnState {
    fn reset_current_turn(&mut self) {
        self.current_user_chars = 0;
        self.current_assistant_chars = 0;
        self.counted_assistant_chars = 0;
        self.input_counted = false;
        self.has_user = false;
        self.has_assistant = false;
        self.current_turn_timestamp = None;
    }
}

fn parse_store_blob(
    value: &str,
    key: &str,
    session: &str,
    session_id: &str,
    cwd: &str,
    seen: &mut HashSet<String>,
) -> Option<UsageRecord> {
    let v: Value = serde_json::from_str(value).ok()?;

    // Cursor bubble / agentKv format from state.vscdb and store.db. The blob's
    // `key` (the blobs table's own primary key) is a stable identity for this
    // row; it does NOT change when the row is updated in place, unlike the
    // token counts the dedup key used to be built from — those mutate as a
    // bubble streams in, so keying on them made an in-place update look like
    // a brand-new (and thus double-counted) row, and made two distinct
    // bubbles that happened to share a createdAt+token-count collapse into one.
    if let Some(rec) = parse_bubble_usage(&v, session) {
        let dedup = format!("bubble:{session_id}:{key}");
        if !seen.insert(dedup.clone()) {
            return None;
        }
        let mut rec = rec;
        rec.id = crate::state::intern(&dedup);
        rec.session_id = crate::state::intern(session_id);
        rec.cwd = crate::state::intern(cwd);
        return Some(rec);
    }

    // CLI message blobs: {"role":"assistant","content":[...]}
    if let Some(role) = v.get("role").and_then(|r| r.as_str()) {
        if role != "assistant" {
            return None;
        }
        let id = v.get("id").and_then(|i| i.as_str()).unwrap_or(key);
        let dedup = format!("msg:{session_id}:{id}");
        if !seen.insert(dedup.clone()) {
            return None;
        }
        let output = estimate_tokens(content_char_count(Some(&v)));
        if output == 0 {
            return None;
        }
        let model = v
            .get("model")
            .and_then(|m| m.as_str())
            .map(normalize_cursor_model)
            .unwrap_or_else(|| "cursor-auto".to_string());
        let cost = pricing::calculate_cost(&model, 0, output, 0, 0);
        // Historical CLI message blobs carry no reliable per-record clock in
        // every Cursor version; prefer an embedded timestamp when present so
        // old messages don't land on "today" (see also parse_bubble_usage's
        // createdAt handling), falling back to observation time only when
        // truly nothing is available.
        let timestamp = v
            .get("timestamp")
            .and_then(|t| t.as_i64())
            .or_else(|| v.get("createdAt").and_then(|t| t.as_i64()))
            .and_then(|ms| {
                let secs = if ms > 1_000_000_000_000 {
                    ms / 1000
                } else {
                    ms
                };
                Utc.timestamp_opt(secs, 0).single()
            })
            .unwrap_or_else(Utc::now);
        return Some(UsageRecord {
            timestamp,
            platform: Platform::Cursor,
            model: crate::state::intern(&model),
            session: crate::state::intern(session),
            session_id: crate::state::intern(session_id),
            cwd: crate::state::intern(cwd),
            title: crate::state::intern(""),
            id: crate::state::intern(&dedup),
            input_tokens: 0,
            output_tokens: output,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            cost_usd: cost,
        });
    }

    None
}

fn parse_bubble_usage(v: &Value, session: &str) -> Option<UsageRecord> {
    let bubble_type = v.get("type").and_then(|t| t.as_i64());
    if bubble_type != Some(0) {
        return None;
    }

    let usage = v.get("tokenCount")?;
    let input = usage
        .get("inputTokens")
        .and_then(|t| t.as_u64())
        .unwrap_or(0);
    let mut output = usage
        .get("outputTokens")
        .and_then(|t| t.as_u64())
        .unwrap_or(0);

    if input == 0 && output == 0 {
        let text_len = v
            .get("text")
            .and_then(|t| t.as_str())
            .map(|s| s.len())
            .unwrap_or(0);
        output = estimate_tokens(text_len);
    }

    if input == 0 && output == 0 {
        return None;
    }

    let model = v
        .get("modelInfo")
        .and_then(|m| m.get("modelName"))
        .and_then(|m| m.as_str())
        .map(normalize_cursor_model)
        .unwrap_or_else(|| "cursor-auto".to_string());

    let ts_ms = v.get("createdAt").and_then(|t| t.as_i64()).unwrap_or(0);
    let timestamp = if ts_ms > 0 {
        let secs = if ts_ms > 1_000_000_000_000 {
            ts_ms / 1000
        } else {
            ts_ms
        };
        Utc.timestamp_opt(secs, 0).single()?
    } else {
        Utc::now()
    };

    let cost = pricing::calculate_cost(&model, input, output, 0, 0);

    Some(UsageRecord {
        timestamp,
        platform: Platform::Cursor,
        model: crate::state::intern(&model),
        session: crate::state::intern(session),
        // Both overwritten by the caller (`parse_store_blob`): `id` with the
        // blob's stable `key`-based dedup id, `session_id` with the real
        // conversation id. Placeholders here so this fn works standalone.
        session_id: crate::state::intern(""),
        cwd: crate::state::intern(""),
        title: crate::state::intern(""),
        id: crate::state::intern(""),
        input_tokens: input,
        output_tokens: output,
        cache_read_tokens: 0,
        cache_creation_tokens: 0,
        cost_usd: cost,
    })
}

fn content_char_count(message: Option<&Value>) -> usize {
    let Some(message) = message else {
        return 0;
    };

    if let Some(content) = message.get("content") {
        if let Some(text) = content.as_str() {
            return text.len();
        }
        if let Some(blocks) = content.as_array() {
            let mut total = 0;
            for block in blocks {
                if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                    total += text.len();
                }
                if let Some(input) = block.get("input") {
                    total += input.to_string().len();
                }
            }
            return total;
        }
    }

    message
        .get("text")
        .and_then(|t| t.as_str())
        .map(|s| s.len())
        .unwrap_or(0)
}

fn estimate_tokens(chars: usize) -> u64 {
    if chars == 0 {
        0
    } else {
        ((chars as f64) / CHARS_PER_TOKEN).ceil() as u64
    }
}

fn normalize_cursor_model(raw: &str) -> String {
    if raw.is_empty() || raw == "default" {
        "cursor-auto".to_string()
    } else {
        raw.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    fn write_transcript(dir: &Path, project: &str, conversation: &str, lines: &[&str]) -> PathBuf {
        let path = dir
            .join("projects")
            .join(project)
            .join("agent-transcripts")
            .join(conversation)
            .join(format!("{conversation}.jsonl"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let content: String = lines.iter().map(|l| format!("{l}\n")).collect();
        fs::write(&path, content).unwrap();
        path
    }

    fn write_store_db(dir: &Path, hash: &str, session: &str, blobs: &[(&str, &str)]) -> PathBuf {
        let path = dir.join("chats").join(hash).join(session);
        fs::create_dir_all(&path).unwrap();
        let db_path = path.join("store.db");
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("CREATE TABLE blobs (key TEXT PRIMARY KEY, value TEXT NOT NULL);")
            .unwrap();
        for (key, value) in blobs {
            conn.execute(
                "INSERT INTO blobs (key, value) VALUES (?1, ?2)",
                [*key, *value],
            )
            .unwrap();
        }
        db_path
    }

    #[test]
    fn parses_transcript_turns_with_char_estimation() {
        let dir = tempfile::tempdir().unwrap();
        write_transcript(
            dir.path(),
            "Users-me-myproject",
            "a3f2c1d8-10e5-4b2a-9c1d-ef0123456789",
            &[
                r#"{"role":"user","message":{"content":[{"type":"text","text":"Hello world"}]}}"#,
                r#"{"role":"assistant","message":{"content":[{"type":"text","text":"Hi there, how can I help?"}]}}"#,
            ],
        );

        let mut reader = CursorReader::new(dir.path().to_path_buf());
        let records = reader.scan_all();
        assert_eq!(records.len(), 1);
        assert!(records[0].input_tokens > 0);
        assert!(records[0].output_tokens > 0);
        assert_eq!(records[0].platform, Platform::Cursor);
        assert_eq!(
            crate::state::resolve(records[0].session),
            "myproject a3f2c1d8"
        );
    }

    #[test]
    fn parses_store_db_bubble_token_counts() {
        let dir = tempfile::tempdir().unwrap();
        write_store_db(
            dir.path(),
            "abc123",
            "sess-uuid-1",
            &[(
                "bubble1",
                r#"{"type":0,"createdAt":1780625874436,"tokenCount":{"inputTokens":120,"outputTokens":45},"modelInfo":{"modelName":"claude-sonnet-4-5"},"text":"done"}"#,
            )],
        );

        let mut reader = CursorReader::new(dir.path().to_path_buf());
        let records = reader.scan_all();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].input_tokens, 120);
        assert_eq!(records[0].output_tokens, 45);
        assert_eq!(crate::state::resolve(records[0].model), "claude-sonnet-4-5");
    }

    #[test]
    fn poll_delta_returns_only_new_transcript_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_transcript(
            dir.path(),
            "Users-me-repo",
            "b3f2c1d8-10e5-4b2a-9c1d-ef0123456789",
            &[
                r#"{"role":"user","message":{"content":[{"type":"text","text":"first"}]}}"#,
                r#"{"role":"assistant","message":{"content":[{"type":"text","text":"reply one"}]}}"#,
            ],
        );

        let mut reader = CursorReader::new(dir.path().to_path_buf());
        assert_eq!(reader.scan_all().len(), 1);
        assert!(reader.poll_delta().is_empty());

        let mut f = fs::OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(
            f,
            r#"{{"role":"user","message":{{"content":[{{"type":"text","text":"second"}}]}}}}"#
        )
        .unwrap();
        writeln!(
            f,
            r#"{{"role":"assistant","message":{{"content":[{{"type":"text","text":"reply two"}}]}}}}"#
        )
        .unwrap();

        let delta = reader.poll_delta();
        assert_eq!(delta.len(), 1);
        assert!(delta[0].output_tokens > 0);
    }

    #[test]
    fn missing_directory_yields_empty() {
        let mut reader = CursorReader::new(PathBuf::from("/nonexistent/cursor"));
        assert!(reader.scan_all().is_empty());
        assert!(reader.poll_delta().is_empty());
    }

    #[test]
    fn store_db_replacement_after_scan_reads_new_records_on_poll() {
        let dir = tempfile::tempdir().unwrap();
        // First db with one bubble record (will get rowid=1)
        let _db1 = write_store_db(
            dir.path(),
            "abc123",
            "sess-uuid-1",
            &[(
                "bubble1",
                r#"{"type":0,"createdAt":1780625874436,"tokenCount":{"inputTokens":10,"outputTokens":5},"modelInfo":{"modelName":"claude-sonnet-4-5"},"text":"v1"}"#,
            )],
        );

        let mut reader = CursorReader::new(dir.path().to_path_buf());
        assert_eq!(reader.scan_all().len(), 1);

        // Simulate Cursor replacing/rotating the store.db file (fresh DB, rowids restart at 1)
        // Delete then recreate at the *same* path. The in-memory last_rowid remains high (1).
        let chats_dir = dir.path().join("chats").join("abc123").join("sess-uuid-1");
        let db_path = chats_dir.join("store.db");
        let _ = fs::remove_file(&db_path);

        let _db2 = write_store_db(
            dir.path(),
            "abc123",
            "sess-uuid-1",
            &[(
                "bubble2",
                r#"{"type":0,"createdAt":1780625875000,"tokenCount":{"inputTokens":20,"outputTokens":7},"modelInfo":{"modelName":"claude-sonnet-4-5"},"text":"v2"}"#,
            )],
        );

        // With the bug, last_rowid=1 and new rowid=1 => WHERE rowid > 1 yields nothing.
        // poll_delta (or any subsequent) should still surface the new record.
        let delta = reader.poll_delta();
        assert_eq!(
            delta.len(),
            1,
            "expected to read new records after store.db replacement/rotation"
        );
        assert_eq!(delta[0].input_tokens, 20);
        assert_eq!(delta[0].output_tokens, 7);
    }

    #[test]
    fn transcript_historical_timestamp_prefers_embedded_or_file_mtime_over_now() {
        let dir = tempfile::tempdir().unwrap();
        // Embed a distinctive historical timestamp in the transcript lines.
        // The parser now extracts it (like every other reader); the test asserts
        // we don't emit pure Utc::now() for historical data.
        let past = "2025-03-10T08:30:00Z";
        let user_line = format!(
            r#"{{"role":"user","timestamp":"{}","message":{{"content":[{{"type":"text","text":"question"}}]}}}}"#,
            past
        );
        let assistant_line = format!(
            r#"{{"role":"assistant","timestamp":"{}","message":{{"content":[{{"type":"text","text":"answer"}}]}}}}"#,
            past
        );
        let _path = write_transcript(
            dir.path(),
            "Users-me-tshist",
            "d4f2c1d8-10e5-4b2a-9c1d-ef0123456789",
            &[user_line.as_str(), assistant_line.as_str()],
        );

        let mut reader = CursorReader::new(dir.path().to_path_buf());
        let records = reader.scan_all();
        assert_eq!(records.len(), 1);
        // The ts from the line was captured (via parse_from_rfc3339) and used in finalize
        // before reset. For real Cursor data lacking the field, the offset==0 mtime override
        // supplies historical proxy instead of pure observation-time now().
        let got = records[0].timestamp.to_rfc3339();
        assert!(got.starts_with("2025-03-10T08:30:00"), "got ts {}", got);
    }

    #[test]
    fn embedded_timestamp_in_the_same_year_as_now_is_not_overwritten() {
        // C5 regression: a precise embedded timestamp must be kept even when
        // its *year* happens to match the scan year — only a genuinely
        // recent (age-based) synthetic now() stamp should ever be replaced
        // by the file mtime proxy.
        let this_year = Utc::now().format("%Y").to_string();
        let past = format!("{this_year}-01-02T03:04:05Z");
        let user_line = format!(
            r#"{{"role":"user","timestamp":"{past}","message":{{"content":[{{"type":"text","text":"question"}}]}}}}"#
        );
        let assistant_line = format!(
            r#"{{"role":"assistant","timestamp":"{past}","message":{{"content":[{{"type":"text","text":"answer"}}]}}}}"#
        );
        let dir = tempfile::tempdir().unwrap();
        write_transcript(
            dir.path(),
            "Users-me-sameyear",
            "e4f2c1d8-10e5-4b2a-9c1d-ef0123456789",
            &[user_line.as_str(), assistant_line.as_str()],
        );

        let mut reader = CursorReader::new(dir.path().to_path_buf());
        let records = reader.scan_all();
        assert_eq!(records.len(), 1);
        assert!(
            records[0].timestamp.to_rfc3339().starts_with(&past[..10]),
            "same-year embedded timestamp was overwritten with file mtime, got {}",
            records[0].timestamp.to_rfc3339()
        );
    }

    #[test]
    fn multi_chunk_assistant_reply_across_polls_is_not_dropped() {
        // C3 regression: an assistant reply streamed as several JSONL lines
        // across multiple poll_delta calls must have every chunk counted —
        // not just the first one that happened to land before a poll fired.
        let dir = tempfile::tempdir().unwrap();
        let path = write_transcript(
            dir.path(),
            "Users-me-stream",
            "f4f2c1d8-10e5-4b2a-9c1d-ef0123456789",
            &[
                r#"{"role":"user","message":{"content":[{"type":"text","text":"question needing a long reply"}]}}"#,
            ],
        );

        let mut reader = CursorReader::new(dir.path().to_path_buf());
        assert!(
            reader.scan_all().is_empty(),
            "no assistant content yet, nothing to bill"
        );

        // First assistant chunk arrives; a poll fires before the reply is done.
        {
            let mut f = fs::OpenOptions::new().append(true).open(&path).unwrap();
            writeln!(
                f,
                r#"{{"role":"assistant","message":{{"content":[{{"type":"text","text":"first part of a long answer "}}]}}}}"#
            )
            .unwrap();
        }
        let first = reader.poll_delta();
        assert_eq!(first.len(), 1, "first chunk should be billed immediately");
        let first_output = first[0].output_tokens;
        assert!(first_output > 0);

        // A second chunk of the SAME reply arrives on a later poll. With the
        // bug, this chunk's content would be silently dropped because the
        // turn was already (prematurely) finalized above.
        {
            let mut f = fs::OpenOptions::new().append(true).open(&path).unwrap();
            writeln!(
                f,
                r#"{{"role":"assistant","message":{{"content":[{{"type":"text","text":"and here is the second part of the long answer"}}]}}}}"#
            )
            .unwrap();
        }
        let second = reader.poll_delta();
        assert_eq!(
            second.len(),
            1,
            "second chunk of the same still-open turn must still be billed"
        );
        assert!(second[0].output_tokens > 0);
    }

    #[test]
    fn store_db_in_place_bubble_update_is_not_double_counted() {
        // C4 regression: dedup must key on the blob's stable identity (its
        // `key` in the blobs table), not on the mutable token counts — an
        // in-place update of the SAME bubble (same key, growing tokens) must
        // not re-emit as a second record on the next poll.
        let dir = tempfile::tempdir().unwrap();
        let db_path = write_store_db(
            dir.path(),
            "abc123",
            "sess-uuid-1",
            &[(
                "bubble1",
                r#"{"type":0,"createdAt":1780625874436,"tokenCount":{"inputTokens":10,"outputTokens":5},"modelInfo":{"modelName":"claude-sonnet-4-5"},"text":"partial"}"#,
            )],
        );

        let mut reader = CursorReader::new(dir.path().to_path_buf());
        assert_eq!(reader.scan_all().len(), 1);
        assert!(reader.poll_delta().is_empty());

        // Update the SAME row (same `key`) in place with grown token counts —
        // this is what Cursor does as a bubble streams in.
        let conn = Connection::open(&db_path).unwrap();
        conn.execute(
            "UPDATE blobs SET value = ?1 WHERE key = 'bubble1'",
            [r#"{"type":0,"createdAt":1780625874436,"tokenCount":{"inputTokens":10,"outputTokens":45},"modelInfo":{"modelName":"claude-sonnet-4-5"},"text":"done"}"#],
        )
        .unwrap();

        // The reader re-scans store.db from rowid 0 on every poll by design
        // (see read_store_delta); the local `seen` key must still recognize
        // this as the same row and suppress it.
        let delta = reader.poll_delta();
        assert!(
            delta.is_empty(),
            "in-place update of the same bubble key must not double-count, got {delta:?}"
        );
    }

    #[test]
    fn store_db_distinct_bubbles_with_identical_createdat_and_tokens_both_counted() {
        // C4 regression: two genuinely distinct bubbles that happen to share
        // the same createdAt + token counts must both be counted — the old
        // dedup key (createdAt + tokens, no row identity) would have
        // collapsed them into one.
        let dir = tempfile::tempdir().unwrap();
        write_store_db(
            dir.path(),
            "abc123",
            "sess-uuid-2",
            &[
                (
                    "bubbleA",
                    r#"{"type":0,"createdAt":1780625874436,"tokenCount":{"inputTokens":10,"outputTokens":5},"modelInfo":{"modelName":"claude-sonnet-4-5"},"text":"a"}"#,
                ),
                (
                    "bubbleB",
                    r#"{"type":0,"createdAt":1780625874436,"tokenCount":{"inputTokens":10,"outputTokens":5},"modelInfo":{"modelName":"claude-sonnet-4-5"},"text":"b"}"#,
                ),
            ],
        );

        let mut reader = CursorReader::new(dir.path().to_path_buf());
        let records = reader.scan_all();
        assert_eq!(
            records.len(),
            2,
            "two distinct bubbles with identical createdAt+tokens must both survive dedup"
        );
    }

    #[test]
    fn decode_cursor_project_cwd_resolves_hyphenated_existing_directory() {
        let cwd = std::env::current_dir().unwrap();
        let encoded = format!(
            "-{}",
            cwd.to_str()
                .unwrap()
                .trim_start_matches('/')
                .replace('/', "-")
        );
        let decoded = decode_cursor_project_cwd(&encoded);
        assert_eq!(decoded, cwd.to_str().unwrap());

        // Non-existent directory returns empty string
        assert_eq!(
            decode_cursor_project_cwd("Users-nonexistent-folder-12345"),
            ""
        );
    }
}
