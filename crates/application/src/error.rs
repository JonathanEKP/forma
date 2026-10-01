//! One error type for the use cases, carrying the stable codes of section 32.
//!
//! Every layer below has its own error enum, and each already knows its own
//! code and hint. This type exists so a transport does not have to match on
//! three unrelated enums to build one response envelope: it collapses them
//! while preserving what each one already decided.

use design_core::DomainError;
use persistence::PersistenceError;
use renderer::RenderError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error(transparent)]
    Domain(#[from] DomainError),

    #[error(transparent)]
    Persistence(#[from] PersistenceError),

    #[error(transparent)]
    Render(#[from] RenderError),

    /// The request was well-formed but asked for something that cannot be done,
    /// and no lower layer has a code for it.
    #[error("{message}")]
    Rejected {
        code: &'static str,
        message: String,
        hint: &'static str,
    },

    /// The caller abandoned the operation and it stopped before finishing.
    /// Nothing it would have returned exists, so there is nothing to retry
    /// against; a caller that still wants the result asks again.
    #[error("the operation was cancelled before it finished")]
    Cancelled,
}

impl ApplicationError {
    pub fn rejected(code: &'static str, message: impl Into<String>, hint: &'static str) -> Self {
        Self::Rejected {
            code,
            message: message.into(),
            hint,
        }
    }

    /// The stable code clients recover against (plan sections 32 and 51.9).
    pub fn code(&self) -> &str {
        match self {
            Self::Domain(e) => e.code(),
            Self::Persistence(e) => e.code(),
            Self::Render(e) => e.code(),
            Self::Rejected { code, .. } => code,
            Self::Cancelled => "CANCELLED",
        }
    }

    pub fn hint(&self) -> &str {
        match self {
            Self::Domain(e) => e.hint(),
            Self::Persistence(e) => e.hint(),
            Self::Render(e) => e.hint(),
            Self::Rejected { hint, .. } => hint,
            Self::Cancelled => "Nothing was kept. Ask again if the result is still wanted.",
        }
    }

    /// Machine-readable specifics, currently the two versions of a conflict.
    pub fn details(&self) -> Option<serde_json::Value> {
        match self {
            Self::Persistence(e) => e.details(),
            _ => None,
        }
    }

    /// True when retrying the identical request can never succeed, so a client
    /// must change something before trying again.
    pub fn is_conflict(&self) -> bool {
        matches!(
            self,
            Self::Persistence(PersistenceError::VersionConflict { .. })
        )
    }

    pub fn is_not_found(&self) -> bool {
        matches!(
            self,
            Self::Persistence(
                PersistenceError::DesignNotFound(_)
                    | PersistenceError::ProjectNotFound(_)
                    | PersistenceError::NotFound(_)
            )
        )
    }
}

pub type Result<T> = std::result::Result<T, ApplicationError>;
