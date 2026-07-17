use super::QuotaInfo;
use super::util::{read_email, signed_in};
use serde_json::Value;

#[cfg(target_os = "macos")]
fn read_keychain_credentials() -> Option<String> {
    use std::process::Command;
    let output = Command::new("/usr/bin/security")
        .args([
            "find-generic-password",
            "-s",
            "Antigravity Safe Storage",
            "-w",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let raw = String::from_utf8(output.stdout).ok()?;
    Some(raw.trim().to_string())
}

#[cfg(target_os = "macos")]
fn read_keychain_email() -> Option<String> {
    use std::process::Command;
    let output = Command::new("/usr/bin/security")
        .args(["find-generic-password", "-s", "Antigravity Safe Storage"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let raw = String::from_utf8(output.stdout).ok()?;
    let marker = "\"acct\"<blob>=\"";
    raw.lines().find_map(|line| {
        let line = line.trim();
        if line.starts_with(marker) {
            let account = line.strip_prefix(marker)?.strip_suffix('\"')?.to_string();
            Some(account)
        } else {
            None
        }
    })
}

fn read_antigravity_token() -> Option<(String, Option<String>)> {
    // 1. Env var
    if let Ok(val) = std::env::var("ANTIGRAVITY_API_KEY") {
        let val = val.trim().to_string();
        if !val.is_empty() {
            return Some((val, Some("env key".to_string())));
        }
    }

    // 2. macOS Keychain
    #[cfg(target_os = "macos")]
    {
        if let Some(tok) = read_keychain_credentials() {
            let email = read_keychain_email();
            return Some((tok, email));
        }
    }

    // 3. Token files
    if let Some(home) = dirs::home_dir() {
        let paths = &[
            home.join(".gemini/antigravity-cli/antigravity-oauth-token"),
            home.join(".gemini/antigravity/mcp_oauth_tokens.json"),
        ];
        for path in paths {
            if let Ok(content) = std::fs::read_to_string(path) {
                let content = content.trim().to_string();
                if !content.is_empty() {
                    if let Ok(json) = serde_json::from_str::<Value>(&content) {
                        let token = json
                            .get("access_token")
                            .or_else(|| json.get("token"))
                            .and_then(|v| v.as_str());
                        if let Some(tok) = token {
                            return Some((tok.to_string(), None));
                        }
                    } else {
                        return Some((content, None));
                    }
                }
            }
        }
    }
    None
}

pub fn fetch_quota() -> Option<QuotaInfo> {
    let email = read_antigravity_token().map(|(token, file_email)| {
        read_email(&token, file_email).unwrap_or_else(|| "Signed in".to_string())
    });
    signed_in("Antigravity CLI", email)
}
