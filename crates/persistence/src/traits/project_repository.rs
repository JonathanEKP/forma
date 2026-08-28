use async_trait::async_trait;
use design_core::{BrandKit, Project, ProjectId};

use crate::error::Result;

#[async_trait]
pub trait ProjectRepository: Send + Sync {
    async fn create(&self, name: &str) -> Result<Project>;
    async fn get(&self, id: ProjectId) -> Result<Project>;
    async fn list(&self) -> Result<Vec<Project>>;
    async fn rename(&self, id: ProjectId, name: &str) -> Result<Project>;
    async fn delete(&self, id: ProjectId) -> Result<()>;

    /// V1 keeps one active Brand Kit per project (plan section 12).
    async fn get_brand(&self, id: ProjectId) -> Result<Option<BrandKit>>;
    async fn put_brand(&self, brand: &BrandKit) -> Result<BrandKit>;
}
