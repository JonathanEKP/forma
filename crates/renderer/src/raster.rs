//! Rasterization (plan section 3.4): the PNG comes from the same normalized tree
//! as the preview SVG, so preview and export cannot drift apart.

use resvg::tiny_skia;

use crate::error::RenderError;

pub fn render_png(tree: &usvg::Tree, scale: f32) -> Result<Vec<u8>, RenderError> {
    render_pixmap(tree, scale)?
        .encode_png()
        .map_err(|e| RenderError::PngEncode(e.to_string()))
}

/// The raster before it is encoded. The contrast check reads pixels, so it would
/// otherwise encode a PNG only to decode it again.
pub fn render_pixmap(tree: &usvg::Tree, scale: f32) -> Result<tiny_skia::Pixmap, RenderError> {
    if scale <= 0.0 {
        return Err(RenderError::InvalidDimension(format!(
            "scale must be positive, got {scale}"
        )));
    }

    let width = (tree.size().width() * scale).round() as u32;
    let height = (tree.size().height() * scale).round() as u32;

    let mut pixmap = tiny_skia::Pixmap::new(width, height).ok_or_else(|| {
        RenderError::InvalidDimension(format!("cannot allocate a {width}x{height} pixmap"))
    })?;

    resvg::render(
        tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );

    Ok(pixmap)
}
