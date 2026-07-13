//! Property-based tests for the core filters (invariants 10-14).

use grindbot::config::Config;
use grindbot::core::filters::is_eligible;
use grindbot::core::state::{Comment, Issue};
use proptest::prelude::*;

fn arb_datetime() -> impl Strategy<Value = chrono::DateTime<chrono::Utc>> {
    (1i64..365 * 50).prop_map(|days| {
        chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2024, 1, 1, 0, 0, 0).unwrap()
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

fn arb_config() -> impl Strategy<Value = Config> {
    proptest::collection::vec("[a-z]{3,8}", 1..5).prop_map(|allowlist| Config {
        github: grindbot::config::GithubConfig {
            owner: "test".to_string(),
            repo: "test".to_string(),
            allowlist,
        },
        ..Config::default()
    })
}

proptest! {
    // Invariant 10: An issue with supervisor as last commenter is never eligible.
    #[test]
    fn prop_supervisor_last_comment_ineligible(
        issue in arb_issue(),
        config in arb_config(),
        active in proptest::collection::vec(1u64..1000, 0..5),
        completed in proptest::collection::vec(1u64..1000, 0..5),
    ) {
        let mut issue = issue;
        if issue.comments.is_empty() || !issue.comments.last().unwrap().is_supervisor {
            issue.comments.push(Comment {
                author: "grindbot".to_string(),
                body: "<!-- grindbot --> Done".to_string(),
                created_at: chrono::Utc::now(),
                is_supervisor: true,
            });
        }
        prop_assert!(!is_eligible(&issue, &config, &active, &completed));
    }

    // Invariant 11: An issue with author not on allowlist is never eligible.
    #[test]
    fn prop_non_allowlisted_author_ineligible(
        issue in arb_issue(),
        config in arb_config(),
    ) {
        let issue = Issue {
            author: "zzznonexistent".to_string(),
            ..issue
        };
        prop_assert!(!is_eligible(&issue, &config, &[], &[]));
    }

    // Invariant 12: An issue in the active list is never eligible.
    #[test]
    fn prop_active_issue_ineligible(
        issue in arb_issue(),
        config in arb_config(),
    ) {
        let active = vec![issue.number];
        let issue = Issue {
            author: if config.github.allowlist.is_empty() {
                "x".to_string()
            } else {
                config.github.allowlist[0].clone()
            },
            comments: vec![],
            ..issue
        };
        prop_assert!(!is_eligible(&issue, &config, &active, &[]));
    }

    // Invariant 13: An issue in the completed list is never eligible.
    #[test]
    fn prop_completed_issue_ineligible(
        issue in arb_issue(),
        config in arb_config(),
    ) {
        let completed = vec![issue.number];
        let issue = Issue {
            author: if config.github.allowlist.is_empty() {
                "x".to_string()
            } else {
                config.github.allowlist[0].clone()
            },
            comments: vec![],
            ..issue
        };
        prop_assert!(!is_eligible(&issue, &config, &[], &completed));
    }

    // Invariant 14: An issue with no comments and allowlisted author is always eligible
    // (if not active/completed).
    #[test]
    fn prop_no_comments_allowlisted_eligible(
        number in 1u64..1000,
        config in arb_config(),
    ) {
        if let Some(author) = config.github.allowlist.first() {
            let issue = Issue {
                number,
                title: "Test".to_string(),
                body: "Body".to_string(),
                author: author.clone(),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                comments: vec![],
            };
            prop_assert!(is_eligible(&issue, &config, &[], &[]));
        }
    }
}
