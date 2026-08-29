//! Phase 2 acceptance (plan section 77): an Instagram 4:5 fixture carrying a
//! background, an image, a headline, a subtitle and a CTA must produce correct
//! SVG and PNG.
//!
//! "Correct" is checked against pixels and reported bounds, not just against a
//! successful return.

use std::collections::HashMap;
use std::path::PathBuf;

use design_core::{AssetId, Color, DesignDocument, FocalPoint, ImageFit, Layer, LayerId, Page};
use renderer::{svg::BrandPalette, FontRegistry, InMemoryAssets, RasterAsset};
use resvg::tiny_skia::Pixmap;
use uuid::Uuid;

const FIXTURE: &str = include_str!("../../../fixtures/designs/phase2-acceptance.json");
const HERO_ID: &str = "eeeeeeee-0000-4000-8000-00000000beef";

/// The frame the image layer occupies in the fixture.
const IMAGE_FRAME: (f32, f32, f32, f32) = (0.0, 0.0, 1080.0, 760.0);

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fonts() -> FontRegistry {
    FontRegistry::from_dir(root().join("data/fonts")).expect("fonts")
}

fn hero_bytes() -> Vec<u8> {
    std::fs::read(root().join("fixtures/assets/hero.png")).expect("hero fixture")
}

fn assets() -> InMemoryAssets {
    let mut assets = InMemoryAssets::new();
    assets.insert(
        AssetId(Uuid::parse_str(HERO_ID).unwrap()),
        RasterAsset::from_bytes(hero_bytes()).expect("hero must be a readable raster"),
    );
    assets
}

fn palette() -> BrandPalette {
    HashMap::from([
        ("brand.background".to_string(), Color::rgb(0x10, 0x10, 0x10)),
        ("brand.primary".to_string(), Color::rgb(0xFF, 0x54, 0x36)),
        ("brand.text".to_string(), Color::rgb(0xFF, 0xFF, 0xFF)),
        (
            "brand.textInverse".to_string(),
            Color::rgb(0x10, 0x10, 0x10),
        ),
        ("brand.muted".to_string(), Color::rgb(0xA3, 0xA3, 0xA3)),
    ])
    .into()
}

fn document() -> (DesignDocument, Page) {
    let document = DesignDocument::migrate(serde_json::from_str(FIXTURE).expect("valid JSON"))
        .expect("schema 1");
    let page = document.pages[0].clone();
    (document, page)
}

fn render() -> Pixmap {
    let (document, page) = document();
    let png = renderer::render_page_png(&document, &page, &palette(), &fonts(), &assets(), 1.0)
        .expect("the acceptance fixture must render");
    Pixmap::decode_png(&png).expect("decodable PNG")
}

fn near(actual: Color, expected: Color, tolerance: i32) -> bool {
    let d = |a: u8, b: u8| (a as i32 - b as i32).abs();
    d(actual.r, expected.r) <= tolerance
        && d(actual.g, expected.g) <= tolerance
        && d(actual.b, expected.b) <= tolerance
}

fn pixel(pixmap: &Pixmap, x: u32, y: u32) -> Color {
    let p = pixmap.pixel(x, y).expect("pixel inside the canvas");
    // tiny-skia stores premultiplied alpha; the fixture is fully opaque.
    Color {
        r: p.red(),
        g: p.green(),
        b: p.blue(),
        a: p.alpha(),
    }
}

#[test]
fn renders_the_four_five_canvas() {
    let pixmap = render();
    assert_eq!((pixmap.width(), pixmap.height()), (1080, 1350));
}

#[test]
fn every_layer_is_addressable_after_normalization() {
    let (document, page) = document();
    let rendered =
        renderer::render_page(&document, &page, &palette(), &fonts(), &assets()).unwrap();

    assert!(rendered.inspection.ids_survived_normalization);
    assert_eq!(
        rendered.inspection.layers.len(),
        page.layers.len(),
        "every layer should report bounds: {:?}",
        rendered.inspection.layers
    );

    // Text bounds come from shaped glyphs, so they are tighter than the frames.
    for layer in &page.layers {
        if let Layer::Text(text) = layer {
            let bounds = rendered.inspection.layer(text.common.id).unwrap();
            assert!(
                bounds.layout.width <= text.common.transform.width,
                "{} overflows its frame: {bounds:?}",
                text.common.name
            );
        }
    }
}

