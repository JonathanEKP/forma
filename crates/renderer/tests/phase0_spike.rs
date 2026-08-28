//! Phase 0 blocking spike (plan section 36).
//!
//! Each test here maps to one of the nine points the plan requires before any
//! editor work starts. If these stop passing, the architecture assumption that
//! Rust is the render authority no longer holds.

use std::collections::HashMap;
use std::path::PathBuf;

use design_core::{
    Color, DesignColor, DesignDocument, DesignFormat, FormatPreset, Layer, LayerCommon, LayerId,
    LayerRole, Page, ShapeKind, ShapeLayer, TextAlign, TextLayer, Transform,
};
use renderer::{flatten, fonts::FontRegistry, svg, NoAssets};

fn font_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/fonts")
}

fn registry() -> FontRegistry {
    FontRegistry::from_dir(font_dir()).expect("data/fonts must be loadable")
}

fn palette() -> svg::BrandPalette {
    HashMap::from([
        ("brand.background".to_string(), Color::rgb(0x10, 0x10, 0x10)),
        ("brand.primary".to_string(), Color::rgb(0xFF, 0x54, 0x36)),
        ("brand.text".to_string(), Color::rgb(0xFF, 0xFF, 0xFF)),
    ])
}

fn common(name: &str, role: LayerRole, transform: Transform) -> LayerCommon {
    LayerCommon {
        id: LayerId::new(),
        name: name.to_string(),
        role: Some(role),
        visible: true,
        locked: false,
        opacity: 1.0,
        transform,
    }
}

/// The fixture from the plan's Phase 2 acceptance, minus the image layer, which
/// needs the assets crate.
fn fixture() -> (DesignDocument, Page) {
    let document = DesignDocument::new(DesignFormat::from_preset(FormatPreset::InstagramPortrait));

    let mut page = Page::new("Slide 1");
    page.background = Some(DesignColor::token("brand.background"));

    page.layers.push(Layer::Shape(ShapeLayer {
        common: common(
            "Accent bar",
            LayerRole::Decoration,
            Transform::new(80.0, 980.0, 240.0, 12.0),
        ),
        kind: ShapeKind::Rectangle,
        fill: Some(DesignColor::token("brand.primary")),
        stroke: None,
        stroke_width: 0.0,
        corner_radius: 6.0,
    }));

    page.layers.push(Layer::Text(TextLayer {
        common: common(
            "Headline",
            LayerRole::Headline,
            Transform::new(80.0, 160.0, 920.0, 400.0),
        ),
        // Plan section 18: explicit line breaks, no implicit wrapping in V1.
        text: "BUILD\nWITH AI".to_string(),
        font_family: "DejaVu Sans".to_string(),
        font_size: 140.0,
        font_weight: 700,
        line_height: 1.05,
        letter_spacing: 0.0,
        align: TextAlign::Left,
        color: DesignColor::token("brand.text"),
    }));

    page.layers.push(Layer::Text(TextLayer {
        common: common(
            "Subtitle",
            LayerRole::Body,
            Transform::new(80.0, 620.0, 920.0, 200.0),
        ),
        text: "A local design runtime\nfor agents.".to_string(),
        font_family: "DejaVu Sans".to_string(),
        font_size: 48.0,
        font_weight: 400,
        line_height: 1.3,
        letter_spacing: 0.0,
        align: TextAlign::Left,
        color: DesignColor::token("brand.text"),
    }));

    (document, page)
}

/// Point 1: load a font from data/fonts, and only from there.
#[test]
fn loads_fonts_from_the_repo_directory() {
    let fonts = registry();
    assert!(
        fonts.contains("DejaVu Sans"),
        "families: {:?}",
        fonts.families()
    );
    // A family installed on the host but absent from data/fonts must not resolve.
    assert!(!fonts.contains("Liberation Sans"));
}

/// Points 2 and 3: author an SVG with text and let usvg process it.
#[test]
fn authors_and_parses_an_svg_with_text() {
    let (document, page) = fixture();
    let fonts = registry();

    let authored = svg::page_to_svg(&document, &page, &palette(), &NoAssets).unwrap();
    assert!(authored.contains("<text"));
    assert!(authored.contains("BUILD"));

    let tree = flatten::parse(&authored, &fonts).unwrap();
    assert_eq!(tree.size().width(), 1080.0);
    assert_eq!(tree.size().height(), 1350.0);
}

