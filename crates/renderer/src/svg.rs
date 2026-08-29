//! Authoring the SVG from the document (plan section 3.4: Rust authors the SVG,
//! the browser is never the layout authority).

use std::collections::HashMap;
use std::fmt::Write as _;

use base64::Engine as _;
use design_core::{
    Color, DesignColor, DesignDocument, ImageLayer, Layer, LayerCommon, OverlayEdge, Page,
    ShapeKind, TextAlign, TextLayer,
};

use crate::error::RenderError;
use crate::images::{place, AssetSource};

/// Resolved `$brand.*` values. The document stays portable (plan section 14);
/// tokens are resolved at render time, not stored expanded.
///
/// Fonts live here beside the colours because they are resolved the same way and
/// at the same moment. They were missing, and the renderer wrote `$brand.display`
/// into the SVG as if it were a family name: usvg found no such family, drew
/// nothing at all, and the validator — which *did* resolve the token — called the
/// design valid. A page that lost its headline entirely passed every check.
#[derive(Debug, Clone, Default)]
pub struct BrandPalette {
    colors: HashMap<String, Color>,
    fonts: HashMap<String, String>,
}

impl BrandPalette {
    pub fn new(colors: HashMap<String, Color>, fonts: HashMap<String, String>) -> Self {
        Self { colors, fonts }
    }

    /// Colours only, for callers with nothing to say about typography. A text
    /// layer naming a font token against one of these fails to render, which is
    /// the same answer an unknown colour token gets.
    pub fn from_colors(colors: HashMap<String, Color>) -> Self {
        Self {
            colors,
            fonts: HashMap::new(),
        }
    }

    pub fn color(&self, token: &str) -> Option<Color> {
        self.colors.get(token).copied()
    }

    pub fn font(&self, token: &str) -> Option<&str> {
        self.fonts.get(token).map(String::as_str)
    }
}

impl From<HashMap<String, Color>> for BrandPalette {
    fn from(colors: HashMap<String, Color>) -> Self {
        Self::from_colors(colors)
    }
}

/// Resolves a font family, which may be a `$brand.*` token. An unresolvable one
/// is fatal for the same reason an unresolvable colour is (plan section 42):
/// there is no sane substitute, and drawing nothing silently is worse.
pub(crate) fn resolve_family(family: &str, palette: &BrandPalette) -> Result<String, RenderError> {
    let Some(token) = family.strip_prefix('$') else {
        return Ok(family.to_string());
    };
    palette
        .font(token)
        .map(str::to_string)
        .ok_or_else(|| RenderError::RenderFailed(format!("UNKNOWN_BRAND_TOKEN: ${token}")))
}

/// Every layer is wrapped in a group carrying this prefix, so the editor can
/// address rendered geometry by layer id (plan section 36).
pub const LAYER_ID_PREFIX: &str = "layer-";

pub fn layer_group_id(common: &LayerCommon) -> String {
    format!("{LAYER_ID_PREFIX}{}", common.id)
}

