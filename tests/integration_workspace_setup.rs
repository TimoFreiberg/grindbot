//! AC.10: Workspace setup creates the jj workspace, .polytoken/hooks.json,
//! .polytoken/permissions.yaml, and .grindbot/base_commit correctly.

mod common;

use grindbot::config::Config;
use grindbot::io::Filesystem;
use grindbot::prompt::{PERMISSIONS_YAML, STOP_HOOK_SCRIPT};
use grindbot::workspace;

use common::MockFilesystem;

#[test]
fn test_workspace_setup_creates_all_files() {
    let config = Config::default();
    let fs = MockFilesystem::new();
    let repo_path = "/tmp/test-repo";

    let workspace_path = workspace::setup_workspace(&config, repo_path, 42, "abc123", &fs).unwrap();

    // Check base_commit
    let base = fs
        .read_to_string(&format!("{}/.grindbot/base_commit", workspace_path))
        .unwrap();
    assert_eq!(base, "abc123");

    // Check hooks.json
    let hooks = fs
        .read_to_string(&format!("{}/.polytoken/hooks.json", workspace_path))
        .unwrap();
    assert!(hooks.contains("grindbot-gate"));
    assert!(hooks.contains("stop"));
    assert!(hooks.contains("bash"));

    // Check permissions.yaml
    let perms = fs
        .read_to_string(&format!("{}/.polytoken/permissions.yaml", workspace_path))
        .unwrap();
    assert!(perms.contains("deny"));
    assert!(perms.contains("rm"));
    assert!(perms.contains("git"));
    assert!(perms.contains("push"));
    assert!(perms.contains("jj"));
    assert!(perms.contains("abandon"));
    assert!(perms.contains(".grindbot"));
    assert!(perms.contains(".polytoken"));

    // Check gitignore
    let gitignore = fs
        .read_to_string(&format!("{}/.gitignore", repo_path))
        .unwrap();
    assert!(gitignore.contains(".grindbot-workspaces"));
}

#[test]
fn test_workspace_setup_uses_config_prefix() {
    let mut config = Config::default();
    config.workspace.prefix = "custom".to_string();
    let fs = MockFilesystem::new();

    let workspace_path = workspace::setup_workspace(&config, "/repo", 7, "base", &fs).unwrap();

    assert!(workspace_path.contains("custom-7"));
}

#[test]
fn test_permissions_yaml_content() {
    // Verify the permissions YAML has the expected deny rules
    assert!(PERMISSIONS_YAML.contains("version: 2"));
    assert!(PERMISSIONS_YAML.contains("flags_present"));
    assert!(PERMISSIONS_YAML.contains("-rf"));
    assert!(PERMISSIONS_YAML.contains("--recursive"));
    assert!(PERMISSIONS_YAML.contains("subcommand: push"));
    assert!(PERMISSIONS_YAML.contains("subcommand: git"));
    assert!(PERMISSIONS_YAML.contains("subcommand: abandon"));
}

#[test]
fn test_stop_hook_script_content() {
    // Verify the stop hook script has the expected logic
    assert!(STOP_HOOK_SCRIPT.contains("POLYTOKEN_PROJECT_DIR"));
    assert!(STOP_HOOK_SCRIPT.contains("result.json"));
    assert!(STOP_HOOK_SCRIPT.contains("handoff done --help"));
    assert!(STOP_HOOK_SCRIPT.contains("stop_counter"));
    assert!(STOP_HOOK_SCRIPT.contains(r#""outcome":"stop""#));
    assert!(STOP_HOOK_SCRIPT.contains(r#""outcome":"continue""#));
    assert!(STOP_HOOK_SCRIPT.contains("\"$COUNT\" -ge 3"));
}

#[test]
fn test_conflict_resolution_workspace_setup() {
    let fs = MockFilesystem::new();
    let workspace_path = "/tmp/test-ws";

    // Pre-create the polytoken dir
    fs.create_dir_all(&format!("{}/.polytoken", workspace_path))
        .unwrap();

    grindbot::workspace::setup_conflict_resolution_workspace(workspace_path, &fs).unwrap();

    let hooks = fs
        .read_to_string(&format!("{}/.polytoken/hooks.json", workspace_path))
        .unwrap();
    // The always-stop hook should NOT contain "continue"
    assert!(hooks.contains("grindbot-gate"));
    assert!(hooks.contains("stop"));
    assert!(!hooks.contains("continue"));
}
