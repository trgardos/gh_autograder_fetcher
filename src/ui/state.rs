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
    GradingModeSelection {
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
    LateGradingInput {
        classroom: Classroom,
        assignment: Assignment,
        on_time_date: String,
        on_time_time: String,
        late_date: String,
        late_time: String,
        penalty_input: String,
        focused_field: LateGradingField,
    },
    FetchingResults {
        _classroom: Classroom,
        assignment: Assignment,
        _deadline: Option<DateTime<Utc>>,
        progress: FetchProgress,
    },
    FetchingLateResults {
        _classroom: Classroom,
        assignment: Assignment,
        _on_time_deadline: DateTime<Utc>,
        _late_deadline: DateTime<Utc>,
        _late_penalty: f64,
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LateGradingField {
    OnTimeDate,
    OnTimeTime,
    LateDate,
    LateTime,
    Penalty,
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
