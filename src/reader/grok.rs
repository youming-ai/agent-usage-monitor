use crate::reader::pricing;
use crate::reader::{UsageSource, basename, find_recursive, session_label};
use crate::state::{Platform, UsageRecord};
use chrono::{TimeZone, Utc};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Tracks prompt-level context token growth while scanning `updates.jsonl`.
#[derive(Clone, Default)]
struct PromptTracker {
    baseline_tokens: u64,
    current_prompt_id: Option<String>,
    current_prompt_max: u64,
    finalized_prompts: HashSet<String>,
}

impl PromptTracker {
    fn observe_line(
        &mut self,
        prompt_id: &str,
        total_tokens: u64,
        timestamp: i64,
        meta: &SessionMeta,
    ) -> Option<UsageRecord> {
        if self.current_prompt_id.as_deref() != Some(prompt_id) {
            let record = self
                .current_prompt_id
                .take()
                .and_then(|prev_id| self.finalize_prompt(&prev_id, timestamp, meta));
            self.current_prompt_id = Some(prompt_id.to_string());
            self.current_prompt_max = total_tokens;
            record
        } else {
            self.current_prompt_max = self.current_prompt_max.max(total_tokens);
            None
        }
    }

    fn finalize_prompt(
        &mut self,
        prompt_id: &str,
        timestamp: i64,
        meta: &SessionMeta,
    ) -> Option<UsageRecord> {
        if prompt_id.is_empty() || self.finalized_prompts.contains(prompt_id) {
            return None;
        }
        let delta = self.current_prompt_max.saturating_sub(self.baseline_tokens);
        if delta == 0 {
            self.finalized_prompts.insert(prompt_id.to_string());
            return None;
        }
        self.baseline_tokens = self.current_prompt_max;
        self.finalized_prompts.insert(prompt_id.to_string());

        let ts = Utc.timestamp_opt(timestamp, 0).single()?;
        let cost = pricing::calculate_cost(&meta.model, delta, 0, 0, 0);

        Some(UsageRecord {
            timestamp: ts,
            platform: Platform::Grok,
            model: crate::state::intern(&meta.model),
            session: crate::state::intern(&meta.session_label()),
            // Grok exposes cumulative context size per inference step, not an
            // input/output split — store the step delta as input tokens.
            input_tokens: delta,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
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
}

#[derive(Clone)]
struct SessionMeta {
    session_id: String,
    cwd: String,
    model: String,
}

impl SessionMeta {
    fn session_label(&self) -> String {
        session_label(&basename(&self.cwd), &self.session_id)
    }
}

#[derive(Default)]
struct FileState {
    tracker: PromptTracker,
}

pub struct GrokReader {
    sessions_dir: PathBuf,
    file_positions: HashMap<PathBuf, u64>,
    file_state: HashMap<PathBuf, FileState>,
    session_meta: HashMap<PathBuf, SessionMeta>,
}

impl GrokReader {
    pub fn new(grok_dir: PathBuf) -> Self {
        Self {
            sessions_dir: grok_dir.join("sessions"),
            file_positions: HashMap::new(),
            file_state: HashMap::new(),
            session_meta: HashMap::new(),
        }
    }

    #[allow(dead_code)]
    pub fn default_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".grok")
    }

    fn find_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        if self.sessions_dir.exists() {
            find_recursive(&self.sessions_dir, &mut files, &|p| {
                p.file_name().is_some_and(|n| n == "updates.jsonl")
            });
        }
        files
    }

    fn session_meta_for(&mut self, updates_path: &Path) -> SessionMeta {
        if let Some(meta) = self.session_meta.get(updates_path) {
            return meta.clone();
        }
        let meta = read_summary_meta(updates_path);
        self.session_meta
            .insert(updates_path.to_path_buf(), meta.clone());
        meta
    }

