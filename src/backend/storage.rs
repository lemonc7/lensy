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

        fs::create_dir_all(root.join("images"))?;
        fs::create_dir_all(root.join("thumbnails"))?;

        let temp_root = root.join("tmp");
        fs::create_dir_all(&temp_root)?;

        // canonicalize放在目录创建之后
        let root = fs::canonicalize(root)?;
        let temp_root = root.join("tmp");
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

        // 提前检查
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
            match fs::remove_file(&image_path) {
                Ok(()) => return Err(save_error),
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Err(save_error),
                Err(rollback_error) => {
                    return Err(StorageError::RollbackFailed {
                        save_error: Box::new(save_error),
                        rollback_error,
                    });
                }
            }
        }

        Ok(())
    }

    pub fn open(&self, key: &str) -> Result<File, StorageError> {
        let path = self.resolve(key)?;
        Ok(File::open(path)?)
    }

    pub fn remove_image(&self, image_key: &str, thumbnail_key: &str) -> Result<(), StorageError> {
        let image_path = self.resolve(image_key)?;
        let thumbnail_path = self.resolve(thumbnail_key)?;

        let mut first_error = None;

        for path in [&image_path, &thumbnail_path] {
            match fs::remove_file(path) {
                Ok(()) => {}
                // 文件不存在视为成功，保证删除操作幂等
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => {
                    if first_error.is_none() {
                        first_error = Some(error)
                    }
                }
            }
        }

        match first_error {
            Some(error) => Err(StorageError::Io(error)),
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
}
