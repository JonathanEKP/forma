//! Tenant isolation through the public REST routes (plan sections 108.2 and 116.2).
//!
//! Repository tests prove that scoped queries exist. This test proves every
//! handler carries the account into those queries: two applications share one
//! pool, know the same ids, and still see different worlds.

use std::sync::Arc;

use actix_web::{
    dev::ServiceResponse,
    http::StatusCode,
    test::{self, TestRequest},
    web, App,
};
use forma_server::{routes, state::AppState};
use persistence::AccountRepository;
use renderer::FontRegistry;
use serde_json::{json, Value};

const BOUNDARY: &str = "forma-tenancy-boundary";

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

struct Fixtures {
    project: String,
    design: String,
    page: String,
    asset: String,
    template: String,
    request: String,
    export: String,
}

async fn body_json(response: ServiceResponse) -> Value {
    serde_json::from_slice(&test::read_body(response).await).expect("the §32 JSON envelope")
}

async fn expect_json(
    app: &impl TestApp,
    request: actix_http::Request,
    status: StatusCode,
    what: &str,
) -> Value {
    let response = test::call_service(app, request).await;
    assert_eq!(response.status(), status, "{what}");
    body_json(response).await
}

fn id(value: &Value) -> String {
    value["id"].as_str().expect("a response id").to_string()
}

fn brand() -> Value {
    json!({
        "name": "Tenant A",
        "colors": {
            "primary": "#FF5436",
            "secondary": "#FDBA2D",
            "accent": "#7C3AED",
            "background": "#101010",
            "surface": "#191919",
            "text": "#FFFFFF",
            "text_inverse": "#101010",
            "muted": "#A3A3A3"
        },
        "typography": {
            "display": "Geist",
            "body": "Geist",
            "mono": "Geist Mono"
        }
    })
}

fn png() -> Vec<u8> {
    let mut pixmap = resvg::tiny_skia::Pixmap::new(2, 2).expect("pixmap");
    pixmap.fill(resvg::tiny_skia::Color::from_rgba8(0xFF, 0x54, 0x36, 0xFF));
    pixmap.encode_png().expect("PNG")
}

fn upload(project: &str, bytes: &[u8]) -> actix_http::Request {
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"file\"; filename=\"tenant.png\"\r\n\r\n",
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

async fn fixtures(app: &impl TestApp) -> Fixtures {
    let project = expect_json(
        app,
        TestRequest::post()
            .uri("/api/v1/projects")
            .set_json(json!({ "name": "Account A project" }))
            .to_request(),
        StatusCode::CREATED,
        "create project",
    )
    .await;
    let project = id(&project);

    expect_json(
        app,
        TestRequest::put()
            .uri(&format!("/api/v1/projects/{project}/brand"))
            .set_json(brand())
            .to_request(),
        StatusCode::OK,
        "create brand",
    )
    .await;

    let design = expect_json(
        app,
        TestRequest::post()
            .uri(&format!("/api/v1/projects/{project}/designs"))
            .set_json(json!({ "name": "Account A design" }))
            .to_request(),
        StatusCode::CREATED,
        "create design",
    )
    .await;
    let page = design["document"]["pages"][0]["id"]
        .as_str()
        .expect("the blank design's page")
        .to_string();
    let design = id(&design);

    expect_json(
        app,
        TestRequest::post()
            .uri(&format!("/api/v1/designs/{design}/operations"))
            .set_json(json!({
                "base_version": 1,
                "operations": [
                    { "op": "set_background", "page_id": page, "color": "#101010" }
                ]
            }))
            .to_request(),
        StatusCode::OK,
        "advance the design so undo is a valid request",
    )
    .await;

    let asset = expect_json(
        app,
        upload(&project, &png()),
        StatusCode::CREATED,
        "upload asset",
    )
    .await;
    let asset = id(&asset);

    let template = expect_json(
        app,
        TestRequest::post()
            .uri(&format!("/api/v1/designs/{design}/template"))
            .set_json(json!({
                "name": "Account A template",
                "description": "A tenant fixture"
            }))
            .to_request(),
        StatusCode::CREATED,
        "save template",
    )
    .await;
    let template = id(&template);

    let request = expect_json(
        app,
        TestRequest::post()
            .uri(&format!("/api/v1/projects/{project}/asset-requests"))
            .set_json(json!({
                "purpose": "hero",
                "description": "An image only account A may resolve",
                "design_id": design
            }))
            .to_request(),
        StatusCode::CREATED,
        "create asset request",
    )
    .await;
    let request = id(&request);

    let export = expect_json(
        app,
        TestRequest::post()
            .uri(&format!("/api/v1/designs/{design}/exports"))
            .set_json(json!({ "scope": "all_pages" }))
            .to_request(),
        StatusCode::CREATED,
        "create export",
    )
    .await;
    let export = export["export"]["id"]
        .as_str()
        .expect("an export id")
        .to_string();

    Fixtures {
        project,
        design,
        page,
        asset,
        template,
        request,
        export,
    }
}

async fn assert_not_found(
    app: &impl TestApp,
    what: &str,
    request: actix_http::Request,
    code: &str,
) {
    let response = test::call_service(app, request).await;
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "{what} disclosed another account's object"
    );

    let body = body_json(response).await;
    assert_eq!(body["error"]["code"], code, "{what}: {body}");
    assert!(
        body["error"]["message"]
            .as_str()
            .is_some_and(|message| !message.is_empty()),
        "{what} has no useful message: {body}"
    );
    assert!(
        body["error"]["hint"]
            .as_str()
            .is_some_and(|hint| !hint.is_empty()),
        "{what} has no useful hint: {body}"
    );
}

