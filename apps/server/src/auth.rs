//! Bearer authentication at the server boundary (plan §§57 and 112.1).
//!
//! The deployment token, each PAT and each service credential are stored only
//! as hashes and compared as BLAKE3 `Hash` values, whose equality is
//! constant-time. Provider sessions are verified by `jwt.rs` at this same choke
//! point.
//!
//! The service credential (Pontis plan §10(b) D1) is the fifth class to arrive
//! here, added in the existing idiom: its own prefix and its own arm. The pass
//! that orders these deliberately is scheduled after PV-1 (plan-status.md,
//! *Credentials and identity need a deliberate pass*).

use actix_web::body::{BoxBody, MessageBody};
use actix_web::dev::{ServiceRequest, ServiceResponse};
use actix_web::middleware::Next;
use actix_web::{web, Error, HttpMessage, ResponseError};
use design_core::{AccountId, AuthContext, AuthKind, AuthUserId};
use persistence::{ApiKeyRepository, ServiceCredentialRepository};

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

/// Set by `forma-mcp` on every request it forwards, and by nothing else.
///
/// See where it is read for why trusting it is safe: it only ever narrows what
/// is accepted.
pub const MCP_ENTRY_HEADER: &str = "x-forma-entry";

/// Who a service-credential request acts for: the represented user's subject.
///
/// Trusted only on the service arm, because only the sidecar can present that
/// credential and the sidecar is what authenticated the person. Everywhere else
/// it is ignored. Even there it is not taken on faith: the user must hold a
/// membership in the named context, checked on every request (ADR-024).
pub const REPRESENTED_USER_HEADER: &str = "x-forma-represented-user";

/// The application context a service-credential request is bound to: an
/// `AccountId`, which is what Forma maps `ApplicationContextId` to (Pontis plan
/// §5).
pub const APPLICATION_CONTEXT_HEADER: &str = "x-forma-application-context";

/// The provider session a resolve request asks about (Pontis plan §10(b) D2).
///
/// Read on [`RESOLVE_PATH`] and nowhere else. `Authorization` carries the
/// sidecar's own credential on that call, so the token being asked about needs
/// a slot of its own.
pub const SUBJECT_TOKEN_HEADER: &str = "x-forma-subject-token";

/// The one place a service credential is accepted without a represented user
/// and an application context, because it is what discovers them.
pub const RESOLVE_PATH: &str = "/api/v1/service/resolve";

/// A subject token the sidecar asked about, verified, on the resolve path only.
///
/// Deliberately not an `AuthContext` and not a bare `VerifiedIdentity`: the
/// request acts as nobody, so nothing that authorizes on either can find it,
/// and only the resolve handler reads this.
#[derive(Debug, Clone)]
pub struct SubjectToResolve {
    pub(crate) identity: crate::jwt::VerifiedIdentity,
}

/// Paths a verified identity may reach *before* it owns an account.
///
/// Everything else under `/api/` answers `ACCOUNT_REQUIRED` without one, which
/// is correct and would also be a dead end: the two doors out of that state are
/// here. They are reached with a provider session and no membership, so their
/// handlers work from the `VerifiedIdentity` rather than an `AuthContext` —
/// there is no account yet for a context to name.
fn is_onboarding(path: &str) -> bool {
    matches!(path, "/api/v1/onboarding" | "/api/v1/accounts")
}

