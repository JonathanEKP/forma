//! Measuring whether text can be read against what is drawn under it.
//!
//! The case that made this exist: a footer set in a mid grey over the mid-grey
//! wet asphalt of a photograph. It validated clean under all seventeen other
//! codes, survived an agent that looked at its own render, and was caught by a
//! person's eye.

use design_core::{
    Color, DesignColor, DesignDocument, DesignFormat, FormatPreset, Layer, LayerCommon, LayerId,
    Page, ShapeKind, ShapeLayer, TextAlign, TextLayer, Transform,
};
use renderer::{contrast, fonts::FontRegistry, render_page, svg::BrandPalette, NoAssets};

fn fonts() -> FontRegistry {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/fonts");
    FontRegistry::from_dir(dir).expect("the shipped fonts must load")
}

fn backdrop_at(fill: Color, x: f32, width: f32) -> Layer {
    Layer::Shape(ShapeLayer {
        common: LayerCommon {
            id: LayerId::new(),
            name: "Backdrop".into(),
            role: None,
            visible: true,
            locked: false,
            opacity: 1.0,
            transform: Transform::new(x, 0.0, width, 1080.0),
        },
        kind: ShapeKind::Rectangle,
        fill: Some(DesignColor::Literal(fill)),
        stroke: None,
        stroke_width: 0.0,
        corner_radius: 0.0,
    })
}

fn line(ink: Color) -> TextLayer {
    TextLayer {
        common: LayerCommon {
            id: LayerId::new(),
            name: "Footer".into(),
            role: None,
            visible: true,
            locked: false,
            opacity: 1.0,
            transform: Transform::new(80.0, 460.0, 900.0, 90.0),
        },
        text: "CRÓNICA PORTEÑA".into(),
        font_family: "Geist".into(),
        font_size: 56.0,
        font_weight: 600,
        line_height: 1.2,
        letter_spacing: 1.0,
        align: TextAlign::Left,
        color: DesignColor::Literal(ink),
    }
}

/// A page covering the whole canvas in one tone.
fn measure(ink: Color, fill: Color) -> contrast::TextContrast {
    measure_over(ink, vec![backdrop_at(fill, 0.0, 1080.0)])
}

/// Returns the contrast measured for the single text layer drawn over
/// `backdrops`.
fn measure_over(ink: Color, backdrops: Vec<Layer>) -> contrast::TextContrast {
    let document = DesignDocument::new(DesignFormat::from_preset(FormatPreset::Square));
    let mut page = Page::new("Sample");
    page.layers.extend(backdrops);
    let text = line(ink);
    let id = text.common.id;
    page.layers.push(Layer::Text(text));

    let palette = BrandPalette::default();
    let fonts = fonts();
    let rendered =
        render_page(&document, &page, &palette, &fonts, &NoAssets).expect("the sample must render");
    let measured = contrast::measure(
        &document,
        &page,
        &palette,
        &fonts,
        &NoAssets,
        &rendered.inspection,
    )
    .expect("the backdrop must be sampleable");

    *measured.get(&id).expect("the text layer must be measured")
}

#[test]
fn near_white_on_near_black_reads() {
    let measured = measure(Color::rgb(242, 239, 233), Color::rgb(20, 21, 26));
    assert!(
        measured.ratio > contrast::READABLE_RATIO,
        "expected a comfortable ratio, got {}",
        measured.ratio
    );
    assert_eq!(measured.illegible_fraction, 0.0);
}

/// The reported failure. `$brand.muted` is defined to sit on `$brand.background`,
/// where it has plenty of contrast; over a photograph of the same tone it has
/// none, and nothing said so.
#[test]
fn a_mid_grey_on_a_mid_grey_backdrop_is_reported() {
    let measured = measure(Color::rgb(138, 139, 135), Color::rgb(104, 108, 112));
    assert!(
        measured.ratio < contrast::READABLE_RATIO,
        "a mid grey on a mid grey should not pass, got {}",
        measured.ratio
    );
    assert!(
        measured.delta_e < contrast::DISTINCT_DELTA_E,
        "and the hue does not rescue it either: {}",
        measured.delta_e
    );
}

/// The accent red on wet asphalt, which is why a luminance ratio cannot be the
/// whole answer. It is nearly the same lightness as its backdrop and perfectly
/// readable, and a check that condemned it would send an agent to fix the one
/// element on the cover that worked.
#[test]
fn a_hue_break_counts_as_contrast() {
    let red_on_asphalt = measure(Color::rgb(217, 58, 34), Color::rgb(104, 108, 112));
    assert!(
        red_on_asphalt.ratio < contrast::READABLE_RATIO,
        "the lightnesses really are close: {}",
        red_on_asphalt.ratio
    );
    assert!(
        red_on_asphalt.delta_e > contrast::DISTINCT_DELTA_E,
        "but the colours are plainly different: {}",
        red_on_asphalt.delta_e
    );
    assert!(
        red_on_asphalt.illegible_fraction < 0.05,
        "so almost none of it should count as illegible: {}",
        red_on_asphalt.illegible_fraction
    );
}

/// A line half on a dark road and half on a bright crosswalk stripe can average
/// out to a passing ratio while being unreadable across most of itself.
#[test]
fn a_line_that_averages_well_but_fails_over_half_its_area_is_caught() {
    // Near-white text over a road that turns into a crosswalk stripe partway.
    let measured = measure_over(
        Color::rgb(242, 239, 233),
        vec![
            backdrop_at(Color::rgb(20, 21, 26), 0.0, 300.0),
            backdrop_at(Color::rgb(236, 236, 232), 300.0, 780.0),
        ],
    );
    assert!(
        measured.illegible_fraction > 0.5,
        "most of the line lies on the bright stripe: {}",
        measured.illegible_fraction
    );
}
