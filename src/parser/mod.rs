use crate::models::{TestDefinition, WorkflowFile};
use anyhow::{Context, Result};

/// Parse workflow YAML content and extract test definitions
pub fn parse_workflow(yaml_content: &str) -> Result<Vec<TestDefinition>> {
    let workflow: WorkflowFile =
        serde_yaml::from_str(yaml_content).context("Failed to parse workflow YAML")?;

    extract_test_definitions(&workflow)
}

/// Extract test definitions from a parsed workflow
fn extract_test_definitions(workflow: &WorkflowFile) -> Result<Vec<TestDefinition>> {
    let job = workflow
        .jobs
        .get("run-autograding-tests")
        .context("Job 'run-autograding-tests' not found in workflow")?;

    let mut tests = Vec::new();

    for step in &job.steps {
        // Only process steps that use autograding-command-grader
        let uses_autograder = step
            .uses
            .as_ref()
            .map(|u| u.contains("autograding-command-grader"))
            .unwrap_or(false);

        if !uses_autograder {
            continue;
        }

        if let (Some(id), Some(with)) = (&step.id, &step.with) {
            if let (Some(_test_name), Some(max_score)) = (&with.test_name, &with.max_score) {
                tests.push(TestDefinition {
                    name: step.name.clone(),
                    id: id.clone(),
                    max_score: *max_score,
                });
            }
        }
    }

    if tests.is_empty() {
        anyhow::bail!("No autograding tests found in workflow");
    }

    Ok(tests)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_workflow() {
        let yaml = r#"
name: Autograding Tests
on: [repository_dispatch]
jobs:
  run-autograding-tests:
    runs-on: ubuntu-latest
    steps:
      - name: Checkout code
        uses: actions/checkout@v4
      - name: "test_1"
        id: "test-1"
        uses: "classroom-resources/autograding-command-grader@v1"
        with:
          test-name: "test_1"
          command: "cargo test test_1"
          timeout: 10
          max-score: 5
      - name: "test_2"
        id: "test-2"
        uses: "classroom-resources/autograding-command-grader@v1"
        with:
          test-name: "test_2"
          command: "cargo test test_2"
          timeout: 10
          max-score: 10
"#;

        let tests = parse_workflow(yaml).unwrap();
        assert_eq!(tests.len(), 2);
        assert_eq!(tests[0].name, "test_1");
        assert_eq!(tests[0].max_score, 5);
        assert_eq!(tests[1].name, "test_2");
        assert_eq!(tests[1].max_score, 10);
    }
}
