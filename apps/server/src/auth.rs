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

use persistence::AccountRepository;

use crate::error::ApiError;

/// The configured token, reduced to something a comparison cannot leak.
#[derive(Clone)]
pub struct RemoteToken(blake3::Hash);

impl RemoteToken {
    pub fn new(token: &str) -> Self {
        Self(blake3::hash(token.as_bytes()))
    }

    pub(crate) fn accepts(&self, presented: &str) -> bool {
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
    // Liveness. It reports nothing about the workspace, and Caddy and any
    // monitor need it before anyone has configured a credential.
    if path == "/api/v1/health" || path == "/api/v1/identity" {
        return true;
    }

    // Everything outside `/api/` is the built SPA or the public skill, and both
    // are meant to be readable by someone who has no credential yet (plan
    // §110.1).
    //
    // This reverses a Phase 11 decision on purpose. The whole origin was closed
    // because serving an app shell to a visitor with no credential "would only
    // let it fail with 401s in the console" — true when the shell had no way to
    // ask for one. It has one now: `/login` exists precisely to be reached
    // before you are authenticated, and Supabase redirects a person to
    // `/oauth/consent` with no Forma credential at all. Closed, both are a 401
    // and the hosted flow cannot start.
    //
    // What this discloses is the bundle: no project, no design, no asset, no
    // name. Every one of those is still behind `/api/`.
    !path.starts_with("/api/")
}

/// Paths a verified identity may reach *before* it owns an account.
///
/// Everything else under `/api/` answers `ACCOUNT_REQUIRED` without one, which
/// is correct and would also be a dead end: the two doors out of that state are
/// here. They are reached with a provider session and no membership, so their
/// handlers work from the `VerifiedIdentity` rather than an `AuthContext` —
/// there is no account yet for a context to name.
fn is_onboarding(path: &str) -> bool {
    matches!(
        path,
        "/api/v1/onboarding" | "/api/v1/accounts" | "/api/v1/bootstrap/claim"
    )
}

/// Resolves a deployment token, a PAT or a provider session into the request's
/// `AuthContext` (plan sections 108 and 112).
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
    } else if let Some((state, verifier)) = state
        .as_ref()
        .zip(state.as_ref().and_then(|state| state.verifier.clone()))
        .filter(|_| hosted)
        .zip(credential.as_deref())
        .map(|((state, verifier), _)| (state.clone(), verifier))
    {
        // A session token from the identity provider (plan section 111.4).
        //
        // Reached only after the deployment token and the PAT prefix have both
        // failed to match, so the three credential classes cannot be confused
        // for one another.
        let Some(token) = credential.as_deref() else {
            return Ok(request.into_response(refusal()));
        };

        let identity = match verifier.verify(token).await {
            Ok(identity) => identity,
            Err(crate::jwt::JwtError::ProviderUnreachable) => {
                // Distinguished from a bad token on purpose: this one is ours to
                // fix, and answering 401 would send a person to re-authenticate
                // against something that is down.
                return Ok(request.into_response(
                    ApiError::internal(
                        "IDENTITY_UNAVAILABLE",
                        "the identity provider could not be reached",
                    )
                    .error_response(),
                ));
            }
            Err(_) => return Ok(request.into_response(refusal())),
        };

        let onboarding = is_onboarding(request.path());
        // Available to a handler even when there is no account behind it, which
        // is what the onboarding routes work from.
        request.extensions_mut().insert(identity.clone());

        match state.accounts.membership_of(identity.subject).await {
            Ok(Some(membership)) => Some(AuthContext {
                account_id: membership.account_id,
                user_id: Some(identity.subject),
                // Supabase populates `client_id` for an OAuth client and leaves
                // it null for a direct user session, which is exactly the
                // distinction plan section 112 rests on.
                kind: if identity.client_id.is_some() {
                    AuthKind::McpOAuth
                } else {
                    AuthKind::BrowserUser
                },
                client_id: identity.client_id,
                api_key_id: None,
            }),
            // A valid identity that is not an authorized Forma account. The one
            // 403 in this codebase, and never for an object in another account —
            // that stays a 404, because a 403 there would confirm it exists.
            // A verified identity with no account, on its way to getting one.
            Ok(None) if onboarding => None,
            Ok(None) => {
                return Ok(request.into_response(
                    ApiError::forbidden(
                        "ACCOUNT_REQUIRED",
                        "this identity has no Forma account yet",
                        "Create an account with an invite, or claim the existing workspace.",
                    )
                    .error_response(),
                ))
            }
            Err(_) => {
                return Ok(request.into_response(
                    ApiError::internal(
                        "AUTH_STORAGE_ERROR",
                        "the account for this identity could not be read",
                    )
                    .error_response(),
                ))
            }
        }
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
