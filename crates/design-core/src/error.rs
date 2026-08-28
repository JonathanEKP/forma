use thiserror::Error;

#[derive(Debug, Error)]
pub enum DomainError {
    #[error("INVALID_COLOR: {0}")]
    InvalidColor(String),

    #[error("INVALID_DIMENSION: {0}")]
    InvalidDimension(String),

    #[error("UNSUPPORTED_SCHEMA_VERSION: {0}")]
    UnsupportedSchemaVersion(u32),
}
