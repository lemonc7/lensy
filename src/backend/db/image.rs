use crate::backend::{
    db::Repository,
    error::ImageWriteError,
    model::{ImageCursor, NewImage, PublicId, Status, StoredImage},
};

impl Repository {
    // Active
    pub async fn list_active_images(
        &self,
        cursor: Option<ImageCursor>,
        limit: i64,
    ) -> Result<Vec<StoredImage>, sqlx::Error> {
        if let Some(cursor) = cursor {
            sqlx::query_as!(
                StoredImage,
                r#"
                SELECT
                    id,
                    public_id AS "public_id: PublicId",
                    status AS "status: Status",
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
                    updated_at,
                    deleted_at
                FROM images
                WHERE status = 'active'
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
                SELECT
                    id,
                    public_id AS "public_id: PublicId",
                    status AS "status: Status",
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
                    updated_at,
                    deleted_at
                FROM images
                WHERE status = 'active'
                ORDER BY created_at DESC, id DESC
                LIMIT ?
                "#,
                limit
            )
            .fetch_all(&self.pool)
            .await
        }
    }

    pub async fn find_active_image_by_public_id(
        &self,
        public_id: &PublicId,
    ) -> Result<Option<StoredImage>, sqlx::Error> {
        sqlx::query_as!(
            StoredImage,
            r#"
            SELECT
                id,
                public_id AS "public_id: PublicId",
                status AS "status: Status",
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
                updated_at,
                deleted_at
            FROM images
            WHERE public_id = ?1
              AND status = 'active'
            LIMIT 1
            "#,
            public_id,
        )
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn find_active_image_by_pixel_hash(
        &self,
        pixel_hash: &str,
    ) -> Result<Option<StoredImage>, sqlx::Error> {
        sqlx::query_as!(
            StoredImage,
            r#"
            SELECT
                id,
                public_id AS "public_id: PublicId",
                status AS "status: Status",
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
                updated_at,
                deleted_at
            FROM images
            WHERE pixel_hash = ?
              AND status = 'active'
            LIMIT 1
            "#,
            pixel_hash
        )
        .fetch_optional(&self.pool)
        .await
    }

    // Trashed
    pub async fn list_trashed_images(
        &self,
        cursor: Option<ImageCursor>,
        limit: i64,
    ) -> Result<Vec<StoredImage>, sqlx::Error> {
        if let Some(cursor) = cursor {
            sqlx::query_as!(
                StoredImage,
                r#"
                SELECT
                    id,
                    public_id AS "public_id: PublicId",
                    status AS "status: Status",
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
                    updated_at,
                    deleted_at
                FROM images
                WHERE status = 'trashed'
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
                SELECT
                    id,
                    public_id AS "public_id: PublicId",
                    status AS "status: Status",
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
                    updated_at,
                    deleted_at
                FROM images
                WHERE status = 'trashed'
                ORDER BY deleted_at DESC, id DESC
                LIMIT ?
                "#,
                limit
            )
            .fetch_all(&self.pool)
            .await
        }
    }

    pub async fn find_trashed_image_by_public_id(
        &self,
        public_id: &PublicId,
    ) -> Result<Option<StoredImage>, sqlx::Error> {
        sqlx::query_as!(
            StoredImage,
            r#"
            SELECT
                id,
                public_id AS "public_id: PublicId",
                status AS "status: Status",
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
                updated_at,
                deleted_at
            FROM images
            WHERE public_id = ?1
              AND status = 'trashed'
            LIMIT 1
            "#,
            public_id
        )
        .fetch_optional(&self.pool)
        .await
    }

    // Upload
    pub async fn create_uploading_image(
        &self,
        image: NewImage<'_>,
    ) -> Result<Option<StoredImage>, sqlx::Error> {
        sqlx::query_as!(
            StoredImage,
            r#"
            INSERT INTO images (
                public_id,
                status,
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
            VALUES (?1, 'uploading', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?13)
            ON CONFLICT(public_id) DO NOTHING
            RETURNING
                id,
                public_id AS "public_id: PublicId",
                status AS "status: Status",
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
                updated_at,
                deleted_at
            "#,
            image.public_id,
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
        .await
    }

    pub async fn activate_image(
        &self,
        id: i64,
        updated_at: i64,
    ) -> Result<Option<StoredImage>, ImageWriteError> {
        let result = sqlx::query_as!(
            StoredImage,
            r#"
            UPDATE images
            SET
                status = 'active',
                updated_at = ?1
            WHERE id = ?2
              AND status = 'uploading'
            RETURNING
                id,
                public_id AS "public_id: PublicId",
                status AS "status: Status",
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
                updated_at,
                deleted_at
            "#,
            updated_at,
            id,
        )
        .fetch_optional(&self.pool)
        .await;

        match result {
            Ok(image) => Ok(image),
            Err(error) if is_active_pixel_conflict(&error) => {
                Err(ImageWriteError::ActivePixelConflict)
            }
            Err(error) => Err(ImageWriteError::Database(error)),
        }
    }

    // true => 成功执行uploading -> deleting，可以删除文件
    // false => 记录不存在或状态已经不是uploading，不能删除文件
    pub async fn mark_upload_for_deletion(
        &self,
        id: i64,
        updated_at: i64,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            UPDATE images
            SET
                status = 'deleting',
                updated_at = ?
            WHERE id = ?
              AND status = 'uploading'
            "#,
            updated_at,
            id
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    // Lifecycle
    pub async fn soft_delete_image(
        &self,
        public_id: &PublicId,
        deleted_at: i64,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            UPDATE images
            SET
                status = 'trashed',
                deleted_at = ?1,
                updated_at = ?1
            WHERE public_id = ?2
              AND status = 'active'
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
                status = 'active',
                deleted_at = NULL,
                updated_at = ?
            WHERE public_id = ?
              AND status = 'trashed'
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

    pub async fn mark_trashed_for_deletion(
        &self,
        public_id: &PublicId,
        updated_at: i64,
    ) -> Result<Option<StoredImage>, sqlx::Error> {
        sqlx::query_as!(
            StoredImage,
            r#"
            UPDATE images
            SET
                status = 'deleting',
                updated_at = ?
            WHERE public_id = ?
              AND status = 'trashed'
            RETURNING
                id,
                public_id AS "public_id: PublicId",
                status AS "status: Status",
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
                updated_at,
                deleted_at
            "#,
            updated_at,
            public_id
        )
        .fetch_optional(&self.pool)
        .await
    }

    // Cleanup
    // stale_before：最后更新时间早于这个时间的上传才算超时
    pub async fn claim_stale_uploads_for_deletion(
        &self,
        stale_before: i64,
        updated_at: i64,
        limit: i64,
    ) -> Result<Vec<StoredImage>, sqlx::Error> {
        sqlx::query_as!(
            StoredImage,
            r#"
            UPDATE images
            SET
                status = 'deleting',
                updated_at = ?
            WHERE id IN (
                SELECT id
                FROM images
                WHERE status = 'uploading'
                  AND updated_at <= ?
                ORDER BY updated_at ASC, id ASC
                LIMIT ?
            )
              AND status = 'uploading'
            RETURNING
                id,
                public_id AS "public_id: PublicId",
                status AS "status: Status",
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
                updated_at,
                deleted_at
            "#,
            updated_at,
            stale_before,
            limit
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn list_deleting_images(&self, limit: i64) -> Result<Vec<StoredImage>, sqlx::Error> {
        sqlx::query_as!(
            StoredImage,
            r#"
            SELECT
                id,
                public_id AS "public_id: PublicId",
                status AS "status: Status",
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
                updated_at,
                deleted_at
            FROM images
            WHERE status = 'deleting'
            ORDER BY updated_at ASC, id ASC
            LIMIT ?
            "#,
            limit
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn finish_image_deletion(&self, id: i64) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM images
            WHERE id = ?
              AND status = 'deleting'
            "#,
            id
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn defer_image_deletion(
        &self,
        id: i64,
        updated_at: i64,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            UPDATE images
            SET updated_at = MAX(updated_at + 1, ?)
            WHERE id = ?
              AND status = 'deleting'
            "#,
            updated_at,
            id
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
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
    use std::error::Error;

    use sqlx::SqlitePool;

    use crate::backend::{
        db::Repository,
        error::ImageWriteError,
        model::{ImageCursor, NewImage, PublicId, Status, StoredImage},
    };

    #[sqlx::test]
    async fn creates_and_transitions_image_through_lifecycle(
        pool: SqlitePool,
    ) -> Result<(), Box<dyn Error>> {
        let repository = Repository::new(pool);
        let image =
            create_uploading_image(&repository, "A00000000001", repeated_hash('a'), 10).await?;

        assert_eq!(image.status, Status::Uploading);
        assert!(
            repository
                .find_active_image_by_public_id(&image.public_id)
                .await?
                .is_none()
        );

        let active = repository
            .activate_image(image.id, 20)
            .await
            .map_err(image_write_database_error)?
            .expect("uploading 图片应当能够被激活");

        assert_eq!(active.status, Status::Active);
        assert_eq!(active.updated_at, 20);
        assert!(
            repository
                .find_active_image_by_pixel_hash(&active.pixel_hash)
                .await?
                .is_some()
        );

        assert!(repository.soft_delete_image(&active.public_id, 30).await?);
        assert!(
            repository
                .find_active_image_by_public_id(&active.public_id)
                .await?
                .is_none()
        );

        let trashed = repository
            .find_trashed_image_by_public_id(&active.public_id)
            .await?
            .expect("软删除后的图片应位于回收站");

        assert_eq!(trashed.status, Status::Trashed);
        assert_eq!(trashed.deleted_at, Some(30));

        assert!(
            repository
                .restore_image(&trashed.public_id, 40)
                .await
                .map_err(image_write_database_error)?
        );

        let restored = repository
            .find_active_image_by_public_id(&trashed.public_id)
            .await?
            .expect("恢复后的图片应重新可见");

        assert_eq!(restored.status, Status::Active);
        assert_eq!(restored.deleted_at, None);

        assert!(
            repository
                .soft_delete_image(&restored.public_id, 50)
                .await?
        );

        let deleting = repository
            .mark_trashed_for_deletion(&restored.public_id, 60)
            .await?
            .expect("回收站图片应能够进入 deleting 状态");

        assert_eq!(deleting.status, Status::Deleting);
        assert!(repository.finish_image_deletion(deleting.id).await?);
        assert!(!repository.finish_image_deletion(deleting.id).await?);

        Ok(())
    }

    #[sqlx::test]
    async fn enforces_unique_pixel_hash_only_for_active_images(
        pool: SqlitePool,
    ) -> Result<(), Box<dyn Error>> {
        let repository = Repository::new(pool);
        let pixel_hash = repeated_hash('b');
        let first =
            create_uploading_image(&repository, "B00000000001", pixel_hash.clone(), 10).await?;
        let second =
            create_uploading_image(&repository, "B00000000002", pixel_hash.clone(), 11).await?;

        repository
            .activate_image(first.id, 20)
            .await
            .map_err(image_write_database_error)?
            .expect("第一张图片应激活成功");

        assert!(matches!(
            repository.activate_image(second.id, 21).await,
            Err(ImageWriteError::ActivePixelConflict),
        ));

        assert!(repository.soft_delete_image(&first.public_id, 30).await?);

        repository
            .activate_image(second.id, 31)
            .await
            .map_err(image_write_database_error)?
            .expect("第一张图片移入回收站后，第二张应能够激活");

        assert!(matches!(
            repository.restore_image(&first.public_id, 32).await,
            Err(ImageWriteError::ActivePixelConflict),
        ));

        let still_trashed = repository
            .find_trashed_image_by_public_id(&first.public_id)
            .await?
            .expect("恢复冲突后原图片应继续留在回收站");
        assert_eq!(still_trashed.status, Status::Trashed);

        Ok(())
    }

    #[sqlx::test]
    async fn paginates_active_and_trashed_images_stably(
        pool: SqlitePool,
    ) -> Result<(), Box<dyn Error>> {
        let repository = Repository::new(pool);
        let first = create_and_activate(&repository, "C00000000001", 'c', 10).await?;
        let second = create_and_activate(&repository, "C00000000002", 'd', 20).await?;
        let third = create_and_activate(&repository, "C00000000003", 'e', 20).await?;

        let first_page = repository.list_active_images(None, 2).await?;
        assert_eq!(ids(&first_page), vec![third.id, second.id]);

        let cursor = ImageCursor {
            timestamp: second.created_at,
            id: second.id,
        };
        let second_page = repository.list_active_images(Some(cursor), 2).await?;
        assert_eq!(ids(&second_page), vec![first.id]);

        assert!(repository.soft_delete_image(&first.public_id, 100).await?);
        assert!(repository.soft_delete_image(&second.public_id, 200).await?);
        assert!(repository.soft_delete_image(&third.public_id, 200).await?);

        let first_page = repository.list_trashed_images(None, 2).await?;
        assert_eq!(ids(&first_page), vec![third.id, second.id]);

        let cursor = ImageCursor {
            timestamp: 200,
            id: second.id,
        };
        let second_page = repository.list_trashed_images(Some(cursor), 2).await?;
        assert_eq!(ids(&second_page), vec![first.id]);

        Ok(())
    }

    #[sqlx::test]
    async fn claims_only_stale_uploads_and_defers_failed_deletions(
        pool: SqlitePool,
    ) -> Result<(), Box<dyn Error>> {
        let repository = Repository::new(pool);
        let stale =
            create_uploading_image(&repository, "D00000000001", repeated_hash('f'), 10).await?;
        let fresh =
            create_uploading_image(&repository, "D00000000002", repeated_hash('1'), 30).await?;

        let claimed = repository
            .claim_stale_uploads_for_deletion(20, 40, 10)
            .await?;

        assert_eq!(ids(&claimed), vec![stale.id]);
        assert_eq!(claimed[0].status, Status::Deleting);

        let deleting = repository.list_deleting_images(10).await?;
        assert_eq!(ids(&deleting), vec![stale.id]);

        assert!(repository.defer_image_deletion(stale.id, 50).await?);
        let deferred = repository.list_deleting_images(10).await?;
        assert_eq!(deferred[0].updated_at, 50);

        assert!(repository.mark_upload_for_deletion(fresh.id, 60).await?);
        assert!(!repository.mark_upload_for_deletion(fresh.id, 61).await?);

        assert!(repository.finish_image_deletion(stale.id).await?);
        assert!(repository.finish_image_deletion(fresh.id).await?);

        Ok(())
    }

    #[sqlx::test]
    async fn returns_none_when_public_id_is_already_reserved(
        pool: SqlitePool,
    ) -> Result<(), Box<dyn Error>> {
        let repository = Repository::new(pool);
        let public_id = PublicId::parse("E00000000001")?;
        let storage_key = format!("images/2026/08/{public_id}.webp");
        let thumbnail_key = format!("thumbnails/2026/08/{public_id}.webp");
        let content_hash = repeated_hash('2');
        let pixel_hash = repeated_hash('3');

        let first = NewImage {
            public_id: &public_id,
            storage_key: &storage_key,
            thumbnail_key: &thumbnail_key,
            original_name: "first.png",
            stored_size: 10,
            thumbnail_size: 5,
            width: 4,
            height: 2,
            thumbnail_width: 2,
            thumbnail_height: 1,
            content_hash: &content_hash,
            pixel_hash: &pixel_hash,
            created_at: 10,
        };

        assert!(repository.create_uploading_image(first).await?.is_some());

        let duplicate = NewImage {
            public_id: &public_id,
            storage_key: &storage_key,
            thumbnail_key: &thumbnail_key,
            original_name: "second.png",
            stored_size: 10,
            thumbnail_size: 5,
            width: 4,
            height: 2,
            thumbnail_width: 2,
            thumbnail_height: 1,
            content_hash: &content_hash,
            pixel_hash: &pixel_hash,
            created_at: 11,
        };

        assert!(
            repository
                .create_uploading_image(duplicate)
                .await?
                .is_none()
        );

        Ok(())
    }

    async fn create_and_activate(
        repository: &Repository,
        public_id: &str,
        hash_character: char,
        created_at: i64,
    ) -> Result<StoredImage, Box<dyn Error>> {
        let image = create_uploading_image(
            repository,
            public_id,
            repeated_hash(hash_character),
            created_at,
        )
        .await?;

        repository
            .activate_image(image.id, created_at)
            .await
            .map_err(image_write_database_error)?
            .ok_or_else(|| "图片激活失败".into())
    }

    async fn create_uploading_image(
        repository: &Repository,
        public_id: &str,
        pixel_hash: String,
        created_at: i64,
    ) -> Result<StoredImage, Box<dyn Error>> {
        let public_id = PublicId::parse(public_id)?;
        let storage_key = format!("images/2026/08/{public_id}.webp");
        let thumbnail_key = format!("thumbnails/2026/08/{public_id}.webp");
        let content_hash = repeated_hash('0');

        let image = NewImage {
            public_id: &public_id,
            storage_key: &storage_key,
            thumbnail_key: &thumbnail_key,
            original_name: "example.png",
            stored_size: 10,
            thumbnail_size: 5,
            width: 4,
            height: 2,
            thumbnail_width: 2,
            thumbnail_height: 1,
            content_hash: &content_hash,
            pixel_hash: &pixel_hash,
            created_at,
        };

        repository
            .create_uploading_image(image)
            .await?
            .ok_or_else(|| "测试 public_id 不应发生冲突".into())
    }

    fn image_write_database_error(error: ImageWriteError) -> Box<dyn Error> {
        match error {
            ImageWriteError::Database(error) => Box::new(error),
            ImageWriteError::ActivePixelConflict => "意外的有效图片像素冲突".into(),
        }
    }

    fn repeated_hash(character: char) -> String {
        std::iter::repeat_n(character, 64).collect()
    }

    fn ids(images: &[StoredImage]) -> Vec<i64> {
        images.iter().map(|image| image.id).collect()
    }
}
