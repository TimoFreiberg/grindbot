use std::path::PathBuf;

use crate::config::Config;
use crate::io::Filesystem;
use crate::prompt::{PERMISSIONS_YAML, STOP_HOOK_SCRIPT, hooks_json, hooks_json_always_stop};

/// Set up a workspace for an implementer session.
///
/// Creates:
/// 1. The jj workspace
/// 2. `.grindbot/base_commit` file
/// 3. `.polytoken/hooks.json` with the stop hook
/// 4. `.polytoken/permissions.yaml` with deny rules
/// 5. Adds `.grindbot-workspaces/` to `.gitignore` (in the main repo)
pub fn setup_workspace(
    config: &Config,
    repo_path: &str,
    issue_number: u64,
    base_commit: &str,
    fs: &dyn Filesystem,
) -> anyhow::Result<String> {
    let workspace_name = config.workspace_name(issue_number);
    let workspaces_dir = format!("{}/{}", repo_path, config.workspace.workspaces_dir);
    let workspace_path = format!("{}/{}", workspaces_dir, workspace_name);

    // 1. Create the jj workspace
    // This is done by the JjClient in the supervisor, but we set up the files here.
    // The supervisor calls jj.create_workspace before calling this function.
    // Actually, let's have this function just create the config files.
    // The jj workspace creation is handled by the supervisor's execute_action.

    // 2. Create .grindbot/ directory and base_commit file
    let grindbot_dir = format!("{}/.grindbot", workspace_path);
    fs.create_dir_all(&grindbot_dir)?;
    fs.write(&format!("{}/base_commit", grindbot_dir), base_commit)?;

    // 3. Create .polytoken/hooks.json
    let polytoken_dir = format!("{}/.polytoken", workspace_path);
    fs.create_dir_all(&polytoken_dir)?;
    let hooks_content = hooks_json(STOP_HOOK_SCRIPT);
    fs.write(&format!("{}/hooks.json", polytoken_dir), &hooks_content)?;

    // 4. Create .polytoken/permissions.yaml
    fs.write(
        &format!("{}/permissions.yaml", polytoken_dir),
        PERMISSIONS_YAML,
    )?;

    // 5. Add .grindbot-workspaces/ to .gitignore in the main repo
    add_to_gitignore(repo_path, &config.workspace.workspaces_dir, fs)?;

    Ok(workspace_path)
}

/// Set up a workspace for a conflict resolution agent.
/// Uses the always-stop hook (no gating).
pub fn setup_conflict_resolution_workspace(
    workspace_path: &str,
    fs: &dyn Filesystem,
) -> anyhow::Result<()> {
    let polytoken_dir = format!("{}/.polytoken", workspace_path);

    // Overwrite hooks.json with always-stop hook
    let hooks_content = hooks_json_always_stop();
    fs.write(&format!("{}/hooks.json", polytoken_dir), &hooks_content)?;

    // Keep the same permissions.yaml
    fs.write(
        &format!("{}/permissions.yaml", polytoken_dir),
        PERMISSIONS_YAML,
    )?;

    Ok(())
}

/// Add a pattern to .gitignore if not already present.
fn add_to_gitignore(repo_path: &str, pattern: &str, fs: &dyn Filesystem) -> anyhow::Result<()> {
    let gitignore_path = format!("{}/.gitignore", repo_path);

    let existing = if fs.exists(&gitignore_path) {
        fs.read_to_string(&gitignore_path)?
    } else {
        String::new()
    };

    // Check if the pattern is already present
    if existing.lines().any(|line| line.trim() == pattern) {
        return Ok(());
    }

    // Append the pattern
    let new_content = if existing.is_empty() || existing.ends_with('\n') {
        format!("{}{}\n", existing, pattern)
    } else {
        format!("{}\n{}\n", existing, pattern)
    };

    fs.write(&gitignore_path, &new_content)?;
    Ok(())
}

/// Compute the workspace path for a given issue number.
pub fn workspace_path(config: &Config, repo_path: &str, issue_number: u64) -> PathBuf {
    PathBuf::from(repo_path)
        .join(&config.workspace.workspaces_dir)
        .join(config.workspace_name(issue_number))
}

/// Clean up a workspace: forget it in jj and remove the directory.
pub fn cleanup_workspace(
    _config: &Config,
    _workspace_name: &str,
    workspace_path: &str,
    fs: &dyn Filesystem,
) -> anyhow::Result<()> {
    // Remove the directory
    if fs.exists(workspace_path) {
        fs.remove_dir_all(workspace_path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct MockFilesystem {
        files: std::sync::Arc<Mutex<std::collections::HashMap<String, String>>>,
    }

    impl MockFilesystem {
        fn new() -> Self {
            Self {
                files: std::sync::Arc::new(Mutex::new(std::collections::HashMap::new())),
            }
        }

        fn get(&self, path: &str) -> Option<String> {
            self.files.lock().unwrap().get(path).cloned()
        }
    }

    impl Filesystem for MockFilesystem {
        fn read_to_string(&self, path: &str) -> anyhow::Result<String> {
            self.files
                .lock()
                .unwrap()
                .get(path)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("not found: {}", path))
        }

        fn write(&self, path: &str, content: &str) -> anyhow::Result<()> {
            self.files
                .lock()
                .unwrap()
                .insert(path.to_string(), content.to_string());
            Ok(())
        }

        fn exists(&self, path: &str) -> bool {
            self.files.lock().unwrap().contains_key(path)
        }

        fn remove_dir_all(&self, _path: &str) -> anyhow::Result<()> {
            Ok(())
        }

        fn create_dir_all(&self, _path: &str) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn test_setup_workspace_creates_files() {
        let config = Config::default();
        let fs = MockFilesystem::new();
        let repo_path = "/tmp/test-repo";

        setup_workspace(&config, repo_path, 42, "abc123", &fs).unwrap();

        // Check base_commit
        let base = fs
            .get("/tmp/test-repo/.grindbot-workspaces/grindbot-42/.grindbot/base_commit")
            .unwrap();
        assert_eq!(base, "abc123");

        // Check hooks.json exists
        let hooks = fs
            .get("/tmp/test-repo/.grindbot-workspaces/grindbot-42/.polytoken/hooks.json")
            .unwrap();
        assert!(hooks.contains("grindbot-gate"));
        assert!(hooks.contains("stop"));

        // Check permissions.yaml exists
        let perms = fs
            .get("/tmp/test-repo/.grindbot-workspaces/grindbot-42/.polytoken/permissions.yaml")
            .unwrap();
        assert!(perms.contains("deny"));
        assert!(perms.contains("rm"));
        assert!(perms.contains("git"));
    }
}
