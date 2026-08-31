//! Phase 4 acceptance (plan section 77) and repository behaviour, against a real
//! temporary SQLite rather than a mock.

use design_core::{
    AccountId, Actor, ActorType, Asset, AssetKind, AssetOrigin, AssetRequest, AssetRequestId,
    AssetRequestStatus, BrandColors, BrandId, BrandKit, BrandRules, BrandTypography, Color,
    DesignColor, DesignDocument, DesignFormat, DesignId, Export, ExportId, ExportScope,
    FormatPreset, Page, Project, ProjectId, Version,
};
use persistence::{
    AssetRepository, AssetRequestRepository, DesignRepository, ExportRepository, PersistenceError,
    ProjectRepository, SqliteAssetRepository, SqliteAssetRequestRepository, SqliteDesignRepository,
    SqliteExportRepository, SqliteProjectRepository,
};
use sqlx::SqlitePool;

/// The account the migration creates. Every project in these tests belongs to
/// it, which is the local case of plan 117.8.
fn account() -> AccountId {
    AccountId(uuid::Uuid::parse_str(design_core::BOOTSTRAP_ACCOUNT).expect("a constant uuid"))
}

struct Harness {
    _dir: tempfile::TempDir,
    pool: SqlitePool,
}

impl Harness {
    async fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("forma.db");
        let pool = persistence::connect(&format!("sqlite://{}", db.display()), dir.path())
            .await
            .expect("migrations must apply");
        Self { _dir: dir, pool }
    }

    fn projects(&self) -> SqliteProjectRepository {
        SqliteProjectRepository::new(self.pool.clone())
    }

    fn designs(&self) -> SqliteDesignRepository {
        SqliteDesignRepository::new(self.pool.clone())
    }

    fn assets(&self) -> SqliteAssetRepository {
        SqliteAssetRepository::new(self.pool.clone())
    }

    fn asset_requests(&self) -> SqliteAssetRequestRepository {
        SqliteAssetRequestRepository::new(self.pool.clone())
    }

    fn exports(&self) -> SqliteExportRepository {
        SqliteExportRepository::new(self.pool.clone())
    }

    async fn project(&self) -> Project {
        self.projects()
            .create(account(), "Test project")
            .await
            .unwrap()
    }
}

fn document(name: &str) -> DesignDocument {
    let mut document = DesignDocument::new(DesignFormat::from_preset(FormatPreset::Square));
    let mut page = Page::new(name);
    page.background = Some(DesignColor::token("brand.background"));
    document.pages.push(page);
    document
}

// --- Acceptance -----------------------------------------------------------

/// Plan section 77, Phase 4: two mutations sharing a base_version. The first
/// wins; the second must be told the design moved on.
#[tokio::test]
async fn the_second_write_on_the_same_base_version_conflicts() {
    let harness = Harness::start().await;
    let project = harness.project().await;
    let designs = harness.designs();

    let created = designs
        .create(account(), project.id, "Carousel", &document("Cover"), &Actor::human())
        .await
        .unwrap();
    assert_eq!(created.version, Version::INITIAL);

    // Both callers read the same version, as a human editor and an agent would.
    let base = created.version;

    let first = designs
        .save(
            account(),            created.id,
            base,
            &document("Edited by the human"),
            &Actor::human(),
        )
        .await
        .expect("the first write must win");
    assert_eq!(first.version, base.next());

    let error = designs
        .save(
            account(),            created.id,
            base,
            &document("Edited by the agent"),
            &Actor::agent("agent:claude"),
        )
        .await
        .expect_err("the second write must be rejected");

    assert_eq!(error.code(), "VERSION_CONFLICT");
    let details = error.details().unwrap();
    assert_eq!(details["expected"], 1);
    assert_eq!(details["actual"], 2);

    // And the losing write left nothing behind.
    let stored = designs.get(account(), created.id).await.unwrap();
    assert_eq!(stored.version, Version(2));
    assert_eq!(stored.document.pages[0].name, "Edited by the human");
}

/// Rebasing is what the conflict is telling the caller to do, so it must work.
#[tokio::test]
async fn the_loser_succeeds_after_refetching() {
    let harness = Harness::start().await;
    let project = harness.project().await;
    let designs = harness.designs();

    let created = designs
        .create(account(), project.id, "Carousel", &document("v1"), &Actor::human())
        .await
        .unwrap();

    designs
        .save(
            account(),            created.id,
            created.version,
            &document("v2"),
            &Actor::human(),
        )
        .await
        .unwrap();

    let current = designs.get(account(), created.id).await.unwrap();
    let rebased = designs
        .save(
            account(),            created.id,
            current.version,
            &document("v3"),
            &Actor::agent("agent:codex"),
        )
        .await
        .expect("a rebased write must succeed");

    assert_eq!(rebased.version, Version(3));
    assert_eq!(rebased.document.pages[0].name, "v3");
}

