use jiff::Timestamp;
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
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub comments: Vec<Comment>,
}

#[derive(Clone, Debug)]
pub struct Comment {
    pub author: String,
    pub body: String,
    pub created_at: Timestamp,
    pub is_supervisor: bool,
}

#[derive(Clone, Debug)]
pub struct ImplementerState {
    pub issue_number: u64,
    pub session_id: String,
    pub workspace_name: String,
    pub workspace_path: String,
    pub base_commit: String,
    pub started_at: Timestamp,
    pub status: ImplementerStatus,
    pub used_tokens: Option<u32>,
    pub limit_tokens: Option<u32>,
    pub stall_cycles: u32,
    pub most_recent_assistant_text: Option<String>,
}

#[derive(Clone, Debug)]
pub enum ImplementerStatus {
    Running,
    Finished(ImplementerResult),
    Malformed { error: String },
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

/// Evidence recorded by an approved handoff. The supervisor checks these facts
/// mechanically and trusts the existing reviewer agents for semantic quality.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct HandoffEvidence {
    pub plan_review: String,
    pub implementation_review: String,
    pub tests: Vec<TestEvidence>,
    pub acceptance_mapping: Vec<AcceptanceTestMapping>,
    #[serde(default)]
    pub unresolved_findings: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TestEvidence {
    pub name: String,
    pub result: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AcceptanceTestMapping {
    pub acceptance_criterion: String,
    pub verification: String,
}

/// Result file written by the handoff binary, read by the supervisor.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum HandoffResult {
    #[serde(rename = "done")]
    Done {
        #[serde(default = "default_manifest_version")]
        manifest_version: u32,
        commit: String,
        timestamp: String,
        #[serde(default)]
        issue: Option<u64>,
        #[serde(default)]
        summary: String,
        #[serde(default)]
        evidence: Option<HandoffEvidence>,
    },
    #[serde(rename = "needs-feedback")]
    NeedsFeedback { message: String, timestamp: String },
}

fn default_manifest_version() -> u32 {
    1
}
