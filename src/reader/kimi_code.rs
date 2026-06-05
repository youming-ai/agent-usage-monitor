use crate::reader::{basename, find_recursive, session_label, UsageSource};
use crate::reader::pricing::calculate_cost;
use crate::state::{Platform, UsageRecord};
use chrono::{TimeZone, Utc};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Reads per-turn usage from Kimi Code's local JSONL `wire.jsonl` files.
/// Each session lives under `~/.kimi-code/sessions/wd_{dir}/session_{id}/`
/// and may contain multiple agents (main + subagents), each with its own
/// `agents/{name}/wire.jsonl`.  Usage records are JSONL lines with
/// `type == "usage.record"`.
///
/// Byte-offset tracking per file enables incremental polling so only new
/// lines are read on each `poll_delta()` call.
pub struct KimiCodeReader {
    /// Root data directory, typically `~/.kimi-code`.
    base_dir: PathBuf,
    /// Byte offset already consumed per wire.jsonl path.
    file_positions: HashMap<PathBuf, u64>,
    /// Session metadata: sessionId → workDir, loaded from session_index.jsonl.
    session_meta: HashMap<String, String>,
}

impl KimiCodeReader {
    pub fn new(base_dir: PathBuf) -> Self {
        let session_meta = load_session_index(&base_dir);
        Self {
            base_dir,
            file_positions: HashMap::new(),
            session_meta,
        }
    }

    #[cfg(test)]
    fn new_with_meta(base_dir: PathBuf, meta: HashMap<String, String>) -> Self {
        Self {
            base_dir,
            file_positions: HashMap::new(),
            session_meta: meta,
        }
    }

    /// Discover all `wire.jsonl` files under the sessions directory.
    fn find_wire_files(&self) -> Vec<PathBuf> {
        let sessions_dir = self.base_dir.join("sessions");
        let mut files = Vec::new();
        find_recursive(&sessions_dir, &mut files, &|p| {
            p.file_name().is_some_and(|n| n == "wire.jsonl")
        });
        files
    }

    /// Read new lines from a single file starting at the tracked byte offset,
    /// returning parsed records and advancing the offset.
    fn read_file_delta(&mut self, path: &Path) -> Vec<UsageRecord> {
        let offset = self.file_positions.get(path).copied().unwrap_or(0);
        let file = match fs::File::open(path) {
            Ok(f) => f,
            Err(_) => return Vec::new(),
        };

        // Seek to the last known position.
        let mut reader = std::io::BufReader::new(file);
        if offset > 0 {
            if reader.seek(SeekFrom::Start(offset)).is_err() {
                return Vec::new();
            }
        }

        // Derive session info from the file path:
        //   .../sessions/wd_{dir}/session_{id}/agents/{agent}/wire.jsonl
        let (session_id, session) = extract_session_info(path, &self.session_meta);

        let mut records = Vec::new();
        let mut line = String::new();
        let mut new_offset = offset;
        loop {
            line.clear();
            let bytes = match reader.read_line(&mut line) {
                Ok(0) => break, // EOF
                Ok(n) => n as u64,
                Err(_) => break,
            };
            if let Some(rec) = parse_usage_record(&line, &session_id, &session) {
                records.push(rec);
            }
            new_offset += bytes;
        }

        self.file_positions.insert(path.to_path_buf(), new_offset);
        records
    }

    /// Full scan: read every file from the beginning.
    fn do_scan_all(&mut self) -> Vec<UsageRecord> {
        // Reload session metadata in case new sessions appeared.
        self.session_meta = load_session_index(&self.base_dir);
        // Clear positions so we re-read everything.
        self.file_positions.clear();
        let files = self.find_wire_files();
        let mut all = Vec::new();
        for f in &files {
            all.extend(self.read_file_delta(f));
        }
        all
    }

    /// Incremental poll: only read new bytes since last poll.
    fn do_poll_delta(&mut self) -> Vec<UsageRecord> {
        // Reload session metadata for new sessions.
        self.session_meta = load_session_index(&self.base_dir);
        let files = self.find_wire_files();
        // Clean up stale entries for deleted files.
        self.file_positions
            .retain(|p, _| p.exists());

        let mut all = Vec::new();
        for f in &files {
            all.extend(self.read_file_delta(&f));
        }
        all
    }
}

impl UsageSource for KimiCodeReader {
    fn platform(&self) -> Platform {
        Platform::KimiCode
    }

    fn scan_all(&mut self) -> Vec<UsageRecord> {
        self.do_scan_all()
    }

    fn poll_delta(&mut self) -> Vec<UsageRecord> {
        self.do_poll_delta()
    }
}

