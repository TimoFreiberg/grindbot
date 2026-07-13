use std::sync::Arc;

use crate::config::Config;
use crate::core::actions::Action;
use crate::core::planner;
use crate::core::state::{
    HandoffResult, ImplementerResult, ImplementerState, ImplementerStatus, Issue, SupervisorState,
    WorkspaceState,
};
use crate::io::github::fetch_comments;
use crate::io::{IoLayer, RebaseResult};
use crate::prompt;
use crate::state_file::{ActiveImplementer, CompletedTask, NeedsFeedbackTask, StateFile};
use crate::workspace;

/// Run the supervisor daemon.
pub async fn run(config: Config) -> anyhow::Result<()> {
    let io = build_io_layer(&config);

    // Load state file
    let mut state_file = StateFile::load(&config)?;

    // Startup cleanup: reconcile workspaces and state file
    startup_cleanup(&config, &io, &mut state_file).await?;
    state_file.save(&config)?;

    loop {
        // 1. Gather state
        let state = match gather_state(&config, &io, &state_file).await {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("failed to gather state: {}", e);
                tokio::time::sleep(std::time::Duration::from_secs(
                    config.supervisor.poll_interval_secs,
                ))
                .await;
                continue;
            }
        };

        // 2. Plan
        let actions = planner::plan(&state);

        // 3. Execute actions
        for action in actions {
            if let Err(e) = execute_action(&action, &config, &io, &mut state_file).await {
                tracing::error!("failed to execute action {:?}: {}", action, e);
            }
            if let Err(e) = state_file.save(&config) {
                tracing::error!("failed to save state file: {}", e);
            }
        }

        // 4. Wait for next poll cycle
        tokio::time::sleep(std::time::Duration::from_secs(
            config.supervisor.poll_interval_secs,
        ))
        .await;
    }
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
    }
}

/// Gather the current state from all I/O sources.
async fn gather_state(
    config: &Config,
    io: &IoLayer,
    state_file: &StateFile,
) -> anyhow::Result<SupervisorState> {
    // Fetch issues from GitHub
    let mut issues = io
        .github
        .list_issues(&config.github.owner, &config.github.repo)
        .await?;

    // Enrich allowlisted issues with comments
    for issue in issues.iter_mut() {
        if config.github.allowlist.contains(&issue.author) {
            if let Ok(comments) =
                fetch_comments(&config.github.owner, &config.github.repo, issue.number).await
            {
                issue.comments = comments;
            }
        }
    }

    // Get current main head
    let main_head = io.jj.current_main().await.unwrap_or_default();

    // Build implementer states from state file + live session checks
    let mut implementers = Vec::new();
    for active in &state_file.active_implementers {
        let session_info = crate::io::SessionInfo {
            session_id: active.session_id.clone(),
            port: 0, // Not stored; the supervisor would need to track this
            credential_file: String::new(),
            bearer_token: String::new(),
        };

        // Check if session is alive
        let alive = io.polytoken.is_alive(&session_info).await;

        // Check for result file
        let result_path = format!("{}/.grindbot/result.json", active.workspace_path);
        let has_result = io.fs.exists(&result_path);

        let status = if !alive && !has_result {
            ImplementerStatus::Crashed
        } else if has_result {
            // Read the result file
            let result_content = io.fs.read_to_string(&result_path).unwrap_or_default();
            match serde_json::from_str::<HandoffResult>(&result_content) {
                Ok(HandoffResult::Done { commit, .. }) => {
                    ImplementerStatus::Finished(ImplementerResult::Done { commit })
                }
                Ok(HandoffResult::NeedsFeedback { message, .. }) => {
                    ImplementerStatus::Finished(ImplementerResult::NeedsFeedback { message })
                }
                Err(_) => ImplementerStatus::Crashed,
            }
        } else {
            ImplementerStatus::Running
        };

        implementers.push(ImplementerState {
            issue_number: active.issue_number,
            session_id: active.session_id.clone(),
            workspace_name: active.workspace_name.clone(),
            workspace_path: active.workspace_path.clone(),
            base_commit: active.base_commit.clone(),
            started_at: chrono::Utc::now(), // Not critical for planning
            status,
        });
    }

    // Build workspace states
    let mut workspaces = Vec::new();
    let all_workspaces = io.jj.list_workspaces().await.unwrap_or_default();
    for ws_name in all_workspaces {
        if !ws_name.starts_with(&config.workspace.prefix) {
            continue;
        }
        let ws_path = workspace::workspace_path(
            config,
            &std::env::current_dir()
                .unwrap_or_default()
                .to_string_lossy(),
            ws_name
                .strip_prefix(&format!("{}-", config.workspace.prefix))
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0),
        )
        .to_string_lossy()
        .to_string();

        let session_id = state_file
            .active_implementers
            .iter()
            .find(|i| i.workspace_name == ws_name)
            .map(|i| i.session_id.clone());

        let daemon_alive = if let Some(ref sid) = session_id {
            let session_info = crate::io::SessionInfo {
                session_id: sid.clone(),
                port: 0,
                credential_file: String::new(),
                bearer_token: String::new(),
            };
            io.polytoken.is_alive(&session_info).await
        } else {
            false
        };

        let result_path = format!("{}/.grindbot/result.json", ws_path);
        let has_result_file = io.fs.exists(&result_path);

        let task_issue = state_file
            .active_implementers
            .iter()
            .find(|i| i.workspace_name == ws_name)
            .map(|i| i.issue_number);

        workspaces.push(WorkspaceState {
            name: ws_name,
            path: ws_path,
            task_issue,
            session_id,
            daemon_alive,
            has_result_file,
        });
    }

    Ok(SupervisorState {
        config: config.clone(),
        issues,
        implementers,
        workspaces,
        main_head,
        completed_issues: state_file.completed_issues(),
    })
}

