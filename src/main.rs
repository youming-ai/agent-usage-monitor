mod state;

#[tokio::main]
async fn main() {
    let _state = state::AppState::new();
    println!("ollama-monitor starting...");
}
