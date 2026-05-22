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
