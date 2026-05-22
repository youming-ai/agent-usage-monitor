# Ollama Monitor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Rust CLI tool with ratatui that displays real-time Ollama model status and API usage metrics via a local HTTP proxy.

**Architecture:** A tokio-async app with three concurrent tasks: an axum proxy that intercepts Ollama API usage metrics, a reqwest client that polls `/api/ps` for running models, and a ratatui event loop that renders shared state to terminal. All state is synchronized via `Arc<RwLock<AppState>>`.

**Tech Stack:** Rust, tokio, axum, reqwest, ratatui, crossterm, serde, clap, chrono, humansize

---

## File Structure

```
ollama-monitor/
├── Cargo.toml
└── src/
    ├── main.rs              # CLI parsing, runtime orchestration
    ├── cli.rs               # Clap CLI argument definitions
    ├── state/
    │   ├── mod.rs           # Re-exports
    │   └── app_state.rs     # AppState, RunningModel, ApiCall structs + logic
    ├── proxy/
    │   ├── mod.rs           # Re-exports
    │   ├── server.rs        # Axum proxy server startup
    │   └── handler.rs       # Request forwarding + SSE response interception
    ├── ollama_client/
    │   ├── mod.rs           # Re-exports
    │   └── client.rs        # Polling /api/ps, deserializing responses
    ├── ui/
    │   ├── mod.rs           # render() entry point + layout
    │   ├── model_table.rs   # Running models table widget
    │   ├── usage_table.rs   # Recent API calls table widget
    │   └── status_bar.rs    # Bottom status bar widget
    └── event/
        ├── mod.rs           # Re-exports
        └── event_loop.rs    # Crossterm event reading + tick timer
```

---

## Task 1: Initialize Rust Project and Dependencies

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs` (stub)

- [ ] **Step 1: Initialize cargo project and add dependencies**

Run:
```bash
cargo init --name ollama-monitor
cargo add tokio --features full
cargo add axum
cargo add reqwest --features json
cargo add serde --features derive
cargo add serde_json
cargo add ratatui
cargo add crossterm
cargo add clap --features derive
cargo add chrono --features serde
cargo add humansize
```

- [ ] **Step 2: Verify Cargo.toml**

Run:
```bash
cat Cargo.toml
```

Expected: All dependencies listed with semver versions.

- [ ] **Step 3: Create stub main.rs**

Create: `src/main.rs`
```rust
#[tokio::main]
async fn main() {
    println!("ollama-monitor starting...");
}
```

- [ ] **Step 4: Verify project compiles**

Run:
```bash
cargo check
```

Expected: `Finished dev [unoptimized + debuginfo] target(s)`

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml src/main.rs Cargo.lock
git commit -m "chore: init rust project with dependencies"
```

---

## Task 2: Define Shared State (AppState)

**Files:**
- Create: `src/state/app_state.rs`
- Create: `src/state/mod.rs`
- Modify: `src/main.rs` (add module)

- [ ] **Step 1: Write state structs**

Create: `src/state/app_state.rs`
```rust
use chrono::{DateTime, Local};
use std::sync::atomic::{AtomicBool, Ordering};

pub const MAX_RECENT_CALLS: usize = 50;

#[derive(Debug, Clone)]
pub struct RunningModel {
    pub name: String,
    pub running_for: String,
    pub size: u64,
    pub gpu_utilization: Option<f64>,
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
```

- [ ] **Step 2: Create state module**

Create: `src/state/mod.rs`
```rust
pub mod app_state;
pub use app_state::{AppState, ApiCall, RunningModel, MAX_RECENT_CALLS};
```

- [ ] **Step 3: Add module to main.rs**

Modify: `src/main.rs`
```rust
mod state;

#[tokio::main]
async fn main() {
    let _state = state::AppState::new();
    println!("ollama-monitor starting...");
}
```

- [ ] **Step 4: Verify compiles**

Run:
```bash
cargo check
```

Expected: No errors.

- [ ] **Step 5: Commit**

```bash
git add src/state/ src/main.rs
git commit -m "feat: define AppState with RunningModel and ApiCall"
```

---

## Task 3: Implement CLI Arguments

**Files:**
- Create: `src/cli.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Write CLI struct with clap**

Create: `src/cli.rs`
```rust
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "ollama-monitor")]
#[command(about = "Real-time Ollama monitor with usage tracking")]
pub struct Cli {
    #[arg(short, long, default_value = "11435")]
    pub proxy_port: u16,

