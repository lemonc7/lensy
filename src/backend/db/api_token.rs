use crate::backend::{db::Repository, model::StoredApiToken};

impl Repository {
    pub(crate) async fn create_api_token(
        &self,
        name: &str,
        token_prefix: &str,
        token_hash: &str,
        created_at: i64,
        expires_at: Option<i64>,
    ) -> Result<StoredApiToken, sqlx::Error> {
        sqlx::query_as!(
            StoredApiToken,
            r#"
            INSERT INTO api_tokens (
                name,
                token_prefix,
                token_hash,
                created_at,
                expires_at
            )
            VALUES (?, ?, ?, ?, ?)
            RETURNING *
            "#,
            name,
            token_prefix,
            token_hash,
            created_at,
            expires_at
        )
        .fetch_one(&self.pool)
        .await
    }

    pub(crate) async fn list_api_tokens(&self) -> Result<Vec<StoredApiToken>, sqlx::Error> {
        sqlx::query_as!(
            StoredApiToken,
            r#"
            SELECT * FROM api_tokens
            ORDER BY created_at DESC, id DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
    }

    pub(crate) async fn find_usable_api_token_by_hash(
        &self,
        token_hash: &str,
        now: i64,
    ) -> Result<Option<StoredApiToken>, sqlx::Error> {
        sqlx::query_as!(
            StoredApiToken,
            r#"
            SELECT * FROM api_tokens
            WHERE token_hash = ?
              AND revoked_at IS NULL 
              AND (
                  expires_at IS NULL
                  OR expires_at > ?
              )
            LIMIT 1
            "#,
            token_hash,
            now
        )
        .fetch_optional(&self.pool)
        .await
    }

    pub(crate) async fn update_api_token_last_used_at(
        &self,
        id: i64,
        now: i64,
        stale_before: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE api_tokens
            SET last_used_at = ?
            WHERE id = ?
              AND revoked_at IS NULL
              AND (
                  last_used_at IS NULL
                  OR last_used_at <= ?
              )
            "#,
            now,
            id,
            stale_before
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn revoke_api_token(
        &self,
        id: i64,
        revoked_at: i64,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            UPDATE api_tokens
            SET revoked_at = COALESCE(revoked_at, ?)
            WHERE id = ?
            RETURNING id
            "#,
            revoked_at,
            id
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(result.is_some())
    }
}
