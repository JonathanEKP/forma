//! Asset requests and exports over HTTP (plan sections 22, 23, 31 and 71).

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

const BOUNDARY: &str = "forma-test-boundary";

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
    serde_json::from_slice(&test::read_body(response).await).expect("JSON")
}

fn png() -> Vec<u8> {
    let mut pixmap = resvg::tiny_skia::Pixmap::new(4, 4).unwrap();
    pixmap.fill(resvg::tiny_skia::Color::from_rgba8(0xFF, 0x5A, 0x36, 0xFF));
    pixmap.encode_png().unwrap()
}

fn upload_request(project: &str, bytes: &[u8]) -> actix_http::Request {
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"file\"; filename=\"hero.png\"\r\n\r\n",
    );
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());

    TestRequest::post()
        .uri(&format!("/api/v1/projects/{project}/assets"))
        .insert_header((
            "content-type",
            format!("multipart/form-data; boundary={BOUNDARY}"),
        ))
        .set_payload(body)
        .to_request()
}

/// Every helper takes the initialised service by reference; the concrete type
/// actix builds is unnameable, so it stays a bound rather than an alias.
trait TestApp:
    actix_web::dev::Service<actix_http::Request, Response = ServiceResponse, Error = actix_web::Error>
{
}

impl<T> TestApp for T where
    T: actix_web::dev::Service<
        actix_http::Request,
        Response = ServiceResponse,
        Error = actix_web::Error,
    >
{
}

