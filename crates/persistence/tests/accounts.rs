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
async fn the_migration_leaves_one_unclaimed_bootstrap_account() {
    let harness = Harness::start().await;
    let account = harness.accounts().bootstrap().await.expect("bootstrap");

    assert_eq!(account.id.to_string(), BOOTSTRAP_ACCOUNT);
    assert!(account.bootstrap);
    assert!(account.is_claimable(), "nobody has claimed it yet");
}

#[tokio::test]
async fn the_bootstrap_account_is_claimable_exactly_once() {
    let harness = Harness::start().await;
    let accounts = harness.accounts();

    let first = accounts
        .claim_bootstrap(AuthUserId::new(), "First@Example.com ")
        .await
        .expect("the claim");
    let first = first.expect("the first claim succeeds");
    assert_eq!(first.role, AccountRole::Owner);
    assert_eq!(first.email, "first@example.com", "addresses are normalized");

    // The whole point of plan 113.1: a second person must not walk into the
    // workspace behind the first.
    let second = accounts
        .claim_bootstrap(AuthUserId::new(), "second@example.com")
        .await
        .expect("the second claim");
    assert!(second.is_none(), "a legacy account is claimed once");

    assert!(!accounts.bootstrap().await.unwrap().is_claimable());
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

    assert!(accounts.open_invite("nobody@example.com").await.unwrap().is_none());

    accounts.invite("  Someone@Example.com").await.expect("invite");
    assert!(
        accounts.open_invite("someone@example.com").await.unwrap().is_some(),
        "case and spacing must not create a second identity"
    );

    let user = AuthUserId::new();
    assert!(accounts.claim_invite("someone@example.com", user).await.unwrap());
    assert!(
        !accounts.claim_invite("someone@example.com", user).await.unwrap(),
        "a claimed invite cannot be claimed again"
    );
    assert!(accounts.open_invite("someone@example.com").await.unwrap().is_none());
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
    assert_eq!(projects.get(a, a_project.id).await.unwrap().id, a_project.id);
    assert_eq!(projects.get(b, b_project.id).await.unwrap().id, b_project.id);

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
        &projects.rename(a, b_project.id, "Stolen").await.unwrap_err()
    ));
    assert!(not_found(&projects.delete(a, b_project.id).await.unwrap_err()));
    assert!(not_found(
        &projects
            .put_brand(a, &brand_kit(b_project.id))
            .await
            .unwrap_err()
    ));

    // A brand read across the boundary reports "no kit" rather than the kit.
    projects.put_brand(b, &brand_kit(b_project.id)).await.unwrap();
    assert!(projects.get_brand(b, b_project.id).await.unwrap().is_some());
    assert!(
        projects.get_brand(a, b_project.id).await.unwrap().is_none(),
        "a brand kit is reached through its project, so it inherits the scope"
    );

    // Nothing above actually touched B's work.
    assert_eq!(projects.list(b).await.unwrap().len(), 1);
    assert_eq!(projects.get(b, b_project.id).await.unwrap().name, "B's work");
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
