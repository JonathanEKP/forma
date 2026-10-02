//! The host-side invocation ledger of Boundary F (Pontis plan §10(b) D6),
//! against a real temporary SQLite. Every assertion about what was stored reads
//! the row back rather than trusting a return value.

use std::sync::atomic::{AtomicUsize, Ordering};

use design_core::{
    AccountId, Actor, AuthUserId, DesignDocument, DesignFormat, DesignId, FormatPreset,
    InvocationId, Page, Version,
};
use persistence::{
    AccountRepository, Claim, DesignRepository, InvocationLedger, InvocationRequest,
    PersistenceError, ProjectRepository, SqliteAccountRepository, SqliteDesignRepository,
    SqliteInvocationLedger, SqliteProjectRepository,
};
use serde_json::json;
use sqlx::SqlitePool;

fn bootstrap() -> AccountId {
    AccountId(uuid::Uuid::parse_str(design_core::BOOTSTRAP_ACCOUNT).expect("a constant uuid"))
}

struct Harness {
    dir: tempfile::TempDir,
    pool: SqlitePool,
}

impl Harness {
    async fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let pool = open(&dir).await;
        Self { dir, pool }
    }

    /// Closes the database the way a stopping server does and opens it again,
    /// as the restarted process would.
    async fn restart(self) -> Self {
        persistence::checkpoint_and_close(&self.pool).await.unwrap();
        let pool = open(&self.dir).await;
        Self {
            dir: self.dir,
            pool,
        }
    }

    fn ledger(&self) -> SqliteInvocationLedger {
        SqliteInvocationLedger::new(self.pool.clone())
    }

    fn designs(&self) -> SqliteDesignRepository {
        SqliteDesignRepository::new(self.pool.clone())
    }

    async fn design(&self, account: AccountId) -> DesignId {
        let project = SqliteProjectRepository::new(self.pool.clone())
            .create(account, "Ledger")
            .await
            .unwrap();
        self.designs()
            .create(
                account,
                project.id,
                "Cover",
                &document("Cover"),
                &Actor::human(),
            )
            .await
            .unwrap()
            .id
    }
}

async fn open(dir: &tempfile::TempDir) -> SqlitePool {
    let db = dir.path().join("forma.db");
    persistence::connect(&format!("sqlite://{}", db.display()), dir.path())
        .await
        .expect("migrations must apply")
}

fn document(name: &str) -> DesignDocument {
    let mut document = DesignDocument::new(DesignFormat::from_preset(FormatPreset::Square));
    document.pages.push(Page::new(name));
    document
}

fn id(raw: &str) -> InvocationId {
    InvocationId::parse(raw).unwrap()
}

fn apply_request(user: AuthUserId, design: DesignId) -> InvocationRequest {
    InvocationRequest {
        represented_user: user,
        capability: "design.applyOperations".into(),
        design: Some(design),
        input: json!({ "base_version": 1, "operations": [{ "op": "rename_page", "name": "Edited" }] }),
    }
}

/// What Boundary F will do with the ledger, reduced to its shape: execute only
/// on `Execute`, record the result, and otherwise answer from the ledger.
/// `executions` counts how often the domain binding actually ran.
async fn deliver(
    harness: &Harness,
    account: AccountId,
    invocation: &InvocationId,
    request: &InvocationRequest,
    executions: &AtomicUsize,
) -> Result<serde_json::Value, Claim> {
    match harness
        .ledger()
        .claim(account, invocation, request)
        .await
        .unwrap()
    {
        Claim::Execute => {
            executions.fetch_add(1, Ordering::SeqCst);
            let design = request.design.expect("these deliveries name a design");
            let saved = harness
                .designs()
                .save(
                    account,
                    design,
                    Version::INITIAL,
                    &document("Edited"),
                    &Actor::human(),
                )
                .await
                .unwrap();
            let result = json!({ "design_id": design, "version": saved.version });
            harness
                .ledger()
                .complete(account, invocation, None, &result)
                .await
                .unwrap();
            Ok(result)
        }
        Claim::Replay(result) => Ok(result),
        other => Err(other),
    }
}

// --- Acceptance -----------------------------------------------------------

