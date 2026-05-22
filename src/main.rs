mod cli;
mod ollama_client;
mod proxy;
mod state;

use clap::Parser;

#[tokio::main]
async fn main() {
    let _args = cli::Cli::parse();
    println!("ollama-monitor starting...");
}