    fn scan_files(&mut self, from_start: bool) -> Vec<UsageRecord> {
        let files = self.find_files();
        let current_files: HashSet<PathBuf> = files.iter().cloned().collect();
        self.file_positions
            .retain(|path, _| current_files.contains(path));
        self.file_state
            .retain(|path, _| current_files.contains(path));
        self.session_meta
            .retain(|path, _| current_files.contains(path));

        let mut records = Vec::new();
        for file in files {
            let offset = if from_start {
                0
            } else {
                self.file_positions.get(&file).copied().unwrap_or(0)
            };
            let meta = self.session_meta_for(&file);
            let st = self.file_state.entry(file.clone()).or_default();
            let (entries, bytes_read) =
                read_updates_from_offset(&file, offset, &meta, &mut st.tracker);
            self.file_positions.insert(file, bytes_read);
            records.extend(entries);
        }
        records.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        records
    }
}

impl UsageSource for GrokReader {
    fn platform(&self) -> Platform {
        Platform::Grok
    }

    fn scan_all(&mut self) -> Vec<UsageRecord> {
        self.file_positions.clear();
        self.file_state.clear();
        self.session_meta.clear();
        self.scan_files(true)
    }

    fn poll_delta(&mut self) -> Vec<UsageRecord> {
        self.scan_files(false)
    }
    fn get_watch_directories(&self) -> Vec<std::path::PathBuf> {
        vec![self.sessions_dir.clone()]
    }
}

fn read_summary_meta(updates_path: &Path) -> SessionMeta {
    let session_dir = updates_path.parent().unwrap_or_else(|| Path::new("."));
    let session_id = session_dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    let summary_path = session_dir.join("summary.json");
    let (cwd, model) = if let Ok(content) = std::fs::read_to_string(&summary_path) {
        if let Ok(v) = serde_json::from_str::<Value>(&content) {
            let cwd = v
                .get("info")
                .and_then(|i| i.get("cwd"))
                .and_then(|c| c.as_str())
                .unwrap_or("")
                .to_string();
            let model = v
                .get("current_model_id")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown")
                .to_string();
            (cwd, model)
        } else {
            (String::new(), "unknown".to_string())
        }
    } else {
        (String::new(), "unknown".to_string())
    };

    SessionMeta {
        session_id,
        cwd,
        model,
    }
}

fn read_updates_from_offset(
    path: &Path,
    skip_bytes: u64,
    meta: &SessionMeta,
    tracker: &mut PromptTracker,
) -> (Vec<UsageRecord>, u64) {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return (Vec::new(), skip_bytes),
    };

    let file_len = file.metadata().map(|m| m.len()).unwrap_or(0);
    if skip_bytes > file_len {
        return read_updates_from_offset(path, 0, meta, tracker);
    }

    let mut reader = BufReader::new(file);
    if skip_bytes > 0 && reader.seek(SeekFrom::Start(skip_bytes)).is_err() {
        return read_updates_from_offset(path, 0, meta, tracker);
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

        if let Some(rec) = parse_updates_line(line.trim_end_matches(['\r', '\n']), meta, tracker) {
            records.push(rec);
        }
        offset += bytes;
    }

    (records, offset)
}

