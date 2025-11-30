use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// GitHub Classroom API Models
// ============================================================================

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Classroom {
    pub id: u64,
    pub name: String,
    pub archived: bool,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Assignment {
    pub id: u64,
    pub title: String,
    pub slug: String,
    #[serde(default)]
    pub accepted: u32,
    #[serde(default)]
    pub submitted: u32,
    #[serde(default)]
    pub passing: u32,
    pub deadline: Option<DateTime<Utc>>,
    pub starter_code_url: Option<String>,
    pub classroom: SimpleClassroom,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SimpleClassroom {
    pub id: u64,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AcceptedAssignment {
    pub id: u64,
    #[serde(default)]
    pub submitted: bool,
    #[serde(default)]
    pub passing: bool,
    #[serde(default)]
    pub commit_count: u32,
    pub grade: Option<String>,
    pub students: Vec<Student>,
    pub repository: Repository,
    pub assignment: AssignmentInfo,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Student {
    pub id: u64,
    pub login: String,
    pub name: Option<String>,
    pub avatar_url: String,
    pub html_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Repository {
    pub id: u64,
    pub full_name: String,
    pub html_url: String,
    pub default_branch: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AssignmentInfo {
    pub id: u64,
    pub title: String,
}

// ============================================================================
// GitHub Actions API Models
// ============================================================================

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkflowRunsResponse {
    pub total_count: u32,
    pub workflow_runs: Vec<WorkflowRun>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkflowRun {
    pub id: u64,
    pub name: String,
    pub head_branch: String,
    pub head_sha: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub run_started_at: Option<DateTime<Utc>>,
    pub event: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JobsResponse {
    pub total_count: u32,
    pub jobs: Vec<Job>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Job {
    pub id: u64,
    pub name: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub steps: Vec<JobStep>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JobStep {
    pub name: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub number: u32,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

// ============================================================================
// GitHub Repository Content API Models
// ============================================================================

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FileContent {
    pub name: String,
    pub path: String,
    pub sha: String,
    pub size: u64,
    pub content: String,
    pub encoding: String,
}

// ============================================================================
// GitHub Checks API Models
// ============================================================================

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CheckRunsResponse {
    pub total_count: u32,
    pub check_runs: Vec<CheckRun>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CheckRun {
    pub id: u64,
    pub name: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub output: Option<CheckRunOutput>,
    pub app: Option<CheckRunApp>,
    pub annotations_count: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CheckRunOutput {
    pub title: Option<String>,
    pub summary: Option<String>,
    pub text: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CheckRunApp {
    pub id: u64,
    pub slug: String,
    pub name: String,
}

// ============================================================================
// Workflow YAML Models
// ============================================================================

#[derive(Debug, Clone, Deserialize)]
pub struct WorkflowFile {
    pub jobs: HashMap<String, WorkflowJob>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkflowJob {
    pub steps: Vec<WorkflowStep>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkflowStep {
    pub name: String,
    pub id: Option<String>,
    pub uses: Option<String>,
    #[serde(rename = "with")]
    pub with: Option<StepWith>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StepWith {
    #[serde(rename = "test-name")]
    pub test_name: Option<String>,
    #[serde(rename = "max-score")]
    pub max_score: Option<u32>,
}

// ============================================================================
// Internal Models for Processing
// ============================================================================

#[derive(Debug, Clone)]
pub struct TestDefinition {
    pub name: String,
    pub id: String,
    pub max_score: u32,
}

#[derive(Debug, Clone)]
pub struct StudentResult {
    pub username: String,
    pub repo_url: String,
    pub workflow_run_timestamp: DateTime<Utc>,
    pub tests: IndexMap<String, TestResult>,
    pub total_awarded: u32,
    pub total_available: u32,
}

#[derive(Debug, Clone)]
pub struct TestResult {
    pub name: String,
    pub points_awarded: u32,
    pub points_available: u32,
    pub passed: bool,
}

#[derive(Debug, Clone)]
pub struct ResultStats {
    pub total_students: usize,
    pub total_tests: usize,
    pub average_score: f64,
    pub median_score: f64,
    pub students_processed: usize,
    pub errors: usize,
}

impl ResultStats {
    pub fn calculate(results: &[StudentResult]) -> Self {
        let total_students = results.len();
        let total_tests = results
            .first()
            .map(|r| r.tests.len())
            .unwrap_or(0);

        let average_score = if total_students > 0 {
            let sum: f64 = results
                .iter()
                .map(|r| {
                    if r.total_available > 0 {
                        (r.total_awarded as f64 / r.total_available as f64) * 100.0
                    } else {
                        0.0
                    }
                })
                .sum();
            sum / total_students as f64
        } else {
            0.0
        };

        let mut scores: Vec<f64> = results
            .iter()
            .map(|r| {
                if r.total_available > 0 {
                    (r.total_awarded as f64 / r.total_available as f64) * 100.0
                } else {
                    0.0
                }
            })
            .collect();
        scores.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let median_score = if !scores.is_empty() {
            let mid = scores.len() / 2;
            if scores.len() % 2 == 0 {
                (scores[mid - 1] + scores[mid]) / 2.0
            } else {
                scores[mid]
            }
        } else {
            0.0
        };

        Self {
            total_students,
            total_tests,
            average_score,
            median_score,
            students_processed: total_students,
            errors: 0,
        }
    }
}
