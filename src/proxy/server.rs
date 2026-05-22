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