/// Load `session_index.jsonl` into a sessionId → workDir map.
fn load_session_index(base_dir: &Path) -> HashMap<String, String> {
    let index_path = base_dir.join("session_index.jsonl");
    let content = match fs::read_to_string(&index_path) {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };
    let mut map = HashMap::new();
    for line in content.lines() {
        if let Ok(v) = serde_json::from_str::<Value>(line)
            && let (Some(sid), Some(wd)) = (
                v.get("sessionId").and_then(|x| x.as_str()),
                v.get("workDir").and_then(|x| x.as_str()),
            )
        {
            map.insert(sid.to_string(), wd.to_string());
        }
    }
    map
}

/// Extract the session ID and a human-readable session label from a wire.jsonl
/// path like `.../sessions/wd_{dir}/session_{uuid}/agents/{agent}/wire.jsonl`.
fn extract_session_info(
    path: &Path,
    session_meta: &HashMap<String, String>,
) -> (String, String) {
    // Walk up from wire.jsonl to find the session_{uuid} directory.
    let mut session_id = String::new();
    for ancestor in path.ancestors() {
        if let Some(name) = ancestor.file_name().and_then(|n| n.to_str())
            && let Some(id) = name.strip_prefix("session_")
        {
            session_id = id.to_string();
            break;
        }
    }

    // Look up workDir from session_index.jsonl.
    let work_dir = session_meta
        .get(&format!("session_{session_id}"))
        .map(|s| s.as_str())
        .unwrap_or("");
    let label = session_label(&basename(work_dir), &session_id);

    (session_id, label)
}

