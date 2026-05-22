mod cli;
mod event;
mod ollama_client;
mod proxy;
mod state;
mod ui;

use clap::Parser;

#[tokio::main]
async fn main() {
    let _args = cli::Cli::parse();
    println!("ollama-monitor starting...");
}
