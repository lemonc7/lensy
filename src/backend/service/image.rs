use std::sync::Arc;

use chrono::{Datelike, Utc};

use crate::backend::{
    error::{ImageWriteError, ServiceError, StorageError},
    image::processor::ProcessedImage,
    model::{
        FileDeletionRecoveryFailure, FileDeletionRecoveryReport, ImageCursor, ImageFileKind,
        ImagePage, NewImage, OpenedImage, PendingUploadRecoveryFailure,
        PendingUploadRecoveryReport, PublicId, StoredImage, UploadImageResult,
    },
    service::Service,
};

const MAX_PUBLIC_ID_ATTEMPTS: usize = 5;
const DEFAULT_PAGE_SIZE: u32 = 30;
const MAX_PAGE_SIZE: u32 = 100;

impl Service {
    pub async fn get_image(&self, public_id: &PublicId) -> Result<StoredImage, ServiceError> {
        self.repository
            .find_active_image_by_public_id(public_id)
            .await?
            .ok_or(ServiceError::ImageNotFound)
    }
    pub async fn get_deleted_image(
        &self,
        public_id: &PublicId,
    ) -> Result<StoredImage, ServiceError> {
        self.repository
            .find_deleted_image_by_public_id(public_id)
            .await?
            .ok_or(ServiceError::ImageNotFound)
    }
    pub async fn list_images(
        &self,
        cursor: Option<ImageCursor>,
        page_size: Option<u32>,
    ) -> Result<ImagePage, ServiceError> {
        let page_size = page_size
            .unwrap_or(DEFAULT_PAGE_SIZE)
            .clamp(1, MAX_PAGE_SIZE);

        // 多查询一条，用来判断是否存在下一页
        let fetch_limit = i64::from(page_size) + 1;
        let mut images = self
            .repository
            .list_active_images(cursor, fetch_limit)
            .await?;
        let has_more = images.len() > page_size as usize;
        images.truncate(page_size as usize);

        let next_cursor = if has_more {
            images.last().map(|image| ImageCursor {
                timestamp: image.created_at,
                id: image.id,
            })
        } else {
            None
        };

        Ok(ImagePage {
            images,
            next_cursor,
        })
    }

    pub async fn list_deleted_images(
        &self,
        cursor: Option<ImageCursor>,
        page_size: Option<u32>,
    ) -> Result<ImagePage, ServiceError> {
        let page_size = page_size
            .unwrap_or(DEFAULT_PAGE_SIZE)
            .clamp(1, MAX_PAGE_SIZE);

        let fetch_limit = i64::from(page_size) + 1;
        let mut images = self
            .repository
            .list_deleted_images(cursor, fetch_limit)
            .await?;

        let has_more = images.len() > page_size as usize;
        images.truncate(page_size as usize);

        let next_cursor = if has_more {
            images.last().map(|image| {
                // repository已经限定deleted_at非空
                let deleted_at = image.deleted_at.expect("回收站中的照片deleted_at一定非空");
                ImageCursor {
                    timestamp: deleted_at,
                    id: image.id,
                }
            })
        } else {
            None
        };
        Ok(ImagePage {
            images,
            next_cursor,
        })
    }

    pub async fn open_image(
        &self,
        public_id: &PublicId,
        kind: ImageFileKind,
    ) -> Result<OpenedImage, ServiceError> {
        // 只允许读取未删除图片
        let image = self
            .repository
            .find_active_image_by_public_id(public_id)
            .await?
            .ok_or(ServiceError::ImageNotFound)?;

        self.open_stored_image(image, kind).await
    }

    pub async fn open_deleted_image(
        &self,
        public_id: &PublicId,
        kind: ImageFileKind,
    ) -> Result<OpenedImage, ServiceError> {
        let image = self
            .repository
            .find_deleted_image_by_public_id(public_id)
            .await?
            .ok_or(ServiceError::ImageNotFound)?;

        self.open_stored_image(image, kind).await
    }

