use async_trait::async_trait;
use design_core::{AccountId, Asset, AssetId, ProjectId};

use crate::error::Result;

/// Scoped through the project that owns the row (plan section 108.1).
///
/// The `AccountId` is required rather than optional: a method that forgets the
/// scope must fail to compile. A row in another account answers the same
/// not-found as an id that was never issued (plan 108.2).
#[async_trait]
pub trait AssetRepository: Send + Sync {
    async fn create(&self, account: AccountId, asset: &Asset) -> Result<Asset>;
    async fn get(&self, account: AccountId, id: AssetId) -> Result<Asset>;
    async fn list_for_project(
        &self,
        account: AccountId,
        project_id: ProjectId,
    ) -> Result<Vec<Asset>>;

    /// Deduplication (plan section 73): identical bytes reuse one asset.
    async fn find_by_hash(
        &self,
        account: AccountId,
        project_id: ProjectId,
        hash: &str,
    ) -> Result<Option<Asset>>;

    /// A soft delete. Historical design versions may still reference the asset,
    /// so the bytes stay put and real GC is deferred (plan section 74).
    async fn soft_delete(&self, account: AccountId, id: AssetId) -> Result<()>;
}
