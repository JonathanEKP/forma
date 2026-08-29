//! Templates over HTTP (plan section 85).
//!
//! What separates a template from a copied design is the brief: the roles read
//! out of the document, so an agent sees what each slot is for and how much text
//! the layout actually held. These tests are about that metadata as much as
//! about the rows.

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

/// A project holding one design with two labelled slots and one unlabelled one.
async fn a_cover(app: &impl TestApp) -> (String, Value) {
    let project = body_json(
        test::call_service(
            app,
            TestRequest::post()
                .uri("/api/v1/projects")
                .set_json(json!({ "name": "Editorial" }))
                .to_request(),
        )
        .await,
    )
    .await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let design = body_json(
        test::call_service(
            app,
            TestRequest::post()
                .uri(&format!("/api/v1/projects/{project}/designs"))
                .set_json(json!({ "name": "Issue 07" }))
                .to_request(),
        )
        .await,
    )
    .await;
    let design_id = design["id"].as_str().unwrap().to_string();
    let page_id = design["document"]["pages"][0]["id"].as_str().unwrap();

    let layer = |name: &str, role: Option<&str>, text: &str, y: f64| {
        let mut layer = json!({
            "type": "text",
            "id": uuid::Uuid::new_v4().to_string(),
            "name": name,
            "transform": { "x": 96.0, "y": y, "width": 888.0, "height": 200.0 },
            "text": text,
            "font_family": "Geist",
            "font_size": 64.0,
            "font_weight": 700,
            "color": "#F4F1EA"
        });
        if let Some(role) = role {
            layer["role"] = json!(role);
        }
        json!({ "op": "add_layer", "page_id": page_id, "layer": layer })
    };

    let updated = body_json(
        test::call_service(
            app,
            TestRequest::post()
                .uri(&format!("/api/v1/designs/{design_id}/operations"))
                .set_json(json!({
                    "base_version": design["version"],
                    "operations": [
                        layer("Masthead", Some("logo"), "VEREDA", 96.0),
                        layer("Headline", Some("headline"), "llueve y la ciudad\nno afloja", 968.0),
                        layer("Rule", None, "—", 1180.0),
                    ]
                }))
                .to_request(),
        )
        .await,
    )
    .await;

    (design_id, updated)
}

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

