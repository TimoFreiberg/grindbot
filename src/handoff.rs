use std::path::{Path, PathBuf};

use crate::core::state::{HandoffEvidence, HandoffResult};

/// Find the workspace root by walking up from CWD until `.jj/` is found.
fn find_workspace_root() -> anyhow::Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    let mut current: &Path = &cwd;
    loop {
        if current.join(".jj").exists() {
            return Ok(current.to_path_buf());
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => anyhow::bail!("could not find .jj directory in any parent of CWD"),
        }
    }
}

/// Write the result file, resetting the stop counter.
fn write_result(workspace_root: &Path, result: &HandoffResult) -> anyhow::Result<()> {
    let grindbot_dir = workspace_root.join(".grindbot");
    std::fs::create_dir_all(&grindbot_dir)?;

    // Reset the stop counter (so a stale counter doesn't interfere)
    let counter_file = grindbot_dir.join("stop_counter");
    let _ = std::fs::remove_file(&counter_file);

    let result_json = serde_json::to_string_pretty(result)?;
    let result_path = grindbot_dir.join("result.json");
    std::fs::write(&result_path, result_json)?;

    println!("Result written to {}", result_path.display());
    Ok(())
}

/// Handle `grindbot handoff done` with structured evidence supplied as CLI arguments.
pub fn done(
    commit: &str,
    plan_review: &str,
    implementation_review: &str,
    all_tests_passed: bool,
    summary: &str,
    issue: Option<u64>,
    unresolved_findings: bool,
) -> anyhow::Result<()> {
    let workspace_root = find_workspace_root()?;
    let evidence = HandoffEvidence {
        plan_review: plan_review.to_string(),
        implementation_review: implementation_review.to_string(),
        all_tests_passed,
        unresolved_findings,
    };
    if evidence.plan_review.trim().is_empty()
        || evidence.implementation_review.trim().is_empty()
        || !evidence.all_tests_passed
        || evidence.unresolved_findings
    {
        anyhow::bail!(
            "handoff evidence is incomplete or has unresolved findings. \
             Run 'grindbot handoff done --help' for the required arguments."
        );
    }
    validate_commit(&workspace_root, commit)?;
    write_result(
        &workspace_root,
        &HandoffResult::Done {
            manifest_version: 1,
            commit: commit.to_string(),
            timestamp: jiff::Timestamp::now().to_string(),
            issue,
            summary: summary.to_string(),
            evidence: Some(evidence),
        },
    )?;
    println!("Handoff complete: done (commit: {})", commit);
    Ok(())
}

fn validate_commit(workspace_root: &Path, commit: &str) -> anyhow::Result<()> {
    let base_commit = std::fs::read_to_string(workspace_root.join(".grindbot/base_commit"))?
        .trim()
        .to_string();
    tracing::debug!(command = "jj log", commit, repository = ?workspace_root, "running external command");
    let output = std::process::Command::new("jj")
        .args([
            "log",
            "-r",
            commit,
            "--no-graph",
            "-R",
            workspace_root.to_str().unwrap(),
        ])
        .output()?;
    if !output.status.success() {
        anyhow::bail!(
            "commit {} does not exist: {}",
            commit,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let revset = format!("{}::{} ~ {}", base_commit, commit, base_commit);
    tracing::debug!(command = "jj log", revset, repository = ?workspace_root, "running external command");
    let output = std::process::Command::new("jj")
        .args([
            "log",
            "-r",
            &revset,
            "--no-graph",
            "-R",
            workspace_root.to_str().unwrap(),
        ])
        .output()?;
    if !output.status.success() || String::from_utf8_lossy(&output.stdout).trim().is_empty() {
        anyhow::bail!(
            "commit {} is not ahead of base {}. Ensure your work is committed with 'jj new'.",
            commit,
            base_commit
        );
    }
    Ok(())
}

/// Handle `grindbot handoff needs-feedback --message <text>`.
pub fn needs_feedback(message: &str) -> anyhow::Result<()> {
    let workspace_root = find_workspace_root()?;

    let timestamp = jiff::Timestamp::now().to_string();
    let result = HandoffResult::NeedsFeedback {
        message: message.to_string(),
        timestamp,
    };

    write_result(&workspace_root, &result)?;
    println!("Handoff complete: needs-feedback");
    Ok(())
}
