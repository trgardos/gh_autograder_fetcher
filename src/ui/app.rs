use crate::api::{ClassroomClient, GitHubClient};
use crate::export;
use crate::fetcher;
use crate::models::{Assignment, Classroom, ResultStats};
use crate::parser;
use crate::ui::render::render_ui;
use crate::ui::state::{AppState, DeadlineField, LateGradingField, FetchProgress};
use anyhow::{Context, Result};
use chrono::{NaiveDate, NaiveDateTime, NaiveTime, Utc};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;

pub struct App {
    classroom_client: ClassroomClient,
    github_client: GitHubClient,
    state: AppState,
    spinner_frame: usize,
    background_task: Option<tokio::task::JoinHandle<Result<AppState>>>,
    progress_rx: Option<tokio::sync::mpsc::UnboundedReceiver<FetchProgress>>,
}

impl App {
    pub fn new(classroom_client: ClassroomClient, github_client: GitHubClient) -> Self {
        Self {
            classroom_client,
            github_client,
            state: AppState::LoadingClassrooms,
            spinner_frame: 0,
            background_task: None,
            progress_rx: None,
        }
    }

    fn spinner_char(&self) -> char {
        const SPINNER_FRAMES: &[char] = &['|', '/', '-', '\\'];
        SPINNER_FRAMES[self.spinner_frame % SPINNER_FRAMES.len()]
    }

    pub async fn run(&mut self) -> Result<()> {
        // Setup terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        // Load initial classrooms
        self.load_classrooms().await?;

        // Main event loop
        let result = self.event_loop(&mut terminal).await;

        // Restore terminal
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;

        result
    }

    async fn event_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<()> {
        loop {
            // Update spinner for progress indication
            self.spinner_frame = self.spinner_frame.wrapping_add(1);
            let spinner = self.spinner_char();

            // Always redraw the UI
            terminal.draw(|f| render_ui(f, &self.state, spinner))?;

            // Check for progress updates
            if let Some(rx) = &mut self.progress_rx {
                while let Ok(progress) = rx.try_recv() {
                    // Update the progress in the current state
                    match &mut self.state {
                        AppState::FetchingResults { progress: p, .. } => {
                            *p = progress;
                        }
                        AppState::FetchingLateResults { progress: p, .. } => {
                            *p = progress;
                        }
                        _ => {}
                    }
                }
            }

            // Check if background task has completed
            if let Some(task) = &mut self.background_task {
                if task.is_finished() {
                    let task = self.background_task.take().unwrap();
                    self.progress_rx = None; // Clear progress channel
                    match task.await {
                        Ok(Ok(new_state)) => {
                            self.state = new_state;
                        }
                        Ok(Err(e)) => {
                            self.state = AppState::Error {
                                message: format!("Failed to fetch results: {}", e),
                            };
                        }
                        Err(e) => {
                            self.state = AppState::Error {
                                message: format!("Background task failed: {}", e),
                            };
                        }
                    }
                }
            }

            // Check for keyboard events with a short timeout
            if event::poll(std::time::Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    if self.handle_key_event(key).await? {
                        break; // User quit
                    }
                }
            }

            // Small yield to allow other async tasks to run
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        Ok(())
    }

    async fn load_classrooms(&mut self) -> Result<()> {
        match self.classroom_client.list_classrooms().await {
            Ok(classrooms) => {
                if classrooms.is_empty() {
                    self.state = AppState::Error {
                        message: "No classrooms found. Please check your GitHub token permissions."
                            .to_string(),
                    };
                } else {
                    self.state = AppState::ClassroomSelection {
                        classrooms,
                        selected_index: 0,
                    };
                }
            }
            Err(e) => {
                self.state = AppState::Error {
                    message: format!("Failed to load classrooms: {}", e),
                };
            }
        }
        Ok(())
    }

