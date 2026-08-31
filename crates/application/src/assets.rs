//! Asset use cases (plan sections 21, 44 and 73).

use assets::{AssetError, Ingested};
use design_core::{AccountId, Asset, AssetId, AssetOrigin, ProjectId};
use persistence::{AssetRepository, ObjectStore, ProjectRepository};

use crate::error::{ApplicationError, Result};

impl From<AssetError> for ApplicationError {
    fn from(error: AssetError) -> Self {
        // The ingest crate already decided both, so nothing is re-judged here.
        ApplicationError::rejected(error.code(), error.to_string(), error.hint())
    }
}

/// Stores an uploaded file and records it.
///
/// Deduplication comes first (plan section 73): identical bytes inside one
/// project reuse the existing row rather than writing a second copy. That check
/// happens after ingest rather than before, because the hash is taken over the
/// canonical bytes — two SVG exports that differ only in a comment normalize to
/// the same picture and should collapse into one asset.
#[allow(clippy::too_many_arguments)]
pub async fn upload(
    projects: &dyn ProjectRepository,
    assets: &dyn AssetRepository,
    store: &dyn ObjectStore,
    account: AccountId,
    project_id: ProjectId,
    bytes: Vec<u8>,
    original_filename: Option<String>,
    max_bytes: usize,
) -> Result<(Asset, bool)> {
    // Fail before touching the bytes if the project is not real — or not this
    // account's, which must be indistinguishable from not real.
    projects.get(account, project_id).await?;

    let ingested: Ingested = assets::ingest(bytes, max_bytes)?;

    if let Some(existing) = assets
        .find_by_hash(account, project_id, &ingested.hash)
        .await?
    {
        return Ok((existing, false));
    }

    // Bytes first: a row pointing at an object that is not there would break
    // every later render, while an object with no row is merely garbage the
    // cleanup of section 74 will collect.
    store.put(&ingested.storage_key, &ingested.bytes).await?;

    let asset = Asset {
        id: AssetId::new(),
        project_id,
        hash: ingested.hash,
        kind: ingested.format.kind(),
        mime_type: ingested.format.mime().to_string(),
        width: ingested.width,
        height: ingested.height,
        byte_size: ingested.bytes.len() as i64,
        storage_key: ingested.storage_key,
        original_filename,
        // V1 only produces uploads; the other origins exist so the schema can
        // grow without V1 depending on those features (plan section 21).
        origin: AssetOrigin::UserUpload,
        attribution: None,
        created_at: String::new(),
    };

    Ok((assets.create(account, &asset).await?, true))
}

pub async fn list(
    assets: &dyn AssetRepository,
    account: AccountId,
    project_id: ProjectId,
) -> Result<Vec<Asset>> {
    Ok(assets.list_for_project(account, project_id).await?)
}

pub async fn get(assets: &dyn AssetRepository, account: AccountId, id: AssetId) -> Result<Asset> {
    Ok(assets.get(account, id).await?)
}

/// Reads the stored bytes back, for serving the file itself.
pub async fn bytes(
    assets: &dyn AssetRepository,
    store: &dyn ObjectStore,
    account: AccountId,
    id: AssetId,
) -> Result<(Asset, Vec<u8>)> {
    let asset = assets.get(account, id).await?;
    let bytes = store.get(&asset.storage_key).await?;
    Ok((asset, bytes))
}

/// A soft delete. Historical design versions may still reference the asset, so
/// the bytes stay put and real collection is deferred (plan section 74).
pub async fn delete(assets: &dyn AssetRepository, account: AccountId, id: AssetId) -> Result<()> {
    Ok(assets.soft_delete(account, id).await?)
}
