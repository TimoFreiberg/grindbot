use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub github: GithubConfig,
    pub supervisor: SupervisorConfig,
    pub polytoken: PolytokenConfig,
    pub workspace: WorkspaceConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GithubConfig {
    pub owner: String,
    pub repo: String,
    pub allowlist: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SupervisorConfig {
    #[serde(default = "default_parallelism")]
    pub max_parallelism: usize,
    #[serde(default = "default_poll_interval")]
    pub poll_interval_secs: u64,
    #[serde(default = "default_base_branch")]
    pub base_branch: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PolytokenConfig {
    #[serde(default = "default_polytoken_binary")]
    pub binary: String,
    #[serde(default = "default_max_tool_turns")]
    pub max_tool_turns: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    #[serde(default = "default_workspace_prefix")]
    pub prefix: String,
    #[serde(default = "default_workspaces_dir")]
    pub workspaces_dir: String,
}

fn default_parallelism() -> usize {
    2
}
fn default_poll_interval() -> u64 {
    30
}
fn default_base_branch() -> String {
    "main".to_string()
}
fn default_polytoken_binary() -> String {
    "polytoken".to_string()
}
fn default_max_tool_turns() -> u32 {
    200
}
fn default_workspace_prefix() -> String {
    "grindbot".to_string()
}
fn default_workspaces_dir() -> String {
    ".grindbot-workspaces".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Config {
            github: GithubConfig {
                owner: String::new(),
                repo: String::new(),
                allowlist: vec![],
            },
            supervisor: SupervisorConfig {
                max_parallelism: default_parallelism(),
                poll_interval_secs: default_poll_interval(),
                base_branch: default_base_branch(),
            },
            polytoken: PolytokenConfig {
                binary: default_polytoken_binary(),
                max_tool_turns: default_max_tool_turns(),
            },
            workspace: WorkspaceConfig {
                prefix: default_workspace_prefix(),
                workspaces_dir: default_workspaces_dir(),
            },
        }
    }
}

impl Config {
    /// Load config from a TOML file path.
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    /// Workspace name for a given issue number: e.g. "grindbot-42"
    pub fn workspace_name(&self, issue_number: u64) -> String {
        format!("{}-{}", self.workspace.prefix, issue_number)
    }

    /// Validate config fields. Pure data check — no subprocess calls.
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.github.owner.is_empty() {
            anyhow::bail!("[github] owner must not be empty");
        }
        if self.github.repo.is_empty() {
            anyhow::bail!("[github] repo must not be empty");
        }
        if self.github.allowlist.is_empty() {
            anyhow::bail!("[github] allowlist must contain at least one GitHub username");
        }
        if self.supervisor.max_parallelism == 0 {
            anyhow::bail!("[supervisor] max_parallelism must be at least 1");
        }
        if self.supervisor.poll_interval_secs == 0 {
            anyhow::bail!("[supervisor] poll_interval_secs must be at least 1");
        }
        if self.supervisor.base_branch.is_empty() {
            anyhow::bail!("[supervisor] base_branch must not be empty");
        }
        if self.workspace.prefix.is_empty() {
            anyhow::bail!("[workspace] prefix must not be empty");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_config() -> Config {
        Config {
            github: GithubConfig {
                owner: "test".to_string(),
                repo: "test".to_string(),
                allowlist: vec!["alice".to_string()],
            },
            supervisor: SupervisorConfig {
                max_parallelism: 2,
                poll_interval_secs: 30,
                base_branch: "main".to_string(),
            },
            polytoken: PolytokenConfig {
                binary: "polytoken".to_string(),
                max_tool_turns: 200,
            },
            workspace: WorkspaceConfig {
                prefix: "grindbot".to_string(),
                workspaces_dir: ".grindbot-workspaces".to_string(),
            },
        }
    }

    #[test]
    fn test_config_validation_valid_config() {
        assert!(valid_config().validate().is_ok());
    }

    #[test]
    fn test_config_validation_empty_owner() {
        let mut cfg = valid_config();
        cfg.github.owner = String::new();
        let err = cfg.validate().unwrap_err().to_string();
        assert!(err.contains("[github] owner"), "got: {err}");
    }

    #[test]
    fn test_config_validation_empty_repo() {
        let mut cfg = valid_config();
        cfg.github.repo = String::new();
        let err = cfg.validate().unwrap_err().to_string();
        assert!(err.contains("[github] repo"), "got: {err}");
    }

    #[test]
    fn test_config_validation_empty_allowlist() {
        let mut cfg = valid_config();
        cfg.github.allowlist = vec![];
        let err = cfg.validate().unwrap_err().to_string();
        assert!(err.contains("[github] allowlist"), "got: {err}");
    }

    #[test]
    fn test_config_validation_zero_parallelism() {
        let mut cfg = valid_config();
        cfg.supervisor.max_parallelism = 0;
        let err = cfg.validate().unwrap_err().to_string();
        assert!(err.contains("max_parallelism"), "got: {err}");
    }

    #[test]
    fn test_config_validation_zero_poll_interval() {
        let mut cfg = valid_config();
        cfg.supervisor.poll_interval_secs = 0;
        let err = cfg.validate().unwrap_err().to_string();
        assert!(err.contains("poll_interval_secs"), "got: {err}");
    }

    #[test]
    fn test_config_validation_empty_base_branch() {
        let mut cfg = valid_config();
        cfg.supervisor.base_branch = String::new();
        let err = cfg.validate().unwrap_err().to_string();
        assert!(err.contains("base_branch"), "got: {err}");
    }

    #[test]
    fn test_config_validation_empty_prefix() {
        let mut cfg = valid_config();
        cfg.workspace.prefix = String::new();
        let err = cfg.validate().unwrap_err().to_string();
        assert!(err.contains("[workspace] prefix"), "got: {err}");
    }
}
