use std::{
    fs::{self, File},
    io::{self, Write},
    path::{Component, Path, PathBuf},
};

use tempfile::{Builder, NamedTempFile, TempDir};

use crate::backend::error::StorageError;

pub struct Storage {
    root: PathBuf,
    // temp_dir在storage被释放时会自动清理
    // 临时目录必须位于data根目录，确保临时文件和正式文件处于同一文件系统
    // rename/persist才能保持原子性
    temp_dir: TempDir,
}

impl Storage {
    pub fn new(root: impl AsRef<Path>) -> Result<Self, StorageError> {
        let root = absolute_path(root.as_ref())?;
        fs::create_dir_all(&root)?;
        let root = fs::canonicalize(root)?;

        create_storage_directory(&root, "images")?;
        create_storage_directory(&root, "thumbnails")?;
        let temp_root = create_storage_directory(&root, "tmp")?;
        let temp_dir = Builder::new().prefix("lensy-").tempdir_in(temp_root)?;
        Ok(Self { root, temp_dir })
    }

    pub fn save_image(
        &self,
        image_key: &str,
        image_data: &[u8],
        thumbnail_key: &str,
        thumbnail_data: &[u8],
    ) -> Result<(), StorageError> {
        let image_path = self.resolve(image_key)?;
        let thumbnail_path = self.resolve(thumbnail_key)?;

        if image_path == thumbnail_path {
            return Err(StorageError::InvalidKey(
                "原图和缩略图不能使用相同的存储键".to_owned(),
            ));
        }

        ensure_not_exists(&image_path)?;
        ensure_not_exists(&thumbnail_path)?;

        // 先写完两份临时文件，任何一步失败时，都会在drop时删除对应临时文件
        let temp_image = self.write_temporary(image_data)?;
        let temp_thumbnail = self.write_temporary(thumbnail_data)?;

        create_parent_directory(&image_path)?;
        create_parent_directory(&thumbnail_path)?;

        // 原图先转正
        persist_noclobber(temp_image, &image_path)?;

        // 缩略图转正失败，需要撤销已经落盘的原图
        if let Err(save_error) = persist_noclobber(temp_thumbnail, &thumbnail_path) {
            match remove_file_and_sync_parent(&image_path) {
                Ok(()) => return Err(save_error),
                Err(rollback_error) => {
                    return Err(StorageError::RollbackFailed {
                        save_error: Box::new(save_error),
                        rollback_error,
                    });
                }
            }
        }

        // 只有目录项也持久化后，调用方才能安全地提交数据库记录。
        sync_parent_directory(&image_path)?;
        sync_parent_directory(&thumbnail_path)?;

        Ok(())
    }

    pub fn open(&self, key: &str) -> Result<File, StorageError> {
        let path = self.resolve(key)?;
        Ok(File::open(path)?)
    }

    pub fn remove_image(&self, image_key: &str, thumbnail_key: &str) -> Result<(), StorageError> {
        let image_path = self.resolve(image_key)?;
        let thumbnail_path = self.resolve(thumbnail_key)?;

        let mut first_error: Option<StorageError> = None;

        for path in [&image_path, &thumbnail_path] {
            match fs::remove_file(path) {
                Ok(()) => {
                    if let Err(error) = sync_parent_directory(path)
                        && first_error.is_none()
                    {
                        first_error = Some(error);
                    }
                }
                // 文件不存在视为成功，保证删除操作幂等
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => {
                    if first_error.is_none() {
                        first_error = Some(StorageError::Io(error))
                    }
                }
            }
        }

        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    pub fn resolve(&self, key: &str) -> Result<PathBuf, StorageError> {
        validate_key(key)?;
        Ok(self.root.join(key))
    }

    fn write_temporary(&self, data: &[u8]) -> Result<NamedTempFile, StorageError> {
        let mut file = Builder::new()
            .prefix("lensy-")
            .suffix(".tmp")
            .tempfile_in(self.temp_dir.path())?;
        file.write_all(data)?;
        // 确保内容已经提交到底层文件系统，再将临时文件转正
        file.as_file_mut().sync_all()?;
        Ok(file)
    }
}

fn absolute_path(path: &Path) -> Result<PathBuf, io::Error> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn validate_key(key: &str) -> Result<(), StorageError> {
    if key.is_empty() {
        return Err(StorageError::InvalidKey(key.to_owned()));
    }

    let path = Path::new(key);
    let mut component_count = 0;

    for component in path.components() {
        match component {
            Component::Normal(_) => {
                component_count += 1;
            }
            // 拒绝绝对路径/Windows前缀/./..
            Component::Prefix(_)
            | Component::RootDir
            | Component::CurDir
            | Component::ParentDir => return Err(StorageError::InvalidKey(key.to_owned())),
        }
    }

    if component_count == 0 {
        return Err(StorageError::InvalidKey(key.to_owned()));
    }

    Ok(())
}

fn ensure_not_exists(path: &Path) -> Result<(), StorageError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Err(StorageError::AlreadyExists(path.to_path_buf())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(StorageError::Io(error)),
    }
}

fn create_parent_directory(path: &Path) -> Result<(), io::Error> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "存储路径没有父目录"))?;
    fs::create_dir_all(parent)
}

fn create_storage_directory(root: &Path, name: &str) -> Result<PathBuf, StorageError> {
    let path = root.join(name);
    fs::create_dir_all(&path)?;
    let canonical = fs::canonicalize(&path)?;
    if !canonical.starts_with(root) {
        return Err(StorageError::InvalidKey(format!(
            "存储目录必须位于数据根目录内: {}",
            path.display()
        )));
    }
    Ok(canonical)
}