/// The case D6 was opened for. Without the ledger, the second delivery reaches
/// `save` with the same base_version and answers VERSION_CONFLICT: an error
/// for a request that succeeded. With it, the retry gets the first answer and
/// the design is not touched again.
#[tokio::test]
async fn a_replay_returns_the_stored_result_and_does_not_execute_again() {
    let harness = Harness::start().await;
    let design = harness.design(bootstrap()).await;
    let request = apply_request(AuthUserId::new(), design);
    let invocation = id("inv_apply_1");
    let executions = AtomicUsize::new(0);

    let first = deliver(&harness, bootstrap(), &invocation, &request, &executions)
        .await
        .expect("first delivery executes");
    let replay = deliver(&harness, bootstrap(), &invocation, &request, &executions)
        .await
        .expect("a duplicate is answered, not refused");

    assert_eq!(replay, first);
    assert_eq!(first["version"], json!(2));
    assert_eq!(
        executions.load(Ordering::SeqCst),
        1,
        "executed exactly once"
    );
    let stored = harness.designs().get(bootstrap(), design).await.unwrap();
    assert_eq!(
        stored.version,
        Version(2),
        "the design moved once, not twice"
    );

    // And the same check against the row itself, not the return value.
    let record = harness
        .ledger()
        .get(bootstrap(), &invocation)
        .await
        .unwrap()
        .expect("the row exists");
    assert_eq!(record.result, Some(first));
    assert_eq!(record.design, Some(design));
    assert_eq!(record.capability, "design.applyOperations");
    assert_eq!(record.represented_user, request.represented_user);
    assert!(record.completed_at.is_some());
    assert!(
        record.claimed_at.ends_with('Z') && record.claimed_at.len() > 20,
        "the repository stamps its own time: {}",
        record.claimed_at
    );
}

/// Same key, different request: refused, in every way a request can differ,
/// and never handed the first delivery's answer.
#[tokio::test]
async fn a_different_fingerprint_under_the_same_id_is_refused() {
    let harness = Harness::start().await;
    let design = harness.design(bootstrap()).await;
    let other_design = harness.design(bootstrap()).await;
    let request = apply_request(AuthUserId::new(), design);
    let invocation = id("inv_apply_2");
    let executions = AtomicUsize::new(0);

    let first = deliver(&harness, bootstrap(), &invocation, &request, &executions)
        .await
        .unwrap();

    let different_input = InvocationRequest {
        input: json!({ "base_version": 1, "operations": [] }),
        ..request.clone()
    };
    let different_user = InvocationRequest {
        represented_user: AuthUserId::new(),
        ..request.clone()
    };
    let different_capability = InvocationRequest {
        capability: "design.export".into(),
        ..request.clone()
    };
    let different_design = InvocationRequest {
        design: Some(other_design),
        ..request.clone()
    };

    for variant in [
        different_input,
        different_user,
        different_capability,
        different_design,
    ] {
        let answer = deliver(&harness, bootstrap(), &invocation, &variant, &executions).await;
        assert_eq!(answer, Err(Claim::Conflict), "{variant:?}");
    }

    assert_eq!(executions.load(Ordering::SeqCst), 1);
    let record = harness
        .ledger()
        .get(bootstrap(), &invocation)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        record.result,
        Some(first),
        "a refusal changes nothing stored"
    );
}

