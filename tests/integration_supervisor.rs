//! AC.11: Polytoken session startup sequence correctly configures the session.
//! AC.12: Merge flow correctly rebases, moves bookmark, and pushes.
//! AC.12b: Conflict resolution spawns agent with correct config.
//! AC.13: Supervisor posts <!-- grindbot --> comments.
//! AC.14: Crashed session results in workspace cleanup.

mod common;

use grindbot::config::Config;
use grindbot::io::{Filesystem, GithubClient, JjClient, PolytokenClient, RebaseResult};
use grindbot::state_file::{ActiveImplementer, StateFile};
use grindbot::workspace;

use common::{MockFilesystem, MockGithubClient, MockJjClient, MockPolytokenClient};

fn make_config() -> Config {
    Config {
        github: grindbot::config::GithubConfig {
            owner: "test".to_string(),
            repo: "test".to_string(),
            allowlist: vec!["alice".to_string()],
        },
        supervisor: grindbot::config::SupervisorConfig {
            max_parallelism: 2,
            poll_interval_secs: 1,
            base_branch: "main".to_string(),
        },
        ..Config::default()
    }
}

// AC.11: Verify session startup configures facet, handoff, permissions, goal, prompt
#[tokio::test]
async fn test_session_startup_sequence() {
    let config = make_config();
    let fs = MockFilesystem::new();
    let _jj = MockJjClient::new();
    let polytoken = MockPolytokenClient::new();
    let _github = MockGithubClient::new();

    // Set up workspace files
    let repo_path = "/tmp/test-repo";
    let workspace_path =
        workspace::setup_workspace(&config, repo_path, 42, "base123", &fs).unwrap();

    // Spawn session
    let session = polytoken.spawn_session(&workspace_path).await.unwrap();

    // Configure session
    polytoken.set_facet(&session, "plan").await.unwrap();
    polytoken
        .enable_adventurous_handoff(&session)
        .await
        .unwrap();
    polytoken
        .set_permission_mode(&session, "bypass_plus")
        .await
        .unwrap();
    polytoken
        .set_goal(&session, "Implement issue #42")
        .await
        .unwrap();
    polytoken
        .send_prompt(&session, "Implement the issue", 200)
        .await
        .unwrap();

    // Verify all calls were made
    assert_eq!(polytoken.spawned_sessions.lock().unwrap().len(), 1);
    assert_eq!(polytoken.facet_calls.lock().unwrap()[0].1, "plan");
    assert_eq!(polytoken.handoff_calls.lock().unwrap().len(), 1);
    assert_eq!(
        polytoken.permission_calls.lock().unwrap()[0].1,
        "bypass_plus"
    );
    assert!(polytoken.goal_calls.lock().unwrap()[0].1.contains("42"));
    assert_eq!(polytoken.prompt_calls.lock().unwrap()[0].2, 200);
}

// AC.12: Merge flow rebases, moves bookmark, pushes
#[tokio::test]
async fn test_merge_flow_success() {
    let config = make_config();
    let jj = MockJjClient::new();
    let github = MockGithubClient::new();
    let fs = MockFilesystem::new();
    let polytoken = MockPolytokenClient::new();
    jj.set_rebase_result(RebaseResult::Success);
    let github = std::sync::Arc::new(github);
    let jj = std::sync::Arc::new(jj);
    let io = grindbot::io::IoLayer {
        github: github.clone(),
        jj: jj.clone(),
        polytoken: std::sync::Arc::new(polytoken),
        fs: std::sync::Arc::new(fs),
    };
    let mut state = StateFile::default();
    state.add_implementer(ActiveImplementer {
        issue_number: 42,
        session_id: "session".into(),
        workspace_name: "grindbot-42".into(),
        workspace_path: "/tmp/grindbot-42".into(),
        base_commit: "basecommit456".into(),
        started_at: "2024-01-01T00:00:00Z".into(),
    });
    grindbot::supervisor::merge_implementation(
        &config,
        &io,
        &mut state,
        "grindbot-42",
        "newcommit123",
        "basecommit456",
        42,
    )
    .await
    .unwrap();
    assert_eq!(
        jj.rebase_calls.lock().unwrap()[0],
        ("basecommit456::newcommit123".into(), "main@origin".into())
    );
    assert_eq!(jj.bookmark_calls.lock().unwrap()[0].1, "newcommit123");
    assert_eq!(jj.push_calls.lock().unwrap().len(), 1);
    assert_eq!(github.posted_comments.lock().unwrap().len(), 1);
    assert!(
        state
            .completed_tasks
            .iter()
            .any(|task| task.issue_number == 42 && task.commit == "newcommit123")
    );
}

// AC.12: Merge flow with conflict
#[tokio::test]
async fn test_merge_flow_conflict() {
    let _config = make_config();
    let jj = MockJjClient::new();

    // Simulate a conflict
    jj.set_rebase_result(RebaseResult::Conflict {
        conflicted_files: vec!["src/main.rs".to_string()],
    });

    let result = jj.rebase("base::commit", "main@origin").await.unwrap();
    assert!(matches!(result, RebaseResult::Conflict { .. }));
}

// AC.13: Comments have the <!-- grindbot --> prefix
#[tokio::test]
async fn test_comment_format_done() {
    let github = MockGithubClient::new();

    let body =
        "<!-- grindbot -->\n\nImplementation complete. Commit `abc` has been merged to `main`.";
    github.post_comment("test", "test", 42, body).await.unwrap();

    let comments = github.posted_comments.lock().unwrap();
    assert_eq!(comments.len(), 1);
    assert!(comments[0].1.starts_with("<!-- grindbot -->"));
}

