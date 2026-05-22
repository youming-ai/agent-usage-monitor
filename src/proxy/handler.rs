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
    let app_state = state.app_state.clone();
    let bytes_stream = upstream_resp.bytes_stream();

    let mapped = bytes_stream.map(move |chunk_result| {
        let chunk = match chunk_result {
            Ok(c) => c,
            Err(e) => return Err(axum::Error::new(e)),
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

        Ok::<_, axum::Error>(chunk)
    });

    let body = Body::from_stream(mapped);
    Response::builder()
        .status(status)
        .body(body)
        .unwrap()
}