// --- Design snapshots and history ----------------------------------------

#[tokio::test]
async fn every_version_keeps_a_readable_snapshot() {
    let harness = Harness::start().await;
    let project = harness.project().await;
    let designs = harness.designs();

    let created = designs
        .create(account(), project.id, "Carousel", &document("first"), &Actor::human())
        .await
        .unwrap();
    designs
        .save(account(), created.id, Version(1), &document("second"), &Actor::human())
        .await
        .unwrap();
    designs
        .save(account(), created.id, Version(2), &document("third"), &Actor::human())
        .await
        .unwrap();

    // A version never changes once written, which is what makes undo and the
    // render cache safe (plan sections 29 and 72).
    assert_eq!(
        designs
            .snapshot(account(), created.id, Version(1))
            .await
            .unwrap()
            .pages[0]
            .name,
        "first"
    );
    assert_eq!(
        designs
            .snapshot(account(), created.id, Version(2))
            .await
            .unwrap()
            .pages[0]
            .name,
        "second"
    );
}

/// Plan section 29: undo restores an old snapshot as a NEW version. History only
/// ever moves forward, so optimistic concurrency is never broken.
#[tokio::test]
async fn undo_moves_history_forward_rather_than_backward() {
    let harness = Harness::start().await;
    let project = harness.project().await;
    let designs = harness.designs();

    let created = designs
        .create(
            account(),            project.id,
            "Carousel",
            &document("original"),
            &Actor::human(),
        )
        .await
        .unwrap();
    designs
        .save(
            account(),            created.id,
            Version(1),
            &document("mistake"),
            &Actor::human(),
        )
        .await
        .unwrap();

    let restored = designs.snapshot(account(), created.id, Version(1)).await.unwrap();
    let undone = designs
        .save(account(), created.id, Version(2), &restored, &Actor::system())
        .await
        .unwrap();

    assert_eq!(undone.version, Version(3), "the version must not decrement");
    assert_eq!(undone.document.pages[0].name, "original");
    assert_eq!(designs.history(account(), created.id).await.unwrap().len(), 3);
}

#[tokio::test]
async fn history_records_who_made_each_version() {
    let harness = Harness::start().await;
    let project = harness.project().await;
    let designs = harness.designs();

    let created = designs
        .create(account(), project.id, "Carousel", &document("v1"), &Actor::human())
        .await
        .unwrap();
    designs
        .save(
            account(),            created.id,
            Version(1),
            &document("v2"),
            &Actor::agent("agent:claude"),
        )
        .await
        .unwrap();

    let history = designs.history(account(), created.id).await.unwrap();
    assert_eq!(history.len(), 2);
    // Newest first.
    assert_eq!(history[0].version, Version(2));
    assert_eq!(history[0].actor_type, ActorType::Agent);
    assert_eq!(history[0].actor_id.as_deref(), Some("agent:claude"));
    assert_eq!(history[1].actor_type, ActorType::Human);
    assert_eq!(history[1].actor_id, None);
}

#[tokio::test]
async fn a_missing_design_is_not_a_conflict() {
    let harness = Harness::start().await;
    let designs = harness.designs();

    let error = designs
        .save(account(), DesignId::new(), Version(1), &document("x"), &Actor::human())
        .await
        .unwrap_err();

    // Distinguishing these matters: one says refetch, the other says give up.
    assert_eq!(error.code(), "DESIGN_NOT_FOUND");
}

#[tokio::test]
async fn a_design_cannot_be_created_in_a_project_that_does_not_exist() {
    let harness = Harness::start().await;
    let error = harness
        .designs()
        .create(account(), ProjectId::new(), "Orphan", &document("x"), &Actor::human())
        .await
        .unwrap_err();

    assert_eq!(error.code(), "PROJECT_NOT_FOUND");
}

