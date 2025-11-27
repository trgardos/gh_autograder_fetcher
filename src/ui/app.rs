use crate::api::{ClassroomClient, GitHubClient};
use crate::export;
use crate::fetcher;
use crate::models::{Assignment, Classroom, ResultStats, StudentResult};
use crate::ui::render::render_ui;
use crate::ui::state::{AppState, DeadlineField, FetchProgress};
use anyhow::Result;
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
}

impl App {
    pub fn new(classroom_client: ClassroomClient, github_client: GitHubClient) -> Self {
        Self {
            classroom_client,
            github_client,
            state: AppState::LoadingClassrooms,
        }
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
            terminal.draw(|f| render_ui(f, &self.state))?;

            if event::poll(std::time::Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    if self.handle_key_event(key).await? {
                        break; // User quit
                    }
                }
            }

            // Check if we need to perform background operations
            if let AppState::LoadingClassrooms = self.state {
                // This should have been handled in load_classrooms
            }
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
                        if selected_index < 1 {
                            // 0: Latest, 1: After deadline
                            selected_index += 1;
                        }
                        self.state = AppState::AssignmentOptions {
                            classroom,
                            assignment,
                            selected_index,
                        };
                    }
                    KeyCode::Enter => {
                        if selected_index == 0 {
                            // Download latest results
                            self.fetch_results(classroom, assignment, None).await?;
                        } else {
                            // Download results after deadline
                            self.state = AppState::DeadlineInput {
                                classroom,
                                assignment,
                                date_input: String::new(),
                                time_input: String::new(),
                                focused_field: DeadlineField::Date,
                            };
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
                                if time_input.len() < 8 {
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
                                self.fetch_results(classroom, assignment, Some(deadline))
                                    .await?;
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

    async fn fetch_results(
        &mut self,
        classroom: Classroom,
        assignment: Assignment,
        deadline: Option<chrono::DateTime<Utc>>,
    ) -> Result<()> {
        // Initialize progress
        self.state = AppState::FetchingResults {
            classroom: classroom.clone(),
            assignment: assignment.clone(),
            deadline,
            progress: FetchProgress::new(0),
        };

        // Fetch results (this is a simplified version - in reality you'd use a background task)
        let results = self.fetch_results_internal(assignment.id, deadline).await?;

        // Export to CSV
        let csv_filename = export::export_to_csv(&results, &assignment.slug)?;

        // Calculate stats
        let stats = ResultStats::calculate(&results);

        self.state = AppState::ResultsComplete {
            classroom,
            assignment,
            stats,
            csv_filename: csv_filename.to_string_lossy().to_string(),
        };

        Ok(())
    }

    async fn fetch_results_internal(
        &mut self,
        assignment_id: u64,
        deadline: Option<chrono::DateTime<Utc>>,
    ) -> Result<Vec<StudentResult>> {
        fetcher::fetch_all_results(
            &self.classroom_client,
            &self.github_client,
            assignment_id,
            deadline,
            None, // TODO: Add progress callback for TUI updates
        )
        .await
    }
}

fn parse_deadline(date_str: &str, time_str: &str) -> Result<chrono::DateTime<Utc>> {
    let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
        .map_err(|e| anyhow::anyhow!("Invalid date format (expected YYYY-MM-DD): {}", e))?;

    let time = NaiveTime::parse_from_str(time_str, "%H:%M:%S")
        .map_err(|e| anyhow::anyhow!("Invalid time format (expected HH:MM:SS): {}", e))?;

    let datetime = NaiveDateTime::new(date, time);
    Ok(datetime.and_utc())
}