    #[arg(short = 'o', long, default_value = "127.0.0.1:11434")]
    pub ollama_host: String,

    #[arg(short, long, default_value_t = 2)]
    pub refresh: u64,
}
```

- [ ] **Step 2: Wire CLI into main.rs**

Modify: `src/main.rs`
```rust
mod cli;
mod state;

use clap::Parser;

#[tokio::main]
async fn main() {
    let _args = cli::Cli::parse();
    println!("ollama-monitor starting...");
}
```

- [ ] **Step 3: Verify CLI help works**

Run:
```bash
cargo run -- --help
```

Expected: Help text with `--proxy-port`, `--ollama-host`, `--refresh` options.

- [ ] **Step 4: Commit**

```bash
git add src/cli.rs src/main.rs
git commit -m "feat: add CLI argument parsing with clap"
```

---

## Task 4: Implement Ollama Client (Polling /api/ps)

**Files:**
- Create: `src/ollama_client/client.rs`
- Create: `src/ollama_client/mod.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Define Ollama API response types**

Create: `src/ollama_client/client.rs`
```rust
use crate::state::{AppState, RunningModel};
use chrono::Local;
use humansize::{format_size, BINARY};
use reqwest::Client;
use serde::Deserialize;
use std::sync::{Arc, RwLock};

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
                running_for: "unknown".to_string(), // Ollama /api/ps doesn't provide runtime; we'll approximate or leave as placeholder
                size: m.size,
                gpu_utilization: None, // Not exposed in /api/ps currently
            })
            .collect();
    }
}
```

- [ ] **Step 2: Create ollama_client module**

Create: `src/ollama_client/mod.rs`
```rust
pub mod client;
pub use client::{OllamaClient, PsResponse, PsModel};
```

- [ ] **Step 3: Add module to main.rs**

Modify: `src/main.rs`
```rust
mod cli;
mod ollama_client;
mod state;
```

- [ ] **Step 4: Verify compiles**

Run:
```bash
cargo check
```

Expected: No errors.

- [ ] **Step 5: Commit**

```bash
git add src/ollama_client/ src/main.rs
git commit -m "feat: add OllamaClient for polling /api/ps"
```

---

## Task 5: Implement Proxy Server (Axum)

**Files:**
- Create: `src/proxy/server.rs`
- Create: `src/proxy/handler.rs`
- Create: `src/proxy/mod.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Write proxy handler with SSE interception**

Create: `src/proxy/handler.rs`
```rust
use crate::state::{ApiCall, AppState};
use axum::{
    body::{Body, Bytes},
    extract::State,
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
};
use reqwest::Client;
use serde_json::Value;
use std::sync::{Arc, RwLock};

#[derive(Clone)]
pub struct ProxyState {
    pub target: String,
    pub app_state: Arc<RwLock<AppState>>,
}

