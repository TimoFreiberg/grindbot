use crate::io::{CommitInfo, JjClient, RebaseResult};

/// Real Jujutsu client using the `jj` CLI.
pub struct RealJjClient {
    /// The main repo path (where the shared .jj lives).
    pub repo_path: String,
}

impl RealJjClient {
    pub fn new(repo_path: &str) -> Self {
        Self {
            repo_path: repo_path.to_string(),
        }
    }

    async fn run_jj(&self, args: &[&str]) -> anyhow::Result<std::process::Output> {
        let mut cmd = tokio::process::Command::new("jj");
        cmd.args(["--repository", &self.repo_path]);
        cmd.args(args);
        cmd.output().await.map_err(Into::into)
    }
}

#[async_trait::async_trait]
impl JjClient for RealJjClient {
    async fn init_colocated(&self, repo_path: &str) -> anyhow::Result<()> {
        let output = tokio::process::Command::new("jj")
            .args(["git", "init", repo_path])
            .output()
            .await?;
        if !output.status.success() {
            anyhow::bail!(
                "jj git init failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Ok(())
    }

    async fn create_workspace(
        &self,
        dest: &str,
        name: &str,
        base_rev: &str,
    ) -> anyhow::Result<()> {
        let output = tokio::process::Command::new("jj")
            .args([
                "workspace",
                "add",
                dest,
                "--name",
                name,
                "-r",
                base_rev,
            ])
            .output()
            .await?;
        if !output.status.success() {
            anyhow::bail!(
                "jj workspace add failed for workspace '{}' at '{}' (base: {}): {}",
                name,
                dest,
                base_rev,
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Ok(())
    }

    async fn forget_workspace(&self, name: &str) -> anyhow::Result<()> {
        let output = self.run_jj(&["workspace", "forget", name]).await?;
        if !output.status.success() {
            anyhow::bail!(
                "jj workspace forget failed for '{}': {}",
                name,
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Ok(())
    }

    async fn list_workspaces(&self) -> anyhow::Result<Vec<String>> {
        let output = self.run_jj(&["workspace", "list"]).await?;
        if !output.status.success() {
            anyhow::bail!(
                "jj workspace list failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let workspaces: Vec<String> = text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.split_whitespace().next().unwrap_or("").to_string())
            .filter(|s| !s.is_empty())
            .collect();
        Ok(workspaces)
    }

    async fn rebase(&self, revset: &str, dest: &str) -> anyhow::Result<RebaseResult> {
        let output = self.run_jj(&["rebase", "-r", revset, "-d", dest]).await?;

        if !output.status.success() {
            // Check if it's a conflict
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("conflict") || stderr.contains("Conflict") {
                // Try to get conflicted files
                let conflicted = self.get_conflicted_files().await.unwrap_or_default();
                return Ok(RebaseResult::Conflict {
                    conflicted_files: conflicted,
                });
            }
            anyhow::bail!("jj rebase failed for revset '{}' onto '{}': {}", revset, dest, stderr);
        }

        // Even on success, check for conflicts (jj may exit 0 with conflict markers)
        if self.has_conflicts().await? {
            let conflicted = self.get_conflicted_files().await.unwrap_or_default();
            return Ok(RebaseResult::Conflict {
                conflicted_files: conflicted,
            });
        }

        Ok(RebaseResult::Success)
    }

    async fn set_bookmark(&self, name: &str, rev: &str) -> anyhow::Result<()> {
        let output = self
            .run_jj(&["bookmark", "set", name, "-r", rev, "--allow-backwards"])
            .await?;
        if !output.status.success() {
            anyhow::bail!(
                "jj bookmark set failed for '{}' -> '{}': {}",
                name,
                rev,
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Ok(())
    }

    async fn log(&self, revset: &str) -> anyhow::Result<Vec<CommitInfo>> {
        let template = r#"change_id ++ "\n" ++ commit_id ++ "\n" ++ description ++ "\n---\n""#;
        let output = self
            .run_jj(&["log", "-r", revset, "--no-graph", "-T", template])
            .await?;
        if !output.status.success() {
            anyhow::bail!(
                "jj log failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let text = String::from_utf8_lossy(&output.stdout);
        let mut commits = Vec::new();
        for block in text.split("\n---\n") {
            let lines: Vec<&str> = block.lines().collect();
            if lines.len() >= 3 {
                commits.push(CommitInfo {
                    change_id: lines[0].to_string(),
                    commit_hash: lines[1].to_string(),
                    description: lines[2..].join("\n"),
                });
            }
        }
        Ok(commits)
    }

    async fn current_main(&self) -> anyhow::Result<String> {
        let output = self
            .run_jj(&["log", "-r", "main@origin", "--no-graph", "-T", "commit_id"])
            .await?;
        if !output.status.success() {
            // Try without @origin
            let output = self
                .run_jj(&["log", "-r", "main", "--no-graph", "-T", "commit_id"])
                .await?;
            if !output.status.success() {
                anyhow::bail!(
                    "jj log main@origin failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            return Ok(String::from_utf8_lossy(&output.stdout)
                .trim()
                .to_string());
        }
        Ok(String::from_utf8_lossy(&output.stdout)
            .trim()
            .to_string())
    }

    async fn push(&self, remote: &str, branch: &str) -> anyhow::Result<()> {
        let output = self
            .run_jj(&["git", "push", "-r", branch, "--allow-new"])
            .await?;
        if !output.status.success() {
            anyhow::bail!(
                "jj git push failed for '{}': {}",
                branch,
                String::from_utf8_lossy(&output.stderr)
            );
        }
        // remote is used for future git push compatibility
        let _ = remote;
        Ok(())
    }

    async fn has_conflicts(&self) -> anyhow::Result<bool> {
        let output = self.run_jj(&["log", "-r", "conflicted", "--no-graph"]).await?;
        // If there are conflicted revisions, stdout will be non-empty
        Ok(!String::from_utf8_lossy(&output.stdout).trim().is_empty())
    }
}

impl RealJjClient {
    async fn get_conflicted_files(&self) -> anyhow::Result<Vec<String>> {
        let output = self.run_jj(&["diff", "--summary"]).await?;
        let text = String::from_utf8_lossy(&output.stdout);
        Ok(text
            .lines()
            .filter(|l| l.contains("conflict") || l.contains("Conflict"))
            .map(|l| l.to_string())
            .collect())
    }
}