#[tokio::test]
async fn a_structurally_invalid_document_is_never_persisted() {
    let harness = Harness::start().await;
    let project = harness.project().await;
    let designs = harness.designs();

    let mut empty = DesignDocument::new(DesignFormat::from_preset(FormatPreset::Square));
    empty.pages.clear();

    let error = designs
        .create(account(), project.id, "Broken", &empty, &Actor::human())
        .await
        .unwrap_err();
    assert_eq!(error.code(), "INVALID_DOCUMENT");
    assert!(designs
        .list_for_project(account(), project.id)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn deleting_a_design_takes_its_versions_with_it() {
    let harness = Harness::start().await;
    let project = harness.project().await;
    let designs = harness.designs();

    let created = designs
        .create(account(), project.id, "Doomed", &document("v1"), &Actor::human())
        .await
        .unwrap();
    designs.delete(account(), created.id).await.unwrap();

    assert!(matches!(
        designs.get(account(), created.id).await,
        Err(PersistenceError::DesignNotFound(_))
    ));
    assert!(designs.history(account(), created.id).await.unwrap().is_empty());
}

// --- Projects and brand ---------------------------------------------------

fn brand_kit(project_id: ProjectId) -> BrandKit {
    BrandKit {
        id: BrandId::new(),
        project_id,
        name: "Default".to_string(),
        colors: BrandColors {
            primary: Color::rgb(0xFF, 0x54, 0x36),
            secondary: Color::rgb(0xFD, 0xBA, 0x2D),
            accent: Color::rgb(0x7C, 0x3A, 0xED),
            background: Color::rgb(0x10, 0x10, 0x10),
            surface: Color::rgb(0x19, 0x19, 0x19),
            text: Color::rgb(0xFF, 0xFF, 0xFF),
            text_inverse: Color::rgb(0x10, 0x10, 0x10),
            muted: Color::rgb(0xA3, 0xA3, 0xA3),
        },
        typography: BrandTypography {
            display: "DejaVu Sans".to_string(),
            body: "DejaVu Sans".to_string(),
            mono: "DejaVu Sans".to_string(),
        },
        logo_asset_id: None,
        voice: "Direct and brief.".to_string(),
        rules: BrandRules::default(),
        version: Version::INITIAL,
    }
}

#[tokio::test]
async fn a_project_keeps_one_active_brand_kit() {
    let harness = Harness::start().await;
    let projects = harness.projects();
    let project = harness.project().await;

    assert!(projects.get_brand(account(), project.id).await.unwrap().is_none());

    let mut brand = brand_kit(project.id);
    projects.put_brand(account(), &brand).await.unwrap();

    // Saving again replaces it rather than accumulating kits.
    brand.name = "Rebrand".to_string();
    brand.colors.primary = Color::rgb(0x00, 0xFF, 0x00);
    projects.put_brand(account(), &brand).await.unwrap();

    let stored = projects.get_brand(account(), project.id).await.unwrap().unwrap();
    assert_eq!(stored.name, "Rebrand");
    assert_eq!(stored.colors.primary, Color::rgb(0x00, 0xFF, 0x00));
    assert_eq!(
        stored.color_palette().len(),
        design_core::COLOR_TOKENS.len()
    );

    // And the project points at it.
    assert_eq!(
        projects.get(account(), project.id).await.unwrap().brand_id,
        Some(brand.id)
    );
}

#[tokio::test]
async fn deleting_a_project_takes_its_designs_with_it() {
    let harness = Harness::start().await;
    let project = harness.project().await;
    let designs = harness.designs();

    let created = designs
        .create(account(), project.id, "Doomed", &document("v1"), &Actor::human())
        .await
        .unwrap();

    harness.projects().delete(account(), project.id).await.unwrap();
    assert!(designs.get(account(), created.id).await.is_err());
}

// --- Assets ---------------------------------------------------------------

fn asset(project_id: ProjectId, hash: &str) -> Asset {
    Asset {
        id: design_core::AssetId::new(),
        project_id,
        hash: hash.to_string(),
        kind: AssetKind::Raster,
        mime_type: "image/png".to_string(),
        width: 1600,
        height: 900,
        byte_size: 7753,
        storage_key: persistence::asset_storage_key(hash, "png"),
        original_filename: Some("hero.png".to_string()),
        origin: AssetOrigin::UserUpload,
        attribution: None,
        created_at: "2026-08-27T00:00:00.000Z".to_string(),
    }
}

#[tokio::test]
async fn assets_deduplicate_by_hash_within_a_project() {
    let harness = Harness::start().await;
    let project = harness.project().await;
    let assets = harness.assets();

    let first = assets
        .create(account(), &asset(project.id, "abcdef123456"))
        .await
        .unwrap();

    let found = assets
        .find_by_hash(account(), project.id, "abcdef123456")
        .await
        .unwrap()
        .expect("identical bytes should resolve to the existing asset");
    assert_eq!(found.id, first.id);

    assert!(assets
        .find_by_hash(account(), project.id, "0000000000")
        .await
        .unwrap()
        .is_none());
}

/// Plan section 74: deleting an asset must not orphan historical versions that
/// still reference it, so the row is only marked.
#[tokio::test]
async fn deleting_an_asset_is_a_soft_delete() {
    let harness = Harness::start().await;
    let project = harness.project().await;
    let assets = harness.assets();

    let created = assets
        .create(account(), &asset(project.id, "deadbeef01"))
        .await
        .unwrap();
    assets.soft_delete(account(), created.id).await.unwrap();

    assert!(assets.get(account(), created.id).await.is_err());
    assert!(assets
        .list_for_project(account(), project.id)
        .await
        .unwrap()
        .is_empty());

    // The hash is released too, so re-uploading the same bytes is a fresh asset
    // rather than resurrecting a deleted one.
    assert!(assets
        .find_by_hash(account(), project.id, "deadbeef01")
        .await
        .unwrap()
        .is_none());
}

// --- Asset requests -------------------------------------------------------

fn asset_request(project_id: ProjectId, design_id: DesignId) -> AssetRequest {
    AssetRequest {
        id: AssetRequestId::new(),
        project_id,
        design_id: Some(design_id),
        purpose: "hero photo".to_string(),
        description: "A futuristic coffee shop at night".to_string(),
        suggested_prompt: Some("wide shot, neon, rain".to_string()),
        requested_width: Some(1600),
        requested_height: Some(900),
        status: AssetRequestStatus::Pending,
        resolved_asset_id: None,
        created_at: "2026-08-27T00:00:00.000Z".to_string(),
        resolved_at: None,
    }
}

#[tokio::test]
async fn an_asset_request_goes_from_pending_to_resolved() {
    let harness = Harness::start().await;
    let project = harness.project().await;
    let design = harness
        .designs()
        .create(
            account(),            project.id,
            "Ad",
            &document("v1"),
            &Actor::agent("agent:claude"),
        )
        .await
        .unwrap();

    let requests = harness.asset_requests();
    let created = requests
        .create(account(), &asset_request(project.id, design.id))
        .await
        .unwrap();
    assert!(created.is_open());

    let uploaded = harness
        .assets()
        .create(account(), &asset(project.id, "cafe01"))
        .await
        .unwrap();

    let resolved = requests.resolve(account(), created.id, uploaded.id).await.unwrap();
    assert_eq!(resolved.status, AssetRequestStatus::Resolved);
    assert_eq!(resolved.resolved_asset_id, Some(uploaded.id));
    assert!(resolved.resolved_at.is_some());
    assert!(!resolved.is_open());
}

/// A second upload must not silently replace the asset the design already uses.
#[tokio::test]
async fn a_request_cannot_be_resolved_twice() {
    let harness = Harness::start().await;
    let project = harness.project().await;
    let design = harness
        .designs()
        .create(account(), project.id, "Ad", &document("v1"), &Actor::human())
        .await
        .unwrap();

    let requests = harness.asset_requests();
    let created = requests
        .create(account(), &asset_request(project.id, design.id))
        .await
        .unwrap();

    let first = harness
        .assets()
        .create(account(), &asset(project.id, "one"))
        .await
        .unwrap();
    let second = harness
        .assets()
        .create(account(), &asset(project.id, "two"))
        .await
        .unwrap();

    requests.resolve(account(), created.id, first.id).await.unwrap();
    assert!(requests.resolve(account(), created.id, second.id).await.is_err());

    assert_eq!(
        requests.get(account(), created.id).await.unwrap().resolved_asset_id,
        Some(first.id)
    );
}

#[tokio::test]
async fn a_cancelled_request_stays_cancelled() {
    let harness = Harness::start().await;
    let project = harness.project().await;
    let design = harness
        .designs()
        .create(account(), project.id, "Ad", &document("v1"), &Actor::human())
        .await
        .unwrap();

    let requests = harness.asset_requests();
    let created = requests
        .create(account(), &asset_request(project.id, design.id))
        .await
        .unwrap();

    let cancelled = requests.cancel(account(), created.id).await.unwrap();
    assert_eq!(cancelled.status, AssetRequestStatus::Cancelled);

    let uploaded = harness
        .assets()
        .create(account(), &asset(project.id, "late"))
        .await
        .unwrap();
    assert!(requests.resolve(account(), created.id, uploaded.id).await.is_err());
}

// --- Exports --------------------------------------------------------------

#[tokio::test]
async fn an_export_pins_the_version_it_came_from() {
    let harness = Harness::start().await;
    let project = harness.project().await;
    let designs = harness.designs();
    let design = designs
        .create(account(), project.id, "Carousel", &document("v1"), &Actor::human())
        .await
        .unwrap();

    let export = Export {
        id: ExportId::new(),
        design_id: design.id,
        design_version: design.version,
        scope: ExportScope::SinglePage,
        page_id: Some(design.document.pages[0].id),
        scale: 1.0,
        storage_key: "exports/x/01.png".to_string(),
        byte_size: 34507,
        // Ignored: the repository stamps its own, so a caller cannot backdate
        // a row or persist a blank one by forgetting the field.
        created_at: String::new(),
    };

    let stored = harness.exports().create(account(), &export).await.unwrap();
    assert!(
        stored.created_at.ends_with('Z'),
        "the repository must stamp the row: {:?}",
        stored.created_at
    );
    assert_eq!(
        stored,
        Export {
            created_at: stored.created_at.clone(),
            ..export.clone()
        }
    );

    // The design moves on; the export still records what it actually contains.
    designs
        .save(account(), design.id, design.version, &document("v2"), &Actor::human())
        .await
        .unwrap();

    let fetched = harness.exports().get(account(), export.id).await.unwrap();
    assert_eq!(fetched.design_version, Version(1));
    assert_eq!(
        harness
            .exports()
            .list_for_design(account(), design.id)
            .await
            .unwrap()
            .len(),
        1
    );
}

// --- shutdown -------------------------------------------------------------

/// The mechanism, checked while the pool is still open so the result cannot
/// depend on timing.
///
/// `wal_checkpoint(TRUNCATE)` is ordinary SQL and finishes before it returns,
/// so the log is empty the moment it does. Relying on `close()` to trigger a
/// checkpoint instead leaves the writes in the log until sqlx's workers get to
/// it, which is a race — and one that showed up as a test failing about one run
/// in ten, on CI first.
#[tokio::test]
async fn checkpointing_empties_the_log_synchronously() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("forma.db");
    let pool = persistence::connect(&format!("sqlite://{}", db.display()), dir.path())
        .await
        .unwrap();

    let projects = SqliteProjectRepository::new(pool.clone());
    for name in ["One", "Two", "Three"] {
        projects.create(account(), name).await.unwrap();
    }

    let wal = dir.path().join("forma.db-wal");
    let before = std::fs::metadata(&wal).map(|m| m.len()).unwrap_or(0);
    assert!(before > 0, "expected the writes to be in the log");

    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&pool)
        .await
        .unwrap();

    // Still open, and the log is already empty: the checkpoint did not have to
    // wait for anything to be closed.
    let after = std::fs::metadata(&wal).map(|m| m.len()).unwrap_or(0);
    assert_eq!(
        after, 0,
        "the log still held {after} bytes after a checkpoint"
    );

    pool.close().await;
}

