use super::QuotaInfo;
use super::util::{decode_jwt_payload, signed_in};
use serde_json::Value;
use std::path::PathBuf;

fn get_auth_json_path() -> PathBuf {
    if let Ok(val) = std::env::var("OPENCODE_AUTH_JSON") {
        let p = PathBuf::from(val);
        if p.is_absolute() {
            return p;
        }
    }
    crate::config::xdg_data_dir()
        .join("opencode")
        .join("auth.json")
}

fn read_opencode_token() -> Option<(String, Option<String>)> {
    let path = get_auth_json_path();
    if !path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(&path).ok()?;
    let json: Value = serde_json::from_str(&content).ok()?;

    if let Some(opencode_cred) = json.get("opencode") {
        let token = opencode_cred
            .get("key")
            .or_else(|| opencode_cred.get("token"))
            .or_else(|| opencode_cred.get("access"))
            .and_then(|v| v.as_str());
        if let Some(tok) = token {
            let email = if let Some(payload) = decode_jwt_payload(tok) {
                payload
                    .get("email")
                    .and_then(|v| v.as_str())
                    .map(String::from)
            } else {
                None
            };
            return Some((
                tok.to_string(),
                email.or_else(|| Some("Zen/Go account".to_string())),
            ));
        }
    }

    if let Some(map) = json.as_object().filter(|m| !m.is_empty()) {
        let first_prov = map.keys().next().cloned().unwrap_or_default();
        return Some((
            "logged_in".to_string(),
            Some(format!("connected ({first_prov})")),
        ));
    }
    None
}

pub fn fetch_quota() -> Option<QuotaInfo> {
    let email = read_opencode_token().and_then(|(_token, email)| email);
    signed_in("opencode", email)
}