    pub async fn upload_image(
        &self,
        original_name: &str,
        source: Vec<u8>,
    ) -> Result<UploadImageResult, ServiceError> {
        if original_name.trim().is_empty() || original_name.chars().count() > 255 {
            return Err(ServiceError::InvalidOriginalName);
        }

        // 先处理图片
        let processed = self.process_image(source).await?;

        // 第一轮查重，避免正常情况下创建文件
        if let Some(existing) = self
            .repository
            .find_active_image_by_pixel_hash(&processed.pixel_hash)
            .await?
        {
            return Ok(UploadImageResult {
                image: existing,
                already_exists: true,
            });
        }

        // 所有可能失败的纯计算都要在预留pending和写文件之前完成。
        // 否则这里一旦返回错误，会留下已经写入的文件和pending记录。
        let stored_size =
            i64::try_from(processed.webp.len()).map_err(|_| ServiceError::FileSizeOverflow)?;
        let thumbnail_size = i64::try_from(processed.thumbnail_webp.len())
            .map_err(|_| ServiceError::FileSizeOverflow)?;

        // 在写文件之前预留public_id
        let upload_time = self.current_time();
        let reserved = self.reserve_upload_id(upload_time).await?;

        // public_id已经写入到pending_uploads, 如果进程崩溃，启动清理可以发现这条未完成的任务
        let storage = Arc::clone(&self.storage);
        let storage_key = reserved.storage_key.clone();
        let thumbnail_key = reserved.thumbnail_key.clone();
        let image_data = processed.webp;
        let thumbnail_data = processed.thumbnail_webp;
        let save_result = tokio::task::spawn_blocking(move || {
            storage.save_image(&storage_key, &image_data, &thumbnail_key, &thumbnail_data)
        })
        .await?;

        if let Err(error) = save_result {
            return self
                .handle_storage_failure(&reserved.public_id, error)
                .await;
        }

        let new_image = NewImage {
            public_id: &reserved.public_id,
            storage_key: &reserved.storage_key,
            thumbnail_key: &reserved.thumbnail_key,
            original_name,
            stored_size,
            thumbnail_size,
            width: i64::from(processed.width),
            height: i64::from(processed.height),
            thumbnail_width: i64::from(processed.thumbnail_width),
            thumbnail_height: i64::from(processed.thumbnail_height),
            content_hash: &processed.content_hash,
            pixel_hash: &processed.pixel_hash,
            created_at: upload_time.timestamp,
        };

        match self.repository.create_image(new_image).await {
            // 插入成功后，数据库trigger会自动删除pending_uploads中对应的记录
            Ok(image) => Ok(UploadImageResult {
                image,
                already_exists: false,
            }),
            Err(ImageWriteError::ActivePixelConflict) => {
                // 两个相同图片可能同时通过前面的查重，数据库部分唯一索引是最终保护
                self.rollback_uploaded_files(
                    &reserved.public_id,
                    &reserved.storage_key,
                    &reserved.thumbnail_key,
                )
                .await?;

                let existing = self
                    .repository
                    .find_active_image_by_pixel_hash(&processed.pixel_hash)
                    .await?
                    .ok_or(ServiceError::MissingConflictingImage)?;

                Ok(UploadImageResult {
                    image: existing,
                    already_exists: true,
                })
            }
            Err(ImageWriteError::Database(error)) => {
                self.rollback_uploaded_files(
                    &reserved.public_id,
                    &reserved.storage_key,
                    &reserved.thumbnail_key,
                )
                .await?;
                Err(ServiceError::Database(error))
            }
        }
    }

    // 清理残留文件
    pub async fn recover_pending_uploads(
        &self,
    ) -> Result<PendingUploadRecoveryReport, ServiceError> {
        let pending_uploads = self.repository.list_pending_uploads().await?;
        let mut report = PendingUploadRecoveryReport::default();

        for pending in pending_uploads {
            if let Err(error) = self
                .remove_image_files(&pending.storage_key, &pending.thumbnail_key)
                .await
            {
                // 文件清理失败时必须保留pending，这样下次启动还能继续重试
                report.failures.push(PendingUploadRecoveryFailure {
                    public_id: pending.public_id,
                    error: error.to_string(),
                });
                continue;
            }

            if let Err(error) = self
                .repository
                .remove_pending_upload(&pending.public_id)
                .await
            {
                // 文件已经删除，但pending删除失败，继续保留记录是安全的，因为文件删除幂等
                report.failures.push(PendingUploadRecoveryFailure {
                    public_id: pending.public_id,
                    error: error.to_string(),
                });

                continue;
            }
            report.cleaned += 1;
        }

        Ok(report)
    }

