//! SQLite setup (plan section 25.1). The server is the only writer.

use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;

pub async fn connect(database_url: &str, data_dir: &Path) -> anyhow::Result<SqlitePool> {
    std::fs::create_dir_all(data_dir)?;

    let options = SqliteConnectOptions::from_str(database_url)?
        .create_if_missing(true)
        // Plan section 25.1: WAL, foreign keys and a busy timeout, so a single
        // writer coexists with concurrent readers.
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));

    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(options)
        .await?;

    sqlx::migrate!("../../migrations/sqlite").run(&pool).await?;

    Ok(pool)
}
