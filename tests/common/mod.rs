//! Shared mock I/O implementations for integration tests.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use grindbot::core::state::Issue;
use grindbot::io::{
    CommitInfo, Filesystem, GithubClient, JjClient, PolytokenClient, RebaseResult, SessionInfo,
    SessionState,
};

// --- Mock Filesystem ---

pub struct MockFilesystem {
    pub files: Arc<Mutex<std::collections::HashMap<String, String>>>,
}

impl MockFilesystem {
    pub fn new() -> Self {
        Self {
            files: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }
}

impl Filesystem for MockFilesystem {
    fn read_to_string(&self, path: &str) -> anyhow::Result<String> {
        self.files
            .lock()
            .unwrap()
            .get(path)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("not found: {}", path))
    }

    fn write(&self, path: &str, content: &str) -> anyhow::Result<()> {
        self.files
            .lock()
            .unwrap()
            .insert(path.to_string(), content.to_string());
        Ok(())
    }

    fn exists(&self, path: &str) -> bool {
        self.files.lock().unwrap().contains_key(path)
    }

    fn remove_dir_all(&self, _path: &str) -> anyhow::Result<()> {
        Ok(())
    }

    fn create_dir_all(&self, _path: &str) -> anyhow::Result<()> {
        Ok(())
    }
}

// --- Mock GitHub Client ---

pub struct MockGithubClient {
    pub posted_comments: Arc<Mutex<Vec<(u64, String)>>>,
}

impl MockGithubClient {
    pub fn new() -> Self {
        Self {
            posted_comments: Arc::new(Mutex::new(vec![])),
        }
    }
}

#[async_trait]
impl GithubClient for MockGithubClient {
    async fn list_issues(&self, _owner: &str, _repo: &str) -> anyhow::Result<Vec<Issue>> {
        Ok(vec![])
    }

    async fn post_comment(
        &self,
        _owner: &str,
        _repo: &str,
        issue: u64,
        body: &str,
    ) -> anyhow::Result<()> {
        self.posted_comments
            .lock()
            .unwrap()
            .push((issue, body.to_string()));
        Ok(())
    }
}

// --- Mock Jj Client ---

pub struct MockJjClient {
    pub rebase_result: Arc<Mutex<Option<RebaseResult>>>,
    pub rebase_calls: Arc<Mutex<Vec<(String, String)>>>,
    pub bookmark_calls: Arc<Mutex<Vec<(String, String)>>>,
    pub push_calls: Arc<Mutex<Vec<(String, String)>>>,
    pub workspaces: Arc<Mutex<Vec<String>>>,
    pub created_workspaces: Arc<Mutex<Vec<(String, String, String)>>>,
    pub forgotten: Arc<Mutex<Vec<String>>>,
    pub has_conflicts_result: Arc<Mutex<bool>>,
}

impl MockJjClient {
    pub fn new() -> Self {
        Self {
            rebase_result: Arc::new(Mutex::new(None)),
            rebase_calls: Arc::new(Mutex::new(vec![])),
            bookmark_calls: Arc::new(Mutex::new(vec![])),
            push_calls: Arc::new(Mutex::new(vec![])),
            workspaces: Arc::new(Mutex::new(vec![])),
            created_workspaces: Arc::new(Mutex::new(vec![])),
            forgotten: Arc::new(Mutex::new(vec![])),
            has_conflicts_result: Arc::new(Mutex::new(false)),
        }
    }

    pub fn set_rebase_result(&self, result: RebaseResult) {
        *self.rebase_result.lock().unwrap() = Some(result);
    }

    pub fn set_has_conflicts(&self, has: bool) {
        *self.has_conflicts_result.lock().unwrap() = has;
    }
}