    pub async fn soft_delete_image(&self, public_id: &PublicId) -> Result<(), ServiceError> {
        let deleted_at = self.current_time().timestamp;

        let deleted = self
            .repository
            .soft_delete_image(public_id, deleted_at)
            .await?;

        if !deleted {
            return Err(ServiceError::ImageNotFound);
        }
        Ok(())
    }

    pub async fn restore_image(&self, public_id: &PublicId) -> Result<(), ServiceError> {
        // 先获取回收站图片，获取pixel_hash
        let deleted = self
            .repository
            .find_deleted_image_by_public_id(public_id)
            .await?
            .ok_or(ServiceError::ImageNotFound)?;

        // 正常情况下提前检查，返回明确的冲突图片id
        if let Some(existing) = self
            .repository
            .find_active_image_by_pixel_hash(&deleted.pixel_hash)
            .await?
        {
            return Err(ServiceError::RestoreConflict(existing.public_id));
        }

        let updated_at = self.current_time().timestamp;
        match self.repository.restore_image(public_id, updated_at).await {
            Ok(true) => Ok(()),
            // 图片可能被另一个请求抢先修复
            Ok(false) => Err(ServiceError::ImageNotFound),
            // 前面的检查和update之间可能并发上传了相同像素图片，因此数据库仍要负责最终保护
            Err(ImageWriteError::ActivePixelConflict) => {
                let existing = self
                    .repository
                    .find_active_image_by_pixel_hash(&deleted.pixel_hash)
                    .await?
                    .ok_or(ServiceError::MissingConflictingImage)?;
                Err(ServiceError::RestoreConflict(existing.public_id))
            }
            Err(ImageWriteError::Database(error)) => Err(ServiceError::Database(error)),
        }
    }

    pub async fn delete_image(&self, public_id: &PublicId) -> Result<(), ServiceError> {
        // 先删除数据库记录，触发器会同步创建pending_file_deletion
        let image = self
            .repository
            .delete_image(public_id)
            .await?
            .ok_or(ServiceError::ImageNotFound)?;

        // 数据库事务已经完成，现在可以清理文件，失败时直接返回，pending记录会保留
        self.remove_image_files(&image.storage_key, &image.thumbnail_key)
            .await?;

        // 文件已经成功删除，移除清理任务
        self.repository
            .remove_pending_file_deletion(&image.storage_key)
            .await?;

        Ok(())
    }

    pub async fn recover_pending_file_deletions(
        &self,
    ) -> Result<FileDeletionRecoveryReport, ServiceError> {
        let pending_deletions = self.repository.list_pending_file_deletions().await?;

        let mut report = FileDeletionRecoveryReport::default();

        for pending in pending_deletions {
            if let Err(error) = self
                .remove_image_files(&pending.storage_key, &pending.thumbnail_key)
                .await
            {
                // 文件删除失败时保留pending，下载启动可以继续尝试
                report.failures.push(FileDeletionRecoveryFailure {
                    storage_key: pending.storage_key,
                    error: error.to_string(),
                });
                continue;
            }

            if let Err(error) = self
                .repository
                .remove_pending_file_deletion(&pending.storage_key)
                .await
            {
                // 文件已经删除但是任务删除失败，保留任务是安全的，因为文件删除幂等
                report.failures.push(FileDeletionRecoveryFailure {
                    storage_key: pending.storage_key,
                    error: error.to_string(),
                });
                continue;
            }
            report.cleaned += 1;
        }
        Ok(report)
    }

    async fn process_image(&self, source: Vec<u8>) -> Result<ProcessedImage, ServiceError> {
        // 等待许可证时，只挂起当前异步任务，不会阻塞线程
        let permit = self
            .processing_limit
            .clone()
            .acquire_owned()
            .await
            .expect("Service 持有 Semaphore，不会被主动关闭");

        let processor = Arc::clone(&self.processor);

        let processed = tokio::task::spawn_blocking(move || {
            // permit在图片处理结束或发生panic后自动释放
            let _permit = permit;
            processor.process(&source)
        })
        .await??;

        Ok(processed)
    }

    async fn remove_image_files(
        &self,
        storage_key: &str,
        thumbnail_key: &str,
    ) -> Result<(), ServiceError> {
        let storage = Arc::clone(&self.storage);
        let storage_key = storage_key.to_owned();
        let thumbnail_key = thumbnail_key.to_owned();

        tokio::task::spawn_blocking(move || storage.remove_image(&storage_key, &thumbnail_key))
            .await??;
        Ok(())
    }

