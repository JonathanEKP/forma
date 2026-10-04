//! The Pontis sidecar's credential at Boundary F (Pontis plan §10(b) D1).
//!
//! A dedicated class with its own prefix, so it can never be read as a PAT or
//! a PAT as it. Two of these tests are what stop it being worse than the PAT
//! reuse it replaced: the credential alone is not account access, and it never
//! opens the MCP door.

use std::sync::Arc;

use actix_web::{
    http::StatusCode,
    test::{self, TestRequest},
    web, App,
};
use design_core::{AccountId, AccountRole, AuthUserId};
use forma_server::auth::{APPLICATION_CONTEXT_HEADER, MCP_ENTRY_HEADER, REPRESENTED_USER_HEADER};
use forma_server::config::AuthMode;
use forma_server::{routes, state::AppState};
use persistence::{
    AccountRepository, ApiKeyRepository, ProjectRepository, ServiceCredentialRepository,
};
use renderer::FontRegistry;

/// A hosted deployment, which is where the sidecar runs. No identity provider
/// is configured: nothing here goes near the session arm.
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

fn bearer(token: &str) -> (&'static str, String) {
    ("authorization", format!("Bearer {token}"))
}

/// A person with an account holding one project, and a sidecar credential.
struct World {
    user: AuthUserId,
    account: AccountId,
    token: String,
}

async fn world(state: &AppState) -> World {
    let accounts = &state.accounts;
    let account = accounts.create("Theirs").await.expect("account").id;
    let user = AuthUserId::new();
    accounts
        .grant(account, user, "someone@example.com", AccountRole::Owner)
        .await
        .expect("grant");
    state
        .projects
        .create(account, "Their project")
        .await
        .expect("project");

    let token = state
        .service_credentials
        .create("pontis sidecar")
        .await
        .expect("mint")
        .token;
    assert!(token.starts_with(persistence::SERVICE_PREFIX));

    World {
        user,
        account,
        token,
    }
}

fn projects(world: &World) -> TestRequest {
    TestRequest::get()
        .uri("/api/v1/pontis/projects")
        .insert_header(bearer(&world.token))
        .insert_header((REPRESENTED_USER_HEADER, world.user.to_string()))
        .insert_header((APPLICATION_CONTEXT_HEADER, world.account.to_string()))
}

