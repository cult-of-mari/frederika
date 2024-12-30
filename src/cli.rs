use clap::Parser;
use std::path::PathBuf;

/// Chat Telegram bot that utilizes Gemini API and written in Rust 🚀🚀🚀
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Cli {
    /// Config .toml file
    #[arg(short, long)]
    pub config: PathBuf,
}

pub fn parse_cli() -> Cli {
    Cli::parse()
}
