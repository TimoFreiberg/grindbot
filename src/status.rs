use std::sync::Arc;

use crate::config::Config;
use crate::io::IoLayer;
use crate::state_file::StateFile;
use crate::supervisor;

/// Entry point for the CLI. Builds a real IO layer and delegates.
pub async fn run(config: Config) -> anyhow::Result<()> {
    let io = build_io_layer(&config);
    let state_file = StateFile::load(&config)?;
    let output = build_status_output(&config, &io, &state_file).await?;
    println!("{}", output);
    Ok(())
}

fn build_io_layer(config: &Config) -> IoLayer {
    let repo_path = std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .to_string_lossy()
        .to_string();

    IoLayer {
        github: Arc::new(crate::io::github::RealGithubClient::new()),
        jj: Arc::new(crate::io::jj::RealJjClient::new(&repo_path)),
        polytoken: Arc::new(crate::io::polytoken::RealPolytokenClient::new(
            &config.polytoken.binary,
        )),
        fs: Arc::new(crate::io::filesystem::RealFilesystem::new()),
        command: Arc::new(crate::io::RealCommandRunner),
    }
}

/// Testable core: takes a pre-built IoLayer (so tests can inject mocks)
/// and returns the formatted status string.
pub(crate) async fn build_status_output(
    config: &Config,
    io: &IoLayer,
    state_file: &StateFile,
) -> anyhow::Result<String> {
    let mut lines = Vec::new();

    lines.push(format!(
        "Grindbot Status — {}/{}",
        config.github.owner, config.github.repo
    ));
    lines.push("".to_string());

    // Active implementers
    if state_file.active_implementers.is_empty() {
        lines.push("Active Implementers: (none)".to_string());
    } else {
        lines.push(format!(
            "Active Implementers ({}):",
            state_file.active_implementers.len()
        ));
        for active in &state_file.active_implementers {
            let session_info = supervisor::reconstruct_session_info_pub(active);
            let alive = io.polytoken.is_alive(&session_info).await;
            let status_str = if alive { "running" } else { "dead" };
            let indicator = if alive { "●" } else { "○" };
            let started = supervisor::parse_started_at(&active.started_at);
            lines.push(format!(
                "  #{}  {}  session: {}  {} {}  started: {}",
                active.issue_number,
                active.workspace_name,
                active.session_id,
                indicator,
                status_str,
                started.format("%Y-%m-%d %H:%M UTC")
            ));
        }
    }

    lines.push("".to_string());

    // Completed tasks
    if state_file.completed_tasks.is_empty() {
        lines.push("Completed: (none)".to_string());
    } else {
        let issue_list: Vec<String> = state_file
            .completed_tasks
            .iter()
            .map(|t| format!("#{}", t.issue_number))
            .collect();
        lines.push(format!(
            "Completed ({}): {}",
            state_file.completed_tasks.len(),
            issue_list.join(", ")
        ));
    }

    // Needs feedback
    if !state_file.needs_feedback.is_empty() {
        lines.push("".to_string());
        lines.push(format!(
            "Needs Feedback ({}):",
            state_file.needs_feedback.len()
        ));
        for nf in &state_file.needs_feedback {
            lines.push(format!("  #{}: {}", nf.issue_number, nf.message));
        }
    }

    // Conflict retries
    if !state_file.conflict_retries.is_empty() {
        lines.push("".to_string());
        lines.push("Conflict Retries:".to_string());
        for cr in &state_file.conflict_retries {
            lines.push(format!("  #{}: {}/3", cr.issue_number, cr.count));
        }
    }

    Ok(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::{Filesystem, GithubClient, JjClient, PolytokenClient, SessionState};

    // Minimal mock for status tests
    struct MockFs;
    impl Filesystem for MockFs {
        fn read_to_string(&self, _path: &str) -> anyhow::Result<String> {
            Ok(String::new())
        }
        fn write(&self, _path: &str, _content: &str) -> anyhow::Result<()> {
            Ok(())
        }
        fn try_create_exclusive(&self, _path: &str, _content: &str) -> anyhow::Result<bool> {
            Ok(true)
        }
        fn exists(&self, _path: &str) -> bool {
            false
        }
        fn remove_dir_all(&self, _path: &str) -> anyhow::Result<()> {
            Ok(())
        }
        fn remove_file(&self, _path: &str) -> anyhow::Result<()> {
            Ok(())
        }
        fn create_dir_all(&self, _path: &str) -> anyhow::Result<()> {
            Ok(())
        }
    }

    struct MockGh;
    #[async_trait::async_trait]
    impl GithubClient for MockGh {
        async fn list_issues(&self, _owner: &str, _repo: &str) -> anyhow::Result<Vec<crate::core::state::Issue>> {
            Ok(vec![])
        }
        async fn post_comment(
            &self,
            _owner: &str,
            _repo: &str,
            _issue: u64,
            _body: &str,
        ) -> anyhow::Result<()> {
            Ok(())
        }
    }

    struct MockJj;
    #[async_trait::async_trait]
    impl JjClient for MockJj {
        async fn fetch(&self) -> anyhow::Result<()> {
            Ok(())
        }
        async fn init_colocated(&self, _repo_path: &str) -> anyhow::Result<()> {
            Ok(())
        }
        async fn create_workspace(&self, _dest: &str, _name: &str, _base_rev: &str) -> anyhow::Result<()> {
            Ok(())
        }
        async fn forget_workspace(&self, _name: &str) -> anyhow::Result<()> {
            Ok(())
        }
        async fn list_workspaces(&self) -> anyhow::Result<Vec<String>> {
            Ok(vec![])
        }
        async fn rebase(&self, _revset: &str, _dest: &str) -> anyhow::Result<crate::io::RebaseResult> {
            Ok(crate::io::RebaseResult::Success)
        }
        async fn set_bookmark(&self, _name: &str, _rev: &str) -> anyhow::Result<()> {
            Ok(())
        }
        async fn log(&self, _revset: &str) -> anyhow::Result<Vec<crate::io::CommitInfo>> {
            Ok(vec![])
        }
        async fn current_main(&self) -> anyhow::Result<String> {
            Ok("main123".to_string())
        }
        async fn push(&self, _remote: &str, _branch: &str) -> anyhow::Result<()> {
            Ok(())
        }
        async fn has_conflicts(&self) -> anyhow::Result<bool> {
            Ok(false)
        }
    }

    struct MockPtAlive;
    #[async_trait::async_trait]
    impl PolytokenClient for MockPtAlive {
        async fn spawn_session(&self, _workspace_dir: &str) -> anyhow::Result<crate::io::SessionInfo> {
            Ok(crate::io::SessionInfo {
                session_id: "test".to_string(),
                port: 12345,
                bearer_token: "tok".to_string(),
                credential_file: String::new(),
            })
        }
        async fn set_facet(&self, _session: &crate::io::SessionInfo, _facet: &str) -> anyhow::Result<()> {
            Ok(())
        }
        async fn enable_adventurous_handoff(&self, _session: &crate::io::SessionInfo) -> anyhow::Result<()> {
            Ok(())
        }
        async fn set_permission_mode(&self, _session: &crate::io::SessionInfo, _mode: &str) -> anyhow::Result<()> {
            Ok(())
        }
        async fn set_goal(&self, _session: &crate::io::SessionInfo, _summary: &str) -> anyhow::Result<()> {
            Ok(())
        }
        async fn send_prompt(&self, _session: &crate::io::SessionInfo, _content: &str, _max_turns: u32) -> anyhow::Result<()> {
            Ok(())
        }
        async fn get_state(&self, _session: &crate::io::SessionInfo) -> anyhow::Result<SessionState> {
            Ok(SessionState { turn_in_flight: false, cwd: None })
        }
        async fn terminate(&self, _session: &crate::io::SessionInfo) -> anyhow::Result<()> {
            Ok(())
        }
        async fn is_alive(&self, _session: &crate::io::SessionInfo) -> bool {
            true
        }
    }

    fn make_config() -> Config {
        Config {
            github: crate::config::GithubConfig {
                owner: "test".to_string(),
                repo: "repo".to_string(),
                allowlist: vec!["alice".to_string()],
            },
            ..Config::default()
        }
    }

    #[tokio::test]
    async fn test_status_output_with_mocks() {
        let config = make_config();
        let io = IoLayer {
            github: Arc::new(MockGh),
            jj: Arc::new(MockJj),
            polytoken: Arc::new(MockPtAlive),
            fs: Arc::new(MockFs),
            command: Arc::new(crate::io::RealCommandRunner),
        };

        let mut state_file = StateFile::default();
        state_file.active_implementers.push(crate::state_file::ActiveImplementer {
            issue_number: 42,
            session_id: "sess-abc".to_string(),
            workspace_name: "grindbot-42".to_string(),
            workspace_path: "/tmp/ws42".to_string(),
            base_commit: "abc".to_string(),
            started_at: "2024-01-15T12:30:00Z".to_string(),
            port: 12345,
            bearer_token: "tok".to_string(),
            credential_file: "/tmp/cred.json".to_string(),
        });
        state_file.completed_tasks.push(crate::state_file::CompletedTask {
            issue_number: 40,
            commit: "def".to_string(),
            completed_at: "2024-01-10T00:00:00Z".to_string(),
        });
        state_file.needs_feedback.push(crate::state_file::NeedsFeedbackTask {
            issue_number: 44,
            message: "Need more info".to_string(),
            timestamp: "2024-01-12T00:00:00Z".to_string(),
        });
        state_file.conflict_retries.push(crate::state_file::ConflictRetry {
            issue_number: 43,
            count: 2,
        });

        let output = build_status_output(&config, &io, &state_file).await.unwrap();

        // Verify headers
        assert!(output.contains("Grindbot Status — test/repo"));
        assert!(output.contains("Active Implementers (1):"));
        assert!(output.contains("#42"));
        assert!(output.contains("sess-abc"));
        assert!(output.contains("running"));
        assert!(output.contains("Completed (1): #40"));
        assert!(output.contains("Needs Feedback (1):"));
        assert!(output.contains("#44: Need more info"));
        assert!(output.contains("Conflict Retries:"));
        assert!(output.contains("#43: 2/3"));
    }

    #[tokio::test]
    async fn test_status_empty_state() {
        let config = make_config();
        let io = IoLayer {
            github: Arc::new(MockGh),
            jj: Arc::new(MockJj),
            polytoken: Arc::new(MockPtAlive),
            fs: Arc::new(MockFs),
            command: Arc::new(crate::io::RealCommandRunner),
        };
        let state_file = StateFile::default();

        let output = build_status_output(&config, &io, &state_file).await.unwrap();

        assert!(output.contains("Active Implementers: (none)"));
        assert!(output.contains("Completed: (none)"));
        // Needs feedback and conflict retries sections should be absent
        assert!(!output.contains("Needs Feedback"));
        assert!(!output.contains("Conflict Retries"));
    }
}