/// Startup cleanup: reconcile workspaces and state file.
async fn startup_cleanup(
    config: &Config,
    io: &IoLayer,
    state_file: &mut StateFile,
) -> anyhow::Result<()> {
    let all_workspaces = io.jj.list_workspaces().await.unwrap_or_default();

    for ws_name in &all_workspaces {
        if !ws_name.starts_with(&config.workspace.prefix) {
            continue;
        }

        // Check if this workspace has an active session in the state file.
        // Clone the relevant fields out to avoid holding an immutable borrow
        // of state_file across the mutable borrows below.
        let active = state_file
            .active_implementers
            .iter()
            .find(|i| i.workspace_name == *ws_name)
            .cloned();

        if let Some(active) = active {
            // Check if session is alive
            let session_info = crate::io::SessionInfo {
                session_id: active.session_id.clone(),
                port: 0,
                credential_file: String::new(),
                bearer_token: String::new(),
            };

            if !io.polytoken.is_alive(&session_info).await {
                tracing::info!("startup cleanup: dead session for {}", ws_name);
                // Process result file if it exists
                let result_path = format!("{}/.grindbot/result.json", active.workspace_path);
                if io.fs.exists(&result_path) {
                    if let Ok(content) = io.fs.read_to_string(&result_path) {
                        if let Ok(result) = serde_json::from_str::<HandoffResult>(&content) {
                            process_result(
                                config,
                                io,
                                state_file,
                                active.issue_number,
                                &active.workspace_name,
                                &active.workspace_path,
                                &active.base_commit,
                                &result,
                            )
                            .await?;
                        }
                    }
                }
                // Clean up
                cleanup_workspace_action(config, io, &active.workspace_name, &active.workspace_path)
                    .await?;
                state_file.remove_implementer(&active.workspace_name);
            }
        } else {
            // Orphaned workspace
            tracing::info!("startup cleanup: orphaned workspace {}", ws_name);
            let ws_path = format!(
                "{}/{}/{}",
                std::env::current_dir()
                    .unwrap_or_default()
                    .to_string_lossy(),
                config.workspace.workspaces_dir,
                ws_name
            );
            cleanup_workspace_action(config, io, ws_name, &ws_path).await?;
        }
    }

    Ok(())
}

