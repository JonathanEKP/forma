//! Phase 0 API tests. They exercise the real routes against a real temporary
//! SQLite, so the migration and the render path are both covered.

use std::sync::Arc;

use actix_web::{test, web, App};
use forma_server::{db, routes, state::AppState};
use renderer::FontRegistry;

async fn state() -> (AppState, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("forma.db");
    let pool = db::connect(&format!("sqlite://{}", db_path.display()), dir.path())
        .await
        .expect("migrations must apply to a fresh database");

    let font_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/fonts");
    let fonts = Arc::new(FontRegistry::from_dir(font_dir).expect("fonts"));

    (AppState { pool, fonts }, dir)
}

#[actix_web::test]
async fn health_reports_ok() {
    let (state, _dir) = state().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .configure(routes::configure),
    )
    .await;

    let request = test::TestRequest::get().uri("/api/v1/health").to_request();
    let body: serde_json::Value = test::call_and_read_body_json(&app, request).await;
    assert_eq!(body["status"], "ok");
}

#[actix_web::test]
async fn status_reports_schema_fonts_and_project_count() {
    let (state, _dir) = state().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .configure(routes::configure),
    )
    .await;

    let request = test::TestRequest::get().uri("/api/v1/status").to_request();
    let body: serde_json::Value = test::call_and_read_body_json(&app, request).await;

    assert_eq!(body["name"], "forma-server");
    assert_eq!(body["schema_version"], design_core::SCHEMA_VERSION);
    // Reading this proves the migration ran: the table has to exist.
    assert_eq!(body["projects"], 0);
    assert!(
        body["fonts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f == "DejaVu Sans"),
        "font catalog missing DejaVu Sans: {body}"
    );
}

#[actix_web::test]
async fn test_card_returns_a_real_png() {
    let (state, _dir) = state().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .configure(routes::configure),
    )
    .await;

    let request = test::TestRequest::post()
        .uri("/api/v1/render/test-card")
        .set_json(serde_json::json!({ "label": "from the test suite" }))
        .to_request();

    let response = test::call_service(&app, request).await;
    assert!(response.status().is_success());
    assert_eq!(response.headers().get("content-type").unwrap(), "image/png");
    assert_eq!(
        response.headers().get("x-forma-canvas").unwrap(),
        "1080x1080"
    );

    let png = test::read_body(response).await;
    assert_eq!(&png[1..4], b"PNG");

    let pixmap = resvg::tiny_skia::Pixmap::decode_png(&png).expect("decodable PNG");
    assert_eq!((pixmap.width(), pixmap.height()), (1080, 1080));
}

#[actix_web::test]
async fn test_card_works_without_a_body() {
    let (state, _dir) = state().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .configure(routes::configure),
    )
    .await;

    let request = test::TestRequest::post()
        .uri("/api/v1/render/test-card")
        .to_request();
    let response = test::call_service(&app, request).await;
    assert!(
        response.status().is_success(),
        "status: {}",
        response.status()
    );
}
