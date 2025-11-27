mod api;
mod config;
mod export;
mod fetcher;
mod models;
mod parser;
mod ui;

use anyhow::{Context, Result};
use config::Config;
use ui::App;

#[tokio::main]
async fn main() -> Result<()> {
    // Load configuration
    let config = Config::load().context("Failed to load configuration")?;

    // Initialize API clients
    let classroom_client = api::ClassroomClient::new(config.github_token.clone());
    let github_client = api::GitHubClient::new(config.github_token);

    // Start TUI application
    let mut app = App::new(classroom_client, github_client);
    app.run().await?;

    Ok(())
}