/// The point of section 85: what comes back is a brief, not a copy. Only layers
/// the author labelled become slots, and each one reports how much text the
/// layout really held.
#[actix_web::test]
async fn saving_a_template_reads_the_brief_out_of_the_design() {
    let (state, _dir) = state().await;
    let app = app!(state);
    let (design_id, _) = a_cover(&app).await;

    let response = test::call_service(
        &app,
        TestRequest::post()
            .uri(&format!("/api/v1/designs/{design_id}/template"))
            .set_json(json!({ "name": "Portada de número", "description": "Foto a sangre" }))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let template = body_json(response).await;
    assert_eq!(template["name"], "Portada de número");

    let roles = template["roles"].as_array().unwrap();
    assert_eq!(roles.len(), 2, "only labelled layers are slots: {roles:?}");

    let headline = roles.iter().find(|r| r["role"] == "headline").unwrap();
    assert_eq!(headline["page_index"], 0);
    assert_eq!(headline["sample_text"], "llueve y la ciudad\nno afloja");
    // The budget is what fitted, counted in characters rather than bytes: the
    // sample has none outside ASCII, but the next one will.
    assert_eq!(headline["max_chars"], 28);

    assert!(roles.iter().any(|r| r["role"] == "logo"));
}

#[actix_web::test]
async fn a_design_made_from_a_template_keeps_the_shape_and_mints_new_ids() {
    let (state, _dir) = state().await;
    let app = app!(state);
    let (design_id, original) = a_cover(&app).await;

    let template = body_json(
        test::call_service(
            &app,
            TestRequest::post()
                .uri(&format!("/api/v1/designs/{design_id}/template"))
                .set_json(json!({ "name": "Portada de número" }))
                .to_request(),
        )
        .await,
    )
    .await;
    let template_id = template["id"].as_str().unwrap();

    let response = test::call_service(
        &app,
        TestRequest::post()
            .uri(&format!("/api/v1/templates/{template_id}/designs"))
            .set_json(json!({ "name": "Issue 08" }))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let made = body_json(response).await;
    assert_eq!(made["name"], "Issue 08");
    assert_eq!(made["version"], 1);
    assert_eq!(made["project_id"], original["project_id"]);

    let old_page = &original["document"]["pages"][0];
    let new_page = &made["document"]["pages"][0];

    // The words travel: a template is "make this kind of piece again", not a
    // blank layout.
    assert_eq!(
        new_page["layers"][1]["text"], old_page["layers"][1]["text"],
        "the copy should carry the template's words"
    );
    assert_eq!(new_page["layers"].as_array().unwrap().len(), 3);

    // The identity does not travel, so two designs from one template never
    // share a layer id. Section 85 addresses slots by page_index for this
    // reason.
    assert_ne!(new_page["id"], old_page["id"]);
    for index in 0..3 {
        assert_ne!(
            new_page["layers"][index]["id"], old_page["layers"][index]["id"],
            "layer {index} kept the template's id"
        );
    }
}

#[actix_web::test]
async fn templates_are_listed_for_their_project_and_can_be_removed() {
    let (state, _dir) = state().await;
    let app = app!(state);
    let (design_id, original) = a_cover(&app).await;
    let project = original["project_id"].as_str().unwrap();

    let template = body_json(
        test::call_service(
            &app,
            TestRequest::post()
                .uri(&format!("/api/v1/designs/{design_id}/template"))
                .set_json(json!({ "name": "Portada de número" }))
                .to_request(),
        )
        .await,
    )
    .await;
    let template_id = template["id"].as_str().unwrap();

    let listed = body_json(
        test::call_service(
            &app,
            TestRequest::get()
                .uri(&format!("/api/v1/projects/{project}/templates"))
                .to_request(),
        )
        .await,
    )
    .await;
    assert_eq!(listed.as_array().unwrap().len(), 1);

    let removed = test::call_service(
        &app,
        TestRequest::delete()
            .uri(&format!("/api/v1/templates/{template_id}"))
            .to_request(),
    )
    .await;
    assert_eq!(removed.status(), StatusCode::NO_CONTENT);

    let gone = test::call_service(
        &app,
        TestRequest::get()
            .uri(&format!("/api/v1/templates/{template_id}"))
            .to_request(),
    )
    .await;
    assert_eq!(gone.status(), StatusCode::NOT_FOUND);
}

/// A template keeps the design as it was when saved. The design goes on to new
/// versions; the template does not follow, or "start from the one that worked"
/// would silently mean "start from whatever it became".
#[actix_web::test]
async fn a_template_does_not_follow_the_design_it_came_from() {
    let (state, _dir) = state().await;
    let app = app!(state);
    let (design_id, updated) = a_cover(&app).await;

    let template = body_json(
        test::call_service(
            &app,
            TestRequest::post()
                .uri(&format!("/api/v1/designs/{design_id}/template"))
                .set_json(json!({ "name": "Portada de número" }))
                .to_request(),
        )
        .await,
    )
    .await;
    let template_id = template["id"].as_str().unwrap();
    let headline_id = updated["document"]["pages"][0]["layers"][1]["id"]
        .as_str()
        .unwrap();

    let changed = test::call_service(
        &app,
        TestRequest::post()
            .uri(&format!("/api/v1/designs/{design_id}/operations"))
            .set_json(json!({
                "base_version": updated["version"],
                "operations": [
                    { "op": "set_text", "layer_id": headline_id, "content": "otra cosa" }
                ]
            }))
            .to_request(),
    )
    .await;
    assert_eq!(changed.status(), StatusCode::OK);

    let reread = body_json(
        test::call_service(
            &app,
            TestRequest::get()
                .uri(&format!("/api/v1/templates/{template_id}"))
                .to_request(),
        )
        .await,
    )
    .await;
    assert_eq!(
        reread["document"]["pages"][0]["layers"][1]["text"],
        "llueve y la ciudad\nno afloja"
    );
}
