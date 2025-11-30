use crate::models::{CheckRunsResponse, FileContent, JobsResponse, WorkflowRunsResponse};
use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, USER_AGENT};
use serde::de::DeserializeOwned;

const API_BASE: &str = "https://api.github.com";

#[derive(Clone)]
pub struct GitHubClient {
    client: reqwest::Client,
    token: String,
}

impl GitHubClient {
    pub fn new(token: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120)) // 2 minute timeout
            .connect_timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to build HTTP client");
        Self { client, token }
    }

    fn build_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", self.token)).unwrap(),
        );
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github+json"),
        );
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static("gh-autograder-fetcher"),
        );
        headers.insert(
            "X-GitHub-Api-Version",
            HeaderValue::from_static("2022-11-28"),
        );
        headers
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", API_BASE, path);
        let response = self
            .client
            .get(&url)
            .headers(self.build_headers())
            .send()
            .await
            .context(format!("Failed to send request to {}", url))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("API request failed with status {}: {}", status, error_text);
        }

        response
            .json()
            .await
            .context("Failed to parse JSON response")
    }

    /// Get file contents from a repository
    pub async fn get_file_contents(&self, owner: &str, repo: &str, path: &str) -> Result<String> {
        let api_path = format!("/repos/{}/{}/contents/{}", owner, repo, path);
        let file_content: FileContent = self.get(&api_path).await?;

        // GitHub API returns base64-encoded content
        if file_content.encoding == "base64" {
            let decoded = base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                file_content.content.replace('\n', ""),
            )
            .context("Failed to decode base64 content")?;

            String::from_utf8(decoded).context("File content is not valid UTF-8")
        } else {
            Ok(file_content.content)
        }
    }

    /// List workflow runs for a repository
    pub async fn list_workflow_runs(
        &self,
        owner: &str,
        repo: &str,
        event: Option<&str>,
        created: Option<&str>,
        status: Option<&str>,
    ) -> Result<WorkflowRunsResponse> {
        let mut path = format!("/repos/{}/{}/actions/runs?per_page=100", owner, repo);

        if let Some(event) = event {
            path.push_str(&format!("&event={}", event));
        }
        if let Some(created) = created {
            path.push_str(&format!("&created={}", created));
        }
        if let Some(status) = status {
            path.push_str(&format!("&status={}", status));
        }

        self.get(&path).await
    }

    /// Get jobs for a workflow run
    pub async fn list_jobs_for_run(
        &self,
        owner: &str,
        repo: &str,
        run_id: u64,
    ) -> Result<JobsResponse> {
        let path = format!("/repos/{}/{}/actions/runs/{}/jobs", owner, repo, run_id);
        self.get(&path).await
    }

    /// List check runs for a git reference (commit SHA, branch, or tag)
    pub async fn list_check_runs_for_ref(
        &self,
        owner: &str,
        repo: &str,
        git_ref: &str,
    ) -> Result<CheckRunsResponse> {
        let path = format!("/repos/{}/{}/commits/{}/check-runs?per_page=100", owner, repo, git_ref);
        self.get(&path).await
    }

    /// Get logs for a job
    pub async fn get_job_logs(
        &self,
        owner: &str,
        repo: &str,
        job_id: u64,
    ) -> Result<String> {
        let url = format!("{}/repos/{}/{}/actions/jobs/{}/logs", API_BASE, owner, repo, job_id);
        let response = self
            .client
            .get(&url)
            .headers(self.build_headers())
            .send()
            .await
            .context(format!("Failed to send request to {}", url))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("API request failed with status {}: {}", status, error_text);
        }

        response
            .text()
            .await
            .context("Failed to read log text")
    }
}
