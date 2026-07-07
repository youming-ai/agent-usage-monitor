use super::util::decode_jwt_payload;
use super::{QuotaError, QuotaInfo};
use crate::state::Platform;
use serde_json::Value;
use std::path::PathBuf;
use std::time::Instant;

fn get_grok_auth_path() -> PathBuf {
    if let Ok(val) = std::env::var("GROK_HOME") {
        let p = PathBuf::from(val);
        if p.is_absolute() {
            return p.join("auth.json");
        }
    }
    if let Some(home) = dirs::home_dir() {
        return home.join(".grok/auth.json");
    }
    PathBuf::from(".grok/auth.json")
}

fn read_grok_creds() -> Option<(String, Option<String>)> {
    // 1. Env vars
    for var in &["XAI_API_KEY", "GROK_BUILD_API_KEY"] {
        if let Ok(val) = std::env::var(var) {
            let val = val.trim().to_string();
            if !val.is_empty() {
                return Some((val, Some("env key".to_string())));
            }
        }
    }

    // 2. auth.json
    let path = get_grok_auth_path();
    if !path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(&path).ok()?;
    let json: Value = serde_json::from_str(&content).ok()?;

    if let Some(obj) = json.as_object() {
        for (_key, val) in obj {
            let token = val
                .get("key")
                .or_else(|| val.get("access_token"))
                .and_then(|v| v.as_str());
            let email = val.get("email").and_then(|v| v.as_str()).map(String::from);
            if let Some(tok) = token {
                return Some((tok.to_string(), email));
            }
        }
    }
    None
}

fn read_email(token: &str, file_email: Option<String>) -> Option<String> {
    decode_jwt_payload(token)
        .and_then(|payload| {
            payload
                .get("email")
                .and_then(|v| v.as_str())
                .map(String::from)
        })
        .or(file_email)
}

pub fn fetch_quota() -> Option<QuotaInfo> {
    if let Some((token, file_email)) = read_grok_creds() {
        let email = read_email(&token, file_email).unwrap_or_else(|| "Signed in".to_string());
        Some(QuotaInfo {
            tool_name: "Grok".to_string(),
            email: Some(email),
            account_id: None,
            windows: vec![],
            fetched_at: Instant::now(),
            error: None,
        })
    } else {
        Some(QuotaInfo {
            tool_name: "Grok".to_string(),
            email: None,
            account_id: None,
            windows: vec![],
            fetched_at: Instant::now(),
            error: Some(QuotaError::NoCredentials),
        })
    }
}

pub struct GrokQuotaFetcher;

impl super::QuotaFetcher for GrokQuotaFetcher {
    fn platform(&self) -> Platform {
        Platform::Grok
    }
    fn fetch(&self) -> Option<QuotaInfo> {
        fetch_quota()
    }
}
