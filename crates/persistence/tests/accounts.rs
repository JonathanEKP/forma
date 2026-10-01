//! The account boundary (plan sections 107, 108 and 116.2).
//!
//! None of this needs an identity provider. `AuthUserId` is whatever subject a
//! provider would give, and every one here is invented — which is the point of
//! building the boundary first (plan 117.1): the question of whether one
//! person's work can reach another's is answerable before any OAuth exists.

use design_core::{
    AccountId, AccountRole, AuthUserId, BrandColors, BrandId, BrandKit, BrandRules,
    BrandTypography, Color, Version, BOOTSTRAP_ACCOUNT,
};
use persistence::{
    AccountRepository, PersistenceError, ProjectRepository, SqliteAccountRepository,
    SqliteProjectRepository,
};
use sqlx::SqlitePool;

struct Harness {
    _dir: tempfile::TempDir,
    pool: SqlitePool,
}

impl Harness {
    async fn start() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let pool = persistence::connect(
            &format!("sqlite://{}", dir.path().join("forma.db").display()),
            dir.path(),
        )
        .await
        .expect("migrations");
        Self { _dir: dir, pool }
    }

    fn accounts(&self) -> SqliteAccountRepository {
        SqliteAccountRepository::new(self.pool.clone())
    }

    fn projects(&self) -> SqliteProjectRepository {
        SqliteProjectRepository::new(self.pool.clone())
    }
}

fn brand_kit(project_id: design_core::ProjectId) -> BrandKit {
    BrandKit {
        id: BrandId::new(),
        project_id,
        name: "Default".into(),
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
        voice: String::new(),
        rules: BrandRules::default(),
        version: Version::INITIAL,
    }
}

#[tokio::test]
async fn the_migration_leaves_one_bootstrap_account_that_nobody_owns() {
    let harness = Harness::start().await;
    let account = harness.accounts().bootstrap().await.expect("bootstrap");

    assert_eq!(account.id.to_string(), BOOTSTRAP_ACCOUNT);
    assert!(account.bootstrap);
    assert!(account.is_unowned(), "nobody has a membership in it yet");
}

/// Ownership is the membership, not a ritual (plan §113.1, as amended).
///
/// The first member marks the account owned, and that is what retires the
/// deployment token. There is no separate claim to make: an account owns
/// projects, and the relation *is* the ownership.
#[tokio::test]
async fn the_first_membership_marks_the_account_owned() {
    let harness = Harness::start().await;
    let accounts = harness.accounts();

    let first = accounts
        .grant(
            accounts.bootstrap().await.unwrap().id,
            AuthUserId::new(),
            "First@Example.com ",
            AccountRole::Owner,
        )
        .await
        .expect("grant");
    assert_eq!(first.role, AccountRole::Owner);
    assert_eq!(first.email, "first@example.com", "addresses are normalized");

    let owned = accounts.bootstrap().await.unwrap();
    assert!(!owned.is_unowned());
    let stamped = owned.claimed_at.clone().expect("a date");

    // A second member does not move the date: the first one is what decided.
    accounts
        .grant(
            owned.id,
            AuthUserId::new(),
            "second@example.com",
            AccountRole::Owner,
        )
        .await
        .expect("grant");
    assert_eq!(
        accounts.bootstrap().await.unwrap().claimed_at.as_deref(),
        Some(stamped.as_str())
    );
}

#[tokio::test]
async fn a_membership_resolves_a_user_to_an_account() {
    let harness = Harness::start().await;
    let accounts = harness.accounts();
    let account = accounts.create("Second workspace").await.expect("create");
    let user = AuthUserId::new();

    assert!(
        accounts.membership_of(user).await.unwrap().is_none(),
        "a valid identity with no membership is not an error"
    );

    accounts
        .grant(account.id, user, "someone@example.com", AccountRole::Owner)
        .await
        .expect("grant");

    let found = accounts.membership_of(user).await.unwrap().expect("found");
    assert_eq!(found.account_id, account.id);
}

