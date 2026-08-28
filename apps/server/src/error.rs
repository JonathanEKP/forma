//! Plan section 32: every error has the same shape, never an ad-hoc string.
//!
//! ```json
//! { "error": { "code": "VERSION_CONFLICT", "message": "...", "details": {...} } }
//! ```
//!
//! The code, the message and the hint all come from the error that was raised.
//! This module decides one thing the layers below cannot: which HTTP status
//! carries it. That mapping matters, because a client distinguishes "retry
//! after rebasing" from "give up" by status long before it parses a body.

use actix_web::{http::StatusCode, HttpResponse, ResponseError};
use application::ApplicationError;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ApiErrorBody {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    /// What the caller should do next. An agent recovering from an error needs
    /// this as much as it needs the code (plan section 51.9).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ApiErrorEnvelope {
    pub error: ApiErrorBody,
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error(transparent)]
    Application(#[from] ApplicationError),

    /// Raised by the transport itself, before any use case runs: a malformed
    /// path id, an unreadable body, an upload over the limit.
    #[error("{message}")]
    Request {
        code: &'static str,
        message: String,
        hint: &'static str,
        status: StatusCode,
    },
}

impl ApiError {
    pub fn bad_request(code: &'static str, message: impl Into<String>, hint: &'static str) -> Self {
        Self::Request {
            code,
            message: message.into(),
            hint,
            status: StatusCode::BAD_REQUEST,
        }
    }

    pub fn payload_too_large(message: impl Into<String>, hint: &'static str) -> Self {
        Self::Request {
            code: "PAYLOAD_TOO_LARGE",
            message: message.into(),
            hint,
            status: StatusCode::PAYLOAD_TOO_LARGE,
        }
    }

    pub fn internal(code: &'static str, message: impl Into<String>) -> Self {
        Self::Request {
            code,
            message: message.into(),
            hint: "This is a server-side failure. Retrying may work; the request was not applied.",
            status: StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code(&self) -> &str {
        match self {
            Self::Application(e) => e.code(),
            Self::Request { code, .. } => code,
        }
    }

    fn hint(&self) -> &str {
        match self {
            Self::Application(e) => e.hint(),
            Self::Request { hint, .. } => hint,
        }
    }

    fn details(&self) -> Option<serde_json::Value> {
        match self {
            Self::Application(e) => e.details(),
            Self::Request { .. } => None,
        }
    }
}

/// Maps a stable code to the status that carries it.
///
/// Kept as one table rather than scattered across handlers so the contract is
/// readable in a single place, and so a new code cannot quietly default to 500
/// in one route and 400 in another.
fn status_for(code: &str) -> StatusCode {
    match code {
        // The design moved under the caller. Refetch, rebase, resend.
        "VERSION_CONFLICT" => StatusCode::CONFLICT,

        // The record exists but is past the state this asks for: an asset
        // request already resolved or cancelled. Unlike a version conflict
        // there is no rebase that makes it succeed, so the hint says so.
        "INVALID_STATE" => StatusCode::CONFLICT,

        "PROJECT_NOT_FOUND" | "DESIGN_NOT_FOUND" | "PAGE_NOT_FOUND" | "LAYER_NOT_FOUND"
        | "ASSET_NOT_FOUND" | "NOT_FOUND" => StatusCode::NOT_FOUND,

        // The request was understood and refused. The caller must change it.
        "INVALID_OP"
        | "INVALID_COLOR"
        | "INVALID_DIMENSION"
        | "INVALID_OPACITY"
        | "INVALID_DOCUMENT"
        | "INVALID_ASSET"
        | "DUPLICATE_LAYER_ID"
        | "UNKNOWN_BRAND_TOKEN"
        | "UNSUPPORTED_SCHEMA_VERSION"
        | "NOTHING_TO_UNDO"
        | "UNSUPPORTED_MEDIA_TYPE"
        | "MALFORMED_ID"
        | "MALFORMED_BODY" => StatusCode::BAD_REQUEST,

        "LIMIT_EXCEEDED" => StatusCode::UNPROCESSABLE_ENTITY,
        "PAYLOAD_TOO_LARGE" => StatusCode::PAYLOAD_TOO_LARGE,

        // Fonts, rasterization and storage failures are ours, not the caller's.
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

impl ResponseError for ApiError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::Application(_) => status_for(self.code()),
            Self::Request { status, .. } => *status,
        }
    }

    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code()).json(ApiErrorEnvelope {
            error: ApiErrorBody {
                code: self.code().to_string(),
                message: self.to_string(),
                details: self.details(),
                hint: Some(self.hint().to_string()),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use design_core::{DesignId, Version};
    use persistence::PersistenceError;

    fn status_of(error: PersistenceError) -> StatusCode {
        ApiError::Application(ApplicationError::Persistence(error)).status_code()
    }

    #[test]
    fn a_conflict_and_a_missing_design_are_never_the_same_status() {
        // These two are the pair the guarded UPDATE works hard to tell apart,
        // so collapsing them here would waste that (plan section 27).
        assert_eq!(
            status_of(PersistenceError::VersionConflict {
                expected: Version(4),
                actual: Version(5),
            }),
            StatusCode::CONFLICT
        );
        assert_eq!(
            status_of(PersistenceError::DesignNotFound(DesignId::new())),
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn a_conflict_reports_both_versions_so_a_client_can_rebase() {
        let error = ApiError::Application(ApplicationError::Persistence(
            PersistenceError::VersionConflict {
                expected: Version(4),
                actual: Version(7),
            },
        ));
        let details = error.details().expect("a conflict must carry details");
        assert_eq!(details["expected"], 4);
        assert_eq!(details["actual"], 7);
    }

    #[test]
    fn a_rejected_batch_is_the_callers_fault_not_a_server_error() {
        let error = ApiError::Application(ApplicationError::Domain(
            design_core::DomainError::invalid_op("bad", "fix it"),
        ));
        assert_eq!(error.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(error.code(), "INVALID_OP");
    }

    #[test]
    fn an_unrecognised_code_fails_closed_as_a_server_error() {
        // Better to over-report a server fault than to tell a client its own
        // request was wrong when we do not know that.
        assert_eq!(
            status_for("SOMETHING_NEW"),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn every_error_carries_a_hint() {
        let error = ApiError::Application(ApplicationError::Persistence(
            PersistenceError::VersionConflict {
                expected: Version(1),
                actual: Version(2),
            },
        ));
        assert!(!error.hint().is_empty());
    }
}
