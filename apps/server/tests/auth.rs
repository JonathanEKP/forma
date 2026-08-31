//! Bearer auth and the bind guard (plan sections 57 and 70).

use std::sync::Arc;

use actix_web::{
    http::StatusCode,
    test::{self, TestRequest},
    web, App,
};
use forma_server::{auth::RemoteToken, config::Config, routes, state::AppState};
use renderer::FontRegistry;

const TOKEN: &str = "a-token-with-enough-entropy-to-be-worth-something";

async fn state() -> (AppState, tempfile::TempDir) {
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

    (AppState::new(pool, fonts, cache, store), dir)
}

macro_rules! guarded {
    ($state:expr) => {
        test::init_service(
            App::new()
                .wrap(actix_web::middleware::from_fn(
                    forma_server::auth::require_bearer,
                ))
                .app_data(web::Data::new($state))
                .app_data(web::Data::new(RemoteToken::new(TOKEN)))
                .configure(routes::configure),
        )
        .await
    };
}

#[actix_web::test]
async fn without_a_token_configured_the_api_is_open() {
    let (state, _dir) = state().await;
    let app = test::init_service(
        App::new()
            .wrap(actix_web::middleware::from_fn(
                forma_server::auth::require_bearer,
            ))
            .app_data(web::Data::new(state))
            .configure(routes::configure),
    )
    .await;

    let response = test::call_service(
        &app,
        TestRequest::get().uri("/api/v1/projects").to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[actix_web::test]
async fn a_configured_token_is_required() {
    let (state, _dir) = state().await;
    let app = guarded!(state);

    let anonymous = test::call_service(
        &app,
        TestRequest::get().uri("/api/v1/projects").to_request(),
    )
    .await;
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);

    let body: serde_json::Value =
        serde_json::from_slice(&test::read_body(anonymous).await).expect("the §32 envelope");
    assert_eq!(body["error"]["code"], "UNAUTHORIZED");

    let wrong = test::call_service(
        &app,
        TestRequest::get()
            .uri("/api/v1/projects")
            .insert_header(("authorization", "Bearer not-the-token"))
            .to_request(),
    )
    .await;
    assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);

    let right = test::call_service(
        &app,
        TestRequest::get()
            .uri("/api/v1/projects")
            .insert_header(("authorization", format!("Bearer {TOKEN}")))
            .to_request(),
    )
    .await;
    assert_eq!(right.status(), StatusCode::OK);
}