/// Execute a single action.
async fn execute_action(
    action: &Action,
    config: &Config,
    io: &IoLayer,
    state_file: &mut StateFile,
) -> anyhow::Result<()> {
    match action {
        Action::Noop => {
            tracing::debug!("no actions this cycle");
        }
        Action::StartImplementer {
            issue,
            workspace_name,
            base_commit,
        } => {
            tracing::info!(
                "starting implementer for issue #{} in workspace {}",
                issue.number,
                workspace_name
            );
            start_implementer(config, io, state_file, issue, workspace_name, base_commit).await?;
        }
        Action::CleanupWorkspace {
            workspace_name,
            reason,
        } => {
            tracing::info!(
                "cleaning up workspace {} (reason: {:?})",
                workspace_name,
                reason
            );
            let ws_path = find_workspace_path(config, state_file, workspace_name);
            if let Some(ref path) = ws_path {
                cleanup_workspace_action(config, io, workspace_name, path).await?;
            }
            state_file.remove_implementer(workspace_name);
        }
        Action::MergeImplementation {
            workspace_name,
            commit,
            base_commit,
            issue_number,
        } => {
            tracing::info!(
                "merging implementation from {} (commit: {})",
                workspace_name,
                commit
            );
            merge_implementation(
                config,
                io,
                state_file,
                workspace_name,
                commit,
                base_commit,
                *issue_number,
            )
            .await?;
        }
        Action::PostComment { issue_number, body } => {
            io.github
                .post_comment(
                    &config.github.owner,
                    &config.github.repo,
                    *issue_number,
                    body,
                )
                .await?;
            tracing::info!("posted comment on issue #{}", issue_number);
        }
        Action::ResolveConflict {
            workspace_name,
            commit,
            base_commit,
            issue_number,
        } => {
            resolve_conflict(
                config,
                io,
                state_file,
                workspace_name,
                commit,
                base_commit,
                *issue_number,
            )
            .await?;
        }
        Action::DiscardImplementation {
            workspace_name,
            issue_number,
        } => {
            tracing::info!(
                "discarding implementation from {} (issue #{})",
                workspace_name,
                issue_number
            );
            let ws_path = find_workspace_path(config, state_file, workspace_name);
            if let Some(ref path) = ws_path {
                cleanup_workspace_action(config, io, workspace_name, path).await?;
            }
            state_file.remove_implementer(workspace_name);
        }
        Action::TerminateSession { session_id } => {
            let session_info = crate::io::SessionInfo {
                session_id: session_id.clone(),
                port: 0,
                credential_file: String::new(),
                bearer_token: String::new(),
            };
            if let Err(e) = io.polytoken.terminate(&session_info).await {
                tracing::warn!("failed to terminate session {}: {}", session_id, e);
            }
        }
        Action::PushToRemote => {
            if let Err(e) = io
                .jj
                .push("origin", &config.supervisor.base_branch)
                .await
            {
                tracing::error!("failed to push to remote: {}", e);
            }
        }
    }
    Ok(())
}

