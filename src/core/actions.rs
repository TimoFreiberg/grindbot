use crate::core::state::Issue;

/// Actions the planner can emit.
#[derive(Clone, Debug)]
pub enum Action {
    StartImplementer {
        issue: Issue,
        workspace_name: String,
        base_commit: String,
    },
    CleanupWorkspace {
        workspace_name: String,
        reason: CleanupReason,
    },
    MergeImplementation {
        workspace_name: String,
        commit: String,
        base_commit: String,
        issue_number: u64,
    },
    PostComment {
        issue_number: u64,
        body: String,
    },
    ResolveConflict {
        workspace_name: String,
        commit: String,
        base_commit: String,
        issue_number: u64,
    },
    DiscardImplementation {
        workspace_name: String,
        issue_number: u64,
    },
    TerminateSession {
        session_id: String,
    },
    PushToRemote,
    Noop,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CleanupReason {
    SessionCrashed,
    SessionStalled,
    SessionFinished,
    MalformedHandoff,
    OrphanedWorkspace,
}
