use crate::state::UsageRecord;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Trait for reading JSONL files with incremental updates
pub trait JsonlReader {
    /// Get mutable reference to per-file byte offsets
    fn file_positions(&mut self) -> &mut HashMap<PathBuf, u64>;

    /// Find all relevant JSONL files
    fn find_files(&self) -> Vec<PathBuf>;

    /// Parse a single line into a UsageRecord
    fn parse_line(&self, line: &str) -> Option<UsageRecord>;

    /// Read records from a file starting from a byte offset. Only complete
    /// newline-terminated lines are consumed; an incomplete trailing line is
    /// left for the next poll.
    fn read_file_from(&self, path: &Path, skip_bytes: u64) -> (Vec<UsageRecord>, u64) {
        let file = match File::open(path) {
            Ok(f) => f,
            Err(_) => return (Vec::new(), skip_bytes),
        };

        let file_len = file.metadata().map(|m| m.len()).unwrap_or(0);
        if skip_bytes > file_len {
            return self.read_file_from(path, 0);
        }

        let mut reader = BufReader::new(file);
        if skip_bytes > 0 && reader.seek(SeekFrom::Start(skip_bytes)).is_err() {
            return self.read_file_from(path, 0);
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

            if let Some(rec) = self.parse_line(line.trim_end_matches(['\r', '\n'])) {
                records.push(rec);
            }
            offset += bytes;
        }

        (records, offset)
    }

    /// Scan all files and return all records
    fn scan_all(&mut self) -> Vec<UsageRecord> {
        let files = self.find_files();
        let mut records = Vec::new();
        for file in files {
            let (entries, bytes_read) = self.read_file_from(&file, 0);
            self.file_positions().insert(file, bytes_read);
            records.extend(entries);
        }
        records.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        records
    }

    /// Poll for new records since last check
    fn poll_delta(&mut self) -> Vec<UsageRecord> {
        let files = self.find_files();

        let current_files: HashSet<PathBuf> = files.iter().cloned().collect();
        self.file_positions().retain(|path, _| current_files.contains(path));

        let mut new_records = Vec::new();
        for file in files {
            let offset = self.file_positions().get(&file).copied().unwrap_or(0);
            let (entries, bytes_read) = self.read_file_from(&file, offset);
            self.file_positions().insert(file, bytes_read);
            new_records.extend(entries);
        }
        new_records.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        new_records
    }
}