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

/// Why DejaVu is still in `data/fonts` even though Geist is the default.
///
/// Geist maps 728 code points and DejaVu 5,918. When a run contains a glyph the
/// named family lacks, usvg falls back per glyph to another family in the
/// registry — but only if one is there. Rendered with Geist alone, Greek comes
/// out as tofu boxes; with DejaVu present it reads.
///
/// This is the test that stops someone reclaiming 1.5 MB by deleting DejaVu and
/// silently turning every non-Latin design into boxes.
#[test]
fn a_glyph_the_named_family_lacks_falls_back_to_one_that_has_it() {
    let full = registry();
    assert!(
        full.contains("DejaVu Sans"),
        "the fallback family is missing"
    );

    let greek = painted_width(&full, "Δοκιμή ελληνικά", "Geist");

    // A Geist-only registry, to show what the fallback is actually doing.
    let dir = tempfile::tempdir().unwrap();
    let shipped = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/fonts");
    for entry in std::fs::read_dir(shipped).unwrap().flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("Geist") {
            std::fs::copy(entry.path(), dir.path().join(name.as_ref())).unwrap();
        }
    }
    let geist_only = FontRegistry::from_dir(dir.path()).unwrap();
    assert!(!geist_only.contains("DejaVu Sans"));

    let tofu = painted_width(&geist_only, "Δοκιμή ελληνικά", "Geist");

    // Tofu boxes are a fixed advance; real glyphs are not. The two layouts
    // differ, which is the observable difference between reading and not.
    assert!(
        (greek - tofu).abs() > 20.0,
        "expected the fallback to change the layout: {greek} vs {tofu}"
    );
}

/// The painted width of one line, which is how a fallback shows up.
fn painted_width(fonts: &FontRegistry, text: &str, family: &str) -> f32 {
    let palette: BrandPalette =
        HashMap::from([("brand.text".to_string(), Color::rgb(0xFF, 0xFF, 0xFF))]);

    let mut document = DesignDocument::new(DesignFormat::from_preset(FormatPreset::Square));
    let mut page = Page::new("probe");
    page.layers.push(Layer::Text(TextLayer {
        common: LayerCommon {
            id: LayerId::new(),
            name: "probe".to_string(),
            role: Some(LayerRole::Body),
            visible: true,
            locked: false,
            opacity: 1.0,
            transform: Transform::new(40.0, 60.0, 1000.0, 150.0),
        },
        text: text.to_string(),
        font_family: family.to_string(),
        font_size: 72.0,
        font_weight: 400,
        line_height: 1.2,
        letter_spacing: 0.0,
        align: TextAlign::Left,
        color: DesignColor::token("brand.text"),
    }));
    document.pages.push(page);

    let rendered =
        render_page(&document, &document.pages[0], &palette, fonts, &NoAssets).expect("render");
    rendered.inspection.layers[0].painted.width
}