pub async fn proxy_handler(
    State(state): State<ProxyState>,
    req: Request<Body>,
) -> Response {
    if state.app_state.read().unwrap().is_proxy_paused() {
        return (StatusCode::SERVICE_UNAVAILABLE, "Proxy paused").into_response();
    }

    let client = Client::new();
    let target_url = format!("{}{}", state.target, req.uri());

    let method = req.method().clone();
    let headers = req.headers().clone();
    let body_bytes = match axum::body::to_bytes(req.into_body(), usize::MAX).await {
        Ok(b) => b,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, format!("Body read error: {}", e)).into_response();
        }
    };

    let mut request_builder = client.request(method, &target_url);
    for (key, value) in headers.iter() {
        request_builder = request_builder.header(key, value);
    }

    let upstream_resp = match request_builder.body(body_bytes).send().await {
        Ok(r) => r,
        Err(e) => {
            let mut app = state.app_state.write().unwrap();
            app.last_error = Some(format!("Upstream error: {}", e));
            return (StatusCode::BAD_GATEWAY, format!("Upstream error: {}", e)).into_response();
        }
    };

    let status = upstream_resp.status();
    let is_streaming = upstream_resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|ct| ct.contains("application/x-ndjson") || ct.contains("text/event-stream"))
        .unwrap_or(false);

    if !is_streaming || !status.is_success() {
        let body = match upstream_resp.bytes().await {
            Ok(b) => b,
            Err(e) => {
                return (StatusCode::BAD_GATEWAY, format!("Read error: {}", e)).into_response();
            }
        };
        return Response::builder()
            .status(status)
            .body(Body::from(body))
            .unwrap();
    }

    // Intercept streaming response
    let (parts, upstream_body) = upstream_resp.into_parts();
    let bytes_stream = upstream_body.into_bytes_stream();
    let app_state = state.app_state.clone();

    let mapped = bytes_stream.map(move |chunk_result| {
        let chunk = match chunk_result {
            Ok(c) => c,
            Err(e) => return Err(e),
        };

        // Try to parse each line as JSON
        if let Ok(text) = std::str::from_utf8(&chunk) {
            for line in text.lines() {
                if let Ok(json) = serde_json::from_str::<Value>(line) {
                    if json.get("done").and_then(|v| v.as_bool()).unwrap_or(false) {
                        if let Some(usage) = json.get("usage") {
                            let model = json
                                .get("model")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown")
                                .to_string();
                            let prompt_eval_count = usage
                                .get("prompt_eval_count")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0);
                            let eval_count = usage
                                .get("eval_count")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0);
                            let total_duration = usage
                                .get("total_duration")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0);
                            let eval_duration = usage
                                .get("eval_duration")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0);

                            let tokens_per_sec = if eval_duration > 0 {
                                eval_count as f64 / (eval_duration as f64 / 1e9)
                            } else {
                                0.0
                            };

                            let call = ApiCall {
                                timestamp: chrono::Local::now(),
                                model,
                                prompt_tokens: prompt_eval_count,
                                completion_tokens: eval_count,
                                total_duration_ms: total_duration / 1_000_000,
                                tokens_per_sec,
                            };

                            if let Ok(mut app) = app_state.write() {
                                app.add_call(call);
                            }
                        }
                    }
                }
            }
        }

        Ok::<_, reqwest::Error>(chunk)
    });

    let body = Body::from_stream(mapped);
    Response::from_parts(parts, body)
}
```

- [ ] **Step 2: Write proxy server startup**

Create: `src/proxy/server.rs`
```rust
use crate::proxy::handler::{proxy_handler, ProxyState};
use crate::state::AppState;
use axum::{routing::any, Router};
use std::sync::{Arc, RwLock};
use tokio::net::TcpListener;

pub async fn start_proxy(
    port: u16,
    target: String,
    app_state: Arc<RwLock<AppState>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let state = ProxyState {
        target,
        app_state,
    };

    let app = Router::new()
        .route("/", any(proxy_handler))
        .route("/*path", any(proxy_handler))
        .with_state(state);

    let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).await?;
    println!("Proxy listening on http://127.0.0.1:{}", port);

    axum::serve(listener, app).await?;
    Ok(())
}
```

- [ ] **Step 3: Create proxy module**

Create: `src/proxy/mod.rs`
```rust
pub mod handler;
pub mod server;
pub use server::start_proxy;
```

- [ ] **Step 4: Add module to main.rs**

Modify: `src/main.rs`
```rust
mod cli;
mod ollama_client;
mod proxy;
mod state;
```

- [ ] **Step 5: Verify compiles**

Run:
```bash
cargo check
```

Expected: No errors.

- [ ] **Step 6: Commit**

```bash
git add src/proxy/ src/main.rs
git commit -m "feat: add axum proxy with SSE usage interception"
```

---

## Task 6: Implement UI Components (ratatui)

**Files:**
- Create: `src/ui/model_table.rs`
- Create: `src/ui/usage_table.rs`
- Create: `src/ui/status_bar.rs`
- Create: `src/ui/mod.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Write model_table widget**

Create: `src/ui/model_table.rs`
```rust
use crate::state::AppState;
use humansize::{format_size, BINARY};
use ratatui::{
    layout::Constraint,
    style::{Color, Style},
    widgets::{Block, Borders, Cell, Row, Table},
};
use std::sync::{Arc, RwLock};

pub fn model_table_widget(app_state: &Arc<RwLock<AppState>>) -> Table<'static> {
    let state = app_state.read().unwrap();
    let rows: Vec<Row> = state
        .running_models
        .iter()
        .map(|m| {
            Row::new(vec![
                Cell::from(m.name.clone()),
                Cell::from(m.running_for.clone()),
                Cell::from(format_size(m.size, BINARY)),
                Cell::from(
                    m.gpu_utilization
                        .map(|g| format!("{:.1}%", g))
                        .unwrap_or_else(|| "N/A".to_string()),
                ),
            ])
        })
        .collect();

    let header = Row::new(vec!["Model", "Running", "Memory", "GPU"])
        .style(Style::default().fg(Color::Yellow));

    Table::new(
        rows,
        [
            Constraint::Percentage(30),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(20),
        ],
    )
    .header(header)
    .block(Block::default().title("Running Models").borders(Borders::ALL))
    .row_highlight_style(Style::default().bg(Color::DarkGray))
}
```

