# Future Improvements: Custom Autograding Results Collection

## Current Limitations

The current implementation relies on GitHub's public APIs to extract autograding results, which has several significant limitations:

1. **No Individual Test Score Granularity**: GitHub's Actions API only exposes pass/fail conclusions for job steps, not the actual points awarded. When GitHub Classroom uses partial credit or deducts points based on test output quality, this information is not available.

2. **Inefficient Log Parsing**: To get the total score, we must download potentially large job logs (1MB+) and parse them for point totals. This is slow, fragile, and doesn't provide test-level breakdown.

3. **No Historical Data**: We can only access results from the most recent workflow run(s). There's no easy way to track score improvements over time or view results from specific commits.

4. **API Rate Limits**: Fetching logs for 80+ students requires many API calls, which can hit GitHub's rate limits for large classes.

5. **No Real-time Updates**: Results are only available after the entire workflow completes, and we must poll the API to check for completion.

## Proposed Solution: Custom Autograding Results API

### Overview

Instead of reverse-engineering results from GitHub's APIs, we can create a custom results collection infrastructure by:

1. Forking/adapting GitHub Education's autograding actions
2. Adding POST requests to our own API endpoint with detailed test results
3. Building a lightweight backend to store and serve results
4. Creating a web dashboard and/or API for instructors to query results

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│ GitHub Classroom Assignment Repository                      │
│                                                             │
│  ┌──────────────────────────────────────────────────────┐   │
│  │ .github/workflows/classroom.yml                      │   │
│  │                                                      │   │
│  │  - uses: custom-org/autograding-command-grader@v1    │   │
│  │    (Modified to collect detailed results)            │   │
│  │                                                      │   │
│  │  - uses: custom-org/autograding-reporter@v1          │   │
│  │    (Modified to POST to our API)                     │   │
│  └──────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
                            │
                            │ HTTPS POST with test results
                            ▼
┌─────────────────────────────────────────────────────────────┐
│ Custom Autograding Results API                              │
│                                                             │
│  POST /api/v1/results                                       │
│  - Receives detailed test results from workflows            │
│  - Validates HMAC signature for security                    │
│  - Stores in database                                       │
│                                                             │
│  GET /api/v1/assignments/{id}/results                       │
│  - Returns all results for an assignment                    │
│  - Supports filtering by student, date range, etc.          │
│                                                             │
│  Database: PostgreSQL or SQLite                             │
│  - assignment_id, student_username, commit_sha              │
│  - timestamp, total_score, test_results (JSON)              │
│  - workflow_run_id for traceability                         │
└─────────────────────────────────────────────────────────────┘
                            │
                            │ Query API
                            ▼
┌─────────────────────────────────────────────────────────────┐
│ CLI Tool / Web Dashboard                                    │
│                                                             │
│  - Fetch latest results for all students                    │
│  - Export to CSV with accurate individual test scores       │
│  - View historical trends                                   │
│  - Compare results across commits                           │
└─────────────────────────────────────────────────────────────┘
```

### Implementation Details

#### 1. Modified Autograding Actions

Fork and modify GitHub's actions to POST results:

**autograding-command-grader** (`action.yml`):
```yaml
name: 'Autograding Command Grader (Custom)'
description: 'Run tests and collect detailed results'
inputs:
  test-name:
    required: true
  command:
    required: true
  timeout:
    required: true
  max-score:
    required: true
  results-api-url:
    description: 'URL to POST test results'
    required: false
    default: ''
  results-api-secret:
    description: 'HMAC secret for authenticating with results API'
    required: false
    default: ''
runs:
  using: 'node20'
  main: 'dist/index.js'
```

**Modified JavaScript** (`src/main.ts`):
```typescript
import * as core from '@actions/core';
import * as exec from '@actions/exec';
import crypto from 'crypto';

