//! Plan section 12. A Project is a workspace; V1 has one active Brand Kit.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::{BrandId, ProjectId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Project {
    pub id: ProjectId,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brand_id: Option<BrandId>,
    /// RFC 3339 timestamps. The domain does not depend on a clock crate.
    pub created_at: String,
    pub updated_at: String,
}
