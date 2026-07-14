use crate::io::Filesystem;

/// Real filesystem implementation using std::fs.
pub struct RealFilesystem;

impl RealFilesystem {
    pub fn new() -> Self {
        Self
    }
}

impl Default for RealFilesystem {
    fn default() -> Self {
        Self::new()
    }
}

impl Filesystem for RealFilesystem {
    fn read_to_string(&self, path: &str) -> anyhow::Result<String> {
        Ok(std::fs::read_to_string(path)?)
    }

    fn write(&self, path: &str, content: &str) -> anyhow::Result<()> {
        if let Some(parent) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)?;
        Ok(())
    }

    fn try_create_exclusive(&self, path: &str, content: &str) -> anyhow::Result<bool> {
        use std::io::Write;
        if let Some(parent) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        match std::fs::OpenOptions::new().write(true).create_new(true).open(path) {
            Ok(mut file) => { file.write_all(content.as_bytes())?; Ok(true) }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    fn exists(&self, path: &str) -> bool {
        std::path::Path::new(path).exists()
    }

    fn remove_dir_all(&self, path: &str) -> anyhow::Result<()> {
        Ok(std::fs::remove_dir_all(path)?)
    }

    fn remove_file(&self, path: &str) -> anyhow::Result<()> {
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    fn create_dir_all(&self, path: &str) -> anyhow::Result<()> {
        Ok(std::fs::create_dir_all(path)?)
    }
}
