//! Whether text can actually be read against whatever sits behind it.
//!
//! Seventeen validation codes and not one of them asked this, so metadata set in
//! a mid grey over mid-grey wet asphalt validated clean and reached a person's
//! eye. It is measured here rather than in the validator for the same reason
//! glyph bounds are: this crate is the render authority, and a second opinion
//! about what a colour resolves to would drift from what is actually drawn.

use std::collections::HashMap;

use design_core::{Color, DesignDocument, LayerId, Page};
use resvg::tiny_skia::Pixmap;

use crate::error::RenderError;
use crate::images::AssetSource;
use crate::svg::{self, BrandPalette};
use crate::{flatten, raster, RenderInspection};

/// WCAG's floor for large text. Design pieces are not web pages, but below this
/// a line stops being comfortable to read against a busy photograph.
pub const READABLE_RATIO: f32 = 3.0;

/// The backdrop is sampled from a half-scale render. Nothing here needs pixel
/// accuracy — the smallest box we measure is still tens of pixels across — and
/// validation already renders every page once.
const SAMPLE_SCALE: f32 = 0.5;

/// Perceptual distance at which two colours are plainly different regardless of
/// their lightness. Calibrated against a real cover: an accent red on wet
/// asphalt measures 75 and reads, while the two mid greys beside it that nobody
/// could see measure 12. Nothing observed lands between 12 and 57.
pub const DISTINCT_DELTA_E: f32 = 25.0;

/// Below this share of opaque pixels there is nothing meaningful to compare
/// against: the text sits over transparency, and what shows through is decided
/// by whatever displays the export.
const MIN_OPAQUE_FRACTION: f32 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextContrast {
    /// WCAG contrast ratio between the text colour and the mean backdrop.
    pub ratio: f32,
    /// Share of the sampled area whose own ratio is below [`READABLE_RATIO`].
    /// A line crossing a bright stripe can average out to a passing number while
    /// half of it is unreadable, which this catches and `ratio` does not.
    pub illegible_fraction: f32,
    /// Perceptual distance (CIE76 ΔE) between the ink and the mean backdrop.
    ///
    /// Luminance is not the whole of legibility. An accent red on wet asphalt
    /// can sit at a ratio of 1.09 -- the two are nearly the same lightness --
    /// and still read perfectly, because it breaks by hue instead. Reporting
    /// that as unreadable would send an agent to fix the one element that
    /// worked, so the ratio never speaks alone.
    pub delta_e: f32,
}

/// Measures every visible text layer of one page against what is drawn under it.
///
/// The backdrop is the page with **all** text hidden, rendered once. Text over
/// text is therefore measured against what is beneath both, which is a
/// degenerate arrangement rather than a case worth a render per layer.
pub fn measure(
    document: &DesignDocument,
    page: &Page,
    palette: &BrandPalette,
    fonts: &crate::FontRegistry,
    assets: &dyn AssetSource,
    inspection: &RenderInspection,
) -> Result<HashMap<LayerId, TextContrast>, RenderError> {
    let texts: Vec<_> = page
        .layers
        .iter()
        .filter_map(|layer| match layer {
            design_core::Layer::Text(text) if text.common.visible => Some(text),
            _ => None,
        })
        .collect();
    if texts.is_empty() {
        return Ok(HashMap::new());
    }

    let mut backdrop = page.clone();
    backdrop
        .layers
        .retain(|layer| !matches!(layer, design_core::Layer::Text(_)));

    let svg = svg::page_to_svg(document, &backdrop, palette, assets)?;
    let tree = flatten::parse(&svg, fonts)?;
    let pixmap = raster::render_pixmap(&tree, SAMPLE_SCALE)?;

    let mut out = HashMap::new();
    for text in texts {
        let Some(bounds) = inspection.layer(text.common.id) else {
            continue;
        };
        let Ok(ink) = svg::resolve(&text.color, palette) else {
            continue;
        };
        if let Some(contrast) = sample(&pixmap, bounds.painted, ink) {
            out.insert(text.common.id, contrast);
        }
    }
    Ok(out)
}

fn sample(pixmap: &Pixmap, area: design_core::Rect, ink: Color) -> Option<TextContrast> {
    let left = ((area.x * SAMPLE_SCALE).floor().max(0.0)) as u32;
    let top = ((area.y * SAMPLE_SCALE).floor().max(0.0)) as u32;
    let right = (((area.x + area.width) * SAMPLE_SCALE).ceil().max(0.0) as u32).min(pixmap.width());
    let bottom =
        (((area.y + area.height) * SAMPLE_SCALE).ceil().max(0.0) as u32).min(pixmap.height());
    if left >= right || top >= bottom {
        return None;
    }

    let ink_luminance = luminance(ink.r, ink.g, ink.b);
    let ink_lab = lab(ink.r, ink.g, ink.b);
    let (mut opaque, mut total, mut sum, mut illegible) = (0u32, 0u32, 0.0f32, 0u32);
    let mut behind_lab = [0.0f32; 3];

    for y in top..bottom {
        for x in left..right {
            total += 1;
            let Some(pixel) = pixmap.pixel(x, y) else {
                continue;
            };
            if pixel.alpha() < 8 {
                continue;
            }
            let pixel = pixel.demultiply();
            opaque += 1;
            let behind = luminance(pixel.red(), pixel.green(), pixel.blue());
            sum += behind;
            let sample = lab(pixel.red(), pixel.green(), pixel.blue());
            for (acc, value) in behind_lab.iter_mut().zip(sample) {
                *acc += value;
            }
            // Illegible means neither lightness nor colour separates the two.
            // Testing lightness alone condemns the one element on the cover that
            // actually worked.
            let distance = delta_e(ink_lab, sample);
            if ratio(ink_luminance, behind) < READABLE_RATIO && distance < DISTINCT_DELTA_E {
                illegible += 1;
            }
        }
    }

    if total == 0 || (opaque as f32 / total as f32) < MIN_OPAQUE_FRACTION {
        return None;
    }

    let mean_lab = behind_lab.map(|total| total / opaque as f32);
    Some(TextContrast {
        ratio: ratio(ink_luminance, sum / opaque as f32),
        illegible_fraction: illegible as f32 / opaque as f32,
        delta_e: delta_e(ink_lab, mean_lab),
    })
}

fn delta_e(one: [f32; 3], other: [f32; 3]) -> f32 {
    one.iter()
        .zip(other)
        .map(|(a, b)| (a - b).powi(2))
        .sum::<f32>()
        .sqrt()
}

/// sRGB to CIELAB, D65. CIE76 is a blunt instrument next to ΔE2000, but the
/// question here is only "are these plainly different colours", and for that the
/// extra machinery buys nothing.
fn lab(r: u8, g: u8, b: u8) -> [f32; 3] {
    let (r, g, b) = (linear(r), linear(g), linear(b));
    let x = (0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.9505;
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let z = (0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.089;
    let f = |t: f32| {
        if t > 0.008856 {
            t.cbrt()
        } else {
            7.787 * t + 16.0 / 116.0
        }
    };
    let (fx, fy, fz) = (f(x), f(y), f(z));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// WCAG relative luminance.
fn luminance(r: u8, g: u8, b: u8) -> f32 {
    0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
}

fn linear(value: u8) -> f32 {
    channel(value)
}

fn channel(value: u8) -> f32 {
    let value = value as f32 / 255.0;
    if value <= 0.03928 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn ratio(one: f32, other: f32) -> f32 {
    let (high, low) = if one > other {
        (one, other)
    } else {
        (other, one)
    };
    (high + 0.05) / (low + 0.05)
}