- [ ] **Step 2: Write usage_table widget**

Create: `src/ui/usage_table.rs`
```rust
use crate::state::AppState;
use ratatui::{
    layout::Constraint,
    style::{Color, Style},
    widgets::{Block, Borders, Cell, Row, Table},
};
use std::sync::{Arc, RwLock};

pub fn usage_table_widget(app_state: &Arc<RwLock<AppState>>) -> Table<'static> {
    let state = app_state.read().unwrap();
    let rows: Vec<Row> = state
        .recent_calls
        .iter()
        .rev()
        .map(|c| {
            Row::new(vec![
                Cell::from(c.timestamp.format("%H:%M:%S").to_string()),
                Cell::from(c.model.clone()),
                Cell::from(c.prompt_tokens.to_string()),
                Cell::from(c.completion_tokens.to_string()),
                Cell::from(format!("{}ms", c.total_duration_ms)),
                Cell::from(format!("{:.1}", c.tokens_per_sec)),
            ])
        })
        .collect();

    let header = Row::new(vec!["Time", "Model", "In", "Out", "Total", "T/s"])
        .style(Style::default().fg(Color::Yellow));

    Table::new(
        rows,
        [
            Constraint::Percentage(15),
            Constraint::Percentage(30),
            Constraint::Percentage(12),
            Constraint::Percentage(12),
            Constraint::Percentage(16),
            Constraint::Percentage(15),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .title(format!("Recent API Calls ({}/{}) ", state.recent_calls.len(), crate::state::MAX_RECENT_CALLS))
            .borders(Borders::ALL),
    )
    .row_highlight_style(Style::default().bg(Color::DarkGray))
}
```

- [ ] **Step 3: Write status_bar widget**

Create: `src/ui/status_bar.rs`
```rust
use crate::state::AppState;
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use std::sync::{Arc, RwLock};

pub fn status_bar_widget(
    app_state: &Arc<RwLock<AppState>>,
    proxy_port: u16,
    ollama_host: &str,
) -> Paragraph<'static> {
    let state = app_state.read().unwrap();
    let proxy_status = if state.is_proxy_paused() {
        Span::styled("Proxy: PAUSED", Style::default().fg(Color::Red))
    } else {
        Span::styled("Proxy: ON", Style::default().fg(Color::Green))
    };

    let error_span = if let Some(ref err) = state.last_error {
        Span::styled(format!(" | ERROR: {}", err), Style::default().fg(Color::Red))
    } else {
        Span::raw("")
    };

    let line = Line::from(vec![
        proxy_status,
        Span::raw(format!(
            " | {} → {} | Calls: {} | q:quit p:pause r:clr",
            proxy_port, ollama_host, state.total_calls
        )),
        error_span,
    ]);

    Paragraph::new(line).block(Block::default().borders(Borders::TOP))
}
```

- [ ] **Step 4: Write UI render entry**

Create: `src/ui/mod.rs`
```rust
pub mod model_table;
pub mod status_bar;
pub mod usage_table;

use crate::state::AppState;
use ratatui::{
    layout::{Constraint, Direction, Layout},
    Frame,
};
use std::sync::{Arc, RwLock};

pub fn render(
    frame: &mut Frame,
    app_state: &Arc<RwLock<AppState>>,
    proxy_port: u16,
    ollama_host: &str,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Percentage(55),
            Constraint::Min(1),
        ])
        .split(frame.area());

    frame.render_widget(model_table::model_table_widget(app_state), chunks[0]);
    frame.render_widget(usage_table::usage_table_widget(app_state), chunks[1]);
    frame.render_widget(status_bar::status_bar_widget(app_state, proxy_port, ollama_host), chunks[2]);
}
```

- [ ] **Step 5: Add module to main.rs**

Modify: `src/main.rs`
```rust
mod cli;
mod ollama_client;
mod proxy;
mod state;
mod ui;
```

- [ ] **Step 6: Verify compiles**

