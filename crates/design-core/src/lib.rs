//! Portable domain types for Forma designs.
//!
//! Per plan section 8.1 this crate depends on no infrastructure: no Actix, no
//! SQLx, no MCP, no filesystem, no HTTP. It holds only the domain.

pub mod color;
pub mod document;
pub mod error;
pub mod geometry;
pub mod ids;
pub mod layer;
pub mod page;

pub use color::{Color, DesignColor};
pub use document::{DesignDocument, DesignFormat, FormatPreset, SCHEMA_VERSION};
pub use error::DomainError;
pub use geometry::Transform;
pub use ids::{LayerId, PageId};
pub use layer::{Layer, LayerCommon, LayerRole, ShapeKind, ShapeLayer, TextAlign, TextLayer};
pub use page::Page;
