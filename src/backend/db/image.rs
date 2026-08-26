use crate::backend::{
    db::Repository,
    error::ImageWriteError,
    model::{ImageCursor, NewImage, PendingFileDeletion, PendingUpload, PublicId, StoredImage},
};

impl Repository {
    pub async fn list_active_images(
        &self,
        cursor: Option<ImageCursor>,
        limit: i64,
    ) -> Result<Vec<StoredImage>, sqlx::Error> {
        if let Some(cursor) = cursor {
            sqlx::query_as!(
                StoredImage,
                r#"
                SELECT * FROM images
                WHERE deleted_at IS NULL
                  AND (
                      created_at < ?1
                      OR (created_at = ?1 AND id < ?2)
                  )
                ORDER BY created_at DESC, id DESC
                LIMIT ?3
                "#,
                cursor.timestamp,
                cursor.id,
                limit
            )
            .fetch_all(&self.pool)
            .await
        } else {
            sqlx::query_as!(
                StoredImage,
                r#"
                SELECT * FROM images
                WHERE deleted_at IS NULL
                ORDER BY created_at DESC, id DESC
                LIMIT ?
                "#,
                limit
            )
            .fetch_all(&self.pool)
            .await
        }
    }

    pub async fn list_deleted_images(
        &self,
        cursor: Option<ImageCursor>,
        limit: i64,
    ) -> Result<Vec<StoredImage>, sqlx::Error> {
        if let Some(cursor) = cursor {
            sqlx::query_as!(
                StoredImage,
                r#"
                SELECT * FROM images
                WHERE deleted_at IS NOT NULL
                  AND (
                      deleted_at < ?1
                      OR (deleted_at = ?1 AND id < ?2)
                  )
                ORDER BY deleted_at DESC, id DESC
                LIMIT ?3
                "#,
                cursor.timestamp,
                cursor.id,
                limit
            )
            .fetch_all(&self.pool)
            .await
        } else {
            sqlx::query_as!(
                StoredImage,
                r#"
                SELECT * FROM images
                WHERE deleted_at IS NOT NULL
                ORDER BY deleted_at DESC, id DESC
                LIMIT ?
                "#,
                limit
            )
            .fetch_all(&self.pool)
            .await
        }
    }

    pub async fn create_image(&self, image: NewImage<'_>) -> Result<StoredImage, ImageWriteError> {
        let result = sqlx::query_as!(
            StoredImage,
            r#"
            INSERT INTO images (
                public_id,
                storage_key,
                thumbnail_key,
                original_name,
                stored_size,
                thumbnail_size,
                width,
                height,
                thumbnail_width,
                thumbnail_height,
                content_hash,
                pixel_hash,
                created_at,
                updated_at
            )
            SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?13
            WHERE EXISTS (
                SELECT 1
                FROM pending_uploads
                WHERE public_id = ?1
                  AND storage_key = ?2
                  AND thumbnail_key = ?3
            )
            RETURNING *
            "#,
            image.public_id.as_str(),
            image.storage_key,
            image.thumbnail_key,
            image.original_name,
            image.stored_size,
            image.thumbnail_size,
            image.width,
            image.height,
            image.thumbnail_width,
            image.thumbnail_height,
            image.content_hash,
            image.pixel_hash,
            image.created_at
        )
        .fetch_optional(&self.pool)
        .await;

        match result {
            Ok(Some(image)) => Ok(image),
            Ok(None) => Err(ImageWriteError::PendingUploadMissing),
            Err(error) if is_active_pixel_conflict(&error) => {
                Err(ImageWriteError::ActivePixelConflict)
            }
            Err(error) => Err(ImageWriteError::Database(error)),
        }
    }

