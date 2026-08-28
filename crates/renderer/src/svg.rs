//! Authoring the SVG from the document (plan section 3.4: Rust authors the SVG,
//! the browser is never the layout authority).

use std::collections::HashMap;
use std::fmt::Write as _;

use design_core::{
    Color, DesignColor, DesignDocument, Layer, LayerCommon, Page, ShapeKind, TextAlign, TextLayer,
};

use crate::error::RenderError;

/// Resolved `$brand.*` values. The document stays portable (plan section 14);
/// tokens are resolved at render time, not stored expanded.
pub type BrandPalette = HashMap<String, Color>;

/// Every layer is wrapped in a group carrying this prefix, so the editor can
/// address rendered geometry by layer id (plan section 36).
pub const LAYER_ID_PREFIX: &str = "layer-";

pub fn layer_group_id(common: &LayerCommon) -> String {
    format!("{LAYER_ID_PREFIX}{}", common.id)
}

fn resolve(color: &DesignColor, palette: &BrandPalette) -> Result<Color, RenderError> {
    match color {
        DesignColor::Literal(c) => Ok(*c),
        DesignColor::BrandToken(token) => palette.get(token).copied().ok_or_else(|| {
            // Plan section 42: UNKNOWN_BRAND_TOKEN is an error, never a fallback color.
            RenderError::RenderFailed(format!("UNKNOWN_BRAND_TOKEN: ${token}"))
        }),
    }
}

fn paint(color: &Color) -> String {
    format!("#{:02X}{:02X}{:02X}", color.r, color.g, color.b)
}

fn opacity_attr(color: &Color, layer_opacity: f32) -> String {
    let alpha = (color.a as f32 / 255.0) * layer_opacity;
    if (alpha - 1.0).abs() < f32::EPSILON {
        String::new()
    } else {
        format!(r#" opacity="{alpha:.4}""#)
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Rotation is applied around the frame center, matching `Transform::center`.
fn transform_attr(common: &LayerCommon) -> String {
    if common.transform.rotation_deg.abs() < f32::EPSILON {
        return String::new();
    }
    let (cx, cy) = common.transform.center();
    format!(
        r#" transform="rotate({:.4} {:.4} {:.4})""#,
        common.transform.rotation_deg, cx, cy
    )
}

pub fn page_to_svg(
    document: &DesignDocument,
    page: &Page,
    palette: &BrandPalette,
) -> Result<String, RenderError> {
    let format = &document.format;
    if format.width <= 0.0 || format.height <= 0.0 {
        return Err(RenderError::InvalidDimension(format!(
            "canvas must be positive, got {}x{}",
            format.width, format.height
        )));
    }

    let mut svg = String::new();
    write!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#,
        w = format.width,
        h = format.height
    )
    .unwrap();

    if let Some(background) = &page.background {
        let color = resolve(background, palette)?;
        write!(
            svg,
            r#"<rect x="0" y="0" width="{}" height="{}" fill="{}"/>"#,
            format.width,
            format.height,
            paint(&color)
        )
        .unwrap();
    }

    // Layer order is the z-order (plan section 15): first drawn is furthest back.
    for layer in &page.layers {
        if !layer.common().visible {
            continue;
        }
        match layer {
            Layer::Shape(shape) => write_shape(&mut svg, shape, palette)?,
            Layer::Text(text) => write_text(&mut svg, text, palette)?,
            // Images arrive in Phase 2 once the assets crate can supply bytes.
            Layer::Image(_) => continue,
        }
    }

    svg.push_str("</svg>");
    Ok(svg)
}

fn write_shape(
    svg: &mut String,
    shape: &design_core::ShapeLayer,
    palette: &BrandPalette,
) -> Result<(), RenderError> {
    let t = &shape.common.transform;
    let fill = match &shape.fill {
        Some(c) => {
            let color = resolve(c, palette)?;
            format!(
                r#" fill="{}"{}"#,
                paint(&color),
                opacity_attr(&color, shape.common.opacity)
            )
        }
        None => r#" fill="none""#.to_string(),
    };
    let stroke = match &shape.stroke {
        Some(c) => {
            let color = resolve(c, palette)?;
            format!(
                r#" stroke="{}" stroke-width="{}""#,
                paint(&color),
                shape.stroke_width
            )
        }
        None => String::new(),
    };

    write!(
        svg,
        r#"<g id="{}"{}>"#,
        layer_group_id(&shape.common),
        transform_attr(&shape.common)
    )
    .unwrap();

    match shape.kind {
        ShapeKind::Rectangle => {
            let radius = if shape.corner_radius > 0.0 {
                format!(r#" rx="{}""#, shape.corner_radius)
            } else {
                String::new()
            };
            write!(
                svg,
                r#"<rect x="{}" y="{}" width="{}" height="{}"{radius}{fill}{stroke}/>"#,
                t.x, t.y, t.width, t.height
            )
            .unwrap();
        }
        ShapeKind::Ellipse => {
            let (cx, cy) = t.center();
            write!(
                svg,
                r#"<ellipse cx="{}" cy="{}" rx="{}" ry="{}"{fill}{stroke}/>"#,
                cx,
                cy,
                t.width / 2.0,
                t.height / 2.0
            )
            .unwrap();
        }
        ShapeKind::Line => {
            write!(
                svg,
                r#"<line x1="{}" y1="{}" x2="{}" y2="{}"{stroke}/>"#,
                t.x,
                t.y,
                t.x + t.width,
                t.y + t.height
            )
            .unwrap();
        }
    }

    svg.push_str("</g>");
    Ok(())
}

fn write_text(
    svg: &mut String,
    text: &TextLayer,
    palette: &BrandPalette,
) -> Result<(), RenderError> {
    let color = resolve(&text.color, palette)?;
    let t = &text.common.transform;

    let (anchor, x) = match text.align {
        TextAlign::Left => ("start", t.x),
        TextAlign::Center => ("middle", t.x + t.width / 2.0),
        TextAlign::Right => ("end", t.x + t.width),
    };

    write!(
        svg,
        r#"<g id="{}"{}>"#,
        layer_group_id(&text.common),
        transform_attr(&text.common)
    )
    .unwrap();

    let letter_spacing = if text.letter_spacing.abs() > f32::EPSILON {
        format!(r#" letter-spacing="{}""#, text.letter_spacing)
    } else {
        String::new()
    };

    write!(
        svg,
        r#"<text x="{x}" y="{y}" font-family="{family}" font-size="{size}" font-weight="{weight}" fill="{fill}" text-anchor="{anchor}"{letter_spacing}{op}>"#,
        y = t.y + text.font_size,
        family = escape(&text.font_family),
        size = text.font_size,
        weight = text.font_weight,
        fill = paint(&color),
        op = opacity_attr(&color, text.common.opacity),
    )
    .unwrap();

    // Plan section 18: V1 does no implicit wrapping, one tspan per explicit line.
    let line_height = text.font_size * text.line_height;
    for (index, line) in text.lines().enumerate() {
        let dy = if index == 0 { 0.0 } else { line_height };
        write!(svg, r#"<tspan x="{x}" dy="{dy}">{}</tspan>"#, escape(line)).unwrap();
    }

    svg.push_str("</text></g>");
    Ok(())
}
