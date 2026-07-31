use crate::state::UsageRecord;
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::path::{Path, PathBuf};

use super::FileScanner;
use super::find_recursive;
use super::{ReaderResult, UsageSource};

/// Per-file running state. Pi session files declare their working directory
/// and id in a top-level `type: "session"` line that's skipped on later polls,
/// so the values must persist on the scanner's per-file state rather than in
/// a single shared field that would leak between sessions.
#[derive(Clone, Default)]
struct FileState {
    dir: String,
    cwd: String,
    sid: String,
}

impl FileState {
    /// Seeded from the file itself so records stay attributable — and their
    /// dedup ids stay distinct between sessions — even if the `session` line is
    /// missing. Pi names session files after their id, so the stem is unique
    /// per session; the `session` line overwrites both fields when present.
    fn from_file(file: &Path) -> Self {
        Self {
            dir: "pi".to_string(),
            cwd: String::new(),
            sid: file
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or_default()
                .to_string(),
        }
    }

    fn session(&self) -> String {
        crate::reader::session_label(&self.dir, &short_session_id(&self.sid))
    }
}

/// Pi session ids are UUIDv7, whose leading hex digits are a millisecond
/// timestamp — the first 8 characters `session_label` would keep encode the
/// creation time to ~65s, so two sessions started in the same directory within
/// a minute would collapse into one row. Take the random tail instead; for
/// shorter or non-UUID ids this is the whole id, same as before.
fn short_session_id(id: &str) -> String {
    let chars: Vec<char> = id.chars().collect();
    chars[chars.len().saturating_sub(8)..].iter().collect()
}

pub struct PiReader {
    data_dir: PathBuf,
    scanner: FileScanner<FileState>,
}

impl PiReader {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            scanner: FileScanner::new(),
        }
    }

    fn find_files(&self) -> ReaderResult<Vec<PathBuf>> {
        let mut files = Vec::new();
        if self.data_dir.exists() {
            find_recursive(&self.data_dir, &mut files, &|p| {
                p.extension().map(|e| e == "jsonl").unwrap_or(false)
            })?;
        }
        Ok(files)
    }

    fn scan(&mut self) -> ReaderResult<Vec<UsageRecord>> {
        let files = self.find_files()?;
        self.scanner
            .scan(files, FileState::from_file, |file, offset, st| {
                crate::reader::read_lines_from_offset(file, offset, |line| parse_pi_line(line, st))
            })
    }

    fn scan_changed(&mut self, paths: &[PathBuf]) -> ReaderResult<Vec<UsageRecord>> {
        let mut files = Vec::new();
        for path in paths {
            if !path.is_file()
                || !path.starts_with(&self.data_dir)
                || path
                    .extension()
                    .is_none_or(|extension| extension != "jsonl")
            {
                return self.scan();
            }
            files.push(path.clone());
        }
        files.sort_unstable();
        files.dedup();
        self.scanner
            .scan_changed(files, FileState::from_file, |file, offset, st| {
                crate::reader::read_lines_from_offset(file, offset, |line| parse_pi_line(line, st))
            })
    }
}

impl UsageSource for PiReader {
    fn scan_all(&mut self) -> ReaderResult<Vec<UsageRecord>> {
        self.scanner.reset();
        self.scan()
    }

    fn poll_delta(&mut self) -> ReaderResult<Vec<UsageRecord>> {
        self.scan()
    }

    fn poll_changed(&mut self, paths: &[PathBuf]) -> ReaderResult<Vec<UsageRecord>> {
        self.scan_changed(paths)
    }
}

