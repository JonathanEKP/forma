//! The shell-only gates around account creation and provider-loss recovery.
//!
//! These tests invoke the binary rather than the repository directly. The
//! repository already proves its normalization and upsert rules; this layer
//! proves the operator commands do not bypass them on the way in.

use std::path::Path;
use std::process::Output;

use design_core::{AuthUserId, BOOTSTRAP_ACCOUNT};
use persistence::AccountRepository;
use sqlx::SqlitePool;
use tokio::process::Command;

async fn run(data_dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_forma-server"))
        .args(args)
        .env("FORMA_DATA_DIR", data_dir)
        .env(
            "FORMA_DATABASE_URL",
            format!("sqlite://{}", data_dir.join("forma.db").display()),
        )
        .env("FORMA_LOG", "error")
        .env_remove("FORMA_REMOTE_TOKEN")
        .output()
        .await
        .expect("forma-server runs")
}

async fn accounts(data_dir: &Path) -> (persistence::SqliteAccountRepository, SqlitePool) {
    let pool = persistence::connect(
        &format!("sqlite://{}", data_dir.join("forma.db").display()),
        data_dir,
    )
    .await
    .expect("migrations");
    (
        persistence::SqliteAccountRepository::new(pool.clone()),
        pool,
    )
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[tokio::test]
async fn inviting_the_same_normalized_address_preserves_one_invite() {
    let dir = tempfile::tempdir().expect("temp dir");

    let first = run(dir.path(), &["invite", "Someone@Example.com "]).await;
    assert!(first.status.success(), "{}", stderr(&first));
    assert!(stdout(&first).contains("someone@example.com"));

    let (repository, pool) = accounts(dir.path()).await;
    let before = repository.list_invites().await.expect("invites");
    assert_eq!(before.len(), 1);
    assert_eq!(before[0].email, "someone@example.com");
    let created_at = before[0].created_at.clone();
    pool.close().await;

    let second = run(dir.path(), &["invite", "someone@example.com"]).await;
    assert!(second.status.success(), "{}", stderr(&second));

    let (repository, pool) = accounts(dir.path()).await;
    let after = repository.list_invites().await.expect("invites");
    assert_eq!(after.len(), 1, "the CLI must not create a second invite");
    assert_eq!(
        after[0].created_at, created_at,
        "inviting twice must not reset the original"
    );
    pool.close().await;
}

#[tokio::test]
async fn invites_marks_claimed_addresses_without_exposing_secrets() {
    let dir = tempfile::tempdir().expect("temp dir");
    let invited = run(dir.path(), &["invite", "claimed@example.com"]).await;
    assert!(invited.status.success(), "{}", stderr(&invited));

    let subject = AuthUserId::new();
    let (repository, pool) = accounts(dir.path()).await;
    assert!(repository
        .claim_invite("CLAIMED@example.com ", subject)
        .await
        .expect("claim"));
    pool.close().await;

    let listed = run(dir.path(), &["invites"]).await;
    assert!(listed.status.success(), "{}", stderr(&listed));
    let output = stdout(&listed);
    assert!(output.contains("claimed\tclaimed@example.com"), "{output}");
    assert!(output.contains(&subject.to_string()), "{output}");
    assert!(!output.to_lowercase().contains("token"), "{output}");
    assert!(!output.to_lowercase().contains("hash"), "{output}");
}

#[tokio::test]
async fn granting_the_same_subject_rebinds_instead_of_duplicating() {
    let dir = tempfile::tempdir().expect("temp dir");
    let subject = AuthUserId::new();
    let subject_arg = subject.to_string();

    let first = run(
        dir.path(),
        &["grant-owner", "Old@Example.com ", &subject_arg],
    )
    .await;
    assert!(first.status.success(), "{}", stderr(&first));

    let second = run(
        dir.path(),
        &["grant-owner", "new@example.com", &subject_arg],
    )
    .await;
    assert!(second.status.success(), "{}", stderr(&second));

    let (repository, pool) = accounts(dir.path()).await;
    let membership = repository
        .membership_of(subject)
        .await
        .expect("lookup")
        .expect("membership");
    assert_eq!(membership.account_id.to_string(), BOOTSTRAP_ACCOUNT);
    assert_eq!(membership.email, "new@example.com");

    let rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM account_memberships WHERE account_id = ? AND user_id = ?",
    )
    .bind(BOOTSTRAP_ACCOUNT)
    .bind(subject.to_string())
    .fetch_one(&pool)
    .await
    .expect("count");
    assert_eq!(rows, 1, "the grant must update the existing membership");
    pool.close().await;
}