#[test]
fn the_image_is_actually_drawn() {
    let pixmap = render();
    let (fx, fy, fw, fh) = IMAGE_FRAME;

    // The source image has an orange dot at its exact center. With `cover` and a
    // centered focal point, that dot must land at the center of the frame.
    let center = pixel(&pixmap, (fx + fw / 2.0) as u32, (fy + fh / 2.0) as u32);
    assert!(
        near(center, Color::rgb(0xFF, 0x54, 0x36), 6),
        "expected the image's centre dot, got {center}"
    );

    // And the frame must be fully covered: no background showing through at the
    // edges, which is what distinguishes cover from contain.
    let background = Color::rgb(0x10, 0x10, 0x10);
    for (x, y) in [(2, 2), (1077, 2), (2, 757), (1077, 757)] {
        let corner = pixel(&pixmap, x, y);
        assert!(
            !near(corner, background, 2),
            "background visible at ({x}, {y}) inside a cover image: {corner}"
        );
    }
}

#[test]
fn cover_crops_according_to_the_focal_point() {
    let (document, mut page) = document();

    // The source marks its top-left corner in red. A centred focal point crops
    // that corner away; pinning the focal point to (0, 0) must bring it back.
    let Layer::Image(image) = &mut page.layers[0] else {
        panic!("layer 0 must be the image");
    };
    image.focal_point = FocalPoint { x: 0.0, y: 0.0 };

    let png =
        renderer::render_page_png(&document, &page, &palette(), &fonts(), &assets(), 1.0).unwrap();
    let pinned = Pixmap::decode_png(&png).unwrap();
    assert!(
        near(pixel(&pinned, 20, 20), Color::rgb(0xFF, 0x00, 0x00), 6),
        "focal (0,0) should expose the source's red corner, got {}",
        pixel(&pinned, 20, 20)
    );

    let centred = render();
    assert!(
        !near(pixel(&centred, 20, 20), Color::rgb(0xFF, 0x00, 0x00), 6),
        "a centred focal point should have cropped the red corner away"
    );
}

#[test]
fn contain_shows_the_whole_image_and_leaves_the_background_visible() {
    let (document, mut page) = document();
    let Layer::Image(image) = &mut page.layers[0] else {
        panic!("layer 0 must be the image");
    };
    image.fit = ImageFit::Contain;

    let png =
        renderer::render_page_png(&document, &page, &palette(), &fonts(), &assets(), 1.0).unwrap();
    let pixmap = Pixmap::decode_png(&png).unwrap();

    // 1600x900 letterboxed into a 1080x760 frame scales by 0.675, giving a
    // 1080x607 image centred vertically: bands of page background above and below.
    let background = Color::rgb(0x10, 0x10, 0x10);
    assert!(
        near(pixel(&pixmap, 540, 5), background, 2),
        "contain should letterbox, got {}",
        pixel(&pixmap, 540, 5)
    );
    // Both source corners survive, because nothing was cropped. The markers are
    // 120px in the source, so ~81px tall here, starting at y = 76.
    assert!(near(
        pixel(&pixmap, 8, 120),
        Color::rgb(0xFF, 0x00, 0x00),
        8
    ));
    assert!(near(
        pixel(&pixmap, 1072, 120),
        Color::rgb(0x00, 0xFF, 0x00),
        8
    ));
}

#[test]
fn text_and_shapes_paint_over_the_background() {
    let pixmap = render();

    // The CTA pill resolved $brand.primary.
    let pill = pixel(&pixmap, 270, 1252);
    assert!(
        near(pill, Color::rgb(0xFF, 0x54, 0x36), 4),
        "CTA pill: {pill}"
    );

    // The headline painted white glyphs somewhere in its band.
    let headline_painted = (860..960).any(|y| {
        (80..1000).any(|x| {
            let p = pixel(&pixmap, x, y);
            p.r > 200 && p.g > 200 && p.b > 200
        })
    });
    assert!(headline_painted, "no headline glyphs found");

    // The CTA label is textInverse, so dark glyphs must sit on the orange pill.
    let cta_dark = (1235..1275).any(|y| {
        (100..440).any(|x| {
            let p = pixel(&pixmap, x, y);
            p.r < 60 && p.g < 60 && p.b < 60
        })
    });
    assert!(cta_dark, "no dark CTA glyphs found on the pill");
}