pub(crate) fn resolve(color: &DesignColor, palette: &BrandPalette) -> Result<Color, RenderError> {
    match color {
        DesignColor::Literal(c) => Ok(*c),
        DesignColor::BrandToken(token) => palette.color(token).ok_or_else(|| {
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
    assets: &dyn AssetSource,
) -> Result<String, RenderError> {
    let format = &document.format;
    if format.width <= 0.0 || format.height <= 0.0 {
        return Err(RenderError::InvalidDimension(format!(
            "canvas must be positive, got {}x{}",
            format.width, format.height
        )));
    }

    // Clip paths are collected into a <defs> block so a layer group can
    // reference its own clip. Putting the clip on the group rather than on the
    // child is what makes usvg report clipped bounds for the layer.
    let mut defs = String::new();
    let mut body = String::new();

    let mut svg = String::new();
    write!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#,
        w = format.width,
        h = format.height
    )
    .unwrap();

    if let Some(background) = &page.background {
        let color = resolve(background, palette)?;
        write!(
            body,
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
            Layer::Shape(shape) => write_shape(&mut body, shape, palette)?,
            Layer::Text(text) => write_text(&mut body, text, palette)?,
            Layer::Image(image) => write_image(&mut body, &mut defs, image, palette, assets)?,
        }
    }

    if !defs.is_empty() {
        write!(svg, "<defs>{defs}</defs>").unwrap();
    }
    svg.push_str(&body);
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
        family = escape(&resolve_family(&text.font_family, palette)?),
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

fn write_image(
    svg: &mut String,
    defs: &mut String,
    image: &ImageLayer,
    palette: &BrandPalette,
    assets: &dyn AssetSource,
) -> Result<(), RenderError> {
    let asset = assets
        .load(image.asset_id)
        .ok_or(RenderError::AssetNotFound(image.asset_id))?;

    let placement = place(
        &image.common.transform,
        (asset.width, asset.height),
        image.fit,
        image.focal_point,
    );

    let group_id = layer_group_id(&image.common);

    // Cover overflows its frame by design, so the frame becomes a clip path.
    // It goes on the group, not on the <image>: that way the clipped extent is
    // what usvg reports as the layer's painted bounds.
    let clip = if placement.clip {
        let t = &image.common.transform;
        write!(
            defs,
            r#"<clipPath id="clip-{group_id}"><rect x="{}" y="{}" width="{}" height="{}"/></clipPath>"#,
            t.x, t.y, t.width, t.height
        )
        .unwrap();
        format!(r##" clip-path="url(#clip-{group_id})""##)
    } else {
        String::new()
    };

    write!(
        svg,
        r#"<g id="{group_id}"{}{clip}>"#,
        transform_attr(&image.common)
    )
    .unwrap();

    let opacity = if (image.common.opacity - 1.0).abs() < f32::EPSILON {
        String::new()
    } else {
        format!(r#" opacity="{:.4}""#, image.common.opacity)
    };

    // The bytes are embedded, never linked: the renderer must not reach out to
    // the network or the filesystem while drawing (plan section 45).
    let encoded = base64::engine::general_purpose::STANDARD.encode(&asset.bytes);

    write!(
        svg,
        r#"<image x="{}" y="{}" width="{}" height="{}" preserveAspectRatio="none"{opacity} xlink:href="data:{};base64,{encoded}"/>"#,
        placement.x, placement.y, placement.width, placement.height, asset.mime_type
    )
    .unwrap();

    // The tint covers the photograph rather than the frame, so a `contain` image
    // is darkened where it actually is and the layer's painted bounds do not
    // grow to the frame it was fitted inside.
    if let Some(overlay) = &image.overlay {
        let color = resolve(&overlay.color, palette)?;
        let strength = (overlay.opacity * image.common.opacity).clamp(0.0, 1.0);
        let paint = match overlay.from {
            None => format!(r#"fill="{}" fill-opacity="{strength:.4}""#, paint(&color)),
            Some(edge) => {
                let (x1, y1, x2, y2) = match edge {
                    OverlayEdge::Top => (0, 0, 0, 1),
                    OverlayEdge::Bottom => (0, 1, 0, 0),
                    OverlayEdge::Left => (0, 0, 1, 0),
                    OverlayEdge::Right => (1, 0, 0, 0),
                };
                write!(
                    defs,
                    r#"<linearGradient id="tint-{group_id}" x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}"><stop offset="0" stop-color="{c}" stop-opacity="{strength:.4}"/><stop offset="1" stop-color="{c}" stop-opacity="0"/></linearGradient>"#,
                    c = paint(&color)
                )
                .unwrap();
                format!(r##"fill="url(#tint-{group_id})""##)
            }
        };
        write!(
            svg,
            r#"<rect x="{}" y="{}" width="{}" height="{}" {paint}/>"#,
            placement.x, placement.y, placement.width, placement.height
        )
        .unwrap();
    }

    svg.push_str("</g>");
    Ok(())
}
