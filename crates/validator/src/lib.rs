//! Deterministic design checks (plan sections 41 to 43).
//!
//! The validator answers "is this document sound", never "is this design
//! beautiful". It measures nothing itself: text bounds come from the render
//! authority, so there is exactly one typographic implementation in the system.

pub mod assets;
pub mod bounds;
pub mod brand;
pub mod context;
pub mod document;
pub mod report;
pub mod severity;
pub mod text;

pub use context::{AssetInfo, MeasuredLayer, TextContrast, ValidationContext};
pub use report::{ValidationIssue, ValidationReport};
pub use severity::Severity;

/// Runs every check and returns one report.
///
/// Checks never short-circuit: an agent fixing a design wants the whole list,
/// not the first problem found.
pub fn validate(ctx: &ValidationContext) -> ValidationReport {
    let mut issues = Vec::new();

    document::check(ctx, &mut issues);
    bounds::check(ctx, &mut issues);
    text::check(ctx, &mut issues);
    assets::check(ctx, &mut issues);
    brand::check(ctx, &mut issues);

    ValidationReport::new(issues)
}
