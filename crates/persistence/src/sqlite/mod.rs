//! SQLite adapters (plan section 25.1). The server is the only writer.

pub mod account;
pub mod api_key;
pub mod asset;
pub mod asset_request;
pub mod design;
pub mod export;
pub mod invocation_ledger;
pub mod project;
mod secret;
pub mod service_credential;
pub mod template;

use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use design_core::DesignDocument;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::error::{PersistenceError, Result};

pub use account::SqliteAccountRepository;
pub use api_key::{SqliteApiKeyRepository, PAT_PREFIX};
pub use asset::SqliteAssetRepository;
pub use asset_request::SqliteAssetRequestRepository;
pub use design::SqliteDesignRepository;
pub use export::SqliteExportRepository;
pub use invocation_ledger::SqliteInvocationLedger;
pub use project::SqliteProjectRepository;
pub use service_credential::{SqliteServiceCredentialRepository, SERVICE_PREFIX};
pub use template::SqliteTemplateRepository;

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

/// Folds the write-ahead log back into the database file, then closes the pool.
///
/// Closing on its own *usually* checkpoints — SQLite does it when the last
/// connection goes — but usually is not something you can copy a file on.
/// `close()` returns once sqlx has handed the connections to its workers, and
/// the final `sqlite3_close` can land after that, so a copy taken immediately
/// afterwards sometimes gets a database file with none of the writes in it.
/// That was a test failing roughly one run in fifteen, and it would equally
/// have been a backup silently missing everything.
///
/// `wal_checkpoint(TRUNCATE)` is ordinary SQL: it finishes before it returns,
/// and leaves the log empty. A checkpoint that cannot run is logged rather than
/// fatal — the data is still durable in the log, and the next open replays it.
pub async fn checkpoint_and_close(pool: &SqlitePool) -> Result<()> {
    if let Err(error) = sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(pool)
        .await
    {
        tracing::warn!(%error, "could not checkpoint the write-ahead log before closing");
    }

    pool.close().await;
    Ok(())
}

/// The projects one account owns.
///
/// Ownership is recorded on `projects` and nowhere else (plan section 107.1), so
/// every scoped query reaches it through this subquery rather than through an
/// `account_id` copied onto each table. The copy would be a second thing that
/// can disagree with the first, and disagreement about who owns a row is the
/// failure the account boundary exists to prevent.
///
/// It binds one parameter, at the position where the fragment appears in the
/// statement.
pub(crate) const OWNED_PROJECTS: &str = "(SELECT id FROM projects WHERE account_id = ?)";

/// The designs one account owns, one hop further out.
///
/// For rows that hang off a design rather than a project — exports, version
/// snapshots — whose account is therefore two joins away.
pub(crate) const OWNED_DESIGNS: &str = "(SELECT d.id FROM designs d \
     JOIN projects p ON p.id = d.project_id WHERE p.account_id = ?)";

/// Refuses unless this account owns the project.
///
/// For writes that name a project in the row being inserted: guarding the INSERT
/// itself would write nothing and report success, which is the worst of both.
pub(crate) async fn owned_project(
    pool: &SqlitePool,
    account: design_core::AccountId,
    project_id: design_core::ProjectId,
) -> Result<()> {
    let found: Option<String> =
        sqlx::query_scalar("SELECT id FROM projects WHERE id = ? AND account_id = ?")
            .bind(project_id.to_string())
            .bind(account.to_string())
            .fetch_optional(pool)
            .await?;

    found
        .map(|_| ())
        .ok_or(PersistenceError::ProjectNotFound(project_id))
}

/// The same, for a row that hangs off a design.
pub(crate) async fn owned_design(
    pool: &SqlitePool,
    account: design_core::AccountId,
    design_id: design_core::DesignId,
) -> Result<()> {
    let found: Option<String> = sqlx::query_scalar(
        "SELECT d.id FROM designs d JOIN projects p ON p.id = d.project_id
         WHERE d.id = ? AND p.account_id = ?",
    )
    .bind(design_id.to_string())
    .bind(account.to_string())
    .fetch_optional(pool)
    .await?;

    found
        .map(|_| ())
        .ok_or(PersistenceError::DesignNotFound(design_id))
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
