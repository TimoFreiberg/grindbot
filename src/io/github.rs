use std::sync::Arc;

use crate::core::state::{Comment, Issue};
use crate::io::GithubClient;

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
        let output = tokio::process::Command::new("gh")
            .args([
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
            ])
            .output()
            .await?;

        if !output.status.success() {
            anyhow::bail!(
                "gh issue list failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        #[derive(serde::Deserialize)]
        struct GhIssue {
            number: u64,
            title: String,
            body: String,
            author: String,
            #[serde(rename = "createdAt")]
            created_at: String,
            #[serde(rename = "updatedAt")]
            updated_at: String,
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
                author: gi.author,
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
        let output = tokio::process::Command::new("gh")
            .args([
                "issue",
                "comment",
                &issue.to_string(),
                "--repo",
                &format!("{}/{}", owner, repo),
                "--body",
                body,
            ])
            .output()
            .await?;

        if !output.status.success() {
            anyhow::bail!(
                "gh issue comment failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        Ok(())
    }
}

/// Fetch comments for a specific issue using `gh issue view --json comments`.
pub async fn fetch_comments(owner: &str, repo: &str, issue: u64) -> anyhow::Result<Vec<Comment>> {
    let output = tokio::process::Command::new("gh")
        .args([
            "issue",
            "view",
            &issue.to_string(),
            "--repo",
            &format!("{}/{}", owner, repo),
            "--json",
            "comments",
        ])
        .output()
        .await?;

    if !output.status.success() {
        anyhow::bail!(
            "gh issue view failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[derive(serde::Deserialize)]
    struct GhComments {
        comments: Vec<GhComment>,
    }

    #[derive(serde::Deserialize)]
    struct GhComment {
        author: String,
        body: String,
        #[serde(rename = "createdAt")]
        created_at: String,
    }

    let gh_comments: GhComments = serde_json::from_slice(&output.stdout)?;

    let comments = gh_comments
        .comments
        .into_iter()
        .map(|c| {
            let is_supervisor = c.body.trim_start().starts_with("<!-- grindbot -->");
            let created_at = parse_gh_datetime(&c.created_at).unwrap_or(chrono::Utc::now());
            Comment {
                author: c.author,
                body: c.body,
                created_at,
                is_supervisor,
            }
        })
        .collect();

    Ok(comments)
}

fn parse_gh_datetime(s: &str) -> anyhow::Result<chrono::DateTime<chrono::Utc>> {
    // gh returns ISO 8601 with 'Z' suffix, e.g. "2024-01-15T12:30:00Z"
    Ok(chrono::DateTime::parse_from_rfc3339(s)?.with_timezone(&chrono::Utc))
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
