use crate::state::{AppState, RunningModel};
use chrono::{DateTime, Local, Utc};
use reqwest::Client;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct PsResponse {
    pub models: Vec<PsModel>,
}

#[derive(Debug, Deserialize)]
pub struct PsModel {
    pub name: String,
    pub model: String,
    pub size: u64,
    #[serde(default)]
    pub size_vram: u64,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub details: PsDetails,
}

#[derive(Debug, Deserialize, Default)]
pub struct PsDetails {
    #[serde(default)]
    pub parameter_size: String,
}

pub struct OllamaClient {
    client: Client,
    base_url: String,
}

impl OllamaClient {
    pub fn new(base_url: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
        }
    }

    pub async fn poll_ps(&self) -> Result<PsResponse, reqwest::Error> {
        let url = format!("{}/api/ps", self.base_url);
        let resp = self.client.get(&url).send().await?;
        let ps = resp.json::<PsResponse>().await?;
        Ok(ps)
    }

    pub fn update_state(ps: PsResponse, state: &mut AppState) {
        state.running_models = ps
            .models
            .into_iter()
            .map(|m| {
                let running_for = match &m.expires_at {
                    Some(expires) => {
                        if let Ok(dt) = DateTime::parse_from_rfc3339(expires) {
                            let now: DateTime<Utc> = Utc::now();
                            let diff = now.signed_duration_since(dt);
                            format_running_for(diff)
                        } else {
                            "unknown".to_string()
                        }
                    }
                    None => "unknown".to_string(),
                };
                RunningModel {
                    name: m.name.clone(),
                    running_for,
                    size: m.size,
                    vram: if m.size_vram > 0 {
                        Some(m.size_vram)
                    } else {
                        None
                    },
                }
            })
            .collect();
    }
}

fn format_running_for(dur: chrono::TimeDelta) -> String {
    let abs = dur.abs();
    let secs = abs.num_seconds();
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    }
}