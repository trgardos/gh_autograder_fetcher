use crate::models::{Assignment, Classroom, ResultStats};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub enum AppState {
    LoadingClassrooms,
    ClassroomSelection {
        classrooms: Vec<Classroom>,
        selected_index: usize,
    },
    LoadingAssignments {
        classroom: Classroom,
    },
    AssignmentSelection {
        classroom: Classroom,
        assignments: Vec<Assignment>,
        selected_index: usize,
    },
    AssignmentOptions {
        classroom: Classroom,
        assignment: Assignment,
        selected_index: usize,
    },
    DeadlineInput {
        classroom: Classroom,
        assignment: Assignment,
        date_input: String,
        time_input: String,
        focused_field: DeadlineField,
    },
    FetchingResults {
        classroom: Classroom,
        assignment: Assignment,
        deadline: Option<DateTime<Utc>>,
        progress: FetchProgress,
    },
    ResultsComplete {
        classroom: Classroom,
        assignment: Assignment,
        stats: ResultStats,
        csv_filename: String,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DeadlineField {
    Date,
    Time,
}

#[derive(Debug, Clone)]
pub struct FetchProgress {
    pub total_students: usize,
    pub completed: usize,
    pub current_student: String,
    pub errors: usize,
    pub status_messages: Vec<String>,
}

impl FetchProgress {
    pub fn new(total_students: usize) -> Self {
        Self {
            total_students,
            completed: 0,
            current_student: String::new(),
            errors: 0,
            status_messages: vec!["Initializing...".to_string()],
        }
    }

    pub fn update(&mut self, completed: usize, current_student: &str) {
        self.completed = completed;
        self.current_student = current_student.to_string();
    }

    pub fn add_status(&mut self, message: String) {
        self.status_messages.push(message);
        // Keep only the last 20 messages to avoid memory issues
        if self.status_messages.len() > 20 {
            self.status_messages.remove(0);
        }
    }

    pub fn percentage(&self) -> f64 {
        if self.total_students == 0 {
            0.0
        } else {
            (self.completed as f64 / self.total_students as f64) * 100.0
        }
    }
}