    async fn handle_key_event(&mut self, key: KeyEvent) -> Result<bool> {
        // Clone state to avoid borrowing issues
        let current_state = std::mem::replace(&mut self.state, AppState::LoadingClassrooms);

        match current_state {
            AppState::ClassroomSelection {
                classrooms,
                mut selected_index,
            } => {
                match key.code {
                    KeyCode::Char('q') => return Ok(true), // Quit
                    KeyCode::Up => {
                        if selected_index > 0 {
                            selected_index -= 1;
                        }
                        self.state = AppState::ClassroomSelection {
                            classrooms,
                            selected_index,
                        };
                    }
                    KeyCode::Down => {
                        if selected_index < classrooms.len().saturating_sub(1) {
                            selected_index += 1;
                        }
                        self.state = AppState::ClassroomSelection {
                            classrooms,
                            selected_index,
                        };
                    }
                    KeyCode::Enter => {
                        let classroom = classrooms[selected_index].clone();
                        self.state = AppState::LoadingAssignments {
                            classroom: classroom.clone(),
                        };

                        // Load assignments
                        match self.classroom_client.list_assignments(classroom.id).await {
                            Ok(assignments) => {
                                self.state = AppState::AssignmentSelection {
                                    classroom,
                                    assignments,
                                    selected_index: 0,
                                };
                            }
                            Err(e) => {
                                self.state = AppState::Error {
                                    message: format!("Failed to load assignments: {}", e),
                                };
                            }
                        }
                    }
                    _ => {
                        self.state = AppState::ClassroomSelection {
                            classrooms,
                            selected_index,
                        };
                    }
                }
            }
            AppState::AssignmentSelection {
                classroom,
                assignments,
                mut selected_index,
            } => {
                match key.code {
                    KeyCode::Char('q') => return Ok(true),
                    KeyCode::Esc => {
                        // Go back to classroom selection
                        self.load_classrooms().await?;
                    }
                    KeyCode::Up => {
                        if selected_index > 0 {
                            selected_index -= 1;
                        }
                        self.state = AppState::AssignmentSelection {
                            classroom,
                            assignments,
                            selected_index,
                        };
                    }
                    KeyCode::Down => {
                        if selected_index < assignments.len().saturating_sub(1) {
                            selected_index += 1;
                        }
                        self.state = AppState::AssignmentSelection {
                            classroom,
                            assignments,
                            selected_index,
                        };
                    }
                    KeyCode::Enter => {
                        let assignment = assignments[selected_index].clone();
                        self.state = AppState::AssignmentOptions {
                            classroom,
                            assignment,
                            selected_index: 0,
                        };
                    }
                    _ => {
                        self.state = AppState::AssignmentSelection {
                            classroom,
                            assignments,
                            selected_index,
                        };
                    }
                }
            }
            AppState::AssignmentOptions {
                classroom,
                assignment,
                mut selected_index,
            } => {
                match key.code {
                    KeyCode::Char('q') => return Ok(true),
                    KeyCode::Esc => {
                        // Go back to assignment selection
                        match self.classroom_client.list_assignments(classroom.id).await {
                            Ok(assignments) => {
                                self.state = AppState::AssignmentSelection {
                                    classroom,
                                    assignments,
                                    selected_index: 0,
                                };
                            }
                            Err(e) => {
                                self.state = AppState::Error {
                                    message: format!("Failed to load assignments: {}", e),
                                };
                            }
                        }
                    }
                    KeyCode::Up => {
                        if selected_index > 0 {
                            selected_index -= 1;
                        }
                        self.state = AppState::AssignmentOptions {
                            classroom,
                            assignment,
                            selected_index,
                        };
                    }
                    KeyCode::Down => {
                        if selected_index < 2 {
                            // 0: Latest, 1: After deadline, 2: Late Grading
                            selected_index += 1;
                        }
                        self.state = AppState::AssignmentOptions {
                            classroom,
                            assignment,
                            selected_index,
                        };
                    }
                    KeyCode::Enter => {
                        match selected_index {
                            0 => {
                                // Download latest results - spawn as background task
                                self.spawn_fetch_results(classroom, assignment, None);
                            }
                            1 => {
                                // Download results after deadline
                                self.state = AppState::DeadlineInput {
                                    classroom,
                                    assignment,
                                    date_input: String::new(),
                                    time_input: String::new(),
                                    focused_field: DeadlineField::Date,
                                };
                            }
                            2 => {
                                // Late Grading Mode
                                self.state = AppState::GradingModeSelection {
                                    classroom,
                                    assignment,
                                    selected_index: 0,
                                };
                            }
                            _ => {}
                        }
                    }
                    _ => {
                        self.state = AppState::AssignmentOptions {
                            classroom,
                            assignment,
                            selected_index,
                        };
                    }
                }
            }
            AppState::DeadlineInput {
                classroom,
                assignment,
                mut date_input,
                mut time_input,
                mut focused_field,
            } => {
                match key.code {
                    KeyCode::Char('q') => return Ok(true),
                    KeyCode::Esc => {
                        // Go back to options
                        self.state = AppState::AssignmentOptions {
                            classroom,
                            assignment,
                            selected_index: 0,
                        };
                    }
                    KeyCode::Tab => {
                        // Switch between date and time fields
                        focused_field = match focused_field {
                            DeadlineField::Date => DeadlineField::Time,
                            DeadlineField::Time => DeadlineField::Date,
                        };
                        self.state = AppState::DeadlineInput {
                            classroom,
                            assignment,
                            date_input,
                            time_input,
                            focused_field,
                        };
                    }
                    KeyCode::Char(c) => {
                        // Add character to focused field
                        match focused_field {
                            DeadlineField::Date => {
                                if date_input.len() < 10 {
                                    date_input.push(c);
                                }
                            }
                            DeadlineField::Time => {
                                if time_input.len() < 6 {
                                    time_input.push(c);
                                }
                            }
                        }
                        self.state = AppState::DeadlineInput {
                            classroom,
                            assignment,
                            date_input,
                            time_input,
                            focused_field,
                        };
                    }
                    KeyCode::Backspace => {
                        // Remove character from focused field
                        match focused_field {
                            DeadlineField::Date => {
                                date_input.pop();
                            }
                            DeadlineField::Time => {
                                time_input.pop();
                            }
                        }
                        self.state = AppState::DeadlineInput {
                            classroom,
                            assignment,
                            date_input,
                            time_input,
                            focused_field,
                        };
                    }
                    KeyCode::Enter => {
                        // Parse and validate deadline
                        match parse_deadline(&date_input, &time_input) {
                            Ok(deadline) => {
                                self.spawn_fetch_results(classroom, assignment, Some(deadline));
                            }
                            Err(e) => {
                                self.state = AppState::Error {
                                    message: format!("Invalid deadline: {}", e),
                                };
                            }
                        }
                    }
                    _ => {
                        self.state = AppState::DeadlineInput {
                            classroom,
                            assignment,
                            date_input,
                            time_input,
                            focused_field,
                        };
                    }
                }
            }
            AppState::GradingModeSelection {
                classroom,
                assignment,
                mut selected_index,
            } => {
                match key.code {
                    KeyCode::Char('q') => return Ok(true),
                    KeyCode::Esc => {
                        // Go back to assignment options
                        self.state = AppState::AssignmentOptions {
                            classroom,
                            assignment,
                            selected_index: 2,
                        };
                    }
                    KeyCode::Up => {
                        if selected_index > 0 {
                            selected_index -= 1;
                        }
                        self.state = AppState::GradingModeSelection {
                            classroom,
                            assignment,
                            selected_index,
                        };
                    }
                    KeyCode::Down => {
                        if selected_index < 1 {
                            selected_index += 1;
                        }
                        self.state = AppState::GradingModeSelection {
                            classroom,
                            assignment,
                            selected_index,
                        };
                    }
                    KeyCode::Enter => {
                        match selected_index {
                            0 => {
                                // Regular grading - single deadline
                                self.state = AppState::DeadlineInput {
                                    classroom,
                                    assignment,
                                    date_input: String::new(),
                                    time_input: String::new(),
                                    focused_field: DeadlineField::Date,
                                };
                            }
                            1 => {
                                // Late grading - on-time + late deadlines
                                self.state = AppState::LateGradingInput {
                                    classroom,
                                    assignment,
                                    on_time_date: String::new(),
                                    on_time_time: String::new(),
                                    late_date: String::new(),
                                    late_time: String::new(),
                                    penalty_input: "20".to_string(),
                                    focused_field: LateGradingField::OnTimeDate,
                                };
                            }
                            _ => {
                                self.state = AppState::GradingModeSelection {
                                    classroom,
                                    assignment,
                                    selected_index,
                                };
                            }
                        }
                    }
                    _ => {
                        self.state = AppState::GradingModeSelection {
                            classroom,
                            assignment,
                            selected_index,
                        };
                    }
                }
            }
            AppState::LateGradingInput {
                classroom,
                assignment,
                mut on_time_date,
                mut on_time_time,
                mut late_date,
                mut late_time,
                mut penalty_input,
                mut focused_field,
            } => {
                match key.code {
                    KeyCode::Char('q') => return Ok(true),
                    KeyCode::Esc => {
                        // Go back to grading mode selection
                        self.state = AppState::GradingModeSelection {
                            classroom,
                            assignment,
                            selected_index: 1,
                        };
                    }
                    KeyCode::Tab => {
                        // Next field
                        focused_field = match focused_field {
                            LateGradingField::OnTimeDate => LateGradingField::OnTimeTime,
                            LateGradingField::OnTimeTime => LateGradingField::LateDate,
                            LateGradingField::LateDate => LateGradingField::LateTime,
                            LateGradingField::LateTime => LateGradingField::Penalty,
                            LateGradingField::Penalty => LateGradingField::OnTimeDate,
                        };
                        self.state = AppState::LateGradingInput {
                            classroom,
                            assignment,
                            on_time_date,
                            on_time_time,
                            late_date,
                            late_time,
                            penalty_input,
                            focused_field,
                        };
                    }
                    KeyCode::BackTab => {
                        // Previous field
                        focused_field = match focused_field {
                            LateGradingField::OnTimeDate => LateGradingField::Penalty,
                            LateGradingField::OnTimeTime => LateGradingField::OnTimeDate,
                            LateGradingField::LateDate => LateGradingField::OnTimeTime,
                            LateGradingField::LateTime => LateGradingField::LateDate,
                            LateGradingField::Penalty => LateGradingField::LateTime,
                        };
                        self.state = AppState::LateGradingInput {
                            classroom,
                            assignment,
                            on_time_date,
                            on_time_time,
                            late_date,
                            late_time,
                            penalty_input,
                            focused_field,
                        };
                    }
                    KeyCode::Char(c) => {
                        // Add character to focused field
                        match focused_field {
                            LateGradingField::OnTimeDate => {
                                if on_time_date.len() < 10 {
                                    on_time_date.push(c);
                                }
                            }
                            LateGradingField::OnTimeTime => {
                                if on_time_time.len() < 5 {
                                    on_time_time.push(c);
                                }
                            }
                            LateGradingField::LateDate => {
                                if late_date.len() < 10 {
                                    late_date.push(c);
                                }
                            }
                            LateGradingField::LateTime => {
                                if late_time.len() < 5 {
                                    late_time.push(c);
                                }
                            }
                            LateGradingField::Penalty => {
                                if penalty_input.len() < 5 {
                                    penalty_input.push(c);
                                }
                            }
                        }
                        self.state = AppState::LateGradingInput {
                            classroom,
                            assignment,
                            on_time_date,
                            on_time_time,
                            late_date,
                            late_time,
                            penalty_input,
                            focused_field,
                        };
                    }
                    KeyCode::Backspace => {
                        // Remove character from focused field
                        match focused_field {
                            LateGradingField::OnTimeDate => {
                                on_time_date.pop();
                            }
                            LateGradingField::OnTimeTime => {
                                on_time_time.pop();
                            }
                            LateGradingField::LateDate => {
                                late_date.pop();
                            }
                            LateGradingField::LateTime => {
                                late_time.pop();
                            }
                            LateGradingField::Penalty => {
                                penalty_input.pop();
                            }
                        }
                        self.state = AppState::LateGradingInput {
                            classroom,
                            assignment,
                            on_time_date,
                            on_time_time,
                            late_date,
                            late_time,
                            penalty_input,
                            focused_field,
                        };
                    }
                    KeyCode::Enter => {
                        // Parse and validate inputs
                        let on_time_deadline = match (
                            NaiveDate::parse_from_str(&on_time_date, "%Y-%m-%d"),
                            NaiveTime::parse_from_str(&on_time_time, "%H:%M"),
                        ) {
                            (Ok(date), Ok(time)) => {
                                chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
                                    date.and_time(time),
                                    chrono::Utc,
                                )
                            }
                            _ => {
                                self.state = AppState::Error {
                                    message: "Invalid on-time deadline format. Use YYYY-MM-DD and HH:MM"
                                        .to_string(),
                                };
                                return Ok(false);
                            }
                        };

                        let late_deadline = match (
                            NaiveDate::parse_from_str(&late_date, "%Y-%m-%d"),
                            NaiveTime::parse_from_str(&late_time, "%H:%M"),
                        ) {
                            (Ok(date), Ok(time)) => {
                                chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
                                    date.and_time(time),
                                    chrono::Utc,
                                )
                            }
                            _ => {
                                self.state = AppState::Error {
                                    message: "Invalid late deadline format. Use YYYY-MM-DD and HH:MM"
                                        .to_string(),
                                };
                                return Ok(false);
                            }
                        };

                        let late_penalty = match penalty_input.parse::<f64>() {
                            Ok(p) if p >= 0.0 && p <= 100.0 => p / 100.0,
                            _ => {
                                self.state = AppState::Error {
                                    message: "Invalid penalty percentage. Use 0-100".to_string(),
                                };
                                return Ok(false);
                            }
                        };

                        // Start fetching late results - spawn as background task
                        self.spawn_fetch_late_results(
                            classroom,
                            assignment,
                            on_time_deadline,
                            late_deadline,
                            late_penalty,
                        );
                    }
                    _ => {
                        self.state = AppState::LateGradingInput {
                            classroom,
                            assignment,
                            on_time_date,
                            on_time_time,
                            late_date,
                            late_time,
                            penalty_input,
                            focused_field,
                        };
                    }
                }
            }
            AppState::ResultsComplete { classroom, assignment, stats, csv_filename } => {
                match key.code {
                    KeyCode::Char('q') => return Ok(true),
                    KeyCode::Enter | KeyCode::Esc => {
                        // Go back to classroom selection
                        self.load_classrooms().await?;
                    }
                    _ => {
                        self.state = AppState::ResultsComplete {
                            classroom,
                            assignment,
                            stats,
                            csv_filename,
                        };
                    }
                }
            }
            AppState::Error { message } => {
                match key.code {
                    KeyCode::Char('q') => return Ok(true),
                    KeyCode::Enter | KeyCode::Esc => {
                        // Go back to classroom selection
                        self.load_classrooms().await?;
                    }
                    _ => {
                        self.state = AppState::Error { message };
                    }
                }
            }
            state => {
                // For other states (LoadingClassrooms, LoadingAssignments, FetchingResults),
                // just restore the state and ignore input
                self.state = state;
            }
        }

        Ok(false)
    }

    fn spawn_fetch_results(
        &mut self,
        classroom: Classroom,
        assignment: Assignment,
        deadline: Option<chrono::DateTime<Utc>>,
    ) {
        // Create progress channel
        let (progress_tx, progress_rx) = tokio::sync::mpsc::unbounded_channel();
        self.progress_rx = Some(progress_rx);

        // Set initial fetching state
        let progress = FetchProgress::new(0);
        self.state = AppState::FetchingResults {
            _classroom: classroom.clone(),
            assignment: assignment.clone(),
            _deadline: deadline,
            progress,
        };

        // Clone clients for the background task
        let classroom_client = self.classroom_client.clone();
        let github_client = self.github_client.clone();

        // Spawn background task
        let task = tokio::spawn(async move {
            Self::do_fetch_results(classroom_client, github_client, classroom, assignment, deadline, progress_tx).await
        });

        self.background_task = Some(task);
    }

    fn spawn_fetch_late_results(
        &mut self,
        classroom: Classroom,
        assignment: Assignment,
        on_time_deadline: chrono::DateTime<Utc>,
        late_deadline: chrono::DateTime<Utc>,
        late_penalty: f64,
    ) {
        // Create progress channel
        let (progress_tx, progress_rx) = tokio::sync::mpsc::unbounded_channel();
        self.progress_rx = Some(progress_rx);

        // Set initial fetching state
        let progress = FetchProgress::new(0);
        self.state = AppState::FetchingLateResults {
            _classroom: classroom.clone(),
            assignment: assignment.clone(),
            _on_time_deadline: on_time_deadline,
            _late_deadline: late_deadline,
            _late_penalty: late_penalty,
            progress,
        };

        // Clone clients for the background task
        let classroom_client = self.classroom_client.clone();
        let github_client = self.github_client.clone();

        // Spawn background task
        let task = tokio::spawn(async move {
            Self::do_fetch_late_results(
                classroom_client,
                github_client,
                classroom,
                assignment,
                on_time_deadline,
                late_deadline,
                late_penalty,
                progress_tx,
            ).await
        });

        self.background_task = Some(task);
    }

    async fn do_fetch_results(
        classroom_client: ClassroomClient,
        github_client: GitHubClient,
        classroom: Classroom,
        assignment: Assignment,
        deadline: Option<chrono::DateTime<Utc>>,
        progress_tx: tokio::sync::mpsc::UnboundedSender<FetchProgress>,
    ) -> Result<AppState> {
        let mut progress = FetchProgress::new(0);

        // Send initial progress
        progress.add_status("Fetching assignment details...".to_string());
        let _ = progress_tx.send(progress.clone());

        // Fetch assignment details
        let assignment_details = classroom_client
            .get_assignment(assignment.id)
            .await
            .context("Failed to fetch assignment details")?;

        progress.add_status("✓ Assignment details loaded".to_string());
        progress.add_status("Fetching list of students...".to_string());
        let _ = progress_tx.send(progress.clone());

        // Get all accepted assignments
        let accepted_assignments = classroom_client
            .list_accepted_assignments(assignment.id)
            .await
            .context("Failed to fetch accepted assignments")?;

        if accepted_assignments.is_empty() {
            anyhow::bail!("No students have accepted this assignment yet");
        }

        progress.total_students = accepted_assignments.len();
        progress.add_status(format!("✓ Found {} students", accepted_assignments.len()));
        progress.add_status("Loading test definitions...".to_string());
        let _ = progress_tx.send(progress.clone());

        // Fetch test definitions
        let test_definitions = if let Some(starter_url) = &assignment_details.starter_code_url {
            fetcher::fetch_test_definitions(&github_client, starter_url).await?
        } else {
            let first_student = &accepted_assignments[0];
            let (owner, repo) = fetcher::parse_repo_url(&first_student.repository.full_name);
            let workflow_content = github_client
                .get_file_contents(owner, repo, ".github/workflows/classroom.yml")
                .await
                .context("Failed to fetch workflow file from first student's repository")?;
            parser::parse_workflow(&workflow_content)?
        };

        progress.add_status(format!("✓ Loaded {} tests", test_definitions.len()));
        progress.add_status("Fetching student results...".to_string());
        let _ = progress_tx.send(progress.clone());

        // Fetch results for each student
        let mut results = Vec::new();
        for (index, student) in accepted_assignments.iter().enumerate() {
            let student_name = student
                .students
                .first()
                .map(|s| s.login.as_str())
                .unwrap_or("unknown");

            progress.completed = index;
            progress.current_student = student_name.to_string();
            progress.add_status(format!("[{}/{}] {}", index + 1, accepted_assignments.len(), student_name));
            let _ = progress_tx.send(progress.clone());

            match fetcher::fetch_student_results(&github_client, student, deadline, &test_definitions).await {
                Ok(result) => {
                    results.push(result);
                    progress.add_status(format!("  ✓ {} - {}/{} points",
                        student_name,
                        results.last().unwrap().total_awarded,
                        results.last().unwrap().total_available));
                }
                Err(e) => {
                    eprintln!("Error fetching results for {}: {}", student_name, e);
                    progress.errors += 1;
                    progress.add_status(format!("  ✗ {} - Error", student_name));
                }
            }
        }

        progress.completed = accepted_assignments.len();
        progress.add_status(format!("✓ Completed {} students", results.len()));
        let _ = progress_tx.send(progress.clone());

        // Export to CSV
        let csv_filename = export::export_to_csv(&results, &assignment.slug)?;

        // Calculate stats
        let stats = ResultStats::calculate(&results);

        Ok(AppState::ResultsComplete {
            classroom,
            assignment,
            stats,
            csv_filename: csv_filename.to_string_lossy().to_string(),
        })
    }

    async fn do_fetch_late_results(
        classroom_client: ClassroomClient,
        github_client: GitHubClient,
        classroom: Classroom,
        assignment: Assignment,
        on_time_deadline: chrono::DateTime<Utc>,
        late_deadline: chrono::DateTime<Utc>,
        late_penalty: f64,
        progress_tx: tokio::sync::mpsc::UnboundedSender<FetchProgress>,
    ) -> Result<AppState> {
        let mut progress = FetchProgress::new(0);

        // Send initial progress
        progress.add_status("Starting late grading fetch...".to_string());
        let _ = progress_tx.send(progress.clone());

        // Create progress callback that sends through the channel
        let progress_tx_clone = progress_tx.clone();
        let progress_callback = Box::new(move |completed: usize, total: usize, student: &str| {
            let mut p = FetchProgress::new(total);
            p.completed = completed.saturating_sub(1);
            p.total_students = total;
            p.current_student = student.to_string();
            p.add_status(format!("[{}/{}] {}", completed, total, student));
            let _ = progress_tx_clone.send(p);
        });

        // Fetch late grading results
        let results = fetcher::fetch_all_late_results(
            &classroom_client,
            &github_client,
            assignment.id,
            on_time_deadline,
            late_deadline,
            late_penalty,
            Some(progress_callback),
        ).await?;

        progress.completed = progress.total_students;
        progress.add_status(format!("✓ Completed {} students", results.len()));
        let _ = progress_tx.send(progress.clone());

        // Export to CSV
        let csv_filename = export::export_late_grading_to_csv(&results, &assignment.slug)?;

        // Calculate stats (using on-time results)
        let regular_results: Vec<_> = results.iter().map(|r| r.on_time_result.clone()).collect();
        let stats = ResultStats::calculate(&regular_results);

        Ok(AppState::ResultsComplete {
            classroom,
            assignment,
            stats,
            csv_filename: csv_filename.to_string_lossy().to_string(),
        })
    }

}

fn parse_deadline(date_str: &str, time_str: &str) -> Result<chrono::DateTime<Utc>> {
    let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
        .map_err(|e| anyhow::anyhow!("Invalid date format (expected YYYY-MM-DD): {}", e))?;

    let time = NaiveTime::parse_from_str(time_str, "%H:%M")
        .map_err(|e| anyhow::anyhow!("Invalid time format (expected HH:MM): {}", e))?;

    let datetime = NaiveDateTime::new(date, time);
    Ok(datetime.and_utc())
}
