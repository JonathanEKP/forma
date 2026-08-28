//! Plan section 15. Layer order is the z-order: the last element is on top.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::color::DesignColor;
use crate::ids::PageId;
use crate::layer::Layer;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Page {
    pub id: PageId,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<DesignColor>,
    #[serde(default)]
    pub layers: Vec<Layer>,
}

impl Page {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: PageId::new(),
            name: name.into(),
            background: None,
            layers: Vec::new(),
        }
    }
}
