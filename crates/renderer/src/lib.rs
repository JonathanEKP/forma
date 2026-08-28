//! Forma renderer: `DesignDocument -> SVG -> PNG` (plan sections 8.3 and 33).
//!
//! This crate is independent of Actix, MCP and persistence. It is the render
//! authority: the editor preview and the exported PNG both come from here.

pub mod cache;
pub mod error;
pub mod flatten;
pub mod fonts;
pub mod images;
pub mod raster;
pub mod svg;

use design_core::{DesignDocument, Page};

pub use cache::RenderCache;
pub use error::RenderError;
pub use flatten::{LayerBounds, Rect, RenderInspection};
pub use fonts::FontRegistry;
pub use images::{AssetSource, InMemoryAssets, NoAssets, RasterAsset};
pub use svg::BrandPalette;

pub struct RenderedPage {
    /// The SVG Forma authored, before usvg touches it.
    pub authored_svg: String,
    /// usvg's normalized output, with text flattened to paths. This is what the
    /// React editor draws underneath its interaction layer (plan section 37).
    pub normalized_svg: String,
    pub inspection: RenderInspection,
}

/// Renders one page to SVG and inspects the result.
pub fn render_page(
    document: &DesignDocument,
    page: &Page,
    palette: &BrandPalette,
    fonts: &FontRegistry,
    assets: &dyn AssetSource,
) -> Result<RenderedPage, RenderError> {
    let authored_svg = svg::page_to_svg(document, page, palette, assets)?;
    let tree = flatten::parse(&authored_svg, fonts)?;

    let expected: Vec<String> = page
        .layers
        .iter()
        .filter(|l| l.common().visible)
        .map(|l| svg::layer_group_id(l.common()))
        .collect();

    Ok(RenderedPage {
        normalized_svg: flatten::to_normalized_svg(&tree),
        inspection: flatten::inspect(&tree, &expected),
        authored_svg,
    })
}

/// Renders one page straight to PNG, from the same tree the preview uses.
pub fn render_page_png(
    document: &DesignDocument,
    page: &Page,
    palette: &BrandPalette,
    fonts: &FontRegistry,
    assets: &dyn AssetSource,
    scale: f32,
) -> Result<Vec<u8>, RenderError> {
    let authored_svg = svg::page_to_svg(document, page, palette, assets)?;
    let tree = flatten::parse(&authored_svg, fonts)?;
    raster::render_png(&tree, scale)
}
