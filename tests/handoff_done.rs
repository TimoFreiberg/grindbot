//! AC.3: grindbot handoff done validates commit and writes result.json.
//! AC.4: grindbot handoff needs-feedback writes result.json.

mod common;

use grindbot::core::state::HandoffResult;
use std::path::Path;

fn create_jj_repo(dir: &Path) -> String {
    // These tests exercise real jj behavior. Skip only when jj is unavailable;
    // command failures in an environment with jj installed must fail the test.
    if std::process::Command::new("jj")
        .arg("--version")
        .status()
        .is_err()
    {
        eprintln!("skipping handoff test: jj is not installed");
        return String::new();
    }
    let init = std::process::Command::new("jj")
        .args(["git", "init", "--colocate"])
        .current_dir(dir)
        .output()
        .expect("jj git init could not be started");
    assert!(
        init.status.success(),
        "jj git init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );

    // Create a file and commit
    std::fs::write(dir.join("test.txt"), "hello").unwrap();
    let new = std::process::Command::new("jj")
        .args(["new"])
        .current_dir(dir)
        .output()
        .expect("jj new could not be started");
    assert!(
        new.status.success(),
        "jj new failed: {}",
        String::from_utf8_lossy(&new.stderr)
    );

    // Get the current commit hash
    let output = std::process::Command::new("jj")
        .args(["log", "-r", "@", "--no-graph", "-T", "commit_id"])
        .current_dir(dir)
        .output()
        .expect("jj log could not be started");
    assert!(
        output.status.success(),
        "jj log failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn get_base_commit(dir: &Path) -> String {
    let output = std::process::Command::new("jj")
        .args(["log", "-r", "root()", "--no-graph", "-T", "commit_id"])
        .current_dir(dir)
        .output()
        .expect("jj log root could not be started");
    assert!(
        output.status.success(),
        "jj log root failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[test]
fn test_handoff_done_writes_result_file() {
    let dir = tempfile::tempdir().unwrap();
    let commit = create_jj_repo(dir.path());
    if commit.is_empty() {
        return;
    }
    let base = get_base_commit(dir.path());

    // Create .grindbot/base_commit
    let grindbot_dir = dir.path().join(".grindbot");
    std::fs::create_dir_all(&grindbot_dir).unwrap();
    std::fs::write(grindbot_dir.join("base_commit"), &base).unwrap();

    // Run handoff done without requiring the agent to create a manifest file.
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_grindbot"))
        .args([
            "handoff",
            "done",
            "--commit",
            &commit,
            "--plan-review",
            "accepted",
            "--implementation-review",
            "accepted",
            "--all-tests-passed",
            "--summary",
            "done",
        ])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "handoff done failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Check result.json
    let result_path = grindbot_dir.join("result.json");
    assert!(result_path.exists());

    let content = std::fs::read_to_string(&result_path).unwrap();
    let result: HandoffResult = serde_json::from_str(&content).unwrap();

    match result {
        HandoffResult::Done { commit: c, .. } => {
            assert_eq!(c, commit);
        }
        _ => panic!("expected Done result"),
    }
}

#[test]
fn test_handoff_done_invalid_commit_fails() {
    let dir = tempfile::tempdir().unwrap();
    let commit = create_jj_repo(dir.path());
    if commit.is_empty() {
        return;
    }
    let base = get_base_commit(dir.path());

    let grindbot_dir = dir.path().join(".grindbot");
    std::fs::create_dir_all(&grindbot_dir).unwrap();
    std::fs::write(grindbot_dir.join("base_commit"), &base).unwrap();

    // Run handoff done with a non-existent commit.
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_grindbot"))
        .args([
            "handoff",
            "done",
            "--commit",
            "nonexistent123456",
            "--plan-review",
            "accepted",
            "--implementation-review",
            "accepted",
            "--all-tests-passed",
        ])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(!output.status.success());

    // result.json should not exist
    assert!(!grindbot_dir.join("result.json").exists());
}

#[test]
fn test_handoff_needs_feedback_writes_result_file() {
    let dir = tempfile::tempdir().unwrap();
    let commit = create_jj_repo(dir.path());
    if commit.is_empty() {
        return;
    }

    let grindbot_dir = dir.path().join(".grindbot");
    std::fs::create_dir_all(&grindbot_dir).unwrap();

    // Run handoff needs-feedback
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_grindbot"))
        .args([
            "handoff",
            "needs-feedback",
            "--message",
            "Need more info about the API",
        ])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "handoff needs-feedback failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Check result.json
    let result_path = grindbot_dir.join("result.json");
    assert!(result_path.exists());

    let content = std::fs::read_to_string(&result_path).unwrap();
    let result: HandoffResult = serde_json::from_str(&content).unwrap();

    match result {
        HandoffResult::NeedsFeedback { message, .. } => {
            assert_eq!(message, "Need more info about the API");
        }
        _ => panic!("expected NeedsFeedback result"),
    }
}

// AC.12: --message-file reads from file and writes to result.json
#[test]
fn test_handoff_needs_feedback_message_file() {
    let dir = tempfile::tempdir().unwrap();
    let _ = create_jj_repo(dir.path());

    let grindbot_dir = dir.path().join(".grindbot");
    std::fs::create_dir_all(&grindbot_dir).unwrap();

    // Write a message file
    let message_path = dir.path().join("message.txt");
    std::fs::write(&message_path, "Need more info about the API endpoint").unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_grindbot"))
        .args([
            "handoff",
            "needs-feedback",
            "--message-file",
            message_path.to_str().unwrap(),
        ])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "handoff needs-feedback --message-file failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let result_path = grindbot_dir.join("result.json");
    assert!(result_path.exists());

    let content = std::fs::read_to_string(&result_path).unwrap();
    let result: HandoffResult = serde_json::from_str(&content).unwrap();

    match result {
        HandoffResult::NeedsFeedback { message, .. } => {
            assert_eq!(message, "Need more info about the API endpoint");
        }
        _ => panic!("expected NeedsFeedback result"),
    }
}

// AC.12: Providing neither --message nor --message-file produces a clap error
#[test]
fn test_handoff_needs_feedback_no_message_errors() {
    let dir = tempfile::tempdir().unwrap();
    let _ = create_jj_repo(dir.path());

    let grindbot_dir = dir.path().join(".grindbot");
    std::fs::create_dir_all(&grindbot_dir).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_grindbot"))
        .args(["handoff", "needs-feedback"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "handoff needs-feedback without message or message-file should fail"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--message")
            || stderr.contains("--message-file")
            || stderr.contains("required"),
        "error should mention --message or --message-file; got: {}",
        stderr
    );
}

// AC.12: --message and --message-file are mutually exclusive
#[test]
fn test_handoff_needs_feedback_both_message_and_file_errors() {
    let dir = tempfile::tempdir().unwrap();
    let _ = create_jj_repo(dir.path());

    let grindbot_dir = dir.path().join(".grindbot");
    std::fs::create_dir_all(&grindbot_dir).unwrap();

    let message_path = dir.path().join("message.txt");
    std::fs::write(&message_path, "from file").unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_grindbot"))
        .args([
            "handoff",
            "needs-feedback",
            "--message",
            "from flag",
            "--message-file",
            message_path.to_str().unwrap(),
        ])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "handoff needs-feedback with both --message and --message-file should fail"
    );
}

// AC.1: `handoff done --help` shows the new --all-tests-passed and --plan-review args
#[test]
fn test_handoff_done_help_shows_args() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_grindbot"))
        .args(["handoff", "done", "--help"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "handoff done --help failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--all-tests-passed"),
        "help output should mention --all-tests-passed; got: {}",
        stdout
    );
    assert!(
        stdout.contains("--plan-review"),
        "help output should mention --plan-review; got: {}",
        stdout
    );
}

// AC.2: `handoff needs-feedback --help` shows help text for --message
#[test]
fn test_handoff_needs_feedback_help_shows_usage() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_grindbot"))
        .args(["handoff", "needs-feedback", "--help"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "handoff needs-feedback --help failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Feedback message text"),
        "help output should contain help text for --message; got: {}",
        stdout
    );
}

// AC.3: `handoff done` without --all-tests-passed fails with a --help hint
#[test]
fn test_handoff_done_missing_all_tests_passed_fails() {
    let dir = tempfile::tempdir().unwrap();
    let _ = create_jj_repo(dir.path());

    let grindbot_dir = dir.path().join(".grindbot");
    std::fs::create_dir_all(&grindbot_dir).unwrap();

    // Write a dummy base_commit so find_workspace_root + validate_commit can
    // proceed; the evidence check should fail before commit validation.
    let base = get_base_commit(dir.path());
    std::fs::write(grindbot_dir.join("base_commit"), &base).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_grindbot"))
        .args([
            "handoff",
            "done",
            "--commit",
            "@",
            "--plan-review",
            "accepted",
            "--implementation-review",
            "accepted",
            "--summary",
            "no tests flag",
        ])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "handoff done without --all-tests-passed should fail"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--help"),
        "error message should mention --help; got: {}",
        stderr
    );
}
