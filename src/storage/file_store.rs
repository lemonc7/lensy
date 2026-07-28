use std::{
    io,
    path::{Component, Path, PathBuf},
};

#[derive(Clone)]
pub struct FileStore {
    root: PathBuf,
}

impl FileStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub async fn initialize(&self) -> io::Result<()> {
        for dir in ["images", "thumbnails", "tmp", "trash"] {
            tokio::fs::create_dir_all(self.root.join(dir)).await?
        }
        Ok(())
    }

    pub fn resolve(&self, storage_key: &str) -> io::Result<PathBuf> {
        let relative = Path::new(storage_key);
        if relative.is_absolute()
            || relative
                .components()
                .any(|part| matches!(part, Component::ParentDir))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid storage key",
            ));
        }

        Ok(self.root.join(relative))
    }

    pub async fn promote(&self, temporary_path: &Path, storage_key: &str) -> io::Result<PathBuf> {
        let destination = self.resolve(storage_key)?;
        if let Some(parent) = destination.parent() {
            tokio::fs::create_dir_all(parent).await?
        }

        tokio::fs::rename(temporary_path, &destination).await?;
        Ok(destination)
    }
}