    async fn reserve_upload_id(
        &self,
        upload_time: UploadTime,
    ) -> Result<ReservedUpload, ServiceError> {
        for _ in 0..MAX_PUBLIC_ID_ATTEMPTS {
            let public_id = PublicId::generate()?;
            let (storage_key, thumbnail_key) = build_storage_keys(&public_id, upload_time);

            if self
                .repository
                .reserve_pending_upload(
                    &public_id,
                    &storage_key,
                    &thumbnail_key,
                    upload_time.timestamp,
                )
                .await?
            {
                return Ok(ReservedUpload {
                    public_id,
                    storage_key,
                    thumbnail_key,
                });
            }
        }

        Err(ServiceError::PublicIdExhausted)
    }

    async fn handle_storage_failure<T>(
        &self,
        public_id: &PublicId,
        error: StorageError,
    ) -> Result<T, ServiceError> {
        // rollback failed表示原图可能仍在磁盘上，此时不能删除pending_uploads，否则会丢失清理依据
        if matches!(error, StorageError::RollbackFailed { .. }) {
            return Err(ServiceError::Storage(error));
        }

        // 上传没有残留文件，删除pending记录
        self.repository.remove_pending_upload(public_id).await?;
        // 返回原错误
        Err(ServiceError::Storage(error))
    }

    // 原图和缩略图保存成功，但是数据库images插入失败，回滚删除对应文件
    async fn rollback_uploaded_files(
        &self,
        public_id: &PublicId,
        storage_key: &str,
        thumbnail_key: &str,
    ) -> Result<(), ServiceError> {
        // 文件删除失败时保留pending_uploads，后续启动清理仍然可以重试
        self.remove_image_files(storage_key, thumbnail_key).await?;

        // 文件已经清理完成，可以删除pending标记
        self.repository.remove_pending_upload(public_id).await?;
        Ok(())
    }

    fn current_time(&self) -> UploadTime {
        // 只获取一次当前时间
        let utc_now = Utc::now();

        // Unix时间戳不受时区影响
        let timestamp = utc_now.timestamp();

        // 年月按照配置的时区计算
        let local_now = utc_now.with_timezone(&self.timezone);

        UploadTime {
            timestamp,
            year: local_now.year(),
            month: local_now.month(),
        }
    }

    async fn open_stored_image(
        &self,
        image: StoredImage,
        kind: ImageFileKind,
    ) -> Result<OpenedImage, ServiceError> {
        let (key, content_length) = match kind {
            ImageFileKind::Original => (image.storage_key.clone(), image.stored_size),
            ImageFileKind::Thumbnail => (image.thumbnail_key.clone(), image.thumbnail_size),
        };

        let storage = Arc::clone(&self.storage);

        let file = tokio::task::spawn_blocking(move || storage.open(&key)).await??;

        Ok(OpenedImage {
            file,
            content_type: "image/webp",
            content_length,
            original_name: image.original_name,
        })
    }
}

struct ReservedUpload {
    public_id: PublicId,
    storage_key: String,
    thumbnail_key: String,
}

#[derive(Debug, Clone, Copy)]
struct UploadTime {
    timestamp: i64,
    year: i32,
    month: u32,
}

fn build_storage_keys(public_id: &PublicId, upload_time: UploadTime) -> (String, String) {
    let year = upload_time.year;
    let month = upload_time.month;
    (
        format!("images/{year:04}/{month:02}/{public_id}.webp"),
        format!("thumbnails/{year:04}/{month:02}/{public_id}.webp"),
    )
}

#[cfg(test)]
mod tests {
    use std::{error::Error, io::Read};

    use chrono_tz::Asia::Shanghai;
    use image::{ColorType, ImageEncoder, codecs::png::PngEncoder};
    use sqlx::SqlitePool;
    use tempfile::tempdir;

    use crate::backend::{
        config::ImageConfig,
        db::Repository,
        error::ServiceError,
        image::processor::ImageProcessor,
        model::{ImageFileKind, PublicId},
        service::Service,
        storage::Storage,
    };