/// Recovery when the identity provider is gone (plan 113.3): the same account,
/// rebound to a fresh subject.
#[tokio::test]
async fn granting_again_rebinds_rather_than_duplicating() {
    let harness = Harness::start().await;
    let accounts = harness.accounts();
    let account = accounts.create("Workspace").await.unwrap();
    let user = AuthUserId::new();

    accounts
        .grant(account.id, user, "old@example.com", AccountRole::Owner)
        .await
        .unwrap();
    accounts
        .grant(account.id, user, "new@example.com", AccountRole::Owner)
        .await
        .unwrap();

    let found = accounts.membership_of(user).await.unwrap().unwrap();
    assert_eq!(found.email, "new@example.com");
}

#[tokio::test]
async fn an_invite_is_usable_once() {
    let harness = Harness::start().await;
    let accounts = harness.accounts();

    assert!(accounts
        .open_invite("nobody@example.com")
        .await
        .unwrap()
        .is_none());

    accounts
        .invite("  Someone@Example.com")
        .await
        .expect("invite");
    assert!(
        accounts
            .open_invite("someone@example.com")
            .await
            .unwrap()
            .is_some(),
        "case and spacing must not create a second identity"
    );

    let user = AuthUserId::new();
    assert!(accounts
        .claim_invite("someone@example.com", user)
        .await
        .unwrap());
    assert!(
        !accounts
            .claim_invite("someone@example.com", user)
            .await
            .unwrap(),
        "a claimed invite cannot be claimed again"
    );
    assert!(accounts
        .open_invite("someone@example.com")
        .await
        .unwrap()
        .is_none());
}

// ---------------------------------------------------------------------------
// Tenant isolation (plan 116.2)
// ---------------------------------------------------------------------------

/// Two accounts, one project each, and every reachable operation tried across
/// the boundary with a **known** id.
///
/// Plan 108.2: cross-account access answers the same not-found as an id that was
/// never issued, unconditionally. A distinct "forbidden" would confirm the row
/// exists, and ids are guessable in aggregate even when a single UUID is not.
#[tokio::test]
async fn one_account_cannot_reach_another_by_known_id() {
    let harness = Harness::start().await;
    let accounts = harness.accounts();
    let projects = harness.projects();

    let a = accounts.bootstrap().await.unwrap().id;
    let b = accounts.create("Account B").await.unwrap().id;

    let a_project = projects.create(a, "A's work").await.unwrap();
    let b_project = projects.create(b, "B's work").await.unwrap();

    // Each sees its own, and only its own.
    assert_eq!(projects.list(a).await.unwrap().len(), 1);
    assert_eq!(projects.list(b).await.unwrap().len(), 1);
    assert_eq!(
        projects.get(a, a_project.id).await.unwrap().id,
        a_project.id
    );
    assert_eq!(
        projects.get(b, b_project.id).await.unwrap().id,
        b_project.id
    );

    let not_found = |e: &PersistenceError| matches!(e, PersistenceError::ProjectNotFound(_));

    // Reads across the boundary.
    let refused = projects.get(a, b_project.id).await.unwrap_err();
    assert!(not_found(&refused), "got {refused:?}");
    assert!(not_found(&projects.get(b, a_project.id).await.unwrap_err()));

    // And it is the *same* error as an id nobody ever issued, so the two are
    // indistinguishable to a caller.
    let never_existed = projects
        .get(a, design_core::ProjectId::new())
        .await
        .unwrap_err();
    assert_eq!(
        std::mem::discriminant(&refused),
        std::mem::discriminant(&never_existed),
        "another account's project must look exactly like one that never existed"
    );

    // Mutations across the boundary.
    assert!(not_found(
        &projects
            .rename(a, b_project.id, "Stolen")
            .await
            .unwrap_err()
    ));
    assert!(not_found(
        &projects.delete(a, b_project.id).await.unwrap_err()
    ));
    assert!(not_found(
        &projects
            .put_brand(a, &brand_kit(b_project.id))
            .await
            .unwrap_err()
    ));

    // A brand read across the boundary reports "no kit" rather than the kit.
    projects
        .put_brand(b, &brand_kit(b_project.id))
        .await
        .unwrap();
    assert!(projects.get_brand(b, b_project.id).await.unwrap().is_some());
    assert!(
        projects.get_brand(a, b_project.id).await.unwrap().is_none(),
        "a brand kit is reached through its project, so it inherits the scope"
    );

    // Nothing above actually touched B's work.
    assert_eq!(projects.list(b).await.unwrap().len(), 1);
    assert_eq!(
        projects.get(b, b_project.id).await.unwrap().name,
        "B's work"
    );
}

