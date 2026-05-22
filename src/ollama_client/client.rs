use crate::state::{AppState, RunningModel};
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
            .map(|m| RunningModel {
                name: m.name.clone(),
                running_for: "unknown".to_string(),
                size: m.size,
                gpu_utilization: None,
            })
            .collect();
    }
}
