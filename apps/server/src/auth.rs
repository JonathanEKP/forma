//! Bearer authentication at the server boundary (plan §§57 and 112.1).
//!
//! The deployment token and each PAT are stored only as hashes and compared as
//! BLAKE3 `Hash` values, whose equality is constant-time. OAuth/JWT validation
//! joins this same choke point in the provider-backed slice; it is deliberately
//! absent here.

use actix_web::body::{BoxBody, MessageBody};
use actix_web::dev::{ServiceRequest, ServiceResponse};
use actix_web::middleware::Next;
use actix_web::{web, Error, HttpMessage, ResponseError};
use design_core::{AuthContext, AuthKind};
use persistence::ApiKeyRepository;

use crate::error::ApiError;

/// The configured token, reduced to something a comparison cannot leak.
#[derive(Clone)]
pub struct RemoteToken(blake3::Hash);

impl RemoteToken {
    pub fn new(token: &str) -> Self {
        Self(blake3::hash(token.as_bytes()))
    }

    fn accepts(&self, presented: &str) -> bool {
        // blake3::Hash's PartialEq is constant time.
        self.0 == blake3::hash(presented.as_bytes())
    }
}

impl std::fmt::Debug for RemoteToken {
    /// So a token cannot reach a log through a derived Debug on some struct that
    /// happens to hold one.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RemoteToken(…)")
    }
}

/// Paths that answer without a token.
///
/// Only liveness. It reports nothing about the workspace, and Caddy and any
/// monitor need to reach it before anyone has configured a credential.
fn is_open(path: &str) -> bool {
    path == "/api/v1/health"
}

/// Resolves a deployment token or PAT into the request's `AuthContext`.
pub async fn require_bearer(
    request: ServiceRequest,
    next: Next<impl MessageBody + 'static>,
) -> Result<ServiceResponse<BoxBody>, Error> {
    // **"No credential configured" is not a synonym for "no credential
    // required".** It is only the same thing in local mode, where the security
    // boundary is the process that started Forma. A hosted deployment that
    // reached this line with nothing to check would serve the whole workspace to
    // anyone, and the bind guard can no longer catch that: it rests on "loopback
    // means local", which a reverse proxy in front of loopback makes false.
    let state = request
        .app_data::<web::Data<crate::state::AppState>>()
        .cloned();
    let hosted = state
        .as_ref()
        .map(|state| state.mode != crate::config::AuthMode::Local)
        .unwrap_or(false);
    let expected = request.app_data::<web::Data<RemoteToken>>().cloned();

    // Liveness has to answer before anything is configured, or a monitor and
    // the proxy are locked out of a deployment that is merely misconfigured.
    if is_open(request.path()) {
        return Ok(next.call(request).await?.map_into_boxed_body());
    }

    let credential = presented(&request);
    let remote_matches = expected
        .as_ref()
        .zip(credential.as_deref())
        .is_some_and(|(expected, value)| expected.accepts(value));

    let auth = if remote_matches {
        state
            .as_ref()
            .map(|state| AuthContext::local(state.account))
    } else if let Some(pat) = credential
        .as_deref()
        .filter(|value| value.starts_with(persistence::PAT_PREFIX))
    {
        let Some(state) = state.as_ref() else {
            return Ok(request.into_response(refusal()));
        };
        let key = match state.api_keys.authenticate(pat).await {
            Ok(Some(key)) => key,
            Ok(None) => return Ok(request.into_response(refusal())),
            Err(_) => {
                return Ok(request.into_response(
                    ApiError::internal(
                        "AUTH_STORAGE_ERROR",
                        "the personal access token could not be verified",
                    )
                    .error_response(),
                ))
            }
        };

        if key.is_revoked() {
            return Ok(request.into_response(revoked_refusal()));
        }

        if state
            .api_keys
            .touch_last_used(key.account_id, key.id)
            .await
            .is_err()
        {
            return Ok(request.into_response(
                ApiError::internal(
                    "AUTH_STORAGE_ERROR",
                    "the personal access token's last use could not be recorded",
                )
                .error_response(),
            ));
        }

        Some(AuthContext {
            account_id: key.account_id,
            user_id: key.created_by_user_id,
            kind: AuthKind::McpPat,
            client_id: None,
            api_key_id: Some(key.id),
        })
    } else if expected.is_some() || hosted {
        // No detail about why: a caller learning whether the header was
        // missing, malformed or simply wrong is a caller being helped to guess.
        return Ok(request.into_response(refusal()));
    } else {
        state
            .as_ref()
            .map(|state| AuthContext::local(state.account))
    };

    if let Some(auth) = auth {
        request.extensions_mut().insert(auth);
    }

    Ok(next.call(request).await?.map_into_boxed_body())
}

fn presented(request: &ServiceRequest) -> Option<String> {
    let raw = request
        .headers()
        .get("authorization")?
        .to_str()
        .ok()?
        .trim()
        .to_string();

    // Case-insensitive scheme, because clients differ and this is not the part
    // worth being strict about.
    let (scheme, value) = raw.split_once(' ')?;
    scheme
        .eq_ignore_ascii_case("bearer")
        .then(|| value.trim().to_string())
}

/// One refusal, worded once.
///
/// No detail about why: a caller learning whether the header was missing,
/// malformed or simply wrong is a caller being helped to guess. The hint is for
/// the operator reading a log, not for the client.
fn refusal() -> actix_web::HttpResponse {
    ApiError::unauthorized(
        "UNAUTHORIZED",
        "this server requires a valid bearer credential",
        "Send a valid deployment token or unrevoked Forma personal access token.",
    )
    .error_response()
}

fn revoked_refusal() -> actix_web::HttpResponse {
    ApiError::unauthorized(
        "API_KEY_REVOKED",
        "this personal access token has been revoked",
        "Create a new personal access token and replace the revoked credential.",
    )
    .error_response()
}
