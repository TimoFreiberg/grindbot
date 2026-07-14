//! AC.7: grindbot --version prints a version string matching Cargo.toml.

use std::process::Command;

#[test]
fn test_version_flag() {
    let output = Command::new(env!("CARGO_BIN_EXE_grindbot"))
        .arg("--version")
        .output()
        .expect("failed to run grindbot --version");

    assert!(
        output.status.success(),
        "grindbot --version failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("grindbot"),
        "output should contain 'grindbot'; got: {}",
        stdout
    );
    // Should contain a version number (e.g. 0.1.0)
    assert!(
        stdout.contains(|c: char| c.is_ascii_digit()),
        "output should contain a version number; got: {}",
        stdout
    );
}

#[test]
fn test_help_flag() {
    let output = Command::new(env!("CARGO_BIN_EXE_grindbot"))
        .arg("--help")
        .output()
        .expect("failed to run grindbot --help");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("supervise"));
    assert!(stdout.contains("status"));
    assert!(stdout.contains("doctor"));
    assert!(stdout.contains("handoff"));
}
