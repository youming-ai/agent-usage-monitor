use super::util::decode_jwt_payload;
use super::{QuotaError, QuotaInfo};
use crate::state::Platform;
use std::path::PathBuf;
use std::time::Instant;

fn get_vscdb_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(home) = dirs::home_dir() {
        // macOS
        paths.push(home.join("Library/Application Support/Cursor/User/globalStorage/state.vscdb"));
        paths.push(
            home.join(
                "Library/Application Support/Cursor - Insiders/User/globalStorage/state.vscdb",
            ),
        );
        // Linux
        paths.push(home.join(".config/Cursor/User/globalStorage/state.vscdb"));
        paths.push(home.join(".config/cursor/User/globalStorage/state.vscdb"));
    }
    // Windows env vars
    if let Ok(app_data) = std::env::var("APPDATA") {
        let app_data_path = PathBuf::from(app_data);
        paths.push(app_data_path.join("Cursor/User/globalStorage/state.vscdb"));
        paths.push(app_data_path.join("Cursor - Insiders/User/globalStorage/state.vscdb"));
    }
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        let local_app_data_path = PathBuf::from(local_app_data);
        paths.push(local_app_data_path.join("Cursor/User/globalStorage/state.vscdb"));
        paths.push(local_app_data_path.join("Programs/Cursor/User/globalStorage/state.vscdb"));
    }
    paths
}

fn read_cursor_token() -> Option<String> {
    for path in get_vscdb_paths() {
        if !path.exists() {
            continue;
        }
        let conn = match rusqlite::Connection::open_with_flags(
            &path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let mut stmt = match conn.prepare("SELECT value FROM itemTable WHERE key = ?1") {
            Ok(s) => s,
            Err(_) => continue,
        };

        for key in &["cursorAuth/accessToken", "cursorAuth/token"] {
            let val: Option<String> = stmt.query_row([key], |row| row.get(0)).ok();
            if let Some(raw_val) = val {
                let token = serde_json::from_str::<String>(&raw_val)
                    .unwrap_or(raw_val)
                    .trim()
                    .to_string();
                if !token.is_empty() {
                    return Some(token);
                }
            }
        }
    }
    None
}

fn read_email(token: &str) -> Option<String> {
    let payload = decode_jwt_payload(token)?;
    payload
        .get("email")
        .and_then(|v| v.as_str())
        .map(String::from)
}

pub fn fetch_quota() -> Option<QuotaInfo> {
    if let Some(token) = read_cursor_token() {
        let email = read_email(&token).unwrap_or_else(|| "Signed in".to_string());
        Some(QuotaInfo {
            tool_name: "Cursor CLI".to_string(),
            email: Some(email),
            account_id: None,
            windows: vec![],
            fetched_at: Instant::now(),
            error: None,
        })
    } else {
        Some(QuotaInfo {
            tool_name: "Cursor CLI".to_string(),
            email: None,
            account_id: None,
            windows: vec![],
            fetched_at: Instant::now(),
            error: Some(QuotaError::NoCredentials),
        })
    }
}

pub struct CursorQuotaFetcher;

impl super::QuotaFetcher for CursorQuotaFetcher {
    fn platform(&self) -> Platform {
        Platform::Cursor
    }
    fn fetch(&self) -> Option<QuotaInfo> {
        fetch_quota()
    }
}