/// Points 4 and 9: real bounding boxes, and whether layer ids survive usvg.
#[test]
fn reports_real_text_bounds_and_keeps_layer_ids() {
    let (document, page) = fixture();
    let fonts = registry();
    let rendered = renderer::render_page(&document, &page, &palette(), &fonts, &NoAssets).unwrap();

    assert!(
        rendered.inspection.ids_survived_normalization,
        "layer groups did not survive usvg: {:?}",
        rendered.inspection.layers
    );

    let headline = match &page.layers[1] {
        Layer::Text(t) => t,
        _ => unreachable!(),
    };
    let bounds = rendered
        .inspection
        .layer(headline.common.id)
        .expect("headline must be addressable by its layer id");

    // Two explicit lines at 140px must occupy real vertical space, not a stub.
    assert!(
        bounds.layout.height > 140.0,
        "headline bounds too short: {bounds:?}"
    );
    assert!(
        bounds.layout.width > 100.0,
        "headline bounds too narrow: {bounds:?}"
    );
    // These are shaped-glyph bounds, so they must be tighter than the layer frame.
    assert!(bounds.layout.width < headline.common.transform.width);
    // The two rectangles are not interchangeable even with no clipping: the
    // layout box spans the line boxes, the painted box hugs the glyph outlines.
    assert!(bounds.layout.height > bounds.painted.height, "{bounds:?}");
    assert!(bounds.painted.y > bounds.layout.y, "{bounds:?}");
}

/// Point 5: text is serialized as paths, which is what frees the editor from
/// browser font layout.
#[test]
fn flattens_text_to_paths() {
    let (document, page) = fixture();
    let fonts = registry();
    let rendered = renderer::render_page(&document, &page, &palette(), &fonts, &NoAssets).unwrap();

    assert!(
        !rendered.normalized_svg.contains("<text"),
        "normalized SVG still carries live text"
    );
    assert!(rendered.normalized_svg.contains("<path"));
    assert!(rendered.normalized_svg.contains("layer-"));
}

/// Points 6, 7 and 8: rasterize the same tree and get a real PNG whose pixels
/// match what the SVG describes.
#[test]
fn rasterizes_to_png_matching_the_svg() {
    let (document, page) = fixture();
    let fonts = registry();
    let png =
        renderer::render_page_png(&document, &page, &palette(), &fonts, &NoAssets, 1.0).unwrap();

    assert_eq!(&png[1..4], b"PNG", "not a PNG signature");

    let decoded = resvg::tiny_skia::Pixmap::decode_png(&png).unwrap();
    assert_eq!(decoded.width(), 1080);
    assert_eq!(decoded.height(), 1350);

    // The background token resolved to #101010 everywhere the design is empty.
    let corner = decoded.pixel(5, 5).unwrap();
    assert_eq!(
        (corner.red(), corner.green(), corner.blue()),
        (0x10, 0x10, 0x10)
    );

    // The accent bar resolved to $brand.primary and landed where it was placed.
    let bar = decoded.pixel(200, 986).unwrap();
    assert_eq!((bar.red(), bar.green(), bar.blue()), (0xFF, 0x54, 0x36));

    // Point 8 proper: the bounds usvg reported must be where the glyphs actually
    // landed in the raster. Scanning a hardcoded band would only test the test.
    let rendered = renderer::render_page(&document, &page, &palette(), &fonts, &NoAssets).unwrap();
    let headline = match &page.layers[1] {
        Layer::Text(t) => t,
        _ => unreachable!(),
    };
    let bounds = rendered.inspection.layer(headline.common.id).unwrap();

    let is_glyph = |x: u32, y: u32| {
        let p = decoded.pixel(x, y).unwrap();
        p.red() > 200 && p.green() > 200 && p.blue() > 200
    };

    let inside = (bounds.layout.y as u32..bounds.layout.bottom() as u32)
        .any(|y| (bounds.layout.x as u32..bounds.layout.right() as u32).any(|x| is_glyph(x, y)));
    assert!(
        inside,
        "no glyph pixels inside the reported bounds {bounds:?}"
    );

    // And nothing painted well above them, so the bounds are not merely permissive.
    let above = (10..(bounds.layout.y as u32).saturating_sub(10))
        .any(|y| (0..1080).any(|x| is_glyph(x, y)));
    assert!(
        !above,
        "glyphs painted above the reported bounds {bounds:?}"
    );
}

/// Determinism: the same document must always produce byte-identical output,
/// otherwise the golden tests of plan section 63 are impossible.
#[test]
fn rendering_is_deterministic() {
    let (document, page) = fixture();
    let fonts = registry();
    let a =
        renderer::render_page_png(&document, &page, &palette(), &fonts, &NoAssets, 1.0).unwrap();
    let b =
        renderer::render_page_png(&document, &page, &palette(), &fonts, &NoAssets, 1.0).unwrap();
    assert_eq!(a, b);
}

/// Plan section 42: an unknown brand token is an error, never a fallback color.
#[test]
fn refuses_an_unknown_brand_token() {
    let (document, mut page) = fixture();
    page.background = Some(DesignColor::token("brand.does_not_exist"));

    let err = svg::page_to_svg(&document, &page, &palette(), &NoAssets).unwrap_err();
    assert!(
        err.to_string().contains("UNKNOWN_BRAND_TOKEN"),
        "got: {err}"
    );
}
