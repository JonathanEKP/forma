//! Asset requests (plan section 22).
//!
//! The mechanism that keeps an agent from stalling when an image it needs does
//! not exist: it records what it needs and the user resolves it by uploading.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::{AssetId, AssetRequestId, DesignId, ProjectId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum AssetRequestStatus {
    Pending,
    Resolved,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AssetRequest {
    pub id: AssetRequestId,
    pub project_id: ProjectId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub design_id: Option<DesignId>,
    /// What the image is for, in the design's terms.
    pub purpose: String,
    pub description: String,
    /// A prompt the user can paste into whatever tool they prefer. Forma never
    /// runs it: image generation is out of scope for V1 (plan section 2.2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested_width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested_height: Option<u32>,
    pub status: AssetRequestStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_asset_id: Option<AssetId>,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<String>,
}

impl AssetRequest {
    pub fn is_open(&self) -> bool {
        self.status == AssetRequestStatus::Pending
    }
}
