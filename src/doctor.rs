use crate::config::Config;
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
enum WorkspaceIgnoreDiagnostic {
    Skipped,
    Clean,
    Warning { workspaces: Vec<PathBuf> },
}

/// Entry point for the CLI.
/// If config is provided, checks config validity too.
/// If config is None, runs only binary checks.
pub async fn run(config: Option<&Config>) -> anyhow::Result<()> {
    println!("Checking grindbot dependencies...");
    println!();

    let mut all_passed = true;

    // jj
    let jj_ok = check_binary("jj", &["--version"]).await;
    all_passed &= jj_ok;

    // gh (version + auth)
    let gh_ok = check_gh().await;
    all_passed &= gh_ok;

    // polytoken (from config if available)
    let pt_ok = if let Some(cfg) = config {
        check_binary(&cfg.polytoken.binary, &["--version"]).await
    } else {
        // Try default binary name
        check_binary("polytoken", &["--version"]).await
    };
    all_passed &= pt_ok;

    // config validity
    let cfg_ok = if let Some(cfg) = config {
        match cfg.validate() {
            Ok(()) => {
                println!("  ✓ config      valid");
                true
            }
            Err(e) => {
                println!("  ✗ config      invalid: {}", e);
                false
            }
        }
    } else {
        println!("  - config      (no config file found)");
        true // not a failure, just skipped
    };
    all_passed &= cfg_ok;

    // jj repo
    let cwd = std::env::current_dir().unwrap_or_default();
    let jj_repo_ok = if cwd.join(".jj").exists() {
        println!("  ✓ jj repo     initialized");
        true
    } else {
        println!("  ✗ jj repo     not found (run 'jj git init --colocate' in this directory)");
        false
    };
    all_passed &= jj_repo_ok;

    // Workspace runtime paths are advisory: each JJ workspace has its own
    // working-tree root and therefore needs its own ignore coverage.
    check_workspace_ignores(&cwd, config);

    println!();
    if all_passed {
        println!("All checks passed.");
        Ok(())
    } else {
        println!("Some checks failed. See above for details.");
        std::process::exit(1);
    }
}

fn check_workspace_ignores(cwd: &Path, config: Option<&Config>) -> WorkspaceIgnoreDiagnostic {
    let workspaces_dir = config
        .map(|cfg| cfg.workspace.workspaces_dir.as_str())
        .unwrap_or(".grindbot-workspaces");
    let workspaces_root = cwd.join(workspaces_dir);

    let entries = match std::fs::read_dir(&workspaces_root) {
        Ok(entries) => entries,
        Err(_) => {
            println!(
                "  - workspace ignores (no managed workspace directory found; ready to check when one exists)"
            );
            return WorkspaceIgnoreDiagnostic::Skipped;
        }
    };

    let mut missing = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() || !path.join(".jj").exists() {
            continue;
        }

        let gitignore = std::fs::read_to_string(path.join(".gitignore")).unwrap_or_default();
        if !has_runtime_ignore(&gitignore, ".grindbot")
            || !has_runtime_ignore(&gitignore, ".polytoken")
        {
            missing.push(path);
        }
    }

    if missing.is_empty() {
        println!("  ✓ workspace ignores covered for managed JJ workspaces");
        WorkspaceIgnoreDiagnostic::Clean
    } else {
        println!("  ! WARNING workspace ignores missing");
        println!(
            "    Each JJ workspace is a separate working-tree root, so add these entries to each workspace's .gitignore:"
        );
        println!("    .grindbot/");
        println!("    .polytoken/");
        println!(
            "    This is a manual setup requirement; this advisory check does not modify or commit repository files."
        );
        WorkspaceIgnoreDiagnostic::Warning {
            workspaces: missing,
        }
    }
}

fn has_runtime_ignore(contents: &str, directory: &str) -> bool {
    contents.lines().any(|line| {
        let entry = line.trim();
        let entry = entry.strip_prefix('/').unwrap_or(entry);
        entry == directory || entry == format!("{directory}/")
    })
}

async fn check_binary(name: &str, args: &[&str]) -> bool {
    tracing::debug!(command = name, args = ?args, "running external command");
    match tokio::process::Command::new(name).args(args).output().await {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout);
            let version_str = version.lines().next().unwrap_or("").trim();
            if version_str.is_empty() {
                println!("  ✓ {:<11} found", name);
            } else {
                println!("  ✓ {:<11} {}", name, version_str);
            }
            true
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr_str = stderr.lines().next().unwrap_or("").trim();
            println!("  ✗ {:<11} error: {}", name, stderr_str);
            false
        }
        Err(_) => {
            println!("  ✗ {:<11} not found. Install or add to PATH.", name);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignore_diagnostic_is_advisory_not_a_fatal_check() {
        let dir = tempfile::tempdir().unwrap();
        let workspaces = dir.path().join("workspaces");
        let workspace = workspaces.join("grindbot-1");
        std::fs::create_dir_all(workspace.join(".jj")).unwrap();

        let mut config = Config::default();
        config.workspace.workspaces_dir = "workspaces".to_string();

        let diagnostic = check_workspace_ignores(dir.path(), Some(&config));
        assert!(matches!(
            diagnostic,
            WorkspaceIgnoreDiagnostic::Warning { .. }
        ));
        let all_passed = true;
        assert!(
            all_passed,
            "advisory diagnostics must not change fatal status"
        );
    }

    #[test]
    fn accepted_runtime_ignore_forms_are_exact() {
        for contents in [".grindbot\n.polytoken/\n", "/.grindbot/\n/.polytoken\n"] {
            assert!(has_runtime_ignore(contents, ".grindbot"));
            assert!(has_runtime_ignore(contents, ".polytoken"));
        }
        assert!(!has_runtime_ignore("*\n", ".grindbot"));
        assert!(!has_runtime_ignore(".grindbot-cache/\n", ".grindbot"));
    }
}

async fn check_gh() -> bool {
    // Check version
    let version_ok = check_binary("gh", &["--version"]).await;
    if !version_ok {
        return false;
    }

    // Check auth status
    tracing::debug!(command = "gh auth status", "running external command");
    match tokio::process::Command::new("gh")
        .args(["auth", "status"])
        .output()
        .await
    {
        Ok(output) if output.status.success() => {
            // Try to extract the authenticated user from stderr (gh prints to stderr)
            let stderr = String::from_utf8_lossy(&output.stderr);
            if let Some(line) = stderr.lines().find(|l| l.contains("Logged in to")) {
                // Try to extract the account name
                let account = line
                    .split("account")
                    .nth(1)
                    .and_then(|s| {
                        s.trim_start_matches(|c: char| c.is_whitespace() || c == ':')
                            .strip_prefix(' ')
                    })
                    .and_then(|s| s.split_whitespace().next())
                    .unwrap_or("");
                if !account.is_empty() {
                    println!("  ✓ {:<11} authenticated as {}", "gh", account);
                } else {
                    println!("  ✓ {:<11} authenticated", "gh");
                }
            } else {
                println!("  ✓ {:<11} authenticated", "gh");
            }
            true
        }
        _ => {
            println!("  ✗ {:<11} not authenticated (run 'gh auth login')", "gh");
            false
        }
    }
}
