//! A provider session becomes an account, or it becomes a 403 (plan §108, §111.4).
//!
//! Offline: tokens are minted against the committed test keypair and the JWKS is
//! served from a fixture, so the paths that matter — an identity with no
//! membership, a token for another project, a session that resolves to somebody
//! else's account — can be exercised without a live provider or a real inbox.

use std::sync::Arc;

use actix_web::{
    http::StatusCode,
    test::{self, TestRequest},
    web, App,
};
use design_core::{AccountRole, AuthUserId};
use forma_server::config::AuthMode;
use forma_server::jwt::{JwksSource, JwtError, Verifier};
use forma_server::{routes, state::AppState};
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use persistence::{AccountRepository, ProjectRepository, SqliteAccountRepository};
use renderer::FontRegistry;
use serde::Serialize;

const ISSUER: &str = "https://project.supabase.co/auth/v1";
const AUDIENCE: &str = "authenticated";
const SIGNING_KEY: &[u8] = include_bytes!("../../../fixtures/identity/test-signing-key.pkcs8.der");
const JWKS: &str = include_str!("../../../fixtures/identity/test-jwks.json");

struct Fixture;

#[async_trait::async_trait]
impl JwksSource for Fixture {
    async fn fetch(&self) -> Result<JwkSet, JwtError> {
        Ok(serde_json::from_str(JWKS).expect("a jwk set"))
    }
}

#[derive(Serialize)]
struct Claims {
    sub: String,
    iss: String,
    aud: String,
    exp: i64,
    email: String,
    email_verified: bool,
}

fn session_for(user: AuthUserId) -> String {
    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some("test-key-1".into());
    encode(
        &header,
        &Claims {
            sub: user.to_string(),
            iss: ISSUER.into(),
            aud: AUDIENCE.into(),
            exp: chrono::Utc::now().timestamp() + 3600,
            email: "someone@example.com".into(),
            email_verified: true,
        },
        &EncodingKey::from_ec_der(SIGNING_KEY),
    )
    .expect("mint")
}

async fn hosted() -> (AppState, tempfile::TempDir) {
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

    let state = AppState::new(pool, fonts, cache, store)
        .with_mode(AuthMode::Supabase)
        .with_verifier(Arc::new(Verifier::new(ISSUER, AUDIENCE, Box::new(Fixture))));

    (state, dir)
}

macro_rules! serve {
    ($state:expr) => {
        test::init_service(
            App::new()
                .wrap(actix_web::middleware::from_fn(
                    forma_server::auth::require_bearer,
                ))
                .app_data(web::Data::new($state))
                .configure(routes::configure),
        )
        .await
    };
}

fn bearer(token: &str) -> (&'static str, String) {
    ("authorization", format!("Bearer {token}"))
}

