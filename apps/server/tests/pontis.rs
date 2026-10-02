//! Boundary F: the operations the Pontis sidecar calls (Pontis plan §4, step 6).
//!
//! Everything here goes through the real authentication arm with a real
//! `forma_svc_` credential, except the one test that needs storage to fail
//! after authentication has already succeeded.

use std::sync::Arc;

use actix_web::{
    dev::Service,
    http::StatusCode,
    test::{self, TestRequest},
    web, App, HttpMessage,
};
use design_core::{
    AccountId, AccountRole, Actor, AuthContext, AuthKind, AuthUserId, DesignDocument, DesignFormat,
    DesignId, DesignOperation, FormatPreset, InvocationId, LayerId, Page, PageId, ProjectId,
};
use forma_server::auth::{APPLICATION_CONTEXT_HEADER, REPRESENTED_USER_HEADER};
use forma_server::config::AuthMode;
use forma_server::routes::pontis::{
    bounds, DESIGN_GET, DESIGN_VALIDATE, INVOCATION_HEADER, INVOCATION_OUTCOME_HEADER,
    MAX_CONCURRENT_VALIDATIONS, PROJECT_LIST, TIMEOUT_HEADER,
};
use forma_server::{routes, state::AppState};
use persistence::{
    AccountRepository, ApiKeyRepository, DesignRepository, InvocationLedger, InvocationRequest,
    ProjectRepository, ServiceCredentialRepository, SqliteInvocationLedger,
};
use renderer::FontRegistry;
use serde_json::{json, Value};

async fn hosted() -> (AppState, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let pool = persistence::connect(
        &format!("sqlite://{}", dir.path().join("forma.db").display()),
        dir.path(),
    )
    .await
    .expect("migrations");

    let font_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/fonts");
    let fonts = Arc::new(FontRegistry::from_dir(font_dir).expect("fonts"));
    let cache = Arc::new(renderer::RenderCache::new(dir.path().join("previews")));
    let store = Arc::new(persistence::LocalObjectStore::new(dir.path()));

    let state = AppState::new(pool, fonts, cache, store).with_mode(AuthMode::Supabase);
    (state, dir)
}

macro_rules! serve {
    ($state:expr) => {
        test::init_service(
            App::new()
                .wrap(actix_web::middleware::from_fn(
                    forma_server::auth::require_bearer,
                ))
                .app_data(web::Data::new($state))
                .configure(routes::configure),
        )
        .await
    };
}

/// A person with an account, one project and one design holding a text layer.
struct Person {
    user: AuthUserId,
    account: AccountId,
    project: ProjectId,
    design: DesignId,
    layer: LayerId,
}

fn headline(page: PageId, layer: LayerId, y: f32) -> DesignOperation {
    serde_json::from_value(json!({
        "op": "add_layer",
        "page_id": page,
        "layer": {
            "type": "text",
            "id": layer,
            "name": "Headline",
            "transform": { "x": 80.0, "y": y, "width": 920.0, "height": 200.0 },
            "text": "HELLO",
            "font_family": "Geist",
            "font_size": 96.0,
            "font_weight": 700,
            "color": "#171717"
        }
    }))
    .expect("an add_layer operation")
}

/// A document of `pages` pages, each with one text layer.
fn document(pages: usize) -> (DesignDocument, LayerId) {
    let mut document = DesignDocument::new(DesignFormat::from_preset(FormatPreset::Square));
    let mut operations = Vec::new();
    for index in 0..pages {
        let page = Page::new(format!("Page {}", index + 1));
        let id = page.id;
        document.pages.push(page);
        operations.push(headline(id, LayerId::new(), 160.0));
    }
    let first_layer = match &operations[0] {
        DesignOperation::AddLayer { layer, .. } => layer.id(),
        _ => unreachable!(),
    };
    let document = design_core::apply_operations(&document, &operations).expect("a valid document");
    (document, first_layer)
}

async fn person(state: &AppState, pages: usize) -> Person {
    let account = state.accounts.create("Theirs").await.expect("account").id;
    let user = AuthUserId::new();
    state
        .accounts
        .grant(
            account,
            user,
            &format!("{user}@example.com"),
            AccountRole::Owner,
        )
        .await
        .expect("grant");
    let project = state
        .projects
        .create(account, "Their project")
        .await
        .expect("project")
        .id;
    let (document, layer) = document(pages);
    let design = state
        .designs
        .create(account, project, "Cover", &document, &Actor::human())
        .await
        .expect("design")
        .id;
    Person {
        user,
        account,
        project,
        design,
        layer,
    }
}

async fn sidecar_token(state: &AppState) -> String {
    state
        .service_credentials
        .create("pontis sidecar")
        .await
        .expect("mint")
        .token
}

fn as_sidecar(request: TestRequest, token: &str, person: &Person) -> TestRequest {
    request
        .insert_header(("authorization", format!("Bearer {token}")))
        .insert_header((REPRESENTED_USER_HEADER, person.user.to_string()))
        .insert_header((APPLICATION_CONTEXT_HEADER, person.account.to_string()))
}

fn apply_request(
    token: &str,
    person: &Person,
    invocation: &str,
    body: Value,
) -> actix_http::Request {
    as_sidecar(
        TestRequest::post()
            .uri(&format!(
                "/api/v1/pontis/designs/{}/operations",
                person.design
            ))
            .insert_header((INVOCATION_HEADER, invocation))
            .set_json(body),
        token,
        person,
    )
    .to_request()
}

