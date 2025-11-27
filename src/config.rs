use anyhow::{Context, Result};
use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub github_token: String,
}

impl Config {
    pub fn load() -> Result<Self> {
        // Load .env file if it exists
        dotenv::dotenv().ok();

        let github_token = env::var("GITHUB_TOKEN")
            .context("GITHUB_TOKEN not found. Please set it in .env file or environment")?;

        if github_token.is_empty() {
            anyhow::bail!("GITHUB_TOKEN is empty");
        }

        Ok(Config { github_token })
    }
}