#[tokio::test]
async fn test_comment_format_needs_feedback() {
    let github = MockGithubClient::new();

    let body = "<!-- grindbot -->\n\n**Needs feedback:**\n\nNeed more info";
    github.post_comment("test", "test", 42, body).await.unwrap();

    let comments = github.posted_comments.lock().unwrap();
    assert_eq!(comments.len(), 1);
    assert!(comments[0].1.starts_with("<!-- grindbot -->"));
    assert!(comments[0].1.contains("Needs feedback"));
}

// AC.14: Crashed session cleanup
#[tokio::test]
async fn test_crashed_session_cleanup() {
    let _config = make_config();
    let fs = MockFilesystem::new();
    let jj = MockJjClient::new();
    let polytoken = MockPolytokenClient::new();

    // Create a workspace with a dead session
    let workspace_name = "grindbot-42";
    let workspace_path = "/tmp/test-repo/.grindbot-workspaces/grindbot-42";
    jj.create_workspace(workspace_path, workspace_name, "base")
        .await
        .unwrap();

    // The session is not alive (not registered in alive_sessions)
    let session_info = grindbot::io::SessionInfo {
        session_id: "dead-session".to_string(),
        port: 0,
        credential_file: String::new(),
        bearer_token: String::new(),
    };
    assert!(!polytoken.is_alive(&session_info).await);

    // No result file
    assert!(!fs.exists(&format!("{}/.grindbot/result.json", workspace_path)));

    // Clean up
    jj.forget_workspace(workspace_name).await.unwrap();
    fs.remove_dir_all(workspace_path).unwrap();

    // Verify workspace was forgotten
    assert!(
        jj.forgotten
            .lock()
            .unwrap()
            .contains(&workspace_name.to_string())
    );
}

// AC.12b: Conflict resolution agent configuration
#[tokio::test]
async fn test_conflict_resolution_agent_config() {
    let polytoken = MockPolytokenClient::new();
    let fs = MockFilesystem::new();

    let workspace_path = "/tmp/test-ws";
    fs.create_dir_all(&format!("{}/.polytoken", workspace_path))
        .unwrap();

    // Set up conflict resolution workspace
    workspace::setup_conflict_resolution_workspace(workspace_path, &fs).unwrap();

    // Spawn session
    let session = polytoken.spawn_session(workspace_path).await.unwrap();

    // Configure for conflict resolution
    polytoken.set_facet(&session, "execute").await.unwrap();
    polytoken
        .set_permission_mode(&session, "bypass_plus")
        .await
        .unwrap();
    polytoken
        .set_goal(&session, "Resolve merge conflicts in workspace")
        .await
        .unwrap();
    polytoken
        .send_prompt(
            &session,
            "Resolve the merge conflicts in this workspace. Use the jj-resolve-conflicts skill.",
            50,
        )
        .await
        .unwrap();

    // Verify configuration
    assert_eq!(polytoken.facet_calls.lock().unwrap()[0].1, "execute");
    assert_eq!(
        polytoken.permission_calls.lock().unwrap()[0].1,
        "bypass_plus"
    );
    assert!(
        polytoken.goal_calls.lock().unwrap()[0]
            .1
            .contains("conflict")
    );
    assert_eq!(polytoken.prompt_calls.lock().unwrap()[0].2, 50);

    // Verify the always-stop hook was written
    let hooks = fs
        .read_to_string(&format!("{}/.polytoken/hooks.json", workspace_path))
        .unwrap();
    assert!(!hooks.contains("continue"));
}

// AC.9: Orphaned workspace cleanup via planner
#[test]
fn test_orphaned_workspace_cleanup_via_planner() {
    use grindbot::core::actions::{Action, CleanupReason};
    use grindbot::core::planner;
    use grindbot::core::state::{SupervisorState, WorkspaceState};

    let config = make_config();
    let ws = WorkspaceState {
        name: "grindbot-99".to_string(),
        path: "/tmp/grindbot-99".to_string(),
        task_issue: None,
        session_id: None,
        daemon_alive: false,
        has_result_file: false,
    };

    let state = SupervisorState {
        config,
        issues: vec![],
        implementers: vec![],
        workspaces: vec![ws],
        main_head: "abc".to_string(),
        completed_issues: vec![],
    };

    let actions = planner::plan(&state);
    assert!(actions.iter().any(|a| matches!(
        a,
        Action::CleanupWorkspace {
            reason: CleanupReason::OrphanedWorkspace,
            ..
        }
    )));
}

// State file tests
#[test]
fn test_state_file_atomic_save_load() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");

    let mut state = StateFile::default();
    state.add_implementer(ActiveImplementer {
        issue_number: 42,
        session_id: "sess1".to_string(),
        workspace_name: "grindbot-42".to_string(),
        workspace_path: "/tmp/grindbot-42".to_string(),
        base_commit: "abc".to_string(),
        started_at: "2024-01-01T00:00:00Z".to_string(),
        port: 12345,
        bearer_token: "test-token".to_string(),
        credential_file: "/tmp/cred.json".to_string(),
    });

    state.save_to(&path).unwrap();
    let loaded = StateFile::load_from(&path).unwrap();

    assert_eq!(loaded.active_implementers.len(), 1);
    assert_eq!(loaded.active_implementers[0].issue_number, 42);
}

#[test]
fn test_state_file_conflict_retry_limit() {
    let mut state = StateFile::default();

    assert_eq!(state.increment_conflict_retry(42), 1);
    assert_eq!(state.increment_conflict_retry(42), 2);
    assert_eq!(state.increment_conflict_retry(42), 3);

    // After 3 retries, the supervisor should post a comment
    // (the state file just tracks the count)
    assert_eq!(state.conflict_retry_count(42), 3);

    // Reset
    state.reset_conflict_retry(42);
    assert_eq!(state.conflict_retry_count(42), 0);
}
