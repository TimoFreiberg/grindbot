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

fn make_workspace_fixture() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join(".jj")).unwrap();
    let config_path =
        make_config_with_polytoken_binary(dir.path(), "nonexistent-polytoken-binary-xyz");
    let workspace = dir.path().join(".grindbot-workspaces").join("grindbot-1");
    std::fs::create_dir_all(workspace.join(".jj")).unwrap();
    (dir, config_path)
}

fn run_doctor_in_fixture(
    dir: &std::path::Path,
    config_path: &std::path::Path,
) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_grindbot"))
        .args(["doctor", "--config", config_path.to_str().unwrap()])
        .current_dir(dir)
        .output()
        .expect("failed to run grindbot doctor")
}

#[test]
fn test_doctor_warns_about_workspace_runtime_paths() {
    let (dir, config_path) = make_workspace_fixture();
    let output = run_doctor_in_fixture(dir.path(), &config_path);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("WARNING workspace ignores missing"),
        "got: {stdout}"
    );
    assert!(stdout.contains(".grindbot/"), "got: {stdout}");
    assert!(stdout.contains(".polytoken/"), "got: {stdout}");
}

#[test]
fn test_doctor_accepts_workspace_runtime_ignore_rules() {
    let (dir, config_path) = make_workspace_fixture();
    let workspace = dir.path().join(".grindbot-workspaces/grindbot-1");
    std::fs::write(workspace.join(".gitignore"), "/.grindbot/\n.polytoken\n").unwrap();

    let output = run_doctor_in_fixture(dir.path(), &config_path);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("WARNING workspace ignores missing"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("workspace ignores covered"),
        "got: {stdout}"
    );
}

#[test]
fn test_doctor_ignore_warning_is_non_fatal() {
    let (dir, config_path) = make_workspace_fixture();
    let output = run_doctor_in_fixture(dir.path(), &config_path);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("WARNING workspace ignores missing"),
        "got: {stdout}"
    );
    assert!(
        !output.status.success(),
        "fixture dependency checks should still fail independently"
    );
}

#[test]
fn test_doctor_reports_missing_binary() {
    let dir = tempfile::tempdir().unwrap();
    // Use a deliberately nonexistent binary name for polytoken
    let config_path =
        make_config_with_polytoken_binary(dir.path(), "nonexistent-polytoken-binary-xyz");

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
    assert!(
        stdout.contains("jj"),
        "doctor should check jj; got: {}",
        stdout
    );
    assert!(
        stdout.contains("no managed workspace directory found"),
        "doctor should report the workspace-ignore check is ready for a future workspace; got: {}",
        stdout
    );
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