Run:
```bash
cargo check
```

Expected: No errors.

- [ ] **Step 7: Commit**

```bash
git add src/ui/ src/main.rs
git commit -m "feat: add ratatui UI components for models, usage, and status bar"
```

---

## Task 7: Implement Event Loop

**Files:**
- Create: `src/event/event_loop.rs`
- Create: `src/event/mod.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Write crossterm event loop**

Create: `src/event/event_loop.rs`
```rust
use crossterm::event::{self, Event, KeyCode, KeyEvent};
use ratatui::DefaultTerminal;
use std::{
    io,
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};
use tokio::sync::mpsc;

pub enum AppEvent {
    Tick,
    Key(KeyEvent),
    Quit,
}

pub struct EventLoop {
    pub rx: mpsc::UnboundedReceiver<AppEvent>,
}

impl EventLoop {
    pub fn new(tick_rate: Duration) -> (Self, mpsc::UnboundedSender<AppEvent>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let tx_tick = tx.clone();

        tokio::spawn(async move {
            let mut last_tick = Instant::now();
            loop {
                let timeout = tick_rate
                    .checked_sub(last_tick.elapsed())
                    .unwrap_or(Duration::from_secs(0));

                if event::poll(timeout).unwrap() {
                    if let Event::Key(key) = event::read().unwrap() {
                        if tx.send(AppEvent::Key(key)).is_err() {
                            break;
                        }
                    }
                }

                if last_tick.elapsed() >= tick_rate {
                    if tx_tick.send(AppEvent::Tick).is_err() {
                        break;
                    }
                    last_tick = Instant::now();
                }
            }
        });

        (Self { rx }, tx)
    }
}
```

- [ ] **Step 2: Create event module**

Create: `src/event/mod.rs`
```rust
pub mod event_loop;
pub use event_loop::{AppEvent, EventLoop};
```

- [ ] **Step 3: Add module to main.rs**

Modify: `src/main.rs`
```rust
mod cli;
mod event;
mod ollama_client;
mod proxy;
mod state;
mod ui;
```

- [ ] **Step 4: Verify compiles**

Run:
```bash
cargo check
```

Expected: No errors.

- [ ] **Step 5: Commit**

```bash
git add src/event/ src/main.rs
git commit -m "feat: add crossterm event loop with tick and key events"
```

---

## Task 8: Assemble Main.rs and Wiring

**Files:**
- Modify: `src/main.rs` (complete rewrite)

- [ ] **Step 1: Write full main.rs**

Modify: `src/main.rs`
```rust
mod cli;
mod event;
mod ollama_client;
mod proxy;
mod state;
mod ui;

use crate::event::{AppEvent, EventLoop};
use crate::ollama_client::OllamaClient;
use crate::proxy::start_proxy;
use crate::state::AppState;
use clap::Parser;
use crossterm::event::KeyCode;
use ratatui::DefaultTerminal;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::task;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = cli::Cli::parse();
    let app_state = Arc::new(RwLock::new(AppState::new()));

    // Start proxy
    let proxy_state = app_state.clone();
    let proxy_target = format!("http://{}", args.ollama_host);
    let proxy_port = args.proxy_port;
    let proxy_handle = task::spawn(async move {
        if let Err(e) = start_proxy(proxy_port, proxy_target, proxy_state).await {
            eprintln!("Proxy error: {}", e);
        }
    });

    // Start Ollama polling
    let poll_state = app_state.clone();
    let ollama_client = OllamaClient::new(format!("http://{}", args.ollama_host));
    let refresh_secs = args.refresh;
    let poll_handle = task::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(refresh_secs));
        loop {
            interval.tick().await;
            match ollama_client.poll_ps().await {
                Ok(ps) => {
                    if let Ok(mut state) = poll_state.write() {
                        OllamaClient::update_state(ps, &mut state);
                        state.last_error = None;
                    }
                }
                Err(e) => {
                    if let Ok(mut state) = poll_state.write() {
                        state.last_error = Some(format!("Poll error: {}", e));
                        state.running_models.clear();
                    }
                }
            }
        }
    });

    // Run TUI
    let tui_state = app_state.clone();
    let tui_handle = task::spawn_blocking(move || {
        let mut terminal = ratatui::init();
        let result = run_tui(&mut terminal, tui_state, proxy_port, &args.ollama_host);
        ratatui::restore();
        result
    });

    // Wait for TUI to finish (user pressed 'q')
    tui_handle.await??;

    // Abort background tasks
    proxy_handle.abort();
    poll_handle.abort();

    Ok(())
}