async function run() {
  const testName = core.getInput('test-name');
  const command = core.getInput('command');
  const timeout = parseInt(core.getInput('timeout'));
  const maxScore = parseInt(core.getInput('max-score'));
  const apiUrl = core.getInput('results-api-url');
  const apiSecret = core.getInput('results-api-secret');

  let exitCode = 0;
  let stdout = '';
  let stderr = '';

  try {
    exitCode = await exec.exec(command, [], {
      timeout: timeout * 1000,
      listeners: {
        stdout: (data) => { stdout += data.toString(); },
        stderr: (data) => { stderr += data.toString(); }
      }
    });
  } catch (error) {
    exitCode = 1;
  }

  const passed = exitCode === 0;
  const pointsAwarded = passed ? maxScore : 0;

  // POST results to custom API if configured
  if (apiUrl && apiSecret) {
    await postResults({
      testName,
      pointsAwarded,
      maxScore,
      passed,
      stdout,
      stderr,
      exitCode
    }, apiUrl, apiSecret);
  }

  // Set outputs for reporter
  core.setOutput('points', pointsAwarded.toString());
  core.setOutput('passed', passed.toString());
}

async function postResults(result: any, apiUrl: string, secret: string) {
  const payload = JSON.stringify(result);
  const signature = crypto
    .createHmac('sha256', secret)
    .update(payload)
    .digest('hex');

  try {
    await fetch(apiUrl, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'X-Hub-Signature-256': `sha256=${signature}`
      },
      body: payload
    });
  } catch (error) {
    core.warning(`Failed to post results to API: ${error}`);
  }
}

run();
```

**autograding-reporter** (similar modifications to collect all test results and POST final summary)

#### 2. Updated Workflow Configuration

**`.github/workflows/classroom.yml`**:
```yaml
name: Autograding Tests
on:
  - push
  - repository_dispatch

jobs:
  run-autograding-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: test_rustfmt_passes
        uses: custom-org/autograding-command-grader@v1
        with:
          test-name: test_rustfmt_passes
          command: cargo test test_rustfmt_passes
          timeout: 10
          max-score: 2
          results-api-url: ${{ secrets.AUTOGRADING_RESULTS_API_URL }}
          results-api-secret: ${{ secrets.AUTOGRADING_RESULTS_API_SECRET }}

      # ... more tests ...

      - name: Autograding Reporter
        uses: custom-org/autograding-reporter@v1
        env:
          RESULTS_API_URL: ${{ secrets.AUTOGRADING_RESULTS_API_URL }}
          RESULTS_API_SECRET: ${{ secrets.AUTOGRADING_RESULTS_API_SECRET }}
          ASSIGNMENT_ID: ${{ github.repository }}
          STUDENT_USERNAME: ${{ github.actor }}
          COMMIT_SHA: ${{ github.sha }}
          WORKFLOW_RUN_ID: ${{ github.run_id }}
```

#### 3. Results API Backend

**Stack Options**:
- **Lightweight**: Rust (Axum/Actix), Go (Gin/Echo), or Node.js (Express/Fastify)
- **Database**: PostgreSQL for production, SQLite for small deployments
- **Deployment**: Docker container on any cloud provider (AWS ECS, Google Cloud Run, Fly.io)

**Schema** (`schema.sql`):
```sql
CREATE TABLE assignments (
    id SERIAL PRIMARY KEY,
    github_org VARCHAR(255) NOT NULL,
    github_repo VARCHAR(255) NOT NULL,
    assignment_name VARCHAR(255) NOT NULL,
    created_at TIMESTAMP DEFAULT NOW(),
    UNIQUE(github_org, github_repo)
);

CREATE TABLE test_runs (
    id SERIAL PRIMARY KEY,
    assignment_id INTEGER REFERENCES assignments(id),
    student_username VARCHAR(255) NOT NULL,
    commit_sha VARCHAR(40) NOT NULL,
    workflow_run_id BIGINT NOT NULL,
    total_score INTEGER NOT NULL,
    total_possible INTEGER NOT NULL,
    submitted_at TIMESTAMP DEFAULT NOW(),
    INDEX(assignment_id, student_username),
    INDEX(workflow_run_id)
);

CREATE TABLE test_results (
    id SERIAL PRIMARY KEY,
    test_run_id INTEGER REFERENCES test_runs(id) ON DELETE CASCADE,
    test_name VARCHAR(255) NOT NULL,
    points_awarded INTEGER NOT NULL,
    max_score INTEGER NOT NULL,
    passed BOOLEAN NOT NULL,
    stdout TEXT,
    stderr TEXT,
    exit_code INTEGER
);
```

**API Endpoints**:

```rust
// POST /api/v1/results
// Called by autograding-reporter action after all tests complete
#[derive(Deserialize)]
struct SubmitResultsRequest {
    assignment_id: String,    // e.g., "org/repo"
    student_username: String,
    commit_sha: String,
    workflow_run_id: u64,
    tests: Vec<TestResult>,
}

