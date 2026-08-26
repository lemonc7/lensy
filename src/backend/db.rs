pub mod api_token;
pub mod image;

use std::{fs, path::Path, str::FromStr, time::Duration};

use sqlx::{
    SqlitePool,
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};

use crate::backend::error::DatabaseError;

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

pub struct Repository {
    pool: SqlitePool,
}

impl Repository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

pub async fn connect(url: &str) -> Result<SqlitePool, DatabaseError> {
    if let Some(path) = url.strip_prefix("sqlite://")
        && let Some(parent) = Path::new(path).parent()
    {
        fs::create_dir_all(parent)?;
    }
    let options = SqliteConnectOptions::from_str(url)?
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(5));

    let pool = SqlitePoolOptions::new()
        .min_connections(1)
        .max_connections(8)
        .acquire_timeout(Duration::from_secs(10))
        .connect_with(options)
        .await?;

    MIGRATOR.run(&pool).await?;
    Ok(pool)
}
