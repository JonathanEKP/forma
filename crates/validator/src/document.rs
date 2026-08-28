//! Structural checks (plan section 42).

use std::collections::HashSet;

use design_core::Layer;

use crate::context::ValidationContext;
use crate::report::ValidationIssue;
use crate::severity::Severity;

pub fn check(ctx: &ValidationContext, issues: &mut Vec<ValidationIssue>) {
    let format = &ctx.document.format;
    if !format.width.is_finite()
        || !format.height.is_finite()
        || format.width <= 0.0
        || format.height <= 0.0
    {
        issues.push(
            ValidationIssue::error(
                "INVALID_PAGE_SIZE",
                format!("canvas is {}x{}", format.width, format.height),
            )
            .with_details(serde_json::json!({
                "width": format.width,
                "height": format.height,
            })),
        );
    }

    let mut seen_layers = HashSet::new();
    for page in &ctx.document.pages {
        if page.layers.is_empty() {
            issues.push(
                ValidationIssue::new(
                    "MISSING_LAYER",
                    Severity::Warning,
                    format!("page \"{}\" has no layers and renders empty", page.name),
                )
                .at_page(page.id),
            );
        }

        for layer in &page.layers {
            let common = layer.common();

            if !seen_layers.insert(common.id) {
                issues.push(
                    ValidationIssue::error(
                        "DUPLICATE_LAYER_ID",
                        format!("layer id {} is used more than once", common.id),
                    )
                    .at_layer(page.id, common.id),
                );
            }

            if !common.opacity.is_finite() || !(0.0..=1.0).contains(&common.opacity) {
                issues.push(
                    ValidationIssue::error(
                        "INVALID_OPACITY",
                        format!("opacity {} is outside 0..=1", common.opacity),
                    )
                    .at_layer(page.id, common.id)
                    .with_details(serde_json::json!({ "opacity": common.opacity })),
                );
            }

            let t = &common.transform;
            let bad_dimension = [
                ("x", t.x, false),
                ("y", t.y, false),
                ("width", t.width, true),
                ("height", t.height, true),
                ("rotation_deg", t.rotation_deg, false),
            ]
            .into_iter()
            .find(|(_, value, must_be_positive)| {
                !value.is_finite() || (*must_be_positive && *value <= 0.0)
            });

            if let Some((field, value, _)) = bad_dimension {
                issues.push(
                    ValidationIssue::error("INVALID_DIMENSION", format!("{field} is {value}"))
                        .at_layer(page.id, common.id)
                        .with_details(serde_json::json!({ "field": field, "value": value })),
                );
            }

            if let Layer::Text(text) = layer {
                if !text.font_size.is_finite() || text.font_size <= 0.0 {
                    issues.push(
                        ValidationIssue::error(
                            "INVALID_DIMENSION",
                            format!("font_size is {}", text.font_size),
                        )
                        .at_layer(page.id, common.id)
                        .with_details(
                            serde_json::json!({ "field": "font_size", "value": text.font_size }),
                        ),
                    );
                }
            }
        }
    }
}
