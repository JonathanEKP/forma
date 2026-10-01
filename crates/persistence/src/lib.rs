//! Persistence for Forma (plan section 8.6).
//!
//! Interfaces plus a SQLite and local-filesystem implementation. The traits are
//! not speculative: the deploy phase swaps in PostgreSQL and an object store
//! without the domain or the application layer changing.

pub mod error;
pub mod local_fs;
pub mod sqlite;
pub mod traits;

pub use error::{PersistenceError, Result};
pub use local_fs::{asset_storage_key, export_storage_key, LocalObjectStore};
pub use sqlite::{
    checkpoint_and_close, connect, SqliteAccountRepository, SqliteApiKeyRepository,
    SqliteAssetRepository, SqliteAssetRequestRepository, SqliteDesignRepository,
    SqliteExportRepository, SqliteProjectRepository, SqliteServiceCredentialRepository,
    SqliteTemplateRepository, PAT_PREFIX, SERVICE_PREFIX,
};
pub use traits::{
    AccountRepository, ApiKeyRepository, AssetRepository, AssetRequestRepository, DesignRepository,
    ExportRepository, ObjectStore, ProjectRepository, ServiceCredentialRepository,
    TemplateRepository, VersionSummary,
};
