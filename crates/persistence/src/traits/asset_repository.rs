use async_trait::async_trait;
use design_core::{Asset, AssetId, ProjectId};

use crate::error::Result;

#[async_trait]
pub trait AssetRepository: Send + Sync {
    async fn create(&self, asset: &Asset) -> Result<Asset>;
    async fn get(&self, id: AssetId) -> Result<Asset>;
    async fn list_for_project(&self, project_id: ProjectId) -> Result<Vec<Asset>>;

    /// Deduplication (plan section 73): identical bytes reuse one asset.
    async fn find_by_hash(&self, project_id: ProjectId, hash: &str) -> Result<Option<Asset>>;

    /// A soft delete. Historical design versions may still reference the asset,
    /// so the bytes stay put and real GC is deferred (plan section 74).
    async fn soft_delete(&self, id: AssetId) -> Result<()>;
}