fn move_headline(person: &Person, base_version: u64, x: f32) -> Value {
    json!({
        "base_version": base_version,
        "operations": [{
            "op": "move_layer",
            "layer_id": person.layer,
            "frame": { "x": x }
        }]
    })
}

fn remove_headline(person: &Person, base_version: u64) -> Value {
    json!({
        "base_version": base_version,
        "operations": [{ "op": "remove_layer", "layer_id": person.layer }]
    })
}

fn approval(invocation: &str) -> Value {
    let now = chrono::Utc::now();
    json!({
        "approval_id": "apr_1",
        "session_id": "ses_1",
        "capability_id": "design.apply_operations",
        "scope": { "kind": "once", "invocation_id": invocation },
        "risk": "destructive",
        "granted_at": (now - chrono::Duration::seconds(5)).to_rfc3339(),
        "expires_at": (now + chrono::Duration::minutes(5)).to_rfc3339(),
    })
}

fn with_approval(mut body: Value, approval: Value) -> Value {
    body["approval"] = approval;
    body
}

/// The §32 envelope, plus the registry code beside Forma's.
async fn refused(
    response: actix_web::dev::ServiceResponse,
    status: StatusCode,
    code: &str,
    capability_error: &str,
) -> Value {
    assert_eq!(response.status(), status);
    let body: Value = test::read_body_json(response).await;
    assert_eq!(body["error"]["code"], code, "{body}");
    assert_eq!(
        body["error"]["capability_error"], capability_error,
        "{body}"
    );
    assert!(body["error"]["hint"].is_string(), "{body}");
    body
}

async fn version_of(state: &AppState, person: &Person) -> u64 {
    state
        .designs
        .get(person.account, person.design)
        .await
        .expect("design")
        .version
        .0
}

// --- the bounded read ---------------------------------------------------------

