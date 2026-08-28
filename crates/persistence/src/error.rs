//! Persistence errors, carrying the stable codes of plan section 51.9.

use design_core::{DesignId, ProjectId, Version};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PersistenceError {
    /// Plan section 27. The caller must refetch, rebase and resend; blindly
    /// retrying the same batch will fail forever.
    #[error("design version changed: expected {expected}, found {actual}")]
    VersionConflict { expected: Version, actual: Version },

    #[error("design {0} does not exist")]
    DesignNotFound(DesignId),

    #[error("project {0} does not exist")]
    ProjectNotFound(ProjectId),

    #[error("{0} does not exist")]
    NotFound(String),

    #[error("stored document is not readable: {0}")]
    CorruptDocument(String),

    #[error("storage failed: {0}")]
    Storage(String),

    /// A record exists but is not in a state that allows what was asked.
    ///
    /// Distinct from `Storage`, which is a server fault: resolving an already
    /// resolved request is the caller's mistake and must not be reported as a
    /// 500 that invites a pointless retry.
    #[error("{0}")]
    InvalidTransition(String),
}

impl PersistenceError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::VersionConflict { .. } => "VERSION_CONFLICT",
            Self::DesignNotFound(_) => "DESIGN_NOT_FOUND",
            Self::ProjectNotFound(_) => "PROJECT_NOT_FOUND",
            Self::NotFound(_) => "NOT_FOUND",
            Self::CorruptDocument(_) => "INVALID_DOCUMENT",
            Self::Storage(_) => "STORAGE_ERROR",
            Self::InvalidTransition(_) => "INVALID_STATE",
        }
    }

    pub fn hint(&self) -> &'static str {
        match self {
            Self::VersionConflict { .. } => {
                "Fetch the design again, rebase your operations onto the new version, and \
                 resend with the version you just read. Never retry the same base_version."
            }
            Self::DesignNotFound(_) => "List the project's designs and use an id it returned.",
            Self::ProjectNotFound(_) => "List projects and use an id it returned.",
            Self::NotFound(_) => "The record does not exist; list first and use a real id.",
            Self::CorruptDocument(_) => {
                "The stored document could not be parsed. Restore an earlier version."
            }
            Self::Storage(_) => "A storage operation failed. Retrying later may work.",
            Self::InvalidTransition(_) => {
                "Read the record back and check its status; retrying this will fail the same way."
            }
        }
    }

    /// Machine-readable specifics for the error envelope of section 32.
    pub fn details(&self) -> Option<serde_json::Value> {
        match self {
            Self::VersionConflict { expected, actual } => Some(serde_json::json!({
                "expected": expected.0,
                "actual": actual.0,
            })),
            _ => None,
        }
    }
}

impl From<sqlx::Error> for PersistenceError {
    fn from(error: sqlx::Error) -> Self {
        Self::Storage(error.to_string())
    }
}

pub type Result<T> = std::result::Result<T, PersistenceError>;