/// The fingerprint is of the request, not of how its JSON was serialized: a
/// sidecar that rebuilds the body for a retry must not conflict with itself.
#[tokio::test]
async fn key_order_does_not_change_the_fingerprint() {
    let harness = Harness::start().await;
    let design = harness.design(bootstrap()).await;
    let user = AuthUserId::new();
    let invocation = id("inv_order");
    let executions = AtomicUsize::new(0);

    let mut request = apply_request(user, design);
    request.input = serde_json::from_str(r#"{"a":1,"b":{"y":2,"x":[{"q":1,"p":2}]}}"#).unwrap();
    let mut reordered = request.clone();
    reordered.input = serde_json::from_str(r#"{"b":{"x":[{"p":2,"q":1}],"y":2},"a":1}"#).unwrap();

    let first = deliver(&harness, bootstrap(), &invocation, &request, &executions).await;
    let again = deliver(&harness, bootstrap(), &invocation, &reordered, &executions).await;

    assert_eq!(again, first);
    assert_eq!(executions.load(Ordering::SeqCst), 1);
}

/// A ledger in memory is not a ledger: the retry it exists for arrives after a
/// restart. Closing and reopening the file must leave both the stored answer
/// and the refusal intact.
#[tokio::test]
async fn the_ledger_survives_reopening_the_database() {
    let harness = Harness::start().await;
    let design = harness.design(bootstrap()).await;
    let request = apply_request(AuthUserId::new(), design);
    let invocation = id("inv_restart");
    let executions = AtomicUsize::new(0);

    let first = deliver(&harness, bootstrap(), &invocation, &request, &executions)
        .await
        .unwrap();

    let harness = harness.restart().await;

    let replay = deliver(&harness, bootstrap(), &invocation, &request, &executions).await;
    assert_eq!(replay, Ok(first));
    assert_eq!(executions.load(Ordering::SeqCst), 1);

    let mut changed = request.clone();
    changed.input = json!({ "base_version": 2 });
    let refused = deliver(&harness, bootstrap(), &invocation, &changed, &executions).await;
    assert_eq!(refused, Err(Claim::Conflict));
}

/// Rows belong to an account. Another account cannot read one, cannot be
/// answered from one, and cannot tell that the id was used at all: the same id
/// there is simply a first delivery of its own.
#[tokio::test]
async fn rows_are_scoped_to_the_account() {
    let harness = Harness::start().await;
    let other = SqliteAccountRepository::new(harness.pool.clone())
        .create("Other")
        .await
        .unwrap()
        .id;
    let ours = harness.design(bootstrap()).await;
    let theirs = harness.design(other).await;
    let invocation = id("inv_shared");
    let user = AuthUserId::new();
    let executions = AtomicUsize::new(0);

    deliver(
        &harness,
        bootstrap(),
        &invocation,
        &apply_request(user, ours),
        &executions,
    )
    .await
    .unwrap();

    assert_eq!(
        harness.ledger().get(other, &invocation).await.unwrap(),
        None,
        "another account cannot read the row"
    );

    // The same request, presented as the other account, is not a replay:
    // naming our design from there is refused as though it did not exist, and
    // nothing is written.
    let crossing = harness
        .ledger()
        .claim(other, &invocation, &apply_request(user, ours))
        .await;
    assert!(matches!(crossing, Err(PersistenceError::DesignNotFound(d)) if d == ours));
    assert_eq!(
        harness.ledger().get(other, &invocation).await.unwrap(),
        None
    );

    // Its own request under the same id is its own first delivery.
    let own = deliver(
        &harness,
        other,
        &invocation,
        &apply_request(user, theirs),
        &executions,
    )
    .await
    .unwrap();
    assert_eq!(own["design_id"], json!(theirs));
    assert_eq!(executions.load(Ordering::SeqCst), 2);

    // And each account still reads only its own.
    let ours_row = harness
        .ledger()
        .get(bootstrap(), &invocation)
        .await
        .unwrap()
        .unwrap();
    let theirs_row = harness
        .ledger()
        .get(other, &invocation)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(ours_row.design, Some(ours));
    assert_eq!(theirs_row.design, Some(theirs));
    assert_ne!(ours_row.fingerprint, theirs_row.fingerprint);
}

// --- The claim's lifecycle ------------------------------------------------

/// A claim that has not completed is never executed again, and a result is
/// written once.
#[tokio::test]
async fn an_unfinished_claim_is_in_flight_and_a_result_is_written_once() {
    let harness = Harness::start().await;
    let design = harness.design(bootstrap()).await;
    let request = apply_request(AuthUserId::new(), design);
    let invocation = id("inv_flight");
    let ledger = harness.ledger();

    assert_eq!(
        ledger
            .claim(bootstrap(), &invocation, &request)
            .await
            .unwrap(),
        Claim::Execute
    );
    let second = ledger
        .claim(bootstrap(), &invocation, &request)
        .await
        .unwrap();
    assert!(matches!(second, Claim::InFlight { .. }), "{second:?}");

    // Still in flight after a restart: a crash between claim and complete
    // leaves the outcome unknown, and unknown is not a licence to run again.
    let harness = harness.restart().await;
    let ledger = harness.ledger();
    let after = ledger
        .claim(bootstrap(), &invocation, &request)
        .await
        .unwrap();
    assert!(matches!(after, Claim::InFlight { .. }), "{after:?}");

    ledger
        .complete(bootstrap(), &invocation, None, &json!({ "version": 2 }))
        .await
        .unwrap();
    let again = ledger
        .complete(bootstrap(), &invocation, None, &json!({ "version": 3 }))
        .await;
    assert!(matches!(again, Err(PersistenceError::InvalidTransition(_))));

    let record = ledger.get(bootstrap(), &invocation).await.unwrap().unwrap();
    assert_eq!(record.result, Some(json!({ "version": 2 })));
}

/// An execution that settled nothing gives its claim back, so the runtime can
/// return under the same id; one that settled something never can.
#[tokio::test]
async fn release_frees_a_claim_but_never_a_result() {
    let harness = Harness::start().await;
    let design = harness.design(bootstrap()).await;
    let request = apply_request(AuthUserId::new(), design);
    let invocation = id("inv_release");
    let ledger = harness.ledger();

    assert_eq!(
        ledger
            .claim(bootstrap(), &invocation, &request)
            .await
            .unwrap(),
        Claim::Execute
    );
    ledger.release(bootstrap(), &invocation).await.unwrap();
    assert_eq!(ledger.get(bootstrap(), &invocation).await.unwrap(), None);

    assert_eq!(
        ledger
            .claim(bootstrap(), &invocation, &request)
            .await
            .unwrap(),
        Claim::Execute,
        "a released id may be claimed again"
    );
    ledger
        .complete(bootstrap(), &invocation, None, &json!({ "ok": true }))
        .await
        .unwrap();

    let refused = ledger.release(bootstrap(), &invocation).await;
    assert!(matches!(
        refused,
        Err(PersistenceError::InvalidTransition(_))
    ));
    assert!(ledger
        .get(bootstrap(), &invocation)
        .await
        .unwrap()
        .is_some());

    let missing = ledger.release(bootstrap(), &id("inv_never")).await;
    assert!(matches!(missing, Err(PersistenceError::NotFound(_))));
}

// --- Deletion -------------------------------------------------------------

/// Deleting a design erases the stored result, which may carry its content,
/// but keeps the fingerprint: a late retry is still recognised and still not
/// executed.
#[tokio::test]
async fn deleting_the_design_tombstones_its_invocations() {
    let harness = Harness::start().await;
    let design = harness.design(bootstrap()).await;
    let request = apply_request(AuthUserId::new(), design);
    let invocation = id("inv_tombstone");
    let executions = AtomicUsize::new(0);

    deliver(&harness, bootstrap(), &invocation, &request, &executions)
        .await
        .unwrap();
    harness.designs().delete(bootstrap(), design).await.unwrap();

    let record = harness
        .ledger()
        .get(bootstrap(), &invocation)
        .await
        .unwrap()
        .expect("the row outlives the design");
    assert_eq!(record.result, None, "the result went with the design");
    assert_eq!(record.design, None);
    assert!(record.target_deleted_at.is_some());

    let replay = harness
        .ledger()
        .claim(bootstrap(), &invocation, &request)
        .await
        .unwrap();
    assert!(
        matches!(replay, Claim::TargetDeleted { .. }),
        "a replay is told, not refused as DESIGN_NOT_FOUND and not executed: {replay:?}"
    );

    let mut changed = request.clone();
    changed.input = json!({});
    assert_eq!(
        harness
            .ledger()
            .claim(bootstrap(), &invocation, &changed)
            .await
            .unwrap(),
        Claim::Conflict,
        "a tombstone still refuses a different request"
    );
    assert_eq!(executions.load(Ordering::SeqCst), 1);
}

/// The case CASCADE would get wrong: an invocation that *made* a design. Were
/// its row dropped with the design, a late retry would make it again.
#[tokio::test]
async fn a_produced_design_is_attached_on_completion_and_deleting_it_tombstones() {
    let harness = Harness::start().await;
    let invocation = id("inv_create");
    let request = InvocationRequest {
        represented_user: AuthUserId::new(),
        capability: "design.create".into(),
        design: None,
        input: json!({ "name": "Cover" }),
    };
    let ledger = harness.ledger();

    assert_eq!(
        ledger
            .claim(bootstrap(), &invocation, &request)
            .await
            .unwrap(),
        Claim::Execute
    );
    let design = harness.design(bootstrap()).await;
    ledger
        .complete(
            bootstrap(),
            &invocation,
            Some(design),
            &json!({ "design_id": design }),
        )
        .await
        .unwrap();
    let record = ledger.get(bootstrap(), &invocation).await.unwrap().unwrap();
    assert_eq!(record.design, Some(design));

    // Reached through the project's cascade, which fires the trigger too.
    let project = harness
        .designs()
        .get(bootstrap(), design)
        .await
        .unwrap()
        .project_id;
    SqliteProjectRepository::new(harness.pool.clone())
        .delete(bootstrap(), project)
        .await
        .unwrap();

    let replay = ledger
        .claim(bootstrap(), &invocation, &request)
        .await
        .unwrap();
    assert!(matches!(replay, Claim::TargetDeleted { .. }), "{replay:?}");
    let record = ledger.get(bootstrap(), &invocation).await.unwrap().unwrap();
    assert_eq!(record.result, None);
}

/// A design the account does not own cannot be attached on completion either.
#[tokio::test]
async fn completion_cannot_attach_another_accounts_design() {
    let harness = Harness::start().await;
    let other = SqliteAccountRepository::new(harness.pool.clone())
        .create("Other")
        .await
        .unwrap()
        .id;
    let theirs = harness.design(other).await;
    let invocation = id("inv_attach");
    let request = InvocationRequest {
        represented_user: AuthUserId::new(),
        capability: "design.create".into(),
        design: None,
        input: json!({}),
    };
    let ledger = harness.ledger();
    assert_eq!(
        ledger
            .claim(bootstrap(), &invocation, &request)
            .await
            .unwrap(),
        Claim::Execute
    );

    let attached = ledger
        .complete(bootstrap(), &invocation, Some(theirs), &json!({}))
        .await;
    assert!(matches!(attached, Err(PersistenceError::DesignNotFound(d)) if d == theirs));
    let record = ledger.get(bootstrap(), &invocation).await.unwrap().unwrap();
    assert_eq!(record.completed_at, None);
    assert_eq!(record.design, None);
}

/// Deleting an account deletes its ledger: nobody can present that account
/// again, so there is no retry left to answer. The schema does it, not a
/// caller.
#[tokio::test]
async fn deleting_the_account_deletes_its_ledger() {
    let harness = Harness::start().await;
    let other = SqliteAccountRepository::new(harness.pool.clone())
        .create("Leaving")
        .await
        .unwrap()
        .id;
    let design = harness.design(other).await;
    let executions = AtomicUsize::new(0);
    deliver(
        &harness,
        other,
        &id("inv_gone"),
        &apply_request(AuthUserId::new(), design),
        &executions,
    )
    .await
    .unwrap();
    // And one that names no design, so only the account's own cascade reaches it.
    let unattached = id("inv_gone_2");
    let request = InvocationRequest {
        design: None,
        ..apply_request(AuthUserId::new(), design)
    };
    assert_eq!(
        harness
            .ledger()
            .claim(other, &unattached, &request)
            .await
            .unwrap(),
        Claim::Execute
    );
    harness
        .ledger()
        .complete(other, &unattached, None, &json!({}))
        .await
        .unwrap();

    sqlx::query("DELETE FROM accounts WHERE id = ?")
        .bind(other.to_string())
        .execute(&harness.pool)
        .await
        .unwrap();

    let left: i64 =
        sqlx::query_scalar("SELECT count(*) FROM invocation_ledger WHERE account_id = ?")
            .bind(other.to_string())
            .fetch_one(&harness.pool)
            .await
            .unwrap();
    assert_eq!(left, 0);
}
