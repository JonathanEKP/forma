//! Design use cases (plan section 8.2).
//!
//! These orchestrate; they do not re-implement. `apply_operations` owns batch
//! semantics, the repository owns the guarded UPDATE, and the renderer owns
//! measurement. What lives here is the order those are called in, which is the
//! part every transport would otherwise duplicate.

use design_core::{
    apply_operations, AccountId, Actor, DesignDocument, DesignId, DesignOperation, DesignRecord,
    DesignSummary, ProjectId, Version,
};
use persistence::{DesignRepository, ExportRepository, ObjectStore, VersionSummary};
use renderer::RenderCache;

use crate::error::{ApplicationError, Result};

pub async fn create(
    designs: &dyn DesignRepository,
    account: AccountId,
    project_id: ProjectId,
    name: &str,
    document: &DesignDocument,
    actor: &Actor,
) -> Result<DesignRecord> {
    if name.trim().is_empty() {
        return Err(ApplicationError::rejected(
            "INVALID_OP",
            "a design name must not be empty",
            "Give the design a name; it is how a human finds it in the project.",
        ));
    }

    // Checked here, not left to the repository. The repository reports a bad
    // document as CorruptDocument, whose hint tells the caller to restore an
    // earlier version — true of a row read back from disk, nonsense for a
    // document the caller just sent. This surfaces the domain's own error.
    document.validate_structure()?;

    Ok(designs
        .create(account, project_id, name, document, actor)
        .await?)
}

pub async fn get(
    designs: &dyn DesignRepository,
    account: AccountId,
    id: DesignId,
) -> Result<DesignRecord> {
    Ok(designs.get(account, id).await?)
}

pub async fn list(
    designs: &dyn DesignRepository,
    account: AccountId,
    project_id: ProjectId,
) -> Result<Vec<DesignSummary>> {
    Ok(designs.list_for_project(account, project_id).await?)
}

pub async fn history(
    designs: &dyn DesignRepository,
    account: AccountId,
    id: DesignId,
) -> Result<Vec<VersionSummary>> {
    designs.get(account, id).await?;
    Ok(designs.history(account, id).await?)
}

/// The document as it was at one version.
///
/// Versions never change once written, which is what lets an export stay
/// reproducible and the render cache stay safe.
pub async fn snapshot(
    designs: &dyn DesignRepository,
    account: AccountId,
    id: DesignId,
    version: Version,
) -> Result<DesignDocument> {
    Ok(designs.snapshot(account, id, version).await?)
}

/// Applies a batch of operations as one version (plan sections 27 and 28).
///
/// The batch is applied to the document as it was read, then persisted with
/// `base_version` guarding the UPDATE. Loading and saving are deliberately not
/// one transaction: the guard is what makes that safe, because a write that
/// slipped in between moves the stored version and the UPDATE then matches zero
/// rows. Callers get `VERSION_CONFLICT` and rebase, rather than silently
/// overwriting the other writer.
pub async fn apply(
    designs: &dyn DesignRepository,
    account: AccountId,
    id: DesignId,
    base_version: Version,
    operations: &[DesignOperation],
    actor: &Actor,
) -> Result<DesignRecord> {
    let current = designs.get(account, id).await?;
    let updated = apply_operations(&current.document, operations)?;
    Ok(designs
        .save(account, id, base_version, &updated, actor)
        .await?)
}

/// Restores an earlier snapshot as a new version (plan section 29).
///
/// History only moves forward: restoring v9 over v10 produces v11. Decrementing
/// would let one version number describe two different documents, which is
/// exactly what optimistic concurrency relies on never happening.
pub async fn undo(
    designs: &dyn DesignRepository,
    account: AccountId,
    id: DesignId,
    base_version: Version,
    actor: &Actor,
) -> Result<DesignRecord> {
    // Resolve the scoped design before reasoning about versions. Otherwise a
    // foreign design reaches the snapshot lookup and returns a generic missing
    // version, while every other route says DESIGN_NOT_FOUND (plan 108.2).
    designs.get(account, id).await?;

    if base_version.0 <= Version::INITIAL.0 {
        return Err(ApplicationError::rejected(
            "NOTHING_TO_UNDO",
            format!("version {base_version} is the first version of this design"),
            "There is no earlier snapshot to restore. Check get_design history first.",
        ));
    }

    let previous = Version(base_version.0 - 1);
    let snapshot = designs.snapshot(account, id, previous).await?;
    Ok(designs
        .save(account, id, base_version, &snapshot, actor)
        .await?)
}

/// Deletes a design and the derived data that only existed for it.
///
/// The row cascade takes the version snapshots and the export records; the files
/// those records pointed at, and the cached previews, are on the filesystem and
/// no SQL reaches them. Plan section 74 draws the line: asset **bytes stay**,
/// because they belong to the person and other designs may reference them.
///
/// The export keys are read before the delete, since the cascade removes the
/// only record of where those files are. The design row goes first all the same:
/// a delete of something that is not there must fail without destroying
/// anything, and cleanup that fails afterwards is logged rather than raised —
/// the design is gone either way, and that is what the caller was promised.
pub async fn delete(
    designs: &dyn DesignRepository,
    exports: &dyn ExportRepository,
    store: &dyn ObjectStore,
    cache: &RenderCache,
    account: AccountId,
    id: DesignId,
) -> Result<()> {
    let exported = exports
        .list_for_design(account, id)
        .await
        .unwrap_or_default();

    designs.delete(account, id).await?;

    for export in exported {
        if let Err(error) = store.delete(&export.storage_key).await {
            tracing::warn!(%error, key = %export.storage_key, "could not remove an export of a deleted design");
        }
    }
    if let Err(error) = cache.forget_design(id) {
        tracing::warn!(%error, design_id = %id, "could not clear the preview cache of a deleted design");
    }
    Ok(())
}
