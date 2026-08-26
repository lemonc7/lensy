use chrono::Utc;

use crate::backend::{
    error::ServiceError,
    model::{ApiToken, CreatedApiToken, TokenSecret},
    service::Service,
};

const LAST_USED_UPDATE_INTERVAL_SECONDS: i64 = 5 * 60;

impl Service {
    pub async fn create_api_token(
        &self,
        name: impl Into<String>,
        expires_at: Option<i64>,
    ) -> Result<CreatedApiToken, ServiceError> {
        let name = name.into();

        let valid_name = !name.is_empty() && name.trim() == name && name.chars().count() <= 100;

        if !valid_name {
            return Err(ServiceError::InvalidApiTokenName);
        }

        let now = Utc::now().timestamp();

        if expires_at.is_some_and(|t| t <= now) {
            return Err(ServiceError::InvalidApiTokenExpiration);
        }

        let secret = TokenSecret::generate()?;
        let token_hash = secret.hash();

        let stored = self
            .repository
            .create_api_token(&name, secret.prefix(), &token_hash, now, expires_at)
            .await?;

        Ok(CreatedApiToken {
            api_token: stored.into(),
            secret,
        })
    }

    pub async fn list_api_tokens(&self) -> Result<Vec<ApiToken>, ServiceError> {
        let tokens = self.repository.list_api_tokens().await?;

        Ok(tokens.into_iter().map(Into::into).collect())
    }

    pub async fn authenticate_api_token(&self, plaintext: &str) -> Result<ApiToken, ServiceError> {
        // 格式错误，token不存在，过期和撤销统一返回相同错误，避免向请求方暴露token状态
        let secret = TokenSecret::parse(plaintext).map_err(|_| ServiceError::InvalidApiToken)?;

        let token_hash = secret.hash();
        let now = Utc::now().timestamp();

        let stored = self
            .repository
            .find_usable_api_token_by_hash(&token_hash, now)
            .await?
            .ok_or(ServiceError::InvalidApiToken)?;

        // 不需要每个请求都写sqlite，最多每五分钟更新一次
        let stale_before = now.saturating_sub(LAST_USED_UPDATE_INTERVAL_SECONDS);

        // last_used_at 仅用于管理展示；短暂的SQLite写锁竞争不应拒绝有效凭据。
        let _ = self
            .repository
            .update_api_token_last_used_at(stored.id, now, stale_before)
            .await;

        Ok(stored.into())
    }

    pub async fn revoke_api_token(&self, id: i64) -> Result<(), ServiceError> {
        let revoked = self
            .repository
            .revoke_api_token(id, Utc::now().timestamp())
            .await?;

        if !revoked {
            return Err(ServiceError::ApiTokenNotFound);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{error::Error, path::Path};

    use chrono::Utc;
    use chrono_tz::Asia::Shanghai;
    use sqlx::SqlitePool;
    use tempfile::tempdir;

    use crate::backend::{
        config::ImageConfig, db::Repository, error::ServiceError, image::processor::ImageProcessor,
        service::Service, storage::Storage,
    };

    #[sqlx::test]
    async fn creates_and_lists_api_tokens(pool: SqlitePool) -> Result<(), Box<dyn Error>> {
        let data_dir = tempdir()?;
        let service = test_service(pool.clone(), data_dir.path())?;

        let created = service
            .create_api_token("deployment", Some(Utc::now().timestamp() + 3600))
            .await?;

        assert_eq!(created.api_token.name, "deployment");
        assert_eq!(created.api_token.token_prefix.len(), 12);
        assert!(created.api_token.token_prefix.starts_with("lensy_"));
        assert!(created.secret.expose_secret().starts_with("lensy_"));

        let stored_hash = sqlx::query_scalar!(
            "SELECT token_hash FROM api_tokens WHERE id = ?",
            created.api_token.id,
        )
        .fetch_one(&pool)
        .await?;

        assert_eq!(stored_hash.len(), 64);
        assert_ne!(stored_hash, created.secret.expose_secret());

        let listed = service.list_api_tokens().await?;

        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, created.api_token.id);
        assert_eq!(listed[0].token_prefix, created.api_token.token_prefix);

        Ok(())
    }

    #[sqlx::test]
    async fn authenticates_and_updates_last_used_at(
        pool: SqlitePool,
    ) -> Result<(), Box<dyn Error>> {
        let data_dir = tempdir()?;
        let service = test_service(pool.clone(), data_dir.path())?;

        let created = service.create_api_token("test", None).await?;

        let authenticated = service
            .authenticate_api_token(created.secret.expose_secret())
            .await?;

        assert_eq!(authenticated.id, created.api_token.id);

        let last_used_at = sqlx::query_scalar!(
            "SELECT last_used_at FROM api_tokens WHERE id = ?",
            created.api_token.id,
        )
        .fetch_one(&pool)
        .await?;

        assert!(last_used_at.is_some());

        Ok(())
    }

    #[sqlx::test]
    async fn authentication_succeeds_when_last_used_update_fails(
        pool: SqlitePool,
    ) -> Result<(), Box<dyn Error>> {
        let data_dir = tempdir()?;
        let service = test_service(pool.clone(), data_dir.path())?;
        let created = service.create_api_token("test", None).await?;

        sqlx::query(
            r#"
            CREATE TRIGGER reject_last_used_update
            BEFORE UPDATE OF last_used_at ON api_tokens
            BEGIN
                SELECT RAISE(ABORT, 'simulated audit update failure');
            END
            "#,
        )
        .execute(&pool)
        .await?;

        let authenticated = service
            .authenticate_api_token(created.secret.expose_secret())
            .await?;

        assert_eq!(authenticated.id, created.api_token.id);
        Ok(())
    }

    #[sqlx::test]
    async fn rejects_revoked_api_token(pool: SqlitePool) -> Result<(), Box<dyn Error>> {
        let data_dir = tempdir()?;
        let service = test_service(pool, data_dir.path())?;

        let created = service.create_api_token("test", None).await?;

        service.revoke_api_token(created.api_token.id).await?;

        let result = service
            .authenticate_api_token(created.secret.expose_secret())
            .await;

        assert!(matches!(result, Err(ServiceError::InvalidApiToken),));

        // 撤销操作是幂等的。
        service.revoke_api_token(created.api_token.id).await?;

        Ok(())
    }

    #[sqlx::test]
    async fn rejects_expired_and_malformed_tokens(pool: SqlitePool) -> Result<(), Box<dyn Error>> {
        let data_dir = tempdir()?;
        let service = test_service(pool.clone(), data_dir.path())?;

        assert!(matches!(
            service
                .create_api_token("expired", Some(Utc::now().timestamp()),)
                .await,
            Err(ServiceError::InvalidApiTokenExpiration),
        ));

        assert!(matches!(
            service.authenticate_api_token("invalid").await,
            Err(ServiceError::InvalidApiToken),
        ));

        let created = service.create_api_token("test", None).await?;

        let now = Utc::now().timestamp();
        let created_at = now - 10;
        let expires_at = now - 1;

        sqlx::query!(
            r#"
            UPDATE api_tokens
            SET created_at = ?1,
                expires_at = ?2
            WHERE id = ?3
            "#,
            created_at,
            expires_at,
            created.api_token.id,
        )
        .execute(&pool)
        .await?;

        assert!(matches!(
            service
                .authenticate_api_token(created.secret.expose_secret(),)
                .await,
            Err(ServiceError::InvalidApiToken),
        ));

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
}
