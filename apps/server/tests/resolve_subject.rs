//! The resolve endpoint the Pontis sidecar asks before a session exists
//! (Pontis plan §10(b) D2).
//!
//! The only place a service credential is accepted without a represented user
//! and an application context, so most of what follows is about how little it
//! accepts: one credential class, a subject token verified by the existing
//! `Verifier`, never the MCP door, and no access to anything but the answer.

use std::sync::Arc;

use actix_web::{
    http::StatusCode,
    test::{self, TestRequest},
    web, App,
};
use design_core::{AccountId, AccountRole, AuthUserId};
use forma_server::auth::{RemoteToken, MCP_ENTRY_HEADER, RESOLVE_PATH, SUBJECT_TOKEN_HEADER};
use forma_server::config::AuthMode;
use forma_server::jwt::{JwksSource, JwtError, Verifier};
use forma_server::{routes, state::AppState};
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use persistence::{
    AccountRepository, ApiKeyRepository, ProjectRepository, ServiceCredentialRepository,
};
use renderer::FontRegistry;
use serde::Serialize;

const ISSUER: &str = "https://project.supabase.co/auth/v1";
const AUDIENCE: &str = "authenticated";
const SIGNING_KEY: &[u8] = include_bytes!("../../../fixtures/identity/test-signing-key.pkcs8.der");
const JWKS: &str = include_str!("../../../fixtures/identity/test-jwks.json");
const DEPLOYMENT_TOKEN: &str = "the-deployment-token";

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

fn session_expiring(user: AuthUserId, exp: i64) -> String {
    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some("test-key-1".into());
    encode(
        &header,
        &Claims {
            sub: user.to_string(),
            iss: ISSUER.into(),
            aud: AUDIENCE.into(),
            exp,
            email: "someone@example.com".into(),
            email_verified: true,
        },
        &EncodingKey::from_ec_der(SIGNING_KEY),
    )
    .expect("mint")
}

