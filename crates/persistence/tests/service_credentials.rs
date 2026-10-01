//! The Pontis sidecar's credential store (Pontis plan §10(b) D1): the PAT
//! machinery, behind its own prefix and its own table.

use persistence::{
    AccountRepository, ApiKeyRepository, ServiceCredentialRepository, SqliteAccountRepository,
    SqliteApiKeyRepository, SqliteServiceCredentialRepository, SERVICE_PREFIX,
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

    fn credentials(&self) -> SqliteServiceCredentialRepository {
        SqliteServiceCredentialRepository::new(self.pool.clone())
    }
}

#[tokio::test]
async fn creation_returns_the_secret_once_and_stores_only_its_hash() {
    let db = Database::new().await;
    let created = db
        .credentials()
        .create(" pontis sidecar ")
        .await
        .expect("mint");

    assert!(created.token.starts_with(SERVICE_PREFIX));
    assert!(!created.token.starts_with(persistence::PAT_PREFIX));
    assert_eq!(created.credential.name, "pontis sidecar");
    assert!(created.token.starts_with(&created.credential.token_prefix));
    assert!(created.credential.token_prefix.len() < created.token.len());

    let stored: String = sqlx::query_scalar("SELECT token_hash FROM service_credentials")
        .fetch_one(&db.pool)
        .await
        .expect("row");
    assert_ne!(stored, created.token, "the secret itself is never stored");
    assert_eq!(
        stored,
        blake3::hash(created.token.as_bytes()).to_hex().to_string()
    );

    let listed = db.credentials().list().await.expect("list");
    assert_eq!(listed, vec![created.credential]);
}

#[tokio::test]
async fn only_the_exact_secret_authenticates() {
    let db = Database::new().await;
    let created = db.credentials().create("sidecar").await.unwrap();

    let found = db.credentials().authenticate(&created.token).await.unwrap();
    assert_eq!(found.map(|c| c.id), Some(created.credential.id));

    // Same prefix, so the same index bucket, but not the same secret.
    let mut wrong = created.token.clone();
    let last = wrong.pop().expect("a character");
    wrong.push(if last == '0' { '1' } else { '0' });
    assert!(db
        .credentials()
        .authenticate(&wrong)
        .await
        .unwrap()
        .is_none());

    for malformed in ["", SERVICE_PREFIX, "forma_svc_nothex", "Bearer x"] {
        assert!(db
            .credentials()
            .authenticate(malformed)
            .await
            .unwrap()
            .is_none());
    }
}

/// Separate tables and separate prefixes: neither store recognises the other's
/// secret, so the classes cannot leak into each other below the HTTP boundary
/// either.
#[tokio::test]
async fn a_pat_is_not_a_service_credential_nor_the_reverse() {
    let db = Database::new().await;
    let account = SqliteAccountRepository::new(db.pool.clone())
        .bootstrap()
        .await
        .unwrap();
    let keys = SqliteApiKeyRepository::new(db.pool.clone());
    let pat = keys.create(account.id, None, "script").await.unwrap().token;
    let service = db.credentials().create("sidecar").await.unwrap().token;

    assert!(db.credentials().authenticate(&pat).await.unwrap().is_none());
    assert!(keys.authenticate(&service).await.unwrap().is_none());
}

#[tokio::test]
async fn revocation_is_idempotent_and_keeps_the_first_date() {
    let db = Database::new().await;
    let created = db.credentials().create("sidecar").await.unwrap();

    let first = db
        .credentials()
        .revoke(created.credential.id)
        .await
        .unwrap();
    assert!(first.is_revoked());
    let second = db
        .credentials()
        .revoke(created.credential.id)
        .await
        .unwrap();
    assert_eq!(first.revoked_at, second.revoked_at);

    // Still found, so the boundary can say "revoked" rather than "unknown".
    let found = db
        .credentials()
        .authenticate(&created.token)
        .await
        .unwrap()
        .expect("found");
    assert!(found.is_revoked());

    assert!(db
        .credentials()
        .revoke(design_core::ServiceCredentialId::new())
        .await
        .is_err());
}

#[tokio::test]
async fn last_use_is_recorded_and_not_for_a_revoked_credential() {
    let db = Database::new().await;
    let created = db.credentials().create("sidecar").await.unwrap();
    let id = created.credential.id;

    db.credentials().touch_last_used(id).await.unwrap();
    let used = db.credentials().list().await.unwrap().remove(0);
    assert!(used.last_used_at.is_some());

    let other = db
        .credentials()
        .create("other")
        .await
        .unwrap()
        .credential
        .id;
    db.credentials().revoke(other).await.unwrap();
    db.credentials().touch_last_used(other).await.unwrap();
    let revoked = db
        .credentials()
        .list()
        .await
        .unwrap()
        .into_iter()
        .find(|c| c.id == other)
        .unwrap();
    assert!(revoked.last_used_at.is_none());
}
