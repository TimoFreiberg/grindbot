//! AC.5: The stop hook script correctly gates session end.

use grindbot::prompt::STOP_HOOK_SCRIPT;
use std::io::Write;

fn run_stop_hook(project_dir: &str) -> serde_json::Value {
    // Write the stop hook script to a temp file and execute it
    let mut tmp = tempfile::NamedTempFile::new().unwrap();
    tmp.write_all(STOP_HOOK_SCRIPT.as_bytes()).unwrap();
    tmp.flush().unwrap();

    let output = std::process::Command::new("bash")
        .arg(tmp.path())
        .env("POLYTOKEN_PROJECT_DIR", project_dir)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stop hook failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("stop hook must emit valid JSON")
}

fn setup_project_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join(".grindbot")).unwrap();
    dir
}

#[test]
fn test_stop_hook_allows_stop_with_result_file() {
    let dir = setup_project_dir();
    let result_path = dir.path().join(".grindbot/result.json");
    std::fs::write(&result_path, r#"{"status":"done","commit":"abc"}"#).unwrap();

    let output = run_stop_hook(dir.path().to_str().unwrap());

    assert_eq!(output["outcome"], "stop");
}

#[test]
fn test_stop_hook_prevents_stop_without_result_file() {
    let dir = setup_project_dir();
    // No result file

    let output = run_stop_hook(dir.path().to_str().unwrap());

    assert_eq!(output["outcome"], "continue");
    assert!(
        output["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("handoff"))
    );
}

#[test]
fn test_stop_hook_allows_stop_after_3_attempts() {
    let dir = setup_project_dir();
    // No result file

    // First attempt
    let output1 = run_stop_hook(dir.path().to_str().unwrap());
    assert_eq!(output1["outcome"], "continue");

    // Second attempt
    let output2 = run_stop_hook(dir.path().to_str().unwrap());
    assert_eq!(output2["outcome"], "continue");

    // Third attempt — should allow stop
    let output3 = run_stop_hook(dir.path().to_str().unwrap());
    assert_eq!(output3["outcome"], "stop");
}

#[test]
fn test_stop_hook_counter_resets_after_result_file() {
    let dir = setup_project_dir();

    // Two failed attempts
    run_stop_hook(dir.path().to_str().unwrap());
    run_stop_hook(dir.path().to_str().unwrap());

    // Now write the result file
    let result_path = dir.path().join(".grindbot/result.json");
    std::fs::write(&result_path, r#"{"status":"done","commit":"abc"}"#).unwrap();

    // Should allow stop immediately
    let output = run_stop_hook(dir.path().to_str().unwrap());
    assert_eq!(output["outcome"], "stop");
}
