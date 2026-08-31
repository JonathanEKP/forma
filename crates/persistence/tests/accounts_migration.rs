//! The upgrade path of plan section 113.2, against a database that already holds
//! work — which is the only version of this that is worth testing.
//!
//! A rebuild of `projects` with foreign keys enabled deletes every row that
//! cascades from it. That is not a hypothetical: the naive form of this
//! migration was measured against a seeded database and came back with zero
//! designs and `PRAGMA foreign_key_check` reporting nothing wrong. This test is
//! that measurement, kept, so the migration cannot quietly regain the bug.

use std::path::Path;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;

const BOOTSTRAP: &str = "00000000-0000-4000-8000-000000000001";

/// Everything before accounts existed, applied by hand.
///
/// The migrator would apply 0004 along with the rest, which tests the resulting
/// schema and not the upgrade. Building the old state explicitly is the only way
/// to run the dangerous statements against rows that were already there.
async fn pre_accounts(dir: &Path) -> SqlitePool {
    let options = SqliteConnectOptions::from_str(&format!("sqlite://{}/forma.db", dir.display()))
        .expect("url")
        .create_if_missing(true)
        // Exactly what `connect` sets. Without this the test proves nothing:
        // the whole failure depends on cascades being live.
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("pool");

    for sql in [
        include_str!("../../../migrations/sqlite/0001_init.sql"),
        include_str!("../../../migrations/sqlite/0002_designs_assets_exports.sql"),
        include_str!("../../../migrations/sqlite/0003_templates.sql"),
    ] {
        sqlx::raw_sql(sql).execute(&pool).await.expect("old schema");
    }

    pool
}

async fn seed(pool: &SqlitePool) {
    for sql in [
        "INSERT INTO projects (id, name, brand_id, created_at, updated_at)
         VALUES ('p1', 'Editorial', NULL, 't', 't')",
        "INSERT INTO designs (id, project_id, name, schema_version, version, document_json, created_at, updated_at)
         VALUES ('d1', 'p1', 'Cover', 1, 3, '{}', 't', 't')",
        "INSERT INTO design_versions (id, design_id, version, document_json, actor_type, actor_id, created_at)
         VALUES ('v1', 'd1', 3, '{}', 'human', NULL, 't')",
        "INSERT INTO assets (id, project_id, hash, kind, mime_type, width, height, byte_size,
                             storage_key, original_filename, origin, attribution, created_at)
         VALUES ('a1', 'p1', 'h', 'image', 'image/png', 10, 10, 100, 'k', NULL, 'user_upload', NULL, 't')",
        "INSERT INTO templates (id, project_id, name, description, document_json, roles_json, created_at)
         VALUES ('t1', 'p1', 'Cover', '', '{}', '[]', 't')",
        "INSERT INTO brand_kits (id, project_id, name, brand_json, version, created_at, updated_at)
         VALUES ('b1', 'p1', 'Default', '{}', 1, 't', 't')",
    ] {
        sqlx::raw_sql(sql).execute(pool).await.expect("seed");
    }
}

async fn count(pool: &SqlitePool, table: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
        .fetch_one(pool)
        .await
        .expect("count")
}

#[tokio::test]
async fn the_rebuild_keeps_every_row_that_cascades_from_projects() {
    let dir = tempfile::tempdir().expect("temp dir");
    let pool = pre_accounts(dir.path()).await;
    seed(&pool).await;

    sqlx::raw_sql(include_str!("../../../migrations/sqlite/0004_accounts.sql"))
        .execute(&pool)
        .await
        .expect("the accounts migration");

    // The whole point. Each of these cascades from `projects`, and the naive
    // rebuild took all of them.
    for table in [
        "projects",
        "designs",
        "design_versions",
        "assets",
        "templates",
        "brand_kits",
    ] {
        assert_eq!(
            count(&pool, table).await,
            1,
            "{table} lost its row to the projects rebuild"
        );
    }
}

#[tokio::test]
async fn existing_projects_join_the_bootstrap_account() {
    let dir = tempfile::tempdir().expect("temp dir");
    let pool = pre_accounts(dir.path()).await;
    seed(&pool).await;

    sqlx::raw_sql(include_str!("../../../migrations/sqlite/0004_accounts.sql"))
        .execute(&pool)
        .await
        .expect("the accounts migration");

    let owner: String = sqlx::query_scalar("SELECT account_id FROM projects WHERE id = 'p1'")
        .fetch_one(&pool)
        .await
        .expect("the backfilled owner");
    assert_eq!(owner, BOOTSTRAP);

    // Nobody has claimed it: it must not belong to whoever signs up first.
    let claimed: Option<String> =
        sqlx::query_scalar("SELECT claimed_at FROM accounts WHERE bootstrap = 1")
            .fetch_one(&pool)
            .await
            .expect("the bootstrap account");
    assert!(claimed.is_none(), "the legacy account starts unclaimed");
    assert!(
        count(&pool, "account_memberships").await == 0,
        "and with no membership, so it cannot be walked into"
    );
}

/// The column is `NOT NULL` with no default on purpose: an INSERT that forgets
/// the scope must fail rather than be guessed for.
#[tokio::test]
async fn a_project_without_an_account_is_refused() {
    let dir = tempfile::tempdir().expect("temp dir");
    let pool = pre_accounts(dir.path()).await;

    sqlx::raw_sql(include_str!("../../../migrations/sqlite/0004_accounts.sql"))
        .execute(&pool)
        .await
        .expect("the accounts migration");

    let refused = sqlx::raw_sql(
        "INSERT INTO projects (id, name, brand_id, created_at, updated_at)
         VALUES ('p2', 'No owner', NULL, 't', 't')",
    )
    .execute(&pool)
    .await;

    assert!(refused.is_err(), "a project must name its account");
}

/// The schema, not a caller's memory, is what keeps the bootstrap account single.
#[tokio::test]
async fn there_can_only_be_one_bootstrap_account() {
    let dir = tempfile::tempdir().expect("temp dir");
    let pool = pre_accounts(dir.path()).await;

    sqlx::raw_sql(include_str!("../../../migrations/sqlite/0004_accounts.sql"))
        .execute(&pool)
        .await
        .expect("the accounts migration");

    let second = sqlx::raw_sql(
        "INSERT INTO accounts (id, name, bootstrap, claimed_at, created_at, updated_at)
         VALUES ('00000000-0000-4000-8000-000000000002', 'Another', 1, NULL, 't', 't')",
    )
    .execute(&pool)
    .await;

    assert!(
        second.is_err(),
        "a second bootstrap account must be refused"
    );
}

/// Foreign keys must be back on when the migration finishes, or the connection
/// returns to the pool enforcing nothing.
#[tokio::test]
async fn foreign_keys_are_on_again_afterwards() {
    let dir = tempfile::tempdir().expect("temp dir");
    let pool = pre_accounts(dir.path()).await;

    sqlx::raw_sql(include_str!("../../../migrations/sqlite/0004_accounts.sql"))
        .execute(&pool)
        .await
        .expect("the accounts migration");

    let on: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(&pool)
        .await
        .expect("the pragma");
    assert_eq!(on, 1, "the migration must not leave cascades disabled");

    let orphan = sqlx::raw_sql(
        "INSERT INTO designs (id, project_id, name, schema_version, version, document_json, created_at, updated_at)
         VALUES ('d9', 'nope', 'Orphan', 1, 1, '{}', 't', 't')",
    )
    .execute(&pool)
    .await;
    assert!(orphan.is_err(), "and references must be enforced again");
}
