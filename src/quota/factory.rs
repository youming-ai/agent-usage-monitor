use super::util::decode_jwt_payload;
use super::{QuotaError, QuotaInfo};
use crate::state::Platform;
use serde_json::Value;
use std::path::PathBuf;
use std::time::Instant;

fn get_factory_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(home) = dirs::home_dir() {
        paths.push(home.join(".factory/settings.json"));
        paths.push(home.join(".factory/settings.local.json"));
        paths.push(home.join(".factory/config.json"));
    }
    paths
}

fn read_factory_token() -> Option<(String, Option<String>)> {
    if let Ok(val) = std::env::var("FACTORY_API_KEY") {
        let val = val.trim().to_string();
        if !val.is_empty() {
            return Some((val, Some("env key".to_string())));
        }
    }

    for path in get_factory_paths() {
        if !path.exists() {
            continue;
        }
        let content = std::fs::read_to_string(&path).ok()?;
        let json: Value = serde_json::from_str(&content).ok()?;

        let token_keys = &[
            "apiKey",
            "api_key",
            "token",
            "access_token",
            "factoryApiKey",
            "factory_api_key",
            "auth_token",
        ];

        for key in token_keys {
            if let Some(tok_val) = json.get(*key).and_then(|v| v.as_str()) {
                let tok_val = tok_val.trim().to_string();
                if !tok_val.is_empty() && !tok_val.starts_with('$') {
                    return Some((tok_val, None));
                }
            }
        }

        if let Some(custom_models) = json
            .get("customModels")
            .or_else(|| json.get("custom_models"))
            .and_then(|v| v.as_array())
        {
            for model_obj in custom_models {
                for key in token_keys {
                    if let Some(tok_val) = model_obj.get(*key).and_then(|v| v.as_str()) {
                        let tok_val = tok_val.trim().to_string();
                        if !tok_val.is_empty() && !tok_val.starts_with('$') {
                            return Some((tok_val, None));
                        }
                    }
                }
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
    if let Some((token, file_email)) = read_factory_token() {
        let email = read_email(&token, file_email).unwrap_or_else(|| "Signed in".to_string());
        Some(QuotaInfo {
            tool_name: "Factory".to_string(),
            email: Some(email),
            account_id: None,
            windows: vec![],
            fetched_at: Instant::now(),
            error: None,
        })
    } else {
        Some(QuotaInfo {
            tool_name: "Factory".to_string(),
            email: None,
            account_id: None,
            windows: vec![],
            fetched_at: Instant::now(),
            error: Some(QuotaError::NoCredentials),
        })
    }
}

pub struct FactoryQuotaFetcher;

impl super::QuotaFetcher for FactoryQuotaFetcher {
    fn platform(&self) -> Platform {
        Platform::Factory
    }
    fn fetch(&self) -> Option<QuotaInfo> {
        fetch_quota()
    }
}