/// A project belongs to exactly one account, and it is not the caller's to pick.
#[tokio::test]
async fn a_project_is_created_in_the_account_that_was_passed() {
    let harness = Harness::start().await;
    let accounts = harness.accounts();
    let projects = harness.projects();

    let b = accounts.create("Account B").await.unwrap().id;
    let project = projects.create(b, "Theirs").await.unwrap();

    let a = AccountId(uuid::Uuid::parse_str(BOOTSTRAP_ACCOUNT).unwrap());
    assert!(projects.get(a, project.id).await.is_err());
    assert!(projects.get(b, project.id).await.is_ok());
}

/// Every object type reachable through REST or MCP, tried across the boundary
/// with a known id (plan section 116.2).
///
/// Ownership is recorded once, on `projects`, so each of these is scoped by a
/// join rather than by a column of its own. That makes one mistake in one query
/// enough to open a hole, which is why this walks the whole surface instead of
/// sampling it.
#[tokio::test]
async fn every_object_type_is_scoped_to_its_account() {
    use design_core::{
        Actor, Asset, AssetId, AssetKind, AssetOrigin, AssetRequest, AssetRequestId,
        AssetRequestStatus, DesignColor, DesignDocument, DesignFormat, Export, ExportId,
        ExportScope, FormatPreset, Page, Template, TemplateId,
    };
    use persistence::{
        AssetRepository, AssetRequestRepository, DesignRepository, ExportRepository,
        SqliteAssetRepository, SqliteAssetRequestRepository, SqliteDesignRepository,
        SqliteExportRepository, SqliteTemplateRepository, TemplateRepository,
    };

    let harness = Harness::start().await;
    let accounts = harness.accounts();
    let projects = harness.projects();
    let designs = SqliteDesignRepository::new(harness.pool.clone());
    let assets = SqliteAssetRepository::new(harness.pool.clone());
    let requests = SqliteAssetRequestRepository::new(harness.pool.clone());
    let templates = SqliteTemplateRepository::new(harness.pool.clone());
    let exports = SqliteExportRepository::new(harness.pool.clone());

    let a = accounts.bootstrap().await.unwrap().id;
    let b = accounts.create("Account B").await.unwrap().id;
    let b_project = projects.create(b, "B's work").await.unwrap();

    let mut document = DesignDocument::new(DesignFormat::from_preset(FormatPreset::Square));
    let mut page = Page::new("Cover");
    page.background = Some(DesignColor::token("brand.background"));
    document.pages.push(page);

    // Everything below belongs to B.
    let design = designs
        .create(b, b_project.id, "B's cover", &document, &Actor::human())
        .await
        .unwrap();

    let asset = assets
        .create(
            b,
            &Asset {
                id: AssetId::new(),
                project_id: b_project.id,
                hash: "abcdef0123456789".into(),
                kind: AssetKind::Raster,
                mime_type: "image/png".into(),
                width: 10,
                height: 10,
                byte_size: 100,
                storage_key: "k".into(),
                original_filename: None,
                origin: AssetOrigin::UserUpload,
                attribution: None,
                created_at: String::new(),
            },
        )
        .await
        .unwrap();

    let request = requests
        .create(
            b,
            &AssetRequest {
                id: AssetRequestId::new(),
                project_id: b_project.id,
                design_id: None,
                purpose: "hero".into(),
                description: "a photograph".into(),
                suggested_prompt: None,
                requested_width: None,
                requested_height: None,
                status: AssetRequestStatus::Pending,
                resolved_asset_id: None,
                created_at: String::new(),
                resolved_at: None,
            },
        )
        .await
        .unwrap();

    let template = templates
        .create(
            b,
            &Template {
                id: TemplateId::new(),
                project_id: b_project.id,
                name: "Cover".into(),
                description: String::new(),
                document: document.clone(),
                roles: Vec::new(),
                created_at: String::new(),
            },
        )
        .await
        .unwrap();

    let export = exports
        .create(
            b,
            &Export {
                id: ExportId::new(),
                design_id: design.id,
                design_version: design.version,
                scope: ExportScope::SinglePage,
                page_id: Some(document.pages[0].id),
                scale: 1.0,
                storage_key: "e".into(),
                byte_size: 10,
                created_at: String::new(),
            },
        )
        .await
        .unwrap();

    // --- A holds every id, and reaches none of it -------------------------

    assert!(designs.get(a, design.id).await.is_err(), "design");
    assert!(
        designs.history(a, design.id).await.unwrap().is_empty(),
        "history"
    );
    assert!(
        designs
            .snapshot(a, design.id, design.version)
            .await
            .is_err(),
        "snapshot"
    );
    assert!(designs.delete(a, design.id).await.is_err(), "design delete");
    assert!(
        designs
            .list_for_project(a, b_project.id)
            .await
            .unwrap()
            .is_empty(),
        "designs of another account's project"
    );

    assert!(assets.get(a, asset.id).await.is_err(), "asset");
    assert!(
        assets
            .list_for_project(a, b_project.id)
            .await
            .unwrap()
            .is_empty(),
        "assets"
    );
    assert!(
        assets
            .find_by_hash(a, b_project.id, "abcdef0123456789")
            .await
            .unwrap()
            .is_none(),
        "a known hash must not confirm another account's asset"
    );
    assert!(
        assets.soft_delete(a, asset.id).await.is_err(),
        "asset delete"
    );

    assert!(requests.get(a, request.id).await.is_err(), "asset request");
    assert!(
        requests
            .list_for_project(a, b_project.id)
            .await
            .unwrap()
            .is_empty(),
        "asset requests"
    );
    assert!(
        requests.resolve(a, request.id, asset.id).await.is_err(),
        "resolving another account's request"
    );
    assert!(requests.cancel(a, request.id).await.is_err(), "cancel");

    assert!(templates.get(a, template.id).await.is_err(), "template");
    assert!(
        templates
            .list_for_project(a, b_project.id)
            .await
            .unwrap()
            .is_empty(),
        "templates"
    );
    assert!(
        templates.delete(a, template.id).await.is_err(),
        "template delete"
    );

    assert!(exports.get(a, export.id).await.is_err(), "export");
    assert!(
        exports
            .list_for_design(a, design.id)
            .await
            .unwrap()
            .is_empty(),
        "exports"
    );

    // Creating into another account's project writes nothing.
    assert!(
        designs
            .create(a, b_project.id, "Intruder", &document, &Actor::human())
            .await
            .is_err(),
        "a design cannot be created in another account's project"
    );

    // --- and B still has all of it, untouched -----------------------------

    assert_eq!(designs.get(b, design.id).await.unwrap().name, "B's cover");
    assert_eq!(designs.history(b, design.id).await.unwrap().len(), 1);
    assert!(designs.snapshot(b, design.id, design.version).await.is_ok());
    assert_eq!(assets.get(b, asset.id).await.unwrap().id, asset.id);
    assert_eq!(
        requests.get(b, request.id).await.unwrap().status,
        AssetRequestStatus::Pending
    );
    assert_eq!(templates.get(b, template.id).await.unwrap().id, template.id);
    assert_eq!(exports.get(b, export.id).await.unwrap().id, export.id);
    assert_eq!(
        designs
            .list_for_project(b, b_project.id)
            .await
            .unwrap()
            .len(),
        1
    );
}

