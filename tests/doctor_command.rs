//! AC.5: grindbot doctor checks dependencies and exits 1 on failure, 0 on success.

use std::process::Command;

fn make_config_with_polytoken_binary(dir: &std::path::Path, binary: &str) -> std::path::PathBuf {
    let config_path = dir.join("grindbot.toml");
    std::fs::write(
        &config_path,
        &format!(
            r#"
[github]
owner = "test"
repo = "test"
allowlist = ["alice"]

[supervisor]
max_parallelism = 2
poll_interval_secs = 30
base_branch = "main"

[polytoken]
binary = "{}"
max_tool_turns = 200

[workspace]
prefix = "grindbot"
workspaces_dir = ".grindbot-workspaces"
"#,
            binary
        ),
    )
    .unwrap();
    config_path
}

#[test]
fn test_doctor_reports_missing_binary() {
    let dir = tempfile::tempdir().unwrap();
    // Use a deliberately nonexistent binary name for polytoken
    let config_path = make_config_with_polytoken_binary(dir.path(), "nonexistent-polytoken-binary-xyz");

    let output = Command::new(env!("CARGO_BIN_EXE_grindbot"))
        .args(["doctor", "--config", config_path.to_str().unwrap()])
        .output()
        .expect("failed to run grindbot doctor");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // The nonexistent polytoken binary should be reported as ✗
    assert!(
        stdout.contains("✗"),
        "doctor should report failure for missing binary; got: {}",
        stdout
    );
    assert!(
        stdout.contains("nonexistent-polytoken-binary-xyz"),
        "doctor should name the missing binary; got: {}",
        stdout
    );

    // Should exit with non-zero status
    assert!(
        !output.status.success(),
        "doctor should exit non-zero when checks fail"
    );
}

#[test]
fn test_doctor_no_config_still_runs() {
    // Without a config file, doctor should still run binary checks
    let dir = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_grindbot"))
        .args(["doctor"])
        .current_dir(dir.path())
        .output()
        .expect("failed to run grindbot doctor");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Checking grindbot dependencies"),
        "doctor should print header; got: {}",
        stdout
    );
    // jj should be checked
    assert!(stdout.contains("jj"), "doctor should check jj; got: {}", stdout);
}

#[cfg(ignore)]
#[test]
fn test_doctor_passes_with_all_present() {
    // This test is environment-dependent: requires jj, gh (authenticated), polytoken
    let dir = tempfile::tempdir().unwrap();
    let config_path = make_config_with_polytoken_binary(dir.path(), "polytoken");

    let output = Command::new(env!("CARGO_BIN_EXE_grindbot"))
        .args(["doctor", "--config", config_path.to_str().unwrap()])
        .output()
        .expect("failed to run grindbot doctor");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("All checks passed"), "got: {}", stdout);
    assert!(output.status.success());
}
