use design_core::AuthUserId;
use persistence::{
    AccountRepository, ApiKeyRepository, PersistenceError, SqliteAccountRepository,
    SqliteApiKeyRepository,
};

struct Database {
    _dir: tempfile::TempDir,
    pool: sqlx::SqlitePool,
}

impl Database {
    async fn new() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let database_url = format!("sqlite://{}", dir.path().join("forma.db").display());
        let pool = persistence::connect(&database_url, dir.path())
            .await
            .expect("migrations");
        Self { _dir: dir, pool }
    }

    fn keys(&self) -> SqliteApiKeyRepository {
        SqliteApiKeyRepository::new(self.pool.clone())
    }

    fn accounts(&self) -> SqliteAccountRepository {
        SqliteAccountRepository::new(self.pool.clone())
    }
}

#[tokio::test]
async fn creation_returns_the_secret_once_and_stores_only_its_hash() {
    let db = Database::new().await;
    let account = db.accounts().bootstrap().await.expect("bootstrap");
    let creator = AuthUserId::new();

    let created = db
        .keys()
        .create(account.id, Some(creator), "Command line")
        .await
        .expect("create key");

    let secret = created
        .token
        .strip_prefix("forma_pat_")
        .expect("public PAT marker");
    assert_eq!(secret.len(), 64);
    assert!(secret.bytes().all(|byte| byte.is_ascii_hexdigit()));
    assert!(created.token.starts_with(&created.key.token_prefix));
    assert_eq!(created.key.created_by_user_id, Some(creator));

    let stored: (String, String) =
        sqlx::query_as("SELECT token_prefix, token_hash FROM api_keys WHERE id = ?")
            .bind(created.key.id.to_string())
            .fetch_one(&db.pool)
            .await
            .expect("stored key");

    assert_eq!(stored.0, created.key.token_prefix);
    assert_ne!(stored.0, created.token);
    assert_ne!(stored.1, created.token);
    assert_eq!(stored.1.len(), 64);

    let authenticated = db
        .keys()
        .authenticate(&created.token)
        .await
        .expect("authenticate")
        .expect("known key");
    assert_eq!(authenticated, created.key);
    assert!(db
        .keys()
        .authenticate("forma_pat_not-a-real-secret")
        .await
        .expect("invalid token is not a storage error")
        .is_none());
}

#[tokio::test]
async fn revocation_is_immediate_and_independent() {
    let db = Database::new().await;
    let account = db.accounts().bootstrap().await.expect("bootstrap");
    let keys = db.keys();
    let first = keys
        .create(account.id, None, "First")
        .await
        .expect("first key");
    let second = keys
        .create(account.id, None, "Second")
        .await
        .expect("second key");

    keys.touch_last_used(account.id, first.key.id)
        .await
        .expect("first touch");
    let used_once = keys
        .authenticate(&first.token)
        .await
        .expect("authenticate")
        .expect("first key")
        .last_used_at
        .expect("last used");
    keys.touch_last_used(account.id, first.key.id)
        .await
        .expect("coalesced touch");
    let used_twice = keys
        .authenticate(&first.token)
        .await
        .expect("authenticate")
        .expect("first key")
        .last_used_at
        .expect("last used");
    assert_eq!(
        used_once, used_twice,
        "the hot path must not rewrite last_used_at for every request"
    );

    let revoked = keys
        .revoke(account.id, first.key.id)
        .await
        .expect("revoke first");
    assert!(revoked.is_revoked());
    assert!(keys
        .authenticate(&first.token)
        .await
        .expect("revoked lookup")
        .expect("the boundary needs to distinguish a revoked key")
        .is_revoked());
    assert!(!keys
        .authenticate(&second.token)
        .await
        .expect("second lookup")
        .expect("second key")
        .is_revoked());
}

#[tokio::test]
async fn listing_and_revocation_are_account_scoped() {
    let db = Database::new().await;
    let first_account = db.accounts().bootstrap().await.expect("bootstrap");
    let second_account = db
        .accounts()
        .create("Second workspace")
        .await
        .expect("second account");
    let keys = db.keys();
    let first = keys
        .create(first_account.id, None, "First")
        .await
        .expect("first key");
    let second = keys
        .create(second_account.id, None, "Second")
        .await
        .expect("second key");

    assert_eq!(
        keys.list(first_account.id).await.expect("first list"),
        vec![first.key.clone()]
    );
    assert_eq!(
        keys.list(second_account.id).await.expect("second list"),
        vec![second.key]
    );

    let hidden = keys.revoke(second_account.id, first.key.id).await;
    assert!(
        matches!(hidden, Err(PersistenceError::NotFound(_))),
        "another account's known key must look absent"
    );
    assert!(!keys
        .authenticate(&first.token)
        .await
        .expect("first lookup")
        .expect("first key")
        .is_revoked());
}
