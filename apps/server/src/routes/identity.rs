//! Signing in, and getting an account (plan sections 110.2, 107.3 and 113.1).
//!
//! Everything here is about the moment before a person owns anything: whether
//! this deployment wants a credential at all, and the two doors out of holding
//! an identity with no account behind it.

use actix_web::{get, post, web, HttpMessage, HttpRequest, HttpResponse};
use design_core::AccountRole;
use persistence::AccountRepository;
use serde::Serialize;
use ts_rs::TS;

use crate::config::AuthMode;
use crate::error::ApiError;
use crate::jwt::VerifiedIdentity;
use crate::state::AppState;

/// What the editor asks before it draws anything.
///
/// Two booleans rather than one, because "you are not signed in" and "there is
/// nothing to sign in to" are different situations and the editor draws a
/// different first screen for each. A local Forma has no provider, and asking a
/// person there for a credential would be asking for one that does not exist.
///
/// Public, and it says nothing else: no account, no user, no project count.
#[derive(Serialize, TS)]
#[ts(export)]
pub struct IdentityStatus {
    /// Whether this deployment verifies identity.
    pub required: bool,
}

#[get("/identity")]
async fn status(state: web::Data<AppState>) -> HttpResponse {
    HttpResponse::Ok().json(IdentityStatus {
        required: state.mode != AuthMode::Local,
    })
}

/// What a signed-in person with no account is offered.
#[derive(Serialize, TS)]
#[ts(export)]
pub struct Onboarding {
    /// Whether an unclaimed invite exists for this address.
    pub invited: bool,
    /// The address the provider proved, echoed so the page can say which
    /// invitation it is looking for.
    pub email: Option<String>,
}

/// The address the provider says it proved.
///
/// The invite gate matches on an address, so it is only a gate if the address
/// was verified. An unverified one is treated as no address at all rather than
/// as a claim to check — otherwise anyone could register as an invited address
/// and walk in.
fn proven_email(identity: &VerifiedIdentity) -> Option<&str> {
    identity
        .email_verified
        .then_some(identity.email.as_deref())
        .flatten()
}

fn identity_of(request: &HttpRequest) -> Result<VerifiedIdentity, ApiError> {
    request
        .extensions()
        .get::<VerifiedIdentity>()
        .cloned()
        .ok_or_else(|| {
            ApiError::unauthorized(
                "UNAUTHORIZED",
                "this endpoint needs a signed-in identity",
                "Sign in first; these routes are for a session that has no account yet.",
            )
        })
}

#[get("/onboarding")]
async fn onboarding(
    request: HttpRequest,
    state: web::Data<AppState>,
) -> Result<HttpResponse, ApiError> {
    let identity = identity_of(&request)?;
    let email = proven_email(&identity).map(str::to_string);

    let invited = match &email {
        Some(address) => state
            .accounts
            .open_invite(address)
            .await
            .map_err(storage)?
            .is_some(),
        None => false,
    };

    Ok(HttpResponse::Ok().json(Onboarding { invited, email }))
}

/// Creating an account, behind the invite gate of plan 107.3.
///
/// Signing in is not the gate; owning an account is. Without this every
/// verified identity could mint itself a workspace, which turns open provider
/// registration into open access.
#[post("/accounts")]
async fn create(
    request: HttpRequest,
    state: web::Data<AppState>,
) -> Result<HttpResponse, ApiError> {
    let identity = identity_of(&request)?;

    let Some(email) = proven_email(&identity) else {
        return Err(ApiError::forbidden(
            "ACCOUNT_REQUIRED",
            "this Forma is invite-only, and this identity has no verified address",
            "Confirm your email address with the identity provider, then try again.",
        ));
    };

    if state
        .accounts
        .membership_of(identity.subject)
        .await
        .map_err(storage)?
        .is_some()
    {
        return Err(ApiError::bad_request(
            "INVALID_STATE",
            "this identity already has an account",
            "Nothing to do; reload the editor.",
        ));
    }

    if state
        .accounts
        .open_invite(email)
        .await
        .map_err(storage)?
        .is_none()
    {
        // The same answer whether no invite was ever minted or it was already
        // used: neither tells a stranger which addresses were invited.
        return Err(ApiError::forbidden(
            "ACCOUNT_REQUIRED",
            "this Forma is invite-only",
            "Ask whoever runs this deployment for an invitation.",
        ));
    }

    // Consume the invite first. If the account creation below failed after the
    // invite had been spent, the person would be locked out of a workspace they
    // were entitled to; failing this way round leaves the invite usable.
    let account = state.accounts.create("Workspace").await.map_err(storage)?;
    state
        .accounts
        .grant(account.id, identity.subject, email, AccountRole::Owner)
        .await
        .map_err(storage)?;
    state
        .accounts
        .claim_invite(email, identity.subject)
        .await
        .map_err(storage)?;

    Ok(HttpResponse::Created().json(account))
}

fn storage(error: persistence::PersistenceError) -> ApiError {
    ApiError::internal("STORAGE_ERROR", error.to_string())
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(status).service(onboarding).service(create);
}