#[derive(Deserialize)]
struct TestResult {
    test_name: String,
    points_awarded: u32,
    max_score: u32,
    passed: bool,
    stdout: Option<String>,
    stderr: Option<String>,
    exit_code: Option<i32>,
}

// Verify HMAC signature, then store in database
async fn submit_results(
    req: HttpRequest,
    body: Bytes,
    data: web::Data<AppState>,
) -> Result<HttpResponse> {
    // Verify signature
    verify_hmac_signature(&req, &body, &data.api_secret)?;

    // Parse JSON
    let request: SubmitResultsRequest = serde_json::from_slice(&body)?;

    // Store in database
    let test_run_id = data.db.insert_test_run(&request).await?;

    Ok(HttpResponse::Ok().json(json!({
        "status": "success",
        "test_run_id": test_run_id
    })))
}

// GET /api/v1/assignments/:org/:repo/results
// Fetch all results for an assignment
async fn get_assignment_results(
    path: web::Path<(String, String)>,
    query: web::Query<QueryParams>,
    data: web::Data<AppState>,
) -> Result<HttpResponse> {
    let (org, repo) = path.into_inner();

    let results = data.db.get_results(
        &org,
        &repo,
        query.student.as_deref(),
        query.since.as_ref(),
        query.until.as_ref(),
    ).await?;

    Ok(HttpResponse::Ok().json(results))
}
```

#### 4. Updated CLI Tool

Replace the current workflow log parsing with simple API calls:

```rust
// src/api/results_api.rs
pub struct ResultsApiClient {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
}

impl ResultsApiClient {
    pub async fn get_assignment_results(
        &self,
        org: &str,
        repo: &str,
    ) -> Result<Vec<StudentResult>> {
        let url = format!("{}/api/v1/assignments/{}/{}/results",
            self.base_url, org, repo);

        let response = self.client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await?;

        let results: Vec<StudentResult> = response.json().await?;
        Ok(results)
    }
}
```

### Benefits of This Approach

1. **Accurate Individual Test Scores**: Full test-level detail with actual points awarded
2. **Fast**: No need to download and parse logs; simple JSON API calls
3. **Historical Data**: Can query results across multiple commits and submissions
4. **Real-time**: Results available immediately as tests complete
5. **Scalable**: Database-backed, can handle large classes efficiently
6. **Flexible**: Can add custom metadata, tags, grading rubrics, etc.
7. **Analytics**: Easy to build dashboards showing trends, common failures, etc.

### Security Considerations

1. **HMAC Signatures**: Use shared secret to verify requests come from legitimate GitHub workflows
2. **Repository Secrets**: Store API URL and secret as GitHub repository/organization secrets
3. **Rate Limiting**: Implement rate limiting on API to prevent abuse
4. **Access Control**: Require API keys for reading results; restrict to instructors
5. **Data Privacy**: Consider FERPA compliance; encrypt sensitive data at rest

### Migration Path

1. **Phase 1**: Deploy API backend and database
2. **Phase 2**: Fork and publish custom autograding actions
3. **Phase 3**: Update one assignment's workflow as proof-of-concept
4. **Phase 4**: Migrate all assignments to use custom actions
5. **Phase 5**: Deprecate log-parsing approach once all assignments migrated

### Cost Estimates

- **Hosting**: ~$5-20/month (Fly.io, Railway, Google Cloud Run)
- **Database**: Included in hosting or ~$5-10/month for managed PostgreSQL
- **Development Time**: ~2-3 weeks for MVP (API + actions + migration)

### Alternative: GitHub App + Webhooks

Instead of POSTing from workflows, could create a GitHub App that:
1. Listens to workflow completion webhooks
2. Fetches check runs and job logs
3. Parses and stores results automatically

**Pros**: No workflow modifications needed
**Cons**: Still limited by API granularity; can't get true individual test scores

### Conclusion

The custom API approach provides the most accurate, flexible, and scalable solution for collecting autograding results. While it requires initial infrastructure setup, the long-term benefits (accurate data, historical tracking, analytics) far outweigh the costs, especially for courses taught repeatedly over multiple semesters.
