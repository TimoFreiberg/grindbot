//! AC.6: grindbot supervise --dry-run prints planned actions and exits
//! without modifying the state file or creating workspaces.

use std::process::Command;

fn make_config(dir: &std::path::Path) -> std::path::PathBuf {
    let config_path = dir.join("grindbot.toml");
    std::fs::write(
        &config_path,
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
binary = "polytoken"
max_tool_turns = 200

[workspace]
prefix = "grindbot"
workspaces_dir = ".grindbot-workspaces"
"#,
    )
    .unwrap();
    config_path
}

#[test]
fn test_dry_run_no_side_effects() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = make_config(dir.path());

    // Set HOME to temp dir so state file is isolated
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).unwrap();

    // Snapshot: no state file should exist before
    let state_path = home.join(".local/share/grindbot/test/test/state.json");
    assert!(!state_path.exists(), "state file should not exist before dry-run");

    let output = Command::new(env!("CARGO_BIN_EXE_grindbot"))
        .args([
            "supervise",
            "--config",
            config_path.to_str().unwrap(),
            "--dry-run",
        ])
        .env("HOME", &home)
        .output()
        .expect("failed to run grindbot supervise --dry-run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "dry-run should exit 0; stderr: {}",
        stderr
    );

    // Should print planned actions header (even if state gathering fails,
    // dry-run should exit 0 without side effects)
    assert!(
        stdout.contains("Planned actions (dry run)"),
        "dry-run should print planned actions header; got: {}",
        stdout
    );

    // State file should NOT exist after dry-run (no side effects)
    assert!(
        !state_path.exists(),
        "state file should not be created during dry-run"
    );
}
