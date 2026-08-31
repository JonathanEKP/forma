use async_trait::async_trait;
use design_core::{AccountId, ProjectId, Template, TemplateId};

use crate::error::Result;

/// Plan section 85. A template is stored as the document it was saved from plus
/// the roles read out of it, and neither moves again: the design it came from
/// goes on to new versions and the template does not follow.
/// Scoped through the project that owns the row (plan section 108.1).
///
/// The `AccountId` is required rather than optional: a method that forgets the
/// scope must fail to compile. A row in another account answers the same
/// not-found as an id that was never issued (plan 108.2).
#[async_trait]
pub trait TemplateRepository: Send + Sync {
    async fn create(&self, account: AccountId, template: &Template) -> Result<Template>;
    async fn get(&self, account: AccountId, id: TemplateId) -> Result<Template>;
    async fn list_for_project(&self, account: AccountId, project_id: ProjectId) -> Result<Vec<Template>>;
    async fn delete(&self, account: AccountId, id: TemplateId) -> Result<()>;
}
