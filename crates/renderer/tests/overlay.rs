//! The tint that belongs to a photograph (agreed order, item 7).
//!
//! A `ShapeLayer` was the only way to darken a photo, and a flat rectangle is
//! opaque to its own edges, so it lays a visible seam across the picture — the
//! most common way an otherwise good piece announces that a machine made it. An
//! overlay anchored to an edge fades to nothing, and there is no edge to see.

use std::collections::HashMap;

use design_core::{
    AssetId, Color, DesignColor, DesignDocument, DesignFormat, FormatPreset, ImageFit, ImageLayer,
    ImageOverlay, Layer, LayerCommon, LayerId, OverlayEdge, Page, Transform,
};
use renderer::{render_page_png, svg::BrandPalette, FontRegistry, InMemoryAssets, RasterAsset};
use resvg::tiny_skia::Pixmap;

const CANVAS: f32 = 600.0;

fn root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fonts() -> FontRegistry {
    FontRegistry::from_dir(root().join("data/fonts")).expect("fonts")
}

/// One page holding a single photograph that fills it, with `overlay` applied.
fn render(overlay: Option<ImageOverlay>) -> Pixmap {
    let asset_id = AssetId::new();
    let document = DesignDocument::new(DesignFormat {
        preset: FormatPreset::Custom,
        width: CANVAS,
        height: CANVAS,
    });
    let mut page = Page::new("Photo");
    page.layers.push(Layer::Image(ImageLayer {
        common: LayerCommon {
            id: LayerId::new(),
            name: "Photo".to_string(),
            role: None,
            visible: true,
            locked: false,
            opacity: 1.0,
            transform: Transform::new(0.0, 0.0, CANVAS, CANVAS),
        },
        asset_id,
        fit: ImageFit::Cover,
        focal_point: Default::default(),
        overlay,
    }));

    let bytes = std::fs::read(root().join("fixtures/assets/hero.png")).expect("hero fixture");
    let mut assets = InMemoryAssets::new();
    assets.insert(
        asset_id,
        RasterAsset::from_bytes(bytes).expect("readable raster"),
    );

    let palette = BrandPalette::from_colors(HashMap::from([(
        "brand.background".to_string(),
        Color::rgb(0, 0, 0),
    )]));

    let png = render_page_png(&document, &page, &palette, &fonts(), &assets, 1.0)
        .expect("the page must render");
    Pixmap::decode_png(&png).expect("decodable PNG")
}

/// Mean luminance of one horizontal band, as a fraction of full brightness.
fn brightness(pixmap: &Pixmap, from: f32, to: f32) -> f32 {
    let (top, bottom) = (
        (from * pixmap.height() as f32) as u32,
        (to * pixmap.height() as f32) as u32,
    );
    let mut total = 0.0;
    let mut count = 0.0;
    for y in top..bottom {
        for x in 0..pixmap.width() {
            let Some(pixel) = pixmap.pixel(x, y) else {
                continue;
            };
            let pixel = pixel.demultiply();
            total += (pixel.red() as f32 + pixel.green() as f32 + pixel.blue() as f32) / 3.0;
            count += 1.0;
        }
    }
    total / count / 255.0
}

fn tint(opacity: f32, from: Option<OverlayEdge>) -> Option<ImageOverlay> {
    Some(ImageOverlay {
        color: DesignColor::token("brand.background"),
        opacity,
        from,
    })
}

#[test]
fn an_overlay_with_no_edge_darkens_the_whole_photograph() {
    let plain = render(None);
    let tinted = render(tint(0.5, None));

    for (from, to) in [(0.0, 0.25), (0.4, 0.6), (0.75, 1.0)] {
        let before = brightness(&plain, from, to);
        let after = brightness(&tinted, from, to);
        assert!(
            after < before * 0.75,
            "the band {from}..{to} should be evenly darker: {before} -> {after}"
        );
    }
}

/// The reason this exists rather than a rectangle: the far edge has to come out
/// untouched, or it is a band with a different name.
#[test]
fn an_anchored_overlay_fades_to_nothing_at_the_far_edge() {
    let plain = render(None);
    let tinted = render(tint(0.85, Some(OverlayEdge::Bottom)));

    // The last tenth still carries a few percent of the ramp -- it spans offsets
    // 0.9 to 1.0, not the endpoint alone -- and that residue is exactly what
    // makes the tint seamless. So "untouched" is a share of the original rather
    // than an absolute difference, which would depend on how bright the photo
    // happens to be there.
    let top_before = brightness(&plain, 0.0, 0.1);
    let top_after = brightness(&tinted, 0.0, 0.1);
    assert!(
        top_after > top_before * 0.95,
        "the far edge should keep its brightness: {top_before} -> {top_after}"
    );

    let bottom_before = brightness(&plain, 0.9, 1.0);
    let bottom_after = brightness(&tinted, 0.9, 1.0);
    assert!(
        bottom_after < bottom_before * 0.5,
        "the bottom should carry the tint: {bottom_before} -> {bottom_after}"
    );

    // And it has to be a ramp, not two flat halves with a step between them.
    let quarters: Vec<f32> = [(0.0, 0.25), (0.25, 0.5), (0.5, 0.75), (0.75, 1.0)]
        .iter()
        .map(|(a, b)| brightness(&tinted, *a, *b) / brightness(&plain, *a, *b))
        .collect();
    assert!(
        quarters.windows(2).all(|pair| pair[0] > pair[1]),
        "the tint should deepen monotonically towards the anchored edge: {quarters:?}"
    );
}

#[test]
fn each_edge_anchors_where_it_says() {
    let plain = render(None);
    for (edge, light, dark) in [
        (OverlayEdge::Top, (0.9, 1.0), (0.0, 0.1)),
        (OverlayEdge::Bottom, (0.0, 0.1), (0.9, 1.0)),
    ] {
        let tinted = render(tint(0.85, Some(edge)));
        assert!(
            brightness(&tinted, light.0, light.1) > brightness(&tinted, dark.0, dark.1),
            "{edge:?} darkened the wrong end"
        );
        let before = brightness(&plain, light.0, light.1);
        let after = brightness(&tinted, light.0, light.1);
        assert!(
            after > before * 0.95,
            "{edge:?} should leave the far edge alone: {before} -> {after}"
        );
    }
}
