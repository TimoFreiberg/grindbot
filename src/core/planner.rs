use crate::core::actions::{Action, CleanupReason};
use crate::core::filters::is_eligible;
use crate::core::state::{ImplementerResult, ImplementerStatus, SupervisorState};

/// The pure decision function. Takes a state snapshot and returns actions to execute.
///
/// This function performs NO I/O. It is fully deterministic and property-testable.
pub fn plan(state: &SupervisorState) -> Vec<Action> {
    let mut actions = Vec::new();

    // Track issues we've already emitted actions for in this cycle
    // (to avoid duplicate StartImplementer for the same issue).
    let mut handled_issues: std::collections::HashSet<u64> = std::collections::HashSet::new();

    // 1. Clean up dead sessions (crashed or daemon not alive).
    // Only flag workspaces that had an active session (session_id is Some).
    // Orphaned workspaces (no session_id) are handled in step 3.
    for ws in &state.workspaces {
        if ws.session_id.is_some() && !ws.daemon_alive && !ws.has_result_file {
            actions.push(Action::CleanupWorkspace {
                workspace_name: ws.name.clone(),
                reason: CleanupReason::SessionCrashed,
            });
        }
    }

    // 2. Process finished sessions
    for imp in &state.implementers {
        if let ImplementerStatus::Malformed { error } = &imp.status {
            handled_issues.insert(imp.issue_number);
            actions.push(Action::PostComment {
                issue_number: imp.issue_number,
                body: format!("<!-- grindbot -->\\n\\n**Invalid handoff:**\\n\\n{}", error),
            });
            actions.push(Action::CleanupWorkspace {
                workspace_name: imp.workspace_name.clone(),
                reason: CleanupReason::MalformedHandoff,
            });
        }
        if let ImplementerStatus::Finished(result) = &imp.status {
            handled_issues.insert(imp.issue_number);
            match result {
                ImplementerResult::Done { commit } => {
                    actions.push(Action::MergeImplementation {
                        workspace_name: imp.workspace_name.clone(),
                        commit: commit.clone(),
                        base_commit: imp.base_commit.clone(),
                        issue_number: imp.issue_number,
                    });
                }
                ImplementerResult::NeedsFeedback { message } => {
                    actions.push(Action::PostComment {
                        issue_number: imp.issue_number,
                        body: format!("<!-- grindbot -->\n\n**Needs feedback:**\n\n{}", message),
                    });
                    actions.push(Action::CleanupWorkspace {
                        workspace_name: imp.workspace_name.clone(),
                        reason: CleanupReason::SessionFinished,
                    });
                }
            }
        }
    }

    // Also handle crashed implementers that have result files (rare but possible
    // if the daemon crashed after writing the result but before clean exit).
    for imp in &state.implementers {
        if matches!(imp.status, ImplementerStatus::Crashed) {
            handled_issues.insert(imp.issue_number);
            // Check if the corresponding workspace has a result file
            let ws = state
                .workspaces
                .iter()
                .find(|w| w.name == imp.workspace_name);
            if let Some(ws) = ws {
                if ws.has_result_file {
                    // Process as finished — we need to read the result.
                    // Since we can't read files in the pure core, we emit
                    // a cleanup action. The supervisor will read the result
                    // file during execution and handle accordingly.
                    // For the pure core, we treat crashed+result as needing
                    // cleanup (the supervisor's execute layer handles the
                    // result file reading).
                    actions.push(Action::CleanupWorkspace {
                        workspace_name: imp.workspace_name.clone(),
                        reason: CleanupReason::SessionCrashed,
                    });
                } else {
                    actions.push(Action::CleanupWorkspace {
                        workspace_name: imp.workspace_name.clone(),
                        reason: CleanupReason::SessionCrashed,
                    });
                }
            }
        }
    }

    // 3. Clean up orphaned workspaces (grindbot- prefix, no active session, no result file)
    for ws in &state.workspaces {
        if ws.name.starts_with(&state.config.workspace.prefix)
            && ws.session_id.is_none()
            && !ws.has_result_file
        {
            // Only flag as orphaned if not already handled above
            let already_handled = actions.iter().any(|a| match a {
                Action::CleanupWorkspace { workspace_name, .. } => *workspace_name == ws.name,
                _ => false,
            });
            if !already_handled {
                actions.push(Action::CleanupWorkspace {
                    workspace_name: ws.name.clone(),
                    reason: CleanupReason::OrphanedWorkspace,
                });
            }
        }
    }

    // 4. Start new implementers if we have capacity
    let active_count = state.active_count();
    if active_count < state.config.supervisor.max_parallelism {
        let active_issues = state.active_issues();
        let slots_available = state.config.supervisor.max_parallelism - active_count;

        // Filter eligible issues
        let mut eligible: Vec<&crate::core::state::Issue> = state
            .issues
            .iter()
            .filter(|issue| {
                !handled_issues.contains(&issue.number)
                    && is_eligible(
                        issue,
                        &state.config,
                        &active_issues,
                        &state.completed_issues,
                    )
            })
            .collect();

        // Sort by creation date (FIFO — oldest first)
        eligible.sort_by_key(|i| i.created_at);

        let mut started_this_cycle: std::collections::HashSet<u64> =
            std::collections::HashSet::new();
        for issue in eligible.into_iter() {
            if started_this_cycle.len() >= slots_available {
                break;
            }
            // Skip if we already started an implementer for this issue this cycle
            if started_this_cycle.contains(&issue.number) {
                continue;
            }
            started_this_cycle.insert(issue.number);
            let workspace_name = state.config.workspace_name(issue.number);
            actions.push(Action::StartImplementer {
                issue: issue.clone(),
                workspace_name,
                base_commit: state.main_head.clone(),
            });
        }
    }

    // If no actions were produced, emit Noop
    if actions.is_empty() {
        actions.push(Action::Noop);
    }

    actions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::core::state::{
        Comment, ImplementerResult, ImplementerState, ImplementerStatus, Issue, WorkspaceState,
    };

    fn make_config(max_parallelism: usize, allowlist: Vec<String>) -> Config {
        Config {
            github: crate::config::GithubConfig {
                owner: "test".to_string(),
                repo: "test".to_string(),
                allowlist,
            },
            supervisor: crate::config::SupervisorConfig {
                max_parallelism,
                poll_interval_secs: 30,
                base_branch: "main".to_string(),
                merge_lock_timeout_secs: 1800,
                final_check_command: None,
                stall_threshold_cycles: 5,
                log_interval_secs: 300,
            },
            ..Config::default()
        }
    }

    fn make_issue(number: u64, author: &str) -> Issue {
        Issue {
            number,
            title: format!("Issue {}", number),
            body: "Body".to_string(),
            author: author.to_string(),
            created_at: jiff::civil::date(2024, 1, number.min(28) as i8)
                .at(0, 0, 0, 0)
                .in_tz("UTC")
                .unwrap()
                .timestamp(),
            updated_at: jiff::Timestamp::now(),
            comments: vec![],
        }
    }

    fn make_state(
        config: Config,
        issues: Vec<Issue>,
        implementers: Vec<ImplementerState>,
        workspaces: Vec<WorkspaceState>,
    ) -> SupervisorState {
        SupervisorState {
            config,
            issues,
            implementers,
            workspaces,
            main_head: "abc123".to_string(),
            completed_issues: vec![],
        }
    }

    #[test]
    fn test_noop_when_no_eligible_issues() {
        let config = make_config(2, vec!["alice".to_string()]);
        let state = make_state(config, vec![], vec![], vec![]);
        let actions = plan(&state);
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], Action::Noop));
    }

    #[test]
    fn test_starts_implementer_for_eligible_issue() {
        let config = make_config(2, vec!["alice".to_string()]);
        let issue = make_issue(1, "alice");
        let state = make_state(config, vec![issue], vec![], vec![]);
        let actions = plan(&state);
        assert_eq!(actions.len(), 1);
        assert!(matches!(
            &actions[0],
            Action::StartImplementer { issue, .. } if issue.number == 1
        ));
    }

    #[test]
    fn test_respects_max_parallelism() {
        let config = make_config(1, vec!["alice".to_string()]);
        let issues = vec![make_issue(1, "alice"), make_issue(2, "alice")];
        let state = make_state(config, issues, vec![], vec![]);
        let actions = plan(&state);
        let start_count = actions
            .iter()
            .filter(|a| matches!(a, Action::StartImplementer { .. }))
            .count();
        assert_eq!(start_count, 1);
    }

    #[test]
    fn test_does_not_start_for_active_issue() {
        let config = make_config(2, vec!["alice".to_string()]);
        let issue = make_issue(1, "alice");
        let imp = ImplementerState {
            issue_number: 1,
            session_id: "sess1".to_string(),
            workspace_name: "grindbot-1".to_string(),
            workspace_path: "/tmp/grindbot-1".to_string(),
            base_commit: "abc".to_string(),
            started_at: jiff::Timestamp::now(),
            status: ImplementerStatus::Running,
            used_tokens: None,
            limit_tokens: None,
            stall_cycles: 0,
            most_recent_assistant_text: None,
        };
        let state = make_state(config, vec![issue], vec![imp], vec![]);
        let actions = plan(&state);
        assert!(actions.iter().all(|a| !matches!(
            a,
            Action::StartImplementer { issue, .. } if issue.number == 1
        )));
    }

    #[test]
    fn test_does_not_start_for_completed_issue() {
        let config = make_config(2, vec!["alice".to_string()]);
        let issue = make_issue(1, "alice");
        let mut state = make_state(config, vec![issue], vec![], vec![]);
        state.completed_issues = vec![1];
        let actions = plan(&state);
        assert!(actions.iter().all(|a| !matches!(
            a,
            Action::StartImplementer { issue, .. } if issue.number == 1
        )));
    }

    #[test]
    fn test_cleans_up_crashed_workspace() {
        let config = make_config(2, vec!["alice".to_string()]);
        let ws = WorkspaceState {
            name: "grindbot-1".to_string(),
            path: "/tmp/grindbot-1".to_string(),
            task_issue: Some(1),
            session_id: Some("sess1".to_string()),
            daemon_alive: false,
            has_result_file: false,
        };
        let state = make_state(config, vec![], vec![], vec![ws]);
        let actions = plan(&state);
        assert!(actions.iter().any(|a| matches!(
            a,
            Action::CleanupWorkspace {
                reason: CleanupReason::SessionCrashed,
                ..
            }
        )));
    }

    #[test]
    fn test_cleans_up_orphaned_workspace() {
        let config = make_config(2, vec!["alice".to_string()]);
        let ws = WorkspaceState {
            name: "grindbot-99".to_string(),
            path: "/tmp/grindbot-99".to_string(),
            task_issue: None,
            session_id: None,
            daemon_alive: false,
            has_result_file: false,
        };
        let state = make_state(config, vec![], vec![], vec![ws]);
        let actions = plan(&state);
        assert!(actions.iter().any(|a| matches!(
            a,
            Action::CleanupWorkspace {
                reason: CleanupReason::OrphanedWorkspace,
                ..
            }
        )));
    }

    #[test]
    fn test_processes_finished_done() {
        let config = make_config(2, vec!["alice".to_string()]);
        let imp = ImplementerState {
            issue_number: 1,
            session_id: "sess1".to_string(),
            workspace_name: "grindbot-1".to_string(),
            workspace_path: "/tmp/grindbot-1".to_string(),
            base_commit: "base".to_string(),
            started_at: jiff::Timestamp::now(),
            status: ImplementerStatus::Finished(ImplementerResult::Done {
                commit: "newcommit".to_string(),
            }),
            used_tokens: None,
            limit_tokens: None,
            stall_cycles: 0,
            most_recent_assistant_text: None,
        };
        let state = make_state(config, vec![], vec![imp], vec![]);
        let actions = plan(&state);
        assert!(actions.iter().any(|a| matches!(
            a,
            Action::MergeImplementation { commit, .. } if commit == "newcommit"
        )));
    }

    #[test]
    fn test_processes_finished_needs_feedback() {
        let config = make_config(2, vec!["alice".to_string()]);
        let imp = ImplementerState {
            issue_number: 1,
            session_id: "sess1".to_string(),
            workspace_name: "grindbot-1".to_string(),
            workspace_path: "/tmp/grindbot-1".to_string(),
            base_commit: "base".to_string(),
            started_at: jiff::Timestamp::now(),
            status: ImplementerStatus::Finished(ImplementerResult::NeedsFeedback {
                message: "Need more info".to_string(),
            }),
            used_tokens: None,
            limit_tokens: None,
            stall_cycles: 0,
            most_recent_assistant_text: None,
        };
        let state = make_state(config, vec![], vec![imp], vec![]);
        let actions = plan(&state);
        assert!(actions.iter().any(|a| matches!(
            a,
            Action::PostComment {
                issue_number: 1,
                ..
            }
        )));
        assert!(actions.iter().any(|a| matches!(
            a,
            Action::CleanupWorkspace {
                reason: CleanupReason::SessionFinished,
                ..
            }
        )));
    }

    #[test]
    fn test_fifo_ordering() {
        let config = make_config(2, vec!["alice".to_string()]);
        // Issue 2 created before issue 1
        let issue2 = Issue {
            number: 2,
            title: "Issue 2".to_string(),
            body: "Body".to_string(),
            author: "alice".to_string(),
            created_at: "2024-01-01T00:00:00Z".parse::<jiff::Timestamp>().unwrap(),
            updated_at: jiff::Timestamp::now(),
            comments: vec![],
        };
        let issue1 = Issue {
            number: 1,
            title: "Issue 1".to_string(),
            body: "Body".to_string(),
            author: "alice".to_string(),
            created_at: "2024-01-02T00:00:00Z".parse::<jiff::Timestamp>().unwrap(),
            updated_at: jiff::Timestamp::now(),
            comments: vec![],
        };
        let state = make_state(config, vec![issue1, issue2], vec![], vec![]);
        let actions = plan(&state);
        // First StartImplementer should be for issue 2 (older)
        let first_start = actions
            .iter()
            .find(|a| matches!(a, Action::StartImplementer { .. }));
        assert!(matches!(
            first_start,
            Some(Action::StartImplementer { issue, .. }) if issue.number == 2
        ));
    }

    #[test]
    fn test_does_not_start_for_supervisor_last_comment() {
        let config = make_config(2, vec!["alice".to_string()]);
        let issue = Issue {
            number: 1,
            title: "Issue 1".to_string(),
            body: "Body".to_string(),
            author: "alice".to_string(),
            created_at: jiff::Timestamp::now(),
            updated_at: jiff::Timestamp::now(),
            comments: vec![Comment {
                author: "grindbot".to_string(),
                body: "<!-- grindbot --> Done".to_string(),
                created_at: jiff::Timestamp::now(),
                is_supervisor: true,
            }],
        };
        let state = make_state(config, vec![issue], vec![], vec![]);
        let actions = plan(&state);
        assert!(actions.iter().all(|a| !matches!(
            a,
            Action::StartImplementer { issue, .. } if issue.number == 1
        )));
    }

    #[test]
    fn test_never_starts_duplicate_for_same_issue() {
        let config = make_config(2, vec!["alice".to_string()]);
        // Two issues with the same number (shouldn't happen in practice, but test the guard)
        let issue = make_issue(1, "alice");
        let state = make_state(config, vec![issue.clone(), issue], vec![], vec![]);
        let actions = plan(&state);
        let start_count = actions
            .iter()
            .filter(|a| matches!(a, Action::StartImplementer { issue, .. } if issue.number == 1))
            .count();
        assert_eq!(start_count, 1);
    }
}
