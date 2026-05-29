use crate::state::{Platform, UsageRecord};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub struct ClaudeReader {
    data_dir: PathBuf,
    file_positions: HashMap<PathBuf, u64>,
}

impl ClaudeReader {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            file_positions: HashMap::new(),
        }
    }

    #[allow(dead_code)]
    pub fn default_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".claude/projects")
    }

    fn find_jsonl_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        if self.data_dir.exists() {
            find_jsonl_recursive(&self.data_dir, &mut files);
        }
        files
    }

    pub fn scan_all(&mut self) -> Vec<UsageRecord> {
        let files = self.find_jsonl_files();
        let mut records = Vec::new();
        for file in files {
            let entries = self.read_file_from(&file, 0);
            self.file_positions.insert(file, entries.len() as u64);
            records.extend(entries);
        }
        records.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        records
    }

    pub fn poll_delta(&mut self) -> Vec<UsageRecord> {
        let files = self.find_jsonl_files();
        let mut new_records = Vec::new();
        for file in files {
            let offset = self.file_positions.get(&file).copied().unwrap_or(0);
            let entries = self.read_file_from(&file, offset);
            if !entries.is_empty() {
                *self.file_positions.entry(file).or_insert(0) += entries.len() as u64;
                new_records.extend(entries);
            }
        }
        new_records.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        new_records
    }

    fn read_file_from(&self, path: &Path, skip_lines: u64) -> Vec<UsageRecord> {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let project = extract_project_name(path, &self.data_dir);

        content
            .lines()
            .skip(skip_lines as usize)
            .filter_map(|line| parse_claude_line(line, &project))
            .collect()
    }
}

fn parse_claude_line(line: &str, project: &str) -> Option<UsageRecord> {
    let v: Value = serde_json::from_str(line).ok()?;

    if v.get("type")?.as_str()? != "assistant" {
        return None;
    }

    let message = v.get("message")?;
    let usage = message.get("usage")?;

    let input_tokens = usage.get("input_tokens")?.as_u64()?;
    let output_tokens = usage.get("output_tokens")?.as_u64().unwrap_or(0);
    let cache_read = usage
        .get("cache_read_input_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let cache_creation = usage
        .get("cache_creation_input_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    if input_tokens == 0 && output_tokens == 0 {
        return None;
    }

    let model = message
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let timestamp_str = v.get("timestamp")?.as_str()?;
    let timestamp: DateTime<Utc> = timestamp_str.parse().ok()?;

    let request_id = v
        .get("requestId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let message_id = message
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let service_tier = usage
        .get("service_tier")
        .and_then(|v| v.as_str())
        .unwrap_or("standard")
        .to_string();

    // Cost: read from JSONL only (comes from Anthropic API response)
    let cost_usd = v
        .get("cost_usd")
        .and_then(|v| v.as_f64())
        .or_else(|| v.get("cost").and_then(|v| v.as_f64()))
        .unwrap_or(0.0);

    Some(UsageRecord {
        timestamp,
        platform: Platform::ClaudeCode,
        model,
        project: project.to_string(),
        input_tokens,
        output_tokens,
        cache_read_tokens: cache_read,
        cache_creation_tokens: cache_creation,
        cost_usd,
        service_tier,
        message_id,
        request_id,
    })
}

fn extract_project_name(path: &Path, data_dir: &Path) -> String {
    path.parent()
        .and_then(|p| p.strip_prefix(data_dir).ok())
        .and_then(|p| p.to_str())
        .map(|s| {
            s.trim_start_matches('-')
                .split('-')
                .last()
                .unwrap_or(s)
                .to_string()
        })
        .unwrap_or_else(|| "unknown".to_string())
}

fn find_jsonl_recursive(dir: &Path, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                find_jsonl_recursive(&path, files);
            } else if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                files.push(path);
            }
        }
    }
}
