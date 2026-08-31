use async_trait::async_trait;
use design_core::{AccountId, AssetId, AssetRequest, AssetRequestId, ProjectId};

use crate::error::Result;

/// Scoped through the project that owns the row (plan section 108.1).
///
/// The `AccountId` is required rather than optional: a method that forgets the
/// scope must fail to compile. A row in another account answers the same
/// not-found as an id that was never issued (plan 108.2).
#[async_trait]
pub trait AssetRequestRepository: Send + Sync {
    async fn create(&self, account: AccountId, request: &AssetRequest) -> Result<AssetRequest>;
    async fn get(&self, account: AccountId, id: AssetRequestId) -> Result<AssetRequest>;
    async fn list_for_project(&self, account: AccountId, project_id: ProjectId) -> Result<Vec<AssetRequest>>;

    /// The user uploaded the missing image (plan section 23).
    async fn resolve(&self, account: AccountId, id: AssetRequestId, asset_id: AssetId) -> Result<AssetRequest>;
    async fn cancel(&self, account: AccountId, id: AssetRequestId) -> Result<AssetRequest>;
}
