//! Property-based tests for the core filters (invariants 10-14).

use chrono::TimeZone;
use grindbot::config::Config;
use grindbot::core::filters::is_eligible;
use grindbot::core::state::{Comment, Issue};
use hegel::generators as gs;
use hegel::{Generator, TestCase};

fn datetime_generator() -> impl hegel::Generator<chrono::DateTime<chrono::Utc>> {
    gs::integers::<i64>()
        .min_value(0)
        .max_value(365 * 50)
        .map(|days| {
            chrono::Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap()
                + chrono::Duration::days(days)
        })
}

fn comment_generator() -> impl hegel::Generator<Comment> {
    hegel::tuples!(
        gs::from_regex("[a-z]{3,8}"),
        gs::from_regex("[a-z ]{5,50}"),
        datetime_generator(),
        gs::booleans(),
    )
    .map(|(author, body, created_at, is_supervisor)| Comment {
        author,
        body,
        created_at,
        is_supervisor,
    })
}

fn issue_generator() -> impl hegel::Generator<Issue> {
    hegel::tuples!(
        gs::integers::<u64>().min_value(1).max_value(1000),
        gs::from_regex("[A-Za-z ]{5,30}"),
        gs::from_regex("[a-z ]{5,100}"),
        gs::from_regex("[a-z]{3,10}"),
        datetime_generator(),
        datetime_generator(),
        gs::vecs(comment_generator()).max_size(5),
    )
    .map(
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

fn config_generator() -> impl hegel::Generator<Config> {
    gs::vecs(gs::from_regex("[a-z]{3,8}"))
        .min_size(1)
        .max_size(5)
        .map(|allowlist| Config {
            github: grindbot::config::GithubConfig {
                owner: "test".to_string(),
                repo: "test".to_string(),
                allowlist,
            },
            ..Config::default()
        })
}

#[hegel::composite]
fn active_numbers(tc: TestCase, issue_number: u64) -> Vec<u64> {
    let mut values =
        tc.draw(gs::vecs(gs::integers::<u64>().min_value(1).max_value(1000)).max_size(5));
    values.push(issue_number);
    values
}

#[hegel::test]
fn prop_supervisor_last_comment_ineligible(tc: TestCase) {
    let mut issue = tc.draw(issue_generator());
    let config = tc.draw(config_generator());
    let active = tc.draw(gs::vecs(gs::integers::<u64>().min_value(1).max_value(1000)).max_size(5));
    let completed =
        tc.draw(gs::vecs(gs::integers::<u64>().min_value(1).max_value(1000)).max_size(5));
    issue.comments.push(Comment {
        author: "grindbot".to_string(),
        body: "<!-- grindbot --> Done".to_string(),
        created_at: issue.updated_at,
        is_supervisor: true,
    });
    assert!(!is_eligible(&issue, &config, &active, &completed));
}

#[hegel::test]
fn prop_non_allowlisted_author_ineligible(tc: TestCase) {
    let issue = Issue {
        author: "not-allowed".to_string(),
        ..tc.draw(issue_generator())
    };
    let config = tc.draw(config_generator());
    assert!(!config.github.allowlist.contains(&issue.author));
    assert!(!is_eligible(&issue, &config, &[], &[]));
}

#[hegel::test]
fn prop_active_issue_ineligible(tc: TestCase) {
    let issue = tc.draw(issue_generator());
    let mut config = tc.draw(config_generator());
    let author = config.github.allowlist[0].clone();
    config.github.allowlist = vec![author.clone()];
    let issue = Issue {
        author,
        comments: vec![],
        ..issue
    };
    let active = tc.draw(active_numbers(issue.number));
    assert!(!is_eligible(&issue, &config, &active, &[]));
}

#[hegel::test]
fn prop_completed_issue_ineligible(tc: TestCase) {
    let issue = tc.draw(issue_generator());
    let mut config = tc.draw(config_generator());
    let author = config.github.allowlist[0].clone();
    config.github.allowlist = vec![author.clone()];
    let issue = Issue {
        author,
        comments: vec![],
        ..issue
    };
    assert!(!is_eligible(
        &issue,
        &config,
        &[],
        &[issue.number, issue.number]
    ));
}

#[hegel::test]
fn prop_no_comments_allowlisted_eligible(tc: TestCase) {
    let config = tc.draw(config_generator());
    let author = config.github.allowlist[0].clone();
    let issue = Issue {
        number: tc.draw(gs::integers::<u64>().min_value(1).max_value(1000)),
        title: "Test".to_string(),
        body: "Body".to_string(),
        author,
        created_at: tc.draw(datetime_generator()),
        updated_at: tc.draw(datetime_generator()),
        comments: vec![],
    };
    assert!(is_eligible(&issue, &config, &[], &[]));
}

#[test]
fn empty_allowlist_and_empty_comments_are_ineligible() {
    let issue = Issue {
        number: 1,
        title: "t".into(),
        body: "b".into(),
        author: "alice".into(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        comments: vec![],
    };
    assert!(!is_eligible(&issue, &Config::default(), &[], &[]));
}
