//! Phase 5 acceptance: every main operation works over HTTP, with no frontend.
//!
//! These drive the real routes against a real temporary SQLite and the real
//! renderer, so a green run here means the surface an agent or the editor talks
//! to actually works end to end — not that the handlers compile.

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

/// The whole documented lifecycle in one pass, because the interesting failures
/// are in the seams between the calls, not inside any one of them.
#[actix_web::test]
async fn a_design_can_be_created_edited_validated_and_rendered_over_http() {
    let (state, _dir) = state().await;
    let app = app!(state);

    // --- project -----------------------------------------------------------
    let response = test::call_service(
        &app,
        TestRequest::post()
            .uri("/api/v1/projects")
            .set_json(json!({ "name": "Launch campaign" }))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let project = body_json(response).await;
    let project_id = project["id"].as_str().unwrap().to_string();

    // --- design ------------------------------------------------------------
    let response = test::call_service(
        &app,
        TestRequest::post()
            .uri(&format!("/api/v1/projects/{project_id}/designs"))
            .set_json(json!({
                "name": "Teaser",
                "format": { "preset": "instagram_portrait", "width": 1080.0, "height": 1350.0 },
            }))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let design = body_json(response).await;
    let design_id = design["id"].as_str().unwrap().to_string();
    assert_eq!(design["version"], 1);

    // --- a batch of operations, recorded as an agent ------------------------
    let page_id = uuid::Uuid::new_v4().to_string();
    let layer_id = uuid::Uuid::new_v4().to_string();

    let response = test::call_service(
        &app,
        TestRequest::post()
            .uri(&format!("/api/v1/designs/{design_id}/operations"))
            .insert_header(("x-forma-actor", "agent:claude"))
            .set_json(json!({
                "base_version": 1,
                "operations": [
                    { "op": "add_page", "page": { "id": page_id, "name": "Slide 1", "layers": [] } },
                    {
                        "op": "add_layer",
                        "page_id": page_id,
                        "layer": {
                            "type": "text",
                            "id": layer_id,
                            "name": "Headline",
                            "transform": { "x": 80.0, "y": 160.0, "width": 920.0, "height": 400.0 },
                            "text": "BUILD\nWITH AI",
                            "font_family": "Geist",
                            "font_size": 120.0,
                            "font_weight": 700,
                            "color": "#F4F1EA"
                        }
                    }
                ]
            }))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK, "operations must apply");
    let updated = body_json(response).await;

    // One batch, one version (plan section 28).
    assert_eq!(updated["version"], 2);
    // The blank design already had a page; the batch added the second.
    assert_eq!(updated["document"]["pages"].as_array().unwrap().len(), 2);

    // --- history remembers who did it (plan section 61) --------------------
    let response = test::call_service(
        &app,
        TestRequest::get()
            .uri(&format!("/api/v1/designs/{design_id}/history"))
            .to_request(),
    )
    .await;
    let history = body_json(response).await;
    let entries = history.as_array().unwrap();
    assert_eq!(entries.len(), 2, "one entry per version: {history}");
    assert_eq!(entries[0]["version"], 2);
    assert_eq!(entries[0]["actor_type"], "agent");
    assert_eq!(entries[0]["actor_id"], "agent:claude");
    assert_eq!(entries[1]["actor_type"], "human");

    // --- validate against real measurements --------------------------------
    let response = test::call_service(
        &app,
        TestRequest::post()
            .uri(&format!("/api/v1/designs/{design_id}/validate"))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let report = body_json(response).await;
    assert!(report["issues"].is_array(), "a report has issues: {report}");

    // --- render ------------------------------------------------------------
    let response = test::call_service(
        &app,
        TestRequest::get()
            .uri(&format!(
                "/api/v1/designs/{design_id}/pages/{page_id}/preview.png"
            ))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get("content-type").unwrap(), "image/png");
    assert_eq!(response.headers().get("x-forma-version").unwrap(), "2");

    let png = test::read_body(response).await;
    let pixmap = resvg::tiny_skia::Pixmap::decode_png(&png).expect("a decodable PNG");
    assert_eq!((pixmap.width(), pixmap.height()), (1080, 1350));

    // --- the SVG preview is the normalized tree the editor draws -----------
    let response = test::call_service(
        &app,
        TestRequest::get()
            .uri(&format!(
                "/api/v1/designs/{design_id}/pages/{page_id}/preview.svg"
            ))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let svg = String::from_utf8(test::read_body(response).await.to_vec()).unwrap();
    assert!(svg.starts_with("<svg"), "{svg:.120}");
    // Plan section 36 Option A: layers stay addressable by id after usvg.
    assert!(
        svg.contains(&format!("layer-{layer_id}")),
        "the layer group did not survive normalization"
    );

    // --- undo moves history forward (plan section 29) ----------------------
    let response = test::call_service(
        &app,
        TestRequest::post()
            .uri(&format!("/api/v1/designs/{design_id}/undo"))
            .set_json(json!({ "base_version": 2 }))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let undone = body_json(response).await;
    assert_eq!(undone["version"], 3, "undo must not decrement");
    assert_eq!(
        undone["document"]["pages"].as_array().unwrap().len(),
        1,
        "undo must restore the document as it was at version 1"
    );
}

/// The conflict path, which is the whole reason mutations carry a base_version.
#[actix_web::test]
async fn a_second_write_on_the_same_base_version_is_a_409_with_both_versions() {
    let (state, _dir) = state().await;
    let app = app!(state);

    let project = body_json(
        test::call_service(
            &app,
            TestRequest::post()
                .uri("/api/v1/projects")
                .set_json(json!({ "name": "P" }))
                .to_request(),
        )
        .await,
    )
    .await;
    let project_id = project["id"].as_str().unwrap().to_string();

    let design = body_json(
        test::call_service(
            &app,
            TestRequest::post()
                .uri(&format!("/api/v1/projects/{project_id}/designs"))
                .set_json(json!({ "name": "D" }))
                .to_request(),
        )
        .await,
    )
    .await;
    let design_id = design["id"].as_str().unwrap().to_string();

    let batch = |version: u64| {
        json!({
            "base_version": version,
            "operations": [{
                "op": "add_page",
                "page": { "id": uuid::Uuid::new_v4().to_string(), "name": "Slide", "layers": [] }
            }]
        })
    };

    let first = test::call_service(
        &app,
        TestRequest::post()
            .uri(&format!("/api/v1/designs/{design_id}/operations"))
            .set_json(batch(1))
            .to_request(),
    )
    .await;
    assert_eq!(first.status(), StatusCode::OK);

    // The same base_version again: the design has moved underneath.
    let second = test::call_service(
        &app,
        TestRequest::post()
            .uri(&format!("/api/v1/designs/{design_id}/operations"))
            .set_json(batch(1))
            .to_request(),
    )
    .await;
    assert_eq!(second.status(), StatusCode::CONFLICT);

    let body = body_json(second).await;
    assert_eq!(body["error"]["code"], "VERSION_CONFLICT");
    // Both versions, so a client can rebase without a second round trip.
    assert_eq!(body["error"]["details"]["expected"], 1);
    assert_eq!(body["error"]["details"]["actual"], 2);
    assert!(
        body["error"]["hint"].as_str().unwrap().contains("again"),
        "a conflict must say what to do next: {body}"
    );
}

/// Every failure mode answers in the one envelope of plan section 32.
#[actix_web::test]
async fn errors_share_one_shape_whatever_went_wrong() {
    let (state, _dir) = state().await;
    let app = app!(state);

    let cases: Vec<(TestRequest, StatusCode, &str)> = vec![
        (
            TestRequest::get().uri("/api/v1/projects/not-a-uuid"),
            StatusCode::BAD_REQUEST,
            "MALFORMED_ID",
        ),
        (
            TestRequest::get().uri(&format!("/api/v1/designs/{}", uuid::Uuid::new_v4())),
            StatusCode::NOT_FOUND,
            "DESIGN_NOT_FOUND",
        ),
        (
            TestRequest::post()
                .uri("/api/v1/projects")
                .set_json(json!({ "name": "   " })),
            StatusCode::BAD_REQUEST,
            "INVALID_OP",
        ),
    ];

    for (request, status, code) in cases {
        let response = test::call_service(&app, request.to_request()).await;
        assert_eq!(response.status(), status, "expected {code}");

        let body = body_json(response).await;
        assert_eq!(body["error"]["code"], code, "{body}");
        assert!(body["error"]["message"].is_string(), "{body}");
        assert!(body["error"]["hint"].is_string(), "{body}");
    }
}

/// A batch is all or nothing (plan section 28).
#[actix_web::test]
async fn a_batch_with_one_bad_operation_persists_none_of_it() {
    let (state, _dir) = state().await;
    let app = app!(state);

    let project = body_json(
        test::call_service(
            &app,
            TestRequest::post()
                .uri("/api/v1/projects")
                .set_json(json!({ "name": "P" }))
                .to_request(),
        )
        .await,
    )
    .await;
    let project_id = project["id"].as_str().unwrap().to_string();

    let design = body_json(
        test::call_service(
            &app,
            TestRequest::post()
                .uri(&format!("/api/v1/projects/{project_id}/designs"))
                .set_json(json!({ "name": "D" }))
                .to_request(),
        )
        .await,
    )
    .await;
    let design_id = design["id"].as_str().unwrap().to_string();

    let response = test::call_service(
        &app,
        TestRequest::post()
            .uri(&format!("/api/v1/designs/{design_id}/operations"))
            .set_json(json!({
                "base_version": 1,
                "operations": [
                    {
                        "op": "add_page",
                        "page": { "id": uuid::Uuid::new_v4().to_string(), "name": "Good", "layers": [] }
                    },
                    // A layer aimed at a page that does not exist.
                    {
                        "op": "add_layer",
                        "page_id": uuid::Uuid::new_v4().to_string(),
                        "layer": {
                            "type": "shape",
                            "id": uuid::Uuid::new_v4().to_string(),
                            "name": "Bar",
                            "transform": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
                            "kind": "rectangle"
                        }
                    }
                ]
            }))
            .to_request(),
    )
    .await;
    // One code, one status, whichever route raised it: the agent branches on
    // the code, and a status that shifted per route would undo that.
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body = body_json(response).await;
    assert_eq!(body["error"]["code"], "PAGE_NOT_FOUND");
    // The message names which operation failed, so a batch of twenty is
    // actionable rather than just rejected.
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("operation 1"),
        "the failing operation must be named: {body}"
    );

    // The good first operation must not have survived the bad second one.
    let after = body_json(
        test::call_service(
            &app,
            TestRequest::get()
                .uri(&format!("/api/v1/designs/{design_id}"))
                .to_request(),
        )
        .await,
    )
    .await;
    assert_eq!(
        after["version"], 1,
        "a rejected batch must not bump version"
    );
    assert_eq!(
        after["document"]["pages"].as_array().unwrap().len(),
        1,
        "only the page the blank design started with"
    );
}

/// A Brand Kit round-trips, and a project without one is not an error.
#[actix_web::test]
async fn a_project_without_a_brand_kit_answers_null_rather_than_404() {
    let (state, _dir) = state().await;
    let app = app!(state);

    let project = body_json(
        test::call_service(
            &app,
            TestRequest::post()
                .uri("/api/v1/projects")
                .set_json(json!({ "name": "P" }))
                .to_request(),
        )
        .await,
    )
    .await;
    let project_id = project["id"].as_str().unwrap().to_string();

    let response = test::call_service(
        &app,
        TestRequest::get()
            .uri(&format!("/api/v1/projects/{project_id}/brand"))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(body_json(response).await.is_null());
}
