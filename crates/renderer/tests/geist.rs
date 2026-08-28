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
        (narrow - wide).abs() < 1.0,
        "Geist Mono did not advance uniformly, so a proportional face was \
         substituted: narrow={narrow} wide={wide}"
    );
}

/// Lays out `text` against a specific registry and returns the width usvg gave
/// it. Passing the registry in is what lets a test compare the same string
/// rendered with and without the family that owns its glyphs.
fn painted_width_at(fonts: &FontRegistry, text: &str, family: &str, weight: u16) -> f32 {
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
        font_weight: weight,
        line_height: 1.1,
        letter_spacing: 0.0,
        align: TextAlign::Left,
        color: DesignColor::token("brand.text"),
    }));

    let rendered = render_page(&document, &page, &palette(), fonts, &NoAssets)
        .expect("the sample must render");
    rendered
        .inspection
        .layer(id)
        .expect("text bounds")
        .layout
        .width
}

/// Same as `measure`, with the sample string chosen by the caller.
fn measure_text(family: &str, text: &str) -> f32 {
    painted_width_at(&registry(), text, family, 400)
}

/// Builds a registry holding only the shipped font files whose name starts with
/// one of `prefixes`. `reversed` copies them in the opposite order, which is how
/// the order-independence test gets two directories a filesystem may enumerate
/// differently.
fn registry_of(prefixes: &[&str], reversed: bool) -> (tempfile::TempDir, FontRegistry) {
    let dir = tempfile::tempdir().unwrap();
    let shipped = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/fonts");
    let mut names: Vec<_> = std::fs::read_dir(shipped)
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            prefixes.iter().any(|prefix| name.starts_with(prefix))
        })
        .collect();
    names.sort();
    if reversed {
        names.reverse();
    }
    for path in names {
        std::fs::copy(&path, dir.path().join(path.file_name().unwrap())).unwrap();
    }
    let registry = FontRegistry::from_dir(dir.path()).unwrap();
    (dir, registry)
}

const GREEK: &str = "Δοκιμή ελληνικά";

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

    let greek = painted_width_at(&full, GREEK, "Geist", 400);
    let (_keep, geist_only) = registry_of(&["Geist"], false);
    assert!(!geist_only.contains("DejaVu Sans"));
    let tofu = painted_width_at(&geist_only, GREEK, "Geist", 400);

    // Rather than a threshold on "different enough", pin both sides: the fallback
    // render must match what the face that owns the glyphs produces on its own,
    // and must not match the tofu one.
    let (_keep, regular_only) = registry_of(&["Geist", "DejaVuSans."], false);
    let dejavu_regular = painted_width_at(&regular_only, GREEK, "Geist", 400);
    assert!(
        (greek - dejavu_regular).abs() < 0.01,
        "the glyphs should come from DejaVu Sans Regular: {greek} vs {dejavu_regular}"
    );
    assert!(
        (greek - tofu).abs() > 1.0,
        "expected the fallback to change the layout: {greek} vs {tofu}"
    );
}

/// The regression test for a CI failure that was a real defect, not a flaky test.
///
/// usvg's default fallback selector returns the *first* face in the database
/// that has the glyph, and its style guard rejects a candidate only when style,
/// weight *and* stretch all differ — so a Bold face answered for Regular text.
/// Since `fontdb` loads a directory in `read_dir` order, which face came first
/// was the filesystem's choice: this machine picked DejaVu Bold and CI picked
/// Regular, from the same commit. Renders have to be identical everywhere, which
/// is the whole reason the fonts are committed at all.
#[test]
fn the_fallback_face_follows_the_requested_weight() {
    let full = registry();
    let regular = painted_width_at(&full, GREEK, "Geist", 400);
    let bold = painted_width_at(&full, GREEK, "Geist", 700);
    assert!(
        (regular - bold).abs() > 1.0,
        "a 400 and a 700 layer should not fall back to the same face: {regular} vs {bold}"
    );

    let (_a, regular_only) = registry_of(&["Geist", "DejaVuSans."], false);
    let (_b, bold_only) = registry_of(&["Geist", "DejaVuSans-Bold"], false);
    assert!(
        (regular - painted_width_at(&regular_only, GREEK, "Geist", 400)).abs() < 0.01,
        "weight 400 should fall back to DejaVu Sans Regular"
    );
    assert!(
        (bold - painted_width_at(&bold_only, GREEK, "Geist", 700)).abs() < 0.01,
        "weight 700 should fall back to DejaVu Sans Bold"
    );
}

/// A directory holding one weight still gets used: matching the weight is a
/// preference, not a requirement. Refusing to fall back would draw tofu.
#[test]
fn a_lone_mismatched_face_is_still_better_than_tofu() {
    let (_keep, bold_only) = registry_of(&["Geist", "DejaVuSans-Bold"], false);
    let (_keep2, geist_only) = registry_of(&["Geist"], false);
    let bold = painted_width_at(&bold_only, GREEK, "Geist", 400);
    let tofu = painted_width_at(&geist_only, GREEK, "Geist", 400);
    assert!(
        (bold - tofu).abs() > 1.0,
        "a weight-400 run should still borrow the only face that has the glyphs"
    );
}

/// `FontRegistry` sorts the files before loading precisely so the enumeration
/// order of the filesystem cannot reach the output.
#[test]
fn the_order_the_files_are_written_in_does_not_reach_the_render() {
    let (_a, forwards) = registry_of(&["Geist", "DejaVu"], false);
    let (_b, backwards) = registry_of(&["Geist", "DejaVu"], true);
    for weight in [400, 700] {
        let one = painted_width_at(&forwards, GREEK, "Geist", weight);
        let other = painted_width_at(&backwards, GREEK, "Geist", weight);
        assert!(
            (one - other).abs() < 0.01,
            "weight {weight} rendered differently depending on file order: {one} vs {other}"
        );
    }
}

/// The other half of the fix, and the half a width comparison cannot reach.
///
/// Matching the weight makes the choice order-independent whenever the
/// candidates differ in weight, so the shipped fonts no longer expose the load
/// order at all. Ties still resolve positionally, and the next font added to
/// `data/fonts` could easily create one — so assert the mechanism itself rather
/// than a rendered width: the database is built in sorted path order, whatever
/// order the filesystem hands the files over in.
#[test]
fn the_faces_are_loaded_in_sorted_order() {
    // Written to disk backwards, so a directory that echoes creation order would
    // hand them back reversed.
    let (_keep, fonts) = registry_of(&["Geist", "DejaVu"], true);
    let db = fonts.db();
    let paths: Vec<_> = db
        .faces()
        .filter_map(|face| match &face.source {
            usvg::fontdb::Source::File(path) => Some(path.display().to_string()),
            _ => None,
        })
        .collect();

    assert!(paths.len() > 1, "expected several faces: {paths:?}");
    assert!(
        paths.windows(2).all(|pair| pair[0] <= pair[1]),
        "the faces are not in sorted order, so the filesystem decides the \
         fallback for a glyph two faces both cover: {paths:?}"
    );
}
