use async_trait::async_trait;
use design_core::{AssetId, AssetRequest, AssetRequestId, ProjectId};

use crate::error::Result;

#[async_trait]
pub trait AssetRequestRepository: Send + Sync {
    async fn create(&self, request: &AssetRequest) -> Result<AssetRequest>;
    async fn get(&self, id: AssetRequestId) -> Result<AssetRequest>;
    async fn list_for_project(&self, project_id: ProjectId) -> Result<Vec<AssetRequest>>;

    /// The user uploaded the missing image (plan section 23).
    async fn resolve(&self, id: AssetRequestId, asset_id: AssetId) -> Result<AssetRequest>;
    async fn cancel(&self, id: AssetRequestId) -> Result<AssetRequest>;
}
