use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::io::Filesystem;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MergeLockMetadata {
    pub ticket: u64,
    pub workspace: String,
    pub session: String,
    pub owner: String,
    pub timestamp: String,
}

pub struct MergeLock {
    fs: Arc<dyn Filesystem>,
    path: String,
}

impl MergeLock {
    pub fn acquire(
        fs: Arc<dyn Filesystem>,
        main_repo: &str,
        ticket: u64,
        workspace: &str,
        session: &str,
        owner: &str,
    ) -> anyhow::Result<Self> {
        let path = format!("{main_repo}/.grindbot/merge.lock");
        let metadata = MergeLockMetadata {
            ticket,
            workspace: workspace.to_string(),
            session: session.to_string(),
            owner: owner.to_string(),
            timestamp: Utc::now().to_rfc3339(),
        };
        let content = serde_json::to_string_pretty(&metadata)?;
        if !fs.try_create_exclusive(&path, &content)? {
            let old = fs.read_to_string(&path).unwrap_or_else(|_| "unknown owner".into());
            anyhow::bail!("merge lock is held: {}", old);
        }
        Ok(Self { fs, path })
    }

    pub fn release(&self) -> anyhow::Result<()> {
        self.fs.remove_file(&self.path)
    }
}

impl Drop for MergeLock {
    fn drop(&mut self) {
        let _ = self.release();
    }
}
