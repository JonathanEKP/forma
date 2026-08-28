//! Text checks (plan section 42).
//!
//! Overflow is measured, never guessed: the bounds come from the renderer's
//! shaped glyphs (plan section 35). Without measurements the check is skipped
//! rather than approximated, because a wrong overflow report would send an
//! agent into pointless edits.

use design_core::{DesignColor, Layer};

use crate::context::ValidationContext;
use crate::report::ValidationIssue;
use crate::severity::Severity;

/// Sub-pixel differences are shaping noise, not overflow.
const OVERFLOW_TOLERANCE: f32 = 1.0;

pub fn check(ctx: &ValidationContext, issues: &mut Vec<ValidationIssue>) {
    let minimum_font_size = ctx.brand.map(|b| b.rules.minimum_font_size);

    for page in &ctx.document.pages {
        for layer in &page.layers {
            let Layer::Text(text) = layer else { continue };
            let common = &text.common;

            // The family must exist in the registry, resolving a brand token
            // first: the agent may legitimately have written $brand.display.
            let family = resolve_family(ctx, &text.font_family);
            match family {
                Some(family) if !ctx.fonts.is_empty() && !ctx.fonts.contains(&family) => {
                    issues.push(
                        ValidationIssue::error(
                            "FONT_NOT_FOUND",
                            format!("font family \"{family}\" is not in the font catalog"),
                        )
                        .at_layer(page.id, common.id)
                        .with_details(serde_json::json!({ "font_family": family })),
                    );
                }
                None => {
                    issues.push(
                        ValidationIssue::error(
                            "UNKNOWN_BRAND_TOKEN",
                            format!(
                                "font token {} is not defined by the Brand Kit",
                                text.font_family
                            ),
                        )
                        .at_layer(page.id, common.id)
                        .with_details(serde_json::json!({ "token": text.font_family })),
                    );
                }
                _ => {}
            }

            if let Some(minimum) = minimum_font_size {
                if text.font_size.is_finite() && text.font_size > 0.0 && text.font_size < minimum {
                    issues.push(
                        ValidationIssue::new(
                            "FONT_TOO_SMALL",
                            Severity::Warning,
                            format!(
                                "{}px is below the brand minimum of {minimum}px",
                                text.font_size
                            ),
                        )
                        .at_layer(page.id, common.id)
                        .with_details(serde_json::json!({
                            "font_size": text.font_size,
                            "minimum": minimum,
                        })),
                    );
                }
            }

            let Some(measured) = ctx.measurements.get(&common.id) else {
                continue;
            };
            let frame = common.transform;

            if measured.layout.width > frame.width + OVERFLOW_TOLERANCE {
                issues.push(
                    ValidationIssue::error(
                        "TEXT_OVERFLOW_WIDTH",
                        format!(
                            "\"{}\" needs {:.0}px but its box is {:.0}px wide",
                            common.name, measured.layout.width, frame.width
                        ),
                    )
                    .at_layer(page.id, common.id)
                    .with_details(serde_json::json!({
                        "measured": measured.layout.width,
                        "available": frame.width,
                        // V1 has no auto-wrap, so the fix is an explicit break.
                        "hint": "Insert a line break, shorten the copy, or reduce font_size.",
                    })),
                );
            }

            if measured.layout.height > frame.height + OVERFLOW_TOLERANCE {
                issues.push(
                    ValidationIssue::error(
                        "TEXT_OVERFLOW_HEIGHT",
                        format!(
                            "\"{}\" needs {:.0}px but its box is {:.0}px tall",
                            common.name, measured.layout.height, frame.height
                        ),
                    )
                    .at_layer(page.id, common.id)
                    .with_details(serde_json::json!({
                        "measured": measured.layout.height,
                        "available": frame.height,
                        "hint": "Remove a line, reduce font_size, or make the box taller.",
                    })),
                );
            }
        }
    }
}

/// Resolves a font token to a real family name. A literal family passes through.
fn resolve_family(ctx: &ValidationContext, family: &str) -> Option<String> {
    let Some(token) = family.strip_prefix('$') else {
        return Some(family.to_string());
    };
    ctx.brand?.font(token).ok().map(str::to_string)
}

/// Colors are checked here because a text layer is where an off-brand color is
/// most visible, but the same rule applies to shapes (see `brand.rs`).
pub fn color_is_token(color: &DesignColor) -> bool {
    matches!(color, DesignColor::BrandToken(_))
}
