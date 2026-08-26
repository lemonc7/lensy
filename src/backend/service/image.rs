use std::sync::Arc;

use chrono::{Datelike, Utc};

use crate::backend::{
    error::{ImageWriteError, ServiceError},
    image::processor::ProcessedImage,
    model::{
        ImageCleanupFailure, ImageCleanupReport, ImageCursor, ImageFileKind, ImagePage, NewImage,
        OpenedImage, PublicId, StoredImage, UploadImageResult,
    },
    service::Service,
};

const MAX_PUBLIC_ID_ATTEMPTS: usize = 5;
const DEFAULT_PAGE_SIZE: u32 = 30;
const MAX_PAGE_SIZE: u32 = 100;
const CLEANUP_BATCH_SIZE: i64 = 100;

impl Service {
    pub async fn get_image(&self, public_id: &PublicId) -> Result<StoredImage, ServiceError> {
        self.repository
            .find_active_image_by_public_id(public_id)
            .await?
            .ok_or(ServiceError::ImageNotFound)
    }

    pub async fn get_trashed_image(
        &self,
        public_id: &PublicId,
    ) -> Result<StoredImage, ServiceError> {
        self.repository
            .find_trashed_image_by_public_id(public_id)
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

    pub async fn list_trashed_images(
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
            .list_trashed_images(cursor, fetch_limit)
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

    pub async fn open_trashed_image(
        &self,
        public_id: &PublicId,
        kind: ImageFileKind,
    ) -> Result<OpenedImage, ServiceError> {
        let image = self
            .repository
            .find_trashed_image_by_public_id(public_id)
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

        let stored_size =
            i64::try_from(processed.webp.len()).map_err(|_| ServiceError::FileSizeOverflow)?;
        let thumbnail_size = i64::try_from(processed.thumbnail_webp.len())
            .map_err(|_| ServiceError::FileSizeOverflow)?;

        // 阻止当前上传被本实例的恢复任务接管
        let _upload_guard = self.upload_recovery_lock.read().await;

        let upload_time = self.current_time();

        // 先插入uploading记录，同时预留public_id和文件路径
        let uploading = self
            .create_uploading_record(
                original_name,
                &processed,
                stored_size,
                thumbnail_size,
                upload_time,
            )
            .await?;

        let storage = Arc::clone(&self.storage);
        let storage_key = uploading.storage_key.clone();
        let thumbnail_key = uploading.thumbnail_key.clone();
        let image_data = processed.webp;
        let thumbnail_data = processed.thumbnail_webp;

        let save_result = tokio::task::spawn_blocking(move || {
            storage.save_image(&storage_key, &image_data, &thumbnail_key, &thumbnail_data)
        })
        .await;

        let save_result = match save_result {
            Ok(result) => result,
            Err(error) => {
                // 阻塞任务异常退出，数据库记录仍然可以转入deleting清理
                let original = ServiceError::BlockingTask(error);
                return Err(self.cleanup_failed_upload(&uploading, original).await);
            }
        };

        if let Err(storage_error) = save_result {
            // save_image可能已经写入部分文件，因此统一走删除状态机
            let original = ServiceError::Storage(storage_error);
            return Err(self.cleanup_failed_upload(&uploading, original).await);
        }

        let updated_at = self.current_time().timestamp;

        match self
            .repository
            .activate_image(uploading.id, updated_at)
            .await
        {
            Ok(Some(image)) => Ok(UploadImageResult {
                image,
                already_exists: false,
            }),
            Ok(None) => {
                // uploading已经被其他清理任务改成deleting，当前任务不能再激活
                Err(ServiceError::UploadInterrupted)
            }
            Err(ImageWriteError::ActivePixelConflict) => {
                // 并发上传了相同图片，数据库唯一索引负责最终查重
                self.discard_upload(&uploading).await?;
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
                // 文件已经保存但激活失败，转为deleting后清理
                let original = ServiceError::Database(error);
                Err(self.cleanup_failed_upload(&uploading, original).await)
            }
        }
    }

    // 清理残留文件
    pub async fn recover_images(&self) -> Result<ImageCleanupReport, ServiceError> {
        // 先读取已经处于deleting的任务
        let mut deleting_images = self
            .repository
            .list_deleting_images(CLEANUP_BATCH_SIZE)
            .await?;

        let now = self.current_time().timestamp;

        // write lock会等待当前实例内正在进行的上传结束
        // 在接管过程中会阻止新上传创建uploading记录
        let claimed_uploads = {
            let _recovery_guard = self.upload_recovery_lock.write().await;

            self.repository
                .claim_stale_uploads_for_deletion(now, now, CLEANUP_BATCH_SIZE)
                .await?
        };

        let claimed_count = claimed_uploads.len();

        // 前面查询的记录原本就是deleting
        // 新接管的记录原本是uploading，因此两组不会重复
        deleting_images.extend(claimed_uploads);

        let mut report = ImageCleanupReport {
            claimed_uploads: claimed_count,
            ..ImageCleanupReport::default()
        };

        for image in deleting_images {
            match self.finish_deleting_image(&image).await {
                Ok(()) => report.cleaned += 1,
                Err(error) => {
                    // 保留deleting记录，下一次继续重试，将任务移至队尾
                    let retry_at = self.current_time().timestamp;
                    let message = match self
                        .repository
                        .defer_image_deletion(image.id, retry_at)
                        .await
                    {
                        Ok(true) => error.to_string(),
                        Ok(false) => format!("{error}；删除任务已经不存在或不再处于 deleting 状态"),
                        Err(defer_error) => {
                            format!("{error}；将删除任务移动到队尾失败：{defer_error}")
                        }
                    };

                    report.failures.push(ImageCleanupFailure {
                        public_id: image.public_id,
                        error: message,
                    });
                }
            }
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
            .find_trashed_image_by_public_id(public_id)
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
            Ok(false) => Err(ServiceError::ImageNotFound),
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
        let updated_at = self.current_time().timestamp;

        // 先执行trashed->deleting，并取得待删除文件的信息
        let image = self
            .repository
            .mark_trashed_for_deletion(public_id, updated_at)
            .await?
            .ok_or(ServiceError::ImageNotFound)?;

        // 删除文件成功后，再删除数据库记录
        self.finish_deleting_image(&image).await
    }

    async fn create_uploading_record(
        &self,
        original_name: &str,
        processed: &ProcessedImage,
        stored_size: i64,
        thumbnail_size: i64,
        upload_time: UploadTime,
    ) -> Result<StoredImage, ServiceError> {
        for _ in 0..MAX_PUBLIC_ID_ATTEMPTS {
            let public_id = PublicId::generate()?;
            let (storage_key, thumbnail_key) = build_storage_keys(&public_id, upload_time);

            let new_image = NewImage {
                public_id: &public_id,
                storage_key: &storage_key,
                thumbnail_key: &thumbnail_key,
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

            if let Some(image) = self.repository.create_uploading_image(new_image).await? {
                return Ok(image);
            }
        }
        Err(ServiceError::PublicIdExhausted)
    }

    async fn discard_upload(&self, image: &StoredImage) -> Result<(), ServiceError> {
        let updated_at = self.current_time().timestamp;
        let claimed = self
            .repository
            .mark_upload_for_deletion(image.id, updated_at)
            .await?;

        if !claimed {
            // 可能已经被恢复任务接管，不能再自行删除文件
            return Err(ServiceError::UploadInterrupted);
        }
        self.finish_deleting_image(image).await
    }

    async fn cleanup_failed_upload(
        &self,
        image: &StoredImage,
        original: ServiceError,
    ) -> ServiceError {
        match self.discard_upload(image).await {
            Ok(()) => original,
            Err(cleanup) => ServiceError::UploadCleanupFailed {
                original: Box::new(original),
                cleanup: Box::new(cleanup),
            },
        }
    }

    async fn finish_deleting_image(&self, image: &StoredImage) -> Result<(), ServiceError> {
        // 先删除文件
        self.remove_image_files(&image.storage_key, &image.thumbnail_key)
            .await?;

        // 文件删除成功后，才能删除数据库记录
        // false通常表示并发清理任务已经完成，可以视为幂等成功
        self.repository.finish_image_deletion(image.id).await?;
        Ok(())
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
    use std::{error::Error, io::Read, path::Path};

    use chrono_tz::Asia::Shanghai;
    use image::{ColorType, ImageEncoder, codecs::png::PngEncoder};
    use sqlx::SqlitePool;
    use tempfile::tempdir;

    use crate::backend::{
        config::ImageConfig,
        db::Repository,
        error::ServiceError,
        image::processor::ImageProcessor,
        model::{ImageFileKind, NewImage, PublicId, Status, StoredImage},
        service::Service,
        storage::Storage,
    };

    #[sqlx::test]
    async fn uploads_opens_and_deduplicates_image(pool: SqlitePool) -> Result<(), Box<dyn Error>> {
        let data_dir = tempdir()?;
        let service = test_service(pool.clone(), data_dir.path())?;
        let source = create_png(4, 2);

        let first = service.upload_image("example.png", source.clone()).await?;

        assert!(!first.already_exists);
        assert_eq!(first.image.status, Status::Active);
        assert_eq!(first.image.original_name, "example.png");
        assert_eq!((first.image.width, first.image.height), (4, 2));
        assert!(data_dir.path().join(&first.image.storage_key).is_file());
        assert!(data_dir.path().join(&first.image.thumbnail_key).is_file());

        let mut opened = service
            .open_image(&first.image.public_id, ImageFileKind::Original)
            .await?;
        let mut stored_bytes = Vec::new();
        opened.file.read_to_end(&mut stored_bytes)?;

        assert_eq!(opened.content_type, "image/webp");
        assert_eq!(opened.content_length, first.image.stored_size);
        assert_eq!(opened.original_name, "example.png");
        assert!(!stored_bytes.is_empty());

        let second = service.upload_image("duplicate.png", source).await?;

        assert!(second.already_exists);
        assert_eq!(second.image.id, first.image.id);
        assert_eq!(second.image.public_id, first.image.public_id);

        let image_count = sqlx::query_scalar!("SELECT COUNT(*) FROM images")
            .fetch_one(&pool)
            .await?;
        assert_eq!(image_count, 1);

        Ok(())
    }

    #[sqlx::test]
    async fn paginates_active_and_trashed_images(pool: SqlitePool) -> Result<(), Box<dyn Error>> {
        let data_dir = tempdir()?;
        let service = test_service(pool.clone(), data_dir.path())?;
        let first = service.upload_image("first.png", create_png(4, 2)).await?;
        let second = service.upload_image("second.png", create_png(5, 2)).await?;
        let third = service.upload_image("third.png", create_png(6, 2)).await?;

        set_created_at(&pool, first.image.id, 10).await?;
        set_created_at(&pool, second.image.id, 20).await?;
        set_created_at(&pool, third.image.id, 20).await?;

        let first_page = service.list_images(None, Some(2)).await?;
        assert_eq!(
            image_ids(&first_page.images),
            vec![third.image.id, second.image.id]
        );

        let second_page = service.list_images(first_page.next_cursor, Some(2)).await?;
        assert_eq!(image_ids(&second_page.images), vec![first.image.id]);
        assert!(second_page.next_cursor.is_none());

        service.soft_delete_image(&first.image.public_id).await?;
        service.soft_delete_image(&second.image.public_id).await?;
        service.soft_delete_image(&third.image.public_id).await?;

        set_deleted_at(&pool, first.image.id, 100).await?;
        set_deleted_at(&pool, second.image.id, 200).await?;
        set_deleted_at(&pool, third.image.id, 200).await?;

        let first_page = service.list_trashed_images(None, Some(2)).await?;
        assert_eq!(
            image_ids(&first_page.images),
            vec![third.image.id, second.image.id]
        );

        let second_page = service
            .list_trashed_images(first_page.next_cursor, Some(2))
            .await?;
        assert_eq!(image_ids(&second_page.images), vec![first.image.id]);
        assert!(second_page.next_cursor.is_none());

        Ok(())
    }

    #[sqlx::test]
    async fn moves_image_to_trash_and_restores_without_changing_files(
        pool: SqlitePool,
    ) -> Result<(), Box<dyn Error>> {
        let data_dir = tempdir()?;
        let service = test_service(pool, data_dir.path())?;
        let uploaded = service
            .upload_image("example.png", create_png(4, 2))
            .await?;
        let public_id = uploaded.image.public_id.clone();
        let storage_path = data_dir.path().join(&uploaded.image.storage_key);
        let thumbnail_path = data_dir.path().join(&uploaded.image.thumbnail_key);

        service.soft_delete_image(&public_id).await?;

        assert!(matches!(
            service.get_image(&public_id).await,
            Err(ServiceError::ImageNotFound),
        ));
        assert_eq!(
            service.get_trashed_image(&public_id).await?.status,
            Status::Trashed
        );
        assert!(storage_path.is_file());
        assert!(thumbnail_path.is_file());

        let mut opened = service
            .open_trashed_image(&public_id, ImageFileKind::Thumbnail)
            .await?;
        let mut bytes = Vec::new();
        opened.file.read_to_end(&mut bytes)?;
        assert!(!bytes.is_empty());

        service.restore_image(&public_id).await?;

        let restored = service.get_image(&public_id).await?;
        assert_eq!(restored.status, Status::Active);
        assert_eq!(restored.deleted_at, None);
        assert!(storage_path.is_file());
        assert!(thumbnail_path.is_file());

        Ok(())
    }

    #[sqlx::test]
    async fn refuses_restore_when_same_pixels_are_already_active(
        pool: SqlitePool,
    ) -> Result<(), Box<dyn Error>> {
        let data_dir = tempdir()?;
        let service = test_service(pool, data_dir.path())?;
        let source = create_png(4, 2);
        let trashed = service.upload_image("first.png", source.clone()).await?;
        let trashed_public_id = trashed.image.public_id.clone();

        service.soft_delete_image(&trashed_public_id).await?;

        let active = service.upload_image("second.png", source).await?;
        assert!(!active.already_exists);

        let error = service.restore_image(&trashed_public_id).await.unwrap_err();

        assert!(matches!(
            error,
            ServiceError::RestoreConflict(existing) if existing == active.image.public_id
        ));
        assert!(service.get_trashed_image(&trashed_public_id).await.is_ok());

        Ok(())
    }

    #[sqlx::test]
    async fn permanently_deletes_only_trashed_images(
        pool: SqlitePool,
    ) -> Result<(), Box<dyn Error>> {
        let data_dir = tempdir()?;
        let service = test_service(pool.clone(), data_dir.path())?;
        let uploaded = service
            .upload_image("example.png", create_png(4, 2))
            .await?;
        let public_id = uploaded.image.public_id.clone();
        let storage_path = data_dir.path().join(&uploaded.image.storage_key);
        let thumbnail_path = data_dir.path().join(&uploaded.image.thumbnail_key);

        assert!(matches!(
            service.delete_image(&public_id).await,
            Err(ServiceError::ImageNotFound),
        ));
        assert!(storage_path.is_file());

        service.soft_delete_image(&public_id).await?;
        service.delete_image(&public_id).await?;

        assert!(!storage_path.exists());
        assert!(!thumbnail_path.exists());
        assert!(
            service
                .repository
                .find_trashed_image_by_public_id(&public_id)
                .await?
                .is_none()
        );

        let image_count = sqlx::query_scalar!("SELECT COUNT(*) FROM images")
            .fetch_one(&pool)
            .await?;
        assert_eq!(image_count, 0);

        Ok(())
    }

    #[sqlx::test]
    async fn recovers_interrupted_uploads_and_deletions(
        pool: SqlitePool,
    ) -> Result<(), Box<dyn Error>> {
        let data_dir = tempdir()?;
        let service = test_service(pool.clone(), data_dir.path())?;

        let interrupted_upload = insert_uploading_image(
            &service,
            "F00000000001",
            "images/2026/08/F00000000001.webp",
            "thumbnails/2026/08/F00000000001.webp",
            '4',
            1,
        )
        .await?;

        service.storage.save_image(
            &interrupted_upload.storage_key,
            b"unfinished image",
            &interrupted_upload.thumbnail_key,
            b"unfinished thumbnail",
        )?;

        let pending_deletion = service.upload_image("delete.png", create_png(5, 2)).await?;
        service
            .soft_delete_image(&pending_deletion.image.public_id)
            .await?;
        service
            .repository
            .mark_trashed_for_deletion(&pending_deletion.image.public_id, 2)
            .await?
            .expect("回收站图片应进入 deleting 状态");

        let report = service.recover_images().await?;

        assert_eq!(report.claimed_uploads, 1);
        assert_eq!(report.cleaned, 2);
        assert!(report.failures.is_empty());
        assert!(
            !data_dir
                .path()
                .join(&interrupted_upload.storage_key)
                .exists()
        );
        assert!(
            !data_dir
                .path()
                .join(&interrupted_upload.thumbnail_key)
                .exists()
        );
        assert!(
            !data_dir
                .path()
                .join(&pending_deletion.image.storage_key)
                .exists()
        );

        let remaining = sqlx::query_scalar!("SELECT COUNT(*) FROM images")
            .fetch_one(&pool)
            .await?;
        assert_eq!(remaining, 0);

        Ok(())
    }

    #[sqlx::test]
    async fn keeps_failed_cleanup_as_deleting_and_moves_it_back(
        pool: SqlitePool,
    ) -> Result<(), Box<dyn Error>> {
        let data_dir = tempdir()?;
        let service = test_service(pool.clone(), data_dir.path())?;
        let image = insert_uploading_image(
            &service,
            "G00000000001",
            "images/../escape.webp",
            "thumbnails/../escape.webp",
            '5',
            1,
        )
        .await?;

        let report = service.recover_images().await?;

        assert_eq!(report.claimed_uploads, 1);
        assert_eq!(report.cleaned, 0);
        assert_eq!(report.failures.len(), 1);
        assert_eq!(report.failures[0].public_id, image.public_id);

        let row = sqlx::query!(
            "SELECT status, updated_at FROM images WHERE id = ?",
            image.id,
        )
        .fetch_one(&pool)
        .await?;

        assert_eq!(row.status, "deleting");
        assert!(row.updated_at > image.updated_at);

        Ok(())
    }

    #[sqlx::test]
    async fn rejects_invalid_original_names(pool: SqlitePool) -> Result<(), Box<dyn Error>> {
        let data_dir = tempdir()?;
        let service = test_service(pool, data_dir.path())?;

        assert!(matches!(
            service.upload_image("  ", create_png(4, 2)).await,
            Err(ServiceError::InvalidOriginalName),
        ));

        Ok(())
    }

    async fn insert_uploading_image(
        service: &Service,
        public_id: &str,
        storage_key: &str,
        thumbnail_key: &str,
        hash_character: char,
        created_at: i64,
    ) -> Result<StoredImage, Box<dyn Error>> {
        let public_id = PublicId::parse(public_id)?;
        let content_hash = repeated_hash('0');
        let pixel_hash = repeated_hash(hash_character);
        let image = NewImage {
            public_id: &public_id,
            storage_key,
            thumbnail_key,
            original_name: "interrupted.png",
            stored_size: 16,
            thumbnail_size: 20,
            width: 4,
            height: 2,
            thumbnail_width: 2,
            thumbnail_height: 1,
            content_hash: &content_hash,
            pixel_hash: &pixel_hash,
            created_at,
        };

        service
            .repository
            .create_uploading_image(image)
            .await?
            .ok_or_else(|| "测试 public_id 不应发生冲突".into())
    }

    async fn set_created_at(
        pool: &SqlitePool,
        id: i64,
        created_at: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE images SET created_at = ?1, updated_at = ?1 WHERE id = ?2",
            created_at,
            id,
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    async fn set_deleted_at(
        pool: &SqlitePool,
        id: i64,
        deleted_at: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE images SET deleted_at = ?1, updated_at = ?1 WHERE id = ?2",
            deleted_at,
            id,
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    fn test_service(pool: SqlitePool, data_path: &Path) -> Result<Service, Box<dyn Error>> {
        Ok(Service::new(
            Repository::new(pool),
            ImageProcessor::new(test_image_config())?,
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

    fn repeated_hash(character: char) -> String {
        std::iter::repeat_n(character, 64).collect()
    }

    fn image_ids(images: &[StoredImage]) -> Vec<i64> {
        images.iter().map(|image| image.id).collect()
    }
}
