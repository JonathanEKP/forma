//! Domain errors.
//!
//! Every variant maps to one of the stable codes of plan sections 42 and 51.9,
//! and carries a hint, because an agent recovering from an error needs to know
//! what to do next, not just what went wrong.

use thiserror::Error;

use crate::ids::{LayerId, PageId};

#[derive(Debug, Error)]
pub enum DomainError {
    #[error("{0}")]
    InvalidColor(String),

    #[error("{0}")]
    InvalidDimension(String),

    #[error("{0}")]
    InvalidOpacity(String),

    #[error("document schema version {0} is not supported by this build")]
    UnsupportedSchemaVersion(u32),

    #[error("page {0} does not exist in this design")]
    PageNotFound(PageId),

    #[error("layer {0} does not exist in this design")]
    LayerNotFound(LayerId),

    #[error("layer id {0} appears more than once")]
    DuplicateLayerId(LayerId),

    #[error("{message}")]
    InvalidOp { message: String, hint: &'static str },

    #[error("{0}")]
    LimitExceeded(String),

    #[error("{0}")]
    InvalidDocument(String),

    #[error("unknown brand token ${0}")]
    UnknownBrandToken(String),

    /// Names the operation a batch failed on, wrapping the error it failed with.
    ///
    /// A rejected batch of twenty is only actionable if the caller knows which
    /// one broke. The code and hint stay those of the wrapped error, because
    /// that is what a client branches on; only the message gains the position.
    #[error("operation {index} ({op}): {source}")]
    InOperation {
        index: usize,
        op: &'static str,
        source: Box<DomainError>,
    },
}

impl DomainError {
    pub fn invalid_op(message: impl Into<String>, hint: &'static str) -> Self {
        Self::InvalidOp {
            message: message.into(),
            hint,
        }
    }

    /// The stable code the REST and MCP layers surface to clients.
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidColor(_) => "INVALID_COLOR",
            Self::InvalidDimension(_) => "INVALID_DIMENSION",
            Self::InvalidOpacity(_) => "INVALID_OPACITY",
            Self::UnsupportedSchemaVersion(_) => "UNSUPPORTED_SCHEMA_VERSION",
            Self::PageNotFound(_) => "PAGE_NOT_FOUND",
            Self::LayerNotFound(_) => "LAYER_NOT_FOUND",
            Self::DuplicateLayerId(_) => "DUPLICATE_LAYER_ID",
            Self::InvalidOp { .. } => "INVALID_OP",
            Self::LimitExceeded(_) => "LIMIT_EXCEEDED",
            Self::InvalidDocument(_) => "INVALID_DOCUMENT",
            Self::UnknownBrandToken(_) => "UNKNOWN_BRAND_TOKEN",
            // The wrapper reports the wrapped error, never a code of its own.
            Self::InOperation { source, .. } => source.code(),
        }
    }

    pub fn hint(&self) -> &'static str {
        match self {
            Self::InvalidColor(_) => "Use #RGB, #RRGGBB, #RRGGBBAA or a $brand.* token.",
            Self::InvalidDimension(_) => {
                "Widths, heights and font sizes must be finite and greater than zero."
            }
            Self::InvalidOpacity(_) => "Opacity is a fraction between 0 and 1.",
            Self::UnsupportedSchemaVersion(_) => {
                "This document was written by a newer Forma. Upgrade before opening it."
            }
            Self::PageNotFound(_) => "Call get_design and use a page_id it returned.",
            Self::LayerNotFound(_) => {
                "Call get_design and use a layer_id it returned; ids are the only valid handles."
            }
            Self::DuplicateLayerId(_) => {
                "Generate a fresh id for the new layer instead of reusing one."
            }
            Self::InvalidOp { hint, .. } => hint,
            Self::LimitExceeded(_) => "Split the work into several batches, each within the limit.",
            Self::InvalidDocument(_) => {
                "Fix the structural problem before persisting; the batch was rejected whole."
            }
            Self::UnknownBrandToken(_) => {
                "Call get_brand and use a token it lists, or pass an explicit color literal."
            }
            Self::InOperation { source, .. } => source.hint(),
        }
    }
}
