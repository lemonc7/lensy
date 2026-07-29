use std::{
    io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use uuid::Uuid;

use crate::{
    image::processor::{ImageProcessor, ProcessedImage},
    model::image::{Image, NewImage},
    service::error::ImageServiceError,
    storage::{file_store::FileStore, image_repo::ImageRepository},
};

pub struct UploadImage {
    /// ImageService 接管该临时文件，上传结束后无论成功失败都会尝试删除。
    pub temporary_source_path: PathBuf,
    pub original_name: String,
}

pub enum UploadResult {
    Created(Image),
    AlreadyExists(Image),
}

pub struct ImageService {
    repository: ImageRepository,
    file_store: FileStore,
    processor: ImageProcessor,
    max_upload_size: u64,
}

impl ImageService {
    pub fn new(
        repository: ImageRepository,
        file_store: FileStore,
        processor: ImageProcessor,
        max_upload_size: u64,
    ) -> Self {
        Self {
            repository,
            file_store,
            processor,
            max_upload_size,
        }
    }

    pub async fn upload(&self, input: UploadImage) -> Result<UploadResult, ImageServiceError> {
        let temporary_source_path = input.temporary_source_path.clone();
        let result = self.upload_inner(input).await;

        cleanup_temporary_file(&temporary_source_path).await;

        result
    }

    async fn upload_inner(&self, input: UploadImage) -> Result<UploadResult, ImageServiceError> {
        validate_original_name(&input.original_name)?;

        // 使用磁盘上的真实大小，不信任调用方提供的上传元数据。
        let source_size = tokio::fs::metadata(&input.temporary_source_path)
            .await?
            .len();

        if source_size > self.max_upload_size {
            return Err(ImageServiceError::UploadTooLarge {
                actual: source_size,
                maximum: self.max_upload_size,
            });
        }

        let source_size = to_database_integer(source_size, "source_size")?;

        let created_at = current_unix_timestamp()?;
        let processed = self
            .processor
            .process(
                &input.temporary_source_path,
                self.file_store.processing_root(),
            )
            .await?;

        self.finished_upload(input.original_name, source_size, created_at, processed)
            .await
    }

    async fn finished_upload(
        &self,
        original_name: String,
        source_size: i64,
        created_at: i64,
        processed: ProcessedImage,
    ) -> Result<UploadResult, ImageServiceError> {
        // 根据最终像素查重
        let existing = self
            .repository
            .find_active_by_pixel_hash(&processed.pixel_hash)
            .await;

        match existing {
            Ok(Some(image)) => {
                cleanup_processed(&processed).await;
                return Ok(UploadResult::AlreadyExists(image));
            }
            Ok(None) => {}
            Err(err) => {
                cleanup_processed(&processed).await;
                return Err(err.into());
            }
        }

        let stored_size = match to_database_integer(processed.stored_size, "stored_size") {
            Ok(value) => value,
            Err(err) => {
                cleanup_processed(&processed).await;
                return Err(err);
            }
        };

        let thumbnail_size = match to_database_integer(processed.thumbnail_size, "thumbnail_size") {
            Ok(value) => value,
            Err(err) => {
                cleanup_processed(&processed).await;
                return Err(err);
            }
        };

        let public_id = Uuid::new_v4().simple().to_string();
        let prefix = &public_id[..2];
        let storage_key = format!("images/{prefix}/{public_id}.webp");
        let thumbnail_key = format!("thumbnails/{prefix}/{public_id}.webp");

        // 先移动正式图片
        if let Err(err) = self
            .file_store
            .promote(&processed.webp_path, &storage_key)
            .await
        {
            cleanup_processed(&processed).await;

            return Err(err.into());
        }

        // 再移动缩略图
        if let Err(err) = self
            .file_store
            .promote(&processed.thumbnail_path, &thumbnail_key)
            .await
        {
            // 缩略图失败，撤销已经移动的正式图片
            self.cleanup_stored_file(&storage_key).await;
            cleanup_processed(&processed).await;
            return Err(err.into());
        }

        let new_image = NewImage {
            public_id,
            storage_key,
            thumbnail_key,

            original_name,
            source_mime: processed.source_mime,

            source_size,
            stored_size,
            thumbnail_size,

            width: processed.width as i64,
            height: processed.height as i64,

            thumbnail_width: processed.thumbnail_width as i64,
            thumbnail_height: processed.thumbnail_height as i64,
            source_hash: processed.source_hash,
            content_hash: processed.content_hash,
            pixel_hash: processed.pixel_hash,
            created_at,
        };

        match self.repository.insert(&new_image).await {
            Ok(image) => {
                cleanup_directory(&processed.temporary_directory).await;
                Ok(UploadResult::Created(image))
            }
            Err(insert_err) => {
                // 数据库写入失败，撤销两个正式文件
                self.cleanup_stored_file(&new_image.storage_key).await;
                self.cleanup_stored_file(&new_image.thumbnail_key).await;

                cleanup_directory(&processed.temporary_directory).await;

                // 并发上传相同图片时，两个请求可能同时通过第一次查重
                // 唯一索引只会允许一个插入成功
                if is_unique_violation(&insert_err)
                    && let Ok(Some(existing)) = self
                        .repository
                        .find_active_by_pixel_hash(&new_image.pixel_hash)
                        .await
                {
                    return Ok(UploadResult::AlreadyExists(existing));
                }
                Err(insert_err.into())
            }
        }
    }

    async fn cleanup_stored_file(&self, storage_key: &str) {
        if let Err(err) = self.file_store.remove(storage_key).await {
            tracing::warn!(
                storage_key,
                error = %err,
                "failed to clean stored file during rollback"
            );
        }
    }
}

async fn cleanup_processed(processed: &ProcessedImage) {
    cleanup_directory(&processed.temporary_directory).await;
}

async fn cleanup_directory(path: &Path) {
    match tokio::fs::remove_dir_all(path).await {
        Ok(()) => {}
        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
        // 清理失败不覆盖原来的业务结果，但必须记录，方便后续清理孤立文件。
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                error = %err,
                "failed to clean temporary directory"
            );
        }
    }
}

async fn cleanup_temporary_file(path: &Path) {
    match tokio::fs::remove_file(path).await {
        Ok(()) => {}
        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                error = %err,
                "failed to clean uploaded temporary file"
            );
        }
    }
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .is_some_and(|database_error| database_error.is_unique_violation())
}

fn validate_original_name(original_name: &str) -> Result<(), ImageServiceError> {
    if original_name.trim().is_empty() {
        return Err(ImageServiceError::EmptyOriginalName);
    }

    if original_name.chars().count() > 255 {
        return Err(ImageServiceError::OriginalNameTooLong);
    }

    Ok(())
}

fn to_database_integer(value: u64, field: &'static str) -> Result<i64, ImageServiceError> {
    i64::try_from(value).map_err(|_| ImageServiceError::IntegerOutOfRange(field))
}

fn current_unix_timestamp() -> Result<i64, ImageServiceError> {
    let seconds = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    to_database_integer(seconds, "created_at")
}