#[tokio::test]
async fn grant_owner_without_a_subject_mints_and_prints_one() {
    let dir = tempfile::tempdir().expect("temp dir");
    let granted = run(dir.path(), &["grant-owner", "owner@example.com"]).await;
    assert!(granted.status.success(), "{}", stderr(&granted));

    let output = stdout(&granted);
    let raw = output
        .lines()
        .find_map(|line| line.strip_prefix("subject "))
        .expect("the minted subject is printed");
    let subject = AuthUserId(uuid::Uuid::parse_str(raw).expect("a subject UUID"));

    let (repository, pool) = accounts(dir.path()).await;
    assert!(repository
        .membership_of(subject)
        .await
        .expect("lookup")
        .is_some());
    pool.close().await;
}

#[tokio::test]
async fn command_failures_are_nonzero_and_actionable() {
    let dir = tempfile::tempdir().expect("temp dir");

    for (args, expected) in [
        (vec!["invite"], "usage: forma-server invite <email>"),
        (
            vec!["grant-owner", "owner@example.com", "not-a-uuid"],
            "subject UUID",
        ),
        (vec!["invites", "extra"], "usage: forma-server invites"),
    ] {
        let failed = run(dir.path(), &args).await;
        assert!(!failed.status.success(), "{args:?} unexpectedly succeeded");
        let error = stderr(&failed);
        assert!(error.contains(expected), "{args:?}: {error}");
    }
}

/// Minting a token on the machine (plan §112.1), which is what a smoke test or
/// an automation holds once the deployment token stops being a credential.
#[tokio::test]
async fn api_key_prints_the_token_once_and_stores_only_its_hash() {
    let dir = tempfile::tempdir().expect("temp dir");
    let out = run(dir.path(), &["api-key", "Smoke test"]).await;
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // stdout carries the value alone, so a shell can capture it without the
    // surrounding words; the explanation goes to stderr.
    let token = String::from_utf8(out.stdout)
        .expect("utf-8")
        .trim()
        .to_string();
    assert!(token.starts_with("forma_pat_"), "got {token:?}");
    assert!(token.len() > 40);

    let (_, pool) = accounts(dir.path()).await;
    let keys = persistence::SqliteApiKeyRepository::new(pool.clone());

    // It authenticates, and what was stored is not the token.
    let found = persistence::ApiKeyRepository::authenticate(&keys, &token)
        .await
        .expect("authenticate")
        .expect("the key it just minted");
    assert_eq!(found.name, "Smoke test");
    assert_eq!(found.account_id.to_string(), BOOTSTRAP_ACCOUNT);
    assert!(!found.is_revoked());

    let stored: String = sqlx::query_scalar("SELECT token_hash FROM api_keys WHERE id = ?")
        .bind(found.id.to_string())
        .fetch_one(&pool)
        .await
        .expect("the row");
    assert!(
        !stored.contains(&token),
        "the token itself must not be stored"
    );
}

#[tokio::test]
async fn api_key_needs_a_name_worth_reading_later() {
    let dir = tempfile::tempdir().expect("temp dir");
    for args in [vec!["api-key"], vec!["api-key", "   "]] {
        let out = run(dir.path(), &args).await;
        assert!(
            !out.status.success(),
            "a nameless key is one nobody can decide to revoke"
        );
    }
}
