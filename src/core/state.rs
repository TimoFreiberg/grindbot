use serde::{Deserialize, Serialize};

/// Complete state snapshot fed to the planner.
#[derive(Debug)]
pub struct SupervisorState {
    pub config: crate::config::Config,
    pub issues: Vec<Issue>,
    pub implementers: Vec<ImplementerState>,
    pub workspaces: Vec<WorkspaceState>,
    pub main_head: String,
    pub completed_issues: Vec<u64>,
}

#[derive(Clone, Debug)]
pub struct Issue {
    pub number: u64,
    pub title: String,
    pub body: String,
    pub author: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub comments: Vec<Comment>,
}

#[derive(Clone, Debug)]
pub struct Comment {
    pub author: String,
    pub body: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub is_supervisor: bool,
}

#[derive(Clone, Debug)]
pub struct ImplementerState {
    pub issue_number: u64,
    pub session_id: String,
    pub workspace_name: String,
    pub workspace_path: String,
    pub base_commit: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub status: ImplementerStatus,
}

#[derive(Clone, Debug)]
pub enum ImplementerStatus {
    Running,
    Finished(ImplementerResult),
    Crashed,
}

#[derive(Clone, Debug)]
pub enum ImplementerResult {
    Done { commit: String },
    NeedsFeedback { message: String },
}

#[derive(Clone, Debug)]
pub struct WorkspaceState {
    pub name: String,
    pub path: String,
    pub task_issue: Option<u64>,
    pub session_id: Option<String>,
    pub daemon_alive: bool,
    pub has_result_file: bool,
}

impl SupervisorState {
    /// Count of implementers currently running (not finished/crashed).
    pub fn active_count(&self) -> usize {
        self.implementers
            .iter()
            .filter(|i| matches!(i.status, ImplementerStatus::Running))
            .count()
    }

    /// Issue numbers currently being implemented.
    pub fn active_issues(&self) -> Vec<u64> {
        self.implementers
            .iter()
            .filter(|i| matches!(i.status, ImplementerStatus::Running))
            .map(|i| i.issue_number)
            .collect()
    }
}

/// Result file written by the handoff binary, read by the supervisor.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum HandoffResult {
    #[serde(rename = "done")]
    Done { commit: String, timestamp: String },
    #[serde(rename = "needs-feedback")]
    NeedsFeedback { message: String, timestamp: String },
}
