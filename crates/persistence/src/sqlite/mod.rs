//! SQLite adapters (plan section 25.1). The server is the only writer.

pub mod asset;
pub mod asset_request;
pub mod design;
pub mod export;
pub mod project;

use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use design_core::DesignDocument;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::error::{PersistenceError, Result};

pub use asset::SqliteAssetRepository;
pub use asset_request::SqliteAssetRequestRepository;
pub use design::SqliteDesignRepository;
pub use export::SqliteExportRepository;
pub use project::SqliteProjectRepository;

/// Opens the pool and applies migrations.
pub async fn connect(database_url: &str, data_dir: &Path) -> Result<SqlitePool> {
    std::fs::create_dir_all(data_dir)
        .map_err(|e| PersistenceError::Storage(format!("{}: {e}", data_dir.display())))?;

    let options = SqliteConnectOptions::from_str(database_url)
        .map_err(|e| PersistenceError::Storage(e.to_string()))?
        .create_if_missing(true)
        // WAL, foreign keys and a busy timeout, so one writer coexists with
        // concurrent readers (plan section 25.1).
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));

    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(options)
        .await?;

    migrate(&pool).await?;
    Ok(pool)
}

pub async fn migrate(pool: &SqlitePool) -> Result<()> {
    sqlx::migrate!("../../migrations/sqlite")
        .run(pool)
        .await
        .map_err(|e| PersistenceError::Storage(e.to_string()))
}

/// RFC 3339 in UTC. Timestamps are strings so the domain stays free of a clock.
pub(crate) fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub(crate) fn parse_uuid(raw: impl AsRef<str>) -> Result<Uuid> {
    Uuid::parse_str(raw.as_ref())
        .map_err(|e| PersistenceError::Storage(format!("stored id is not a UUID: {e}")))
}

/// Parsing goes through the migration entry point, so a stored document written
/// by an older schema is upgraded rather than rejected (plan section 76).
pub(crate) fn parse_document(json: impl AsRef<str>) -> Result<DesignDocument> {
    let value: serde_json::Value = serde_json::from_str(json.as_ref())
        .map_err(|e| PersistenceError::CorruptDocument(e.to_string()))?;
    DesignDocument::migrate(value).map_err(|e| PersistenceError::CorruptDocument(e.to_string()))
}