/// A valid identity that owns no account is the one 403 in this codebase.
#[actix_web::test]
async fn an_identity_with_no_account_is_told_so() {
    let (state, _dir) = hosted().await;
    let app = serve!(state);

    let response = test::call_service(
        &app,
        TestRequest::get()
            .uri("/api/v1/projects")
            .insert_header(bearer(&session_for(AuthUserId::new())))
            .to_request(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body: serde_json::Value = test::read_body_json(response).await;
    assert_eq!(body["error"]["code"], "ACCOUNT_REQUIRED");
}

/// And a membership makes the same token reach exactly that account's work.
#[actix_web::test]
async fn a_membership_resolves_the_session_to_its_own_account() {
    let (state, _dir) = hosted().await;
    let accounts = SqliteAccountRepository::new(state.pool.clone());
    let projects = persistence::SqliteProjectRepository::new(state.pool.clone());

    let mine = accounts.bootstrap().await.expect("bootstrap").id;
    let theirs = accounts.create("Someone else").await.expect("create").id;
    projects.create(mine, "Mine").await.expect("mine");
    projects.create(theirs, "Theirs").await.expect("theirs");

    let user = AuthUserId::new();
    accounts
        .grant(mine, user, "someone@example.com", AccountRole::Owner)
        .await
        .expect("grant");

    let app = serve!(state);
    let response = test::call_service(
        &app,
        TestRequest::get()
            .uri("/api/v1/projects")
            .insert_header(bearer(&session_for(user)))
            .to_request(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = test::read_body_json(response).await;
    let names: Vec<&str> = body
        .as_array()
        .expect("a list")
        .iter()
        .map(|p| p["name"].as_str().expect("a name"))
        .collect();
    assert_eq!(
        names,
        vec!["Mine"],
        "a session must reach its own account's work and no one else's"
    );
}

/// A token this deployment did not ask for is refused, and says nothing about why.
#[actix_web::test]
async fn a_token_from_elsewhere_is_refused() {
    let (state, _dir) = hosted().await;
    let app = serve!(state);

    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some("test-key-1".into());
    let elsewhere = encode(
        &header,
        &Claims {
            sub: AuthUserId::new().to_string(),
            iss: "https://someone-elses.supabase.co/auth/v1".into(),
            aud: AUDIENCE.into(),
            exp: chrono::Utc::now().timestamp() + 3600,
            email: "someone@example.com".into(),
            email_verified: true,
        },
        &EncodingKey::from_ec_der(SIGNING_KEY),
    )
    .expect("mint");

    let response = test::call_service(
        &app,
        TestRequest::get()
            .uri("/api/v1/projects")
            .insert_header(bearer(&elsewhere))
            .to_request(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body: serde_json::Value = test::read_body_json(response).await;
    assert_eq!(
        body["error"]["code"], "UNAUTHORIZED",
        "a wrong issuer must not be distinguishable from any other bad token"
    );
}

/// The bundle stays public in hosted mode; only the workspace is not.
#[actix_web::test]
async fn the_shell_is_still_reachable_without_a_session() {
    let (state, _dir) = hosted().await;
    let app = serve!(state);

    for path in ["/login", "/oauth/consent", "/skill", "/api/v1/health"] {
        let response = test::call_service(&app, TestRequest::get().uri(path).to_request()).await;
        assert_ne!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
    }

    let refused = test::call_service(
        &app,
        TestRequest::get().uri("/api/v1/projects").to_request(),
    )
    .await;
    assert_eq!(refused.status(), StatusCode::UNAUTHORIZED);
}

/// Plan §117.2: authenticated, the history says who someone is rather than what
/// they typed.
#[actix_web::test]
async fn a_session_cannot_claim_to_be_someone_else_in_the_history() {
    let (state, _dir) = hosted().await;
    let accounts = SqliteAccountRepository::new(state.pool.clone());
    let account = accounts.bootstrap().await.expect("bootstrap").id;
    let user = AuthUserId::new();
    accounts
        .grant(account, user, "someone@example.com", AccountRole::Owner)
        .await
        .expect("grant");

    let app = serve!(state);
    let created = test::call_service(
        &app,
        TestRequest::post()
            .uri("/api/v1/projects")
            .insert_header(bearer(&session_for(user)))
            // The caller claims to be an agent. It is not.
            .insert_header(("x-forma-actor", "agent:not-really"))
            .set_json(serde_json::json!({ "name": "A project" }))
            .to_request(),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);

    let project: serde_json::Value = test::read_body_json(created).await;
    let id = project["id"].as_str().expect("an id");

    let design = test::call_service(
        &app,
        TestRequest::post()
            .uri(&format!("/api/v1/projects/{id}/designs"))
            .insert_header(bearer(&session_for(user)))
            .insert_header(("x-forma-actor", "agent:not-really"))
            .set_json(serde_json::json!({ "name": "A design", "format": { "preset": "square" } }))
            .to_request(),
    )
    .await;
    assert_eq!(design.status(), StatusCode::CREATED);
    let design: serde_json::Value = test::read_body_json(design).await;

    let history = test::call_service(
        &app,
        TestRequest::get()
            .uri(&format!(
                "/api/v1/designs/{}/history",
                design["id"].as_str().expect("an id")
            ))
            .insert_header(bearer(&session_for(user)))
            .to_request(),
    )
    .await;
    let history: serde_json::Value = test::read_body_json(history).await;

    assert_eq!(history[0]["actor_type"], "human");
    assert_eq!(
        history[0]["actor_id"],
        format!("user:{user}"),
        "the header said agent:not-really; the history says who signed in"
    );
}
