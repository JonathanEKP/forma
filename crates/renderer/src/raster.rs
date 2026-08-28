//! Rasterization (plan section 3.4): the PNG comes from the same normalized tree
//! as the preview SVG, so preview and export cannot drift apart.

use resvg::tiny_skia;

use crate::error::RenderError;

pub fn render_png(tree: &usvg::Tree, scale: f32) -> Result<Vec<u8>, RenderError> {
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

    pixmap
        .encode_png()
        .map_err(|e| RenderError::PngEncode(e.to_string()))
}
