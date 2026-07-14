pub mod filesystem;
pub mod github;
pub mod jj;
pub mod polytoken;

use std::sync::Arc;

use crate::core::state::Issue;

#[async_trait::async_trait]
pub trait GithubClient: Send + Sync {
    async fn list_issues(&self, owner: &str, repo: &str) -> anyhow::Result<Vec<Issue>>;
    async fn post_comment(
        &self,
        owner: &str,
        repo: &str,
        issue: u64,
        body: &str,
    ) -> anyhow::Result<()>;
}

#[derive(Clone, Debug)]
pub struct CommandOutput {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

pub trait CommandRunner: Send + Sync {
    fn run(&self, command: &str, cwd: &str) -> anyhow::Result<CommandOutput>;
}

pub struct RealCommandRunner;

impl CommandRunner for RealCommandRunner {
    fn run(&self, command: &str, cwd: &str) -> anyhow::Result<CommandOutput> {
        let output = std::process::Command::new("sh")
            .args(["-c", command])
            .current_dir(cwd)
            .output()?;
        Ok(CommandOutput {
            status: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

#[async_trait::async_trait]
pub trait JjClient: Send + Sync {
    async fn fetch(&self) -> anyhow::Result<()>;
    async fn init_colocated(&self, repo_path: &str) -> anyhow::Result<()>;
    async fn create_workspace(&self, dest: &str, name: &str, base_rev: &str) -> anyhow::Result<()>;
    async fn forget_workspace(&self, name: &str) -> anyhow::Result<()>;
    async fn list_workspaces(&self) -> anyhow::Result<Vec<String>>;
    async fn rebase(&self, revset: &str, dest: &str) -> anyhow::Result<RebaseResult>;
    async fn set_bookmark(&self, name: &str, rev: &str) -> anyhow::Result<()>;
    async fn log(&self, revset: &str) -> anyhow::Result<Vec<CommitInfo>>;
    async fn current_main(&self) -> anyhow::Result<String>;
    async fn push(&self, remote: &str, branch: &str) -> anyhow::Result<()>;
    async fn has_conflicts(&self) -> anyhow::Result<bool>;
}

#[derive(Clone, Debug)]
pub struct CommitInfo {
    pub change_id: String,
    pub commit_hash: String,
    pub description: String,
}

#[derive(Clone, Debug)]
pub enum RebaseResult {
    Success,
    Conflict { conflicted_files: Vec<String> },
}

#[async_trait::async_trait]
pub trait PolytokenClient: Send + Sync {
    async fn spawn_session(&self, workspace_dir: &str) -> anyhow::Result<SessionInfo>;
    async fn set_facet(&self, session: &SessionInfo, facet: &str) -> anyhow::Result<()>;
    async fn enable_adventurous_handoff(&self, session: &SessionInfo) -> anyhow::Result<()>;
    async fn set_permission_mode(&self, session: &SessionInfo, mode: &str) -> anyhow::Result<()>;
    async fn set_goal(&self, session: &SessionInfo, summary: &str) -> anyhow::Result<()>;
    async fn send_prompt(
        &self,
        session: &SessionInfo,
        content: &str,
        max_turns: u32,
    ) -> anyhow::Result<()>;
    async fn get_state(&self, session: &SessionInfo) -> anyhow::Result<SessionState>;
    async fn terminate(&self, session: &SessionInfo) -> anyhow::Result<()>;
    async fn is_alive(&self, session: &SessionInfo) -> bool;
}

#[derive(Clone, Debug)]
pub struct SessionInfo {
    pub session_id: String,
    pub port: u16,
    pub credential_file: String,
    pub bearer_token: String,
}

#[derive(Clone, Debug)]
pub struct SessionState {
    pub turn_in_flight: bool,
    pub cwd: Option<String>,
    pub used_tokens: Option<u32>,
    pub limit_tokens: Option<u32>,
    pub most_recent_assistant_text: Option<String>,
}

pub trait Filesystem: Send + Sync {
    fn read_to_string(&self, path: &str) -> anyhow::Result<String>;
    fn write(&self, path: &str, content: &str) -> anyhow::Result<()>;
    fn try_create_exclusive(&self, path: &str, content: &str) -> anyhow::Result<bool>;
    fn exists(&self, path: &str) -> bool;
    fn remove_dir_all(&self, path: &str) -> anyhow::Result<()>;
    fn remove_file(&self, path: &str) -> anyhow::Result<()>;
    fn create_dir_all(&self, path: &str) -> anyhow::Result<()>;
}

/// Bundles all trait objects for the supervisor.
pub struct IoLayer {
    pub github: Arc<dyn GithubClient>,
    pub jj: Arc<dyn JjClient>,
    pub polytoken: Arc<dyn PolytokenClient>,
    pub fs: Arc<dyn Filesystem>,
    pub command: Arc<dyn CommandRunner>,
}
