use std::sync::Arc;

use crate::core::state::{Comment, Issue};
use crate::io::GithubClient;

/// The user object shape returned by `gh --json ...` fields such as `author`.
#[derive(serde::Deserialize)]
struct GhUser {
    login: String,
}

#[derive(serde::Deserialize)]
struct GhIssue {
    number: u64,
    title: String,
    body: String,
    author: GhUser,
    #[serde(rename = "createdAt")]
    created_at: String,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

#[derive(serde::Deserialize)]
struct GhComments {
    comments: Vec<GhComment>,
}

#[derive(serde::Deserialize)]
struct GhComment {
    author: GhUser,
    body: String,
    #[serde(rename = "createdAt")]
    created_at: String,
}

/// Real GitHub client using the `gh` CLI.
pub struct RealGithubClient;

impl RealGithubClient {
    pub fn new() -> Self {
        Self
    }
}

impl Default for RealGithubClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl GithubClient for RealGithubClient {
    async fn list_issues(&self, owner: &str, repo: &str) -> anyhow::Result<Vec<Issue>> {
        // Step 1: List issues with basic fields
        let args = [
            "issue",
            "list",
            "--repo",
            &format!("{}/{}", owner, repo),
            "--json",
            "number,title,body,author,createdAt,updatedAt",
            "--state",
            "open",
            "--limit",
            "100",
        ];
        tracing::debug!(command = "gh issue list", args = ?args, "running external command");
        let output = tokio::process::Command::new("gh")
            .args(args)
            .output()
            .await?;

        if !output.status.success() {
            anyhow::bail!(
                "gh issue list failed for {}/{}: {}\n\
                 Ensure gh is installed, authenticated (gh auth login), and the repository exists.",
                owner,
                repo,
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let gh_issues: Vec<GhIssue> = serde_json::from_slice(&output.stdout)?;

        let mut issues = Vec::with_capacity(gh_issues.len());
        for gi in gh_issues {
            let created_at = parse_gh_datetime(&gi.created_at)?;
            let updated_at = parse_gh_datetime(&gi.updated_at)?;

            issues.push(Issue {
                number: gi.number,
                title: gi.title,
                body: gi.body,
                author: gi.author.login,
                created_at,
                updated_at,
                comments: vec![], // Fetched separately
            });
        }

        Ok(issues)
    }

    async fn post_comment(
        &self,
        owner: &str,
        repo: &str,
        issue: u64,
        body: &str,
    ) -> anyhow::Result<()> {
        let args = [
            "issue",
            "comment",
            &issue.to_string(),
            "--repo",
            &format!("{}/{}", owner, repo),
            "--body",
            body,
        ];
        tracing::debug!(
            command = "gh issue comment",
            issue,
            owner,
            repo,
            "running external command"
        );
        let output = tokio::process::Command::new("gh")
            .args(args)
            .output()
            .await?;

        if !output.status.success() {
            anyhow::bail!(
                "gh issue comment failed for issue #{} in {}/{}: {}",
                issue,
                owner,
                repo,
                String::from_utf8_lossy(&output.stderr)
            );
        }

        Ok(())
    }
}

/// Fetch comments for a specific issue using `gh issue view --json comments`.
pub async fn fetch_comments(owner: &str, repo: &str, issue: u64) -> anyhow::Result<Vec<Comment>> {
    let args = [
        "issue",
        "view",
        &issue.to_string(),
        "--repo",
        &format!("{}/{}", owner, repo),
        "--json",
        "comments",
    ];
    tracing::debug!(
        command = "gh issue view",
        issue,
        owner,
        repo,
        "running external command"
    );
    let output = tokio::process::Command::new("gh")
        .args(args)
        .output()
        .await?;

    if !output.status.success() {
        anyhow::bail!(
            "gh issue view failed for issue #{} in {}/{}: {}",
            issue,
            owner,
            repo,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let gh_comments: GhComments = serde_json::from_slice(&output.stdout)?;

    let comments = gh_comments
        .comments
        .into_iter()
        .map(|c| {
            let is_supervisor = c.body.trim_start().starts_with("<!-- grindbot -->");
            let created_at = parse_gh_datetime(&c.created_at).unwrap_or(jiff::Timestamp::now());
            Comment {
                author: c.author.login,
                body: c.body,
                created_at,
                is_supervisor,
            }
        })
        .collect();

    Ok(comments)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gh_user_objects_decode_to_logins() {
        let issue: GhIssue = serde_json::from_str(
            r#"{"number":1,"title":"Fix","body":"Details","author":{"login":"TimoFreiberg"},"createdAt":"2024-01-15T12:30:00Z","updatedAt":"2024-01-15T12:30:00Z"}"#,
        )
        .unwrap();
        assert_eq!(issue.author.login, "TimoFreiberg");

        let comments: GhComments = serde_json::from_str(
            r#"{"comments":[{"author":{"login":"alice"},"body":"Looks good","createdAt":"2024-01-15T12:30:00Z"}]}"#,
        )
        .unwrap();
        assert_eq!(comments.comments[0].author.login, "alice");
    }
}

fn parse_gh_datetime(s: &str) -> anyhow::Result<jiff::Timestamp> {
    // gh returns ISO 8601 with 'Z' suffix, e.g. "2024-01-15T12:30:00Z"
    Ok(s.parse::<jiff::Timestamp>()?)
}

/// Enrich issues with comments (only for allowlisted authors to minimize API calls).
pub async fn enrich_with_comments(
    _client: &Arc<dyn GithubClient>,
    owner: &str,
    repo: &str,
    issues: &mut [Issue],
    allowlist: &[String],
) -> anyhow::Result<()> {
    for issue in issues.iter_mut() {
        if allowlist.contains(&issue.author) {
            if let Ok(comments) = fetch_comments(owner, repo, issue.number).await {
                issue.comments = comments;
            }
        }
    }
    Ok(())
}
