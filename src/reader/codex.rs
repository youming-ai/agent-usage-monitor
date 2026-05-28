use crate::reader::pricing;
use crate::state::{Platform, UsageRecord};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub struct CodexReader {
    sessions_dir: PathBuf,
    file_positions: HashMap<PathBuf, u64>,
    current_model: String,
}

impl CodexReader {
    pub fn new(codex_dir: PathBuf) -> Self {
        let sessions_dir = codex_dir.join("sessions");
        Self {
            sessions_dir,
            file_positions: HashMap::new(),
            current_model: "unknown".to_string(),
        }
    }

    pub fn default_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".codex")
    }

    fn find_rollout_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        if self.sessions_dir.exists() {
            find_rollout_recursive(&self.sessions_dir, &mut files);
        }
        files
    }

    pub fn scan_all(&mut self) -> Vec<UsageRecord> {
        let files = self.find_rollout_files();
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
        let files = self.find_rollout_files();
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

    fn read_file_from(&mut self, path: &Path, skip_lines: u64) -> Vec<UsageRecord> {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let project = extract_codex_project(path);

        content
            .lines()
            .skip(skip_lines as usize)
            .filter_map(|line| self.parse_codex_line(line, &project))
            .collect()
    }

    fn parse_codex_line(&mut self, line: &str, project: &str) -> Option<UsageRecord> {
        let v: Value = serde_json::from_str(line).ok()?;
        let event_type = v.get("type")?.as_str()?;

        // turn_context is a top-level event (not wrapped in event_msg)
        if event_type == "turn_context" {
            if let Some(model) = v.get("payload").and_then(|p| p.get("model")).and_then(|m| m.as_str()) {
                self.current_model = model.to_string();
            }
            return None;
        }

        if event_type != "event_msg" {
            return None;
        }

        let payload = v.get("payload")?;
        let payload_type = payload.get("type")?.as_str()?;

        // Also check task_started for model (fallback)
        if payload_type == "task_started" {
            if let Some(model) = payload
                .get("collaboration_mode")
                .and_then(|c| c.get("settings"))
                .and_then(|s| s.get("model"))
                .and_then(|m| m.as_str())
            {
                self.current_model = model.to_string();
            }
            return None;
        }

        if payload_type != "token_count" {
            return None;
        }

        let info = payload.get("info")?;
        if info.is_null() {
            return None;
        }

        let total = info.get("total_token_usage")?;
        let input_tokens = total.get("input_tokens")?.as_u64()?;
        let output_tokens = total.get("output_tokens")?.as_u64().unwrap_or(0);
        let cached = total
            .get("cached_input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let last = info.get("last_token_usage");
        let delta_input = last
            .and_then(|l| l.get("input_tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or(input_tokens);
        let delta_output = last
            .and_then(|l| l.get("output_tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or(output_tokens);
        let delta_cached = last
            .and_then(|l| l.get("cached_input_tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or(cached);

        if delta_input == 0 && delta_output == 0 {
            return None;
        }

        let timestamp_str = v.get("timestamp")?.as_str()?;
        let timestamp: DateTime<Utc> = timestamp_str.parse().ok()?;

        let cost_usd =
            pricing::calculate_cost(&self.current_model, delta_input, delta_output, delta_cached, 0);

        Some(UsageRecord {
            timestamp,
            platform: Platform::Codex,
            model: self.current_model.clone(),
            project: project.to_string(),
            input_tokens: delta_input,
            output_tokens: delta_output,
            cache_read_tokens: delta_cached,
            cache_creation_tokens: 0,
            cost_usd,
            service_tier: String::new(),
            message_id: String::new(),
            request_id: String::new(),
        })
    }
}

fn extract_codex_project(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.split('-').last().unwrap_or(s).to_string())
        .unwrap_or_else(|| "codex".to_string())
}

fn find_rollout_recursive(dir: &Path, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                find_rollout_recursive(&path, files);
            } else if path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("rollout-") && n.ends_with(".jsonl"))
                .unwrap_or(false)
            {
                files.push(path);
            }
        }
    }
}