#[test]
fn the_normalized_svg_embeds_the_image_and_flattens_the_text() {
    let (document, page) = document();
    let rendered =
        renderer::render_page(&document, &page, &palette(), &fonts(), &assets()).unwrap();

    assert!(!rendered.normalized_svg.contains("<text"));
    // The bytes travel inside the document; nothing is fetched at render time.
    assert!(rendered.authored_svg.contains("data:image/png;base64,"));
    // Only the SVG namespace declarations may mention a URL: no href may point
    // anywhere remote (plan sections 21 and 45).
    for href in ["xlink:href=\"http", "href=\"http", "url(http"] {
        assert!(
            !rendered.authored_svg.contains(href),
            "authored SVG references something remote via {href}"
        );
    }
}

#[test]
fn a_missing_asset_is_an_error_not_a_blank_space() {
    let (document, page) = document();
    let error = renderer::render_page_png(
        &document,
        &page,
        &palette(),
        &fonts(),
        &renderer::NoAssets,
        1.0,
    )
    .unwrap_err();

    assert!(error.code() == "ASSET_NOT_FOUND", "got: {error}");
}

#[test]
fn rendering_the_fixture_is_deterministic() {
    let (document, page) = document();
    let a =
        renderer::render_page_png(&document, &page, &palette(), &fonts(), &assets(), 1.0).unwrap();
    let b =
        renderer::render_page_png(&document, &page, &palette(), &fonts(), &assets(), 1.0).unwrap();
    assert_eq!(a, b);
}

#[test]
fn scale_multiplies_the_raster_without_changing_the_document() {
    let (document, page) = document();
    let png =
        renderer::render_page_png(&document, &page, &palette(), &fonts(), &assets(), 2.0).unwrap();
    let pixmap = Pixmap::decode_png(&png).unwrap();
    assert_eq!((pixmap.width(), pixmap.height()), (2160, 2700));
}

/// Not part of the acceptance, but the layer id has to survive far enough for
/// the editor to hit-test it later.
#[test]
fn image_bounds_match_the_frame_it_was_given() {
    let (document, page) = document();
    let rendered =
        renderer::render_page(&document, &page, &palette(), &fonts(), &assets()).unwrap();

    let image_id = LayerId(Uuid::parse_str("dddddddd-0000-4000-8000-000000000001").unwrap());
    let bounds = rendered.inspection.layer(image_id).expect("image bounds");
    let (fx, fy, fw, fh) = IMAGE_FRAME;

    // The visible bounds are the frame, because cover clips to it.
    assert!((bounds.painted.x - fx).abs() < 1.0, "{bounds:?}");
    assert!((bounds.painted.y - fy).abs() < 1.0, "{bounds:?}");
    assert!((bounds.painted.width - fw).abs() < 1.0, "{bounds:?}");
    assert!((bounds.painted.height - fh).abs() < 1.0, "{bounds:?}");

    // The layout box is wider, because a cover image is meant to overflow.
    // A validator must not read this as the image escaping the canvas.
    assert!(bounds.layout.width > bounds.painted.width, "{bounds:?}");
    assert!(bounds.layout.x < 0.0, "{bounds:?}");
}

/// A `cover` image must not paint outside its frame. The acceptance fixture
/// cannot show this because its image is full-bleed, so the overflow would fall
/// off the canvas anyway; this narrows the frame so the leak becomes visible.
#[test]
fn cover_does_not_paint_outside_its_frame() {
    let (document, mut page) = document();
    let Layer::Image(image) = &mut page.layers[0] else {
        panic!("layer 0 must be the image");
    };
    image.common.transform = design_core::Transform::new(340.0, 200.0, 400.0, 400.0);

    let png =
        renderer::render_page_png(&document, &page, &palette(), &fonts(), &assets(), 1.0).unwrap();
    let pixmap = Pixmap::decode_png(&png).unwrap();
    let background = Color::rgb(0x10, 0x10, 0x10);

    // Just left and right of the frame, at its vertical middle.
    for x in [300, 800] {
        let outside = pixel(&pixmap, x, 400);
        assert!(
            near(outside, background, 2),
            "cover leaked outside its frame at x={x}: {outside}"
        );
    }
}