/// Start a new implementer session.
async fn start_implementer(
    config: &Config,
    io: &IoLayer,
    state_file: &mut StateFile,
    issue: &Issue,
    workspace_name: &str,
    base_commit: &str,
) -> anyhow::Result<()> {
    let repo_path = std::env::current_dir()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let workspaces_dir = format!("{}/{}", repo_path, config.workspace.workspaces_dir);
    let workspace_path = format!("{}/{}", workspaces_dir, workspace_name);

    // 1. Create the jj workspace
    io.jj
        .create_workspace(&workspace_path, workspace_name, base_commit)
        .await?;

    // 2. Set up workspace files (.grindbot, .polytoken)
    workspace::setup_workspace(config, &repo_path, issue.number, base_commit, io.fs.as_ref())?;

    // 3. Spawn Polytoken session
    let session_info = io.polytoken.spawn_session(&workspace_path).await?;

    // 4. Configure the session
    io.polytoken.set_facet(&session_info, "plan").await?;
    io.polytoken
        .enable_adventurous_handoff(&session_info)
        .await?;
    io.polytoken
        .set_permission_mode(&session_info, "bypass_plus")
        .await?;
    io.polytoken
        .set_goal(&session_info, &format!("Implement issue #{}", issue.number))
        .await?;

    // 5. Build and send the prompt
    let grindbot_path = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "grindbot".to_string());
    let github_url = format!(
        "https://github.com/{}/{}",
        config.github.owner, config.github.repo
    );
    let prompt_content = prompt::build_prompt(issue, &github_url, &grindbot_path);
    io.polytoken
        .send_prompt(&session_info, &prompt_content, config.polytoken.max_tool_turns)
        .await?;

    // 6. Record in state file
    state_file.add_implementer(ActiveImplementer {
        issue_number: issue.number,
        session_id: session_info.session_id.clone(),
        workspace_name: workspace_name.to_string(),
        workspace_path: workspace_path.clone(),
        base_commit: base_commit.to_string(),
        started_at: chrono::Utc::now().to_rfc3339(),
    });

    tracing::info!(
        "started implementer for issue #{} (session: {})",
        issue.number,
        session_info.session_id
    );

    Ok(())
}

/// Merge an implementation: rebase, move bookmark, push, post comment.
async fn merge_implementation(
    config: &Config,
    io: &IoLayer,
    state_file: &mut StateFile,
    workspace_name: &str,
    commit: &str,
    base_commit: &str,
    issue_number: u64,
) -> anyhow::Result<()> {
    // Rebase implementer's commits onto current main
    let revset = format!("{}::{}", base_commit, commit);
    let dest = format!("{}@origin", config.supervisor.base_branch);
    let rebase_result = io.jj.rebase(&revset, &dest).await?;

    match rebase_result {
        RebaseResult::Success => {
            // Move the bookmark to the rebased tip
            io.jj
                .set_bookmark(&config.supervisor.base_branch, commit)
                .await?;

            // Push to remote
            io.jj
                .push("origin", &config.supervisor.base_branch)
                .await?;

            // Post comment on the issue
            let comment_body = format!(
                "<!-- grindbot -->\n\nImplementation complete. Commit `{}` has been merged to `{}`.",
                commit,
                config.supervisor.base_branch
            );
            io.github
                .post_comment(
                    &config.github.owner,
                    &config.github.repo,
                    issue_number,
                    &comment_body,
                )
                .await?;

            // Record as completed
            state_file.add_completed(CompletedTask {
                issue_number,
                commit: commit.to_string(),
                completed_at: chrono::Utc::now().to_rfc3339(),
            });

            // Reset conflict retries
            state_file.reset_conflict_retry(issue_number);

            // Clean up workspace
            let ws_path = find_workspace_path(config, state_file, workspace_name);
            if let Some(ref path) = ws_path {
                cleanup_workspace_action(config, io, workspace_name, path).await?;
            }
            state_file.remove_implementer(workspace_name);

            tracing::info!("merged implementation for issue #{}", issue_number);
        }
        RebaseResult::Conflict { conflicted_files } => {
            tracing::warn!(
                "merge conflict in workspace {} for issue #{}: {:?}",
                workspace_name,
                issue_number,
                conflicted_files
            );
            // Spawn conflict resolution agent
            resolve_conflict(config, io, state_file, workspace_name, commit, base_commit, issue_number)
                .await?;
        }
    }

    Ok(())
}

