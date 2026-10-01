//! Resolving a session for the Pontis sidecar (Pontis plan §10(b) D2).
//!
//! Pontis needs two answers before a session exists: who the person is, and
//! which application context they may enter (ADR-024 Decision 1). Forma's
//! context is an `AccountId`, a row only this process may read, so the sidecar
//! asks here instead of verifying the token itself — which would also have been
//! a second implementation of `jwt.rs`.
//!
//! The route performs no domain action. It returns identifiers, and none of
//! them is a credential: acting as the person still takes the sidecar's own
//! credential plus a context the membership check re-runs on every request.

use actix_web::{get, web, HttpMessage, HttpRequest, HttpResponse};
use design_core::{AccountId, AccountRole, AuthUserId};
use persistence::AccountRepository;
use serde::Serialize;
use ts_rs::TS;

use crate::auth::SubjectToResolve;
use crate::error::ApiError;
use crate::state::AppState;

/// Who a session token belongs to and the account they may act in.
#[derive(Serialize, TS)]
#[ts(export)]
pub struct SubjectResolution {
    pub subject: AuthUserId,
    /// What the sidecar binds as the session's `ApplicationContextId`.
    pub account_id: AccountId,
    pub role: AccountRole,
}

#[get("/service/resolve")]
async fn resolve(
    request: HttpRequest,
    state: web::Data<AppState>,
) -> Result<HttpResponse, ApiError> {
    // Set only by the service arm, and only on this path. Any other credential
    // class reaching here — a PAT, the deployment token, a browser session, or
    // local mode with nothing at all — arrives without it.
    let Some(subject) = request.extensions().get::<SubjectToResolve>().cloned() else {
        return Err(ApiError::unauthorized(
            "UNAUTHORIZED",
            "this endpoint takes a service credential",
            "Only the Pontis sidecar resolves sessions; present its forma_svc_ credential.",
        ));
    };
    let subject = subject.identity.subject;

    match state.accounts.membership_of(subject).await {
        Ok(Some(membership)) => Ok(HttpResponse::Ok().json(SubjectResolution {
            subject,
            account_id: membership.account_id,
            role: membership.role,
        })),
        // The same answer the session arm gives this identity: a valid person
        // with no account. Nothing is invented for them here.
        Ok(None) => Err(ApiError::forbidden(
            "ACCOUNT_REQUIRED",
            "this identity has no Forma account yet",
            "Create an account with an invite, or claim the existing workspace.",
        )),
        Err(_) => Err(ApiError::internal(
            "AUTH_STORAGE_ERROR",
            "the account for this identity could not be read",
        )),
    }
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(resolve);
}