/// Resolves a deployment token, a PAT, a service credential or a provider
/// session into the request's `AuthContext` (plan sections 108 and 112).
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

    // Once the workspace has been claimed, the deployment token stops being a
    // credential (plan section 113.1). It becomes a migration secret: the thing
    // that proved you run the machine, spent once to hand the workspace to a
    // person.
    //
    // Keeping it would keep the weakest link the whole phase set out to retire —
    // a shared secret that belongs to nobody, records no actor, cannot be
    // revoked without rotating every other holder, and reaches an account
    // without passing through a membership. Personal access tokens replace it
    // for automation, and `forma-server grant-owner` replaces it for recovery,
    // behind shell access rather than a header.
    //
    // Only asked when the token is what was presented, so nothing else pays for
    // the read.
    let remote_matches = if remote_matches {
        match state.as_ref() {
            Some(state) => match state.accounts.bootstrap().await {
                Ok(account) => account.is_unowned(),
                // Unreadable is not a reason to widen. Everything else is
                // refused when the database is unreachable; this is not the
                // exception.
                Err(_) => false,
            },
            None => false,
        }
    } else {
        false
    };

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
            service_credential_id: None,
        })
    } else if let Some(presented) = credential
        .as_deref()
        .filter(|value| value.starts_with(persistence::SERVICE_PREFIX))
    {
        // The Pontis integration sidecar at Boundary F (Pontis plan §10(b) D1).
        //
        // Its prefix is disjoint from `forma_pat_`, so this arm and the PAT arm
        // cannot both match, and neither can the provider-session arm below,
        // which is only reached when neither prefix did.
        let Some(state) = state.as_ref() else {
            return Ok(request.into_response(refusal()));
        };

        // **Never at the MCP door.** `forma-mcp` marks every request it
        // forwards, and its `admits()` probes `/api/v1/status` carrying the
        // host's bearer and no represented user. A sidecar credential leaked to
        // an MCP host must not open `/mcp`: the sidecar talks to REST directly
        // and has no reason to arrive marked, so a marked request with this
        // prefix is somebody else holding it.
        if request.headers().contains_key(MCP_ENTRY_HEADER) {
            return Ok(request.into_response(refusal()));
        }

        // **The single exception to the rule below**, and bounded by the path
        // rather than by anything the caller says: resolving who a session
        // token belongs to and which account they may enter is what produces a
        // represented user and a context, so it cannot require them. It acts
        // as nobody — no `AuthContext` is set — and its handler reaches no
        // project, design, asset or template.
        if request.path() == RESOLVE_PATH {
            return resolve_subject(request, next, state, presented).await;
        }

        // **The credential alone is never account access.** It names no person
        // and no workspace — that is what lets one sidecar serve many people —
        // so a request without both is refused rather than given a default.
        // Any default here would make this credential the deployment token
        // again, with a longer name.
        let Some((user, context)) = represented(&request) else {
            return Ok(request.into_response(refusal()));
        };

        let service = match state.service_credentials.authenticate(presented).await {
            Ok(Some(service)) => service,
            Ok(None) => return Ok(request.into_response(refusal())),
            Err(_) => {
                return Ok(request.into_response(
                    ApiError::internal(
                        "AUTH_STORAGE_ERROR",
                        "the service credential could not be verified",
                    )
                    .error_response(),
                ))
            }
        };

        if service.is_revoked() {
            return Ok(request.into_response(service_revoked_refusal()));
        }

        // Revalidated on every request, never cached: ADR-024 says a stored
        // context does not preserve access that has since been revoked.
        match state.accounts.membership_of(user).await {
            Ok(Some(membership)) if membership.account_id == context => {}
            Ok(_) => {
                return Ok(request.into_response(
                    ApiError::forbidden(
                        "CONTEXT_FORBIDDEN",
                        "the represented user may not act in this application context",
                        "Resolve the user's account again; the membership may have changed.",
                    )
                    .error_response(),
                ))
            }
            Err(_) => {
                return Ok(request.into_response(
                    ApiError::internal(
                        "AUTH_STORAGE_ERROR",
                        "the represented user's account could not be read",
                    )
                    .error_response(),
                ))
            }
        }

        if state
            .service_credentials
            .touch_last_used(service.id)
            .await
            .is_err()
        {
            return Ok(request.into_response(
                ApiError::internal(
                    "AUTH_STORAGE_ERROR",
                    "the service credential's last use could not be recorded",
                )
                .error_response(),
            ));
        }

        Some(AuthContext {
            account_id: context,
            user_id: Some(user),
            kind: AuthKind::Service,
            client_id: None,
            api_key_id: None,
            service_credential_id: Some(service.id),
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
        // Reached only after the deployment token and the PAT and service
        // prefixes have all failed to match, so the credential classes cannot
        // be confused for one another.
        let Some(token) = credential.as_deref() else {
            return Ok(request.into_response(refusal()));
        };

        // Which door this arrived at decides which credential classes are
        // acceptable (plan section 112). `forma-mcp` marks its own requests;
        // everything else is a browser or a script talking to REST directly.
        //
        // **The marker can only ever tighten the check**, and that is what makes
        // it safe to trust for a security decision. Setting it on a direct call
        // to `/api/v1/*` demands a *stricter* credential — an agent audience
        // instead of either — so forging it buys an attacker nothing. And it
        // cannot be stripped from a real MCP request, because `forma-mcp` is
        // what adds it, on the loopback hop that the internet cannot reach.
        let accepts = if request.headers().contains_key(MCP_ENTRY_HEADER) {
            crate::jwt::Accepts::AgentOnly
        } else {
            crate::jwt::Accepts::Either
        };

        let identity = match verifier.verify(token, accepts).await {
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
                service_credential_id: None,
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

/// The service arm on [`RESOLVE_PATH`] (Pontis plan §10(b) D2).
///
/// The sidecar's credential is checked first, so a caller without one learns
/// nothing about the subject token and cannot spend the verifier's JWKS refresh.
/// The subject token is then verified by the same `Verifier` as a browser
/// session — never a second implementation — with `Accepts::Either`, which is
/// why the caller has already been refused if it arrived marked as MCP.
async fn resolve_subject(
    request: ServiceRequest,
    next: Next<impl MessageBody + 'static>,
    state: &web::Data<crate::state::AppState>,
    presented: &str,
) -> Result<ServiceResponse<BoxBody>, Error> {
    let service = match state.service_credentials.authenticate(presented).await {
        Ok(Some(service)) => service,
        Ok(None) => return Ok(request.into_response(refusal())),
        Err(_) => {
            return Ok(request.into_response(
                ApiError::internal(
                    "AUTH_STORAGE_ERROR",
                    "the service credential could not be verified",
                )
                .error_response(),
            ))
        }
    };

    if service.is_revoked() {
        return Ok(request.into_response(service_revoked_refusal()));
    }

    let Some(verifier) = state.verifier.clone() else {
        return Ok(request.into_response(
            ApiError::internal(
                "IDENTITY_UNAVAILABLE",
                "this deployment verifies no identity provider",
            )
            .error_response(),
        ));
    };

    let token = request
        .headers()
        .get(SUBJECT_TOKEN_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let Some(token) = token else {
        return Ok(request.into_response(subject_refusal()));
    };

    let identity = match verifier.verify(&token, crate::jwt::Accepts::Either).await {
        Ok(identity) => identity,
        Err(crate::jwt::JwtError::ProviderUnreachable) => {
            return Ok(request.into_response(
                ApiError::internal(
                    "IDENTITY_UNAVAILABLE",
                    "the identity provider could not be reached",
                )
                .error_response(),
            ));
        }
        Err(_) => return Ok(request.into_response(subject_refusal())),
    };

    if state
        .service_credentials
        .touch_last_used(service.id)
        .await
        .is_err()
    {
        return Ok(request.into_response(
            ApiError::internal(
                "AUTH_STORAGE_ERROR",
                "the service credential's last use could not be recorded",
            )
            .error_response(),
        ));
    }

    request
        .extensions_mut()
        .insert(SubjectToResolve { identity });
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

/// The represented user and application context a service request names.
///
/// Both or neither: `None` when either is absent or not a UUID, so the caller
/// refuses rather than filling the gap.
fn represented(request: &ServiceRequest) -> Option<(AuthUserId, AccountId)> {
    let uuid = |name: &str| {
        request
            .headers()
            .get(name)?
            .to_str()
            .ok()
            .and_then(|raw| uuid::Uuid::parse_str(raw.trim()).ok())
    };
    Some((
        AuthUserId(uuid(REPRESENTED_USER_HEADER)?),
        AccountId(uuid(APPLICATION_CONTEXT_HEADER)?),
    ))
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
        // Deliberately does not name the deployment token. On a claimed
        // workspace that credential is spent for good (§113.1), and pointing
        // somebody at it would send them to look for a value that cannot work —
        // the most expensive kind of wrong hint, because it looks like an
        // answer.
        "Sign in to Forma, or present an unrevoked personal access token. \
         Create one with `forma-server api-key <name>` on the machine.",
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

/// The sidecar authenticated, and the session it asked about did not.
///
/// Its own code because the two failures send the sidecar different ways: this
/// one means the person signs in again, `UNAUTHORIZED` means the sidecar is
/// misconfigured. Absent, malformed, expired and foreign tokens are one answer,
/// for the same reason `refusal` is.
fn subject_refusal() -> actix_web::HttpResponse {
    ApiError::unauthorized(
        "SUBJECT_TOKEN_INVALID",
        "the subject token is missing or did not verify",
        "Have the person sign in again and resolve the new session token.",
    )
    .error_response()
}

fn service_revoked_refusal() -> actix_web::HttpResponse {
    ApiError::unauthorized(
        "SERVICE_CREDENTIAL_REVOKED",
        "this service credential has been revoked",
        "Mint a new one with `forma-server service-credential <name>` and reconfigure the sidecar.",
    )
    .error_response()
}
