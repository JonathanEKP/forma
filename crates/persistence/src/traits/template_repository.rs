use async_trait::async_trait;
use design_core::{ProjectId, Template, TemplateId};

use crate::error::Result;

/// Plan section 85. A template is stored as the document it was saved from plus
/// the roles read out of it, and neither moves again: the design it came from
/// goes on to new versions and the template does not follow.
#[async_trait]
pub trait TemplateRepository: Send + Sync {
    async fn create(&self, template: &Template) -> Result<Template>;
    async fn get(&self, id: TemplateId) -> Result<Template>;
    async fn list_for_project(&self, project_id: ProjectId) -> Result<Vec<Template>>;
    async fn delete(&self, id: TemplateId) -> Result<()>;
}
