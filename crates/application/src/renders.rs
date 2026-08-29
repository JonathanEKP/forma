//! Render and validation use cases.
//!
//! The preview the editor draws and the PNG that gets exported come from this
//! one path (plan section 3.4). Anything that rendered a design a second way
//! would eventually disagree with the export, which is risk R3.

use std::collections::HashMap;

use design_core::{
    AssetId, BrandKit, DesignDocument, DesignId, DesignRecord, DomainError, Layer, LayerId, Page,
    PageId, Version,
};
use persistence::{AssetRepository, ObjectStore};
use renderer::{
    render_page, svg::BrandPalette, AssetSource, FontRegistry, InMemoryAssets, RasterAsset,
    RenderCache, RenderedPage,
};
use validator::{AssetInfo, MeasuredLayer, TextContrast, ValidationContext, ValidationReport};

use crate::error::{ApplicationError, Result};

/// The colors `$brand.*` tokens resolve against.
///
/// A project with no Brand Kit yet still has to render, so an absent kit yields
/// an empty palette: the renderer then reports the unresolved token rather than
/// inventing a color.
pub fn palette(brand: Option<&BrandKit>) -> BrandPalette {
    let Some(brand) = brand else {
        return BrandPalette::default();
    };
    BrandPalette::new(brand.color_palette(), brand.font_families())
}

fn find_page(document: &DesignDocument, page_id: PageId) -> Result<&Page> {
    document
        .pages
        .iter()
        .find(|p| p.id == page_id)
        .ok_or_else(|| DomainError::PageNotFound(page_id).into())
}

/// What the validator is told about each asset a document names.
///
/// An `ImageLayer` may reference an asset that was soft-deleted or that is not
/// an image at all; both are reported, and they are different problems.
pub async fn asset_report(
    assets: &dyn AssetRepository,
    store: &dyn ObjectStore,
    document: &DesignDocument,
) -> HashMap<AssetId, AssetInfo> {
    let mut report = HashMap::new();

    for page in &document.pages {
        for layer in &page.layers {
            let Layer::Image(image) = layer else { continue };
            if report.contains_key(&image.asset_id) {
                continue;
            }

            let info = match assets.get(image.asset_id).await {
                Err(_) => AssetInfo::unreadable(),
                Ok(asset) if !asset.is_usable_as_image() => AssetInfo::wrong_type(),
                Ok(asset) => match store.exists(&asset.storage_key).await {
                    Ok(true) => AssetInfo::usable(),
                    _ => AssetInfo::unreadable(),
                },
            };
            report.insert(image.asset_id, info);
        }
    }
    report
}

/// Loads the bytes for every image on one page.
///
/// The renderer embeds images as data URIs and never resolves a path or a URL
/// itself, so the bytes have to be in hand before drawing starts.
pub async fn load_page_assets(
    assets: &dyn AssetRepository,
    store: &dyn ObjectStore,
    document: &DesignDocument,
    page_id: PageId,
) -> Result<InMemoryAssets> {
    let page = find_page(document, page_id)?;
    let mut loaded = InMemoryAssets::new();

    for layer in &page.layers {
        let Layer::Image(image) = layer else { continue };
        let asset = assets.get(image.asset_id).await?;
        let bytes = store.get(&asset.storage_key).await?;

        let raster = RasterAsset::from_bytes(bytes).ok_or_else(|| {
            ApplicationError::rejected(
                "INVALID_ASSET",
                format!("asset {} is not a readable image", image.asset_id),
                "Re-upload the asset; the stored bytes are not a PNG, JPEG or WebP.",
            )
        })?;
        loaded.insert(image.asset_id, raster);
    }
    Ok(loaded)
}

/// Renders one page to SVG, and reports what usvg laid out.
pub fn render(
    document: &DesignDocument,
    page_id: PageId,
    brand: Option<&BrandKit>,
    fonts: &FontRegistry,
    assets: &dyn AssetSource,
) -> Result<RenderedPage> {
    let page = find_page(document, page_id)?;
    Ok(render_page(document, page, &palette(brand), fonts, assets)?)
}

/// The largest scale a caller may ask for. Beyond this a single request would
/// allocate hundreds of megabytes of pixels on a machine that is also serving
/// the editor.
pub const MAX_RENDER_SCALE: f32 = 4.0;

