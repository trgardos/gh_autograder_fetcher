use crate::models::{LateGradingResult, StudentResult};
use anyhow::{Context, Result};
use chrono::Utc;
use std::path::PathBuf;

/// Export student results to CSV file
pub fn export_to_csv(
    results: &[StudentResult],
    assignment_name: &str,
) -> Result<PathBuf> {
    if results.is_empty() {
        anyhow::bail!("No results to export");
    }

    // Generate filename with timestamp
    let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
    let filename = format!("results_{}_{}.csv", assignment_name, timestamp);
    let filepath = PathBuf::from(&filename);

    // Collect all unique test names (preserve order from first student)
    let test_names: Vec<String> = results
        .first()
        .map(|r| r.tests.keys().cloned().collect())
        .unwrap_or_default();

    // Build CSV headers
    let mut headers = vec![
        "student_username".to_string(),
        "student_repo_url".to_string(),
        "workflow_run_timestamp".to_string(),
    ];

    // Add test names as headers
    headers.extend(test_names.clone());

    // Add summary columns
    headers.extend_from_slice(&[
        "total_points_awarded".to_string(),
        "total_points_available".to_string(),
        "percentage".to_string(),
    ]);

    // Create CSV writer
    let mut wtr = csv::Writer::from_path(&filepath)
        .context("Failed to create CSV file")?;

    // Write headers
    wtr.write_record(&headers)
        .context("Failed to write CSV headers")?;

    // Write each student's results
    for student in results {
        let mut record = vec![
            student.username.clone(),
            student.repo_url.clone(),
            student.workflow_run_timestamp.to_rfc3339(),
        ];

        // Add test scores
        for test_name in &test_names {
            let score = student
                .tests
                .get(test_name)
                .map(|t| t.points_awarded.to_string())
                .unwrap_or_else(|| "0".to_string());
            record.push(score);
        }

        // Add totals
        record.push(student.total_awarded.to_string());
        record.push(student.total_available.to_string());

        // Calculate percentage
        let percentage = if student.total_available > 0 {
            (student.total_awarded as f64 / student.total_available as f64) * 100.0
        } else {
            0.0
        };
        record.push(format!("{:.2}", percentage));

        wtr.write_record(&record)
            .context("Failed to write CSV record")?;
    }

    wtr.flush().context("Failed to flush CSV writer")?;

    Ok(filepath)
}

/// Export late grading results to CSV file
pub fn export_late_grading_to_csv(
    results: &[LateGradingResult],
    assignment_name: &str,
) -> Result<PathBuf> {
    if results.is_empty() {
        anyhow::bail!("No results to export");
    }

    // Generate filename with timestamp
    let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
    let filename = format!("results_late_{}_{}.csv", assignment_name, timestamp);
    let filepath = PathBuf::from(&filename);

    // Collect all unique test names (preserve order from first student)
    let test_names: Vec<String> = results
        .first()
        .map(|r| r.on_time_result.tests.keys().cloned().collect())
        .unwrap_or_default();

    // Build CSV headers
    let mut headers = vec![
        "student_username".to_string(),
        "student_repo_url".to_string(),
        "on_time_timestamp".to_string(),
        "late_timestamp".to_string(),
    ];

    // Add test names as headers (will show on-time scores)
    headers.extend(test_names.clone());

    // Add summary columns
    headers.extend_from_slice(&[
        "total_points_available".to_string(),
        "on_time_points".to_string(),
        "late_points".to_string(),
        "final_points".to_string(),
        "final_percentage".to_string(),
    ]);

    // Create CSV writer
    let mut wtr = csv::Writer::from_path(&filepath)
        .context("Failed to create CSV file")?;

    // Write headers
    wtr.write_record(&headers)
        .context("Failed to write CSV headers")?;

    // Write each student's results
    for result in results {
        let mut record = vec![
            result.username.clone(),
            result.repo_url.clone(),
            result.on_time_result.workflow_run_timestamp.to_rfc3339(),
            result.late_result.workflow_run_timestamp.to_rfc3339(),
        ];

        // Add test scores (from on-time submission)
        for test_name in &test_names {
            let score = result
                .on_time_result
                .tests
                .get(test_name)
                .map(|t| t.points_awarded.to_string())
                .unwrap_or_else(|| "0".to_string());
            record.push(score);
        }

        // Add summary data
        record.push(result.on_time_result.total_available.to_string());
        record.push(result.on_time_result.total_awarded.to_string());
        record.push(result.late_result.total_awarded.to_string());
        record.push(result.final_score.to_string());

        // Calculate final percentage
        let percentage = if result.on_time_result.total_available > 0 {
            (result.final_score as f64 / result.on_time_result.total_available as f64) * 100.0
        } else {
            0.0
        };
        record.push(format!("{:.2}", percentage));

        wtr.write_record(&record)
            .context("Failed to write CSV record")?;
    }

    wtr.flush().context("Failed to flush CSV writer")?;

    Ok(filepath)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{TestResult};
    use chrono::Utc;
    use indexmap::IndexMap;

    #[test]
    fn test_export_csv() {
        let mut tests = IndexMap::new();
        tests.insert(
            "test_1".to_string(),
            TestResult {
                _name: "test_1".to_string(),
                points_awarded: 5,
                _points_available: 5,
                _passed: true,
            },
        );
        tests.insert(
            "test_2".to_string(),
            TestResult {
                _name: "test_2".to_string(),
                points_awarded: 0,
                _points_available: 10,
                _passed: false,
            },
        );

        let results = vec![StudentResult {
            username: "student1".to_string(),
            repo_url: "https://github.com/org/repo".to_string(),
            workflow_run_timestamp: Utc::now(),
            tests,
            total_awarded: 5,
            total_available: 15,
        }];

        let filepath = export_to_csv(&results, "test_assignment").unwrap();
        assert!(filepath.exists());

        // Clean up
        std::fs::remove_file(filepath).ok();
    }
}