#[actix_web::test]
async fn known_ids_from_another_account_are_not_found_through_every_route() {
    let dir = tempfile::tempdir().expect("temp dir");
    let pool = persistence::connect(
        &format!("sqlite://{}", dir.path().join("forma.db").display()),
        dir.path(),
    )
    .await
    .expect("migrations");

    let accounts = persistence::SqliteAccountRepository::new(pool.clone());
    let account_a = accounts.bootstrap().await.expect("bootstrap account").id;
    let account_b = accounts
        .create("Account B")
        .await
        .expect("second account")
        .id;

    let fonts = Arc::new(
        FontRegistry::from_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/fonts"))
            .expect("fonts"),
    );
    let cache = Arc::new(renderer::RenderCache::new(dir.path().join("previews")));
    let store = Arc::new(persistence::LocalObjectStore::new(dir.path()));

    let state_a = AppState::new(pool.clone(), fonts.clone(), cache.clone(), store.clone())
        .with_account(account_a);
    let state_b = AppState::new(pool, fonts, cache, store).with_account(account_b);

    let app_a = test::init_service(
        App::new()
            .app_data(web::Data::new(state_a))
            .configure(routes::configure),
    )
    .await;
    let app_b = test::init_service(
        App::new()
            .app_data(web::Data::new(state_b))
            .configure(routes::configure),
    )
    .await;

    let fixtures = fixtures(&app_a).await;

    let a_projects = expect_json(
        &app_a,
        TestRequest::get().uri("/api/v1/projects").to_request(),
        StatusCode::OK,
        "list A projects",
    )
    .await;
    assert_eq!(a_projects.as_array().expect("projects").len(), 1);

    let b_projects = expect_json(
        &app_b,
        TestRequest::get().uri("/api/v1/projects").to_request(),
        StatusCode::OK,
        "list B projects",
    )
    .await;
    assert!(b_projects.as_array().expect("projects").is_empty());

    let p = &fixtures.project;
    let project_cases = vec![
        (
            "read project",
            TestRequest::get()
                .uri(&format!("/api/v1/projects/{p}"))
                .to_request(),
        ),
        (
            "rename project",
            TestRequest::patch()
                .uri(&format!("/api/v1/projects/{p}"))
                .set_json(json!({ "name": "Taken over" }))
                .to_request(),
        ),
        (
            "delete project",
            TestRequest::delete()
                .uri(&format!("/api/v1/projects/{p}"))
                .to_request(),
        ),
        (
            "read brand",
            TestRequest::get()
                .uri(&format!("/api/v1/projects/{p}/brand"))
                .to_request(),
        ),
        (
            "write brand",
            TestRequest::put()
                .uri(&format!("/api/v1/projects/{p}/brand"))
                .set_json(brand())
                .to_request(),
        ),
        (
            "list designs",
            TestRequest::get()
                .uri(&format!("/api/v1/projects/{p}/designs"))
                .to_request(),
        ),
        (
            "create design",
            TestRequest::post()
                .uri(&format!("/api/v1/projects/{p}/designs"))
                .set_json(json!({ "name": "Intruder design" }))
                .to_request(),
        ),
        (
            "list assets",
            TestRequest::get()
                .uri(&format!("/api/v1/projects/{p}/assets"))
                .to_request(),
        ),
        ("upload asset", upload(p, &png())),
        (
            "list templates",
            TestRequest::get()
                .uri(&format!("/api/v1/projects/{p}/templates"))
                .to_request(),
        ),
        (
            "list asset requests",
            TestRequest::get()
                .uri(&format!("/api/v1/projects/{p}/asset-requests"))
                .to_request(),
        ),
        (
            "create asset request",
            TestRequest::post()
                .uri(&format!("/api/v1/projects/{p}/asset-requests"))
                .set_json(json!({ "description": "Intruder request" }))
                .to_request(),
        ),
    ];
    for (what, request) in project_cases {
        assert_not_found(&app_b, what, request, "PROJECT_NOT_FOUND").await;
    }

    let d = &fixtures.design;
    let page = &fixtures.page;
    let design_cases = vec![
        (
            "read design",
            TestRequest::get()
                .uri(&format!("/api/v1/designs/{d}"))
                .to_request(),
        ),
        (
            "apply operations",
            TestRequest::post()
                .uri(&format!("/api/v1/designs/{d}/operations"))
                .set_json(json!({ "base_version": 2, "operations": [] }))
                .to_request(),
        ),
        (
            "undo design",
            TestRequest::post()
                .uri(&format!("/api/v1/designs/{d}/undo"))
                .set_json(json!({ "base_version": 2 }))
                .to_request(),
        ),
        (
            "read history",
            TestRequest::get()
                .uri(&format!("/api/v1/designs/{d}/history"))
                .to_request(),
        ),
        (
            "validate design",
            TestRequest::post()
                .uri(&format!("/api/v1/designs/{d}/validate"))
                .to_request(),
        ),
        (
            "render design",
            TestRequest::post()
                .uri(&format!("/api/v1/designs/{d}/render"))
                .set_json(json!({}))
                .to_request(),
        ),
        (
            "read SVG preview",
            TestRequest::get()
                .uri(&format!("/api/v1/designs/{d}/pages/{page}/preview.svg"))
                .to_request(),
        ),
        (
            "read PNG preview",
            TestRequest::get()
                .uri(&format!("/api/v1/designs/{d}/pages/{page}/preview.png"))
                .to_request(),
        ),
        (
            "save template",
            TestRequest::post()
                .uri(&format!("/api/v1/designs/{d}/template"))
                .set_json(json!({ "name": "Intruder template" }))
                .to_request(),
        ),
        (
            "delete design",
            TestRequest::delete()
                .uri(&format!("/api/v1/designs/{d}"))
                .to_request(),
        ),
        (
            "create export",
            TestRequest::post()
                .uri(&format!("/api/v1/designs/{d}/exports"))
                .set_json(json!({ "scope": "all_pages" }))
                .to_request(),
        ),
        (
            "list exports",
            TestRequest::get()
                .uri(&format!("/api/v1/designs/{d}/exports"))
                .to_request(),
        ),
    ];
    for (what, request) in design_cases {
        assert_not_found(&app_b, what, request, "DESIGN_NOT_FOUND").await;
    }

    let a = &fixtures.asset;
    for (what, request) in [
        (
            "read asset",
            TestRequest::get()
                .uri(&format!("/api/v1/assets/{a}"))
                .to_request(),
        ),
        (
            "read asset content",
            TestRequest::get()
                .uri(&format!("/api/v1/assets/{a}/content"))
                .to_request(),
        ),
        (
            "delete asset",
            TestRequest::delete()
                .uri(&format!("/api/v1/assets/{a}"))
                .to_request(),
        ),
    ] {
        assert_not_found(&app_b, what, request, "NOT_FOUND").await;
    }

    let t = &fixtures.template;
    for (what, request) in [
        (
            "read template",
            TestRequest::get()
                .uri(&format!("/api/v1/templates/{t}"))
                .to_request(),
        ),
        (
            "create design from template",
            TestRequest::post()
                .uri(&format!("/api/v1/templates/{t}/designs"))
                .set_json(json!({ "name": "Intruder copy" }))
                .to_request(),
        ),
        (
            "delete template",
            TestRequest::delete()
                .uri(&format!("/api/v1/templates/{t}"))
                .to_request(),
        ),
    ] {
        assert_not_found(&app_b, what, request, "NOT_FOUND").await;
    }

    let r = &fixtures.request;
    for (what, request) in [
        (
            "read asset request",
            TestRequest::get()
                .uri(&format!("/api/v1/asset-requests/{r}"))
                .to_request(),
        ),
        (
            "resolve asset request",
            TestRequest::post()
                .uri(&format!("/api/v1/asset-requests/{r}/resolve"))
                .set_json(json!({ "asset_id": a }))
                .to_request(),
        ),
        (
            "cancel asset request",
            TestRequest::post()
                .uri(&format!("/api/v1/asset-requests/{r}/cancel"))
                .to_request(),
        ),
    ] {
        assert_not_found(&app_b, what, request, "NOT_FOUND").await;
    }

    let e = &fixtures.export;
    for (what, request) in [
        (
            "read export",
            TestRequest::get()
                .uri(&format!("/api/v1/exports/{e}"))
                .to_request(),
        ),
        (
            "download export",
            TestRequest::get()
                .uri(&format!("/api/v1/exports/{e}/download"))
                .to_request(),
        ),
    ] {
        assert_not_found(&app_b, what, request, "NOT_FOUND").await;
    }

    // Failed mutations through B did not change A's work.
    let a_projects = expect_json(
        &app_a,
        TestRequest::get().uri("/api/v1/projects").to_request(),
        StatusCode::OK,
        "list A projects after attacks",
    )
    .await;
    assert_eq!(a_projects.as_array().expect("projects").len(), 1);
    assert_eq!(a_projects[0]["name"], "Account A project");

    for (what, uri) in [
        ("project", format!("/api/v1/projects/{p}")),
        ("design", format!("/api/v1/designs/{d}")),
        ("asset", format!("/api/v1/assets/{a}")),
        ("template", format!("/api/v1/templates/{t}")),
        ("asset request", format!("/api/v1/asset-requests/{r}")),
        ("export", format!("/api/v1/exports/{e}")),
    ] {
        let response = test::call_service(&app_a, TestRequest::get().uri(&uri).to_request()).await;
        assert_eq!(response.status(), StatusCode::OK, "A lost its {what}");
    }
}