fn session_for(user: AuthUserId) -> String {
    session_expiring(user, chrono::Utc::now().timestamp() + 3600)
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

/// With a deployment token configured too, so that class can be presented.
macro_rules! serve {
    ($state:expr) => {
        test::init_service(
            App::new()
                .wrap(actix_web::middleware::from_fn(
                    forma_server::auth::require_bearer,
                ))
                .app_data(web::Data::new($state))
                .app_data(web::Data::new(RemoteToken::new(DEPLOYMENT_TOKEN)))
                .configure(routes::configure),
        )
        .await
    };
}

fn bearer(token: &str) -> (&'static str, String) {
    ("authorization", format!("Bearer {token}"))
}

/// A person who owns an account with a project in it, and a sidecar credential.
struct World {
    user: AuthUserId,
    account: AccountId,
    service: String,
}

async fn world(state: &AppState) -> World {
    let account = state.accounts.create("Theirs").await.expect("account").id;
    let user = AuthUserId::new();
    state
        .accounts
        .grant(account, user, "someone@example.com", AccountRole::Owner)
        .await
        .expect("grant");
    state
        .projects
        .create(account, "Their project")
        .await
        .expect("project");
    let service = state
        .service_credentials
        .create("pontis sidecar")
        .await
        .expect("mint")
        .token;
    World {
        user,
        account,
        service,
    }
}

fn resolve(credential: &str, subject: &str) -> TestRequest {
    TestRequest::get()
        .uri(RESOLVE_PATH)
        .insert_header(bearer(credential))
        .insert_header((SUBJECT_TOKEN_HEADER, subject.to_string()))
}

async fn code_of(response: actix_web::dev::ServiceResponse) -> String {
    let body: serde_json::Value = test::read_body_json(response).await;
    body["error"]["code"]
        .as_str()
        .expect("the §32 envelope")
        .to_string()
}

/// The positive control every refusal below is measured against.
#[actix_web::test]
async fn a_valid_subject_resolves_to_its_account_and_role() {
    let (state, _dir) = hosted().await;
    let world = world(&state).await;
    let app = serve!(state);

    let response = test::call_service(
        &app,
        resolve(&world.service, &session_for(world.user)).to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = test::read_body_json(response).await;
    assert_eq!(
        body,
        serde_json::json!({
            "subject": world.user.to_string(),
            "account_id": world.account.to_string(),
            "role": "owner",
        }),
        "identifiers only: nothing in the answer is a credential"
    );
}

#[actix_web::test]
async fn the_service_credential_is_required() {
    let (state, _dir) = hosted().await;
    let world = world(&state).await;
    let app = serve!(state);

    let bare = TestRequest::get()
        .uri(RESOLVE_PATH)
        .insert_header((SUBJECT_TOKEN_HEADER, session_for(world.user)))
        .to_request();
    assert_eq!(
        test::call_service(&app, bare).await.status(),
        StatusCode::UNAUTHORIZED,
        "a subject token with no credential in Authorization"
    );

    let unknown = format!("{}not-a-real-secret", persistence::SERVICE_PREFIX);
    let response = test::call_service(
        &app,
        resolve(&unknown, &session_for(world.user)).to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(code_of(response).await, "UNAUTHORIZED");
}

/// Every other credential class in the Authorization slot is refused, even
/// carrying a perfectly valid subject token for a person with an account.
#[actix_web::test]
async fn no_other_credential_class_may_ask() {
    let (state, _dir) = hosted().await;
    let world = world(&state).await;
    let pat = state
        .api_keys
        .create(world.account, Some(world.user), "a script")
        .await
        .expect("pat")
        .token;
    let browser = session_for(world.user);
    let app = serve!(state);

    for (class, credential) in [
        ("a personal access token", pat.as_str()),
        ("the deployment token", DEPLOYMENT_TOKEN),
        ("a browser session", browser.as_str()),
    ] {
        let response = test::call_service(&app, resolve(credential, &browser).to_request()).await;
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{class} in Authorization"
        );
    }
}

#[actix_web::test]
async fn an_absent_invalid_or_expired_subject_token_is_refused() {
    let (state, _dir) = hosted().await;
    let world = world(&state).await;
    let expired = session_expiring(world.user, chrono::Utc::now().timestamp() - 3600);
    let app = serve!(state);

    let absent = TestRequest::get()
        .uri(RESOLVE_PATH)
        .insert_header(bearer(&world.service))
        .to_request();
    let response = test::call_service(&app, absent).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "absent");
    assert_eq!(code_of(response).await, "SUBJECT_TOKEN_INVALID");

    for (case, subject) in [
        ("invalid", "not.a.jwt"),
        ("the sidecar's own credential", world.service.as_str()),
        ("expired", expired.as_str()),
    ] {
        let response =
            test::call_service(&app, resolve(&world.service, subject).to_request()).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{case}");
        assert_eq!(code_of(response).await, "SUBJECT_TOKEN_INVALID", "{case}");
    }
}

/// A valid person with no membership is told so, in the shape the session arm
/// already uses, and nothing is invented for them.
#[actix_web::test]
async fn a_subject_with_no_membership_is_account_required() {
    let (state, _dir) = hosted().await;
    let world = world(&state).await;
    let app = serve!(state);

    let response = test::call_service(
        &app,
        resolve(&world.service, &session_for(AuthUserId::new())).to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body: serde_json::Value = test::read_body_json(response).await;
    assert_eq!(body["error"]["code"], "ACCOUNT_REQUIRED");
    assert!(body.get("account_id").is_none());
}

/// The sidecar must not mark this call: a browser session is verified with
/// `Accepts::Either`, and a marked request is somebody else holding the
/// credential at the MCP door.
#[actix_web::test]
async fn a_request_marked_as_mcp_is_refused() {
    let (state, _dir) = hosted().await;
    let world = world(&state).await;
    let app = serve!(state);

    let response = test::call_service(
        &app,
        resolve(&world.service, &session_for(world.user))
            .insert_header((MCP_ENTRY_HEADER, "mcp"))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn a_revoked_credential_cannot_resolve() {
    let (state, _dir) = hosted().await;
    let world = world(&state).await;
    let id = state
        .service_credentials
        .authenticate(&world.service)
        .await
        .unwrap()
        .expect("the credential")
        .id;
    state.service_credentials.revoke(id).await.expect("revoke");
    let app = serve!(state);

    let response = test::call_service(
        &app,
        resolve(&world.service, &session_for(world.user)).to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(code_of(response).await, "SERVICE_CREDENTIAL_REVOKED");
}

/// The exception is the path, not the subject token: the same pair of
/// credentials anywhere else is a service request with no represented user, and
/// is refused. Resolving acts as nobody. Refused as out of scope since F-50-1:
/// the credential is checked first, so these are 403 rather than 401.
#[actix_web::test]
async fn the_exception_does_not_leave_its_path() {
    let (state, _dir) = hosted().await;
    let world = world(&state).await;
    let app = serve!(state);

    for path in [
        "/api/v1/projects",
        "/api/v1/status",
        "/api/v1/service/resolve/x",
        // A trailing slash, the first thing anyone probing the exception
        // tries. Nothing normalizes paths here, so it is not `RESOLVE_PATH`.
        "/api/v1/service/resolve/",
        // Percent-encoded. The middleware compares the raw path, so whatever
        // the router later makes of `%72`, this never takes the exception.
        "/api/v1/service/%72esolve",
    ] {
        let response = test::call_service(
            &app,
            TestRequest::get()
                .uri(path)
                .insert_header(bearer(&world.service))
                .insert_header((SUBJECT_TOKEN_HEADER, session_for(world.user)))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{path}");
        assert_eq!(
            code_of(response).await,
            "SERVICE_CREDENTIAL_OUT_OF_SCOPE",
            "{path}"
        );
    }

    let post = TestRequest::post()
        .uri(RESOLVE_PATH)
        .insert_header(bearer(&world.service))
        .insert_header((SUBJECT_TOKEN_HEADER, session_for(world.user)))
        .to_request();
    assert!(
        !test::call_service(&app, post).await.status().is_success(),
        "only the read is routed"
    );
}
