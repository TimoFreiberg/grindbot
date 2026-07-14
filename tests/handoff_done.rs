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

    std::fs::write(dir.path().join("manifest.json"), serde_json::json!({
        "status": "done", "manifest_version": 1, "commit": commit,
        "timestamp": "2024-01-01T00:00:00Z", "summary": "done",
        "evidence": {"plan_review": "accepted", "implementation_review": "accepted",
        "tests": [{"name": "cargo test", "result": "passed"}],
        "acceptance_mapping": [{"acceptance_criterion": "AC", "verification": "test"}],
        "unresolved_findings": false}
    }).to_string()).unwrap();

    // Run handoff done
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_grindbot"))
        .args(["handoff", "done", "--manifest", "manifest.json"])
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

    std::fs::write(dir.path().join("manifest.json"), serde_json::json!({
        "status": "done", "manifest_version": 1, "commit": "nonexistent123456",
        "timestamp": "2024-01-01T00:00:00Z", "evidence": {
        "plan_review": "accepted", "implementation_review": "accepted",
        "tests": [{"name": "test", "result": "passed"}],
        "acceptance_mapping": [{"acceptance_criterion": "AC", "verification": "test"}],
        "unresolved_findings": false}
    }).to_string()).unwrap();

    // Run handoff done with a non-existent commit
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_grindbot"))
        .args(["handoff", "done", "--manifest", "manifest.json"])
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
        stderr.contains("--message") || stderr.contains("--message-file") || stderr.contains("required"),
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
