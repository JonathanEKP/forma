//! Brand checks (plan sections 42 and 43).
//!
//! Section 43 is explicit that a literal color is allowed: the human editor may
//! use one, and a literal the user asked for explicitly must survive. So an
//! off-brand color is a warning, never a rejection.

use design_core::{DesignColor, Layer};

use crate::context::ValidationContext;
use crate::report::ValidationIssue;
use crate::severity::Severity;

pub fn check(ctx: &ValidationContext, issues: &mut Vec<ValidationIssue>) {
    let Some(brand) = ctx.brand else {
        // No Brand Kit yet: there is nothing to be off-brand against.
        return;
    };

    // A Brand Kit naming fonts the renderer cannot draw will silently degrade
    // every design that uses a token, so it is worth reporting once.
    if !ctx.fonts.is_empty() {
        for (token, family) in [
            ("brand.display", &brand.typography.display),
            ("brand.body", &brand.typography.body),
            ("brand.mono", &brand.typography.mono),
        ] {
            if !ctx.fonts.contains(family) {
                issues.push(
                    ValidationIssue::new(
                        "MISSING_BRAND_FONT",
                        Severity::Warning,
                        format!("${token} is \"{family}\", which is not in the font catalog"),
                    )
                    .with_details(serde_json::json!({ "token": token, "font_family": family })),
                );
            }
        }
    }

    let palette: Vec<_> = brand.color_palette().into_values().collect();

    for page in &ctx.document.pages {
        if let Some(background) = &page.background {
            // A page background has no layer of its own to blame.
            check_color(
                brand,
                &palette,
                background,
                None,
                "page background",
                page.id,
                issues,
            );
        }

        for layer in &page.layers {
            match layer {
                Layer::Text(text) => check_color(
                    brand,
                    &palette,
                    &text.color,
                    Some(text.common.id),
                    "text color",
                    page.id,
                    issues,
                ),
                Layer::Shape(shape) => {
                    if let Some(fill) = &shape.fill {
                        check_color(
                            brand,
                            &palette,
                            fill,
                            Some(shape.common.id),
                            "fill",
                            page.id,
                            issues,
                        );
                    }
                    if let Some(stroke) = &shape.stroke {
                        check_color(
                            brand,
                            &palette,
                            stroke,
                            Some(shape.common.id),
                            "stroke",
                            page.id,
                            issues,
                        );
                    }
                }
                Layer::Image(_) => {}
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn check_color(
    brand: &design_core::BrandKit,
    palette: &[design_core::Color],
    color: &DesignColor,
    layer_id: Option<design_core::LayerId>,
    what: &str,
    page_id: design_core::PageId,
    issues: &mut Vec<ValidationIssue>,
) {
    let locate = |issue: ValidationIssue| match layer_id {
        Some(layer) => issue.at_layer(page_id, layer),
        None => issue.at_page(page_id),
    };

    match color {
        DesignColor::BrandToken(token) => {
            if brand.color(token).is_err() {
                issues.push(
                    locate(ValidationIssue::error(
                        "UNKNOWN_BRAND_TOKEN",
                        format!("{what} uses ${token}, which the Brand Kit does not define"),
                    ))
                    .with_details(serde_json::json!({ "token": token })),
                );
            }
        }
        DesignColor::Literal(literal) => {
            if !palette.contains(literal) {
                issues.push(
                    locate(ValidationIssue::new(
                        "COLOR_OUTSIDE_BRAND",
                        Severity::Warning,
                        format!("{what} uses {literal}, which is not a brand color"),
                    ))
                    .with_details(serde_json::json!({
                        "color": literal.to_string(),
                        "hint": "Use a $brand.* token unless the literal was requested explicitly.",
                    })),
                );
            }
        }
    }
}
