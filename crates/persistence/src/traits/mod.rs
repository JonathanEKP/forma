//! Repository interfaces (plan section 8.6).
//!
//! These exist because they isolate a real infrastructure boundary: the deploy
//! phase swaps SQLite for PostgreSQL and the local filesystem for an object
//! store, without the domain or the application layer noticing.

pub mod account_repository;
pub mod asset_repository;
pub mod asset_request_repository;
pub mod design_repository;
pub mod export_repository;
pub mod object_store;
pub mod project_repository;
pub mod template_repository;

pub use account_repository::AccountRepository;
pub use asset_repository::AssetRepository;
pub use asset_request_repository::AssetRequestRepository;
pub use design_repository::{DesignRepository, VersionSummary};
pub use export_repository::ExportRepository;
pub use object_store::ObjectStore;
pub use project_repository::ProjectRepository;
pub use template_repository::TemplateRepository;
