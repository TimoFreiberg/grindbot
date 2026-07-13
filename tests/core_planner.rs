//! Property-based tests for the core planner (invariants 1-9).

use grindbot::config::Config;
use grindbot::core::actions::{Action, CleanupReason};
use grindbot::core::planner;
use grindbot::core::state::{
    Comment, ImplementerResult, ImplementerState, ImplementerStatus, Issue, SupervisorState,
    WorkspaceState,
};
use proptest::prelude::*;

fn arb_datetime() -> impl Strategy<Value = chrono::DateTime<chrono::Utc>> {
    (1i64..365 * 50).prop_map(|days| {
        chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2024, 1, 1, 0, 0, 0)
            .unwrap()
            + chrono::Duration::days(days)
    })
}

fn arb_comment() -> impl Strategy<Value = Comment> {
    (
        "[a-z]{3,8}",
        "[a-z ]{5,50}",
        arb_datetime(),
        any::<bool>(),
    )
        .prop_map(|(author, body, created_at, is_supervisor)| Comment {
            author,
            body,
            created_at,
            is_supervisor,
        })
}

fn arb_issue() -> impl Strategy<Value = Issue> {
    (
        1u64..1000,
        "[A-Za-z ]{5,30}",
        "[a-z ]{5,100}",
        "[a-z]{3,10}",
        arb_datetime(),
        arb_datetime(),
        proptest::collection::vec(arb_comment(), 0..5),
    )
        .prop_map(
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

fn arb_implementer_status() -> impl Strategy<Value = ImplementerStatus> {
    prop_oneof![
        Just(ImplementerStatus::Running),
        Just(ImplementerStatus::Crashed),
        "[a-f0-9]{8,40}".prop_map(|commit| {
            ImplementerStatus::Finished(ImplementerResult::Done { commit })
        }),
        "[a-z ]{5,50}".prop_map(|message| {
            ImplementerStatus::Finished(ImplementerResult::NeedsFeedback { message })
        }),
    ]
}

fn arb_implementer() -> impl Strategy<Value = ImplementerState> {
    (
        1u64..1000,
        "[a-f0-9]{8,16}",
        "grindbot-[0-9]{1,4}",
        "/tmp/grindbot-[0-9]{1,4}",
        "[a-f0-9]{8,40}",
        arb_datetime(),
        arb_implementer_status(),
    )
        .prop_map(
            |(issue_number, session_id, workspace_name, workspace_path, base_commit, started_at, status)| {
                ImplementerState {
                    issue_number,
                    session_id,
                    workspace_name,
                    workspace_path,
                    base_commit,
                    started_at,
                    status,
                }
            },
        )
}

fn arb_workspace() -> impl Strategy<Value = WorkspaceState> {
    (
        "grindbot-[0-9]{1,4}",
        "/tmp/grindbot-[0-9]{1,4}",
        proptest::option::of(1u64..1000),
        proptest::option::of("[a-f0-9]{8,16}"),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(|(name, path, task_issue, session_id, daemon_alive, has_result_file)| {
            WorkspaceState {
                name,
                path,
                task_issue,
                session_id,
                daemon_alive,
                has_result_file,
            }
        })
}

fn arb_config() -> impl Strategy<Value = Config> {
    (1usize..5, proptest::collection::vec("[a-z]{3,8}", 1..5)).prop_map(
        |(max_parallelism, allowlist)| Config {
            github: grindbot::config::GithubConfig {
                owner: "test".to_string(),
                repo: "test".to_string(),
                allowlist,
            },
            supervisor: grindbot::config::SupervisorConfig {
                max_parallelism,
                poll_interval_secs: 30,
                base_branch: "main".to_string(),
            },
            ..Config::default()
        },
    )
}

fn arb_state() -> impl Strategy<Value = SupervisorState> {
    (
        arb_config(),
        proptest::collection::vec(arb_issue(), 0..10),
        proptest::collection::vec(arb_implementer(), 0..5),
        proptest::collection::vec(arb_workspace(), 0..5),
        "[a-f0-9]{8,40}",
        proptest::collection::vec(1u64..1000, 0..5),
    )
        .prop_map(
            |(config, issues, implementers, workspaces, main_head, completed_issues)| {
                SupervisorState {
                    config,
                    issues,
                    implementers,
                    workspaces,
                    main_head,
                    completed_issues,
                }
            },
        )
}

// Invariant 1: plan(state) never starts more than max_parallelism implementers in one cycle.
proptest! {
    #[test]
    fn prop_never_exceeds_max_parallelism(state in arb_state()) {
        let actions = planner::plan(&state);
        let started = actions.iter().filter(|a| matches!(a, Action::StartImplementer { .. })).count();
        let active = state.active_count();
        let max = state.config.supervisor.max_parallelism;
        // The planner should never start more than the available slots.
        // If active >= max, no new implementers should be started.
        if active >= max {
            prop_assert!(started == 0, "started {} when active {} >= max {}", started, active, max);
        } else {
            prop_assert!(started <= max - active, "started {} > available slots {}", started, max - active);
        }
    }

    // Invariant 2: plan(state) never starts an implementer for a task that's already active.
    #[test]
    fn prop_never_starts_active_task(state in arb_state()) {
        let active_issues = state.active_issues();
        let actions = planner::plan(&state);
        for action in &actions {
            if let Action::StartImplementer { issue, .. } = action {
                prop_assert!(!active_issues.contains(&issue.number));
            }
        }
    }

    // Invariant 3: plan(state) never starts an implementer for a completed task.
    #[test]
    fn prop_never_starts_completed_task(state in arb_state()) {
        let completed = &state.completed_issues;
        let actions = planner::plan(&state);
        for action in &actions {
            if let Action::StartImplementer { issue, .. } = action {
                prop_assert!(!completed.contains(&issue.number));
            }
        }
    }

    // Invariant 4: plan(state) never starts an implementer for a task where last activity is by supervisor.
    #[test]
    fn prop_never_starts_supervisor_active_task(state in arb_state()) {
        let actions = planner::plan(&state);
        for action in &actions {
            if let Action::StartImplementer { issue, .. } = action {
                let last_by_supervisor = issue.comments.last()
                    .map(|c| c.is_supervisor)
                    .unwrap_or(false);
                prop_assert!(!last_by_supervisor);
            }
        }
    }

    // Invariant 5: plan(state) never starts an implementer for a task whose author is not on the allowlist.
    #[test]
    fn prop_never_starts_non_allowlisted(state in arb_state()) {
        let allowlist = &state.config.github.allowlist;
        let actions = planner::plan(&state);
        for action in &actions {
            if let Action::StartImplementer { issue, .. } = action {
                prop_assert!(allowlist.contains(&issue.author));
            }
        }
    }

    // Invariant 6: When state contains a workspace with daemon_alive=false, session_id=Some,
    // and has_result_file=false, plan produces a CleanupWorkspace{SessionCrashed} action.
    #[test]
    fn prop_cleans_up_crashed_sessions(state in arb_state()) {
        let actions = planner::plan(&state);
        for ws in &state.workspaces {
            if ws.session_id.is_some() && !ws.daemon_alive && !ws.has_result_file {
                let has_cleanup = actions.iter().any(|a| matches!(
                    a,
                    Action::CleanupWorkspace { workspace_name, reason: CleanupReason::SessionCrashed }
                    if *workspace_name == ws.name
                ));
                prop_assert!(has_cleanup);
            }
        }
    }

    // Invariant 7: plan(state) always processes result files from finished sessions.
    #[test]
    fn prop_processes_finished_sessions(state in arb_state()) {
        let actions = planner::plan(&state);
        for imp in &state.implementers {
            if let ImplementerStatus::Finished(result) = &imp.status {
                match result {
                    ImplementerResult::Done { commit, .. } => {
                        let has_merge = actions.iter().any(|a| matches!(
                            a,
                            Action::MergeImplementation { commit: c, .. } if c == commit
                        ));
                        prop_assert!(has_merge);
                    }
                    ImplementerResult::NeedsFeedback { .. } => {
                        let has_comment = actions.iter().any(|a| matches!(
                            a,
                            Action::PostComment { issue_number, .. } if *issue_number == imp.issue_number
                        ));
                        prop_assert!(has_comment);
                    }
                }
            }
        }
    }

    // Invariant 8: plan(state) never starts two implementers for the same issue.
    #[test]
    fn prop_never_starts_duplicate_issue(state in arb_state()) {
        let actions = planner::plan(&state);
        let mut started_issues = std::collections::HashSet::new();
        for action in &actions {
            if let Action::StartImplementer { issue, .. } = action {
                prop_assert!(!started_issues.contains(&issue.number));
                started_issues.insert(issue.number);
            }
        }
    }
}

// Invariant 9: plan(state) produces Noop when there are no eligible issues and no pending cleanup.
#[test]
fn prop_noop_when_idle() {
    let config = Config {
        github: grindbot::config::GithubConfig {
            owner: "test".to_string(),
            repo: "test".to_string(),
            allowlist: vec!["alice".to_string()],
        },
        supervisor: grindbot::config::SupervisorConfig {
            max_parallelism: 2,
            poll_interval_secs: 30,
            base_branch: "main".to_string(),
        },
        ..Config::default()
    };
    let state = SupervisorState {
        config,
        issues: vec![],
        implementers: vec![],
        workspaces: vec![],
        main_head: "abc".to_string(),
        completed_issues: vec![],
    };
    let actions = planner::plan(&state);
    assert!(actions.len() == 1 && matches!(actions[0], Action::Noop));
}
