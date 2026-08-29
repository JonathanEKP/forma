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
