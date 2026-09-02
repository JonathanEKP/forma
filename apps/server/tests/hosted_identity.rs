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

// ---------------------------------------------------------------------------
// Onboarding (plan §107.3 and §113.1)
// ---------------------------------------------------------------------------

fn session_for_email(user: AuthUserId, email: &str, verified: bool) -> String {
    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some("test-key-1".into());
    encode(
        &header,
        &Claims {
            sub: user.to_string(),
            iss: ISSUER.into(),
            aud: AUDIENCE.into(),
            exp: chrono::Utc::now().timestamp() + 3600,
            email: email.into(),
            email_verified: verified,
        },
        &EncodingKey::from_ec_der(SIGNING_KEY),
    )
    .expect("mint")
}

/// Signing in is not the gate; owning an account is.
#[actix_web::test]
async fn an_uninvited_identity_cannot_mint_itself_a_workspace() {
    let (state, _dir) = hosted().await;
    let app = serve!(state);

    let response = test::call_service(
        &app,
        TestRequest::post()
            .uri("/api/v1/accounts")
            .insert_header(bearer(&session_for_email(
                AuthUserId::new(),
                "stranger@example.com",
                true,
            )))
            .to_request(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body: serde_json::Value = test::read_body_json(response).await;
    assert_eq!(body["error"]["code"], "ACCOUNT_REQUIRED");
}

/// The gate rests on the address having been proven. If it did not, anyone
/// could register as an invited address and walk in.
#[actix_web::test]
async fn an_unverified_address_is_not_an_invitation() {
    let (state, _dir) = hosted().await;
    let accounts = SqliteAccountRepository::new(state.pool.clone());
    accounts
        .invite("invited@example.com")
        .await
        .expect("invite");

    let app = serve!(state);
    let response = test::call_service(
        &app,
        TestRequest::post()
            .uri("/api/v1/accounts")
            .insert_header(bearer(&session_for_email(
                AuthUserId::new(),
                // The right address, claimed and not proven.
                "invited@example.com",
                false,
            )))
            .to_request(),
    )
    .await;

    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "an unverified address must not open an invitation"
    );
}

#[actix_web::test]
async fn an_invitation_creates_an_account_once() {
    let (state, _dir) = hosted().await;
    let accounts = SqliteAccountRepository::new(state.pool.clone());
    // Cased and padded on purpose: the same invitation, however it is typed.
    accounts
        .invite("  Invited@Example.com ")
        .await
        .expect("invite");

    let app = serve!(state);
    let user = AuthUserId::new();
    let token = session_for_email(user, "invited@example.com", true);

    let created = test::call_service(
        &app,
        TestRequest::post()
            .uri("/api/v1/accounts")
            .insert_header(bearer(&token))
            .to_request(),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);

    // The invitation is spent.
    assert!(accounts
        .open_invite("invited@example.com")
        .await
        .unwrap()
        .is_none());

    // And a second identity cannot reuse it.
    let second = test::call_service(
        &app,
        TestRequest::post()
            .uri("/api/v1/accounts")
            .insert_header(bearer(&session_for_email(
                AuthUserId::new(),
                "invited@example.com",
                true,
            )))
            .to_request(),
    )
    .await;
    assert_eq!(second.status(), StatusCode::FORBIDDEN);

    // The first identity now reaches its own workspace rather than a 403.
    let projects = test::call_service(
        &app,
        TestRequest::get()
            .uri("/api/v1/projects")
            .insert_header(bearer(&token))
            .to_request(),
    )
    .await;
    assert_eq!(projects.status(), StatusCode::OK);
}

/// An invitation for one address is not an invitation for another.
#[actix_web::test]
async fn an_invitation_belongs_to_the_address_it_names() {
    let (state, _dir) = hosted().await;
    SqliteAccountRepository::new(state.pool.clone())
        .invite("invited@example.com")
        .await
        .expect("invite");

    let app = serve!(state);
    let response = test::call_service(
        &app,
        TestRequest::post()
            .uri("/api/v1/accounts")
            .insert_header(bearer(&session_for_email(
                AuthUserId::new(),
                "someone.else@example.com",
                true,
            )))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

/// The claim needs both halves: the token proves you run the machine, the
/// session says who to hand it to.
#[actix_web::test]
async fn claiming_the_workspace_needs_the_token_and_a_session() {
    let (state, _dir) = hosted().await;
    let app = test::init_service(
        App::new()
            .wrap(actix_web::middleware::from_fn(
                forma_server::auth::require_bearer,
            ))
            .app_data(web::Data::new(state))
            .app_data(web::Data::new(forma_server::auth::RemoteToken::new(
                "the-deployment-token",
            )))
            .configure(routes::configure),
    )
    .await;

    let user = AuthUserId::new();
    let session = session_for_email(user, "owner@example.com", true);

    // The session, and the wrong token.
    let wrong = test::call_service(
        &app,
        TestRequest::post()
            .uri("/api/v1/bootstrap/claim")
            .insert_header(bearer(&session))
            .set_json(serde_json::json!({ "token": "not-it" }))
            .to_request(),
    )
    .await;
    assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);

    // The token, and no session at all.
    let anonymous = test::call_service(
        &app,
        TestRequest::post()
            .uri("/api/v1/bootstrap/claim")
            .set_json(serde_json::json!({ "token": "the-deployment-token" }))
            .to_request(),
    )
    .await;
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);

    // Both.
    let claimed = test::call_service(
        &app,
        TestRequest::post()
            .uri("/api/v1/bootstrap/claim")
            .insert_header(bearer(&session))
            .set_json(serde_json::json!({ "token": "the-deployment-token" }))
            .to_request(),
    )
    .await;
    assert_eq!(claimed.status(), StatusCode::OK);

    // And never twice, which is what stops the workspace falling to whoever
    // signs up second.
    let again = test::call_service(
        &app,
        TestRequest::post()
            .uri("/api/v1/bootstrap/claim")
            .insert_header(bearer(&session_for_email(
                AuthUserId::new(),
                "someone.else@example.com",
                true,
            )))
            .set_json(serde_json::json!({ "token": "the-deployment-token" }))
            .to_request(),
    )
    .await;
    assert_eq!(again.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = test::read_body_json(again).await;
    assert_eq!(body["error"]["code"], "LEGACY_ACCOUNT_ALREADY_CLAIMED");
}

/// The editor asks this before it draws anything, and it must answer to someone
/// holding nothing at all.
#[actix_web::test]
async fn whether_identity_is_required_is_public() {
    let (state, _dir) = hosted().await;
    let app = serve!(state);

    let response = test::call_service(
        &app,
        TestRequest::get().uri("/api/v1/identity").to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = test::read_body_json(response).await;
    assert_eq!(body["required"], true);
    assert_eq!(
        body.as_object().expect("an object").len(),
        1,
        "it must say whether, and nothing else"
    );
}

/// The claim door is offered only while there is something to claim.
#[actix_web::test]
async fn onboarding_says_which_doors_are_open() {
    let (state, _dir) = hosted().await;
    let accounts = SqliteAccountRepository::new(state.pool.clone());
    accounts
        .invite("invited@example.com")
        .await
        .expect("invite");

    let app = serve!(state);
    let body: serde_json::Value = test::call_and_read_body_json(
        &app,
        TestRequest::get()
            .uri("/api/v1/onboarding")
            .insert_header(bearer(&session_for_email(
                AuthUserId::new(),
                "invited@example.com",
                true,
            )))
            .to_request(),
    )
    .await;
    assert_eq!(body["invited"], true);
    assert_eq!(body["claimable"], true);
    assert_eq!(body["email"], "invited@example.com");

    // Once claimed, that door closes.
    accounts
        .claim_bootstrap(AuthUserId::new(), "owner@example.com")
        .await
        .expect("claim");

    let body: serde_json::Value = test::call_and_read_body_json(
        &app,
        TestRequest::get()
            .uri("/api/v1/onboarding")
            .insert_header(bearer(&session_for_email(
                AuthUserId::new(),
                "nobody@example.com",
                true,
            )))
            .to_request(),
    )
    .await;
    assert_eq!(body["claimable"], false);
    assert_eq!(body["invited"], false);
}

/// The door decides which credential is acceptable (plan §112).
///
/// `forma-mcp` marks its requests, and the marker only ever narrows what is
/// accepted — which is what makes trusting a header for this safe. Forging it
/// on a direct REST call demands a stricter credential, not a weaker one.
#[actix_web::test]
async fn a_browser_session_is_refused_at_the_mcp_door() {
    let (state, _dir) = hosted().await;
    let accounts = SqliteAccountRepository::new(state.pool.clone());
    let user = AuthUserId::new();
    accounts
        .grant(
            accounts.bootstrap().await.unwrap().id,
            user,
            "someone@example.com",
            AccountRole::Owner,
        )
        .await
        .expect("grant");

    let app = serve!(state);
    let session = session_for(user);

    // Through the front door it is exactly what it should be.
    let rest = test::call_service(
        &app,
        TestRequest::get()
            .uri("/api/v1/projects")
            .insert_header(bearer(&session))
            .to_request(),
    )
    .await;
    assert_eq!(rest.status(), StatusCode::OK);

    // Marked as arriving through MCP, the same token is refused: an agent
    // credential is required there, and a session is not one.
    let as_agent = test::call_service(
        &app,
        TestRequest::get()
            .uri("/api/v1/projects")
            .insert_header(bearer(&session))
            .insert_header((forma_server::auth::MCP_ENTRY_HEADER, "mcp"))
            .to_request(),
    )
    .await;
    assert_eq!(
        as_agent.status(),
        StatusCode::UNAUTHORIZED,
        "forging the marker must only ever demand more, never less"
    );
}

/// Plan §113.1: the deployment token is a migration secret, not a credential.
///
/// It opens the door until somebody walks through it, and then it does not.
/// Keeping it would keep the one credential that belongs to nobody, records no
/// actor, and cannot be revoked without rotating every other holder.
#[actix_web::test]
async fn the_deployment_token_stops_working_once_the_workspace_is_claimed() {
    let (state, _dir) = hosted().await;
    let accounts = SqliteAccountRepository::new(state.pool.clone());

    let app = test::init_service(
        App::new()
            .wrap(actix_web::middleware::from_fn(
                forma_server::auth::require_bearer,
            ))
            .app_data(web::Data::new(state))
            .app_data(web::Data::new(forma_server::auth::RemoteToken::new(
                "the-deployment-token",
            )))
            .configure(routes::configure),
    )
    .await;

    let ask = || {
        TestRequest::get()
            .uri("/api/v1/projects")
            .insert_header(bearer("the-deployment-token"))
            .to_request()
    };

    // Before: it is how an unclaimed deployment is reached at all.
    assert_eq!(
        test::call_service(&app, ask()).await.status(),
        StatusCode::OK
    );

    accounts
        .claim_bootstrap(AuthUserId::new(), "owner@example.com")
        .await
        .expect("claim")
        .expect("the first claim succeeds");

    // After: the same token, refused.
    assert_eq!(
        test::call_service(&app, ask()).await.status(),
        StatusCode::UNAUTHORIZED,
        "a claimed workspace must not still answer to the migration secret"
    );
}

/// And what replaces it keeps working, because otherwise retiring it would just
/// be locking the door with everyone outside.
#[actix_web::test]
async fn a_personal_access_token_outlives_the_claim() {
    use persistence::{ApiKeyRepository, SqliteApiKeyRepository};

    let (state, _dir) = hosted().await;
    let accounts = SqliteAccountRepository::new(state.pool.clone());
    let keys = SqliteApiKeyRepository::new(state.pool.clone());

    let account = accounts.bootstrap().await.unwrap().id;
    let minted = keys
        .create(account, None, "Smoke test")
        .await
        .expect("mint");

    accounts
        .claim_bootstrap(AuthUserId::new(), "owner@example.com")
        .await
        .expect("claim");

    let app = test::init_service(
        App::new()
            .wrap(actix_web::middleware::from_fn(
                forma_server::auth::require_bearer,
            ))
            .app_data(web::Data::new(state))
            .app_data(web::Data::new(forma_server::auth::RemoteToken::new(
                "the-deployment-token",
            )))
            .configure(routes::configure),
    )
    .await;

    let response = test::call_service(
        &app,
        TestRequest::get()
            .uri("/api/v1/projects")
            .insert_header(bearer(&minted.token))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}
