//! Property-based tests for the core planner (invariants 1-9).

use grindbot::config::Config;
use grindbot::core::actions::{Action, CleanupReason};
use grindbot::core::planner;
use grindbot::core::state::{
    ImplementerResult, ImplementerState, ImplementerStatus, Issue, SupervisorState, WorkspaceState,
};
use hegel::generators as gs;
use hegel::{Generator, TestCase};

fn datetime_generator() -> impl Generator<jiff::Timestamp> {
    gs::integers::<i64>()
        .min_value(0)
        .max_value(365 * 50)
        .map(|days| {
            jiff::civil::date(2024, 1, 1).at(0, 0, 0, 0).in_tz("UTC").unwrap().timestamp()
                + jiff::Span::new().hours(days * 24)
        })
}

fn issue_generator() -> impl Generator<Issue> {
    hegel::tuples!(
        gs::integers::<u64>().min_value(1).max_value(1000),
        gs::from_regex("[A-Za-z ]{5,30}"),
        gs::from_regex("[a-z ]{5,100}"),
        gs::from_regex("[a-z]{3,10}"),
        datetime_generator(),
        datetime_generator(),
        gs::vecs(
            hegel::tuples!(
                gs::from_regex("[a-z]{3,8}"),
                gs::from_regex("[a-z ]{5,50}"),
                datetime_generator(),
                gs::booleans(),
            )
            .map(|(author, body, created_at, is_supervisor)| {
                grindbot::core::state::Comment {
                    author,
                    body,
                    created_at,
                    is_supervisor,
                }
            })
        )
        .max_size(5),
    )
    .map(
        |(number, title, body, author, created_at, updated_at, comments)| Issue {
            number,
            title,
            body,
            author,
            created_at,
            updated_at,
            comments,
        },
    )
}

#[hegel::composite]
fn config_generator(tc: TestCase) -> Config {
    Config {
        github: grindbot::config::GithubConfig {
            owner: "test".into(),
            repo: "test".into(),
            allowlist: vec![tc.draw(gs::from_regex("[a-z]{3,8}"))],
        },
        supervisor: grindbot::config::SupervisorConfig {
            max_parallelism: tc.draw(gs::integers::<usize>().min_value(1).max_value(4)),
            poll_interval_secs: 30,
            base_branch: "main".into(),
            merge_lock_timeout_secs: 1800,
            final_check_command: None,
            stall_threshold_cycles: 5,
        },
        ..Config::default()
    }
}

#[hegel::composite]
fn linked_state(tc: TestCase) -> SupervisorState {
    let config = tc.draw(config_generator());
    let mut issues = tc.draw(gs::vecs(issue_generator()).max_size(8));
    if issues.is_empty() {
        issues.push(tc.draw(issue_generator()));
    }
    let main_head = "abcdef12".to_string();
    let completed_issues =
        tc.draw(gs::vecs(gs::integers::<u64>().min_value(1).max_value(1000)).max_size(4));
    let mut implementers = Vec::new();
    let mut workspaces = Vec::new();
    if tc.draw(gs::booleans()) {
        let issue = &issues[0];
        let workspace_name = "grindbot-linked".to_string();
        let workspace_path = "/tmp/grindbot-linked".to_string();
        implementers.push(ImplementerState {
            issue_number: issue.number,
            session_id: "session-linked".into(),
            workspace_name: workspace_name.clone(),
            workspace_path: workspace_path.clone(),
            base_commit: main_head.clone(),
            started_at: issue.created_at,
            status: ImplementerStatus::Running,
            used_tokens: None,
            limit_tokens: None,
            stall_cycles: 0,
            most_recent_assistant_text: None,
        });
        workspaces.push(WorkspaceState {
            name: workspace_name,
            path: workspace_path,
            task_issue: Some(issue.number),
            session_id: Some("session-linked".into()),
            daemon_alive: true,
            has_result_file: false,
        });
    }
    SupervisorState {
        config,
        issues,
        implementers,
        workspaces,
        main_head,
        completed_issues,
    }
}

fn start_actions(actions: &[Action]) -> Vec<&Action> {
    actions
        .iter()
        .filter(|a| matches!(a, Action::StartImplementer { .. }))
        .collect()
}

