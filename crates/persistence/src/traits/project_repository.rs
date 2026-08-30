use async_trait::async_trait;
use design_core::{AccountId, BrandKit, Project, ProjectId};

use crate::error::Result;

/// Projects, scoped to the account that owns them (plan section 108.1).
///
/// **Every method takes an `AccountId`, and none takes an `Option<AccountId>`.**
/// Auditing call sites is not a mechanism; a method that omits the scope must
/// fail to compile. This is the same device as repositories stamping their own
/// `created_at`: the type carries the invariant so no caller can be careless,
/// and it is what puts the boundary below the HTTP handler rather than in it.
///
/// A read for an account that does not own the row returns `ProjectNotFound`,
/// never a distinct "forbidden" — an id in another account must be
/// indistinguishable from an id that was never issued (plan 108.2).
#[async_trait]
pub trait ProjectRepository: Send + Sync {
    async fn create(&self, account: AccountId, name: &str) -> Result<Project>;
    async fn get(&self, account: AccountId, id: ProjectId) -> Result<Project>;
    async fn list(&self, account: AccountId) -> Result<Vec<Project>>;
    async fn rename(&self, account: AccountId, id: ProjectId, name: &str) -> Result<Project>;
    async fn delete(&self, account: AccountId, id: ProjectId) -> Result<()>;

    /// V1 keeps one active Brand Kit per project (plan section 12).
    async fn get_brand(&self, account: AccountId, id: ProjectId) -> Result<Option<BrandKit>>;
    async fn put_brand(&self, account: AccountId, brand: &BrandKit) -> Result<BrandKit>;
}
