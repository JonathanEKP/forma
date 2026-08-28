//! Portable domain types for Forma designs.
//!
//! Per plan section 8.1 this crate depends on no infrastructure: no Actix, no
//! SQLx, no MCP, no filesystem, no HTTP. It holds only the domain.

pub mod asset;
pub mod asset_request;
pub mod brand;
pub mod color;
pub mod document;
pub mod error;
pub mod export;
pub mod geometry;
pub mod ids;
pub mod layer;
pub mod operation;
pub mod page;
pub mod project;
pub mod version;

pub use asset::{Asset, AssetKind, AssetOrigin};
pub use asset_request::{AssetRequest, AssetRequestStatus};
pub use brand::{BrandColors, BrandKit, BrandRules, BrandTypography, COLOR_TOKENS, FONT_TOKENS};
pub use color::{Color, DesignColor};
pub use document::{
    DesignDocument, DesignFormat, DesignRecord, DesignSummary, FormatPreset, SCHEMA_VERSION,
};
pub use error::DomainError;
pub use export::{Export, ExportScope};
pub use geometry::{Rect, Transform};
pub use ids::{AssetId, AssetRequestId, BrandId, DesignId, ExportId, LayerId, PageId, ProjectId};
pub use layer::{
    FocalPoint, ImageFit, ImageLayer, Layer, LayerCommon, LayerRole, ShapeKind, ShapeLayer,
    TextAlign, TextLayer,
};
pub use operation::{
    apply_operations, DesignOperation, FramePatch, ShapeStylePatch, TextStylePatch,
    MAX_OPS_PER_BATCH,
};
pub use page::Page;
pub use project::Project;
pub use version::{Actor, ActorType, Version};
