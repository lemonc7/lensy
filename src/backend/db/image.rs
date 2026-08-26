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
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?13)
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
        .fetch_one(&self.pool)
        .await;

        match result {
            Ok(image) => Ok(image),
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