/// Renders one page to PNG, reusing a cached entry when there is one.
///
/// A version's document never changes, so a cache hit can never be stale — only
/// unused (plan section 72).
#[allow(clippy::too_many_arguments)]
pub fn render_png(
    cache: &RenderCache,
    design_id: DesignId,
    version: Version,
    document: &DesignDocument,
    page_id: PageId,
    brand: Option<&BrandKit>,
    fonts: &FontRegistry,
    assets: &dyn AssetSource,
    scale: f32,
) -> Result<Vec<u8>> {
    if !scale.is_finite() || scale <= 0.0 || scale > MAX_RENDER_SCALE {
        return Err(ApplicationError::rejected(
            "INVALID_DIMENSION",
            format!("scale {scale} is outside the supported range"),
            "Use a scale greater than 0 and no more than 4.",
        ));
    }

    // Part of the key: a $brand.* token resolves at render time, so a brand
    // edit changes the picture without changing the design's version.
    let brand_version = brand.map(|kit| kit.version.0).unwrap_or(0);

    if let Some(hit) = cache.get(design_id, version, page_id, scale, brand_version) {
        tracing::debug!(
            cache = "hit",
            byte_size = hit.len(),
            "served a cached render"
        );
        return Ok(hit);
    }

    let page = find_page(document, page_id)?;

    // Rendering is the slow part, and a preview that got slower is the kind of
    // regression nobody notices without a number (plan section 60).
    let started = std::time::Instant::now();
    let png = renderer::render_page_png(document, page, &palette(brand), fonts, assets, scale)?;
    let render_ms = started.elapsed().as_millis();

    // Also recorded on the caller's span when it declares the field, so one
    // request line carries the timing rather than two lines needing a join.
    tracing::Span::current().record("render_ms", render_ms as u64);
    tracing::info!(
        cache = "miss",
        render_ms = render_ms as u64,
        scale,
        byte_size = png.len(),
        "rendered a page"
    );

    // A cache that cannot be written is a degraded cache, not a failed render:
    // the caller already holds the bytes it asked for, so this must not become
    // an error response.
    let _ = cache.put(design_id, version, page_id, scale, brand_version, &png);

    Ok(png)
}

/// Validates a design against real measurements from the render authority.
///
/// Every page is rendered, because text overflow cannot be judged without
/// shaped glyph bounds. A page that fails to render contributes no
/// measurements rather than aborting the report: the failure has its own issue
/// already, and an agent fixing a design wants the whole list (plan section 41).
pub async fn validate(
    record: &DesignRecord,
    brand: Option<&BrandKit>,
    fonts: &FontRegistry,
    assets: &dyn AssetRepository,
    store: &dyn ObjectStore,
) -> Result<ValidationReport> {
    let document = &record.document;
    let asset_infos = asset_report(assets, store, document).await;

    let mut measurements: HashMap<LayerId, MeasuredLayer> = HashMap::new();
    for page in &document.pages {
        let Ok(loaded) = load_page_assets(assets, store, document, page.id).await else {
            continue;
        };
        let Ok(rendered) = render(document, page.id, brand, fonts, &loaded) else {
            continue;
        };

        // What is behind each text layer, from the same render authority that
        // shaped it. A page that cannot be sampled contributes no contrast and
        // the check is skipped, exactly like the bounds above.
        let backdrops = renderer::contrast::measure(
            document,
            page,
            &palette(brand),
            fonts,
            &loaded,
            &rendered.inspection,
        )
        .unwrap_or_default();

        for layer in &page.layers {
            if let Some(bounds) = rendered.inspection.layer(layer.id()) {
                measurements.insert(
                    layer.id(),
                    MeasuredLayer {
                        layout: bounds.layout,
                        painted: bounds.painted,
                        contrast: backdrops.get(&layer.id()).map(|c| TextContrast {
                            ratio: c.ratio,
                            illegible_fraction: c.illegible_fraction,
                            delta_e: c.delta_e,
                        }),
                    },
                );
            }
        }
    }

    let mut context = ValidationContext::new(document).with_fonts(fonts.families().to_vec());
    if let Some(brand) = brand {
        context = context.with_brand(brand);
    }
    for (id, info) in asset_infos {
        context = context.with_asset(id, info);
    }
    context.measurements = measurements;

    Ok(validator::validate(&context))
}