async fn a_project(app: &impl TestApp) -> String {
    body_json(
        test::call_service(
            app,
            TestRequest::post()
                .uri("/api/v1/projects")
                .set_json(json!({ "name": "Campaign" }))
                .to_request(),
        )
        .await,
    )
    .await["id"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn a_design(app: &impl TestApp, project: &str, name: &str) -> Value {
    body_json(
        test::call_service(
            app,
            TestRequest::post()
                .uri(&format!("/api/v1/projects/{project}/designs"))
                .set_json(json!({ "name": name }))
                .to_request(),
        )
        .await,
    )
    .await
}

/// The flow of plan section 23, end to end: the agent records what it needs,
/// the user uploads, the request closes.
#[actix_web::test]
async fn a_missing_image_becomes_a_request_the_user_resolves() {
    let (state, _dir) = state().await;
    let app = app!(state);
    let project = a_project(&app).await;
    let design = a_design(&app, &project, "Teaser").await;

    let created = test::call_service(
        &app,
        TestRequest::post()
            .uri(&format!("/api/v1/projects/{project}/asset-requests"))
            .insert_header(("x-forma-actor", "agent:claude"))
            .set_json(json!({
                "purpose": "hero",
                "description": "A wide product shot on a dark background",
                "suggested_prompt": "product photography, dark background, soft key light",
                "design_id": design["id"],
                "requested_width": 1080,
                "requested_height": 1350
            }))
            .to_request(),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);

    let request = body_json(created).await;
    assert_eq!(request["status"], "pending");
    assert_eq!(request["resolved_asset_id"], Value::Null);
    let request_id = request["id"].as_str().unwrap().to_string();

    // The user uploads the image.
    let asset = body_json(test::call_service(&app, upload_request(&project, &png())).await).await;
    let asset_id = asset["id"].as_str().unwrap().to_string();

    let resolved = test::call_service(
        &app,
        TestRequest::post()
            .uri(&format!("/api/v1/asset-requests/{request_id}/resolve"))
            .set_json(json!({ "asset_id": asset_id }))
            .to_request(),
    )
    .await;
    assert_eq!(resolved.status(), StatusCode::OK);

    let resolved = body_json(resolved).await;
    assert_eq!(resolved["status"], "resolved");
    assert_eq!(resolved["resolved_asset_id"], asset_id);
    assert!(resolved["resolved_at"].is_string());
}

/// A second resolve is the caller's mistake, not a server fault.
#[actix_web::test]
async fn resolving_twice_is_a_conflict_not_a_server_error() {
    let (state, _dir) = state().await;
    let app = app!(state);
    let project = a_project(&app).await;

    let request = body_json(
        test::call_service(
            &app,
            TestRequest::post()
                .uri(&format!("/api/v1/projects/{project}/asset-requests"))
                .set_json(json!({ "description": "anything" }))
                .to_request(),
        )
        .await,
    )
    .await;
    let request_id = request["id"].as_str().unwrap().to_string();

    let asset = body_json(test::call_service(&app, upload_request(&project, &png())).await).await;
    let asset_id = asset["id"].as_str().unwrap().to_string();

    let resolve = || {
        TestRequest::post()
            .uri(&format!("/api/v1/asset-requests/{request_id}/resolve"))
            .set_json(json!({ "asset_id": asset_id }))
            .to_request()
    };

    assert_eq!(
        test::call_service(&app, resolve()).await.status(),
        StatusCode::OK
    );

    let second = test::call_service(&app, resolve()).await;
    assert_ne!(
        second.status(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "a double resolve is the caller's error"
    );
    assert_eq!(second.status(), StatusCode::CONFLICT);
    assert_eq!(body_json(second).await["error"]["code"], "INVALID_STATE");
}

/// An asset from another project must not close a request.
#[actix_web::test]
async fn a_request_cannot_be_resolved_with_another_projects_asset() {
    let (state, _dir) = state().await;
    let app = app!(state);
    let mine = a_project(&app).await;
    let theirs = a_project(&app).await;

    let request = body_json(
        test::call_service(
            &app,
            TestRequest::post()
                .uri(&format!("/api/v1/projects/{mine}/asset-requests"))
                .set_json(json!({ "description": "a hero image" }))
                .to_request(),
        )
        .await,
    )
    .await;

    let outsider = body_json(test::call_service(&app, upload_request(&theirs, &png())).await).await;

    let response = test::call_service(
        &app,
        TestRequest::post()
            .uri(&format!(
                "/api/v1/asset-requests/{}/resolve",
                request["id"].as_str().unwrap()
            ))
            .set_json(json!({ "asset_id": outsider["id"] }))
            .to_request(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body_json(response).await["error"]["code"], "INVALID_OP");
}

#[actix_web::test]
async fn a_cancelled_request_stays_cancelled() {
    let (state, _dir) = state().await;
    let app = app!(state);
    let project = a_project(&app).await;

    let request = body_json(
        test::call_service(
            &app,
            TestRequest::post()
                .uri(&format!("/api/v1/projects/{project}/asset-requests"))
                .set_json(json!({ "description": "no longer needed" }))
                .to_request(),
        )
        .await,
    )
    .await;
    let id = request["id"].as_str().unwrap().to_string();

    let cancelled = test::call_service(
        &app,
        TestRequest::post()
            .uri(&format!("/api/v1/asset-requests/{id}/cancel"))
            .to_request(),
    )
    .await;
    assert_eq!(cancelled.status(), StatusCode::OK);
    assert_eq!(body_json(cancelled).await["status"], "cancelled");

    let again = test::call_service(
        &app,
        TestRequest::post()
            .uri(&format!("/api/v1/asset-requests/{id}/cancel"))
            .to_request(),
    )
    .await;
    assert_eq!(again.status(), StatusCode::CONFLICT);
}

/// Plan section 71: a single page carries the design's name, a set is numbered.
#[actix_web::test]
async fn a_single_page_export_downloads_as_a_named_png() {
    let (state, _dir) = state().await;
    let app = app!(state);
    let project = a_project(&app).await;
    let design = a_design(&app, &project, "Launch Teaser").await;
    let design_id = design["id"].as_str().unwrap().to_string();

    let created = test::call_service(
        &app,
        TestRequest::post()
            .uri(&format!("/api/v1/designs/{design_id}/exports"))
            .set_json(json!({ "scope": "single_page", "scale": 1.0 }))
            .to_request(),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);

    let body = body_json(created).await;
    let export = &body["export"];
    assert_eq!(export["scope"], "single_page");
    // The version is pinned, which is what makes the export reproducible.
    assert_eq!(export["design_version"], 1);
    assert_eq!(body["files"][0]["filename"], "launch-teaser-page-01.png");

    let id = export["id"].as_str().unwrap();
    let download = test::call_service(
        &app,
        TestRequest::get()
            .uri(&format!("/api/v1/exports/{id}/download"))
            .to_request(),
    )
    .await;
    assert_eq!(download.status(), StatusCode::OK);
    assert!(download
        .headers()
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("launch-teaser-page-01.png"));

    let png = test::read_body(download).await;
    let pixmap = resvg::tiny_skia::Pixmap::decode_png(&png).expect("a real PNG");
    assert_eq!((pixmap.width(), pixmap.height()), (1080, 1080));
}

#[actix_web::test]
async fn a_carousel_exports_one_numbered_file_per_page() {
    let (state, _dir) = state().await;
    let app = app!(state);
    let project = a_project(&app).await;
    let design = a_design(&app, &project, "Carousel").await;
    let design_id = design["id"].as_str().unwrap().to_string();

    // Two more pages, so the design has three.
    let response = test::call_service(
        &app,
        TestRequest::post()
            .uri(&format!("/api/v1/designs/{design_id}/operations"))
            .set_json(json!({
                "base_version": 1,
                "operations": [
                    { "op": "add_page", "page": { "id": uuid::Uuid::new_v4().to_string(), "name": "Two", "layers": [] } },
                    { "op": "add_page", "page": { "id": uuid::Uuid::new_v4().to_string(), "name": "Three", "layers": [] } }
                ]
            }))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let body = body_json(
        test::call_service(
            &app,
            TestRequest::post()
                .uri(&format!("/api/v1/designs/{design_id}/exports"))
                .set_json(json!({ "scope": "all_pages" }))
                .to_request(),
        )
        .await,
    )
    .await;

    let files: Vec<String> = body["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["filename"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(files, vec!["01.png", "02.png", "03.png"]);

    let id = body["export"]["id"].as_str().unwrap();

    // Without naming a file there is nothing single to send, and V1 has no ZIP.
    let bare = test::call_service(
        &app,
        TestRequest::get()
            .uri(&format!("/api/v1/exports/{id}/download"))
            .to_request(),
    )
    .await;
    assert_eq!(bare.status(), StatusCode::BAD_REQUEST);
    assert!(body_json(bare).await["error"]["hint"]
        .as_str()
        .unwrap()
        .contains("ZIP"));

    // Naming one works.
    let one = test::call_service(
        &app,
        TestRequest::get()
            .uri(&format!("/api/v1/exports/{id}/download?file=02.png"))
            .to_request(),
    )
    .await;
    assert_eq!(one.status(), StatusCode::OK);
    assert!(resvg::tiny_skia::Pixmap::decode_png(&test::read_body(one).await).is_ok());
}

/// The filename comes from a URL, so it must not walk out of the export.
#[actix_web::test]
async fn an_export_download_cannot_escape_its_own_directory() {
    let (state, _dir) = state().await;
    let app = app!(state);
    let project = a_project(&app).await;
    let design = a_design(&app, &project, "D").await;

    let body = body_json(
        test::call_service(
            &app,
            TestRequest::post()
                .uri(&format!(
                    "/api/v1/designs/{}/exports",
                    design["id"].as_str().unwrap()
                ))
                .set_json(json!({ "scope": "all_pages" }))
                .to_request(),
        )
        .await,
    )
    .await;
    let id = body["export"]["id"].as_str().unwrap();

    for attempt in ["../../forma.db", "..%2Fescape.png", "a/b.png"] {
        let response = test::call_service(
            &app,
            TestRequest::get()
                .uri(&format!("/api/v1/exports/{id}/download?file={attempt}"))
                .to_request(),
        )
        .await;
        assert_ne!(
            response.status(),
            StatusCode::OK,
            "{attempt:?} should not have been served"
        );
    }
}

/// Every record a caller can create must carry a real creation time.
///
/// Three repositories bound the caller's `created_at` rather than stamping
/// their own, so a use case that left the field empty persisted a blank string.
///
/// Each record is read back rather than taken from the create response. The
/// repositories return a struct they built in memory instead of re-reading the
/// row, so a create response can look right while the stored row is empty —
/// which is exactly how this went unnoticed.
#[actix_web::test]
async fn every_created_record_is_stamped_with_a_real_time() {
    let (state, _dir) = state().await;
    let app = app!(state);
    let project = a_project(&app).await;
    let design = a_design(&app, &project, "Stamped").await;
    let design_id = design["id"].as_str().unwrap().to_string();

    let created = |value: &Value| value["id"].as_str().unwrap().to_string();

    let asset =
        created(&body_json(test::call_service(&app, upload_request(&project, &png())).await).await);

    let request = created(
        &body_json(
            test::call_service(
                &app,
                TestRequest::post()
                    .uri(&format!("/api/v1/projects/{project}/asset-requests"))
                    .set_json(json!({ "description": "a hero image" }))
                    .to_request(),
            )
            .await,
        )
        .await,
    );

    let export_body = body_json(
        test::call_service(
            &app,
            TestRequest::post()
                .uri(&format!("/api/v1/designs/{design_id}/exports"))
                .set_json(json!({ "scope": "single_page" }))
                .to_request(),
        )
        .await,
    )
    .await;
    let export = export_body["export"]["id"].as_str().unwrap().to_string();

    let cases = [
        ("design", format!("/api/v1/designs/{design_id}"), false),
        ("asset", format!("/api/v1/assets/{asset}"), false),
        (
            "asset request",
            format!("/api/v1/asset-requests/{request}"),
            false,
        ),
        ("export", format!("/api/v1/exports/{export}"), true),
    ];

    for (what, uri, nested) in cases {
        let fetched =
            body_json(test::call_service(&app, TestRequest::get().uri(&uri).to_request()).await)
                .await;

        let record = if nested { &fetched["export"] } else { &fetched };
        let stamp = record["created_at"].as_str().unwrap_or_default();

        assert!(
            stamp.len() >= 20 && stamp.ends_with('Z'),
            "{what} stored no creation time: {stamp:?}"
        );
    }
}
