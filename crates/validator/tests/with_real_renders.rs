//! The validator against real measurements from the render authority.
//!
//! The unit tests in `codes.rs` feed hand-written numbers, which proves the
//! rules. This proves the rules hold against what the renderer actually
//! produces, which is where a wrong assumption would show up.

use std::collections::HashMap;
use std::path::PathBuf;

use design_core::{
    AssetId, BrandColors, BrandId, BrandKit, BrandRules, BrandTypography, Color, DesignDocument,
    Layer, Version,
};
use renderer::{FontRegistry, InMemoryAssets, RasterAsset};
use uuid::Uuid;
use validator::{validate, AssetInfo, MeasuredLayer, Severity, ValidationContext};

const FIXTURE: &str = include_str!("../../../fixtures/designs/phase2-acceptance.json");
const HERO_ID: &str = "eeeeeeee-0000-4000-8000-00000000beef";

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn brand() -> BrandKit {
    BrandKit {
        id: BrandId::new(),
        project_id: design_core::ProjectId::new(),
        name: "Default".to_string(),
        colors: BrandColors {
            primary: Color::rgb(0xFF, 0x54, 0x36),
            secondary: Color::rgb(0xFD, 0xBA, 0x2D),
            accent: Color::rgb(0x7C, 0x3A, 0xED),
            background: Color::rgb(0x10, 0x10, 0x10),
            surface: Color::rgb(0x19, 0x19, 0x19),
            text: Color::rgb(0xFF, 0xFF, 0xFF),
            text_inverse: Color::rgb(0x10, 0x10, 0x10),
            muted: Color::rgb(0xA3, 0xA3, 0xA3),
        },
        typography: BrandTypography {
            display: "DejaVu Sans".to_string(),
            body: "DejaVu Sans".to_string(),
            mono: "DejaVu Sans".to_string(),
        },
        logo_asset_id: None,
        voice: String::new(),
        rules: BrandRules {
            safe_margin: 80.0,
            minimum_font_size: 24.0,
        },
        version: Version::INITIAL,
    }
}

fn hero_id() -> AssetId {
    AssetId(Uuid::parse_str(HERO_ID).unwrap())
}

fn assets() -> InMemoryAssets {
    let mut assets = InMemoryAssets::new();
    assets.insert(
        hero_id(),
        RasterAsset::from_bytes(std::fs::read(root().join("fixtures/assets/hero.png")).unwrap())
            .unwrap(),
    );
    assets
}

/// Renders the document and returns what the renderer measured, keyed by layer.
fn measure(document: &DesignDocument) -> HashMap<design_core::LayerId, MeasuredLayer> {
    let fonts = FontRegistry::from_dir(root().join("data/fonts")).unwrap();
    let brand = brand();
    let palette = brand.color_palette();
    let page = &document.pages[0];

    let rendered =
        renderer::render_page(document, page, &palette, &fonts, &assets()).expect("render");

    page.layers
        .iter()
        .filter_map(|layer| {
            let bounds = rendered.inspection.layer(layer.id())?;
            Some((
                layer.id(),
                MeasuredLayer {
                    layout: bounds.layout,
                    painted: bounds.painted,
                },
            ))
        })
        .collect()
}

fn fixture() -> DesignDocument {
    DesignDocument::migrate(serde_json::from_str(FIXTURE).unwrap()).unwrap()
}

#[test]
fn the_acceptance_fixture_validates_clean() {
    let document = fixture();
    let brand = brand();

    let mut context = ValidationContext::new(&document)
        .with_brand(&brand)
        .with_fonts(["DejaVu Sans"])
        .with_asset(hero_id(), AssetInfo::usable());
    context.measurements = measure(&document);

    let report = validate(&context);
    assert!(
        report.valid,
        "the fixture should export cleanly: {:?}",
        report.errors().collect::<Vec<_>>()
    );
}

/// The payoff of measuring instead of guessing: shrinking the box has to be
/// caught, and only because the renderer told us how wide the glyphs are.
#[test]
fn a_shrunk_text_box_overflows_against_real_glyph_bounds() {
    let mut document = fixture();
    let brand = brand();

    let Layer::Text(headline) = &mut document.pages[0].layers[1] else {
        panic!("layer 1 must be the headline");
    };
    headline.common.transform.width = 300.0;
    let headline_id = headline.common.id;

    let mut context = ValidationContext::new(&document)
        .with_brand(&brand)
        .with_fonts(["DejaVu Sans"])
        .with_asset(hero_id(), AssetInfo::usable());
    context.measurements = measure(&document);

    let report = validate(&context);
    let issue = report
        .issues
        .iter()
        .find(|i| i.code == "TEXT_OVERFLOW_WIDTH")
        .expect("expected the overflow to be caught");

    assert_eq!(issue.severity, Severity::Error);
    assert_eq!(issue.layer_id, Some(headline_id));
    assert!(!report.valid);

    // The measurement is real, not the frame echoed back.
    let measured = issue.details.as_ref().unwrap()["measured"]
        .as_f64()
        .unwrap();
    assert!(measured > 300.0, "measured {measured}");
}

/// A full-bleed cover image overflows its frame by design. Judging it by its
/// layout box would flag every such photo; the painted box is what counts.
#[test]
fn a_full_bleed_cover_image_is_not_reported_as_leaving_the_canvas() {
    let document = fixture();
    let brand = brand();
    let measurements = measure(&document);

    let image_id = document.pages[0].layers[0].id();
    let measured = measurements[&image_id];
    // Precondition: the two rectangles really do disagree here.
    assert!(
        measured.layout.x < 0.0 && measured.painted.x >= 0.0,
        "expected an overflowing layout box: {measured:?}"
    );

    let mut context = ValidationContext::new(&document)
        .with_brand(&brand)
        .with_fonts(["DejaVu Sans"])
        .with_asset(hero_id(), AssetInfo::usable());
    context.measurements = measurements;

    let report = validate(&context);
    assert!(
        !report.has("OUTSIDE_CANVAS"),
        "cover overflow was misreported: {:?}",
        report.issues
    );
}
