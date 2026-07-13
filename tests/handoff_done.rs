//! AC.3: grindbot handoff done validates commit and writes result.json.
//! AC.4: grindbot handoff needs-feedback writes result.json.

mod common;

use grindbot::core::state::HandoffResult;
use std::path::Path;

fn create_jj_repo(dir: &Path) -> String {
    // Initialize a jj repo
    std::process::Command::new("jj")
        .args(["git", "init", "--colocate"])
        .current_dir(dir)
        .output()
        .expect("jj git init failed");

    // Create a file and commit
    std::fs::write(dir.join("test.txt"), "hello").unwrap();
    std::process::Command::new("jj")
        .args(["new"])
        .current_dir(dir)
        .output()
        .expect("jj new failed");

    // Get the current commit hash
    let output = std::process::Command::new("jj")
        .args(["log", "-r", "@", "--no-graph", "-T", "commit_id"])
        .current_dir(dir)
        .output()
        .expect("jj log failed");

    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn get_base_commit(dir: &Path) -> String {
    let output = std::process::Command::new("jj")
        .args(["log", "-r", "root()", "--no-graph", "-T", "commit_id"])
        .current_dir(dir)
        .output()
        .expect("jj log root failed");

    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[test]
fn test_handoff_done_writes_result_file() {
    let dir = tempfile::tempdir().unwrap();
    let commit = create_jj_repo(dir.path());
    let base = get_base_commit(dir.path());

    // Create .grindbot/base_commit
    let grindbot_dir = dir.path().join(".grindbot");
    std::fs::create_dir_all(&grindbot_dir).unwrap();
    std::fs::write(grindbot_dir.join("base_commit"), &base).unwrap();

    // Run handoff done
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_grindbot"))
        .args(["handoff", "done", "--commit", &commit])
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
    let _ = create_jj_repo(dir.path());
    let base = get_base_commit(dir.path());

    let grindbot_dir = dir.path().join(".grindbot");
    std::fs::create_dir_all(&grindbot_dir).unwrap();
    std::fs::write(grindbot_dir.join("base_commit"), &base).unwrap();

    // Run handoff done with a non-existent commit
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_grindbot"))
        .args(["handoff", "done", "--commit", "nonexistent123456"])
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
    let _ = create_jj_repo(dir.path());

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
