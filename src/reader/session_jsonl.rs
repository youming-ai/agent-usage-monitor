use crate::state::UsageRecord;
use serde_json::Value;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::Path;

/// Per-file session metadata carried across lines and incremental polls.
#[derive(Clone, Default)]
pub(crate) struct SessionFileState {
    pub dir: String,
    pub sid: String,
}

impl SessionFileState {
    pub fn session_label(&self) -> String {
        crate::reader::session_label(&self.dir, &self.sid)
    }

    pub fn with_dir(dir: &str, sid: &str) -> Self {
        Self {
            dir: dir.to_string(),
            sid: sid.to_string(),
        }
    }
}

/// Update session id / working directory from a top-level `type: "session"` line.
pub(crate) fn apply_session_line(v: &Value, st: &mut SessionFileState) {
    if v.get("type").and_then(|t| t.as_str()) != Some("session") {
        return;
    }
    if let Some(id) = v.get("id").and_then(|x| x.as_str()) {
        st.sid = id.to_string();
    }
    if let Some(cwd) = v.get("cwd").and_then(|x| x.as_str()) {
        st.dir = crate::reader::basename(cwd);
    }
}

/// Read newline-terminated lines from `path` starting at `skip_bytes`, invoking
/// `parse_line` for each. Returns consumed records and the byte offset after the
/// last complete line (an incomplete trailing line is left for the next poll).
pub(crate) fn read_jsonl_from_offset(
    path: &Path,
    skip_bytes: u64,
    st: &mut SessionFileState,
    mut parse_line: impl FnMut(&str, &SessionFileState) -> Option<UsageRecord>,
) -> (Vec<UsageRecord>, u64) {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return (Vec::new(), skip_bytes),
    };

    let file_len = file.metadata().map(|m| m.len()).unwrap_or(0);
    if skip_bytes > file_len {
        // File was truncated or rotated — re-read from the start.
        return read_jsonl_from_offset(path, 0, st, parse_line);
    }

    let mut reader = BufReader::new(file);
    if skip_bytes > 0 && reader.seek(SeekFrom::Start(skip_bytes)).is_err() {
        return read_jsonl_from_offset(path, 0, st, parse_line);
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
            // Incomplete trailing line — leave offset unchanged so we retry it.
            break;
        }

        let trimmed = line.trim_end_matches(['\r', '\n']);
        if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
            apply_session_line(&v, st);
        }
        if let Some(rec) = parse_line(trimmed, st) {
            records.push(rec);
        }
        offset += bytes;
    }

    (records, offset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Platform, UsageRecord};
    use chrono::Utc;
    use std::fs;
    use std::io::Write;

    fn count_assistant_messages(line: &str, st: &SessionFileState) -> Option<UsageRecord> {
        let v: Value = serde_json::from_str(line).ok()?;
        if v.get("type")?.as_str()? != "message" {
            return None;
        }
        if v.get("message")?.get("role")?.as_str()? != "assistant" {
            return None;
        }
        Some(UsageRecord {
            timestamp: Utc::now(),
            platform: Platform::Pi,
            model: crate::state::intern("test"),
            session: crate::state::intern(&st.session_label()),
            input_tokens: 1,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            cost_usd: 0.0,
            files_read: 0,
            files_edited: 0,
            files_added: 0,
            files_deleted: 0,
            terminal_commands: 0,
            lines_read: 0,
            lines_edited: 0,
        })
    }

    fn write_jsonl(path: &Path, content: &str) {
        let mut f = fs::File::create(path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
    }

    #[test]
    fn apply_session_line_updates_dir_and_sid() {
        let v: Value =
            serde_json::from_str(r#"{"type":"session","id":"abc123","cwd":"/Users/me/project"}"#)
                .unwrap();
        let mut st = SessionFileState::default();
        apply_session_line(&v, &mut st);
        assert_eq!(st.sid, "abc123");
        assert_eq!(st.dir, "project");
    }

    #[test]
    fn incomplete_trailing_line_does_not_advance_offset() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.jsonl");
        let session = r#"{"type":"session","id":"s1","cwd":"/tmp/repo"}"#;
        let partial = r#"{"type":"message","message":{"role":"assistant"}}"#;
        write_jsonl(&path, &format!("{session}\n{partial}"));

        let mut st = SessionFileState::default();
        let (records, offset) = read_jsonl_from_offset(&path, 0, &mut st, count_assistant_messages);
        assert_eq!(records.len(), 0);
        assert!(offset > 0, "session line should be consumed");
        assert!(offset < fs::metadata(&path).unwrap().len());

        // Complete the line and poll again from the saved offset.
        let mut f = fs::OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(b"\n").unwrap();

        let (records, _) = read_jsonl_from_offset(&path, offset, &mut st, count_assistant_messages);
        assert_eq!(records.len(), 1);
        assert_eq!(crate::state::resolve(records[0].session), "repo s1");
    }

    #[test]
    fn offset_beyond_file_len_restarts_from_start() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.jsonl");
        write_jsonl(
            &path,
            r#"{"type":"session","id":"s1","cwd":"/tmp/repo"}
{"type":"message","message":{"role":"assistant"}}
"#,
        );

        let mut st = SessionFileState::default();
        let (records, _) = read_jsonl_from_offset(&path, 9999, &mut st, count_assistant_messages);
        assert_eq!(records.len(), 1);
        assert_eq!(st.sid, "s1");
        assert_eq!(st.dir, "repo");
    }
}
