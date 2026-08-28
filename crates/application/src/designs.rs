//! Design use cases (plan section 8.2).
//!
//! These orchestrate; they do not re-implement. `apply_operations` owns batch
//! semantics, the repository owns the guarded UPDATE, and the renderer owns
//! measurement. What lives here is the order those are called in, which is the
//! part every transport would otherwise duplicate.

use design_core::{
    apply_operations, Actor, DesignDocument, DesignId, DesignOperation, DesignRecord,
    DesignSummary, ProjectId, Version,
};
use persistence::{DesignRepository, VersionSummary};

use crate::error::{ApplicationError, Result};

pub async fn create(
    designs: &dyn DesignRepository,
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

    Ok(designs.create(project_id, name, document, actor).await?)
}

pub async fn get(designs: &dyn DesignRepository, id: DesignId) -> Result<DesignRecord> {
    Ok(designs.get(id).await?)
}

pub async fn list(
    designs: &dyn DesignRepository,
    project_id: ProjectId,
) -> Result<Vec<DesignSummary>> {
    Ok(designs.list_for_project(project_id).await?)
}

pub async fn history(designs: &dyn DesignRepository, id: DesignId) -> Result<Vec<VersionSummary>> {
    Ok(designs.history(id).await?)
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
    id: DesignId,
    base_version: Version,
    operations: &[DesignOperation],
    actor: &Actor,
) -> Result<DesignRecord> {
    let current = designs.get(id).await?;
    let updated = apply_operations(&current.document, operations)?;
    Ok(designs.save(id, base_version, &updated, actor).await?)
}

/// Restores an earlier snapshot as a new version (plan section 29).
///
/// History only moves forward: restoring v9 over v10 produces v11. Decrementing
/// would let one version number describe two different documents, which is
/// exactly what optimistic concurrency relies on never happening.
pub async fn undo(
    designs: &dyn DesignRepository,
    id: DesignId,
    base_version: Version,
    actor: &Actor,
) -> Result<DesignRecord> {
    if base_version.0 <= Version::INITIAL.0 {
        return Err(ApplicationError::rejected(
            "NOTHING_TO_UNDO",
            format!("version {base_version} is the first version of this design"),
            "There is no earlier snapshot to restore. Check get_design history first.",
        ));
    }

    let previous = Version(base_version.0 - 1);
    let snapshot = designs.snapshot(id, previous).await?;
    Ok(designs.save(id, base_version, &snapshot, actor).await?)
}

pub async fn delete(designs: &dyn DesignRepository, id: DesignId) -> Result<()> {
    Ok(designs.delete(id).await?)
}
