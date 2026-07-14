use std::path::{Path, PathBuf};

use crate::core::state::HandoffResult;

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

/// Handle `grindbot handoff done --manifest <path>`.
pub fn done_manifest(manifest_path: &Path) -> anyhow::Result<()> {
    let workspace_root = find_workspace_root()?;
    let manifest: HandoffResult = serde_json::from_str(&std::fs::read_to_string(manifest_path)?)?;
    let HandoffResult::Done { commit, timestamp, issue, summary, evidence, .. } = manifest else {
        anyhow::bail!("approved handoff manifest must have status=done");
    };
    let evidence = evidence.ok_or_else(|| anyhow::anyhow!("approved handoff is missing evidence"))?;
    if evidence.plan_review.trim().is_empty()
        || evidence.implementation_review.trim().is_empty()
        || evidence.tests.is_empty()
        || evidence.acceptance_mapping.is_empty()
        || evidence.unresolved_findings
    {
        anyhow::bail!("approved handoff evidence is incomplete or has unresolved findings");
    }
    validate_commit(&workspace_root, &commit)?;
    write_result(&workspace_root, &HandoffResult::Done {
        manifest_version: 1, commit: commit.clone(), timestamp, issue, summary,
        evidence: Some(evidence),
    })?;
    println!("Handoff complete: done (commit: {})", commit);
    Ok(())
}

fn validate_commit(workspace_root: &Path, commit: &str) -> anyhow::Result<()> {
    let base_commit = std::fs::read_to_string(workspace_root.join(".grindbot/base_commit"))?.trim().to_string();
    tracing::debug!(command = "jj log", commit, repository = ?workspace_root, "running external command");
    let output = std::process::Command::new("jj").args(["log", "-r", commit, "--no-graph", "-R", workspace_root.to_str().unwrap()]).output()?;
    if !output.status.success() { anyhow::bail!("commit {} does not exist: {}", commit, String::from_utf8_lossy(&output.stderr)); }
    let revset = format!("{}::{} ~ {}", base_commit, commit, base_commit);
    tracing::debug!(command = "jj log", revset, repository = ?workspace_root, "running external command");
    let output = std::process::Command::new("jj").args(["log", "-r", &revset, "--no-graph", "-R", workspace_root.to_str().unwrap()]).output()?;
    if !output.status.success() || String::from_utf8_lossy(&output.stdout).trim().is_empty() {
        anyhow::bail!("commit {} is not ahead of base {}", commit, base_commit);
    }
    Ok(())
}

/// Legacy direct commit path retained for reading old callers.
pub fn done(commit: &str) -> anyhow::Result<()> {
    let workspace_root = find_workspace_root()?;

    // Read base commit
    let base_commit_path = workspace_root.join(".grindbot").join("base_commit");
    if !base_commit_path.exists() {
        anyhow::bail!(
            "base_commit file not found at {}. Was this workspace set up by the supervisor?",
            base_commit_path.display()
        );
    }
    let base_commit = std::fs::read_to_string(&base_commit_path)?
        .trim()
        .to_string();

    // Validate commit exists
    tracing::debug!(command = "jj log", commit, repository = ?workspace_root, "running external command");
    let log_output = std::process::Command::new("jj")
        .args([
            "log",
            "-r",
            commit,
            "--no-graph",
            "-R",
            workspace_root.to_str().unwrap(),
        ])
        .output()?;

    if !log_output.status.success() {
        anyhow::bail!(
            "commit {} does not exist in the repository. Run 'jj log' to see available commits.\n{}",
            commit,
            String::from_utf8_lossy(&log_output.stderr)
        );
    }

    // Validate commit is ahead of base (not identical to base)
    // `jj log -r '<base>::<commit> ~ <base>'` should be non-empty
    let revset = format!("{}::{} ~ {}", base_commit, commit, base_commit);
    tracing::debug!(command = "jj log", revset, repository = ?workspace_root, "running external command");
    let ahead_output = std::process::Command::new("jj")
        .args([
            "log",
            "-r",
            &revset,
            "--no-graph",
            "-R",
            workspace_root.to_str().unwrap(),
        ])
        .output()?;

    if !ahead_output.status.success() {
        // The revset might error if commit == base; that's a failure
        anyhow::bail!(
            "commit {} is not ahead of base {}: {}",
            commit,
            base_commit,
            String::from_utf8_lossy(&ahead_output.stderr)
        );
    }

    let ahead_stdout = String::from_utf8_lossy(&ahead_output.stdout);
    if ahead_stdout.trim().is_empty() {
        anyhow::bail!(
            "commit {} is identical to base {} — no changes to hand off. \
             Ensure you have committed your work with 'jj new'.",
            commit,
            base_commit
        );
    }

    let timestamp = jiff::Timestamp::now().to_string();
    let result = HandoffResult::Done {
        manifest_version: 1,
        commit: commit.to_string(),
        timestamp,
        issue: None,
        summary: String::new(),
        evidence: None,
    };

    write_result(&workspace_root, &result)?;
    println!("Handoff complete: done (commit: {})", commit);
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
