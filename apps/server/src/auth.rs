//! Bearer auth for remote mode (plan section 57).
//!
//! Deliberately not OAuth: §2.2 puts that out of scope for V1, and §57 says the
//! first remote testing is a bearer token that can be migrated later if a
//! specific integration needs it.
//!
//! The token is never stored in memory in the clear beyond startup: what is kept
//! is its hash, and an incoming header is hashed and compared against it. blake3
//! is already in the tree for asset hashing, and its `Hash` compares in constant
//! time, so a wrong token cannot be found one byte at a time.

use actix_web::body::{BoxBody, MessageBody};
use actix_web::dev::{ServiceRequest, ServiceResponse};
use actix_web::middleware::Next;
use actix_web::{web, Error, ResponseError};

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

/// Requires `Authorization: Bearer <token>` when one is configured.
///
/// With no token configured this is a pass-through, which is the local case:
/// the server is on loopback and the person running it is the only caller.
/// `main` refuses to bind a non-loopback address without one, so "no token" and
/// "reachable from the network" cannot both be true.
pub async fn require_bearer(
    request: ServiceRequest,
    next: Next<impl MessageBody + 'static>,
) -> Result<ServiceResponse<BoxBody>, Error> {
    let expected = request.app_data::<web::Data<RemoteToken>>().cloned();

    if let Some(expected) = expected {
        if !is_open(request.path()) && !presented(&request).is_some_and(|t| expected.accepts(&t)) {
            // No detail about why: a caller learning whether the header was
            // missing, malformed or simply wrong is a caller being helped to
            // guess. The hint is for the operator reading a log, not the client.
            let refusal = ApiError::unauthorized(
                "UNAUTHORIZED",
                "this server requires a bearer token",
                "Send Authorization: Bearer <token>, with the value of FORMA_REMOTE_TOKEN.",
            )
            .error_response();
            return Ok(request.into_response(refusal));
        }
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
