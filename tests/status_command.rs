//! AC.4: grindbot status smoke test (exit 0, expected headers).

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
fn test_status_command_smoke() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = make_config(dir.path());

    // Set HOME to the temp dir so the state file path is isolated
    let output = Command::new(env!("CARGO_BIN_EXE_grindbot"))
        .args(["status", "--config", config_path.to_str().unwrap()])
        .env("HOME", dir.path())
        .output()
        .expect("failed to run grindbot status");

    assert!(
        output.status.success(),
        "grindbot status failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Grindbot Status"),
        "output should contain 'Grindbot Status'; got: {}",
        stdout
    );
    assert!(
        stdout.contains("test/test"),
        "output should contain owner/repo; got: {}",
        stdout
    );
    // With no state file, should show (none) for active implementers
    assert!(
        stdout.contains("(none)"),
        "output should show (none) for empty state; got: {}",
        stdout
    );
}
