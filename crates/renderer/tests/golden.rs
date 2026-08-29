//! Golden rendering tests (plan section 63).
//!
//! These exist to catch the failure the whole product rests on: a change that
//! silently alters what Forma draws. The preview and the export come off this
//! one pipeline, so a regression here is a regression in both.
//!
//! Comparison is a pixel diff with a documented threshold rather than a hash,
//! because a legitimate renderer change should be reviewable rather than simply
//! "different". See `THRESHOLD` for how the number was arrived at.
//!
//! To update a golden after a change you believe is correct:
//!
//! ```text
//! UPDATE_GOLDEN=1 cargo test -p renderer --test golden
//! ```
//!
//! Then **look at the images** before committing them. An updated golden that
//! nobody looked at is a test that has stopped testing anything.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use design_core::{AssetId, Color, DesignDocument, Layer};
use renderer::{svg::BrandPalette, FontRegistry, InMemoryAssets, RasterAsset};
use resvg::tiny_skia::Pixmap;

/// Per-channel difference that counts as a changed pixel.
///
/// Anti-aliasing is where a renderer legitimately wobbles, and a difference of
/// one or two levels in a single channel is invisible. Anything a person could
/// see moves a channel far further than this.
const CHANNEL_TOLERANCE: u8 = 2;

/// The fraction of pixels allowed to differ beyond `CHANNEL_TOLERANCE`.
///
/// Measured rather than invented, which plan section 93.3 asks for explicitly.
/// On `shapes-and-type.json`, a 1080×1080 canvas:
///
/// | change | pixels differing |
/// | --- | --- |
/// | none — the same document rendered twice | 0.0000% |
/// | one layer nudged by a single pixel | 0.1528% |
/// | one font size reduced by a point | 0.2262% |
/// | a colour token resolving differently | 80.78% |
///
/// The noise floor is zero because the renderer is byte-deterministic, so the
/// threshold's only job is to absorb the anti-aliasing differences another
/// resvg build or CPU might produce. 0.01% allows about 117 pixels of this
/// canvas to move — plenty for that — while sitting fifteen times below the
/// smallest change a person would call a regression.
///
/// `measured_sensitivity` re-measures these on every run and fails if the gap
/// ever closes, so the number cannot quietly stop meaning anything.
const THRESHOLD: f64 = 0.0001;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fonts() -> FontRegistry {
    FontRegistry::from_dir(root().join("data/fonts")).expect("data/fonts must load")
}

/// The palette the goldens were rendered with. Written out rather than read
/// from a Brand Kit so a change to the shipped kit cannot silently invalidate
/// every golden.
fn palette() -> BrandPalette {
    HashMap::from([
        ("brand.background".to_string(), Color::rgb(0x10, 0x10, 0x10)),
        ("brand.surface".to_string(), Color::rgb(0x19, 0x19, 0x19)),
        ("brand.primary".to_string(), Color::rgb(0xFF, 0x54, 0x36)),
        ("brand.secondary".to_string(), Color::rgb(0xFD, 0xBA, 0x2D)),
        ("brand.accent".to_string(), Color::rgb(0x7C, 0x3A, 0xED)),
        ("brand.text".to_string(), Color::rgb(0xFF, 0xFF, 0xFF)),
        (
            "brand.textInverse".to_string(),
            Color::rgb(0x10, 0x10, 0x10),
        ),
        ("brand.muted".to_string(), Color::rgb(0xA3, 0xA3, 0xA3)),
    ])
    .into()
}

/// Every asset any fixture references, all pointing at the one test image.
///
/// The fixtures use different ids on purpose — they were written for different
/// phases — and what a golden checks is the drawing, not which file it came
/// from.
fn assets(document: &DesignDocument) -> InMemoryAssets {
    let bytes = std::fs::read(root().join("fixtures/assets/hero.png")).expect("hero fixture");
    let mut assets = InMemoryAssets::new();

    for page in &document.pages {
        for layer in &page.layers {
            if let Layer::Image(image) = layer {
                assets.insert(
                    image.asset_id,
                    RasterAsset::from_bytes(bytes.clone()).expect("hero must be a readable raster"),
                );
            }
        }
    }
    let _ = AssetId::new();
    assets
}

fn load(fixture: &str) -> DesignDocument {
    let raw = std::fs::read_to_string(root().join("fixtures/designs").join(fixture))
        .unwrap_or_else(|e| panic!("{fixture}: {e}"));
    DesignDocument::migrate(serde_json::from_str(&raw).expect("valid JSON")).expect("schema 1")
}

fn render(document: &DesignDocument, page: usize) -> Pixmap {
    let png = renderer::render_page_png(
        document,
        &document.pages[page],
        &palette(),
        &fonts(),
        &assets(document),
        1.0,
    )
    .expect("the fixture must render");
    Pixmap::decode_png(&png).expect("decodable PNG")
}

/// The fraction of pixels that differ beyond the per-channel tolerance.
///
/// Returns 1.0 for a size mismatch: a canvas that changed size is not a near
/// miss, it is a different picture.
fn difference(a: &Pixmap, b: &Pixmap) -> f64 {
    if a.width() != b.width() || a.height() != b.height() {
        return 1.0;
    }

    let differing = a
        .pixels()
        .iter()
        .zip(b.pixels())
        .filter(|(x, y)| {
            let d = |p: u8, q: u8| p.abs_diff(q) > CHANNEL_TOLERANCE;
            d(x.red(), y.red())
                || d(x.green(), y.green())
                || d(x.blue(), y.blue())
                || d(x.alpha(), y.alpha())
        })
        .count();

    differing as f64 / (a.width() * a.height()) as f64
}

