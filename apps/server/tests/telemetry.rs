//! What the tracing middleware does that a client can observe.
//!
//! The log lines themselves are checked by reading them; what is worth pinning
//! in a test is the correlation id, because a client quoting an id in a bug
//! report only helps if the id it saw is the id in the log.

use std::sync::Arc;

use actix_web::{
    http::StatusCode,
    middleware,
    test::{self, TestRequest},
    web, App,
};
use forma_server::{routes, state::AppState, telemetry};
use renderer::FontRegistry;

async fn state() -> (AppState, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("forma.db");
    let pool = persistence::connect(&format!("sqlite://{}", db_path.display()), dir.path())
        .await
        .expect("migrations");

    let font_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/fonts");
    let fonts = Arc::new(FontRegistry::from_dir(font_dir).expect("fonts"));
    let cache = Arc::new(renderer::RenderCache::new(dir.path().join("previews")));
    let store = Arc::new(persistence::LocalObjectStore::new(dir.path()));

    (AppState::new(pool, fonts, cache, store), dir)
}

macro_rules! app {
    ($state:expr) => {
        test::init_service(
            App::new()
                .wrap(middleware::from_fn(telemetry::trace_request))
                .app_data(web::Data::new($state))
                .configure(routes::configure),
        )
        .await
    };
}

#[actix_web::test]
async fn every_response_carries_a_request_id() {
    let (state, _dir) = state().await;
    let app = app!(state);

    let response =
        test::call_service(&app, TestRequest::get().uri("/api/v1/status").to_request()).await;

    assert_eq!(response.status(), StatusCode::OK);
    let id = response
        .headers()
        .get("x-request-id")
        .expect("a response must carry the id its log line used")
        .to_str()
        .unwrap();
    assert!(uuid::Uuid::parse_str(id).is_ok(), "not a UUID: {id}");
}

/// A client-supplied id wins, so one trace survives the hop from forma-mcp.
#[actix_web::test]
async fn a_supplied_request_id_is_kept() {
    let (state, _dir) = state().await;
    let app = app!(state);

    let response = test::call_service(
        &app,
        TestRequest::get()
            .uri("/api/v1/status")
            .insert_header(("x-request-id", "mcp-correlation-1"))
            .to_request(),
    )
    .await;

    assert_eq!(
        response.headers().get("x-request-id").unwrap(),
        "mcp-correlation-1"
    );
}

/// A header long enough to be an attack, or empty enough to be useless, is
/// replaced rather than logged.
#[actix_web::test]
async fn an_unusable_request_id_is_replaced() {
    let (state, _dir) = state().await;
    let app = app!(state);

    for supplied in ["", "   ", &"x".repeat(500)] {
        let response = test::call_service(
            &app,
            TestRequest::get()
                .uri("/api/v1/status")
                .insert_header(("x-request-id", supplied))
                .to_request(),
        )
        .await;

        let id = response
            .headers()
            .get("x-request-id")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(
            uuid::Uuid::parse_str(id).is_ok(),
            "an unusable id should have been replaced, got {id:?}"
        );
    }
}

/// A rejected request is still traced: those are the ones worth finding.
#[actix_web::test]
async fn a_failed_request_is_still_correlated() {
    let (state, _dir) = state().await;
    let app = app!(state);

    let response = test::call_service(
        &app,
        TestRequest::get()
            .uri("/api/v1/designs/not-a-uuid")
            .to_request(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(response.headers().get("x-request-id").is_some());
}
