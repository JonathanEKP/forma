//! Typed ID wrappers (plan section 11.1): IDs must not be interchangeable strings.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

macro_rules! typed_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
        #[serde(transparent)]
        #[ts(export, type = "string")]
        pub struct $name(pub Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

typed_id!(ProjectId);
typed_id!(DesignId);
typed_id!(PageId);
typed_id!(LayerId);
typed_id!(AssetId);
typed_id!(BrandId);
typed_id!(AssetRequestId);
typed_id!(ExportId);
typed_id!(TemplateId);
