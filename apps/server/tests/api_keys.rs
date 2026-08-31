//! Personal access tokens through the real REST authentication boundary.

use std::sync::Arc;

use actix_web::{
    http::StatusCode,
    test::{self, TestRequest},
    web, App,
};
use forma_server::{auth::RemoteToken, routes, AppState};
use persistence::{AccountRepository, ApiKeyRepository, ProjectRepository};
use renderer::FontRegistry;

const DEPLOYMENT_TOKEN: &str = "deployment-token-that-remains-independent";

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

fn bearer(token: &str) -> (actix_web::http::header::HeaderName, String) {
    (
        actix_web::http::header::AUTHORIZATION,
        format!("Bearer {token}"),
    )
}

#[actix_web::test]
async fn creation_lists_only_safe_metadata_and_revokes_through_rest() {
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
        TestRequest::post()
            .uri("/api/v1/api-keys")
            .set_json(serde_json::json!({ "name": "Codex CLI" }))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let created: serde_json::Value = test::read_body_json(response).await;
    let token = created["token"]
        .as_str()
        .expect("one-time token")
        .to_string();
    let id = created["key"]["id"].as_str().expect("key id").to_string();
    assert!(token.starts_with("forma_pat_"));

    let response = test::call_service(
        &app,
        TestRequest::get().uri("/api/v1/api-keys").to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let listed: serde_json::Value = test::read_body_json(response).await;
    assert_eq!(listed.as_array().expect("key list").len(), 1);
    let serialized = listed.to_string();
    assert!(!serialized.contains(&token));
    assert!(!serialized.contains("token_hash"));
    assert!(listed[0].get("token").is_none());

    let response = test::call_service(
        &app,
        TestRequest::delete()
            .uri(&format!("/api/v1/api-keys/{id}"))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let response = test::call_service(
        &app,
        TestRequest::get().uri("/api/v1/api-keys").to_request(),
    )
    .await;
    let listed: serde_json::Value = test::read_body_json(response).await;
    assert!(listed[0]["revoked_at"].as_str().is_some());
}

#[actix_web::test]
async fn a_pat_scopes_routes_to_the_account_the_key_belongs_to() {
    let (state, _dir) = state().await;
    let account_a = state.accounts.bootstrap().await.expect("bootstrap");
    let account_b = state.accounts.create("Account B").await.expect("account B");
    let project_a = state
        .projects
        .create(account_a.id, "A project")
        .await
        .expect("A project");
    let project_b = state
        .projects
        .create(account_b.id, "B project")
        .await
        .expect("B project");
    let key_a = state
        .api_keys
        .create(account_a.id, None, "A key")
        .await
        .expect("A key");
    let key_b = state
        .api_keys
        .create(account_b.id, None, "B key")
        .await
        .expect("B key");

    let app = test::init_service(
        App::new()
            .wrap(actix_web::middleware::from_fn(
                forma_server::auth::require_bearer,
            ))
            // The process defaults to A. The PAT must override this per request.
            .app_data(web::Data::new(state))
            .configure(routes::configure),
    )
    .await;

    let response = test::call_service(
        &app,
        TestRequest::get()
            .uri("/api/v1/projects")
            .insert_header(bearer(&key_b.token))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let projects: serde_json::Value = test::read_body_json(response).await;
    assert_eq!(projects.as_array().expect("project list").len(), 1);
    assert_eq!(projects[0]["id"], project_b.id.to_string());
    assert_ne!(projects[0]["id"], project_a.id.to_string());

    let hidden = test::call_service(
        &app,
        TestRequest::delete()
            .uri(&format!("/api/v1/api-keys/{}", key_a.key.id))
            .insert_header(bearer(&key_b.token))
            .to_request(),
    )
    .await;
    assert_eq!(hidden.status(), StatusCode::NOT_FOUND);
    let body: serde_json::Value = test::read_body_json(hidden).await;
    assert_eq!(body["error"]["code"], "NOT_FOUND");
}

#[actix_web::test]
async fn revoking_one_pat_leaves_other_credentials_alive() {
    let (state, _dir) = state().await;
    let account = state.accounts.bootstrap().await.expect("bootstrap");
    let first = state
        .api_keys
        .create(account.id, None, "First")
        .await
        .expect("first");
    let second = state
        .api_keys
        .create(account.id, None, "Second")
        .await
        .expect("second");

    let app = test::init_service(
        App::new()
            .wrap(actix_web::middleware::from_fn(
                forma_server::auth::require_bearer,
            ))
            .app_data(web::Data::new(RemoteToken::new(DEPLOYMENT_TOKEN)))
            .app_data(web::Data::new(state))
            .configure(routes::configure),
    )
    .await;

    let revoke = test::call_service(
        &app,
        TestRequest::delete()
            .uri(&format!("/api/v1/api-keys/{}", first.key.id))
            .insert_header(bearer(&second.token))
            .to_request(),
    )
    .await;
    assert_eq!(revoke.status(), StatusCode::NO_CONTENT);

    let revoked = test::call_service(
        &app,
        TestRequest::get()
            .uri("/api/v1/projects")
            .insert_header(bearer(&first.token))
            .to_request(),
    )
    .await;
    assert_eq!(revoked.status(), StatusCode::UNAUTHORIZED);
    let body: serde_json::Value = test::read_body_json(revoked).await;
    assert_eq!(body["error"]["code"], "API_KEY_REVOKED");

    for credential in [&second.token, DEPLOYMENT_TOKEN] {
        let live = test::call_service(
            &app,
            TestRequest::get()
                .uri("/api/v1/projects")
                .insert_header(bearer(credential))
                .to_request(),
        )
        .await;
        assert_eq!(
            live.status(),
            StatusCode::OK,
            "revoking one key disconnected another credential"
        );
    }
}

#[actix_web::test]
async fn a_pat_authenticated_mutation_records_the_key_not_a_claimed_actor() {
    let (state, _dir) = state().await;
    let account = state.accounts.bootstrap().await.expect("bootstrap");
    let project = state
        .projects
        .create(account.id, "PAT history")
        .await
        .expect("project");
    let key = state
        .api_keys
        .create(account.id, None, "History key")
        .await
        .expect("key");

    let app = test::init_service(
        App::new()
            .wrap(actix_web::middleware::from_fn(
                forma_server::auth::require_bearer,
            ))
            .app_data(web::Data::new(state))
            .configure(routes::configure),
    )
    .await;

    let created = test::call_service(
        &app,
        TestRequest::post()
            .uri(&format!("/api/v1/projects/{}/designs", project.id))
            .insert_header(bearer(&key.token))
            .insert_header(("x-forma-actor", "human"))
            .set_json(serde_json::json!({ "name": "Made with a PAT" }))
            .to_request(),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let design: serde_json::Value = test::read_body_json(created).await;
    let design_id = design["id"].as_str().expect("design id");

    let history = test::call_service(
        &app,
        TestRequest::get()
            .uri(&format!("/api/v1/designs/{design_id}/history"))
            .insert_header(bearer(&key.token))
            .to_request(),
    )
    .await;
    assert_eq!(history.status(), StatusCode::OK);
    let history: serde_json::Value = test::read_body_json(history).await;
    assert_eq!(history[0]["actor_type"], "agent");
    assert_eq!(
        history[0]["actor_id"],
        format!("pat:{}", key.key.id),
        "a caller-controlled actor header must not forge PAT history"
    );
}
