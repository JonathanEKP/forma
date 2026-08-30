//! Asset requests (plan sections 22 and 23).
//!
//! What keeps an agent from stalling on a missing image: it records what it
//! needs and carries on, and the user resolves the request by uploading. Forma
//! never generates the image — that is out of scope for V1 — so the suggested
//! prompt is text for a human to take elsewhere.

use design_core::{AccountId, AssetId, AssetRequest, AssetRequestId, AssetRequestStatus, DesignId, ProjectId};
use persistence::{AssetRepository, AssetRequestRepository, DesignRepository, ProjectRepository};

use crate::error::{ApplicationError, Result};

/// What a caller must supply to open a request.
#[derive(Debug, Clone)]
pub struct NewAssetRequest {
    pub project_id: ProjectId,
    pub design_id: Option<DesignId>,
    pub purpose: String,
    pub description: String,
    pub suggested_prompt: Option<String>,
    pub requested_width: Option<u32>,
    pub requested_height: Option<u32>,
}

pub async fn create(
    projects: &dyn ProjectRepository,
    designs: &dyn DesignRepository,
    requests: &dyn AssetRequestRepository,
    account: AccountId,
    new: NewAssetRequest,
) -> Result<AssetRequest> {
    projects.get(account, new.project_id).await?;

    // A request pinned to a design that does not exist would show up in no
    // editor and be resolvable by nobody.
    if let Some(design_id) = new.design_id {
        designs.get(design_id).await?;
    }

    if new.description.trim().is_empty() {
        return Err(ApplicationError::rejected(
            "INVALID_OP",
            "an asset request needs a description",
            "Say what the image should show; a human has to act on this text.",
        ));
    }

    let request = AssetRequest {
        id: AssetRequestId::new(),
        project_id: new.project_id,
        design_id: new.design_id,
        purpose: new.purpose,
        description: new.description,
        suggested_prompt: new.suggested_prompt,
        requested_width: new.requested_width,
        requested_height: new.requested_height,
        status: AssetRequestStatus::Pending,
        resolved_asset_id: None,
        created_at: String::new(),
        resolved_at: None,
    };

    Ok(requests.create(&request).await?)
}

pub async fn get(
    requests: &dyn AssetRequestRepository,
    id: AssetRequestId,
) -> Result<AssetRequest> {
    Ok(requests.get(id).await?)
}

pub async fn list(
    requests: &dyn AssetRequestRepository,
    project_id: ProjectId,
) -> Result<Vec<AssetRequest>> {
    Ok(requests.list_for_project(project_id).await?)
}

/// Marks a request satisfied by an asset the user uploaded.
///
/// The asset has to belong to the same project: resolving with a picture from
/// somewhere else would put it into a design whose project never owned it.
pub async fn resolve(
    requests: &dyn AssetRequestRepository,
    assets: &dyn AssetRepository,
    id: AssetRequestId,
    asset_id: AssetId,
) -> Result<AssetRequest> {
    let request = requests.get(id).await?;
    let asset = assets.get(asset_id).await?;

    if asset.project_id != request.project_id {
        return Err(ApplicationError::rejected(
            "INVALID_OP",
            format!(
                "asset {asset_id} belongs to project {} but the request belongs to {}",
                asset.project_id, request.project_id
            ),
            "Upload the image into the request's own project and resolve with that asset.",
        ));
    }

    // The repository's guarded UPDATE is what actually prevents a double
    // resolve; this only makes the common case a clearer error.
    Ok(requests.resolve(id, asset_id).await?)
}

pub async fn cancel(
    requests: &dyn AssetRequestRepository,
    id: AssetRequestId,
) -> Result<AssetRequest> {
    Ok(requests.cancel(id).await?)
}