#[hegel::test]
fn prop_never_exceeds_max_parallelism(tc: TestCase) {
    let state = tc.draw(linked_state());
    let actions = planner::plan(&state);
    assert!(
        start_actions(&actions).len()
            <= state
                .config
                .supervisor
                .max_parallelism
                .saturating_sub(state.active_count())
    );
}

#[hegel::test]
fn prop_never_starts_active_or_completed_task(tc: TestCase) {
    let state = tc.draw(linked_state());
    let active = state.active_issues();
    for action in planner::plan(&state) {
        if let Action::StartImplementer { issue, .. } = action {
            assert!(!active.contains(&issue.number));
            assert!(!state.completed_issues.contains(&issue.number));
        }
    }
}

#[hegel::test]
fn prop_never_starts_ineligible_issue(tc: TestCase) {
    let state = tc.draw(linked_state());
    for action in planner::plan(&state) {
        if let Action::StartImplementer { issue, .. } = action {
            assert!(state.config.github.allowlist.contains(&issue.author));
            assert!(
                !issue
                    .comments
                    .last()
                    .is_some_and(|comment| comment.is_supervisor)
            );
        }
    }
}

#[hegel::test]
fn prop_crashed_sessions_are_cleaned_exactly_once(tc: TestCase) {
    let mut state = tc.draw(linked_state());
    state.workspaces.push(WorkspaceState {
        name: "grindbot-crashed".into(),
        path: "/tmp/grindbot-crashed".into(),
        task_issue: Some(77),
        session_id: Some("dead".into()),
        daemon_alive: false,
        has_result_file: false,
    });
    let actions = planner::plan(&state);
    assert_eq!(
        actions
            .iter()
            .filter(|action| matches!(action,
        Action::CleanupWorkspace { workspace_name, reason: CleanupReason::SessionCrashed }
        if workspace_name == "grindbot-crashed"))
            .count(),
        1
    );
}

#[test]
fn malformed_handoff_is_diagnostic_cleanup_only() {
    let config = Config::default();
    let state = SupervisorState {
        config,
        issues: vec![],
        implementers: vec![ImplementerState {
            issue_number: 42, session_id: "s".into(), workspace_name: "ws".into(),
            workspace_path: "/tmp/ws".into(), base_commit: "base".into(),
            started_at: jiff::Timestamp::now(), status: ImplementerStatus::Malformed { error: "bad json".into() },
            used_tokens: None, limit_tokens: None, stall_cycles: 0, most_recent_assistant_text: None,
        }],
        workspaces: vec![], main_head: "main".into(), completed_issues: vec![],
    };
    let actions = grindbot::core::planner::plan(&state);
    assert!(!actions.iter().any(|a| matches!(a, grindbot::core::actions::Action::MergeImplementation { .. })));
    assert!(actions.iter().any(|a| matches!(a, grindbot::core::actions::Action::PostComment { issue_number: 42, .. })));
    assert!(actions.iter().any(|a| matches!(a, grindbot::core::actions::Action::CleanupWorkspace { workspace_name, .. } if workspace_name == "ws")));
}

#[hegel::test]
fn prop_finished_sessions_have_complete_actions(tc: TestCase) {
    let mut state = tc.draw(linked_state());
    let issue_number = state.issues[0].number;
    state.implementers.push(ImplementerState {
        issue_number,
        session_id: "finished".into(),
        workspace_name: "grindbot-finished".into(),
        workspace_path: "/tmp/grindbot-finished".into(),
        base_commit: "base123".into(),
        started_at: state.issues[0].created_at,
        status: ImplementerStatus::Finished(ImplementerResult::Done {
            commit: "commit123".into(),
        }),
        used_tokens: None,
        limit_tokens: None,
        stall_cycles: 0,
        most_recent_assistant_text: None,
    });
    let actions = planner::plan(&state);
    assert!(actions.iter().any(|a| matches!(a, Action::MergeImplementation {
        workspace_name, commit, base_commit, issue_number: n
    } if workspace_name == "grindbot-finished" && commit == "commit123" && base_commit == "base123" && *n == issue_number)));
}

