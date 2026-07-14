use serde::{Deserialize, Serialize};

use crate::config::Config;

/// The supervisor's persistent state file.
/// Tracks active implementers, completed tasks, and needs-feedback tasks.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StateFile {
    pub version: u32,
    pub active_implementers: Vec<ActiveImplementer>,
    pub completed_tasks: Vec<CompletedTask>,
    pub needs_feedback: Vec<NeedsFeedbackTask>,
    /// Tracks conflict-retry count per issue.
    #[serde(default)]
    pub conflict_retries: Vec<ConflictRetry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActiveImplementer {
    pub issue_number: u64,
    pub session_id: String,
    pub workspace_name: String,
    pub workspace_path: String,
    pub base_commit: String,
    pub started_at: String,
    #[serde(default)]
    pub port: u16,
    #[serde(default)]
    pub bearer_token: String,
    #[serde(default)]
    pub credential_file: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompletedTask {
    pub issue_number: u64,
    pub commit: String,
    pub completed_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NeedsFeedbackTask {
    pub issue_number: u64,
    pub message: String,
    pub timestamp: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConflictRetry {
    pub issue_number: u64,
    pub count: u32,
}

impl Default for StateFile {
    fn default() -> Self {
        Self {
            version: 1,
            active_implementers: vec![],
            completed_tasks: vec![],
            needs_feedback: vec![],
            conflict_retries: vec![],
        }
    }
}

impl StateFile {
    /// Load the state file from the default path.
    pub fn load(config: &Config) -> anyhow::Result<Self> {
        let path = Self::default_path(config);
        Self::load_from(&path)
    }

    /// Load from a specific path. If the file doesn't exist, return a fresh state.
    /// If the version doesn't match, discard the old state with a warning.
    pub fn load_from(path: &std::path::Path) -> anyhow::Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(path)?;
        match serde_json::from_str::<StateFile>(&content) {
            Ok(state) if state.version == 1 => Ok(state),
            Ok(state) => {
                tracing::warn!(
                    "state file version {} does not match expected version 1; starting fresh",
                    state.version
                );
                Ok(Self::default())
            }
            Err(e) => {
                tracing::warn!("failed to parse state file: {}; starting fresh", e);
                Ok(Self::default())
            }
        }
    }

    /// Save the state file atomically (write to temp file, then rename).
    pub fn save(&self, config: &Config) -> anyhow::Result<()> {
        let path = Self::default_path(config);
        self.save_to(&path)
    }

    /// Save to a specific path atomically.
    pub fn save_to(&self, path: &std::path::Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        let tmp_path = path.with_extension("json.tmp");
        std::fs::write(&tmp_path, content)?;
        std::fs::rename(&tmp_path, path)?;
        Ok(())
    }

    /// Default state file path: ~/.local/share/grindbot/{owner}/{repo}/state.json
    fn default_path(config: &Config) -> std::path::PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        std::path::PathBuf::from(home)
            .join(".local/share/grindbot")
            .join(&config.github.owner)
            .join(&config.github.repo)
            .join("state.json")
    }

    /// Add an active implementer.
    pub fn add_implementer(&mut self, imp: ActiveImplementer) {
        // Remove any existing entry for the same issue
        self.active_implementers
            .retain(|i| i.issue_number != imp.issue_number);
        self.active_implementers.push(imp);
    }

    /// Remove an active implementer by workspace name.
    pub fn remove_implementer(&mut self, workspace_name: &str) {
        self.active_implementers
            .retain(|i| i.workspace_name != workspace_name);
    }

    /// Mark a task as completed.
    pub fn add_completed(&mut self, task: CompletedTask) {
        self.completed_tasks
            .retain(|t| t.issue_number != task.issue_number);
        self.completed_tasks.push(task);
    }

    /// Add a needs-feedback task.
    pub fn add_needs_feedback(&mut self, task: NeedsFeedbackTask) {
        self.needs_feedback
            .retain(|t| t.issue_number != task.issue_number);
        self.needs_feedback.push(task);
    }

    /// Get the list of completed issue numbers.
    pub fn completed_issues(&self) -> Vec<u64> {
        self.completed_tasks.iter().map(|t| t.issue_number).collect()
    }

    /// Get the list of active issue numbers.
    pub fn active_issues(&self) -> Vec<u64> {
        self.active_implementers
            .iter()
            .map(|i| i.issue_number)
            .collect()
    }

    /// Increment conflict retry count for an issue. Returns the new count.
    pub fn increment_conflict_retry(&mut self, issue_number: u64) -> u32 {
        if let Some(retry) = self
            .conflict_retries
            .iter_mut()
            .find(|r| r.issue_number == issue_number)
        {
            retry.count += 1;
            return retry.count;
        }
        self.conflict_retries.push(ConflictRetry {
            issue_number,
            count: 1,
        });
        1
    }

    /// Get conflict retry count for an issue.
    pub fn conflict_retry_count(&self, issue_number: u64) -> u32 {
        self.conflict_retries
            .iter()
            .find(|r| r.issue_number == issue_number)
            .map(|r| r.count)
            .unwrap_or(0)
    }

    /// Reset conflict retry count for an issue.
    pub fn reset_conflict_retry(&mut self, issue_number: u64) {
        self.conflict_retries
            .retain(|r| r.issue_number != issue_number);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_file_roundtrip() {
        let mut state = StateFile::default();
        state.add_implementer(ActiveImplementer {
            issue_number: 42,
            session_id: "sess1".to_string(),
            workspace_name: "grindbot-42".to_string(),
            workspace_path: "/tmp/grindbot-42".to_string(),
            base_commit: "abc".to_string(),
            started_at: "2024-01-01T00:00:00Z".to_string(),
            port: 12345,
            bearer_token: "test-token".to_string(),
            credential_file: "/tmp/cred.json".to_string(),
        });
        state.add_completed(CompletedTask {
            issue_number: 40,
            commit: "def".to_string(),
            completed_at: "2024-01-01T00:00:00Z".to_string(),
        });

        let json = serde_json::to_string(&state).unwrap();
        let restored: StateFile = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.version, 1);
        assert_eq!(restored.active_implementers.len(), 1);
        assert_eq!(restored.completed_tasks.len(), 1);
        assert_eq!(restored.active_implementers[0].issue_number, 42);
        assert_eq!(restored.completed_tasks[0].issue_number, 40);
    }

    #[test]
    fn test_conflict_retry_tracking() {
        let mut state = StateFile::default();
        assert_eq!(state.conflict_retry_count(42), 0);
        assert_eq!(state.increment_conflict_retry(42), 1);
        assert_eq!(state.increment_conflict_retry(42), 2);
        assert_eq!(state.conflict_retry_count(42), 2);
        state.reset_conflict_retry(42);
        assert_eq!(state.conflict_retry_count(42), 0);
    }

    #[test]
    fn test_load_missing_file_returns_default() {
        let path = std::path::Path::new("/nonexistent/path/state.json");
        let state = StateFile::load_from(path).unwrap();
        assert_eq!(state.version, 1);
        assert!(state.active_implementers.is_empty());
    }

    #[test]
    fn test_version_mismatch_resets() {
        let json = r#"{"version":99,"active_implementers":[],"completed_tasks":[],"needs_feedback":[]}"#;
        let path = std::env::temp_dir().join("grindbot_test_version.json");
        std::fs::write(&path, json).unwrap();
        let state = StateFile::load_from(&path).unwrap();
        assert_eq!(state.version, 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_active_implementer_roundtrip_with_session_info() {
        let imp = ActiveImplementer {
            issue_number: 42,
            session_id: "sess-abc".to_string(),
            workspace_name: "grindbot-42".to_string(),
            workspace_path: "/tmp/grindbot-42".to_string(),
            base_commit: "abc123".to_string(),
            started_at: "2024-01-15T12:30:00Z".to_string(),
            port: 8080,
            bearer_token: "secret-token".to_string(),
            credential_file: "/tmp/creds.json".to_string(),
        };
        let json = serde_json::to_string(&imp).unwrap();
        let restored: ActiveImplementer = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.port, 8080);
        assert_eq!(restored.bearer_token, "secret-token");
        assert_eq!(restored.credential_file, "/tmp/creds.json");
        assert_eq!(restored.session_id, "sess-abc");
    }

    #[test]
    fn test_old_state_file_deserializes_with_defaults() {
        // Simulate an old state file that lacks port/bearer_token/credential_file
        let json = r#"{
            "version": 1,
            "active_implementers": [{
                "issue_number": 42,
                "session_id": "sess1",
                "workspace_name": "grindbot-42",
                "workspace_path": "/tmp/grindbot-42",
                "base_commit": "abc",
                "started_at": "2024-01-01T00:00:00Z"
            }],
            "completed_tasks": [],
            "needs_feedback": []
        }"#;
        let state: StateFile = serde_json::from_str(json).unwrap();
        assert_eq!(state.active_implementers.len(), 1);
        assert_eq!(state.active_implementers[0].port, 0);
        assert_eq!(state.active_implementers[0].bearer_token, "");
        assert_eq!(state.active_implementers[0].credential_file, "");
    }

    #[test]
    fn test_state_file_path_per_repo() {
        let config_a = Config {
            github: crate::config::GithubConfig {
                owner: "alice".to_string(),
                repo: "project-a".to_string(),
                allowlist: vec![],
            },
            ..Config::default()
        };
        let config_b = Config {
            github: crate::config::GithubConfig {
                owner: "alice".to_string(),
                repo: "project-b".to_string(),
                allowlist: vec![],
            },
            ..Config::default()
        };

        // SAFETY: This test runs single-threaded; no other code accesses HOME concurrently.
        unsafe {
            std::env::set_var("HOME", "/tmp/test-home");
        }
        let path_a = StateFile::default_path(&config_a);
        let path_b = StateFile::default_path(&config_b);

        assert!(
            path_a.ends_with("alice/project-a/state.json"),
            "path_a was: {:?}",
            path_a
        );
        assert!(
            path_b.ends_with("alice/project-b/state.json"),
            "path_b was: {:?}",
            path_b
        );
        assert_ne!(path_a, path_b);
    }
}
