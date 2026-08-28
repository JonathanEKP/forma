//! Asset checks (plan section 42).

use design_core::Layer;

use crate::context::ValidationContext;
use crate::report::ValidationIssue;

pub fn check(ctx: &ValidationContext, issues: &mut Vec<ValidationIssue>) {
    for page in &ctx.document.pages {
        for layer in &page.layers {
            let Layer::Image(image) = layer else { continue };
            let common = &image.common;

            match ctx.assets.get(&image.asset_id) {
                None => issues.push(
                    ValidationIssue::error(
                        "ASSET_NOT_FOUND",
                        format!("asset {} does not exist in this project", image.asset_id),
                    )
                    .at_layer(page.id, common.id)
                    .with_details(serde_json::json!({ "asset_id": image.asset_id })),
                ),
                Some(info) if !info.readable => issues.push(
                    ValidationIssue::error(
                        "ASSET_UNREADABLE",
                        format!("asset {} cannot be read from storage", image.asset_id),
                    )
                    .at_layer(page.id, common.id)
                    .with_details(serde_json::json!({ "asset_id": image.asset_id })),
                ),
                Some(info) if !info.raster => issues.push(
                    ValidationIssue::error(
                        "INVALID_ASSET_TYPE",
                        format!(
                            "asset {} is not an image and cannot fill an image layer",
                            image.asset_id
                        ),
                    )
                    .at_layer(page.id, common.id)
                    .with_details(serde_json::json!({ "asset_id": image.asset_id })),
                ),
                Some(_) => {}
            }
        }
    }
}
