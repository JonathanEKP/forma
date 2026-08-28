use async_trait::async_trait;
use design_core::{DesignId, Export, ExportId};

use crate::error::Result;

#[async_trait]
pub trait ExportRepository: Send + Sync {
    async fn create(&self, export: &Export) -> Result<Export>;
    async fn get(&self, id: ExportId) -> Result<Export>;
    async fn list_for_design(&self, design_id: DesignId) -> Result<Vec<Export>>;
}