fn parse_pi_line(line: &str, st: &mut FileState) -> Option<UsageRecord> {
    let v: Value = serde_json::from_str(line).ok()?;

    let line_type = v.get("type")?.as_str()?;

    // Top-level `session` line declares the conversation's working dir and id.
    if line_type == "session" {
        if let Some(cwd) = v.get("cwd").and_then(|c| c.as_str()) {
            st.dir = crate::reader::basename(cwd);
            st.cwd = cwd.to_string();
        }
        if let Some(id) = v.get("id").and_then(|i| i.as_str()) {
            st.sid = id.to_string();
        }
        return None;
    }

    if line_type != "message" {
        return None;
    }

    let message = v.get("message")?;
    if message.get("role")?.as_str()? != "assistant" {
        return None;
    }

    let usage = message.get("usage")?;

    // Every field is optional in the same way: the all-zero check below is what
    // decides whether there is anything to bill, so no single missing key
    // should discard an otherwise-usable record.
    let input = usage.get("input").and_then(|v| v.as_u64()).unwrap_or(0);
    let output = usage.get("output").and_then(|v| v.as_u64()).unwrap_or(0);
    let cache_read = usage.get("cacheRead").and_then(|v| v.as_u64()).unwrap_or(0);
    let cache_write = usage
        .get("cacheWrite")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    if input == 0 && output == 0 && cache_read == 0 && cache_write == 0 {
        return None;
    }

    let model = message
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let timestamp_str = v.get("timestamp")?.as_str()?;
    let timestamp: DateTime<Utc> = timestamp_str.parse().ok()?;

    let cost_usd = usage
        .get("cost")
        .and_then(|c| c.get("total"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    // message.id is the strongest per-event identifier; fall back to the raw
    // line so a truncation-triggered re-read still dedups. Pi's ids are only 8
    // hex digits (32 bits), which collide across sessions well within one
    // process's history, so scope them to the session the way the Codex and
    // Cursor readers do — a bare id would silently drop the colliding record.
    let event_id = v.get("id").and_then(|i| i.as_str()).unwrap_or(line);
    let record_id = format!("{}:{event_id}", st.sid);

    Some(UsageRecord {
        timestamp,
        model: crate::state::intern(&model),
        session: crate::state::intern(&st.session()),
        session_id: crate::state::intern(&st.sid),
        cwd: crate::state::intern(&st.cwd),
        // Pi session files don't record a conversation title.
        title: crate::state::intern(""),
        id: crate::state::record_id(&record_id),
        input_tokens: input,
        output_tokens: output,
        cache_read_tokens: cache_read,
        cache_creation_tokens: cache_write,
        cost_usd,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use std::path::Path;

    fn sample_jsonl() -> String {
        [
            r#"{"type":"session","version":3,"id":"abc123","timestamp":"2026-06-05T10:00:00Z","cwd":"/Users/me/project"}"#,
            r#"{"type":"message","id":"msg1","parentId":null,"timestamp":"2026-06-05T10:00:01Z","message":{"role":"user","content":"hello"}}"#,
            r#"{"type":"message","id":"msg2","parentId":"msg1","timestamp":"2026-06-05T10:00:02Z","message":{"role":"assistant","content":[{"type":"text","text":"Hi!"}],"provider":"anthropic","model":"claude-sonnet-4-5","usage":{"input":100,"output":50,"cacheRead":10,"cacheWrite":5,"totalTokens":165,"cost":{"input":0.015,"output":0.0075,"cacheRead":0.00015,"cacheWrite":0.0001875,"total":0.0228375}},"stopReason":"stop","timestamp":1717584002000}}"#,
            "",
        ]
        .join("\n")
    }

    fn write_file(dir: &Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path
    }

    #[test]
    fn parses_assistant_messages() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "session.jsonl", &sample_jsonl());

        let mut reader = PiReader::new(dir.path().to_path_buf());
        let records = reader.scan_all().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(crate::state::resolve(records[0].model), "claude-sonnet-4-5");
        assert_eq!(records[0].input_tokens, 100);
        assert_eq!(records[0].output_tokens, 50);
        assert_eq!(records[0].cache_read_tokens, 10);
        assert_eq!(records[0].cache_creation_tokens, 5);
        assert_eq!(crate::state::resolve(records[0].session), "project abc123");
        // Real ids and working dir are threaded through for resume.
        assert_eq!(crate::state::resolve(records[0].session_id), "abc123");
        assert_eq!(crate::state::resolve(records[0].cwd), "/Users/me/project");
    }

    /// Pi ids are 8 hex digits, so the same id shows up in different sessions
    /// long before a process's history is large. Scoping the dedup identity to
    /// the session keeps both records; a bare id would make `AppState` treat
    /// the second as already-seen and drop its tokens silently.
    #[test]
    fn same_message_id_in_two_sessions_yields_distinct_record_ids() {
        let dir = tempfile::tempdir().unwrap();
        let session = |sid: &str, cwd: &str| {
            [
                format!(
                    r#"{{"type":"session","id":"{sid}","timestamp":"2026-06-05T10:00:00Z","cwd":"{cwd}"}}"#
                ),
                r#"{"type":"message","id":"70e82758","timestamp":"2026-06-05T10:00:02Z","message":{"role":"assistant","model":"m","usage":{"input":10,"output":5,"cost":{"total":0.01}}}}"#.to_string(),
                String::new(),
            ]
            .join("\n")
        };
        write_file(dir.path(), "a.jsonl", &session("019f8d54-aaaa", "/tmp/one"));
        write_file(dir.path(), "b.jsonl", &session("019f8d54-bbbb", "/tmp/two"));

        let mut reader = PiReader::new(dir.path().to_path_buf());
        let records = reader.scan_all().unwrap();
        assert_eq!(records.len(), 2);
        assert_ne!(
            records[0].id, records[1].id,
            "one session's message id must not shadow another's"
        );
    }

    /// UUIDv7 ids share a leading timestamp, so the label must be built from
    /// the random tail or two sessions started a moment apart in the same
    /// directory become one row (with only one of them resumable).
    #[test]
    fn sessions_started_moments_apart_keep_distinct_labels() {
        let first = "019f8d54-e0d5-7a76-9b45-cc4cca5a02e1";
        let second = "019f8d54-e0d6-7b21-8c33-1af09be7d412";
        assert_eq!(&first[..8], &second[..8], "fixture ids must share a prefix");

        let label = |sid: &str| {
            FileState {
                dir: "myproject".to_string(),
                sid: sid.to_string(),
                ..Default::default()
            }
            .session()
        };
        assert_ne!(label(first), label(second));
    }

    #[test]
    fn poll_delta_returns_nothing_when_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "session.jsonl", &sample_jsonl());

        let mut reader = PiReader::new(dir.path().to_path_buf());
        assert_eq!(reader.scan_all().unwrap().len(), 1);
        assert!(reader.poll_delta().unwrap().is_empty());
    }

    #[test]
    fn appended_assistant_message_is_picked_up() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_file(dir.path(), "session.jsonl", &sample_jsonl());

        let mut reader = PiReader::new(dir.path().to_path_buf());
        assert_eq!(reader.scan_all().unwrap().len(), 1);

        // Append another assistant message and a non-record line.
        let appended = format!(
            "{}\n{}\n",
            r#"{"type":"user","message":{"role":"user","content":"hi again"}}"#,
            r#"{"type":"message","id":"msg3","parentId":"msg2","timestamp":"2026-06-05T10:00:03Z","message":{"role":"assistant","model":"claude-sonnet-4-5","usage":{"input":40,"output":20,"cacheRead":0,"cacheWrite":0,"totalTokens":60,"cost":{"total":0.01}}}}"#,
        );
        let mut f = fs::OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(appended.as_bytes()).unwrap();

        let delta = reader.poll_delta().unwrap();
        assert_eq!(delta.len(), 1, "only the new assistant message");
        assert_eq!(delta[0].input_tokens, 40);
    }
}