/// Closing the pool has to fold the write-ahead log back into the database
/// file, so that `forma.db` on its own holds everything.
///
/// WAL is durable either way and nothing is lost by skipping this — but the
/// writes would live in a sidecar file, and a backup that copied `forma.db`
/// alone would be a backup of almost nothing. Measured before shutdown closed
/// the pool: a 4 KB database file beside a 177 KB log.
///
/// The check is that the file *contains* the data rather than that the sidecar
/// has been unlinked: SQLite removes it a moment after the last connection
/// goes, and asserting on that would be a race.
#[tokio::test]
async fn closing_the_pool_leaves_everything_in_the_database_file() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("forma.db");
    let pool = persistence::connect(&format!("sqlite://{}", db.display()), dir.path())
        .await
        .unwrap();

    let projects = SqliteProjectRepository::new(pool.clone());
    for name in ["One", "Two", "Three"] {
        projects.create(account(), name).await.unwrap();
    }

    // While it is open, the writes are out in the log rather than the file.
    let wal = dir.path().join("forma.db-wal");
    assert!(
        wal.exists(),
        "expected a write-ahead log while the pool is open"
    );

    persistence::checkpoint_and_close(&pool).await.unwrap();

    // Copy only the database file, which is what a backup would take.
    let backup_dir = tempfile::tempdir().unwrap();
    let backup = backup_dir.path().join("forma.db");
    std::fs::copy(&db, &backup).unwrap();

    let restored =
        persistence::connect(&format!("sqlite://{}", backup.display()), backup_dir.path())
            .await
            .unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM projects")
        .fetch_one(&restored)
        .await
        .unwrap();

    assert_eq!(count, 3, "the database file alone did not hold the writes");
}