/// Clients differ on the scheme's case and that is not the part worth being
/// strict about.
#[actix_web::test]
async fn the_scheme_is_case_insensitive() {
    let (state, _dir) = state().await;
    let app = guarded!(state);

    let response = test::call_service(
        &app,
        TestRequest::get()
            .uri("/api/v1/projects")
            .insert_header(("authorization", format!("bearer {TOKEN}")))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

/// Caddy and any monitor have to reach liveness before anyone has configured a
/// credential, and it reports nothing about the workspace.
#[actix_web::test]
async fn health_answers_without_a_token() {
    let (state, _dir) = state().await;
    let app = guarded!(state);

    let response =
        test::call_service(&app, TestRequest::get().uri("/api/v1/health").to_request()).await;
    assert_eq!(response.status(), StatusCode::OK);
}

/// A mutation is guarded too, not only the reads. Obvious, and exactly the kind
/// of obvious thing that is wrong in half the codebases that have it.
#[actix_web::test]
async fn writing_is_guarded_as_well_as_reading() {
    let (state, _dir) = state().await;
    let app = guarded!(state);

    let response = test::call_service(
        &app,
        TestRequest::post()
            .uri("/api/v1/projects")
            .set_json(serde_json::json!({ "name": "Should not exist" }))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// The configuration that must not start: reachable from the network with
/// nothing in front of it (plan section 70).
#[test]
fn a_public_bind_without_a_token_refuses_to_start() {
    let config = |bind: &str, token: Option<&str>| Config {
        bind: bind.to_string(),
        data_dir: "data".into(),
        database_url: "sqlite://data/forma.db".to_string(),
        font_dir: "data/fonts".into(),
        log: "info".to_string(),
        web_dir: "apps/web/dist".into(),
        remote_token: token.map(str::to_string),
        auth_mode: Ok(forma_server::config::AuthMode::Local),
        supabase_url: None,
        supabase_issuer: None,
        supabase_jwks_url: None,
    };

    assert!(config("127.0.0.1:4765", None).binding_is_safe().is_ok());
    assert!(config("[::1]:4765", None).binding_is_safe().is_ok());
    assert!(config("0.0.0.0:4765", Some(TOKEN))
        .binding_is_safe()
        .is_ok());

    let refused = config("0.0.0.0:4765", None).binding_is_safe();
    assert!(refused.is_err(), "a public bind with no token must refuse");
    assert!(refused.unwrap_err().contains("FORMA_REMOTE_TOKEN"));

    assert!(
        config("203.0.113.10:4765", None).binding_is_safe().is_err(),
        "a routable address is not loopback"
    );
    // A hostname could resolve anywhere, so it is treated as remote rather than
    // guessed in the permissive direction.
    assert!(config("example.com:4765", None).binding_is_safe().is_err());
}

// ---------------------------------------------------------------------------
// Auth mode (plan sections 106.2 and 109.1)
// ---------------------------------------------------------------------------

fn hosted(issuer: Option<&str>, jwks: Option<&str>) -> Config {
    Config {
        bind: "127.0.0.1:4765".to_string(),
        data_dir: "data".into(),
        database_url: "sqlite://data/forma.db".to_string(),
        font_dir: "data/fonts".into(),
        log: "info".to_string(),
        web_dir: "apps/web/dist".into(),
        remote_token: None,
        auth_mode: Ok(forma_server::config::AuthMode::Supabase),
        supabase_url: None,
        supabase_issuer: issuer.map(str::to_string),
        supabase_jwks_url: jwks.map(str::to_string),
    }
}

/// The failure the old guard could no longer catch.
///
/// It rested on "loopback means only this machine can reach it", which stopped
/// being true when Caddy was put in front of loopback. In hosted mode the
/// question is not where we bind but whether identity is configured at all —
/// otherwise retiring `FORMA_REMOTE_TOKEN` after the legacy claim would bring
/// the deployment up serving everything unauthenticated.
#[test]
fn hosted_mode_without_identity_configuration_refuses_to_start() {
    let refused = hosted(None, None).binding_is_safe();
    assert!(refused.is_err(), "a hosted deployment needs a verifier");
    let reason = refused.unwrap_err();
    assert!(reason.contains("FORMA_SUPABASE_ISSUER"));
    assert!(reason.contains("FORMA_SUPABASE_JWKS_URL"));

    // Half-configured is still refused: one of the two is not enough.
    assert!(hosted(Some("https://x/auth/v1"), None)
        .binding_is_safe()
        .is_err());
    assert!(hosted(None, Some("https://x/jwks.json"))
        .binding_is_safe()
        .is_err());

    assert!(
        hosted(Some("https://x/auth/v1"), Some("https://x/jwks.json"))
            .binding_is_safe()
            .is_ok()
    );
}

/// A mode nobody can parse must not quietly become the permissive one.
#[test]
fn an_unreadable_auth_mode_refuses_to_start() {
    let mut config = hosted(Some("i"), Some("j"));
    config.auth_mode =
        Err("FORMA_AUTH_MODE is \"supabas\"; it must be \"local\" or \"supabase\"".into());

    let refused = config.binding_is_safe();
    assert!(refused.is_err());
    assert!(refused.unwrap_err().contains("FORMA_AUTH_MODE"));
}

/// The second line of the same defence: even if such a process started, it
/// refuses every request rather than allowing every request.
#[actix_web::test]
async fn a_hosted_process_with_no_verifier_refuses_everything() {
    let (state, _dir) = state().await;
    let app = test::init_service(
        App::new()
            .wrap(actix_web::middleware::from_fn(
                forma_server::auth::require_bearer,
            ))
            // Hosted, and no RemoteToken registered: in local mode this exact
            // shape is the open case.
            .app_data(web::Data::new(
                state.with_mode(forma_server::config::AuthMode::Supabase),
            ))
            .configure(routes::configure),
    )
    .await;

    let response = test::call_service(
        &app,
        TestRequest::get().uri("/api/v1/projects").to_request(),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "no credential configured must not mean no credential required"
    );

    // Liveness still answers, so a monitor and Caddy are not locked out.
    let health =
        test::call_service(&app, TestRequest::get().uri("/api/v1/health").to_request()).await;
    assert_eq!(health.status(), StatusCode::OK);
}

/// Local mode keeps exactly the behaviour it had: no token, no credential.
#[actix_web::test]
async fn local_mode_is_still_open() {
    let (state, _dir) = state().await;
    let app = test::init_service(
        App::new()
            .wrap(actix_web::middleware::from_fn(
                forma_server::auth::require_bearer,
            ))
            .app_data(web::Data::new(state))
            .configure(routes::configure),
    )
    .await;

    let response = test::call_service(
        &app,
        TestRequest::get().uri("/api/v1/projects").to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

/// Plan 109.3: the envelope had no 403 at all before this phase.
#[test]
fn the_envelope_can_say_forbidden() {
    use actix_web::ResponseError;

    let error = forma_server::error::ApiError::forbidden(
        "ACCOUNT_REQUIRED",
        "this identity has no Forma account",
        "Ask the owner of this deployment for an invite.",
    );
    assert_eq!(error.status_code(), StatusCode::FORBIDDEN);
    assert_eq!(error.code(), "ACCOUNT_REQUIRED");
}

/// The skill is public: an agent that cannot reach the workspace must still be
/// able to read how to use it (plan §117.3).
#[actix_web::test]
async fn the_skill_is_served_without_a_credential() {
    let (state, _dir) = state().await;
    let app = guarded!(state);

    let response = test::call_service(&app, TestRequest::get().uri("/skill").to_request()).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok()),
        Some("text/markdown; charset=utf-8"),
        "a client asked for the skill, not for a web page"
    );

    let body = String::from_utf8(test::read_body(response).await.to_vec()).expect("utf-8");
    assert!(
        body.starts_with("---"),
        "the route must serve the skill itself, front matter and all, not the app shell"
    );
    assert!(body.contains("apply_operations"));
}

/// Plan §110.1, stated as a test so it cannot be closed again by accident.
///
/// The bundle is public and the data is not. `/login` exists to be reached
/// before you have a credential, and Supabase sends a person to
/// `/oauth/consent` holding none at all — if the origin refuses those, the
/// hosted flow cannot start.
#[actix_web::test]
async fn the_bundle_is_public_and_the_workspace_is_not() {
    let (state, _dir) = state().await;
    let app = guarded!(state);

    for path in [
        "/",
        "/index.html",
        "/login",
        "/oauth/consent",
        "/agents",
        "/skill",
    ] {
        let response = test::call_service(&app, TestRequest::get().uri(path).to_request()).await;
        assert_ne!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{path} is what a visitor with no credential is for"
        );
    }

    for path in ["/api/v1/projects", "/api/v1/templates", "/api/v1/api-keys"] {
        let response = test::call_service(&app, TestRequest::get().uri(path).to_request()).await;
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{path} is the workspace and stays behind the credential"
        );
    }
}
