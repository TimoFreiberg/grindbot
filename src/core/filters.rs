use crate::config::Config;
use crate::core::state::Issue;

/// An issue is eligible for implementation if ALL of:
/// 1. Issue author is on the configured allowlist
/// 2. Last activity on the issue was by a human (not by the supervisor)
/// 3. Issue is not currently being implemented (not in active implementers list)
/// 4. Issue is not in the completed list (local state file)
pub fn is_eligible(
    issue: &Issue,
    config: &Config,
    active_issues: &[u64],
    completed_issues: &[u64],
) -> bool {
    // 1. Author on allowlist
    if !config.github.allowlist.contains(&issue.author) {
        return false;
    }
    // 2. Last activity by human (not supervisor)
    if last_activity_by_supervisor(issue) {
        return false;
    }
    // 3. Not currently being implemented
    if active_issues.contains(&issue.number) {
        return false;
    }
    // 4. Not already completed
    if completed_issues.contains(&issue.number) {
        return false;
    }
    true
}

/// Check if the last comment on the issue was by the supervisor.
/// Supervisor comments are detected by the `<!-- grindbot -->` prefix.
fn last_activity_by_supervisor(issue: &Issue) -> bool {
    issue
        .comments
        .last()
        .map(|c| c.is_supervisor)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::state::Comment;

    fn make_issue(author: &str, comments: Vec<Comment>) -> Issue {
        Issue {
            number: 1,
            title: "Test".to_string(),
            body: "Body".to_string(),
            author: author.to_string(),
            created_at: jiff::Timestamp::now(),
            updated_at: jiff::Timestamp::now(),
            comments,
        }
    }

    fn make_comment(is_supervisor: bool) -> Comment {
        Comment {
            author: if is_supervisor {
                "grindbot".to_string()
            } else {
                "human".to_string()
            },
            body: if is_supervisor {
                "<!-- grindbot --> Done".to_string()
            } else {
                "Nice work".to_string()
            },
            created_at: jiff::Timestamp::now(),
            is_supervisor,
        }
    }

    fn make_config(allowlist: Vec<String>) -> Config {
        Config {
            github: crate::config::GithubConfig {
                owner: "test".to_string(),
                repo: "test".to_string(),
                allowlist,
            },
            ..Config::default()
        }
    }

    #[test]
    fn test_allowlisted_author_no_comments_eligible() {
        let issue = make_issue("alice", vec![]);
        let config = make_config(vec!["alice".to_string()]);
        assert!(is_eligible(&issue, &config, &[], &[]));
    }

    #[test]
    fn test_non_allowlisted_author_ineligible() {
        let issue = make_issue("bob", vec![]);
        let config = make_config(vec!["alice".to_string()]);
        assert!(!is_eligible(&issue, &config, &[], &[]));
    }

    #[test]
    fn test_supervisor_last_comment_ineligible() {
        let issue = make_issue("alice", vec![make_comment(false), make_comment(true)]);
        let config = make_config(vec!["alice".to_string()]);
        assert!(!is_eligible(&issue, &config, &[], &[]));
    }

    #[test]
    fn test_human_last_comment_eligible() {
        let issue = make_issue("alice", vec![make_comment(true), make_comment(false)]);
        let config = make_config(vec!["alice".to_string()]);
        assert!(is_eligible(&issue, &config, &[], &[]));
    }

    #[test]
    fn test_active_issue_ineligible() {
        let issue = make_issue("alice", vec![]);
        let config = make_config(vec!["alice".to_string()]);
        assert!(!is_eligible(&issue, &config, &[1], &[]));
    }

    #[test]
    fn test_completed_issue_ineligible() {
        let issue = make_issue("alice", vec![]);
        let config = make_config(vec!["alice".to_string()]);
        assert!(!is_eligible(&issue, &config, &[], &[1]));
    }
}
