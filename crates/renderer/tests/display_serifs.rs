//! The display serifs ship as variable fonts, and this is what checks the axis.
//!
//! `data/fonts/README.md` explains why Geist ships as a static file per weight:
//! it sidesteps the question of how reliably usvg reads a variation axis. Bodoni
//! Moda and Playfair Display are only published as variable fonts, so that
//! question has to be answered by measurement instead.
//!
//! Measure ink, not width. The advance width comes from the variable *metrics*,
//! which usvg can read correctly while still flattening an outline from the
//! default instance — the bug would be invisible to a width comparison and
//! obvious on the page. Ink is the drawn result.

use std::collections::HashMap;
use std::path::PathBuf;

use design_core::{
    Color, DesignColor, DesignDocument, DesignFormat, FormatPreset, Layer, LayerCommon, LayerId,
    Page, TextAlign, TextLayer, Transform,
};
use renderer::{fonts::FontRegistry, render_page_png, svg::BrandPalette, NoAssets};
use resvg::tiny_skia::Pixmap;

const SERIFS: [&str; 2] = ["Bodoni Moda", "Playfair Display"];

fn registry() -> FontRegistry {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/fonts");
    FontRegistry::from_dir(dir).expect("the shipped fonts must load")
}

/// Opaque pixels in a render of one glyph: how much of it is actually drawn.
fn ink(fonts: &FontRegistry, glyph: char, family: &str, weight: u16) -> usize {
    let document = DesignDocument::new(DesignFormat {
        preset: FormatPreset::Custom,
        width: 240.0,
        height: 200.0,
    });
    let mut page = Page::new("Glyph");
    page.layers.push(Layer::Text(TextLayer {
        common: LayerCommon {
            id: LayerId::new(),
            name: "Glyph".to_string(),
            role: None,
            visible: true,
            locked: false,
            opacity: 1.0,
            transform: Transform::new(20.0, 20.0, 200.0, 160.0),
        },
        text: glyph.to_string(),
        font_family: family.to_string(),
        font_size: 110.0,
        font_weight: weight,
        line_height: 1.1,
        letter_spacing: 0.0,
        align: TextAlign::Left,
        color: DesignColor::token("brand.text"),
    }));
    let palette: BrandPalette = BrandPalette::from_colors(HashMap::from([(
        "brand.text".to_string(),
        Color::rgb(20, 21, 26),
    )]));
    let png = render_page_png(&document, &page, &palette, fonts, &NoAssets, 1.0)
        .expect("the glyph must render");
    Pixmap::decode_png(&png)
        .expect("decodable PNG")
        .pixels()
        .iter()
        .filter(|pixel| pixel.alpha() > 128)
        .count()
}

#[test]
fn the_display_serifs_are_in_the_catalog() {
    let fonts = registry();
    for family in SERIFS {
        assert!(
            fonts.contains(family),
            "{family} is missing from {:?}",
            fonts.families()
        );
    }
}

/// Every letter has to answer the weight axis, not just most of them.
///
/// A font whose glyphs came out of the default instance would still advance
/// correctly and still pass a width check, and would render one hairline letter
/// in the middle of a heavy word. Comparing drawn ink per glyph is what sees it.
#[test]
fn every_letter_of_the_display_serifs_responds_to_the_weight_axis() {
    let fonts = registry();
    for family in SERIFS {
        let mut deaf = Vec::new();
        for glyph in ('A'..='Z').chain('a'..='z') {
            let light = ink(&fonts, glyph, family, 400);
            let heavy = ink(&fonts, glyph, family, 900);
            // A responding glyph gains a lot of ink; these two faces gain 90% or
            // more. A tenth of that is comfortably clear of antialiasing noise
            // while still failing an outline that never moved.
            if heavy < light + light / 10 {
                deaf.push((glyph, light, heavy));
            }
        }
        assert!(
            deaf.is_empty(),
            "{family}: these glyphs barely gained ink between weight 400 and 900, \
             so their outlines came from the default instance: {deaf:?}"
        );
    }
}

/// The two serifs must be distinguishable from each other and from Geist, or a
/// design that names one is not getting what it asked for.
#[test]
fn each_serif_draws_differently_from_the_others() {
    let fonts = registry();
    let bodoni = ink(&fonts, 'R', "Bodoni Moda", 400);
    let playfair = ink(&fonts, 'R', "Playfair Display", 400);
    let geist = ink(&fonts, 'R', "Geist", 400);
    assert!(
        bodoni != playfair && playfair != geist && bodoni != geist,
        "the families rendered identically, so one silently resolved to another: \
         bodoni={bodoni} playfair={playfair} geist={geist}"
    );
}