/// The guarded UPDATE must not become an oracle.
///
/// `save` answers `VERSION_CONFLICT` with the version actually stored, so
/// answering one across the boundary would confirm the design exists and say how
/// often somebody else edits it. It has to be not-found instead.
#[tokio::test]
async fn a_cross_account_save_is_not_found_rather_than_a_version_conflict() {
    use design_core::{
        Actor, DesignColor, DesignDocument, DesignFormat, FormatPreset, Page, Version,
    };
    use persistence::{DesignRepository, SqliteDesignRepository};

    let harness = Harness::start().await;
    let accounts = harness.accounts();
    let designs = SqliteDesignRepository::new(harness.pool.clone());

    let a = accounts.bootstrap().await.unwrap().id;
    let b = accounts.create("Account B").await.unwrap().id;
    let b_project = harness.projects().create(b, "B's work").await.unwrap();

    let mut document = DesignDocument::new(DesignFormat::from_preset(FormatPreset::Square));
    let mut page = Page::new("Cover");
    page.background = Some(DesignColor::token("brand.background"));
    document.pages.push(page);

    let design = designs
        .create(b, b_project.id, "B's cover", &document, &Actor::human())
        .await
        .unwrap();

    let refused = designs
        .save(a, design.id, Version::INITIAL, &document, &Actor::human())
        .await
        .unwrap_err();

    assert!(
        matches!(refused, PersistenceError::DesignNotFound(_)),
        "a conflict here would leak the stored version: {refused:?}"
    );

    // And B's design did not move.
    assert_eq!(
        designs.get(b, design.id).await.unwrap().version,
        Version::INITIAL
    );
}