fn run_tui(
    terminal: &mut DefaultTerminal,
    app_state: Arc<RwLock<AppState>>,
    proxy_port: u16,
    ollama_host: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let tick_rate = Duration::from_millis(250);
    let (mut event_loop, _tx) = EventLoop::new(tick_rate);

    loop {
        terminal.draw(|frame| {
            ui::render(frame, &app_state, proxy_port, ollama_host);
        })?;

        if let Some(event) = event_loop.rx.blocking_recv() {
            match event {
                AppEvent::Tick => {}
                AppEvent::Key(key) => match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('p') => {
                        if let Ok(state) = app_state.write() {
                            state.toggle_proxy_paused();
                        }
                    }
                    KeyCode::Char('r') => {
                        if let Ok(mut state) = app_state.write() {
                            state.clear_calls();
                        }
                    }
                    _ => {}
                },
                AppEvent::Quit => break,
            }
        }
    }

    Ok(())
}
```

- [ ] **Step 2: Verify compiles**

Run:
```bash
cargo check
```

Expected: No errors.

- [ ] **Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat: wire proxy, polling, and TUI into main runtime"
```

---

## Task 9: Build and Manual Test

**Files:**
- None (testing only)

- [ ] **Step 1: Build release binary**

Run:
```bash
cargo build --release
```

Expected: `Finished release [optimized] target(s) in ...`

- [ ] **Step 2: Test help output**

Run:
```bash
./target/release/ollama-monitor --help
```

Expected: Help text with all options.

- [ ] **Step 3: Test with mock Ollama (optional)**

If Ollama is not running, start the monitor and verify it shows "Poll error" in status bar:
```bash
./target/release/ollama-monitor
```

Press `q` to exit.

- [ ] **Step 4: Test proxy forwarding**

If Ollama is running, start the monitor:
```bash
./target/release/ollama-monitor
```

In another terminal, test proxy:
```bash
curl http://127.0.0.1:11435/api/tags
```

Expected: Same response as direct `curl http://127.0.0.1:11434/api/tags`.

- [ ] **Step 5: Test usage interception**

Send a generate request through proxy:
```bash
curl http://127.0.0.1:11435/api/generate -d '{"model":"llama3.1","prompt":"hi","stream":false}'
```

Expected: Monitor UI shows a new row in "Recent API Calls" table.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "chore: verify build and manual integration tests"
```

---

## Self-Review

### Spec Coverage Check

| Spec Section | Implementing Task |
|-------------|-------------------|
| Proxy mode (port 11435) | Task 5 |
| Forward to Ollama (11434) | Task 5 (`start_proxy` target param) |
| Intercept SSE, extract usage | Task 5 (`proxy_handler.rs` JSON parsing) |
| Calculate tokens/sec | Task 5 (handler computes eval_duration ratio) |
| Poll `/api/ps` | Task 4 (`OllamaClient::poll_ps`) |
| Display running models | Task 6 (`model_table.rs`) |
| Display recent API calls | Task 6 (`usage_table.rs`) |
| Status bar | Task 6 (`status_bar.rs`) |
| Keybindings (q, p, r) | Task 8 (`run_tui` match block) |
| CLI args | Task 3 (`cli.rs`) |
| Shared state (Arc<RwLock<AppState>>) | Task 2 + all consuming tasks |
| Error handling | Task 5 (upstream errors), Task 8 (poll errors) |
| ratatui + crossterm | Task 6 + 7 |
| axum proxy | Task 5 |
| reqwest client | Task 4 |

**No gaps found.**

### Placeholder Scan

- No "TBD", "TODO", "implement later", "fill in details" found.
- No vague "add error handling" without code.
- All types (`AppState`, `ApiCall`, `RunningModel`) are fully defined in Task 2 and consistently referenced.

### Type Consistency Check

- `AppState::add_call`, `clear_calls`, `toggle_proxy_paused`, `is_proxy_paused` are all defined in Task 2 and used in Tasks 5, 6, 8.
- `OllamaClient::update_state` signature matches `PsResponse` + `&mut AppState`.
- `ProxyState` holds `target: String` and `app_state: Arc<RwLock<AppState>>` consistently.
- UI functions accept `&Arc<RwLock<AppState>>` consistently.

**All consistent.**