    #[sqlx::test]
    async fn uploads_and_deduplicates_image(pool: SqlitePool) -> Result<(), Box<dyn Error>> {
        let data_dir = tempdir()?;

        let repository = Repository::new(pool.clone());

        let processor = ImageProcessor::new(test_image_config());

        let storage = Storage::new(data_dir.path())?;

        let service = Service::new(repository, processor, storage, Shanghai);

        let source = create_png(4, 2);

        let first = service.upload_image("example.png", source.clone()).await?;

        assert!(!first.already_exists);
        assert_eq!(first.image.original_name, "example.png");
        assert_eq!(first.image.width, 4);
        assert_eq!(first.image.height, 2);

        assert!(data_dir.path().join(&first.image.storage_key).is_file());

        assert!(data_dir.path().join(&first.image.thumbnail_key).is_file());

        assert!(first.image.storage_key.starts_with("images/"));

        assert!(first.image.thumbnail_key.starts_with("thumbnails/"));

        let pending_count = sqlx::query_scalar!("SELECT COUNT(*) FROM pending_uploads")
            .fetch_one(&pool)
            .await?;

        assert_eq!(pending_count, 0);

        let first_id = first.image.id;
        let first_public_id = first.image.public_id.clone();

        let second = service.upload_image("duplicate.png", source).await?;

        assert!(second.already_exists);
        assert_eq!(second.image.id, first_id);
        assert_eq!(second.image.public_id, first_public_id,);

        let image_count = sqlx::query_scalar!("SELECT COUNT(*) FROM images")
            .fetch_one(&pool)
            .await?;

        assert_eq!(image_count, 1);

        Ok(())
    }

    #[sqlx::test]
    async fn lists_active_images_with_stable_cursor_pagination(
        pool: SqlitePool,
    ) -> Result<(), Box<dyn Error>> {
        let data_dir = tempdir()?;
        let service = test_service(pool.clone(), data_dir.path())?;

        let first = service.upload_image("first.png", create_png(4, 2)).await?;
        let second = service.upload_image("second.png", create_png(5, 2)).await?;
        let third = service.upload_image("third.png", create_png(6, 2)).await?;

        set_created_at(&pool, first.image.id, 10).await?;
        set_created_at(&pool, second.image.id, 20).await?;
        set_created_at(&pool, third.image.id, 20).await?;

        let first_page = service.list_images(None, Some(2)).await?;

        assert_eq!(first_page.images.len(), 2);
        assert_eq!(first_page.images[0].public_id, third.image.public_id);
        assert_eq!(first_page.images[1].public_id, second.image.public_id);

        let cursor = first_page.next_cursor.expect("应当存在下一页游标");
        assert_eq!(cursor.timestamp, 20);
        assert_eq!(cursor.id, second.image.id);

        let second_page = service.list_images(Some(cursor), Some(2)).await?;

        assert_eq!(second_page.images.len(), 1);
        assert_eq!(second_page.images[0].public_id, first.image.public_id);
        assert!(second_page.next_cursor.is_none());

        Ok(())
    }

    #[sqlx::test]
    async fn lists_only_deleted_images_with_stable_cursor_pagination(
        pool: SqlitePool,
    ) -> Result<(), Box<dyn Error>> {
        let data_dir = tempdir()?;
        let service = test_service(pool.clone(), data_dir.path())?;

        let first = service.upload_image("first.png", create_png(4, 2)).await?;
        let second = service.upload_image("second.png", create_png(5, 2)).await?;
        let third = service.upload_image("third.png", create_png(6, 2)).await?;
        let active = service.upload_image("active.png", create_png(7, 2)).await?;

        service.soft_delete_image(&first.image.public_id).await?;
        service.soft_delete_image(&second.image.public_id).await?;
        service.soft_delete_image(&third.image.public_id).await?;

        set_deleted_at(&pool, first.image.id, 10).await?;
        set_deleted_at(&pool, second.image.id, 20).await?;
        set_deleted_at(&pool, third.image.id, 20).await?;

        let first_page = service.list_deleted_images(None, Some(2)).await?;

        assert_eq!(first_page.images.len(), 2);
        assert_eq!(first_page.images[0].public_id, third.image.public_id);
        assert_eq!(first_page.images[1].public_id, second.image.public_id);
        assert!(
            first_page
                .images
                .iter()
                .all(|image| image.deleted_at.is_some())
        );
        assert!(
            first_page
                .images
                .iter()
                .all(|image| image.public_id != active.image.public_id)
        );

        let cursor = first_page.next_cursor.expect("应当存在下一页游标");
        assert_eq!(cursor.timestamp, 20);
        assert_eq!(cursor.id, second.image.id);

        let second_page = service.list_deleted_images(Some(cursor), Some(2)).await?;

        assert_eq!(second_page.images.len(), 1);
        assert_eq!(second_page.images[0].public_id, first.image.public_id);
        assert!(second_page.next_cursor.is_none());

        Ok(())
    }

