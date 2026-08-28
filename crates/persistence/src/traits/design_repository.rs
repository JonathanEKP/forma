use async_trait::async_trait;
use design_core::{
    Actor, DesignDocument, DesignId, DesignRecord, DesignSummary, ProjectId, Version,
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

#[async_trait]
pub trait DesignRepository: Send + Sync {
    async fn create(
        &self,
        project_id: ProjectId,
        name: &str,
        document: &DesignDocument,
        actor: &Actor,
    ) -> Result<DesignRecord>;

    async fn get(&self, id: DesignId) -> Result<DesignRecord>;

    async fn list_for_project(&self, project_id: ProjectId) -> Result<Vec<DesignSummary>>;

    /// Persists a new version, guarded by `base_version`.
    ///
    /// Returns `VERSION_CONFLICT` when the stored version has moved on, which is
    /// the single mechanism keeping a human editor and an agent from silently
    /// overwriting each other (plan section 27).
    async fn save(
        &self,
        id: DesignId,
        base_version: Version,
        document: &DesignDocument,
        actor: &Actor,
    ) -> Result<DesignRecord>;

    async fn history(&self, id: DesignId) -> Result<Vec<VersionSummary>>;

    /// The document as it was at one version. Versions never change once
    /// written, which is what makes undo and the render cache safe.
    async fn snapshot(&self, id: DesignId, version: Version) -> Result<DesignDocument>;

    async fn delete(&self, id: DesignId) -> Result<()>;
}