/// The positive control every refusal below is measured against.
#[actix_web::test]
async fn a_represented_user_in_their_own_context_is_admitted() {
    let (state, _dir) = hosted().await;
    let world = world(&state).await;
    let app = serve!(state);

    let response = test::call_service(&app, projects(&world).to_request()).await;
    assert_eq!(response.status(), StatusCode::OK);

    let listed: serde_json::Value = test::read_body_json(response).await;
    let names: Vec<_> = listed
        .as_array()
        .expect("a list")
        .iter()
        .map(|project| project["name"].as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(
        names,
        ["Their project"],
        "the context's projects, and only those"
    );
}

/// Mandatory (D1). Without a represented user and an application context the
/// credential names nobody and no workspace, and must not become access to one.
#[actix_web::test]
async fn the_credential_alone_is_not_account_access() {
    let (state, _dir) = hosted().await;
    let world = world(&state).await;
    let app = serve!(state);

    let bare = TestRequest::get()
        .uri("/api/v1/pontis/projects")
        .insert_header(bearer(&world.token))
        .to_request();
    assert_eq!(
        test::call_service(&app, bare).await.status(),
        StatusCode::UNAUTHORIZED,
        "a service credential with no represented user and no context"
    );

    let user_only = TestRequest::get()
        .uri("/api/v1/pontis/projects")
        .insert_header(bearer(&world.token))
        .insert_header((REPRESENTED_USER_HEADER, world.user.to_string()))
        .to_request();
    assert_eq!(
        test::call_service(&app, user_only).await.status(),
        StatusCode::UNAUTHORIZED,
        "a represented user is not enough without the context"
    );

    let context_only = TestRequest::get()
        .uri("/api/v1/pontis/projects")
        .insert_header(bearer(&world.token))
        .insert_header((APPLICATION_CONTEXT_HEADER, world.account.to_string()))
        .to_request();
    assert_eq!(
        test::call_service(&app, context_only).await.status(),
        StatusCode::UNAUTHORIZED,
        "a context is not enough without a user"
    );
}

/// Mandatory (D1). `forma-mcp` marks every request it forwards, and `admits()`
/// probes `/api/v1/status` with the host's bearer and nothing else. A sidecar
/// credential leaked to an MCP host must not get through that door — not even
/// with a represented user and context it has somehow learned.
#[actix_web::test]
async fn the_credential_never_opens_the_mcp_door() {
    let (state, _dir) = hosted().await;
    let world = world(&state).await;
    let app = serve!(state);

    let marked = projects(&world)
        .insert_header((MCP_ENTRY_HEADER, "mcp"))
        .to_request();
    assert_eq!(
        test::call_service(&app, marked).await.status(),
        StatusCode::UNAUTHORIZED,
        "a complete service request is refused once it is marked as arriving through MCP"
    );

    // Exactly what `ServerClient::admits` sends.
    let probe = TestRequest::get()
        .uri("/api/v1/status")
        .insert_header(bearer(&world.token))
        .insert_header((MCP_ENTRY_HEADER, "mcp"))
        .to_request();
    assert!(
        !test::call_service(&app, probe).await.status().is_success(),
        "the admission probe must not succeed"
    );
}

/// ADR-024: membership is revalidated on every request, so naming a context
/// the user does not belong to is refused rather than trusted.
#[actix_web::test]
async fn a_context_the_user_does_not_belong_to_is_forbidden() {
    let (state, _dir) = hosted().await;
    let world = world(&state).await;
    let elsewhere = state.accounts.create("Somebody else").await.unwrap().id;
    state
        .projects
        .create(elsewhere, "Not theirs")
        .await
        .unwrap();
    let app = serve!(state);

    let response = test::call_service(
        &app,
        TestRequest::get()
            .uri("/api/v1/pontis/projects")
            .insert_header(bearer(&world.token))
            .insert_header((REPRESENTED_USER_HEADER, world.user.to_string()))
            .insert_header((APPLICATION_CONTEXT_HEADER, elsewhere.to_string()))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body: serde_json::Value = test::read_body_json(response).await;
    assert_eq!(body["error"]["code"], "CONTEXT_FORBIDDEN");

    // And a user with no membership at all, in a real context.
    let stranger = test::call_service(
        &app,
        TestRequest::get()
            .uri("/api/v1/pontis/projects")
            .insert_header(bearer(&world.token))
            .insert_header((REPRESENTED_USER_HEADER, AuthUserId::new().to_string()))
            .insert_header((APPLICATION_CONTEXT_HEADER, world.account.to_string()))
            .to_request(),
    )
    .await;
    assert_eq!(stranger.status(), StatusCode::FORBIDDEN);
}

#[actix_web::test]
async fn a_revoked_credential_is_refused_with_its_own_code() {
    let (state, _dir) = hosted().await;
    let world = world(&state).await;
    let id = state
        .service_credentials
        .authenticate(&world.token)
        .await
        .unwrap()
        .expect("the credential")
        .id;
    state.service_credentials.revoke(id).await.expect("revoke");
    let app = serve!(state);

    let response = test::call_service(&app, projects(&world).to_request()).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body: serde_json::Value = test::read_body_json(response).await;
    assert_eq!(body["error"]["code"], "SERVICE_CREDENTIAL_REVOKED");
}

/// The prefix is the class. A PAT carrying the represented-user headers stays a
/// PAT, scoped to its own account; and a service secret relabelled with the
/// PAT prefix is no PAT at all.
#[actix_web::test]
async fn the_two_classes_cannot_be_confused() {
    let (state, _dir) = hosted().await;
    let world = world(&state).await;

    let other = state.accounts.create("PAT owner").await.unwrap().id;
    state.projects.create(other, "PAT project").await.unwrap();
    let pat = state
        .api_keys
        .create(other, None, "a script")
        .await
        .unwrap()
        .token;
    let relabelled = world
        .token
        .replacen(persistence::SERVICE_PREFIX, persistence::PAT_PREFIX, 1);
    let app = serve!(state);

    let response = test::call_service(
        &app,
        TestRequest::get()
            .uri("/api/v1/projects")
            .insert_header(bearer(&pat))
            .insert_header((REPRESENTED_USER_HEADER, world.user.to_string()))
            .insert_header((APPLICATION_CONTEXT_HEADER, world.account.to_string()))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let listed: serde_json::Value = test::read_body_json(response).await;
    assert_eq!(
        listed[0]["name"], "PAT project",
        "a PAT ignores the service headers and keeps its own account"
    );
    assert_eq!(listed.as_array().map(Vec::len), Some(1));

    let response = test::call_service(
        &app,
        TestRequest::get()
            .uri("/api/v1/projects")
            .insert_header(bearer(&relabelled))
            .insert_header((REPRESENTED_USER_HEADER, world.user.to_string()))
            .insert_header((APPLICATION_CONTEXT_HEADER, world.account.to_string()))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// No REST route mints, lists or revokes one: an account owner must not be
/// able to create a credential that reaches every account. Asked with a PAT,
/// because a service credential is now refused before routing can say anything.
#[actix_web::test]
async fn service_credentials_are_not_managed_over_rest() {
    let (state, _dir) = hosted().await;
    let world = world(&state).await;
    let pat = state
        .api_keys
        .create(world.account, None, "a script")
        .await
        .unwrap()
        .token;
    let app = serve!(state);

    let attempt = test::call_service(
        &app,
        TestRequest::get()
            .uri("/api/v1/service-credentials")
            .insert_header(bearer(&pat))
            .to_request(),
    )
    .await;
    assert_eq!(attempt.status(), StatusCode::NOT_FOUND);
}

/// A request as the sidecar would make it, for any method and path.
fn service_request(world: &World, request: TestRequest) -> TestRequest {
    request
        .insert_header(bearer(&world.token))
        .insert_header((REPRESENTED_USER_HEADER, world.user.to_string()))
        .insert_header((APPLICATION_CONTEXT_HEADER, world.account.to_string()))
}

async fn code_of(
    response: actix_web::dev::ServiceResponse<impl actix_web::body::MessageBody>,
) -> String {
    let body: serde_json::Value = test::read_body_json(response).await;
    body["error"]["code"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

/// F-50-1. The credential is worth Boundary F and nothing else: with a
/// perfectly valid represented user and context it still reaches no ordinary
/// REST route — not a read, not a write, and above all not the one that mints
/// a personal access token that would outlive it.
#[actix_web::test]
async fn the_credential_is_refused_outside_boundary_f() {
    let (state, _dir) = hosted().await;
    let world = world(&state).await;
    let app = serve!(state.clone());

    let read = test::call_service(
        &app,
        service_request(&world, TestRequest::get().uri("/api/v1/projects")).to_request(),
    )
    .await;
    assert_eq!(read.status(), StatusCode::FORBIDDEN, "a normal REST read");
    assert_eq!(code_of(read).await, "SERVICE_CREDENTIAL_OUT_OF_SCOPE");

    let write = test::call_service(
        &app,
        service_request(&world, TestRequest::post().uri("/api/v1/projects"))
            .set_json(serde_json::json!({ "name": "throwaway" }))
            .to_request(),
    )
    .await;
    assert_eq!(write.status(), StatusCode::FORBIDDEN, "a normal REST write");
    assert_eq!(code_of(write).await, "SERVICE_CREDENTIAL_OUT_OF_SCOPE");

    let mint = test::call_service(
        &app,
        service_request(&world, TestRequest::post().uri("/api/v1/api-keys"))
            .set_json(serde_json::json!({ "name": "probe" }))
            .to_request(),
    )
    .await;
    assert_eq!(mint.status(), StatusCode::FORBIDDEN, "minting a PAT");
    assert_eq!(code_of(mint).await, "SERVICE_CREDENTIAL_OUT_OF_SCOPE");

    // Refused is not the same as ignored: nothing was created behind the 403.
    let projects = state.projects.list(world.account).await.expect("list");
    assert_eq!(projects.len(), 1, "the refused write created no project");
    assert!(
        state
            .api_keys
            .list(world.account)
            .await
            .expect("keys")
            .is_empty(),
        "the refused request minted no personal access token"
    );
}

/// The check is on the normalised path, so no spelling of one outside
/// Boundary F is let through by starting with the right words.
#[actix_web::test]
async fn no_spelling_of_the_path_routes_around_the_scope() {
    let (state, _dir) = hosted().await;
    let world = world(&state).await;
    let app = serve!(state);

    for path in [
        "/api/v1/pontis/../projects",
        "/api/v1/pontis/./../projects",
        "/api/v1/pontis/%2e%2e/projects",
        "/api/v1/pontis/%2E%2E/projects",
        "/api/v1/pontis%2F..%2Fprojects",
        "/api/v1/pontis%2fprojects",
        "/api/v1/pontis/..%5Cprojects",
        "/api/v1/PONTIS/projects",
        "/api/v1/pontis",
        "/api/v1/pontisx/projects",
        "/api/v1//projects",
        "/api/v1/pontis//projects",
        "/api/v1/service/resolve/../../projects",
        "/api/v1/service/resolve/",
        "/api/v1/service/%72esolve",
        "/api/v1/service/resolvex",
        "/api/v1/pontis/%zz",
        "/api/v1/pontis/%ff",
    ] {
        let response = test::call_service(
            &app,
            service_request(&world, TestRequest::get().uri(path)).to_request(),
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "{path} is not Boundary F"
        );
        assert_eq!(
            code_of(response).await,
            "SERVICE_CREDENTIAL_OUT_OF_SCOPE",
            "{path}"
        );
    }
}

/// The 403 must not tell a stranger that a guessed credential is real: an
/// unknown one is a 401 on every path, in or out of scope.
#[actix_web::test]
async fn an_unknown_credential_learns_nothing_from_the_scope() {
    let (state, _dir) = hosted().await;
    let world = world(&state).await;
    let app = serve!(state);

    let forged = format!("{}not-a-real-secret", persistence::SERVICE_PREFIX);
    for path in [
        "/api/v1/projects",
        "/api/v1/api-keys",
        "/api/v1/pontis/projects",
    ] {
        let response = test::call_service(
            &app,
            TestRequest::get()
                .uri(path)
                .insert_header(bearer(&forged))
                .insert_header((REPRESENTED_USER_HEADER, world.user.to_string()))
                .insert_header((APPLICATION_CONTEXT_HEADER, world.account.to_string()))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
        assert_eq!(code_of(response).await, "UNAUTHORIZED", "{path}");
    }
}

/// The other half: what the sidecar actually calls keeps working, including a
/// spelling that percent-encodes an id the way `encodeURIComponent` would.
#[actix_web::test]
async fn boundary_f_is_still_open_to_the_credential() {
    let (state, _dir) = hosted().await;
    let world = world(&state).await;
    let app = serve!(state);

    for path in [
        "/api/v1/pontis/projects".to_string(),
        "/api/v1/pontis/guide".to_string(),
        format!("/api/v1/pontis/designs/{}", uuid::Uuid::new_v4()),
    ] {
        let response = test::call_service(
            &app,
            service_request(&world, TestRequest::get().uri(&path)).to_request(),
        )
        .await;
        assert_ne!(response.status(), StatusCode::FORBIDDEN, "{path}");
        assert_ne!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
    }

    let listed = test::call_service(
        &app,
        service_request(&world, TestRequest::get().uri("/api/v1/pontis/projects")).to_request(),
    )
    .await;
    assert_eq!(listed.status(), StatusCode::OK);
}