#[hegel::test]
fn prop_finished_needs_feedback_posts_and_cleans_up(tc: TestCase) {
    let mut state = tc.draw(linked_state());
    let issue_number = state.issues[0].number;
    state.implementers.push(ImplementerState {
        issue_number,
        session_id: "feedback".into(),
        workspace_name: "grindbot-feedback".into(),
        workspace_path: "/tmp/grindbot-feedback".into(),
        base_commit: "base123".into(),
        started_at: state.issues[0].created_at,
        status: ImplementerStatus::Finished(ImplementerResult::NeedsFeedback {
            message: "need more detail".into(),
        }),
        used_tokens: None,
        limit_tokens: None,
        stall_cycles: 0,
        most_recent_assistant_text: None,
    });
    let actions = planner::plan(&state);
    assert_eq!(actions.iter().filter(|action| matches!(action,
        Action::PostComment { issue_number: n, body } if *n == issue_number && body.contains("need more detail"))).count(), 1);
    assert_eq!(
        actions
            .iter()
            .filter(|action| matches!(action,
        Action::CleanupWorkspace { workspace_name, reason: CleanupReason::SessionFinished }
        if workspace_name == "grindbot-feedback"))
            .count(),
        1
    );
}

#[test]
fn duplicate_running_implementers_are_not_started_again() {
    let mut state = SupervisorState {
        config: Config {
            github: grindbot::config::GithubConfig {
                owner: "test".into(),
                repo: "test".into(),
                allowlist: vec!["alice".into()],
            },
            ..Config::default()
        },
        issues: vec![],
        implementers: vec![],
        workspaces: vec![],
        main_head: "abc".into(),
        completed_issues: vec![],
    };
    for (session_id, workspace_name) in [("one", "grindbot-one"), ("two", "grindbot-two")] {
        state.implementers.push(ImplementerState {
            issue_number: 42,
            session_id: session_id.into(),
            workspace_name: workspace_name.into(),
            workspace_path: format!("/tmp/{workspace_name}"),
            base_commit: "abc".into(),
            started_at: jiff::Timestamp::now(),
            status: ImplementerStatus::Running,
            used_tokens: None,
            limit_tokens: None,
            stall_cycles: 0,
            most_recent_assistant_text: None,
        });
    }
    assert!(
        planner::plan(&state)
            .iter()
            .all(|action| !matches!(action, Action::StartImplementer { .. }))
    );
}

#[test]
fn duplicate_running_implementers_same_workspace_are_not_started_again() {
    let mut state = SupervisorState {
        config: Config {
            github: grindbot::config::GithubConfig {
                owner: "test".into(),
                repo: "test".into(),
                allowlist: vec!["alice".into()],
            },
            supervisor: grindbot::config::SupervisorConfig {
                max_parallelism: 1,
                ..Config::default().supervisor
            },
            ..Config::default()
        },
        issues: vec![],
        implementers: vec![],
        workspaces: vec![],
        main_head: "00a00a0a".into(),
        completed_issues: vec![],
    };
    for (session_id, base_commit) in [("00000aaa", "000000a0"), ("000000aa", "a00000a0")] {
        state.implementers.push(ImplementerState {
            issue_number: 1,
            session_id: session_id.into(),
            workspace_name: "grindbot-0".into(),
            workspace_path: "/tmp/grindbot-0".into(),
            base_commit: base_commit.into(),
            started_at: "2024-01-02T00:00:00Z".parse::<jiff::Timestamp>().unwrap(),
            status: ImplementerStatus::Running,
            used_tokens: None,
            limit_tokens: None,
            stall_cycles: 0,
            most_recent_assistant_text: None,
        });
    }
    assert!(
        planner::plan(&state)
            .iter()
            .all(|action| !matches!(action, Action::StartImplementer { .. }))
    );
}

#[test]
fn idle_state_has_exactly_one_noop() {
    let mut config = Config::default();
    config.github.allowlist = vec!["alice".into()];
    let state = SupervisorState {
        config,
        issues: vec![],
        implementers: vec![],
        workspaces: vec![],
        main_head: "abc".into(),
        completed_issues: vec![],
    };
    let actions = planner::plan(&state);
    assert_eq!(actions.len(), 1);
    assert!(matches!(actions[0], Action::Noop));
}
