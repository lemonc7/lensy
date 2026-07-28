use sqlx::SqlitePool;

use crate::model::image::{Image, NewImage};

pub struct ImageRepository {
    pool: SqlitePool,
}

impl ImageRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn find_active_by_pixel_hash(
        &self,
        pixel_hash: &str,
    ) -> Result<Option<Image>, sqlx::Error> {
        sqlx::query_as!(
            Image,
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

    pub async fn find_active_by_public_id(
        &self,
        public_id: &str,
    ) -> Result<Option<Image>, sqlx::Error> {
        sqlx::query_as!(
            Image,
            r#"
            SELECT *
            FROM images
            WHERE public_id = ?
              AND deleted_at IS NULL
            LIMIT 1
            "#,
            public_id,
        )
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn insert(&self, image: &NewImage) -> Result<Image, sqlx::Error> {
        sqlx::query_as!(
            Image,
            r#"
            INSERT INTO images (
                public_id,
                storage_key,
                thumbnail_key,
                original_name,
                source_mime,
                source_size,
                stored_size,
                thumbnail_size,
                width,
                height,
                thumbnail_width,
                thumbnail_height,
                source_hash,
                content_hash,
                pixel_hash,
                created_at,
                updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            RETURNING *
            "#,
            &image.public_id,
            &image.storage_key,
            &image.thumbnail_key,
            &image.original_name,
            &image.source_mime,
            image.source_size,
            image.stored_size,
            image.thumbnail_size,
            image.width,
            image.height,
            image.thumbnail_width,
            image.thumbnail_height,
            &image.source_hash,
            &image.content_hash,
            &image.pixel_hash,
            image.created_at,
            image.created_at
        )
        .fetch_one(&self.pool)
        .await
    }

    pub async fn soft_delete(&self, public_id: &str, deleted_at: i64) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            UPDATE images
            SET deleted_at = ?,
                updated_at = ?
            WHERE public_id = ?
              AND deleted_at IS NULL
            "#,
            deleted_at,
            deleted_at,
            public_id
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn restore(&self, public_id: &str, updated_at: i64) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            UPDATE images
            SET deleted_at = NULL,
                updated_at = ?
            WHERE public_id = ?
              AND deleted_at IS NOT NULL
            "#,
            updated_at,
            public_id
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn find_deleted_by_public_id(
        &self,
        public_id: &str,
    ) -> Result<Option<Image>, sqlx::Error> {
        sqlx::query_as!(
            Image,
            r#"
            SELECT *
            FROM images
            WHERE public_id = ?
              AND deleted_at IS NOT NULL
            LIMIT 1
            "#,
            public_id
        )
        .fetch_optional(&self.pool)
        .await
    }

    // 分页查询
    pub async fn list_active(
        &self,
        cursor: Option<(i64, i64)>,
        limit: u32,
    ) -> Result<Vec<Image>, sqlx::Error> {
        let limit = limit.clamp(1, 100) as i64;
        match cursor {
            Some((created_at, id)) => {
                sqlx::query_as!(
                    Image,
                    r#"
                    SELECT *
                    FROM images
                    WHERE deleted_at IS NULL
                      AND (
                        created_at < ?
                        OR (created_at = ? AND id < ?)
                      )
                    ORDER BY created_at DESC, id DESC
                    LIMIT ?
                    "#,
                    created_at,
                    created_at,
                    id,
                    limit
                )
                .fetch_all(&self.pool)
                .await
            }
            None => {
                sqlx::query_as!(
                    Image,
                    r#"
                    SELECT *
                    FROM images
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
    }

    // 回收站分页查询
    pub async fn list_deleted(
        &self,
        cursor: Option<(i64, i64)>,
        limit: u32,
    ) -> Result<Vec<Image>, sqlx::Error> {
        let limit = limit.clamp(1, 100) as i64;
        match cursor {
            Some((deleted_at, id)) => {
                sqlx::query_as!(
                    Image,
                    r#"
                    SELECT *
                    FROM images
                    WHERE deleted_at IS NOT NULL
                      AND (
                        deleted_at < ?
                        OR (deleted_at = ? AND id < ?)
                      )
                    ORDER BY deleted_at DESC, id DESC
                    LIMIT ?
                    "#,
                    deleted_at,
                    deleted_at,
                    id,
                    limit
                )
                .fetch_all(&self.pool)
                .await
            }
            None => {
                sqlx::query_as!(
                    Image,
                    r#"
                    SELECT *
                    FROM images
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
    }

    // 永久删除
    pub async fn delete_permanently(&self, public_id: &str) -> Result<Option<Image>, sqlx::Error> {
        sqlx::query_as!(
            Image,
            r#"
            DELETE FROM images
            WHERE public_id = ?
              AND deleted_at IS NOT NULL
            RETURNING *
            "#,
            public_id
        )
        .fetch_optional(&self.pool)
        .await
    }
}
