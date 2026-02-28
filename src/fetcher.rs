use crate::api::{ClassroomClient, GitHubClient};
use crate::models::{AcceptedAssignment, StudentResult, TestDefinition, TestResult};
use crate::parser;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use indexmap::IndexMap;

/// Parse repository URL to extract owner and repo name
pub fn parse_repo_url(full_name: &str) -> (&str, &str) {
    let parts: Vec<&str> = full_name.split('/').collect();
    if parts.len() >= 2 {
        (parts[0], parts[1])
    } else {
        ("", "")
    }
}

/// Parse per-test scores from GitHub Classroom autograding reporter logs.
/// Looks for lines like "Total points for {runner-id}: {score}/{max}".
/// Returns a map of runner ID (step id) → points awarded.
fn parse_test_scores_from_logs(logs: &str) -> std::collections::HashMap<String, u32> {
    let mut scores = std::collections::HashMap::new();
    let marker = "Total points for ";

    for line in logs.lines() {
        if let Some(idx) = line.find(marker) {
            let rest = &line[idx + marker.len()..];
            if let Some(colon_idx) = rest.find(": ") {
                let runner_id = rest[..colon_idx].trim().to_string();
                let score_part = &rest[colon_idx + 2..];
                if let Some(slash_idx) = score_part.find('/') {
                    let score_str = score_part[..slash_idx].trim();
                    if let Ok(score_f) = score_str.parse::<f64>() {
                        scores.insert(runner_id, score_f.round() as u32);
                    }
                }
            }
        }
    }

    scores
}

/// Fetch test definitions from the assignment's starter repository
pub async fn fetch_test_definitions(
    github_client: &GitHubClient,
    starter_code_url: &str,
) -> Result<Vec<TestDefinition>> {
    // Extract owner/repo from starter code URL
    // URL format: https://github.com/owner/repo
    let url_parts: Vec<&str> = starter_code_url
        .trim_end_matches('/')
        .split('/')
        .collect();

    if url_parts.len() < 2 {
        anyhow::bail!("Invalid starter code URL: {}", starter_code_url);
    }

    let repo = url_parts[url_parts.len() - 1];
    let owner = url_parts[url_parts.len() - 2];

    // Fetch workflow YAML file
    let workflow_content = github_client
        .get_file_contents(owner, repo, ".github/workflows/classroom.yml")
        .await
        .context("Failed to fetch workflow file from starter repository")?;

    // Parse workflow to extract test definitions
    parser::parse_workflow(&workflow_content)
        .context("Failed to parse workflow file")
}