    #[sqlx::test]
    async fn opens_active_and_deleted_image_files_according_to_state(
        pool: SqlitePool,
    ) -> Result<(), Box<dyn Error>> {
        let data_dir = tempdir()?;
        let service = test_service(pool, data_dir.path())?;
        let uploaded = service
            .upload_image("example.png", create_png(4, 2))
            .await?;

        let public_id = uploaded.image.public_id.clone();
        let expected_original = std::fs::read(data_dir.path().join(&uploaded.image.storage_key))?;
        let expected_thumbnail =
            std::fs::read(data_dir.path().join(&uploaded.image.thumbnail_key))?;

        let mut original = service
            .open_image(&public_id, ImageFileKind::Original)
            .await?;
        let mut original_bytes = Vec::new();
        original.file.read_to_end(&mut original_bytes)?;

        assert_eq!(original.content_type, "image/webp");
        assert_eq!(original.content_length, uploaded.image.stored_size);
        assert_eq!(original.original_name, "example.png");
        assert_eq!(original_bytes, expected_original);

        let mut thumbnail = service
            .open_image(&public_id, ImageFileKind::Thumbnail)
            .await?;
        let mut thumbnail_bytes = Vec::new();
        thumbnail.file.read_to_end(&mut thumbnail_bytes)?;

        assert_eq!(thumbnail.content_length, uploaded.image.thumbnail_size);
        assert_eq!(thumbnail_bytes, expected_thumbnail);

        service.soft_delete_image(&public_id).await?;

        assert!(matches!(
            service
                .open_image(&public_id, ImageFileKind::Original)
                .await,
            Err(ServiceError::ImageNotFound),
        ));

        let mut deleted_thumbnail = service
            .open_deleted_image(&public_id, ImageFileKind::Thumbnail)
            .await?;
        let mut deleted_thumbnail_bytes = Vec::new();
        deleted_thumbnail
            .file
            .read_to_end(&mut deleted_thumbnail_bytes)?;

        assert_eq!(deleted_thumbnail_bytes, expected_thumbnail);

        Ok(())
    }

    #[sqlx::test]
    async fn recovers_pending_upload_files(pool: SqlitePool) -> Result<(), Box<dyn Error>> {
        let data_dir = tempdir()?;
        let service = test_service(pool.clone(), data_dir.path())?;
        let public_id = PublicId::parse("A8kLm2Pq7XzB")?;
        let storage_key = "images/2026/08/A8kLm2Pq7XzB.webp";
        let thumbnail_key = "thumbnails/2026/08/A8kLm2Pq7XzB.webp";

        assert!(
            service
                .repository
                .reserve_pending_upload(&public_id, storage_key, thumbnail_key, 1)
                .await?
        );

        service
            .storage
            .save_image(storage_key, b"image", thumbnail_key, b"thumbnail")?;

        let report = service.recover_pending_uploads().await?;

        assert_eq!(report.cleaned, 1);
        assert!(report.failures.is_empty());
        assert!(!data_dir.path().join(storage_key).exists());
        assert!(!data_dir.path().join(thumbnail_key).exists());

        let pending_count = sqlx::query_scalar!("SELECT COUNT(*) FROM pending_uploads")
            .fetch_one(&pool)
            .await?;

        assert_eq!(pending_count, 0);

        Ok(())
    }

    #[sqlx::test]
    async fn keeps_pending_upload_when_file_cleanup_fails(
        pool: SqlitePool,
    ) -> Result<(), Box<dyn Error>> {
        let data_dir = tempdir()?;
        let service = test_service(pool.clone(), data_dir.path())?;
        let public_id = PublicId::parse("B8kLm2Pq7XzB")?;

        assert!(
            service
                .repository
                .reserve_pending_upload(
                    &public_id,
                    "images/../escape.webp",
                    "thumbnails/../escape.webp",
                    1,
                )
                .await?
        );

        let report = service.recover_pending_uploads().await?;

        assert_eq!(report.cleaned, 0);
        assert_eq!(report.failures.len(), 1);
        assert_eq!(report.failures[0].public_id, public_id);

        let pending_count = sqlx::query_scalar!("SELECT COUNT(*) FROM pending_uploads")
            .fetch_one(&pool)
            .await?;

        assert_eq!(pending_count, 1);

        Ok(())
    }

