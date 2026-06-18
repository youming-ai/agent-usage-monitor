use crate::state::{Platform, UsageRecord};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use super::UsageSource;
use super::find_recursive;
use super::is_uuid_jsonl;
use super::session_jsonl::{SessionFileState, read_jsonl_from_offset};

pub struct FactoryReader {
    data_dir: PathBuf,
    file_positions: HashMap<PathBuf, u64>,
    file_state: HashMap<PathBuf, SessionFileState>,
}

impl FactoryReader {
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
            .join(".factory/projects")
    }

    fn find_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        if self.data_dir.exists() {
            find_recursive(&self.data_dir, &mut files, &|p| {
                p.extension().map(|e| e == "jsonl").unwrap_or(false) && is_uuid_jsonl(p)
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
                read_jsonl_from_offset(&file, offset, &mut st, parse_factory_line);
            self.file_positions.insert(file.clone(), bytes_read);
            self.file_state.insert(file, st);
            records.extend(entries);
        }
        records.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        records
    }
}

impl UsageSource for FactoryReader {
    fn platform(&self) -> Platform {
        Platform::Factory
    }

    fn scan_all(&mut self) -> Vec<UsageRecord> {
        self.file_positions.clear();
        self.file_state.clear();
        self.scan_files(true)
    }

    fn poll_delta(&mut self) -> Vec<UsageRecord> {
        self.scan_files(false)
    }
    fn get_watch_directories(&self) -> Vec<std::path::PathBuf> {
        vec![self.data_dir.clone()]
    }
}

fn parse_factory_line(line: &str, st: &SessionFileState) -> Option<UsageRecord> {
    let v: Value = serde_json::from_str(line).ok()?;

    if v.get("type")?.as_str()? != "message" {
        return None;
    }

    let message = v.get("message")?;
    if message.get("role")?.as_str()? != "assistant" {
        return None;
    }

    let usage = message.get("usage")?;

    let input = usage.get("input_tokens")?.as_u64()?;
    let output = usage.get("output_tokens")?.as_u64().unwrap_or(0);
    let cache_read = usage
        .get("cache_read_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let cache_write = usage
        .get("cache_write_tokens")
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
        platform: Platform::Factory,
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
            r#"{"type":"message","id":"msg1","timestamp":"2026-06-05T10:00:01Z","message":{"role":"assistant","model":"claude-sonnet-4-5","usage":{"input_tokens":1200,"output_tokens":450,"cache_read_tokens":800,"cache_write_tokens":150},"cost":{"total":0.02}}}"#,
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
        write_file(
            dir.path(),
            "a3f2c1d8-10e5-4b2a-9c1d-ef0123456789.jsonl",
            &sample_jsonl(),
        );

        let mut reader = FactoryReader::new(dir.path().to_path_buf());
        let records = reader.scan_all();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].model, "claude-sonnet-4-5");
        assert_eq!(records[0].input_tokens, 1200);
        assert_eq!(records[0].output_tokens, 450);
        assert_eq!(records[0].cache_read_tokens, 800);
        assert_eq!(records[0].cache_creation_tokens, 150);
        assert_eq!(records[0].platform, Platform::Factory);
        assert_eq!(records[0].session, "project abc123");
    }

    #[test]
    fn ignores_non_uuid_jsonl_files() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "session.jsonl", &sample_jsonl());

        let mut reader = FactoryReader::new(dir.path().to_path_buf());
        assert!(reader.scan_all().is_empty());
    }

    #[test]
    fn poll_delta_returns_nothing_when_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        write_file(
            dir.path(),
            "a3f2c1d8-10e5-4b2a-9c1d-ef0123456789.jsonl",
            &sample_jsonl(),
        );

        let mut reader = FactoryReader::new(dir.path().to_path_buf());
        assert_eq!(reader.scan_all().len(), 1);
        assert_eq!(reader.poll_delta().len(), 0);
    }

    #[test]
    fn missing_directory_yields_empty() {
        let mut reader = FactoryReader::new(PathBuf::from("/nonexistent/factory"));
        assert!(reader.scan_all().is_empty());
        assert!(reader.poll_delta().is_empty());
    }
}