#[async_trait]
impl JjClient for MockJjClient {
    async fn init_colocated(&self, _repo_path: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn create_workspace(
        &self,
        dest: &str,
        name: &str,
        base_rev: &str,
    ) -> anyhow::Result<()> {
        self.created_workspaces.lock().unwrap().push((
            dest.to_string(),
            name.to_string(),
            base_rev.to_string(),
        ));
        self.workspaces.lock().unwrap().push(name.to_string());
        Ok(())
    }

    async fn forget_workspace(&self, name: &str) -> anyhow::Result<()> {
        self.forgotten.lock().unwrap().push(name.to_string());
        self.workspaces.lock().unwrap().retain(|w| w != name);
        Ok(())
    }

    async fn list_workspaces(&self) -> anyhow::Result<Vec<String>> {
        Ok(self.workspaces.lock().unwrap().clone())
    }

    async fn rebase(&self, revset: &str, dest: &str) -> anyhow::Result<RebaseResult> {
        self.rebase_calls
            .lock()
            .unwrap()
            .push((revset.to_string(), dest.to_string()));
        Ok(self
            .rebase_result
            .lock()
            .unwrap()
            .clone()
            .unwrap_or(RebaseResult::Success))
    }

    async fn set_bookmark(&self, name: &str, rev: &str) -> anyhow::Result<()> {
        self.bookmark_calls
            .lock()
            .unwrap()
            .push((name.to_string(), rev.to_string()));
        Ok(())
    }

    async fn log(&self, _revset: &str) -> anyhow::Result<Vec<CommitInfo>> {
        Ok(vec![])
    }

    async fn current_main(&self) -> anyhow::Result<String> {
        Ok("maincommit123".to_string())
    }

    async fn push(&self, remote: &str, branch: &str) -> anyhow::Result<()> {
        self.push_calls
            .lock()
            .unwrap()
            .push((remote.to_string(), branch.to_string()));
        Ok(())
    }

    async fn has_conflicts(&self) -> anyhow::Result<bool> {
        Ok(*self.has_conflicts_result.lock().unwrap())
    }
}

// --- Mock Polytoken Client ---

pub struct MockPolytokenClient {
    pub spawned_sessions: Arc<Mutex<Vec<String>>>,
    pub facet_calls: Arc<Mutex<Vec<(String, String)>>>,
    pub handoff_calls: Arc<Mutex<Vec<String>>>,
    pub permission_calls: Arc<Mutex<Vec<(String, String)>>>,
    pub goal_calls: Arc<Mutex<Vec<(String, String)>>>,
    pub prompt_calls: Arc<Mutex<Vec<(String, String, u32)>>>,
    pub terminate_calls: Arc<Mutex<Vec<String>>>,
    pub alive_sessions: Arc<Mutex<std::collections::HashSet<String>>>,
    pub turn_in_flight: Arc<Mutex<bool>>,
}

impl MockPolytokenClient {
    pub fn new() -> Self {
        Self {
            spawned_sessions: Arc::new(Mutex::new(vec![])),
            facet_calls: Arc::new(Mutex::new(vec![])),
            handoff_calls: Arc::new(Mutex::new(vec![])),
            permission_calls: Arc::new(Mutex::new(vec![])),
            goal_calls: Arc::new(Mutex::new(vec![])),
            prompt_calls: Arc::new(Mutex::new(vec![])),
            terminate_calls: Arc::new(Mutex::new(vec![])),
            alive_sessions: Arc::new(Mutex::new(std::collections::HashSet::new())),
            turn_in_flight: Arc::new(Mutex::new(false)),
        }
    }
}

#[async_trait]
impl PolytokenClient for MockPolytokenClient {
    async fn spawn_session(&self, workspace_dir: &str) -> anyhow::Result<SessionInfo> {
        let session_id = format!("session-{}", self.spawned_sessions.lock().unwrap().len());
        self.spawned_sessions
            .lock()
            .unwrap()
            .push(workspace_dir.to_string());
        self.alive_sessions
            .lock()
            .unwrap()
            .insert(session_id.clone());
        Ok(SessionInfo {
            session_id,
            port: 12345,
            credential_file: "/tmp/cred.json".to_string(),
            bearer_token: "test-token".to_string(),
        })
    }

    async fn set_facet(&self, session: &SessionInfo, facet: &str) -> anyhow::Result<()> {
        self.facet_calls
            .lock()
            .unwrap()
            .push((session.session_id.clone(), facet.to_string()));
        Ok(())
    }

    async fn enable_adventurous_handoff(&self, session: &SessionInfo) -> anyhow::Result<()> {
        self.handoff_calls
            .lock()
            .unwrap()
            .push(session.session_id.clone());
        Ok(())
    }

    async fn set_permission_mode(&self, session: &SessionInfo, mode: &str) -> anyhow::Result<()> {
        self.permission_calls
            .lock()
            .unwrap()
            .push((session.session_id.clone(), mode.to_string()));
        Ok(())
    }

    async fn set_goal(&self, session: &SessionInfo, summary: &str) -> anyhow::Result<()> {
        self.goal_calls
            .lock()
            .unwrap()
            .push((session.session_id.clone(), summary.to_string()));
        Ok(())
    }

    async fn send_prompt(
        &self,
        session: &SessionInfo,
        content: &str,
        max_turns: u32,
    ) -> anyhow::Result<()> {
        self.prompt_calls.lock().unwrap().push((
            session.session_id.clone(),
            content.to_string(),
            max_turns,
        ));
        Ok(())
    }

    async fn get_state(&self, session: &SessionInfo) -> anyhow::Result<SessionState> {
        Ok(SessionState {
            turn_in_flight: *self.turn_in_flight.lock().unwrap(),
            cwd: Some(session.session_id.clone()),
        })
    }

    async fn terminate(&self, session: &SessionInfo) -> anyhow::Result<()> {
        self.terminate_calls
            .lock()
            .unwrap()
            .push(session.session_id.clone());
        self.alive_sessions
            .lock()
            .unwrap()
            .remove(&session.session_id);
        Ok(())
    }

    async fn is_alive(&self, session: &SessionInfo) -> bool {
        self.alive_sessions
            .lock()
            .unwrap()
            .contains(&session.session_id)
    }
}
