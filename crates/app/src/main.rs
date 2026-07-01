mod cli;
mod config;
mod mcp;
mod query;
mod services;
mod tui;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    cli::run().await
}
