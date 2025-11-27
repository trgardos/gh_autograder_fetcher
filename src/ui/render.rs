use crate::ui::state::{AppState, DeadlineField};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Wrap},
    Frame,
};

pub fn render_ui(frame: &mut Frame, state: &AppState) {
    match state {
        AppState::LoadingClassrooms => render_loading(frame, "Loading classrooms..."),
        AppState::ClassroomSelection {
            classrooms,
            selected_index,
        } => render_classroom_selection(frame, classrooms, *selected_index),
        AppState::LoadingAssignments { classroom } => {
            render_loading(frame, &format!("Loading assignments for {}...", classroom.name))
        }
        AppState::AssignmentSelection {
            classroom,
            assignments,
            selected_index,
        } => render_assignment_selection(frame, classroom, assignments, *selected_index),
        AppState::AssignmentOptions {
            classroom,
            assignment,
            selected_index,
        } => render_assignment_options(frame, classroom, assignment, *selected_index),
        AppState::DeadlineInput {
            classroom,
            assignment,
            date_input,
            time_input,
            focused_field,
        } => render_deadline_input(frame, classroom, assignment, date_input, time_input, *focused_field),
        AppState::FetchingResults {
            assignment,
            progress,
            ..
        } => render_fetching_results(frame, assignment, progress),
        AppState::ResultsComplete {
            assignment,
            stats,
            csv_filename,
            ..
        } => render_results_complete(frame, assignment, stats, csv_filename),
        AppState::Error { message } => render_error(frame, message),
    }
}

fn render_loading(frame: &mut Frame, message: &str) {
    let area = frame.area();
    let block = Block::default()
        .title("GitHub Classroom Autograder Fetcher")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let paragraph = Paragraph::new(message)
        .block(block)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });

    frame.render_widget(paragraph, area);
}

fn render_classroom_selection(
    frame: &mut Frame,
    classrooms: &[crate::models::Classroom],
    selected_index: usize,
) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(3)])
        .split(area);

    let items: Vec<ListItem> = classrooms
        .iter()
        .enumerate()
        .map(|(i, classroom)| {
            let style = if i == selected_index {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let prefix = if i == selected_index { "> " } else { "  " };
            let archived = if classroom.archived { " [Archived]" } else { "" };
            let content = format!("{}{}{}", prefix, classroom.name, archived);

            ListItem::new(content).style(style)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .title("Select Classroom")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        );

    frame.render_widget(list, chunks[0]);

    let help = Paragraph::new(format!(
        "Found: {} classroom(s) | [↑↓: Navigate | Enter: Select | q: Quit]",
        classrooms.len()
    ))
    .block(Block::default().borders(Borders::ALL))
    .alignment(Alignment::Center);

    frame.render_widget(help, chunks[1]);
}

fn render_assignment_selection(
    frame: &mut Frame,
    classroom: &crate::models::Classroom,
    assignments: &[crate::models::Assignment],
    selected_index: usize,
) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(3)])
        .split(area);

    let items: Vec<ListItem> = assignments
        .iter()
        .enumerate()
        .map(|(i, assignment)| {
            let style = if i == selected_index {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let prefix = if i == selected_index { "> " } else { "  " };
            let deadline = assignment
                .deadline
                .map(|d| format!(" (Due: {})", d.format("%Y-%m-%d")))
                .unwrap_or_default();
            let content = format!(
                "{}{}{} - {}/{} submitted",
                prefix, assignment.title, deadline, assignment.submitted, assignment.accepted
            );

            ListItem::new(content).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(format!("Classroom: {} - Select Assignment", classroom.name))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );

    frame.render_widget(list, chunks[0]);

    let help = Paragraph::new(format!(
        "Found: {} assignment(s) | [↑↓: Navigate | Enter: Select | Esc: Back | q: Quit]",
        assignments.len()
    ))
    .block(Block::default().borders(Borders::ALL))
    .alignment(Alignment::Center);

    frame.render_widget(help, chunks[1]);
}

fn render_assignment_options(
    frame: &mut Frame,
    classroom: &crate::models::Classroom,
    assignment: &crate::models::Assignment,
    selected_index: usize,
) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(3),
            Constraint::Length(3),
        ])
        .split(area);

    // Assignment info
    let info = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("Assignment: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(&assignment.title),
        ]),
        Line::from(vec![
            Span::styled("Classroom: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(&classroom.name),
        ]),
        Line::from(vec![
            Span::styled("Starter Repo: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(assignment.starter_code_url.as_deref().unwrap_or("N/A")),
        ]),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );

    frame.render_widget(info, chunks[0]);

    // Options
    let options = vec!["Download Latest Results", "Download Results After Deadline"];
    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, option)| {
            let style = if i == selected_index {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let prefix = if i == selected_index { "> " } else { "  " };
            ListItem::new(format!("{}{}", prefix, option)).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title("Options")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );

    frame.render_widget(list, chunks[1]);

    let help = Paragraph::new("[↑↓: Navigate | Enter: Select | Esc: Back | q: Quit]")
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Center);

    frame.render_widget(help, chunks[2]);
}

