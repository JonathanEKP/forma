//! Canvas and safe-area checks (plan sections 15 and 42).
//!
//! Plan section 15 is explicit that leaving the canvas is not automatically an
//! error: a photo may bleed deliberately. What matters is whether *content the
//! reader needs* is being lost, so text is judged more strictly than decoration.

use design_core::{Layer, LayerRole, Rect};

use crate::context::ValidationContext;
use crate::report::ValidationIssue;
use crate::severity::Severity;

/// Layers whose loss actually costs the reader something.
fn is_critical(layer: &Layer) -> bool {
    match layer {
        Layer::Text(_) => true,
        Layer::Image(image) => matches!(image.common.role, Some(LayerRole::Logo)),
        Layer::Shape(_) => false,
    }
}

/// What the layer actually puts on the canvas. Falls back to the frame when the
/// design has not been rendered.
fn painted_area(ctx: &ValidationContext, layer: &Layer) -> Rect {
    ctx.measurements
        .get(&layer.id())
        .map(|m| m.painted)
        .unwrap_or_else(|| Rect::from(layer.common().transform))
}

pub fn check(ctx: &ValidationContext, issues: &mut Vec<ValidationIssue>) {
    let canvas = ctx.canvas();
    if canvas.width <= 0.0 || canvas.height <= 0.0 {
        // INVALID_PAGE_SIZE already covers this; bounds are meaningless here.
        return;
    }
    let safe_area = ctx.safe_area();

    for page in &ctx.document.pages {
        for layer in &page.layers {
            let common = layer.common();
            if !common.visible {
                continue;
            }

            let area = painted_area(ctx, layer);
            let critical = is_critical(layer);

            if canvas.is_disjoint(&area) {
                // Nothing of this layer reaches the canvas at all, which is a
                // mistake whatever the layer is.
                issues.push(
                    ValidationIssue::error(
                        "OUTSIDE_CANVAS",
                        format!("\"{}\" is entirely outside the canvas", common.name),
                    )
                    .at_layer(page.id, common.id)
                    .with_details(serde_json::json!({
                        "painted": area,
                        "canvas": canvas,
                        "clipped": "fully",
                    })),
                );
            } else if !canvas.contains(&area) {
                // Partly off-canvas. A bleeding photo is intentional; a
                // half-visible headline is lost copy.
                let severity = if critical {
                    Severity::Error
                } else {
                    Severity::Warning
                };
                let message = if critical {
                    format!("\"{}\" is cut off by the canvas edge", common.name)
                } else {
                    format!("\"{}\" bleeds past the canvas edge", common.name)
                };
                issues.push(
                    ValidationIssue::new("OUTSIDE_CANVAS", severity, message)
                        .at_layer(page.id, common.id)
                        .with_details(serde_json::json!({
                            "painted": area,
                            "canvas": canvas,
                            "clipped": "partially",
                        })),
                );
            }

            // Safe area only constrains content that must be readable.
            if let Some(safe) = safe_area {
                if critical && !safe.contains(&area) && !canvas.is_disjoint(&area) {
                    issues.push(
                        ValidationIssue::new(
                            "OUTSIDE_SAFE_AREA",
                            Severity::Warning,
                            format!(
                                "\"{}\" extends past the {}px safe margin",
                                common.name,
                                ctx.brand.map(|b| b.rules.safe_margin).unwrap_or_default()
                            ),
                        )
                        .at_layer(page.id, common.id)
                        .with_details(serde_json::json!({
                            "painted": area,
                            "safe_area": safe,
                        })),
                    );
                }
            }
        }
    }
}