/// Resolve merge conflicts by spawning a one-shot Polytoken agent.
async fn resolve_conflict(
    config: &Config,
    io: &IoLayer,
    state_file: &mut StateFile,
    workspace_name: &str,
    commit: &str,
    base_commit: &str,
    issue_number: u64,
) -> anyhow::Result<()> {
    let ws_path = find_workspace_path(config, state_file, workspace_name)
        .unwrap_or_else(|| format!("./{}/{}", config.workspace.workspaces_dir, workspace_name));

    // Check retry count
    let retry_count = state_file.conflict_retry_count(issue_number);
    if retry_count >= 3 {
        tracing::warn!(
            "issue #{} has reached conflict retry limit (3); posting feedback",
            issue_number
        );
        let comment_body = format!(
            "<!-- grindbot -->\n\n**Persistent merge conflict:**\n\nThe implementation for this issue has failed to merge after 3 conflict resolution attempts. The conflicts may indicate that the issue requires a different approach or manual intervention.\n\nPlease review and provide guidance."
        );
        io.github
            .post_comment(
                &config.github.owner,
                &config.github.repo,
                issue_number,
                &comment_body,
            )
            .await?;

        // Discard the workspace
        cleanup_workspace_action(config, io, workspace_name, &ws_path).await?;
        state_file.remove_implementer(workspace_name);
        return Ok(());
    }

    // Set up the workspace for conflict resolution
    workspace::setup_conflict_resolution_workspace(&ws_path, io.fs.as_ref())?;

    // Spawn a conflict resolution agent
    let session_info = io.polytoken.spawn_session(&ws_path).await?;

    // Configure: execute facet, bypass+ permissions, no adventurous handoff
    io.polytoken.set_facet(&session_info, "execute").await?;
    io.polytoken
        .set_permission_mode(&session_info, "bypass_plus")
        .await?;
    io.polytoken
        .set_goal(&session_info, "Resolve merge conflicts in workspace")
        .await?;

    let resolution_prompt = "Resolve the merge conflicts in this workspace. Use the jj-resolve-conflicts skill. Do not make any changes beyond what is needed to resolve the conflicts.";
    io.polytoken
        .send_prompt(&session_info, resolution_prompt, 50)
        .await?;

    // Wait for the agent to finish (poll with timeout)
    let timeout = std::time::Duration::from_secs(600); // 10 minutes
    let start = std::time::Instant::now();

    loop {
        if start.elapsed() > timeout {
            tracing::warn!("conflict resolution agent timed out for issue #{}", issue_number);
            let _ = io.polytoken.terminate(&session_info).await;
            // Increment retry count
            state_file.increment_conflict_retry(issue_number);

            // Discard workspace
            cleanup_workspace_action(config, io, workspace_name, &ws_path).await?;
            state_file.remove_implementer(workspace_name);
            return Ok(());
        }

        match io.polytoken.get_state(&session_info).await {
            Ok(state) => {
                if !state.turn_in_flight {
                    // Agent finished; check if conflicts are resolved
                    break;
                }
            }
            Err(_) => {
                // Session died
                tracing::warn!("conflict resolution agent died for issue #{}", issue_number);
                state_file.increment_conflict_retry(issue_number);
                cleanup_workspace_action(config, io, workspace_name, &ws_path).await?;
                state_file.remove_implementer(workspace_name);
                return Ok(());
            }
        }

        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    }

    // Check if conflicts are resolved
    let has_conflicts = io.jj.has_conflicts().await.unwrap_or(true);

    if has_conflicts {
        tracing::warn!(
            "conflict resolution failed for issue #{}; discarding",
            issue_number
        );
        let _ = io.polytoken.terminate(&session_info).await;
        state_file.increment_conflict_retry(issue_number);
        cleanup_workspace_action(config, io, workspace_name, &ws_path).await?;
        state_file.remove_implementer(workspace_name);
        return Ok(());
    }

    // Conflicts resolved — terminate the agent and proceed with merge
    let _ = io.polytoken.terminate(&session_info).await;

    // Retry the merge
    let revset = format!("{}::{}", base_commit, commit);
    let dest = format!("{}@origin", config.supervisor.base_branch);
    let rebase_result = io.jj.rebase(&revset, &dest).await?;

    match rebase_result {
        RebaseResult::Success => {
            io.jj
                .set_bookmark(&config.supervisor.base_branch, commit)
                .await?;
            io.jj
                .push("origin", &config.supervisor.base_branch)
                .await?;

            let comment_body = format!(
                "<!-- grindbot -->\n\nImplementation complete (after conflict resolution). Commit `{}` has been merged to `{}`.",
                commit,
                config.supervisor.base_branch
            );
            io.github
                .post_comment(
                    &config.github.owner,
                    &config.github.repo,
                    issue_number,
                    &comment_body,
                )
                .await?;

            state_file.add_completed(CompletedTask {
                issue_number,
                commit: commit.to_string(),
                completed_at: chrono::Utc::now().to_rfc3339(),
            });
            state_file.reset_conflict_retry(issue_number);

            cleanup_workspace_action(config, io, workspace_name, &ws_path).await?;
            state_file.remove_implementer(workspace_name);

            tracing::info!("merged implementation for issue #{} (after conflict resolution)", issue_number);
        }
        RebaseResult::Conflict { .. } => {
            tracing::warn!(
                "conflict persisted after resolution for issue #{}; discarding",
                issue_number
            );
            state_file.increment_conflict_retry(issue_number);
            cleanup_workspace_action(config, io, workspace_name, &ws_path).await?;
            state_file.remove_implementer(workspace_name);
        }
    }

    Ok(())
}

