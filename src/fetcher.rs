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

/// Parse points from GitHub Classroom check run summary
/// Looks for patterns like "Points XX/YY" or "XX/YY"
fn parse_points_from_summary(summary: &str) -> Option<u32> {
    // Try to find "Points XX/YY" pattern
    if let Some(points_idx) = summary.find("Points") {
        let after_points = &summary[points_idx + 6..];
        // Extract the numbers
        if let Some(slash_idx) = after_points.find('/') {
            let before_slash = &after_points[..slash_idx].trim();
            // Extract just the number
            if let Some(num_str) = before_slash.split_whitespace().last() {
                if let Ok(points) = num_str.parse::<u32>() {
                    return Some(points);
                }
            }
        }
    }

    // Try to find "XX/YY" pattern directly
    for line in summary.lines() {
        if let Some(slash_idx) = line.find('/') {
            // Get the characters before the slash
            let before_slash = &line[..slash_idx];
            // Try to extract the number
            if let Some(num_str) = before_slash.split_whitespace().last() {
                if let Ok(points) = num_str.parse::<u32>() {
                    return Some(points);
                }
            }
        }
    }

    None
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

    // Get the student username (first student in the list)
    let username = student
        .students
        .first()
        .map(|s| s.login.clone())
        .unwrap_or_else(|| "unknown".to_string());

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

    for test_def in test_definitions {
        // Find matching step by name
        let step = autograding_job
            .steps
            .iter()
            .find(|s| s.name == test_def.name);

        let (points_awarded, passed) = if let Some(step) = step {
            match step.conclusion.as_deref() {
                Some("success") => (test_def.max_score, true),
                Some("failure") => (0, false),
                _ => (0, false),
            }
        } else {
            (0, false)
        };

        tests.insert(
            test_def.name.clone(),
            TestResult {
                name: test_def.name.clone(),
                points_awarded,
                points_available: test_def.max_score,
                passed,
            },
        );
    }

    // Get actual points from job logs
    // GitHub Classroom autograding reporter outputs the final score in the logs
    let mut total_awarded: u32 = tests.values().map(|t| t.points_awarded).sum();

    // Try to fetch job logs and parse actual points (handles partial credit)
    if let Ok(logs) = github_client.get_job_logs(owner, repo, autograding_job.id).await {
        if let Some(points) = parse_points_from_summary(&logs) {
            total_awarded = points;
        }
    }

    let total_available = test_definitions.iter().map(|t| t.max_score).sum();

    Ok(StudentResult {
        username,
        repo_url: student.repository.html_url.clone(),
        workflow_run_timestamp: run.created_at,
        tests,
        total_awarded,
        total_available,
    })
}

/// Fetch results for all students in an assignment
pub async fn fetch_all_results(
    classroom_client: &ClassroomClient,
    github_client: &GitHubClient,
    assignment_id: u64,
    deadline: Option<DateTime<Utc>>,
    progress_callback: Option<Box<dyn Fn(usize, usize, &str) + Send>>,
) -> Result<Vec<StudentResult>> {
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

        match fetch_student_results(github_client, student, deadline, &test_definitions).await {
            Ok(result) => results.push(result),
            Err(e) => {
                eprintln!("Error fetching results for {}: {}", student_name, e);
                // Continue with other students
            }
        }
    }

    Ok(results)
}