/// Decision D7: the account a user resolves to cannot be an arbitrary winner,
/// because there can only be one candidate. A session bound to the answer
/// (Pontis ADR-024) keeps it for its whole life.
#[tokio::test]
async fn a_user_cannot_hold_a_second_account() {
    let harness = Harness::start().await;
    let accounts = harness.accounts();
    let first = accounts.create("First").await.unwrap();
    let second = accounts.create("Second").await.unwrap();
    let user = AuthUserId::new();

    accounts
        .grant(first.id, user, "someone@example.com", AccountRole::Owner)
        .await
        .expect("the first membership");

    let refused = accounts
        .grant(second.id, user, "someone@example.com", AccountRole::Owner)
        .await;
    assert!(
        matches!(refused, Err(PersistenceError::InvalidTransition(_))),
        "a second account is the caller's mistake, not a storage fault: {refused:?}"
    );

    // Nothing was written, and the second account was not marked owned by it.
    assert_eq!(
        accounts
            .membership_of(user)
            .await
            .unwrap()
            .unwrap()
            .account_id,
        first.id
    );
    assert!(accounts.get(second.id).await.unwrap().is_unowned());
}

/// The schema, not `grant`'s check, is the guarantee: a writer that goes
/// around the repository still cannot create the second row.
#[tokio::test]
async fn the_schema_refuses_a_second_membership_for_one_user() {
    let harness = Harness::start().await;
    let accounts = harness.accounts();
    let first = accounts.create("First").await.unwrap();
    let second = accounts.create("Second").await.unwrap();
    let user = AuthUserId::new();

    accounts
        .grant(first.id, user, "someone@example.com", AccountRole::Owner)
        .await
        .unwrap();

    let raw = sqlx::query(
        "INSERT INTO account_memberships (account_id, user_id, email, role, created_at)
         VALUES (?, ?, 'someone@example.com', 'owner', 't')",
    )
    .bind(second.id.to_string())
    .bind(user.to_string())
    .execute(&harness.pool)
    .await;
    assert!(raw.is_err(), "user_id must be unique across accounts");

    let rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM account_memberships WHERE user_id = ?")
            .bind(user.to_string())
            .fetch_one(&harness.pool)
            .await
            .unwrap();
    assert_eq!(rows, 1);
}