    #[sqlx::test]
    async fn soft_deletes_image_without_removing_files(
        pool: SqlitePool,
    ) -> Result<(), Box<dyn Error>> {
        let data_dir = tempdir()?;
        let service = test_service(pool.clone(), data_dir.path())?;

        let source = create_png(4, 2);

        let uploaded = service.upload_image("example.png", source).await?;

        let public_id = uploaded.image.public_id.clone();
        let storage_key = uploaded.image.storage_key.clone();
        let thumbnail_key = uploaded.image.thumbnail_key.clone();

        service.soft_delete_image(&public_id).await?;

        // 有效图片查询不到。
        let active = service
            .repository
            .find_active_image_by_public_id(&public_id)
            .await?;

        assert!(active.is_none());

        // 回收站中可以查询。
        let deleted = service
            .repository
            .find_deleted_image_by_public_id(&public_id)
            .await?;

        assert!(deleted.is_some());
        assert!(deleted.unwrap().deleted_at.is_some());

        // 软删除不能删除磁盘文件。
        assert!(data_dir.path().join(storage_key).is_file());

        assert!(data_dir.path().join(thumbnail_key).is_file());

        // 再次软删除返回不存在。
        let second_delete = service.soft_delete_image(&public_id).await;

        assert!(matches!(second_delete, Err(ServiceError::ImageNotFound),));

        Ok(())
    }

    #[sqlx::test]
    async fn restores_deleted_image_without_changing_files(
        pool: SqlitePool,
    ) -> Result<(), Box<dyn Error>> {
        let data_dir = tempdir()?;
        let service = test_service(pool.clone(), data_dir.path())?;
        let source = create_png(4, 2);
        let uploaded = service.upload_image("example.png", source).await?;
        let public_id = uploaded.image.public_id.clone();
        let storage_key = uploaded.image.storage_key.clone();
        let thumbnail_key = uploaded.image.thumbnail_key.clone();

        service.soft_delete_image(&public_id).await?;
        service.restore_image(&public_id).await?;

        let restored = service.get_image(&public_id).await?;

        assert!(restored.deleted_at.is_none());
        assert!(data_dir.path().join(storage_key).is_file());
        assert!(data_dir.path().join(thumbnail_key).is_file());
        assert!(matches!(
            service.get_deleted_image(&public_id).await,
            Err(ServiceError::ImageNotFound),
        ));

        let second_restore = service.restore_image(&public_id).await;

        assert!(matches!(second_restore, Err(ServiceError::ImageNotFound),));

        Ok(())
    }

    #[sqlx::test]
    async fn refuses_to_restore_when_active_pixel_already_exists(
        pool: SqlitePool,
    ) -> Result<(), Box<dyn Error>> {
        let data_dir = tempdir()?;
        let service = test_service(pool.clone(), data_dir.path())?;
        let source = create_png(4, 2);

        let deleted = service.upload_image("first.png", source.clone()).await?;
        let deleted_public_id = deleted.image.public_id.clone();

        service.soft_delete_image(&deleted_public_id).await?;

        let active = service.upload_image("second.png", source).await?;

        assert!(!active.already_exists);
        assert_ne!(active.image.public_id, deleted_public_id);

        let restore_error = service.restore_image(&deleted_public_id).await.unwrap_err();

        assert!(matches!(
            restore_error,
            ServiceError::RestoreConflict (
                existing_public_id
            ) if existing_public_id
                == active.image.public_id
        ));
        assert!(service.get_deleted_image(&deleted_public_id).await.is_ok());
        assert!(service.get_image(&active.image.public_id).await.is_ok());

        let active_count = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM images WHERE pixel_hash = ?1 AND deleted_at IS NULL",
            active.image.pixel_hash,
        )
        .fetch_one(&pool)
        .await?;

        assert_eq!(active_count, 1);