    pub async fn find_active_image_by_pixel_hash(
        &self,
        pixel_hash: &str,
    ) -> Result<Option<StoredImage>, sqlx::Error> {
        sqlx::query_as!(
            StoredImage,
            r#"
            SELECT *
            FROM images
            WHERE pixel_hash = ?
              AND deleted_at IS NULL
            LIMIT 1
            "#,
            pixel_hash
        )
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn reserve_pending_upload(
        &self,
        public_id: &PublicId,
        storage_key: &str,
        thumbnail_key: &str,
        created_at: i64,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            INSERT INTO pending_uploads (
                public_id,
                storage_key,
                thumbnail_key,
                created_at
            )
            SELECT ?1, ?2, ?3, ?4
            WHERE NOT EXISTS (
                SELECT 1
                FROM images
                WHERE public_id = ?1
            )
              AND NOT EXISTS (
                  SELECT 1
                  FROM pending_file_deletions
                  WHERE storage_key = ?2
                    OR thumbnail_key = ?3
              )
            ON CONFLICT(public_id) DO NOTHING
            "#,
            public_id.as_str(),
            storage_key,
            thumbnail_key,
            created_at
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn remove_pending_upload(&self, public_id: &PublicId) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            DELETE FROM pending_uploads
            WHERE public_id = ?
            "#,
            public_id.as_str()
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_pending_uploads(&self) -> Result<Vec<PendingUpload>, sqlx::Error> {
        sqlx::query_as!(
            PendingUpload,
            r#"
            SELECT *
            FROM pending_uploads
            ORDER BY created_at, public_id
            "#,
        )
        .fetch_all(&self.pool)
        .await
    }

    // 原子检查恢复任务是否可以删除文件
    pub async fn claim_pending_upload_for_cleanup(
        &self,
        pending: &PendingUpload,
    ) -> Result<bool, sqlx::Error> {
        let mut transaction = self.pool.begin().await?;

        // 先把清理任务转入持久删除队列，再移除上传许可
        // 正常任务：创建pending_uploads -> 写入文件 -> 插入images -> 触发器删除pending_uploads
        // 可能在插入images时出错，后续恢复任务需要清理文件和pending_uploads
        // 删除pending_uploading -> 进程崩溃 -> 文件还没删除 -> 数据库没有文件路径 -> 形成孤儿文件
        // 所以先将路径写入pending_file_deletions
        // pending_uploading -> pending_file_deletions -> 删除磁盘文件 -> 删除pending_file_deletions
        let queued = sqlx::query!(
            r#"
            INSERT INTO pending_file_deletions (
                storage_key,
                thumbnail_key,
                created_at
            )
            SELECT storage_key, thumbnail_key, created_at
            FROM pending_uploads
            WHERE public_id = ?1
              AND storage_key = ?2
              AND thumbnail_key = ?3
            ON CONFLICT DO NOTHING
            "#,
            pending.public_id.as_str(),
            pending.storage_key,
            pending.thumbnail_key,
        )
        .execute(&mut *transaction)
        .await?;

        // 如果pending不存在，或者无法转入到删除队列，说明现在不能删除文件
        if queued.rows_affected() == 0 {
            return Ok(false);
        }

        let removed = sqlx::query!(
            r#"
            DELETE FROM pending_uploads
            WHERE public_id = ?1
              AND storage_key = ?2
              AND thumbnail_key = ?3
            "#,
            pending.public_id.as_str(),
            pending.storage_key,
            pending.thumbnail_key,
        )
        .execute(&mut *transaction)
        .await?;

        if removed.rows_affected() != 1 {
            return Ok(false);
        }

        transaction.commit().await?;
        Ok(true)
    }

    pub async fn find_active_image_by_public_id(
        &self,
        public_id: &PublicId,
    ) -> Result<Option<StoredImage>, sqlx::Error> {
        sqlx::query_as!(
            StoredImage,
            r#"
            SELECT *
            FROM images
            WHERE public_id = ?1
              AND deleted_at IS NULL
            LIMIT 1
            "#,
            public_id.as_str(),
        )
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn find_deleted_image_by_public_id(
        &self,
        public_id: &PublicId,
    ) -> Result<Option<StoredImage>, sqlx::Error> {
        sqlx::query_as!(
            StoredImage,
            r#"
            SELECT *
            FROM images
            WHERE public_id = ?1
              AND deleted_at IS NOT NULL
            LIMIT 1
            "#,
            public_id.as_str()
        )
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn soft_delete_image(
        &self,
        public_id: &PublicId,
        deleted_at: i64,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            UPDATE images
            SET
                deleted_at = ?1,
                updated_at = ?1
            WHERE public_id = ?2
              AND deleted_at IS NULL
            "#,
            deleted_at,
            public_id.as_str()
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn restore_image(
        &self,
        public_id: &PublicId,
        updated_at: i64,
    ) -> Result<bool, ImageWriteError> {
        let result = sqlx::query!(
            r#"
            UPDATE images
            SET
                deleted_at = NULL,
                updated_at = ?
            WHERE public_id = ?
              AND deleted_at IS NOT NULL
            "#,
            updated_at,
            public_id.as_str()
        )
        .execute(&self.pool)
        .await;
        match result {
            Ok(result) => Ok(result.rows_affected() == 1),
            Err(error) if is_active_pixel_conflict(&error) => {
                Err(ImageWriteError::ActivePixelConflict)
            }
            Err(error) => Err(ImageWriteError::Database(error)),
        }
    }

    pub async fn delete_image(
        &self,
        public_id: &PublicId,
    ) -> Result<Option<StoredImage>, sqlx::Error> {
        sqlx::query_as!(
            StoredImage,
            r#"
            DELETE FROM images
            WHERE public_id = ?1
              AND deleted_at IS NOT NULL
            RETURNING *
            "#,
            public_id.as_str()
        )
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn list_pending_file_deletions(
        &self,
    ) -> Result<Vec<PendingFileDeletion>, sqlx::Error> {
        sqlx::query_as!(
            PendingFileDeletion,
            r#"
            SELECT *
            FROM pending_file_deletions
            ORDER BY created_at, storage_key
            "#,
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn remove_pending_file_deletion(&self, storage_key: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            DELETE FROM pending_file_deletions
            WHERE storage_key = ?
            "#,
            storage_key
        )
        .execute(&self.pool)
        .await?;
        // 记录存在 删除，不存在仍然成功
        Ok(())
    }
}

fn is_active_pixel_conflict(error: &sqlx::Error) -> bool {
    let sqlx::Error::Database(database_error) = error else {
        return false;
    };

    database_error.is_unique_violation()
        && database_error
            .message()
            .split(',')
            .any(|column| column.trim().ends_with("images.pixel_hash"))
}

#[cfg(test)]
mod tests {
    use sqlx::{SqlitePool, sqlite::SqliteQueryResult};

    use crate::backend::{
        db::Repository,
        error::ImageWriteError,
        model::{NewImage, PublicId},
    };

    use super::is_active_pixel_conflict;

    #[sqlx::test]
    async fn recognizes_only_pixel_hash_unique_conflicts(
        pool: SqlitePool,
    ) -> Result<(), sqlx::Error> {
        let pixel_hash = "a".repeat(64);
        insert_image(&pool, "A8kLm2Pq7XzB", "first", &pixel_hash).await?;

        let pixel_error = insert_image(&pool, "B8kLm2Pq7XzB", "second", &pixel_hash)
            .await
            .unwrap_err();

        assert!(is_active_pixel_conflict(&pixel_error));

        let other_pixel_hash = "b".repeat(64);
        let public_id_error = insert_image(&pool, "A8kLm2Pq7XzB", "third", &other_pixel_hash)
            .await
            .unwrap_err();

        assert!(!is_active_pixel_conflict(&public_id_error));

        Ok(())
    }

    #[sqlx::test]
    async fn pending_cleanup_and_image_creation_are_mutually_exclusive(
        pool: SqlitePool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let repository = Repository::new(pool);
        let public_id = PublicId::parse("C8kLm2Pq7XzB")?;
        let storage_key = "images/2026/08/race.webp";
        let thumbnail_key = "thumbnails/2026/08/race.webp";

        assert!(
            repository
                .reserve_pending_upload(&public_id, storage_key, thumbnail_key, 1)
                .await?
        );

        let pending = repository
            .list_pending_uploads()
            .await?
            .into_iter()
            .next()
            .expect("应存在 pending upload");

        assert!(
            repository
                .claim_pending_upload_for_cleanup(&pending)
                .await?
        );

        let content_hash = "c".repeat(64);
        let pixel_hash = "d".repeat(64);
        let result = repository
            .create_image(NewImage {
                public_id: &public_id,
                storage_key,
                thumbnail_key,
                original_name: "race.png",
                stored_size: 100,
                thumbnail_size: 50,
                width: 10,
                height: 10,
                thumbnail_width: 5,
                thumbnail_height: 5,
                content_hash: &content_hash,
                pixel_hash: &pixel_hash,
                created_at: 1,
            })
            .await;

        assert!(matches!(result, Err(ImageWriteError::PendingUploadMissing)));

        let completed_id = PublicId::parse("D8kLm2Pq7XzB")?;
        let completed_storage_key = "images/2026/08/completed.webp";
        let completed_thumbnail_key = "thumbnails/2026/08/completed.webp";
        assert!(
            repository
                .reserve_pending_upload(
                    &completed_id,
                    completed_storage_key,
                    completed_thumbnail_key,
                    2,
                )
                .await?
        );
        let stale_pending = repository
            .list_pending_uploads()
            .await?
            .into_iter()
            .find(|pending| pending.public_id == completed_id)
            .expect("应存在第二条 pending upload");
        let completed_pixel_hash = "e".repeat(64);
        repository
            .create_image(NewImage {
                public_id: &completed_id,
                storage_key: completed_storage_key,
                thumbnail_key: completed_thumbnail_key,
                original_name: "completed.png",
                stored_size: 100,
                thumbnail_size: 50,
                width: 10,
                height: 10,
                thumbnail_width: 5,
                thumbnail_height: 5,
                content_hash: &content_hash,
                pixel_hash: &completed_pixel_hash,
                created_at: 2,
            })
            .await
            .expect("持有 pending 时应能创建图片");

        assert!(
            !repository
                .claim_pending_upload_for_cleanup(&stale_pending)
                .await?
        );
        Ok(())
    }

    async fn insert_image(
        pool: &SqlitePool,
        public_id: &str,
        key_suffix: &str,
        pixel_hash: &str,
    ) -> Result<SqliteQueryResult, sqlx::Error> {
        let storage_key = format!("images/2026/08/{key_suffix}.webp");
        let thumbnail_key = format!("thumbnails/2026/08/{key_suffix}.webp");

        sqlx::query(
            r#"
            INSERT INTO images (
                public_id,
                storage_key,
                thumbnail_key,
                original_name,
                stored_size,
                thumbnail_size,
                width,
                height,
                thumbnail_width,
                thumbnail_height,
                content_hash,
                pixel_hash,
                created_at,
                updated_at
            )
            VALUES (?1, ?2, ?3, 'test.png', 100, 50, 10, 10, 5, 5, ?4, ?5, 1, 1)
            "#,
        )
        .bind(public_id)
        .bind(storage_key)
        .bind(thumbnail_key)
        .bind("0".repeat(64))
        .bind(pixel_hash)
        .execute(pool)
        .await
    }
}
