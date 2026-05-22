use chrono::{DateTime, Local};
use std::sync::atomic::{AtomicBool, Ordering};

pub const MAX_RECENT_CALLS: usize = 50;

#[derive(Debug, Clone)]
pub struct RunningModel {
    pub name: String,
    pub running_for: String,
    pub size: u64,
    pub vram: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct ApiCall {
    pub timestamp: DateTime<Local>,
    pub model: String,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_duration_ms: u64,
    pub tokens_per_sec: f64,
}

pub struct AppState {
    pub running_models: Vec<RunningModel>,
    pub recent_calls: Vec<ApiCall>,
    pub total_calls: usize,
    pub proxy_paused: AtomicBool,
    pub last_error: Option<String>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            running_models: Vec::new(),
            recent_calls: Vec::with_capacity(MAX_RECENT_CALLS),
            total_calls: 0,
            proxy_paused: AtomicBool::new(false),
            last_error: None,
        }
    }

    pub fn add_call(&mut self, call: ApiCall) {
        if self.recent_calls.len() >= MAX_RECENT_CALLS {
            self.recent_calls.remove(0);
        }
        self.recent_calls.push(call);
        self.total_calls += 1;
    }

    pub fn clear_calls(&mut self) {
        self.recent_calls.clear();
        self.total_calls = 0;
    }

    pub fn is_proxy_paused(&self) -> bool {
        self.proxy_paused.load(Ordering::Relaxed)
    }

    pub fn toggle_proxy_paused(&self) {
        let current = self.proxy_paused.load(Ordering::Relaxed);
        self.proxy_paused.store(!current, Ordering::Relaxed);
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}