/// Fetch results for a single student
pub async fn fetch_student_results(
    github_client: &GitHubClient,
    student: &AcceptedAssignment,
    deadline: Option<DateTime<Utc>>,
    test_definitions: &[TestDefinition],
) -> Result<StudentResult> {
    let (owner, repo) = parse_repo_url(&student.repository.full_name);

    if owner.is_empty() || repo.is_empty() {
        anyhow::bail!("Invalid repository name: {}", student.repository.full_name);
    }

    // Get the student username and display name (first student in the list)
    let username = student
        .students
        .first()
        .map(|s| s.login.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let display_name = student
        .students
        .first()
        .and_then(|s| s.name.clone());

    // Build filter for workflow runs
    let created_filter = deadline.map(|dt| format!(">={}", dt.to_rfc3339()));

    // Get workflow runs
    let runs_response = github_client
        .list_workflow_runs(
            owner,
            repo,
            Some("repository_dispatch"),
            created_filter.as_deref(),
            Some("completed"),
        )
        .await
        .context(format!("Failed to fetch workflow runs for {}", username))?;

    // Find the first completed run after deadline (or latest if no deadline)
    let target_run = if let Some(_deadline) = deadline {
        // Get first run after deadline (minimum created_at)
        runs_response
            .workflow_runs
            .into_iter()
            .filter(|r| r.conclusion.is_some())
            .min_by_key(|r| r.created_at)
    } else {
        // Get latest run (maximum created_at)
        runs_response
            .workflow_runs
            .into_iter()
            .filter(|r| r.conclusion.is_some())
            .max_by_key(|r| r.created_at)
    };

    let run = target_run.context(format!(
        "No completed workflow run found for {}",
        username
    ))?;

    // Note: We don't use check runs as they don't contain actual points information
    // The points are only available in the job logs

    // Initialize tests with pass/fail from job steps
    let jobs_response = github_client
        .list_jobs_for_run(owner, repo, run.id)
        .await
        .context(format!("Failed to fetch jobs for {}", username))?;

    let autograding_job = jobs_response
        .jobs
        .into_iter()
        .find(|j| j.name == "run-autograding-tests")
        .context(format!(
            "No 'run-autograding-tests' job found for {}",
            username
        ))?;

    let mut tests = IndexMap::new();

    // Initialize all tests with 0 points; scores will be set from job logs below
    for test_def in test_definitions {
        tests.insert(
            test_def.name.clone(),
            TestResult {
                _name: test_def.name.clone(),
                points_awarded: 0,
                _points_available: test_def.max_score,
                _passed: false,
            },
        );
    }

    // Parse per-test scores from job logs using the reporter's
    // "Total points for {runner-id}: {score}/{max}" lines.
    // The runner-id matches the workflow step id field.
    if let Ok(logs) = github_client.get_job_logs(owner, repo, autograding_job.id).await {
        let log_scores = parse_test_scores_from_logs(&logs);

        for test_def in test_definitions {
            if let Some(&score) = log_scores.get(&test_def.id) {
                if let Some(result) = tests.get_mut(&test_def.name) {
                    result.points_awarded = score;
                    result._passed = score > 0;
                }
            }
        }
    }

    let total_awarded: u32 = tests.values().map(|t| t.points_awarded).sum();

    let total_available = test_definitions.iter().map(|t| t.max_score).sum();

    Ok(StudentResult {
        username,
        display_name,
        repo_url: student.repository.html_url.clone(),
        workflow_run_timestamp: run.created_at,
        tests,
        total_awarded,
        total_available,
    })
}

/// Fetch results for late grading (both on-time and late deadlines)
pub async fn fetch_all_late_results(
    classroom_client: &ClassroomClient,
    github_client: &GitHubClient,
    assignment_id: u64,
    on_time_deadline: DateTime<Utc>,
    late_deadline: DateTime<Utc>,
    late_penalty: f64,
    progress_callback: Option<Box<dyn Fn(usize, usize, &str) + Send>>,
) -> Result<Vec<crate::models::LateGradingResult>> {
    // Get assignment details
    let assignment = classroom_client
        .get_assignment(assignment_id)
        .await
        .context("Failed to fetch assignment details")?;

    // Get all accepted assignments (students)
    let accepted_assignments = classroom_client
        .list_accepted_assignments(assignment_id)
        .await
        .context("Failed to fetch accepted assignments")?;

    if accepted_assignments.is_empty() {
        anyhow::bail!("No students have accepted this assignment yet");
    }

    // Fetch test definitions from starter repo, or from first student's repo if no starter
    let test_definitions = if let Some(starter_url) = &assignment.starter_code_url {
        fetch_test_definitions(github_client, starter_url).await?
    } else {
        // No starter repo, fetch from first student's repository
        let first_student = &accepted_assignments[0];
        let (owner, repo) = parse_repo_url(&first_student.repository.full_name);

        if owner.is_empty() || repo.is_empty() {
            anyhow::bail!("Invalid repository name: {}", first_student.repository.full_name);
        }

        let workflow_content = github_client
            .get_file_contents(owner, repo, ".github/workflows/classroom.yml")
            .await
            .context("Failed to fetch workflow file from first student's repository")?;

        parser::parse_workflow(&workflow_content)
            .context("Failed to parse workflow file")?
    };

    let total_students = accepted_assignments.len();
    let mut results = Vec::new();

    // Fetch results for each student
    for (index, student) in accepted_assignments.iter().enumerate() {
        let student_name = student
            .students
            .first()
            .map(|s| s.login.as_str())
            .unwrap_or("unknown");

        // Call progress callback if provided
        if let Some(ref callback) = progress_callback {
            callback(index + 1, total_students, student_name);
        }

        // Fetch on-time results
        let on_time_result = match fetch_student_results(
            github_client,
            student,
            Some(on_time_deadline),
            &test_definitions
        ).await {
            Ok(result) => result,
            Err(e) => {
                eprintln!("Error fetching on-time results for {}: {}", student_name, e);
                continue;
            }
        };

        // Fetch late results
        let late_result = match fetch_student_results(
            github_client,
            student,
            Some(late_deadline),
            &test_definitions
        ).await {
            Ok(result) => result,
            Err(e) => {
                eprintln!("Error fetching late results for {}: {}", student_name, e);
                continue;
            }
        };

        // Create late grading result
        let late_grading_result = crate::models::LateGradingResult::new(
            on_time_result,
            late_result,
            late_penalty,
        );

        results.push(late_grading_result);
    }

    Ok(results)
}
