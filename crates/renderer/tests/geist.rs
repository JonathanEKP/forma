//! Geist is loadable by the renderer (plan sections 34 and 93.2).
//!
//! Adding a font file to `data/fonts` is not enough on its own: usvg silently
//! falls back to another family when it cannot resolve the one a `TextLayer`
//! asks for, so a design would still render — just not in the font it named.
//! These tests measure real glyph bounds to tell resolution apart from
//! fallback.

use std::collections::HashMap;
use std::path::PathBuf;

use design_core::{
    Color, DesignColor, DesignDocument, DesignFormat, FormatPreset, Layer, LayerCommon, LayerId,
    LayerRole, Page, Rect, TextAlign, TextLayer, Transform,
};
use renderer::{fonts::FontRegistry, render_page, svg::BrandPalette, NoAssets};

const SAMPLE: &str = "Structure under\ncontrolled transformation";

fn registry() -> FontRegistry {
    FontRegistry::from_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/fonts"))
        .expect("data/fonts must be loadable")
}

fn palette() -> BrandPalette {
    HashMap::from([("brand.text".to_string(), Color::rgb(0xF4, 0xF1, 0xEA))])
}

/// Lays out one text layer alone on a page and returns the box usvg gave it.
fn measure(family: &str, weight: u16) -> Rect {
    let document = DesignDocument::new(DesignFormat::from_preset(FormatPreset::InstagramPortrait));
    let mut page = Page::new("Sample");
    let id = LayerId::new();

    page.layers.push(Layer::Text(TextLayer {
        common: LayerCommon {
            id,
            name: "Sample".to_string(),
            role: Some(LayerRole::Headline),
            visible: true,
            locked: false,
            opacity: 1.0,
            transform: Transform::new(80.0, 160.0, 920.0, 400.0),
        },
        text: SAMPLE.to_string(),
        font_family: family.to_string(),
        font_size: 72.0,
        font_weight: weight,
        line_height: 1.1,
        letter_spacing: 0.0,
        align: TextAlign::Left,
        color: DesignColor::token("brand.text"),
    }));

    let rendered = render_page(&document, &page, &palette(), &registry(), &NoAssets)
        .expect("the sample must render");
    rendered.inspection.layer(id).expect("text bounds").layout
}

#[test]
fn the_registry_exposes_both_geist_families() {
    let fonts = registry();
    for family in ["Geist", "Geist Mono"] {
        assert!(
            fonts.contains(family),
            "{family} is missing; families: {:?}",
            fonts.families()
        );
    }
    // Adding Geist must not have displaced the default BrandKit typography.
    assert!(fonts.contains("DejaVu Sans"), "{:?}", fonts.families());
}

#[test]
fn geist_is_resolved_rather_than_quietly_falling_back() {
    let geist = measure("Geist", 400);
    let dejavu = measure("DejaVu Sans", 400);

    // Identical boxes would mean usvg drew DejaVu for both and the Geist file
    // was never reached.
    assert!(
        (geist.width - dejavu.width).abs() > 1.0,
        "Geist laid out exactly like DejaVu, so it was not resolved: \
         geist={geist:?} dejavu={dejavu:?}"
    );
    assert!(geist.width > 0.0 && geist.height > 0.0, "{geist:?}");
}

#[test]
fn the_shipped_weights_are_selected_individually() {
    let regular = measure("Geist", 400);
    let bold = measure("Geist", 700);

    // One weight file standing in for all four would report the same advance
    // widths at 400 and 700.
    assert!(
        (bold.width - regular.width).abs() > 1.0,
        "400 and 700 laid out identically, so the weight files are not being \
         picked apart: regular={regular:?} bold={bold:?}"
    );
}

#[test]
fn geist_mono_advances_uniformly() {
    let fonts = registry();
    assert!(fonts.contains("Geist Mono"), "{:?}", fonts.families());

    // A monospaced face gives these two equal-length strings the same width;
    // a proportional fallback would not.
    let narrow = measure_text("Geist Mono", "iiiiiiiiii");
    let wide = measure_text("Geist Mono", "WWWWWWWWWW");
    assert!(
        (narrow.width - wide.width).abs() < 1.0,
        "Geist Mono did not advance uniformly, so a proportional face was \
         substituted: narrow={narrow:?} wide={wide:?}"
    );
}

/// Same as `measure`, with the sample string chosen by the caller.
fn measure_text(family: &str, text: &str) -> Rect {
    let document = DesignDocument::new(DesignFormat::from_preset(FormatPreset::InstagramPortrait));
    let mut page = Page::new("Sample");
    let id = LayerId::new();

    page.layers.push(Layer::Text(TextLayer {
        common: LayerCommon {
            id,
            name: "Sample".to_string(),
            role: Some(LayerRole::Body),
            visible: true,
            locked: false,
            opacity: 1.0,
            transform: Transform::new(80.0, 160.0, 920.0, 400.0),
        },
        text: text.to_string(),
        font_family: family.to_string(),
        font_size: 72.0,
        font_weight: 400,
        line_height: 1.1,
        letter_spacing: 0.0,
        align: TextAlign::Left,
        color: DesignColor::token("brand.text"),
    }));

    let rendered = render_page(&document, &page, &palette(), &registry(), &NoAssets)
        .expect("the sample must render");
    rendered.inspection.layer(id).expect("text bounds").layout
}