#[actix_web::test]
async fn a_read_returns_only_the_represented_accounts_data() {
    let (state, _dir) = hosted().await;
    let them = person(&state, 1).await;
    let other = person(&state, 1).await;
    let token = sidecar_token(&state).await;
    let app = serve!(state.clone());

    let response = test::call_service(
        &app,
        as_sidecar(
            TestRequest::get().uri("/api/v1/pontis/projects"),
            &token,
            &them,
        )
        .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let listed: Value = test::read_body_json(response).await;
    let ids: Vec<_> = listed
        .as_array()
        .expect("a list")
        .iter()
        .map(|project| project["id"].as_str().expect("an id").to_string())
        .collect();
    assert_eq!(ids, [them.project.to_string()]);

    let response = test::call_service(
        &app,
        as_sidecar(
            TestRequest::get().uri(&format!("/api/v1/pontis/designs/{}", them.design)),
            &token,
            &them,
        )
        .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let design: Value = test::read_body_json(response).await;
    assert_eq!(design["version"], 1, "the version a batch must echo");

    // Another account's design, named by its real id: indistinguishable from
    // an id that was never issued.
    let response = test::call_service(
        &app,
        as_sidecar(
            TestRequest::get().uri(&format!("/api/v1/pontis/designs/{}", other.design)),
            &token,
            &them,
        )
        .to_request(),
    )
    .await;
    refused(
        response,
        StatusCode::NOT_FOUND,
        "DESIGN_NOT_FOUND",
        "not_found",
    )
    .await;

    // And it cannot be reached by naming the other account as the context:
    // the arm revalidates membership before any route runs.
    let response = test::call_service(
        &app,
        TestRequest::get()
            .uri(&format!("/api/v1/pontis/designs/{}", other.design))
            .insert_header(("authorization", format!("Bearer {token}")))
            .insert_header((REPRESENTED_USER_HEADER, them.user.to_string()))
            .insert_header((APPLICATION_CONTEXT_HEADER, other.account.to_string()))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[actix_web::test]
async fn only_the_service_credential_reaches_boundary_f() {
    let (state, _dir) = hosted().await;
    let them = person(&state, 1).await;
    let pat = state
        .api_keys
        .create(them.account, Some(them.user), "automation")
        .await
        .expect("pat")
        .token;
    let app = serve!(state.clone());

    // A PAT is valid account access everywhere else in /api/v1, and still not
    // a represented person acting through Pontis.
    let response = test::call_service(
        &app,
        TestRequest::get()
            .uri("/api/v1/pontis/projects")
            .insert_header(("authorization", format!("Bearer {pat}")))
            .to_request(),
    )
    .await;
    refused(
        response,
        StatusCode::FORBIDDEN,
        "SERVICE_CREDENTIAL_REQUIRED",
        "authorization_denied",
    )
    .await;
}

#[actix_web::test]
async fn a_list_over_its_bound_is_refused_rather_than_cut() {
    let (state, _dir) = hosted().await;
    let them = person(&state, 1).await;
    let max_items = bounds(PROJECT_LIST)
        .max_list_items
        .expect("project.list declares an item bound") as usize;
    for index in 0..max_items {
        state
            .projects
            .create(them.account, &format!("Project {index}"))
            .await
            .expect("project");
    }
    let token = sidecar_token(&state).await;
    let app = serve!(state.clone());

    let response = test::call_service(
        &app,
        as_sidecar(
            TestRequest::get().uri("/api/v1/pontis/projects"),
            &token,
            &them,
        )
        .to_request(),
    )
    .await;
    let body = refused(
        response,
        StatusCode::UNPROCESSABLE_ENTITY,
        "RESULT_TOO_LARGE",
        "result_too_large",
    )
    .await;
    assert_eq!(body["error"]["details"]["items"], max_items + 1);
    assert_eq!(body["error"]["details"]["max_items"], max_items);
}

#[actix_web::test]
async fn an_unknown_operation_is_not_a_generic_proxy() {
    let (state, _dir) = hosted().await;
    let them = person(&state, 1).await;
    let token = sidecar_token(&state).await;
    let app = serve!(state.clone());

    let response = test::call_service(
        &app,
        as_sidecar(
            TestRequest::delete().uri(&format!("/api/v1/pontis/designs/{}", them.design)),
            &token,
            &them,
        )
        .to_request(),
    )
    .await;
    refused(
        response,
        StatusCode::NOT_FOUND,
        "NOT_FOUND",
        "capability_not_found",
    )
    .await;
    assert_eq!(version_of(&state, &them).await, 1);
}

// --- the write, and idempotency -----------------------------------------------

#[actix_web::test]
async fn the_same_invocation_twice_executes_once_and_replays_the_first_answer() {
    let (state, _dir) = hosted().await;
    let them = person(&state, 1).await;
    let token = sidecar_token(&state).await;
    let app = serve!(state.clone());

    let first = test::call_service(
        &app,
        apply_request(&token, &them, "inv_move", move_headline(&them, 1, 120.0)),
    )
    .await;
    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(
        first
            .headers()
            .get(INVOCATION_OUTCOME_HEADER)
            .expect("header"),
        "executed"
    );
    let first: Value = test::read_body_json(first).await;
    assert_eq!(first["version"], 2);

    // Without the ledger this is VERSION_CONFLICT: base_version 1 is stale now.
    let second = test::call_service(
        &app,
        apply_request(&token, &them, "inv_move", move_headline(&them, 1, 120.0)),
    )
    .await;
    assert_eq!(second.status(), StatusCode::OK);
    assert_eq!(
        second
            .headers()
            .get(INVOCATION_OUTCOME_HEADER)
            .expect("header"),
        "replayed"
    );
    let second: Value = test::read_body_json(second).await;
    assert_eq!(
        second, first,
        "the replay is the first answer, byte for byte"
    );

    assert_eq!(version_of(&state, &them).await, 2, "executed once");
    let history = state
        .designs
        .history(them.account, them.design)
        .await
        .expect("history");
    assert_eq!(history.len(), 2);
    let actor = history
        .iter()
        .find(|entry| entry.version.0 == 2)
        .and_then(|entry| entry.actor_id.clone())
        .expect("an actor");
    assert!(
        actor.starts_with("service:") && actor.ends_with(&them.user.to_string()),
        "{actor}"
    );
}

#[actix_web::test]
async fn a_different_request_under_the_same_invocation_is_refused() {
    let (state, _dir) = hosted().await;
    let them = person(&state, 1).await;
    let token = sidecar_token(&state).await;
    let app = serve!(state.clone());

    let first = test::call_service(
        &app,
        apply_request(&token, &them, "inv_once", move_headline(&them, 1, 120.0)),
    )
    .await;
    assert_eq!(first.status(), StatusCode::OK);

    let response = test::call_service(
        &app,
        apply_request(&token, &them, "inv_once", move_headline(&them, 2, 999.0)),
    )
    .await;
    refused(
        response,
        StatusCode::CONFLICT,
        "INVOCATION_REUSED",
        "invocation_already_completed",
    )
    .await;
    assert_eq!(version_of(&state, &them).await, 2, "nothing executed");
}

#[actix_web::test]
async fn a_command_without_an_invocation_id_is_refused() {
    let (state, _dir) = hosted().await;
    let them = person(&state, 1).await;
    let token = sidecar_token(&state).await;
    let app = serve!(state.clone());

    let response = test::call_service(
        &app,
        as_sidecar(
            TestRequest::post()
                .uri(&format!(
                    "/api/v1/pontis/designs/{}/operations",
                    them.design
                ))
                .set_json(move_headline(&them, 1, 120.0)),
            &token,
            &them,
        )
        .to_request(),
    )
    .await;
    refused(
        response,
        StatusCode::BAD_REQUEST,
        "INVOCATION_ID_REQUIRED",
        "validation_error",
    )
    .await;
}

#[actix_web::test]
async fn a_rejected_batch_settles_nothing_and_frees_its_invocation() {
    let (state, _dir) = hosted().await;
    let them = person(&state, 1).await;
    let token = sidecar_token(&state).await;
    let app = serve!(state.clone());

    // The design moves under the agent.
    let moved = test::call_service(
        &app,
        apply_request(&token, &them, "inv_other", move_headline(&them, 1, 10.0)),
    )
    .await;
    assert_eq!(moved.status(), StatusCode::OK);

    // Stale: refetch and rebase, never "give up".
    let response = test::call_service(
        &app,
        apply_request(&token, &them, "inv_rebase", move_headline(&them, 1, 120.0)),
    )
    .await;
    let body = refused(
        response,
        StatusCode::CONFLICT,
        "VERSION_CONFLICT",
        "conflict",
    )
    .await;
    assert_eq!(body["error"]["details"]["actual"], 2);

    // The claim was released, so the rebased batch runs under the same id.
    let response = test::call_service(
        &app,
        apply_request(&token, &them, "inv_rebase", move_headline(&them, 2, 120.0)),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(version_of(&state, &them).await, 3);

    // An invalid batch is the caller's to fix.
    let response = test::call_service(
        &app,
        apply_request(
            &token,
            &them,
            "inv_bad",
            json!({ "base_version": 3, "operations": [{ "op": "set_text", "layer_id": LayerId::new(), "content": "x" }] }),
        ),
    )
    .await;
    // §32's status stays; the registry code says whose mistake it is.
    refused(
        response,
        StatusCode::NOT_FOUND,
        "LAYER_NOT_FOUND",
        "validation_error",
    )
    .await;
}

#[actix_web::test]
async fn a_request_body_over_its_bound_is_refused() {
    let (state, _dir) = hosted().await;
    let them = person(&state, 1).await;
    let token = sidecar_token(&state).await;
    let app = serve!(state.clone());

    let huge = "x".repeat(forma_server::routes::pontis::MAX_REQUEST_BYTES + 1);
    let response = test::call_service(
        &app,
        apply_request(
            &token,
            &them,
            "inv_huge",
            json!({ "base_version": 1, "operations": [{ "op": "set_text", "layer_id": them.layer, "content": huge }] }),
        ),
    )
    .await;
    let body = refused(
        response,
        StatusCode::BAD_REQUEST,
        "INVALID_BODY",
        "validation_error",
    )
    .await;
    let message = body["error"]["message"].as_str().unwrap_or_default();
    assert!(message.contains("larger than allowed"), "{message}");
    assert_eq!(version_of(&state, &them).await, 1);
}

#[actix_web::test]
async fn an_in_flight_claim_is_never_executed_again() {
    let (state, _dir) = hosted().await;
    let them = person(&state, 1).await;
    let token = sidecar_token(&state).await;
    let app = serve!(state.clone());

    // The claim a crash would leave: written, never completed. Built the way
    // the route builds it, so the fingerprints agree.
    let body = move_headline(&them, 1, 120.0);
    let operations: Vec<DesignOperation> =
        serde_json::from_value(body["operations"].clone()).expect("operations");
    let ledger = SqliteInvocationLedger::new(state.pool.clone());
    let invocation = InvocationId::parse("inv_crashed").expect("an id");
    let claim = ledger
        .claim(
            them.account,
            &invocation,
            &InvocationRequest {
                represented_user: them.user,
                capability: "design.apply_operations".into(),
                design: Some(them.design),
                input: json!({ "base_version": 1, "operations": operations }),
            },
        )
        .await
        .expect("claim");
    assert_eq!(claim, persistence::Claim::Execute);

    let response = test::call_service(
        &app,
        apply_request(&token, &them, "inv_crashed", body.clone()),
    )
    .await;
    assert_eq!(response.headers().get("retry-after").expect("header"), "1");
    refused(
        response,
        StatusCode::CONFLICT,
        "INVOCATION_IN_FLIGHT",
        "conflict",
    )
    .await;

    // Old enough that it is not running: its outcome is unknown, and it still
    // does not run.
    sqlx::query("UPDATE invocation_ledger SET claimed_at = '2020-01-01T00:00:00.000Z'")
        .execute(&state.pool)
        .await
        .expect("age the claim");
    let response =
        test::call_service(&app, apply_request(&token, &them, "inv_crashed", body)).await;
    let body = refused(
        response,
        StatusCode::CONFLICT,
        "INVOCATION_OUTCOME_UNKNOWN",
        "conflict",
    )
    .await;
    assert_eq!(
        body["error"]["details"]["claimed_at"],
        "2020-01-01T00:00:00.000Z"
    );
    assert_eq!(version_of(&state, &them).await, 1, "never executed");
}

#[actix_web::test]
async fn a_replay_after_its_design_was_deleted_does_not_run_again() {
    let (state, _dir) = hosted().await;
    let them = person(&state, 1).await;
    let token = sidecar_token(&state).await;
    let app = serve!(state.clone());

    let response = test::call_service(
        &app,
        apply_request(&token, &them, "inv_gone", move_headline(&them, 1, 120.0)),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    state
        .designs
        .delete(them.account, them.design)
        .await
        .expect("delete");

    let response = test::call_service(
        &app,
        apply_request(&token, &them, "inv_gone", move_headline(&them, 1, 120.0)),
    )
    .await;
    refused(
        response,
        StatusCode::NOT_FOUND,
        "INVOCATION_TARGET_DELETED",
        "not_found",
    )
    .await;
}

// --- the same write, elevated by its input ------------------------------------

#[actix_web::test]
async fn a_removal_is_refused_without_consent_for_exactly_this_invocation() {
    let (state, _dir) = hosted().await;
    let them = person(&state, 1).await;
    let token = sidecar_token(&state).await;
    let app = serve!(state.clone());

    let removal = remove_headline(&them, 1);
    let attempts = [
        // No evidence at all: ask for it.
        (
            removal.clone(),
            StatusCode::PRECONDITION_REQUIRED,
            "APPROVAL_REQUIRED",
            "approval_required",
        ),
        // Consent at `write` does not reach `destructive`.
        (
            with_approval(removal.clone(), {
                let mut approval = approval("inv_remove");
                approval["risk"] = json!("write");
                approval
            }),
            StatusCode::PRECONDITION_REQUIRED,
            "APPROVAL_REQUIRED",
            "approval_required",
        ),
        // Real consent, for another invocation.
        (
            with_approval(removal.clone(), approval("inv_something_else")),
            StatusCode::FORBIDDEN,
            "APPROVAL_SCOPE_MISMATCH",
            "approval_scope_mismatch",
        ),
        // Real consent, for another capability.
        (
            with_approval(removal.clone(), {
                let mut approval = approval("inv_remove");
                approval["capability_id"] = json!("design.export");
                approval
            }),
            StatusCode::FORBIDDEN,
            "APPROVAL_SCOPE_MISMATCH",
            "approval_scope_mismatch",
        ),
        // A session grant is not what a removal takes.
        (
            with_approval(removal.clone(), {
                let mut approval = approval("inv_remove");
                approval["scope"] =
                    json!({ "kind": "session", "capability_ids": ["design.apply_operations"] });
                approval
            }),
            StatusCode::FORBIDDEN,
            "APPROVAL_SCOPE_MISMATCH",
            "approval_scope_mismatch",
        ),
        // Consent whose window has closed.
        (
            with_approval(removal.clone(), {
                let mut approval = approval("inv_remove");
                approval["expires_at"] =
                    json!((chrono::Utc::now() - chrono::Duration::seconds(1)).to_rfc3339());
                approval
            }),
            StatusCode::FORBIDDEN,
            "APPROVAL_EXPIRED",
            "approval_expired",
        ),
        // Labelling the request lower is not a way out: the class is decided
        // from the operations, and a field the caller invents is refused.
        (
            {
                let mut body = removal.clone();
                body["risk"] = json!("write");
                body
            },
            StatusCode::BAD_REQUEST,
            "INVALID_BODY",
            "validation_error",
        ),
    ];

    for (body, status, code, capability_error) in attempts {
        let response =
            test::call_service(&app, apply_request(&token, &them, "inv_remove", body)).await;
        refused(response, status, code, capability_error).await;
    }
    assert_eq!(version_of(&state, &them).await, 1, "nothing was removed");

    // None of those settled anything, so the same id now runs with consent.
    let response = test::call_service(
        &app,
        apply_request(
            &token,
            &them,
            "inv_remove",
            with_approval(removal, approval("inv_remove")),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(INVOCATION_OUTCOME_HEADER)
            .expect("header"),
        "executed"
    );
    let design = state
        .designs
        .get(them.account, them.design)
        .await
        .expect("design");
    assert_eq!(design.version.0, 2);
    assert!(
        design.document.pages[0].layers.is_empty(),
        "the layer is gone"
    );
}

#[actix_web::test]
async fn an_approved_removal_against_a_moved_design_is_a_state_change() {
    let (state, _dir) = hosted().await;
    let them = person(&state, 1).await;
    let token = sidecar_token(&state).await;
    let app = serve!(state.clone());

    let moved = test::call_service(
        &app,
        apply_request(&token, &them, "inv_edit", move_headline(&them, 1, 10.0)),
    )
    .await;
    assert_eq!(moved.status(), StatusCode::OK);

    // Consent was given against version 1, and version 1 is gone.
    let response = test::call_service(
        &app,
        apply_request(
            &token,
            &them,
            "inv_remove",
            with_approval(remove_headline(&them, 1), approval("inv_remove")),
        ),
    )
    .await;
    refused(
        response,
        StatusCode::CONFLICT,
        "VERSION_CONFLICT",
        "resource_state_changed",
    )
    .await;
    assert_eq!(version_of(&state, &them).await, 2);
}

// --- the cancellable read -----------------------------------------------------

fn validate_request(token: &str, person: &Person) -> TestRequest {
    as_sidecar(
        TestRequest::post().uri(&format!(
            "/api/v1/pontis/designs/{}/validate",
            person.design
        )),
        token,
        person,
    )
}

#[actix_web::test]
async fn a_validate_past_its_deadline_stops() {
    let (state, _dir) = hosted().await;
    // Enough pages that rendering them takes far longer than a millisecond.
    let them = person(&state, 12).await;
    let token = sidecar_token(&state).await;
    let app = serve!(state.clone());

    let response = test::call_service(&app, validate_request(&token, &them).to_request()).await;
    assert_eq!(response.status(), StatusCode::OK, "the positive control");
    let report: Value = test::read_body_json(response).await;
    assert!(report.is_object(), "{report}");

    let response = test::call_service(
        &app,
        validate_request(&token, &them)
            .insert_header((TIMEOUT_HEADER, "1"))
            .to_request(),
    )
    .await;
    refused(
        response,
        StatusCode::GATEWAY_TIMEOUT,
        "DEADLINE_EXCEEDED",
        "timeout",
    )
    .await;

    let response = test::call_service(
        &app,
        validate_request(&token, &them)
            .insert_header((TIMEOUT_HEADER, "3600000"))
            .to_request(),
    )
    .await;
    refused(
        response,
        StatusCode::BAD_REQUEST,
        "INVALID_TIMEOUT",
        "validation_error",
    )
    .await;
}

#[actix_web::test]
async fn validate_is_throttled_per_account() {
    let (state, _dir) = hosted().await;
    let them = person(&state, 1).await;
    let token = sidecar_token(&state).await;
    let app = serve!(state.clone());

    let held: Vec<_> = (0..MAX_CONCURRENT_VALIDATIONS)
        .map(|_| {
            state
                .pontis_validations
                .enter(them.account)
                .expect("a slot")
        })
        .collect();

    let response = test::call_service(&app, validate_request(&token, &them).to_request()).await;
    assert_eq!(response.headers().get("retry-after").expect("header"), "1");
    refused(
        response,
        StatusCode::TOO_MANY_REQUESTS,
        "RATE_LIMITED",
        "rate_limited",
    )
    .await;

    drop(held);
    let response = test::call_service(&app, validate_request(&token, &them).to_request()).await;
    assert_eq!(response.status(), StatusCode::OK);
}

// --- failures that are the server's -------------------------------------------

#[actix_web::test]
async fn a_corrupt_design_is_an_internal_error_with_no_detail() {
    let (state, _dir) = hosted().await;
    let them = person(&state, 1).await;
    let token = sidecar_token(&state).await;
    sqlx::query("UPDATE designs SET document_json = '{\"secret_path\": 1' WHERE id = ?")
        .bind(them.design.to_string())
        .execute(&state.pool)
        .await
        .expect("corrupt the row");
    let app = serve!(state.clone());

    let response = test::call_service(
        &app,
        as_sidecar(
            TestRequest::get().uri(&format!("/api/v1/pontis/designs/{}", them.design)),
            &token,
            &them,
        )
        .to_request(),
    )
    .await;
    let body = refused(
        response,
        StatusCode::INTERNAL_SERVER_ERROR,
        "INVALID_DOCUMENT",
        "internal_error",
    )
    .await;
    assert_eq!(
        body["error"]["message"],
        "the operation failed on the server"
    );
    assert!(body["error"].get("details").is_none(), "{body}");
}

/// Storage failing *after* authentication succeeded cannot be staged through
/// the real arm, which reads the same database first. So this one stands in
/// for the arm with the context it would have set, then closes the pool.
#[actix_web::test]
async fn storage_going_away_is_unavailable() {
    let (state, _dir) = hosted().await;
    let them = person(&state, 1).await;
    let context = AuthContext {
        account_id: them.account,
        user_id: Some(them.user),
        kind: AuthKind::Service,
        client_id: None,
        api_key_id: None,
        service_credential_id: Some(design_core::ServiceCredentialId::new()),
    };
    let app = test::init_service(
        App::new()
            .wrap_fn(move |request, service| {
                request.extensions_mut().insert(context.clone());
                service.call(request)
            })
            .app_data(web::Data::new(state.clone()))
            .configure(routes::configure),
    )
    .await;

    state.pool.close().await;
    let response = test::call_service(
        &app,
        TestRequest::get()
            .uri("/api/v1/pontis/projects")
            .to_request(),
    )
    .await;
    let body = refused(
        response,
        StatusCode::SERVICE_UNAVAILABLE,
        "STORAGE_ERROR",
        "unavailable",
    )
    .await;
    assert_eq!(
        body["error"]["message"],
        "the operation failed on the server"
    );
}

// --- what the bounds were measured against -------------------------------------
//
// The bounds of `agent_catalog::operations` carry the numbers these tests
// measure. They re-measure them on every run, so a change that makes a result
// heavier — a longer issue message, a new field on every layer — shows up here
// as lost headroom rather than as a real design refused in front of a person.

/// The brand of the real cover the bounds were measured against.
async fn with_brand(state: &AppState, person: &Person) {
    let draft = serde_json::from_value(json!({
        "name": "Cruce",
        "colors": {
            "primary": "#E8E4DA",
            "secondary": "#FDBA2D",
            "accent": "#FF5436",
            "background": "#101112",
            "surface": "#191919",
            "text": "#F4F1EA",
            "text_inverse": "#101010",
            "muted": "#A3A3A3"
        },
        "typography": { "display": "Geist", "body": "Geist", "mono": "Geist Mono" }
    }))
    .expect("a brand draft");
    application::projects::put_brand(&*state.projects, person.account, person.project, draft)
        .await
        .expect("brand");
}

fn text_layer(
    page: PageId,
    name: &str,
    (x, y, width, height): (f32, f32, f32, f32),
    text: &str,
    font_size: f32,
    color: &str,
) -> Value {
    json!({
        "op": "add_layer",
        "page_id": page,
        "layer": {
            "type": "text", "id": LayerId::new(), "name": name, "role": "body",
            "transform": { "x": x, "y": y, "width": width, "height": height },
            "text": text, "font_family": "Geist", "font_size": font_size, "font_weight": 400,
            "line_height": 1.1, "letter_spacing": 0.0, "align": "left", "color": color
        }
    })
}

fn shape_layer(page: PageId, name: &str, (x, y, width, height): (f32, f32, f32, f32)) -> Value {
    json!({
        "op": "add_layer",
        "page_id": page,
        "layer": {
            "type": "shape", "id": LayerId::new(), "name": name, "role": "decoration",
            "opacity": 0.6, "transform": { "x": x, "y": y, "width": width, "height": height },
            "kind": "rectangle", "fill": "$brand.background", "stroke_width": 0.0,
            "corner_radius": 0.0
        }
    })
}

/// The largest real design on a working Forma, layer for layer: a 1080x1350
/// cover with a photo, two scrims, four lines of type and an accent rule.
fn cover_page(page: PageId) -> Vec<Value> {
    vec![
        json!({
            "op": "add_layer",
            "page_id": page,
            "layer": {
                "type": "image", "id": LayerId::new(), "name": "Foto BA lluvia", "role": "photo",
                "transform": { "x": 0.0, "y": 0.0, "width": 1080.0, "height": 1350.0 },
                "asset_id": LayerId::new(), "fit": "cover", "focal_point": { "x": 0.5, "y": 0.5 }
            }
        }),
        shape_layer(page, "Scrim superior", (0.0, 0.0, 1080.0, 170.0)),
        shape_layer(page, "Scrim inferior", (0.0, 1100.0, 1080.0, 250.0)),
        text_layer(
            page,
            "Masthead",
            (96.0, 44.0, 584.0, 75.0),
            "CRUCE",
            42.0,
            "$brand.text",
        ),
        text_layer(
            page,
            "Titular",
            (98.0, 1097.0, 888.0, 180.0),
            "buenos aires\nbajo la lluvia",
            64.0,
            "$brand.text",
        ),
        shape_layer(page, "Acento sección", (96.0, 1292.0, 40.0, 4.0)),
        text_layer(
            page,
            "Sección y número",
            (150.0, 1288.0, 500.0, 36.0),
            "CIUDAD · N.º 07",
            24.0,
            "$brand.muted",
        ),
        text_layer(
            page,
            "Créditos de foto",
            (600.0, 1315.0, 384.0, 30.0),
            "Foto: M. Duarte",
            20.0,
            "$brand.muted",
        ),
    ]
}

/// A page denser than anything made yet: `layers` layers, every other one a
/// 300-character paragraph.
fn dense_page(page: PageId, layers: usize) -> Vec<Value> {
    let copy: String = "La garúa no moja, insiste. "
        .chars()
        .cycle()
        .take(300)
        .collect();
    let copy = copy
        .chars()
        .collect::<Vec<_>>()
        .chunks(40)
        .map(|line| line.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");
    (0..layers)
        .map(|index| {
            let y = 100.0 + index as f32 * 20.0;
            if index % 2 == 0 {
                text_layer(
                    page,
                    &format!("Párrafo {index}"),
                    (96.0, y, 888.0, 600.0),
                    &copy,
                    28.0,
                    "$brand.text",
                )
            } else {
                shape_layer(page, &format!("Barra {index}"), (96.0, y, 400.0, 12.0))
            }
        })
        .collect()
}

/// A page of `layers` text layers, each wrong in every way the validator
/// reports at once: off-brand colour, too small, outside the safe area, and
/// overflowing its frame both ways.
fn broken_page(page: PageId, layers: usize) -> Vec<Value> {
    (0..layers)
        .map(|index| {
            text_layer(
                page,
                &format!("Mal {index}"),
                (10.0 + index as f32, 1300.0, 60.0, 10.0),
                "TEXTO DEMASIADO LARGO PARA SU CAJA\nY CON OTRA LÍNEA MÁS",
                12.0,
                "#121313",
            )
        })
        .collect()
}

fn built(pages: usize, page: impl Fn(PageId) -> Vec<Value>) -> DesignDocument {
    let mut document =
        DesignDocument::new(DesignFormat::from_preset(FormatPreset::InstagramPortrait));
    let mut operations = Vec::new();
    for index in 0..pages {
        let new = Page::new(format!("Página {}", index + 1));
        operations.extend(page(new.id));
        document.pages.push(new);
    }
    for batch in operations.chunks(design_core::MAX_OPS_PER_BATCH) {
        let batch: Vec<DesignOperation> = batch
            .iter()
            .map(|operation| serde_json::from_value(operation.clone()).expect("an operation"))
            .collect();
        document = design_core::apply_operations(&document, &batch).expect("a valid document");
    }
    document
}

async fn stored(state: &AppState, person: &Person, document: &DesignDocument) -> DesignId {
    state
        .designs
        .create(
            person.account,
            person.project,
            "Portada BA bajo la lluvia",
            document,
            &Actor::human(),
        )
        .await
        .expect("design")
        .id
}

fn get_request(token: &str, person: &Person, design: DesignId) -> TestRequest {
    as_sidecar(
        TestRequest::get().uri(&format!("/api/v1/pontis/designs/{design}")),
        token,
        person,
    )
}

fn validate_of(token: &str, person: &Person, design: DesignId) -> TestRequest {
    as_sidecar(
        TestRequest::post().uri(&format!("/api/v1/pontis/designs/{design}/validate")),
        token,
        person,
    )
}

/// Bytes of a successful answer.
async fn answered(response: actix_web::dev::ServiceResponse) -> usize {
    assert_eq!(response.status(), StatusCode::OK);
    test::read_body(response).await.len()
}

/// How many times over `bytes` fits under `capability`'s declared bound.
fn headroom(capability: &str, bytes: usize) -> f64 {
    let max = bounds(capability).max_result_bytes as f64;
    eprintln!(
        "{capability}: {bytes} bytes against {max}, {:.2}x",
        max / bytes as f64
    );
    max / bytes as f64
}

#[actix_web::test]
async fn the_bounds_keep_the_headroom_they_were_chosen_with() {
    let (state, _dir) = hosted().await;
    let them = person(&state, 1).await;
    with_brand(&state, &them).await;
    let token = sidecar_token(&state).await;
    let app = serve!(state.clone());

    // design.get: a 20-page carousel at the real cover's density, and a far
    // denser stress case, both answered.
    let carousel = stored(&state, &them, &built(20, cover_page)).await;
    let bytes =
        answered(test::call_service(&app, get_request(&token, &them, carousel).to_request()).await)
            .await;
    assert!(
        headroom(DESIGN_GET, bytes) >= 4.0,
        "a real carousel is {bytes} bytes"
    );
    let dense = stored(&state, &them, &built(20, |page| dense_page(page, 20))).await;
    let bytes =
        answered(test::call_service(&app, get_request(&token, &them, dense).to_request()).await)
            .await;
    assert!(
        headroom(DESIGN_GET, bytes) >= 1.2,
        "the stress case is {bytes} bytes"
    );

    // design.validate: that carousel with every line of it broken.
    let broken = stored(&state, &them, &built(20, |page| broken_page(page, 8))).await;
    let response = test::call_service(&app, validate_of(&token, &them, broken).to_request()).await;
    assert_eq!(response.status(), StatusCode::OK);
    let report = test::read_body(response).await;
    let issues = serde_json::from_slice::<Value>(&report).expect("a report")["issues"]
        .as_array()
        .map_or(0, Vec::len);
    assert_eq!(issues, 20 * 8 * 5, "five issues on every broken layer");
    assert!(
        headroom(DESIGN_VALIDATE, report.len()) >= 1.9,
        "the broken carousel reports {} bytes",
        report.len()
    );
    // The reason the two differ: the report is far heavier than the design.
    let design =
        answered(test::call_service(&app, get_request(&token, &them, broken).to_request()).await)
            .await;
    assert!(
        report.len() > 3 * design,
        "{} against {design}",
        report.len()
    );

    // project.list: the item bound at full length, with long names.
    let max_items = bounds(PROJECT_LIST).max_list_items.expect("an item bound") as usize;
    for _ in 1..max_items {
        state
            .projects
            .create(
                them.account,
                "Campaña de otoño — piezas para redes y prensa (2026)",
            )
            .await
            .expect("project");
    }
    let response = test::call_service(
        &app,
        as_sidecar(
            TestRequest::get().uri("/api/v1/pontis/projects"),
            &token,
            &them,
        )
        .to_request(),
    )
    .await;
    let bytes = answered(response).await;
    assert!(
        headroom(PROJECT_LIST, bytes) >= 1.5,
        "{max_items} projects are {bytes} bytes"
    );

    // design.apply_operations answers four fields, whatever the design weighs.
    let response = test::call_service(
        &app,
        apply_request(&token, &them, "inv_measure", move_headline(&them, 1, 120.0)),
    )
    .await;
    let bytes = answered(response).await;
    assert!(bytes < 512, "{bytes}");
    headroom(forma_server::routes::pontis::APPLY_OPERATIONS, bytes);
}

/// The route enforces exactly what the catalog declares: a design answering at
/// its bound to the byte is returned, and one byte more is refused. Fails if
/// the route ever takes its bound from anywhere else.
#[actix_web::test]
async fn a_design_at_its_declared_bound_is_returned_and_one_byte_over_is_refused() {
    let (state, _dir) = hosted().await;
    let them = person(&state, 1).await;
    let token = sidecar_token(&state).await;
    let app = serve!(state.clone());
    let max_bytes = bounds(DESIGN_GET).max_result_bytes as usize;

    // Every field but the copy has a fixed width — ids, millisecond timestamps,
    // version 1 — so the answer grows by exactly one byte per ASCII character.
    let sized = |characters: usize| {
        let (mut document, layer) = document(1);
        let set_text: DesignOperation = serde_json::from_value(json!({
            "op": "set_text", "layer_id": layer, "content": "x".repeat(characters)
        }))
        .expect("a set_text operation");
        document = design_core::apply_operations(&document, &[set_text]).expect("valid");
        document
    };
    let probe = stored(&state, &them, &sized(1)).await;
    let base =
        answered(test::call_service(&app, get_request(&token, &them, probe).to_request()).await)
            .await;

    let at = stored(&state, &them, &sized(1 + max_bytes - base)).await;
    let bytes =
        answered(test::call_service(&app, get_request(&token, &them, at).to_request()).await).await;
    assert_eq!(
        bytes, max_bytes,
        "the design meant to sit exactly at the bound"
    );

    let over = stored(&state, &them, &sized(2 + max_bytes - base)).await;
    let response = test::call_service(&app, get_request(&token, &them, over).to_request()).await;
    let body = refused(
        response,
        StatusCode::UNPROCESSABLE_ENTITY,
        "RESULT_TOO_LARGE",
        "result_too_large",
    )
    .await;
    assert_eq!(body["error"]["details"]["max_bytes"], max_bytes);
    assert_eq!(body["error"]["details"]["bytes"], max_bytes + 1);
}

/// A report is bounded by its own declaration, not the design's: this design
/// is returned whole while its report is refused against validate's bound.
#[actix_web::test]
async fn a_report_over_its_declared_bound_is_refused_though_its_design_is_not() {
    let (state, _dir) = hosted().await;
    let them = person(&state, 1).await;
    with_brand(&state, &them).await;
    let token = sidecar_token(&state).await;
    let app = serve!(state.clone());

    let broken = stored(&state, &them, &built(20, |page| broken_page(page, 20))).await;
    let design =
        answered(test::call_service(&app, get_request(&token, &them, broken).to_request()).await)
            .await;
    assert!(design <= bounds(DESIGN_GET).max_result_bytes as usize);

    let response = test::call_service(&app, validate_of(&token, &them, broken).to_request()).await;
    let body = refused(
        response,
        StatusCode::UNPROCESSABLE_ENTITY,
        "RESULT_TOO_LARGE",
        "result_too_large",
    )
    .await;
    assert_eq!(
        body["error"]["details"]["max_bytes"],
        bounds(DESIGN_VALIDATE).max_result_bytes
    );
}
