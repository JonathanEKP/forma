//! Text checks (plan section 42).
//!
//! Overflow is measured, never guessed: the bounds come from the renderer's
//! shaped glyphs (plan section 35). Without measurements the check is skipped
//! rather than approximated, because a wrong overflow report would send an
//! agent into pointless edits.

use design_core::{forbidden_in_xml, DesignColor, Layer};

use crate::context::ValidationContext;
use crate::report::ValidationIssue;
use crate::severity::Severity;

/// Sub-pixel differences are shaping noise, not overflow.
const OVERFLOW_TOLERANCE: f32 = 1.0;

/// WCAG's floor for large text. Below it, a line stops being comfortable
/// against a photograph.
const READABLE_RATIO: f32 = 3.0;

/// Near enough to the backdrop's own tone that the copy is simply lost. That is
/// an error rather than a warning for the same reason text off the canvas is:
/// the words do not reach the reader.
const INVISIBLE_RATIO: f32 = 1.5;

/// Perceptual distance at which two colours are plainly different whatever their
/// lightness. The renderer measures it; this is the line. Without it the check
/// calls an accent red on wet asphalt invisible -- the two are nearly the same
/// lightness -- and sends an agent to fix the one element that worked.
const DISTINCT_DELTA_E: f32 = 25.0;

/// A line can average out to a passing ratio while most of it lies on a patch
/// that swallows it.
const MOSTLY_ILLEGIBLE: f32 = 0.5;

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

            // A character XML cannot carry makes the whole page fail to render,
            // so the document is unusable rather than merely ugly. New documents
            // cannot acquire one -- `validate_structure` rejects them -- but a
            // design written before that check exists still needs to be told
            // what is wrong with it, rather than meeting a 500 at render time.
            if let Some(bad) = text.text.chars().find(|c| forbidden_in_xml(*c)) {
                issues.push(
                    ValidationIssue::error(
                        "TEXT_INVALID_CHARACTER",
                        format!(
                            "\"{}\" contains U+{:04X}, which the renderer cannot draw",
                            common.name, bad as u32
                        ),
                    )
                    .at_layer(page.id, common.id)
                    .with_details(serde_json::json!({
                        "codepoint": format!("U+{:04X}", bad as u32),
                        "hint": "Remove the control character from the text; only tab, \
                                 newline and carriage return are legal.",
                    })),
                );
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

            if let Some(contrast) = measured.contrast {
                // Two ways to be unreadable. A line can sit on a backdrop of
                // nearly its own tone, and a line can average out to a passing
                // number while crossing a bright stripe that swallows half of
                // it, so both the mean and the spread are checked.
                let alike = contrast.delta_e < DISTINCT_DELTA_E;
                let issue = if alike && contrast.ratio < INVISIBLE_RATIO {
                    Some((
                        Severity::Error,
                        format!(
                            "\"{}\" is nearly the same tone as what is behind it",
                            common.name
                        ),
                    ))
                } else if alike && contrast.ratio < READABLE_RATIO {
                    Some((
                        Severity::Warning,
                        format!(
                            "\"{}\" has little contrast against its backdrop",
                            common.name
                        ),
                    ))
                } else if contrast.illegible_fraction > MOSTLY_ILLEGIBLE {
                    Some((
                        Severity::Warning,
                        format!(
                            "\"{}\" reads on average but not over most of its own area",
                            common.name
                        ),
                    ))
                } else {
                    None
                };

                if let Some((severity, message)) = issue {
                    issues.push(
                        ValidationIssue::new("TEXT_LOW_CONTRAST", severity, message)
                            .at_layer(page.id, common.id)
                            .with_details(serde_json::json!({
                                "ratio": contrast.ratio,
                                "illegible_fraction": contrast.illegible_fraction,
                                "delta_e": contrast.delta_e,
                                "readable_ratio": READABLE_RATIO,
                                "hint": "Use a near-white or near-black over photography; \
                                         mid tones only read against a solid background.",
                            })),
                    );
                }
            }
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