fn render_deadline_input(
    frame: &mut Frame,
    _classroom: &crate::models::Classroom,
    assignment: &crate::models::Assignment,
    date_input: &str,
    time_input: &str,
    focused_field: DeadlineField,
) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(3),
        ])
        .split(area);

    // Title
    let title = Paragraph::new(format!("Enter Deadline for: {}", assignment.title))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .alignment(Alignment::Center);

    frame.render_widget(title, chunks[0]);

    // Date input
    let date_style = if focused_field == DeadlineField::Date {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };

    let date = Paragraph::new(format!("Date (YYYY-MM-DD): {}_", date_input))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(date_style),
        );

    frame.render_widget(date, chunks[1]);

    // Time input
    let time_style = if focused_field == DeadlineField::Time {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };

    let time = Paragraph::new(format!("Time (HH:MM:SS): {}_", time_input))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(time_style),
        );

    frame.render_widget(time, chunks[2]);

    // Help
    let help = Paragraph::new("[Tab: Switch Field | Enter: Confirm | Esc: Cancel | q: Quit]")
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Center);

    frame.render_widget(help, chunks[4]);
}

fn render_fetching_results(
    frame: &mut Frame,
    assignment: &crate::models::Assignment,
    progress: &crate::ui::state::FetchProgress,
) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(3),
        ])
        .split(area);

    // Title
    let title = Paragraph::new(format!("Fetching Results: {}", assignment.title))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .alignment(Alignment::Center);

    frame.render_widget(title, chunks[0]);

    // Progress bar
    let gauge = Gauge::default()
        .block(Block::default().title("Progress").borders(Borders::ALL))
        .gauge_style(Style::default().fg(Color::Green))
        .percent(progress.percentage() as u16)
        .label(format!(
            "{}/{} students | {} errors",
            progress.completed, progress.total_students, progress.errors
        ));

    frame.render_widget(gauge, chunks[1]);

    // Status messages (scrolling log)
    let status_items: Vec<ListItem> = progress
        .status_messages
        .iter()
        .map(|msg| {
            ListItem::new(format!("• {}", msg))
                .style(Style::default().fg(Color::Green))
        })
        .collect();

    let status_list = List::new(status_items)
        .block(
            Block::default()
                .title("Status Log")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        );

    frame.render_widget(status_list, chunks[2]);

    // Summary info
    let info_text = if progress.current_student.is_empty() {
        "Preparing...".to_string()
    } else {
        format!("Current student: {}", progress.current_student)
    };

    let info = Paragraph::new(info_text)
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Cyan));

    frame.render_widget(info, chunks[3]);
}

fn render_results_complete(
    frame: &mut Frame,
    assignment: &crate::models::Assignment,
    stats: &crate::models::ResultStats,
    csv_filename: &str,
) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(3)])
        .split(area);

    let text = vec![
        Line::from(vec![
            Span::styled("Results Exported!", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Assignment: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(&assignment.title),
        ]),
        Line::from(vec![
            Span::styled("File: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(csv_filename),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Students processed: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!("{}", stats.students_processed)),
        ]),
        Line::from(vec![
            Span::styled("Tests per student: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!("{}", stats.total_tests)),
        ]),
        Line::from(vec![
            Span::styled("Average score: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!("{:.2}%", stats.average_score)),
        ]),
        Line::from(vec![
            Span::styled("Median score: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!("{:.2}%", stats.median_score)),
        ]),
    ];

    let paragraph = Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .alignment(Alignment::Center);

    frame.render_widget(paragraph, chunks[0]);

    let help = Paragraph::new("[Enter: Continue | q: Quit]")
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Center);

    frame.render_widget(help, chunks[1]);
}

fn render_error(frame: &mut Frame, message: &str) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(3)])
        .split(area);

    let text = vec![
        Line::from(vec![
            Span::styled("Error", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(message),
    ];

    let paragraph = Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Red)),
        )
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });

    frame.render_widget(paragraph, chunks[0]);

    let help = Paragraph::new("[Enter: Continue | q: Quit]")
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Center);

    frame.render_widget(help, chunks[1]);
}
