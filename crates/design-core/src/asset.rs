//! Assets (plan section 21).
//!
//! An asset is content the project owns. A layer references it by id only, so
//! provenance and licence stay attached to the asset rather than to the design.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::{AssetId, ProjectId};

/// What the renderer can do with the asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum AssetKind {
    Raster,
    Vector,
}

/// Where the asset came from. V1 only produces `UserUpload`; the other variants
/// exist so the schema can grow without V1 depending on those features.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum AssetOrigin {
    UserUpload,
    RemoteUrl,
    HostGenerated,
    Stock,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Asset {
    pub id: AssetId,
    pub project_id: ProjectId,
    /// BLAKE3 of the bytes, used for deduplication (plan section 73).
    pub hash: String,
    pub kind: AssetKind,
    pub mime_type: String,
    pub width: u32,
    pub height: u32,
    #[ts(type = "number")]
    pub byte_size: i64,
    /// Where the object store keeps the bytes. Never a URL the renderer follows.
    pub storage_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_filename: Option<String>,
    pub origin: AssetOrigin,
    /// Licence or credit text, kept with the asset and not with the layer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution: Option<String>,
    pub created_at: String,
}

impl Asset {
    pub fn is_usable_as_image(&self) -> bool {
        matches!(self.kind, AssetKind::Raster | AssetKind::Vector)
    }
}