fn golden_path(name: &str) -> PathBuf {
    root().join("fixtures/golden").join(format!("{name}.png"))
}

/// Compares one render against its golden, or writes the golden when asked.
fn check(name: &str, rendered: &Pixmap) {
    let path = golden_path(name);

    if std::env::var("UPDATE_GOLDEN").is_ok() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, rendered.encode_png().unwrap()).unwrap();
        eprintln!("wrote {} — look at it before committing", path.display());
        return;
    }

    let bytes = std::fs::read(&path).unwrap_or_else(|_| {
        panic!(
            "no golden at {}. If this fixture is new, run:\n\
             \n    UPDATE_GOLDEN=1 cargo test -p renderer --test golden\n\n\
             then look at the image before committing it.",
            path.display()
        )
    });
    let golden = Pixmap::decode_png(&bytes).expect("the golden must be a readable PNG");

    let diff = difference(&golden, rendered);
    if diff > THRESHOLD {
        // Write what we actually rendered next to the golden, so the reviewer
        // can look at both rather than guess from a number.
        let actual = std::env::temp_dir().join(format!("forma-golden-{name}-actual.png"));
        let _ = std::fs::write(&actual, rendered.encode_png().unwrap());

        panic!(
            "{name} drifted: {:.4}% of pixels differ, threshold is {:.4}%.\n\
             golden:   {}\n\
             rendered: {}\n\
             Look at both. If the change is correct, rerun with UPDATE_GOLDEN=1.",
            diff * 100.0,
            THRESHOLD * 100.0,
            path.display(),
            actual.display(),
        );
    }
}

#[test]
fn carousel_cover() {
    let document = load("carousel.json");
    check("carousel-cover", &render(&document, 0));
}

#[test]
fn carousel_detail() {
    let document = load("carousel.json");
    check("carousel-detail", &render(&document, 1));
}

#[test]
fn acceptance_post() {
    let document = load("phase2-acceptance.json");
    check("acceptance-post", &render(&document, 0));
}

#[test]
fn shapes_and_type() {
    let document = load("shapes-and-type.json");
    check("shapes-and-type", &render(&document, 0));
}

/// The noise floor: the same document twice is the same pixels.
///
/// If this ever fails, the threshold is meaningless and the golden tests are
/// measuring randomness.
#[test]
fn re_rendering_changes_nothing() {
    let document = load("shapes-and-type.json");
    assert_eq!(
        difference(&render(&document, 0), &render(&document, 0)),
        0.0
    );
}

/// The other end of the scale: the threshold has to be far below the smallest
/// change a person would call a regression.
///
/// This is what justifies the number rather than asserting it — if a future
/// renderer change makes small edits nearly invisible to the diff, the
/// threshold needs revisiting and this fails first.
#[test]
fn measured_sensitivity() {
    let document = load("shapes-and-type.json");
    let baseline = render(&document, 0);

    let mut nudged = document.clone();
    let Layer::Text(text) = &mut nudged.pages[0].layers[4] else {
        panic!("layer 4 must be the centred headline");
    };
    text.common.transform.x += 1.0;
    let one_pixel = difference(&baseline, &render(&nudged, 0));

    let mut smaller = document.clone();
    let Layer::Text(text) = &mut smaller.pages[0].layers[4] else {
        unreachable!()
    };
    text.font_size -= 1.0;
    let one_point = difference(&baseline, &render(&smaller, 0));

    let mut recoloured = document.clone();
    recoloured.pages[0].background = Some("$brand.background".parse().unwrap());
    let one_token = difference(&baseline, &render(&recoloured, 0));

    // Printed first, so a failure shows every measurement rather than only the
    // one that tripped. These are the numbers THRESHOLD's documentation cites.
    eprintln!(
        "sensitivity: 1px {:.4}%  1pt {:.4}%  token {:.4}%  (threshold {:.4}%)",
        one_pixel * 100.0,
        one_point * 100.0,
        one_token * 100.0,
        THRESHOLD * 100.0,
    );

    for (what, measured) in [
        ("a one-pixel nudge", one_pixel),
        ("a one-point font size", one_point),
        ("a different colour token", one_token),
    ] {
        assert!(
            measured > THRESHOLD * 10.0,
            "{what} changes only {:.4}% of pixels, too close to the {:.4}% threshold: \
             the threshold no longer separates noise from regressions",
            measured * 100.0,
            THRESHOLD * 100.0,
        );
    }
}

/// A golden that does not exist is a test that silently passes, so the set is
/// pinned: adding a fixture means adding its golden deliberately.
#[test]
fn every_golden_is_present() {
    if std::env::var("UPDATE_GOLDEN").is_ok() {
        return; // they are being written by the other tests in this same run
    }

    for name in [
        "carousel-cover",
        "carousel-detail",
        "acceptance-post",
        "shapes-and-type",
    ] {
        let path = golden_path(name);
        assert!(
            Path::new(&path).exists(),
            "missing golden {}. Run UPDATE_GOLDEN=1 and review the image.",
            path.display()
        );
    }
}
