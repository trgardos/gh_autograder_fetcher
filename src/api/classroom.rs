use crate::models::{AcceptedAssignment, Assignment, Classroom};
use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, USER_AGENT};
use serde::de::DeserializeOwned;

const API_BASE: &str = "https://api.github.com";

#[derive(Clone)]
pub struct ClassroomClient {
    client: reqwest::Client,
    token: String,
}

impl ClassroomClient {
    pub fn new(token: String) -> Self {
        let client = reqwest::Client::new();
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

        // Get the response text first for debugging
        let response_text = response.text().await.context("Failed to get response text")?;

        // Try to parse JSON and provide helpful error message
        serde_json::from_str(&response_text).with_context(|| {
            format!(
                "Failed to parse JSON response from {}. Response body (first 500 chars): {}",
                url,
                &response_text.chars().take(500).collect::<String>()
            )
        })
    }

    pub async fn list_classrooms(&self) -> Result<Vec<Classroom>> {
        let mut all_classrooms = Vec::new();
        let mut page = 1;

        loop {
            let path = format!("/classrooms?page={}&per_page=100", page);
            let classrooms: Vec<Classroom> = self.get(&path).await?;

            if classrooms.is_empty() {
                break;
            }

            all_classrooms.extend(classrooms);
            page += 1;

            // GitHub Classroom typically doesn't have many classrooms per user
            // Break after 10 pages to avoid infinite loops
            if page > 10 {
                break;
            }
        }

        Ok(all_classrooms)
    }

    pub async fn get_classroom(&self, classroom_id: u64) -> Result<Classroom> {
        let path = format!("/classrooms/{}", classroom_id);
        self.get(&path).await
    }

    pub async fn list_assignments(&self, classroom_id: u64) -> Result<Vec<Assignment>> {
        let mut all_assignments = Vec::new();
        let mut page = 1;

        loop {
            let path = format!(
                "/classrooms/{}/assignments?page={}&per_page=100",
                classroom_id, page
            );
            let assignments: Vec<Assignment> = self.get(&path).await?;

            if assignments.is_empty() {
                break;
            }

            all_assignments.extend(assignments);
            page += 1;

            // Break after 10 pages
            if page > 10 {
                break;
            }
        }

        Ok(all_assignments)
    }

    pub async fn get_assignment(&self, assignment_id: u64) -> Result<Assignment> {
        let path = format!("/assignments/{}", assignment_id);
        self.get(&path).await
    }

    pub async fn list_accepted_assignments(
        &self,
        assignment_id: u64,
    ) -> Result<Vec<AcceptedAssignment>> {
        let mut all_accepted = Vec::new();
        let mut page = 1;

        loop {
            let path = format!(
                "/assignments/{}/accepted_assignments?page={}&per_page=100",
                assignment_id, page
            );
            let accepted: Vec<AcceptedAssignment> = self.get(&path).await?;

            if accepted.is_empty() {
                break;
            }

            all_accepted.extend(accepted);
            page += 1;

            // Break after 100 pages (10,000 students should be enough!)
            if page > 100 {
                break;
            }
        }

        Ok(all_accepted)
    }
}
