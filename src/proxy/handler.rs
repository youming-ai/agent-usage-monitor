use crate::state::{ApiCall, AppState};
use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
};
use futures_util::StreamExt;
use reqwest::Client;
use serde_json::Value;
use std::sync::{Arc, Mutex, RwLock};

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
    let content_type = upstream_resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let is_streaming = content_type.contains("application/x-ndjson")
        || content_type.contains("text/event-stream");
    let is_json = content_type.contains("application/json");

    // Non-streaming JSON response: parse entire body for usage
    if !is_streaming && is_json && status.is_success() {
        let body = match upstream_resp.bytes().await {
            Ok(b) => b,
            Err(e) => {
                return (StatusCode::BAD_GATEWAY, format!("Read error: {}", e)).into_response();
            }
        };
        if let Ok(json) = serde_json::from_slice::<Value>(&body) {
            // For non-streaming, done is always true, but check anyway
            if json.get("done").and_then(|v| v.as_bool()).unwrap_or(true) {
                extract_usage(&json, &state.app_state);
            }
        }
        return Response::builder()
            .status(status)
            .body(Body::from(body))
            .unwrap();
    }

    // Non-success or non-JSON: just forward
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

    // Intercept streaming response with line buffering to handle chunks split across TCP packets
    let app_state = state.app_state.clone();
    let bytes_stream = upstream_resp.bytes_stream();
    let line_buffer: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));

    let mapped = bytes_stream.map(move |chunk_result| {
        let chunk = match chunk_result {
            Ok(c) => c,
            Err(e) => return Err(axum::Error::new(e)),
        };

        let mut buffer = line_buffer.lock().unwrap_or_else(|e| e.into_inner());
        if let Ok(text) = std::str::from_utf8(&chunk) {
            buffer.push_str(text);
            // Process complete lines, keep partial last line in buffer
            let mut lines = buffer.lines().peekable();
            let mut new_buffer = String::new();
            while let Some(line) = lines.next() {
                if lines.peek().is_none() {
                    // Last line might be incomplete, keep it for next chunk
                    new_buffer.push_str(line);
                    break;
                }
                // Try to parse complete line as JSON
                if let Ok(json) = serde_json::from_str::<Value>(line) {
                    // Ollama generate/chat: done=true in final chunk has usage
                    if json.get("done").and_then(|v| v.as_bool()).unwrap_or(false) {
                        extract_usage(&json, &app_state);
                    }
                }
            }
            *buffer = new_buffer;
        }

        Ok::<_, axum::Error>(chunk)
    });

    let body = Body::from_stream(mapped);
    Response::builder()
        .status(status)
        .body(body)
        .unwrap()
}

fn extract_usage(json: &Value, app_state: &Arc<RwLock<AppState>>) {
    let model = json
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let prompt_eval_count = json
        .get("prompt_eval_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let eval_count = json
        .get("eval_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let total_duration = json
        .get("total_duration")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let eval_duration = json
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