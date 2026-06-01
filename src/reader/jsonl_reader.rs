use crate::state::UsageRecord;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Trait for reading JSONL files with incremental updates
pub trait JsonlReader {
    /// Get mutable reference to file positions map
    fn file_positions(&mut self) -> &mut HashMap<PathBuf, u64>;
    
    /// Find all relevant JSONL files
    fn find_files(&self) -> Vec<PathBuf>;
    
    /// Parse a single line into a UsageRecord
    fn parse_line(&self, line: &str) -> Option<UsageRecord>;
    
    /// Read records from a file starting from a given line offset
    /// Returns (records, total_lines_read)
    fn read_file_from(&self, path: &Path, skip_lines: u64) -> (Vec<UsageRecord>, u64) {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return (Vec::new(), skip_lines),
        };
        
        let complete_lines = if content.ends_with('\n') {
            content.lines().count()
        } else {
            content.lines().count().saturating_sub(1)
        };
        
        let records = content
            .lines()
            .take(complete_lines)
            .skip(skip_lines as usize)
            .filter_map(|line| self.parse_line(line))
            .collect();
        
        (records, complete_lines as u64)
    }
    
    /// Scan all files and return all records
    fn scan_all(&mut self) -> Vec<UsageRecord> {
        let files = self.find_files();
        let mut records = Vec::new();
        for file in files {
            let (entries, lines_read) = self.read_file_from(&file, 0);
            self.file_positions().insert(file, lines_read);
            records.extend(entries);
        }
        records.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        records
    }
    
    /// Poll for new records since last check
    fn poll_delta(&mut self) -> Vec<UsageRecord> {
        let files = self.find_files();
        
        // Clean up positions for files that no longer exist
        let current_files: HashSet<PathBuf> = files.iter().cloned().collect();
        self.file_positions().retain(|path, _| current_files.contains(path));
        
        let mut new_records = Vec::new();
        for file in files {
            let offset = self.file_positions().get(&file).copied().unwrap_or(0);
            let (entries, lines_read) = self.read_file_from(&file, offset);
            self.file_positions().insert(file, lines_read);
            new_records.extend(entries);
        }
        new_records.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        new_records
    }
}
