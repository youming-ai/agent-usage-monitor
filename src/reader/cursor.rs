use crate::reader::pricing;
use crate::reader::{UsageSource, find_recursive, is_under_dir_named, session_label};
use crate::state::{Platform, UsageRecord};
use chrono::{TimeZone, Utc};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};

const CHARS_PER_TOKEN: f64 = 4.0;

/// Tracks turn assembly while scanning Composer 2 JSONL transcripts.
#[derive(Clone, Default)]
struct TranscriptTurnState {
    project: String,
    conversation_id: String,
    model: String,
    finalized_turns: u64,
    current_user_chars: usize,
    current_assistant_chars: usize,
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
    store_max_rowid: HashMap<PathBuf, i64>,
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
            store_max_rowid: HashMap::new(),
            store_seen_keys: HashMap::new(),
            conversation_models: HashMap::new(),
        }
    }

    #[allow(dead_code)]
    pub fn default_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".cursor")
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
            self.store_max_rowid.clear();
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
        self.store_max_rowid
            .retain(|p, _| current_stores.contains(p));
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
                if let Ok(md) = std::fs::metadata(&file) {
                    if let Ok(modified) = md.modified() {
                        if let Ok(dur) = modified.duration_since(std::time::UNIX_EPOCH) {
                            if let Some(file_ts) =
                                Utc.timestamp_opt(dur.as_secs() as i64, 0).single()
                            {
                                for r in &mut entries {
                                    let age_secs = scan_now
                                        .signed_duration_since(r.timestamp)
                                        .num_seconds()
                                        .abs();
                                    let rec_year = r.timestamp.format("%Y").to_string();
                                    let scan_year = scan_now.format("%Y").to_string();
                                    if age_secs < 300 || rec_year == scan_year {
                                        // was produced by now() (recent or same-year synthetic) for a
                                        // historical full read (offset==0); use file mtime instead.
                                        // Precise embedded "timestamp" from line (different year or old)
                                        // is kept because age/year check fails.
                                        r.timestamp = file_ts;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            self.transcript_positions.insert(file.clone(), bytes_read);
            self.transcript_state.insert(file, st);
            records.extend(entries);
        }

        for file in store_files {
            records.extend(self.read_store_delta(&file, from_start));
        }

        records.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        records
    }

    fn read_store_delta(&mut self, path: &Path, _from_start: bool) -> Vec<UsageRecord> {
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

        // Always scan from rowid 0 — dedup is handled by `store_seen_keys` (HashSet),
        // which survives store.db replacement/rotation. `store_max_rowid` is just a
        // bookkeeping field tracking the highest rowid ever seen; it is NOT used as
        // a query resume cursor (that was the root cause of silent data loss after
        // store.db recreation).
        let query_start_rowid = 0i64;
        let session_id = extract_store_session_id(path);
        let project = extract_store_project(path);
        let session = session_label(&project, &session_id);

        let mut stmt = match conn
            .prepare("SELECT rowid, key, value FROM blobs WHERE rowid > ?1 ORDER BY rowid")
        {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt.query_map([query_start_rowid], |row| {
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
        let mut max_rowid = query_start_rowid;

        for row in rows.flatten() {
            let (rowid, key, value) = row;
            max_rowid = max_rowid.max(rowid);
            if let Some(rec) = parse_store_blob(&value, &key, &session, &session_id, seen) {
                records.push(rec);
            }
        }

        self.store_max_rowid.insert(path.to_path_buf(), max_rowid);
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

fn transcript_meta_for(path: &Path, models: &HashMap<String, String>) -> TranscriptTurnState {
    let (project, conversation_id) = extract_transcript_paths(path);
    let model = models
        .get(&conversation_id)
        .cloned()
        .unwrap_or_else(|| "cursor-auto".to_string());
    TranscriptTurnState {
        project,
        conversation_id,
        model,
        ..Default::default()
    }
}

fn extract_transcript_paths(path: &Path) -> (String, String) {
    let mut project = "cursor".to_string();
    let mut conversation_id = String::new();

    for ancestor in path.ancestors() {
        if let Some(name) = ancestor.file_name().and_then(|n| n.to_str()) {
            if name == "agent-transcripts" {
                if let Some(proj) = ancestor
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                {
                    project = prettify_project_id(proj);
                }
                break;
            }
            if conversation_id.is_empty() && is_uuid_stem(name) {
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

fn extract_store_project(path: &Path) -> String {
    path.ancestors()
        .find_map(|a| {
            let name = a.file_name()?.to_str()?;
            if name == "chats" {
                None
            } else if a
                .parent()
                .is_some_and(|p| p.file_name().is_some_and(|n| n == "chats"))
            {
                Some(prettify_project_id(name))
            } else {
                None
            }
        })
        .unwrap_or_else(|| "cursor".to_string())
}

fn prettify_project_id(raw: &str) -> String {
    let stripped = raw.strip_prefix("-Users-").unwrap_or(raw);
    stripped
        .split('-')
        .filter(|s| !s.is_empty())
        .next_back()
        .unwrap_or(stripped)
        .to_string()
}

fn is_uuid_stem(stem: &str) -> bool {
    if stem.len() != 36 {
        return false;
    }
    let bytes = stem.as_bytes();
    const DASHES: [usize; 4] = [8, 13, 18, 23];
    bytes.iter().enumerate().all(|(i, &b)| {
        if DASHES.contains(&i) {
            b == b'-'
        } else {
            b.is_ascii_hexdigit()
        }
    })
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
        return read_transcript_from_offset(path, 0, st);
    }

    let mut reader = BufReader::new(file);
    if skip_bytes > 0 && reader.seek(SeekFrom::Start(skip_bytes)).is_err() {
        return read_transcript_from_offset(path, 0, st);
    }

    let mut records = Vec::new();
    let mut offset = skip_bytes;
    let mut line = String::new();

    loop {
        line.clear();
        let bytes = match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(n) => n as u64,
            Err(_) => break,
        };

        if !line.ends_with('\n') {
            break;
        }

        if let Some(rec) = parse_transcript_line(line.trim_end_matches(['\r', '\n']), st) {
            records.push(rec);
        }
        offset += bytes;
    }

    if let Some(rec) = finalize_transcript_turn(st) {
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
            let record = finalize_transcript_turn(st);
            let chars = content_char_count(v.get("message"));
            st.current_user_chars = chars;
            st.current_assistant_chars = 0;
            st.has_user = chars > 0;
            st.has_assistant = false;
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

fn finalize_transcript_turn(st: &mut TranscriptTurnState) -> Option<UsageRecord> {
    if !st.has_user || !st.has_assistant {
        return None;
    }

    let input = estimate_tokens(st.current_user_chars);
    let output = estimate_tokens(st.current_assistant_chars);
    if input == 0 && output == 0 {
        st.reset_current_turn();
        return None;
    }

    let ts = st.current_turn_timestamp.unwrap_or_else(Utc::now);
    st.finalized_turns += 1;
    st.reset_current_turn();

    let model = normalize_cursor_model(&st.model);
    let cost = pricing::calculate_cost(&model, input, output, 0, 0);

    Some(UsageRecord {
        timestamp: ts,
        platform: Platform::Cursor,
        model,
        session: session_label(&st.project, &st.conversation_id),
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
    seen: &mut HashSet<String>,
) -> Option<UsageRecord> {
    let v: Value = serde_json::from_str(value).ok()?;

    // Cursor bubble / agentKv format from state.vscdb and store.db.
    if let Some(rec) = parse_bubble_usage(&v, session) {
        let dedup = format!(
            "bubble:{}:{}:{}:{}",
            session_id,
            v.get("createdAt").and_then(|t| t.as_i64()).unwrap_or(0),
            rec.input_tokens,
            rec.output_tokens
        );
        if !seen.insert(dedup) {
            return None;
        }
        return Some(rec);
    }

    // CLI message blobs: {"role":"assistant","content":[...]}
    if let Some(role) = v.get("role").and_then(|r| r.as_str()) {
        if role != "assistant" {
            return None;
        }
        let id = v.get("id").and_then(|i| i.as_str()).unwrap_or(key);
        let dedup = format!("msg:{session_id}:{id}");
        if !seen.insert(dedup) {
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
        return Some(UsageRecord {
            timestamp: Utc::now(),
            platform: Platform::Cursor,
            model,
            session: session.to_string(),
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
        model,
        session: session.to_string(),
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
        assert_eq!(records[0].session, "myproject a3f2c1d8");
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
        assert_eq!(records[0].model, "claude-sonnet-4-5");
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
}