/// Process a result file from a finished session.
async fn process_result(
    config: &Config,
    io: &IoLayer,
    state_file: &mut StateFile,
    issue_number: u64,
    workspace_name: &str,
    workspace_path: &str,
    base_commit: &str,
    result: &HandoffResult,
) -> anyhow::Result<()> {
    match result {
        HandoffResult::Done { commit, .. } => {
            merge_implementation(
                config,
                io,
                state_file,
                workspace_name,
                commit,
                base_commit,
                issue_number,
            )
            .await?;
        }
        HandoffResult::NeedsFeedback { message, .. } => {
            let comment_body = format!(
                "<!-- grindbot -->\n\n**Needs feedback:**\n\n{}",
                message
            );
            io.github
                .post_comment(
                    &config.github.owner,
                    &config.github.repo,
                    issue_number,
                    &comment_body,
                )
                .await?;

            state_file.add_needs_feedback(NeedsFeedbackTask {
                issue_number,
                message: message.clone(),
                timestamp: chrono::Utc::now().to_rfc3339(),
            });

            cleanup_workspace_action(config, io, workspace_name, workspace_path).await?;
            state_file.remove_implementer(workspace_name);
        }
    }
    Ok(())
}

/// Clean up a workspace: forget in jj and remove directory.
async fn cleanup_workspace_action(
    _config: &Config,
    io: &IoLayer,
    workspace_name: &str,
    workspace_path: &str,
) -> anyhow::Result<()> {
    // Forget the workspace in jj
    if let Err(e) = io.jj.forget_workspace(workspace_name).await {
        tracing::warn!("failed to forget workspace {}: {}", workspace_name, e);
    }

    // Remove the directory
    if io.fs.exists(workspace_path) {
        if let Err(e) = io.fs.remove_dir_all(workspace_path) {
            tracing::warn!("failed to remove workspace dir {}: {}", workspace_path, e);
        }
    }

    Ok(())
}

/// Find the workspace path from the state file.
fn find_workspace_path(
    _config: &Config,
    state_file: &StateFile,
    workspace_name: &str,
) -> Option<String> {
    state_file
        .active_implementers
        .iter()
        .find(|i| i.workspace_name == workspace_name)
        .map(|i| i.workspace_path.clone())
}
