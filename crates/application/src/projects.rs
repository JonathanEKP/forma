//! Project and Brand Kit use cases.

use design_core::{
    AssetId, BrandColors, BrandId, BrandKit, BrandRules, BrandTypography, Project, ProjectId,
    Version,
};
use persistence::ProjectRepository;
use serde::Deserialize;
use ts_rs::TS;

use crate::error::{ApplicationError, Result};

/// The editable half of a Brand Kit.
///
/// Identity and version are the server's to decide, so a client neither mints a
/// `BrandId` nor picks a version number: it says what the brand *is*, and gets
/// back the stored kit.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct BrandKitDraft {
    pub name: String,
    pub colors: BrandColors,
    pub typography: BrandTypography,
    #[serde(default)]
    pub logo_asset_id: Option<AssetId>,
    #[serde(default)]
    pub voice: String,
    #[serde(default)]
    pub rules: BrandRules,
}

fn check_name(name: &str) -> Result<()> {
    if name.trim().is_empty() {
        return Err(ApplicationError::rejected(
            "INVALID_OP",
            "a project name must not be empty",
            "Give the project a name.",
        ));
    }
    Ok(())
}

pub async fn create(projects: &dyn ProjectRepository, name: &str) -> Result<Project> {
    check_name(name)?;
    Ok(projects.create(name).await?)
}

pub async fn get(projects: &dyn ProjectRepository, id: ProjectId) -> Result<Project> {
    Ok(projects.get(id).await?)
}

pub async fn list(projects: &dyn ProjectRepository) -> Result<Vec<Project>> {
    Ok(projects.list().await?)
}

pub async fn rename(
    projects: &dyn ProjectRepository,
    id: ProjectId,
    name: &str,
) -> Result<Project> {
    check_name(name)?;
    Ok(projects.rename(id, name).await?)
}

pub async fn delete(projects: &dyn ProjectRepository, id: ProjectId) -> Result<()> {
    Ok(projects.delete(id).await?)
}

pub async fn get_brand(
    projects: &dyn ProjectRepository,
    id: ProjectId,
) -> Result<Option<BrandKit>> {
    Ok(projects.get_brand(id).await?)
}

/// Stores the project's Brand Kit (plan section 12: one active kit per project).
///
/// The kit's identity survives an edit and its version is bumped here rather
/// than trusted from the request: a client that reused an id, or sent a version
/// that had already been used, would otherwise make the history meaningless.
pub async fn put_brand(
    projects: &dyn ProjectRepository,
    project_id: ProjectId,
    draft: BrandKitDraft,
) -> Result<BrandKit> {
    // Fails with PROJECT_NOT_FOUND rather than writing an orphan kit.
    projects.get(project_id).await?;

    let existing = projects.get_brand(project_id).await?;
    let (id, version) = match &existing {
        // Keeping the id stable means the project's brand_id does not churn on
        // every save, and anything pointing at the kit keeps pointing at it.
        Some(current) => (current.id, current.version.next()),
        None => (BrandId::new(), Version::INITIAL),
    };

    let brand = BrandKit {
        id,
        project_id,
        name: draft.name,
        colors: draft.colors,
        typography: draft.typography,
        logo_asset_id: draft.logo_asset_id,
        voice: draft.voice,
        rules: draft.rules,
        version,
    };

    Ok(projects.put_brand(&brand).await?)
}
