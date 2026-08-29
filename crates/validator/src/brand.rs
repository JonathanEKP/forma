//! Brand checks (plan sections 42 and 43).
//!
//! Section 43 is explicit that a literal color is allowed: the human editor may
//! use one, and a literal the user asked for explicitly must survive. So an
//! off-brand color is a warning, never a rejection.
//!
//! Note the asymmetry when a project has no Brand Kit yet. An off-brand literal
//! is unreportable: there is no palette to be off. But a `$brand.*` token has
//! nothing to resolve against, and the renderer rejects it outright, so it must
//! still be an error here. Otherwise a design validates clean and then fails to
//! render, which breaks the validate-fix-render-export loop of section 51.7.

use design_core::{DesignColor, Layer};

use crate::context::ValidationContext;
use crate::report::ValidationIssue;
use crate::severity::Severity;

pub fn check(ctx: &ValidationContext, issues: &mut Vec<ValidationIssue>) {
    // A Brand Kit naming fonts the renderer cannot draw will silently degrade
    // every design that uses a token, so it is worth reporting once.
    if let Some(brand) = ctx.brand {
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
    }

    // Empty when there is no Brand Kit, which is exactly why literals are not
    // judged in that case while tokens still are.
    let palette: Vec<_> = ctx
        .brand
        .map(|brand| brand.color_palette().into_values().collect())
        .unwrap_or_default();

    for page in &ctx.document.pages {
        if let Some(background) = &page.background {
            // A page background has no layer of its own to blame.
            check_color(
                ctx.brand,
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
                    ctx.brand,
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
                            ctx.brand,
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
                            ctx.brand,
                            &palette,
                            stroke,
                            Some(shape.common.id),
                            "stroke",
                            page.id,
                            issues,
                        );
                    }
                }
                // An image layer used to carry no colour at all. Its overlay
                // does, and a token the kit cannot resolve is fatal in the
                // renderer, so leaving this arm empty would let a design
                // validate clean and then refuse to draw.
                Layer::Image(image) => {
                    if let Some(overlay) = &image.overlay {
                        check_color(
                            ctx.brand,
                            &palette,
                            &overlay.color,
                            Some(image.common.id),
                            "overlay color",
                            page.id,
                            issues,
                        );
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn check_color(
    brand: Option<&design_core::BrandKit>,
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
            // With no Brand Kit the token is unresolvable, which the renderer
            // treats as fatal, so it cannot be waved through here either.
            let (unresolved, message) = match brand {
                Some(brand) => (
                    brand.color(token).is_err(),
                    format!("{what} uses ${token}, which the Brand Kit does not define"),
                ),
                None => (
                    true,
                    format!("{what} uses ${token}, but this project has no Brand Kit"),
                ),
            };

            if unresolved {
                issues.push(
                    locate(ValidationIssue::error("UNKNOWN_BRAND_TOKEN", message)).with_details(
                        serde_json::json!({
                            "token": token,
                            "hint": match brand {
                                Some(_) => "Call get_brand and use a token it lists, or pass a color literal.",
                                None => "Define a Brand Kit for this project, or use a color literal instead of a token.",
                            },
                        }),
                    ),
                );
            }
        }
        DesignColor::Literal(literal) => {
            // Nothing to be off-brand against without a Brand Kit.
            if brand.is_some() && !palette.contains(literal) {
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