fn parse_updates_line(
    line: &str,
    meta: &SessionMeta,
    tracker: &mut PromptTracker,
) -> Option<UsageRecord> {
    let v: Value = serde_json::from_str(line).ok()?;
    let params = v.get("params")?;
    let prompt_meta = params.get("_meta")?;
    let prompt_id = prompt_meta.get("promptId")?.as_str()?;
    let total_tokens = prompt_meta.get("totalTokens")?.as_u64()?;

    let timestamp = v.get("timestamp").and_then(|t| t.as_i64()).or_else(|| {
        prompt_meta
            .get("agentTimestampMs")
            .and_then(|t| t.as_i64())
            .map(|ms| ms / 1000)
    })?;

    tracker.observe_line(prompt_id, total_tokens, timestamp, meta)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    fn write_session(
        root: &Path,
        cwd_segment: &str,
        session_id: &str,
        cwd: &str,
        model: &str,
        updates: &[&str],
    ) -> PathBuf {
        let dir = root.join("sessions").join(cwd_segment).join(session_id);
        fs::create_dir_all(&dir).unwrap();

        let summary = format!(
            r#"{{"info":{{"id":"{session_id}","cwd":"{cwd}"}},"current_model_id":"{model}"}}"#
        );
        fs::write(dir.join("summary.json"), summary).unwrap();

        let updates_path = dir.join("updates.jsonl");
        let mut content = updates.join("\n");
        if !content.is_empty() {
            content.push('\n');
        }
        fs::write(&updates_path, content).unwrap();
        updates_path
    }

    fn sample_updates() -> [&'static str; 4] {
        [
            r#"{"timestamp":1000,"method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"agent_thought_chunk"},"_meta":{"promptId":"prompt-a","totalTokens":1000,"agentTimestampMs":1000000}}}"#,
            r#"{"timestamp":1001,"method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"tool_call"},"_meta":{"promptId":"prompt-a","totalTokens":1500,"agentTimestampMs":1001000}}}"#,
            r#"{"timestamp":1002,"method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"agent_thought_chunk"},"_meta":{"promptId":"prompt-b","totalTokens":2200,"agentTimestampMs":1002000}}}"#,
            r#"{"timestamp":1003,"method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"tool_call"},"_meta":{"promptId":"prompt-b","totalTokens":2500,"agentTimestampMs":1003000}}}"#,
        ]
    }

    #[test]
    fn parses_prompt_context_deltas() {
        let dir = tempfile::tempdir().unwrap();
        write_session(
            dir.path(),
            "project",
            "019ea524-5702-7421-a300-ead404f0ee6f",
            "/Users/me/project",
            "grok-build",
            &sample_updates(),
        );

        let mut reader = GrokReader::new(dir.path().to_path_buf());
        let records = reader.scan_all();
        assert_eq!(records.len(), 1, "only the first completed prompt emits");
        assert_eq!(records[0].input_tokens, 1500);
        assert_eq!(crate::state::resolve(records[0].model), "grok-build");
        assert_eq!(crate::state::resolve(records[0].session), "project 019ea524");
        assert_eq!(records[0].platform, Platform::Grok);
    }

    #[test]
    fn poll_delta_emits_when_prompt_changes() {
        let dir = tempfile::tempdir().unwrap();
        let updates_path = write_session(
            dir.path(),
            "project",
            "019ea524-5702-7421-a300-ead404f0ee6f",
            "/Users/me/project",
            "grok-composer-2.5-fast",
            &[
                r#"{"timestamp":1000,"method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"agent_thought_chunk"},"_meta":{"promptId":"prompt-a","totalTokens":1000,"agentTimestampMs":1000000}}}"#,
            ],
        );

        let mut reader = GrokReader::new(dir.path().to_path_buf());
        assert!(reader.scan_all().is_empty(), "single in-progress prompt");

        let mut f = fs::OpenOptions::new()
            .append(true)
            .open(&updates_path)
            .unwrap();
        writeln!(
            f,
            r#"{{"timestamp":1001,"method":"session/update","params":{{"sessionId":"s1","update":{{"sessionUpdate":"tool_call"}},"_meta":{{"promptId":"prompt-b","totalTokens":1800,"agentTimestampMs":1001000}}}}}}"#
        )
        .unwrap();

        let delta = reader.poll_delta();
        assert_eq!(delta.len(), 1);
        assert_eq!(delta[0].input_tokens, 1000);
        assert_eq!(crate::state::resolve(delta[0].model), "grok-composer-2.5-fast");
    }

    #[test]
    fn poll_delta_returns_nothing_when_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        write_session(
            dir.path(),
            "project",
            "019ea524-5702-7421-a300-ead404f0ee6f",
            "/Users/me/project",
            "grok-build",
            &sample_updates(),
        );

        let mut reader = GrokReader::new(dir.path().to_path_buf());
        assert_eq!(reader.scan_all().len(), 1);
        assert!(reader.poll_delta().is_empty());
    }

    #[test]
    fn missing_directory_yields_empty() {
        let mut reader = GrokReader::new(PathBuf::from("/nonexistent/grok"));
        assert!(reader.scan_all().is_empty());
        assert!(reader.poll_delta().is_empty());
    }
}
