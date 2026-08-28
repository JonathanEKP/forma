//! The Brand Kit endpoints.
//!
//! The kit's identity and version are the server's, so these check that a
//! client can say what the brand *is* without inventing a BrandId or guessing a
//! version number.

use std::sync::Arc;

use actix_web::{
    dev::ServiceResponse,
    http::StatusCode,
    test::{self, TestRequest},
    web, App,
};
use forma_server::{routes, state::AppState};
use renderer::FontRegistry;
use serde_json::{json, Value};

async fn state() -> (AppState, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("forma.db");
    let pool = persistence::connect(&format!("sqlite://{}", db_path.display()), dir.path())
        .await
        .expect("migrations must apply to a fresh database");

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
                .app_data(web::Data::new($state))
                .configure(routes::configure),
        )
        .await
    };
}

async fn body_json(response: ServiceResponse) -> Value {
    let bytes = test::read_body(response).await;
    serde_json::from_slice(&bytes).expect("a JSON body")
}

/// Only the fields a person actually decides.
fn draft(name: &str, primary: &str) -> Value {
    json!({
        "name": name,
        "colors": {
            "primary": primary,
            "secondary": "#FDBA2D",
            "accent": "#7C3AED",
            "background": "#101010",
            "surface": "#191919",
            "text": "#FFFFFF",
            "text_inverse": "#101010",
            "muted": "#A3A3A3"
        },
        "typography": { "display": "Geist", "body": "Geist", "mono": "Geist Mono" }
    })
}

async fn create_project(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = ServiceResponse,
        Error = actix_web::Error,
    >,
) -> String {
    let response = test::call_service(
        app,
        TestRequest::post()
            .uri("/api/v1/projects")
            .set_json(json!({ "name": "Brand test" }))
            .to_request(),
    )
    .await;
    body_json(response).await["id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[actix_web::test]
async fn a_kit_is_stored_without_the_client_minting_an_id() {
    let (state, _dir) = state().await;
    let app = app!(state);
    let project_id = create_project(&app).await;

    let response = test::call_service(
        &app,
        TestRequest::put()
            .uri(&format!("/api/v1/projects/{project_id}/brand"))
            .set_json(draft("Default", "#FF5436"))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let stored = body_json(response).await;
    // The server filled in everything the client did not send.
    assert!(stored["id"].as_str().is_some_and(|id| !id.is_empty()));
    assert_eq!(stored["project_id"], project_id);
    assert_eq!(stored["version"], 1);
    assert_eq!(stored["colors"]["primary"], "#FF5436");
    // Omitted optional fields fall back rather than failing the request.
    assert_eq!(stored["voice"], "");
    assert_eq!(stored["rules"]["safe_margin"], 80.0);

    // And it reads back.
    let response = test::call_service(
        &app,
        TestRequest::get()
            .uri(&format!("/api/v1/projects/{project_id}/brand"))
            .to_request(),
    )
    .await;
    assert_eq!(body_json(response).await["id"], stored["id"]);
}

/// Editing must not churn the id: the project points at the kit, and so may
/// anything else later.
#[actix_web::test]
async fn editing_a_kit_keeps_its_id_and_bumps_the_version() {
    let (state, _dir) = state().await;
    let app = app!(state);
    let project_id = create_project(&app).await;

    let first = body_json(
        test::call_service(
            &app,
            TestRequest::put()
                .uri(&format!("/api/v1/projects/{project_id}/brand"))
                .set_json(draft("Default", "#FF5436"))
                .to_request(),
        )
        .await,
    )
    .await;

    let second = body_json(
        test::call_service(
            &app,
            TestRequest::put()
                .uri(&format!("/api/v1/projects/{project_id}/brand"))
                .set_json(draft("Rebrand", "#00FF00"))
                .to_request(),
        )
        .await,
    )
    .await;

    assert_eq!(
        second["id"], first["id"],
        "the kit's identity must survive an edit"
    );
    assert_eq!(second["version"], 2);
    assert_eq!(second["name"], "Rebrand");
    assert_eq!(second["colors"]["primary"], "#00FF00");

    // The project still points at the same kit.
    let project = body_json(
        test::call_service(
            &app,
            TestRequest::get()
                .uri(&format!("/api/v1/projects/{project_id}"))
                .to_request(),
        )
        .await,
    )
    .await;
    assert_eq!(project["brand_id"], first["id"]);
}

#[actix_web::test]
async fn a_kit_cannot_be_stored_on_a_project_that_does_not_exist() {
    let (state, _dir) = state().await;
    let app = app!(state);

    let response = test::call_service(
        &app,
        TestRequest::put()
            .uri("/api/v1/projects/8ee0f1ac-0000-4000-8000-000000000000/brand")
            .set_json(draft("Orphan", "#FF5436"))
            .to_request(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        body_json(response).await["error"]["code"],
        "PROJECT_NOT_FOUND"
    );
}

#[actix_web::test]
async fn an_incomplete_kit_is_rejected() {
    let (state, _dir) = state().await;
    let app = app!(state);
    let project_id = create_project(&app).await;

    // Colours are not optional: a kit missing one cannot resolve its tokens.
    let response = test::call_service(
        &app,
        TestRequest::put()
            .uri(&format!("/api/v1/projects/{project_id}/brand"))
            .set_json(json!({ "name": "Half", "typography": { "display": "Geist", "body": "Geist", "mono": "Geist Mono" } }))
            .to_request(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