/// Parse one JSONL line into a `UsageRecord` if it's a `usage.record` with
/// token data. Returns `None` for non-usage lines or zero-token rows.
fn parse_usage_record(
    line: &str,
    session_id: &str,
    session: &str,
) -> Option<UsageRecord> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let v: Value = serde_json::from_str(line).ok()?;

    // Only process usage.record events.
    if v.get("type")?.as_str()? != "usage.record" {
        return None;
    }

    let usage = v.get("usage")?;
    let u64_at = |obj: &Value, key: &str| obj.get(key).and_then(|x| x.as_u64()).unwrap_or(0);

    let input = u64_at(usage, "inputOther");
    let output = u64_at(usage, "output");
    let cache_read = u64_at(usage, "inputCacheRead");
    let cache_creation = u64_at(usage, "inputCacheCreation");

    // Skip zero-token rows.
    if input == 0 && output == 0 && cache_read == 0 && cache_creation == 0 {
        return None;
    }

    let model = v
        .get("model")
        .and_then(|x| x.as_str())
        .unwrap_or("unknown")
        .to_string();
    let time_ms = v.get("time")?.as_i64()?;
    let timestamp = Utc.timestamp_millis_opt(time_ms).single()?;
    let cost = calculate_cost(&model, input, output, cache_read, cache_creation);

    Some(UsageRecord {
        timestamp,
        platform: Platform::KimiCode,
        model,
        session: if session.is_empty() {
            format!("unknown {}", &session_id[..8.min(session_id.len())])
        } else {
            session.to_string()
        },
        input_tokens: input,
        output_tokens: output,
        cache_read_tokens: cache_read,
        cache_creation_tokens: cache_creation,
        cost_usd: cost,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        // Create session_index.jsonl
        fs::write(
            dir.path().join("session_index.jsonl"),
            r#"{"sessionId":"session_abc12345-0000","sessionDir":"/tmp/s","workDir":"/Users/me/myproject"}"#,
        )
        .unwrap();
        dir
    }

    fn wire_path(dir: &Path, session: &str, agent: &str) -> PathBuf {
        let p = dir
            .join("sessions")
            .join("wd_test")
            .join(format!("session_{session}"))
            .join("agents")
            .join(agent);
        fs::create_dir_all(&p).unwrap();
        p.join("wire.jsonl")
    }

    fn write_lines(path: &Path, lines: &[&str]) {
        let content: String = lines.iter().map(|l| format!("{l}\n")).collect();
        fs::write(path, content).unwrap();
    }

    #[test]
    fn parses_usage_records_and_skips_non_usage() {
        let dir = setup_dir();
        let wire = wire_path(dir.path(), "abc12345-0000", "main");
        write_lines(
            &wire,
            &[
                r#"{"type":"metadata","protocol_version":"1.0"}"#,
                r#"{"type":"usage.record","model":"xiaomi-token-plan-cn/mimo-v2.5-pro","usage":{"inputOther":100,"output":40,"inputCacheRead":10,"inputCacheCreation":0},"usageScope":"turn","time":1780625874436}"#,
                r#"{"type":"turn.prompt","input":[{"type":"text","text":"hello"}]}"#,
                r#"{"type":"usage.record","model":"xiaomi-token-plan-cn/mimo-v2.5-pro","usage":{"inputOther":200,"output":80,"inputCacheRead":0,"inputCacheCreation":5},"usageScope":"turn","time":1780625875000}"#,
            ],
        );
        let mut meta = HashMap::new();
        meta.insert(
            "session_abc12345-0000".to_string(),
            "/Users/me/myproject".to_string(),
        );
        let mut reader = KimiCodeReader::new_with_meta(dir.path().to_path_buf(), meta);

        let records = reader.scan_all();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].model, "xiaomi-token-plan-cn/mimo-v2.5-pro");
        assert_eq!(records[0].input_tokens, 100);
        assert_eq!(records[0].output_tokens, 40);
        assert_eq!(records[0].cache_read_tokens, 10);
        assert_eq!(records[0].cache_creation_tokens, 0);
        assert_eq!(records[0].platform, Platform::KimiCode);
        assert!(records[0].session.starts_with("myproject"));
        assert_eq!(records[1].input_tokens, 200);
        assert_eq!(records[1].cache_creation_tokens, 5);
    }

    #[test]
    fn poll_delta_returns_only_new_records() {
        let dir = setup_dir();
        let wire = wire_path(dir.path(), "abc12345-0000", "main");
        write_lines(
            &wire,
            &[r#"{"type":"usage.record","model":"mimo-v2-pro","usage":{"inputOther":100,"output":40,"inputCacheRead":0,"inputCacheCreation":0},"usageScope":"turn","time":1780625874436}"#],
        );
        let mut meta = HashMap::new();
        meta.insert(
            "session_abc12345-0000".to_string(),
            "/Users/me/myproject".to_string(),
        );
        let mut reader = KimiCodeReader::new_with_meta(dir.path().to_path_buf(), meta);

        assert_eq!(reader.scan_all().len(), 1);
        assert_eq!(reader.poll_delta().len(), 0);

        // Append a new line.
        let mut f = fs::OpenOptions::new().append(true).open(&wire).unwrap();
        use std::io::Write;
        writeln!(f, r#"{{"type":"usage.record","model":"mimo-v2-pro","usage":{{"inputOther":200,"output":80,"inputCacheRead":0,"inputCacheCreation":0}},"usageScope":"turn","time":1780625875000}}"#).unwrap();

        let delta = reader.poll_delta();
        assert_eq!(delta.len(), 1);
        assert_eq!(delta[0].input_tokens, 200);
    }

    #[test]
    fn reads_multiple_agents_per_session() {
        let dir = setup_dir();
        let main_wire = wire_path(dir.path(), "abc12345-0000", "main");
        let sub_wire = wire_path(dir.path(), "abc12345-0000", "agent-0");
        write_lines(
            &main_wire,
            &[r#"{"type":"usage.record","model":"mimo-v2-pro","usage":{"inputOther":100,"output":40,"inputCacheRead":0,"inputCacheCreation":0},"usageScope":"turn","time":1780625874436}"#],
        );
        write_lines(
            &sub_wire,
            &[r#"{"type":"usage.record","model":"mimo-v2-pro","usage":{"inputOther":50,"output":20,"inputCacheRead":0,"inputCacheCreation":0},"usageScope":"turn","time":1780625875000}"#],
        );
        let mut meta = HashMap::new();
        meta.insert(
            "session_abc12345-0000".to_string(),
            "/Users/me/myproject".to_string(),
        );
        let mut reader = KimiCodeReader::new_with_meta(dir.path().to_path_buf(), meta);

        let records = reader.scan_all();
        assert_eq!(records.len(), 2);
    }

    #[test]
    fn missing_directory_yields_empty() {
        let mut reader =
            KimiCodeReader::new(PathBuf::from("/nonexistent/kimi-code"));
        assert!(reader.scan_all().is_empty());
        assert!(reader.poll_delta().is_empty());
    }

    #[test]
    fn zero_token_rows_are_skipped() {
        let dir = setup_dir();
        let wire = wire_path(dir.path(), "abc12345-0000", "main");
        write_lines(
            &wire,
            &[r#"{"type":"usage.record","model":"mimo-v2-pro","usage":{"inputOther":0,"output":0,"inputCacheRead":0,"inputCacheCreation":0},"usageScope":"turn","time":1780625874436}"#],
        );
        let mut reader = KimiCodeReader::new(dir.path().to_path_buf());
        assert!(reader.scan_all().is_empty());
    }

    #[test]
    fn session_label_falls_back_to_unknown() {
        let dir = setup_dir();
        // No matching session in session_index.jsonl for this ID.
        let wire = wire_path(dir.path(), "zzz99999-9999", "main");
        write_lines(
            &wire,
            &[r#"{"type":"usage.record","model":"mimo-v2-pro","usage":{"inputOther":100,"output":40,"inputCacheRead":0,"inputCacheCreation":0},"usageScope":"turn","time":1780625874436}"#],
        );
        let mut reader = KimiCodeReader::new(dir.path().to_path_buf());
        let records = reader.scan_all();
        assert_eq!(records.len(), 1);
        assert!(records[0].session.starts_with("unknown"));
    }
}
