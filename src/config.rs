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
}