fn sync_parent_directory(path: &Path) -> Result<(), StorageError> {
    let parent = path.parent().ok_or_else(|| {
        StorageError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "存储路径没有父目录",
        ))
    })?;

    sync_directory(parent).map_err(|source| StorageError::Durability {
        path: parent.to_path_buf(),
        source,
    })
}

fn remove_file_and_sync_parent(path: &Path) -> Result<(), io::Error> {
    match fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "存储路径没有父目录"))?;
    sync_directory(parent)
}

fn sync_directory(_path: &Path) -> Result<(), io::Error> {
    cfg_select! {
      unix => File::open(_path)?.sync_all(),
      _ => Ok(())
    }
}

fn persist_noclobber(temp: NamedTempFile, target: &Path) -> Result<(), StorageError> {
    match temp.persist_noclobber(target) {
        Ok(file) => {
            // 临时文件在persist前已经sync_all
            drop(file);
            Ok(())
        }
        Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {
            Err(StorageError::AlreadyExists(target.to_path_buf()))
        }
        Err(error) => Err(StorageError::Io(error.error)),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use tempfile::tempdir;

    use crate::backend::{error::StorageError, storage::Storage};

    #[test]
    fn saves_and_opens_image_files() {
        let root = tempdir().unwrap();
        let store = Storage::new(root.path()).unwrap();

        let image_key = "images/2026/08/abc.webp";

        let thumbnail_key = "thumbnails/2026/08/abc.webp";

        store
            .save_image(image_key, b"original", thumbnail_key, b"thumbnail")
            .unwrap();

        let mut image = Vec::new();

        store
            .open(image_key)
            .unwrap()
            .read_to_end(&mut image)
            .unwrap();

        assert_eq!(image, b"original");

        let mut thumbnail = Vec::new();

        store
            .open(thumbnail_key)
            .unwrap()
            .read_to_end(&mut thumbnail)
            .unwrap();

        assert_eq!(thumbnail, b"thumbnail");
    }

    #[test]
    fn refuses_to_overwrite_existing_files() {
        let root = tempdir().unwrap();
        let store = Storage::new(root.path()).unwrap();

        let image_key = "images/2026/08/abc.webp";

        let thumbnail_key = "thumbnails/2026/08/abc.webp";

        store
            .save_image(image_key, b"first image", thumbnail_key, b"first thumbnail")
            .unwrap();

        let error = store
            .save_image(
                image_key,
                b"second image",
                thumbnail_key,
                b"second thumbnail",
            )
            .unwrap_err();

        assert!(matches!(error, StorageError::AlreadyExists(_),));
    }

    #[test]
    fn rejects_unsafe_storage_keys() {
        let root = tempdir().unwrap();
        let store = Storage::new(root.path()).unwrap();

        for key in [
            "",
            ".",
            "..",
            "../image.webp",
            "images/../../image.webp",
            "/tmp/image.webp",
        ] {
            let result = store.resolve(key);

            assert!(
                matches!(result, Err(StorageError::InvalidKey(_)),),
                "key 应被拒绝: {key}",
            );
        }
    }

    #[test]
    fn remove_is_idempotent() {
        let root = tempdir().unwrap();
        let store = Storage::new(root.path()).unwrap();

        let image_key = "images/2026/08/abc.webp";

        let thumbnail_key = "thumbnails/2026/08/abc.webp";

        store
            .save_image(image_key, b"image", thumbnail_key, b"thumbnail")
            .unwrap();

        store.remove_image(image_key, thumbnail_key).unwrap();

        // 第二次删除仍然成功。
        store.remove_image(image_key, thumbnail_key).unwrap();

        assert!(!root.path().join(image_key).exists());
        assert!(!root.path().join(thumbnail_key).exists());
    }

    #[test]
    fn remove_is_idempotent_when_parent_directories_are_missing() {
        let root = tempdir().unwrap();
        let store = Storage::new(root.path()).unwrap();

        store
            .remove_image(
                "images/2099/12/missing.webp",
                "thumbnails/2099/12/missing.webp",
            )
            .unwrap();
    }

    #[test]
    fn does_not_leave_image_when_thumbnail_target_exists() {
        let root = tempdir().unwrap();
        let store = Storage::new(root.path()).unwrap();

        let image_key = "images/2026/08/new.webp";

        let thumbnail_key = "thumbnails/2026/08/existing.webp";

        let thumbnail_path = root.path().join(thumbnail_key);

        std::fs::create_dir_all(thumbnail_path.parent().unwrap()).unwrap();

        std::fs::write(&thumbnail_path, b"existing").unwrap();

        let error = store.save_image(image_key, b"new image", thumbnail_key, b"new thumbnail");

        assert!(matches!(error, Err(StorageError::AlreadyExists(_)),));

        assert!(!root.path().join(image_key).exists());

        // 原有缩略图不能被覆盖。
        assert_eq!(std::fs::read(thumbnail_path).unwrap(), b"existing",);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_storage_directory() {
        use std::os::unix::fs::symlink;

        let root = tempdir().unwrap();
        let outside = tempdir().unwrap();
        symlink(outside.path(), root.path().join("images")).unwrap();

        let error = match Storage::new(root.path()) {
            Ok(_) => panic!("符号链接目录应被拒绝"),
            Err(error) => error,
        };

        assert!(matches!(error, StorageError::InvalidKey(_)));
    }
}