        Ok(())
    }

    #[sqlx::test]
    async fn permanently_deletes_trashed_image(pool: SqlitePool) -> Result<(), Box<dyn Error>> {
        let data_dir = tempdir()?;
        let service = test_service(pool.clone(), data_dir.path())?;

        let source = create_png(4, 2);

        let uploaded = service.upload_image("example.png", source).await?;

        let public_id = uploaded.image.public_id.clone();
        let storage_key = uploaded.image.storage_key.clone();
        let thumbnail_key = uploaded.image.thumbnail_key.clone();

        service.soft_delete_image(&public_id).await?;

        service.delete_image(&public_id).await?;

        assert!(
            service
                .repository
                .find_deleted_image_by_public_id(&public_id,)
                .await?
                .is_none()
        );

        assert!(!data_dir.path().join(&storage_key).exists());

        assert!(!data_dir.path().join(&thumbnail_key).exists());

        let pending_count = sqlx::query_scalar!("SELECT COUNT(*) FROM pending_file_deletions")
            .fetch_one(&pool)
            .await?;

        assert_eq!(pending_count, 0);

        Ok(())
    }

    #[sqlx::test]
    async fn refuses_to_permanently_delete_active_image(
        pool: SqlitePool,
    ) -> Result<(), Box<dyn Error>> {
        let data_dir = tempdir()?;
        let service = test_service(pool, data_dir.path())?;

        let source = create_png(4, 2);

        let uploaded = service.upload_image("example.png", source).await?;

        let result = service.delete_image(&uploaded.image.public_id).await;

        assert!(matches!(result, Err(ServiceError::ImageNotFound),));

        assert!(data_dir.path().join(&uploaded.image.storage_key).is_file());

        assert!(service.get_image(&uploaded.image.public_id).await.is_ok());

        Ok(())
    }

    #[sqlx::test]
    async fn recovers_pending_file_deletion(pool: SqlitePool) -> Result<(), Box<dyn Error>> {
        let data_dir = tempdir()?;
        let service = test_service(pool.clone(), data_dir.path())?;

        let source = create_png(4, 2);

        let uploaded = service.upload_image("example.png", source).await?;

        let public_id = uploaded.image.public_id.clone();
        let storage_key = uploaded.image.storage_key.clone();
        let thumbnail_key = uploaded.image.thumbnail_key.clone();

        service.soft_delete_image(&public_id).await?;

        // 只执行数据库删除，模拟删除后立即崩溃，
        // 此时文件还存在，pending 已由触发器创建。
        let deleted = service.repository.delete_image(&public_id).await?;

        assert!(deleted.is_some());
        assert!(data_dir.path().join(&storage_key).is_file());

        let report = service.recover_pending_file_deletions().await?;

        assert_eq!(report.cleaned, 1);
        assert!(report.failures.is_empty());
        assert!(!data_dir.path().join(&storage_key).exists());
        assert!(!data_dir.path().join(&thumbnail_key).exists());

        let pending_count = sqlx::query_scalar!("SELECT COUNT(*) FROM pending_file_deletions")
            .fetch_one(&pool)
            .await?;

        assert_eq!(pending_count, 0);

        Ok(())
    }

    async fn set_created_at(
        pool: &SqlitePool,
        image_id: i64,
        created_at: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE images
            SET created_at = ?1,
                updated_at = ?1
            WHERE id = ?2
            "#,
            created_at,
            image_id,
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    async fn set_deleted_at(
        pool: &SqlitePool,
        image_id: i64,
        deleted_at: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE images
            SET deleted_at = ?1,
                updated_at = ?1
            WHERE id = ?2
            "#,
            deleted_at,
            image_id,
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    fn test_service(
        pool: SqlitePool,
        data_path: &std::path::Path,
    ) -> Result<Service, Box<dyn Error>> {
        Ok(Service::new(
            Repository::new(pool),
            ImageProcessor::new(test_image_config()),
            Storage::new(data_path)?,
            Shanghai,
        ))
    }

    fn test_image_config() -> ImageConfig {
        ImageConfig {
            max_upload_size: 1024 * 1024,
            max_pixels: 1_000_000,
            quality: 82.0,
            thumbnail_quality: 75.0,
            method: 4,
            thumbnail_max_edge: 2,
            max_concurrent_processing: 2,
        }
    }

    fn create_png(width: u32, height: u32) -> Vec<u8> {
        let pixel_count = width as usize * height as usize;

        let mut rgba = Vec::with_capacity(pixel_count * 4);

        for index in 0..pixel_count {
            rgba.extend_from_slice(&[(index % 255) as u8, 80, 40, 255]);
        }

        let mut output = Vec::new();

        PngEncoder::new(&mut output)
            .write_image(&rgba, width, height, ColorType::Rgba8.into())
            .expect("测试 PNG 应编码成功");

        output
    }
}
