# GitHub Classroom Autograder Fetcher

A command-line tool with an interactive TUI for fetching and exporting GitHub Classroom autograder results to CSV format.

## Features

- **Interactive TUI**: Navigate through classrooms and assignments with an intuitive terminal interface
- **Flexible Date Filtering**: Download latest results or results from the first run after a specific deadline
- **Individual Test Results**: Export detailed test-by-test scores for each student
- **Dynamic CSV Format**: Test names as column headers, making it easy to analyze in spreadsheet software
- **Parallel Processing**: Efficient fetching of results for multiple students
- **Statistics**: View average and median scores for assignments

## Prerequisites

- Rust 1.70 or later
- A GitHub Personal Access Token with the following scopes:
  - `read:org` (for accessing GitHub Classroom classrooms)
  - `repo` (for accessing student repositories and Actions data)

## Installation

1. Clone this repository:
```bash
git clone <repository-url>
cd gh_autograder_fetcher
```

2. Create a `.env` file with your GitHub token:
```bash
cp .env.example .env
# Edit .env and add your GitHub token
```

3. Build the project:
```bash
cargo build --release
```

## Usage

Run the application:
```bash
cargo run --release
```

Or run the compiled binary:
```bash
./target/release/gh_autograder_fetcher
```

### Navigation

The TUI interface guides you through the following steps:

1. **Select Classroom**: Choose from your available GitHub Classroom classrooms
2. **Select Assignment**: Pick an assignment from the selected classroom
3. **Choose Option**:
   - **Download Latest Results**: Fetches the most recent autograder run for all students
   - **Download Results After Deadline**: Fetches the first autograder run after a specified deadline
4. **Enter Deadline** (if applicable): Input date and time in the format `YYYY-MM-DD HH:MM:SS`
5. **View Results**: See statistics and the location of the exported CSV file

### Keyboard Shortcuts

- `↑/↓`: Navigate through lists
- `Enter`: Select/Confirm
- `Esc`: Go back to previous screen
- `Tab`: Switch between input fields (on deadline entry screen)
- `q`: Quit the application

## CSV Export Format

The exported CSV file includes:

- **Fixed Columns**:
  - `student_username`: GitHub username of the student
  - `student_repo_url`: URL to the student's assignment repository
  - `workflow_run_timestamp`: Timestamp of the autograder workflow run

- **Dynamic Test Columns**: One column for each test in the assignment, showing points earned

- **Summary Columns**:
  - `total_points_awarded`: Total points earned by the student
  - `total_points_available`: Maximum possible points
  - `percentage`: Score as a percentage

### Example CSV Output

```csv
student_username,student_repo_url,workflow_run_timestamp,test_clippy_passes,test_rustfmt_passes,q1::tests::test_series_creation,total_points_awarded,total_points_available,percentage
student1,https://github.com/cdsds210/assignment1-student1,2025-01-15T10:30:00Z,2,2,1,5,10,50.00
student2,https://github.com/cdsds210/assignment1-student2,2025-01-15T11:45:00Z,2,2,1,5,10,50.00
```

## How It Works

1. **Fetch Classrooms**: Uses the GitHub Classroom API to list all classrooms you have access to
2. **Fetch Assignments**: Lists assignments for the selected classroom
3. **Parse Test Definitions**: Fetches the workflow YAML file from the assignment's starter repository to extract test names and max scores
4. **Fetch Workflow Runs**: For each student, queries the GitHub Actions API to find the target workflow run
5. **Extract Test Results**: Matches workflow job steps to test definitions and calculates points based on success/failure
6. **Export to CSV**: Generates a CSV file with dynamic columns for each test

## GitHub Classroom Workflow Requirements

This tool expects your GitHub Classroom assignments to use the standard autograding workflow with:

- Workflow file located at `.github/workflows/classroom.yml`
- Job named `run-autograding-tests`
- Test steps using `classroom-resources/autograding-command-grader@v1`
- Each test step having:
  - A unique `id`
  - A `name` field
  - `with.test-name` and `with.max-score` parameters

Example workflow step:
```yaml
- name: "test_clippy_passes"
  id: "test-clippy-passes"
  uses: "classroom-resources/autograding-command-grader@v1"
  with:
    test-name: "test_clippy_passes"
    command: "cargo test test_clippy_passes"
    timeout: 30
    max-score: 2
```

## Troubleshooting

### "No classrooms found"
- Verify your GitHub token has the `read:org` scope
- Ensure you're a member of at least one GitHub Classroom organization

### "Failed to fetch workflow file from starter repository"
- Ensure the assignment has a starter code repository configured
- Verify the workflow file exists at `.github/workflows/classroom.yml`
- Check that your token has the `repo` scope

### "No completed workflow run found"
- Students may not have accepted the assignment yet
- Workflow runs may still be in progress
- The deadline filter may be excluding all runs

## Development

### Running Tests

```bash
cargo test
```

### Project Structure

```
src/
├── main.rs              # Application entry point
├── config.rs            # Configuration loading
├── api/
│   ├── classroom.rs     # GitHub Classroom API client
│   └── github.rs        # GitHub API client
├── models/
│   └── mod.rs           # Data models
├── parser/
│   └── mod.rs           # Workflow YAML parser
├── fetcher.rs           # Core fetching logic
├── export.rs            # CSV export functionality
└── ui/
    ├── app.rs           # TUI application logic
    ├── render.rs        # UI rendering
    └── state.rs         # Application state
```

## License

MIT

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.
