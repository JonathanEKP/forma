//! Project and Brand Kit use cases.

use design_core::{BrandKit, Project, ProjectId};
use persistence::ProjectRepository;

use crate::error::{ApplicationError, Result};

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
pub async fn put_brand(projects: &dyn ProjectRepository, brand: &BrandKit) -> Result<BrandKit> {
    // The kit carries its own project_id, so a mismatched path would otherwise
    // write the kit onto a project the caller did not name.
    projects.get(brand.project_id).await?;
    Ok(projects.put_brand(brand).await?)
}
