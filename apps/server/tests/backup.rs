//! Backing up, and the property that makes a backup worth having: the copy
//! holds what the live server holds, including writes that have not left the
//! write-ahead log yet.

use std::path::{Path, PathBuf};

use forma_server::{backup, Config};
use design_core::AccountId;
use persistence::{
    AssetRepository, ProjectRepository, SqliteAssetRepository, SqliteProjectRepository,
};
use sqlx::SqlitePool;

fn config(data_dir: &Path) -> Config {
    Config {
        bind: "127.0.0.1:0".to_string(),
        database_url: format!("sqlite://{}", data_dir.join("forma.db").display()),
        data_dir: data_dir.to_path_buf(),
        font_dir: PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/fonts")),
        log: "error".to_string(),
        web_dir: PathBuf::from("./does-not-exist"),
        remote_token: None,
    }
}

/// A live server with a few projects and one uploaded file.
async fn live(data_dir: &Path) -> SqlitePool {
    let pool = persistence::connect(
        &format!("sqlite://{}", data_dir.join("forma.db").display()),
        data_dir,
    )
    .await
    .expect("migrations");

    let projects = SqliteProjectRepository::new(pool.clone());
    for name in ["One", "Two", "Three"] {
        projects.create(account(), name).await.unwrap();
    }
    pool
}

fn write_asset(data_dir: &Path, hash: &str, bytes: &[u8]) -> String {
    let key = persistence::asset_storage_key(hash, "png");
    let path = data_dir.join(&key);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, bytes).unwrap();
    key
}

async fn count_projects(db: &Path) -> i64 {
    let pool = persistence::connect(&format!("sqlite://{}", db.display()), db.parent().unwrap())
        .await
        .expect("the backup must open as a database");

    let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM projects")
        .fetch_one(&pool)
        .await
        .unwrap();
    pool.close().await;
    count
}

/// The whole point: a backup taken while the server is running holds the
/// writes, even the ones still sitting in the write-ahead log. A plain file
/// copy would miss them.
#[tokio::test]
async fn a_backup_of_a_live_database_holds_its_writes() {
    let dir = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let pool = live(dir.path()).await;

    // Still open, so the writes are in the log rather than the file.
    assert!(dir.path().join("forma.db-wal").exists());

    let summary = backup::run(&config(dir.path()), &out.path().join("backup"))
        .await
        .expect("backup");

    assert!(summary.database_bytes > 0);
    assert_eq!(count_projects(&summary.out.join("forma.db")).await, 3);

    pool.close().await;
}

#[tokio::test]
async fn assets_travel_with_the_database() {
    let dir = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let pool = live(dir.path()).await;

    let key = write_asset(dir.path(), "abcdef0123456789", b"the original bytes");

    let summary = backup::run(&config(dir.path()), &out.path().join("backup"))
        .await
        .unwrap();

    assert_eq!(summary.asset_files, 1);
    assert_eq!(summary.asset_bytes, 18);
    assert_eq!(
        std::fs::read(summary.out.join(&key)).unwrap(),
        b"the original bytes",
        "the copied asset differs from the original"
    );

    pool.close().await;
}

/// Derived data is left out on purpose, and the manifest says so rather than
/// leaving a restorer wondering what happened to it.
#[tokio::test]
async fn derived_data_is_left_out_and_said_so() {
    let dir = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let pool = live(dir.path()).await;

    for derived in ["previews/design/1", "exports/some-export"] {
        let path = dir.path().join(derived);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("file.png"), b"derived").unwrap();
    }

    let summary = backup::run(&config(dir.path()), &out.path().join("backup"))
        .await
        .unwrap();

    assert!(!summary.out.join("previews").exists());
    assert!(!summary.out.join("exports").exists());

    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(summary.out.join("manifest.json")).unwrap()).unwrap();
    let excluded = manifest["excluded"].as_array().unwrap();
    assert!(excluded
        .iter()
        .any(|e| e.as_str().unwrap().contains("previews")));
    assert!(excluded
        .iter()
        .any(|e| e.as_str().unwrap().contains("exports")));

    pool.close().await;
}

#[tokio::test]
async fn the_manifest_counts_what_was_copied() {
    let dir = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let pool = live(dir.path()).await;

    // One asset row, so the manifest can be checked against the database.
    let assets = SqliteAssetRepository::new(pool.clone());
    let projects = SqliteProjectRepository::new(pool.clone())
        .list(account())
        .await
        .unwrap();
    assets
        .create(account(), &design_core::Asset {
            id: design_core::AssetId::new(),
            project_id: projects[0].id,
            hash: "abcdef0123456789".to_string(),
            kind: design_core::AssetKind::Raster,
            mime_type: "image/png".to_string(),
            width: 10,
            height: 10,
            byte_size: 18,
            storage_key: write_asset(dir.path(), "abcdef0123456789", b"the original bytes"),
            original_filename: Some("hero.png".to_string()),
            origin: design_core::AssetOrigin::UserUpload,
            attribution: None,
            created_at: "2026-08-28T00:00:00.000Z".to_string(),
        })
        .await
        .unwrap();

    let summary = backup::run(&config(dir.path()), &out.path().join("backup"))
        .await
        .unwrap();

    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(summary.out.join("manifest.json")).unwrap()).unwrap();

    assert_eq!(manifest["projects"], 3);
    assert_eq!(manifest["assets"], 1);
    assert_eq!(manifest["asset_files"], 1);
    assert_eq!(
        manifest["document_schema_version"],
        design_core::SCHEMA_VERSION
    );

    pool.close().await;
}

/// Writing into a directory that already holds something would restore as a
/// mixture of two backups, which is worse than no backup.
#[tokio::test]
async fn it_refuses_to_write_over_an_existing_backup() {
    let dir = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let pool = live(dir.path()).await;

    let destination = out.path().join("backup");
    backup::run(&config(dir.path()), &destination)
        .await
        .unwrap();

    let error = backup::run(&config(dir.path()), &destination)
        .await
        .expect_err("a second backup into the same directory must be refused");
    assert!(error.to_string().contains("already exists"), "{error}");

    pool.close().await;
}

/// The account the migration creates; these tests are the local case.
fn account() -> AccountId {
    AccountId(uuid::Uuid::parse_str(design_core::BOOTSTRAP_ACCOUNT).expect("a constant uuid"))
}
