use async_trait::async_trait;
use design_core::{AccountId, DesignId, Export, ExportId};

use crate::error::Result;

/// Scoped through the design, and then its project (plan section 108.1).
///
/// An export hangs off a design rather than a project, so its account is two
/// joins away. It is still one place: ownership is only ever recorded on
/// `projects`.
#[async_trait]
pub trait ExportRepository: Send + Sync {
    async fn create(&self, account: AccountId, export: &Export) -> Result<Export>;
    async fn get(&self, account: AccountId, id: ExportId) -> Result<Export>;
    async fn list_for_design(&self, account: AccountId, design_id: DesignId) -> Result<Vec<Export>>;
}
