use crate::state::{Platform, UsageRecord};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use super::UsageSource;
use super::find_recursive;
use super::is_under_dir_named;
use super::session_jsonl::{SessionFileState, read_jsonl_from_offset};

pub struct OpenClawReader {
    data_dir: PathBuf,
    file_positions: HashMap<PathBuf, u64>,
    file_state: HashMap<PathBuf, SessionFileState>,
}

impl OpenClawReader {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            file_positions: HashMap::new(),
            file_state: HashMap::new(),
        }
    }

    #[allow(dead_code)]
    pub fn default_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".openclaw/agents")
    }

    fn find_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        if self.data_dir.exists() {
            find_recursive(&self.data_dir, &mut files, &|p| {
                p.extension().map(|e| e == "jsonl").unwrap_or(false)
                    && is_under_dir_named(p, "sessions")
            });
        }
        files
    }

    fn scan_files(&mut self, from_start: bool) -> Vec<UsageRecord> {
        let files = self.find_files();
        let current_files: HashSet<PathBuf> = files.iter().cloned().collect();
        self.file_positions
            .retain(|path, _| current_files.contains(path));
        self.file_state
            .retain(|path, _| current_files.contains(path));

        let mut records = Vec::new();
        for file in files {
            let offset = if from_start {
                0
            } else {
                self.file_positions.get(&file).copied().unwrap_or(0)
            };
            let mut st = self
                .file_state
                .get(&file)
                .cloned()
                .unwrap_or_else(|| SessionFileState::with_dir("unknown", ""));
            let (entries, bytes_read) =
                read_jsonl_from_offset(&file, offset, &mut st, parse_openclaw_line);
            self.file_positions.insert(file.clone(), bytes_read);
            self.file_state.insert(file, st);
            records.extend(entries);
        }
        records.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        records
    }
}

impl UsageSource for OpenClawReader {
    fn platform(&self) -> Platform {
        Platform::OpenClaw
    }

    fn scan_all(&mut self) -> Vec<UsageRecord> {
        self.file_positions.clear();
        self.file_state.clear();
        self.scan_files(true)
    }

    fn poll_delta(&mut self) -> Vec<UsageRecord> {
        self.scan_files(false)
    }
}

fn parse_openclaw_line(line: &str, st: &SessionFileState) -> Option<UsageRecord> {
    let v: Value = serde_json::from_str(line).ok()?;

    if v.get("type")?.as_str()? != "message" {
        return None;
    }

    let message = v.get("message")?;
    if message.get("role")?.as_str()? != "assistant" {
        return None;
    }

    let usage = message.get("usage")?;

    let input = usage.get("input")?.as_u64()?;
    let output = usage.get("output")?.as_u64().unwrap_or(0);
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

    let dir = v
        .get("cwd")
        .and_then(|c| c.as_str())
        .map(crate::reader::basename)
        .unwrap_or_else(|| st.dir.clone());
    let session_id = v
        .get("sessionId")
        .and_then(|s| s.as_str())
        .unwrap_or(st.sid.as_str());
    let session = crate::reader::session_label(&dir, session_id);

    let cost_usd = usage
        .get("cost")
        .and_then(|c| c.get("total"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    Some(UsageRecord {
        timestamp,
        platform: Platform::OpenClaw,
        model,
        session,
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

    fn sample_jsonl() -> String {
        [
            r#"{"type":"session","id":"abc123","timestamp":"2026-06-05T10:00:00Z","cwd":"/Users/me/project"}"#,
            r#"{"type":"message","id":"msg1","timestamp":"2026-06-05T10:00:01Z","message":{"role":"assistant","model":"claude-opus-4","usage":{"input":100,"output":50,"cacheRead":10,"cacheWrite":5},"cost":{"total":0.02}}}"#,
            "",
        ]
        .join("\n")
    }

    fn write_file(dir: &std::path::Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path
    }

    #[test]
    fn parses_assistant_messages() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = dir.path().join("agent").join("sessions");
        fs::create_dir_all(&sessions).unwrap();
        write_file(&sessions, "session.jsonl", &sample_jsonl());

        let mut reader = OpenClawReader::new(dir.path().to_path_buf());
        let records = reader.scan_all();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].model, "claude-opus-4");
        assert_eq!(records[0].input_tokens, 100);
        assert_eq!(records[0].output_tokens, 50);
        assert_eq!(records[0].cache_read_tokens, 10);
        assert_eq!(records[0].cache_creation_tokens, 5);
        assert_eq!(records[0].platform, Platform::OpenClaw);
        assert_eq!(records[0].session, "project abc123");
    }

    #[test]
    fn ignores_jsonl_outside_sessions_dir() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "config.jsonl", &sample_jsonl());

        let mut reader = OpenClawReader::new(dir.path().to_path_buf());
        assert!(reader.scan_all().is_empty());
    }

    #[test]
    fn poll_delta_returns_nothing_when_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = dir.path().join("sessions");
        fs::create_dir_all(&sessions).unwrap();
        write_file(&sessions, "session.jsonl", &sample_jsonl());

        let mut reader = OpenClawReader::new(dir.path().to_path_buf());
        assert_eq!(reader.scan_all().len(), 1);
        assert_eq!(reader.poll_delta().len(), 0);
    }

    #[test]
    fn missing_directory_yields_empty() {
        let mut reader = OpenClawReader::new(PathBuf::from("/nonexistent/openclaw"));
        assert!(reader.scan_all().is_empty());
        assert!(reader.poll_delta().is_empty());
    }
}
