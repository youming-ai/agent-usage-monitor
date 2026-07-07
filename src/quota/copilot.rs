use super::util::decode_jwt_payload;
use super::{QuotaError, QuotaInfo};
use crate::state::Platform;
use serde_json::Value;
use std::path::PathBuf;
use std::time::Instant;

fn get_hosts_json_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(home) = dirs::home_dir() {
        paths.push(home.join(".config/github-copilot/hosts.json"));
        paths.push(home.join(".config/github-copilot/apps.json"));
        paths.push(home.join(".copilot/config.json"));
        paths.push(home.join(".config/gh/hosts.yml"));
    }
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        let path = PathBuf::from(local_app_data);
        paths.push(path.join("github-copilot/hosts.json"));
        paths.push(path.join("github-copilot/apps.json"));
    }
    paths
}

fn read_copilot_creds() -> Option<(String, Option<String>)> {
    // 1. Env vars
    for var in &["COPILOT_GITHUB_TOKEN", "GH_TOKEN", "GITHUB_TOKEN"] {
        if let Ok(val) = std::env::var(var) {
            let val = val.trim().to_string();
            if !val.is_empty() {
                let email = if let Some(payload) = decode_jwt_payload(&val) {
                    payload
                        .get("email")
                        .and_then(|v| v.as_str())
                        .map(String::from)
                } else {
                    None
                };
                return Some((val, email.or_else(|| Some(format!("env {var}")))));
            }
        }
    }

    // 2. Local config files
    for path in get_hosts_json_paths() {
        if !path.exists() {
            continue;
        }

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        if path.extension().and_then(|s| s.to_str()) == Some("yml") {
            let mut user = None;
            let mut token = None;
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("user:") {
                    user = trimmed.split("user:").nth(1).map(|s| s.trim().to_string());
                } else if trimmed.starts_with("oauth_token:") {
                    token = trimmed
                        .split("oauth_token:")
                        .nth(1)
                        .map(|s| s.trim().to_string());
                }
            }
            if let Some(tok) = token {
                return Some((tok, user));
            }
        } else if let Ok(json) = serde_json::from_str::<Value>(&content) {
            if let Some(gh) = json.get("github.com") {
                let token = gh.get("oauth_token").and_then(|v| v.as_str());
                let user = gh.get("user").and_then(|v| v.as_str()).map(String::from);
                if let Some(tok) = token {
                    return Some((tok.to_string(), user));
                }
            }
            let token = json
                .get("oauth_token")
                .or_else(|| json.get("token"))
                .or_else(|| json.get("access_token"))
                .and_then(|v| v.as_str());
            let user = json
                .get("user")
                .or_else(|| json.get("email"))
                .and_then(|v| v.as_str())
                .map(String::from);
            if let Some(tok) = token {
                return Some((tok.to_string(), user));
            }
        }
    }
    None
}

fn read_email(token: &str, file_user: Option<String>) -> Option<String> {
    decode_jwt_payload(token)
        .and_then(|payload| {
            payload
                .get("email")
                .and_then(|v| v.as_str())
                .map(String::from)
        })
        .or(file_user)
}

pub fn fetch_quota() -> Option<QuotaInfo> {
    if let Some((token, file_user)) = read_copilot_creds() {
        let email = read_email(&token, file_user).unwrap_or_else(|| "Signed in".to_string());
        Some(QuotaInfo {
            tool_name: "Copilot CLI".to_string(),
            email: Some(email),
            account_id: None,
            windows: vec![],
            fetched_at: Instant::now(),
            error: None,
        })
    } else {
        Some(QuotaInfo {
            tool_name: "Copilot CLI".to_string(),
            email: None,
            account_id: None,
            windows: vec![],
            fetched_at: Instant::now(),
            error: Some(QuotaError::NoCredentials),
        })
    }
}

pub struct CopilotQuotaFetcher;

impl super::QuotaFetcher for CopilotQuotaFetcher {
    fn platform(&self) -> Platform {
        Platform::Copilot
    }
    fn fetch(&self) -> Option<QuotaInfo> {
        fetch_quota()
    }
}
