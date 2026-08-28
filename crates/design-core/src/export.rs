//! Exports (plan section 71).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::{DesignId, ExportId, PageId};
use crate::version::Version;

/// A single page or the whole design.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ExportScope {
    SinglePage,
    AllPages,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Export {
    pub id: ExportId,
    pub design_id: DesignId,
    /// The version that was exported. Pinning it is what makes an export
    /// reproducible after the design moves on.
    pub design_version: Version,
    pub scope: ExportScope,
    /// Set for a single-page export.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_id: Option<PageId>,
    pub scale: f32,
    pub storage_key: String,
    #[ts(type = "number")]
    pub byte_size: i64,
    pub created_at: String,
}
