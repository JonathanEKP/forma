//! Plan section 42: warnings do not necessarily block export, errors do.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Severity {
    /// Valid but probably not what was intended. Never blocks export.
    Warning,
    /// The design is broken or content is being lost. Blocks export.
    Error,
}

impl Severity {
    pub fn blocks_export(self) -> bool {
        matches!(self, Self::Error)
    }
}
