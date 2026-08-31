use async_trait::async_trait;
use design_core::{
    AccountId, Actor, DesignDocument, DesignId, DesignRecord, DesignSummary, ProjectId, Version,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::Result;

/// One entry of a design's history (plan section 61).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct VersionSummary {
    pub version: Version,
    pub actor_type: design_core::ActorType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor_id: Option<String>,
    pub created_at: String,
}

/// Designs, scoped through the project that owns them (plan section 108.1).
///
/// A design carries no `account_id` of its own: it is reached through its
/// project, which is where ownership is recorded once.
#[async_trait]
pub trait DesignRepository: Send + Sync {
    async fn create(
        &self,
        account: AccountId,
        project_id: ProjectId,
        name: &str,
        document: &DesignDocument,
        actor: &Actor,
    ) -> Result<DesignRecord>;

    async fn get(&self, account: AccountId, id: DesignId) -> Result<DesignRecord>;

    async fn list_for_project(
        &self,
        account: AccountId,
        project_id: ProjectId,
    ) -> Result<Vec<DesignSummary>>;

    /// Persists a new version, guarded by `base_version`.
    ///
    /// Returns `VERSION_CONFLICT` when the stored version has moved on, which is
    /// the single mechanism keeping a human editor and an agent from silently
    /// overwriting each other (plan section 27).
    ///
    /// A design in another account answers `DESIGN_NOT_FOUND`, never
    /// `VERSION_CONFLICT`: a conflict reports the version that is actually
    /// stored, which would confirm the design exists and leak how often someone
    /// else edits it.
    async fn save(
        &self,
        account: AccountId,
        id: DesignId,
        base_version: Version,
        document: &DesignDocument,
        actor: &Actor,
    ) -> Result<DesignRecord>;

    async fn history(&self, account: AccountId, id: DesignId) -> Result<Vec<VersionSummary>>;

    /// The document as it was at one version. Versions never change once
    /// written, which is what makes undo and the render cache safe.
    async fn snapshot(
        &self,
        account: AccountId,
        id: DesignId,
        version: Version,
    ) -> Result<DesignDocument>;

    async fn delete(&self, account: AccountId, id: DesignId) -> Result<()>;

    /// Every design that still exists, for cleanup to compare against.
    ///
    /// Derived data keyed by design id — the render cache — has no other way to
    /// learn that a design was deleted, because a project delete cascades in
    /// SQL and never touches the filesystem.
    ///
    /// **Deliberately not account-scoped**, and the only read here that is not.
    /// It answers a question about the filesystem rather than about anyone's
    /// workspace: the sweep must see every design in the database, or it would
    /// delete the cached renders of every account but one. It returns ids and
    /// nothing else, and no request path reaches it.
    async fn all_ids(&self) -> Result<Vec<DesignId>>;
}
