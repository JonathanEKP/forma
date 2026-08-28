//! Renders the shipped families side by side, for the visual half of the
//! default-font decision (plan section 94, 93.2).
//!
//!     cargo run -p renderer --example font_specimen

use std::collections::HashMap;
use std::path::PathBuf;

use design_core::{
    Color, DesignColor, DesignDocument, DesignFormat, FormatPreset, Layer, LayerCommon, LayerId,
    LayerRole, Page, TextAlign, TextLayer, Transform,
};
use renderer::{fonts::FontRegistry, svg::BrandPalette, NoAssets};

fn text(name: &str, y: f32, size: f32, weight: u16, family: &str, body: &str) -> Layer {
    Layer::Text(TextLayer {
        common: LayerCommon {
            id: LayerId::new(),
            name: name.to_string(),
            role: Some(LayerRole::Body),
            visible: true,
            locked: false,
            opacity: 1.0,
            transform: Transform::new(70.0, y, 940.0, size * 1.6),
        },
        text: body.to_string(),
        font_family: family.to_string(),
        font_size: size,
        font_weight: weight,
        line_height: 1.2,
        letter_spacing: 0.0,
        align: TextAlign::Left,
        color: DesignColor::token("brand.text"),
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fonts = FontRegistry::from_dir(root.join("data/fonts"))?;

    let palette: BrandPalette = HashMap::from([
        ("brand.background".to_string(), Color::rgb(0x0B, 0x0B, 0x0C)),
        ("brand.text".to_string(), Color::rgb(0xFF, 0xFF, 0xFF)),
        ("brand.muted".to_string(), Color::rgb(0x8A, 0x8A, 0x90)),
    ]);

    let mut document = DesignDocument::new(DesignFormat::from_preset(FormatPreset::Square));
    document.format.height = 1240.0;

    let mut page = Page::new("Specimen");
    page.background = Some(DesignColor::token("brand.background"));

    let sample = "Give visual ideas a form";
    let mut y = 60.0;

    for (family, weights) in [
        (
            "Geist",
            vec![
                (400, "Regular"),
                (500, "Medium"),
                (600, "SemiBold"),
                (700, "Bold"),
            ],
        ),
        ("DejaVu Sans", vec![(400, "Regular"), (700, "Bold")]),
    ] {
        page.layers
            .push(text("label", y, 22.0, 400, "Geist Mono", family));
        y += 46.0;
        for (weight, name) in weights {
            page.layers.push(text(
                "weight",
                y,
                17.0,
                400,
                "Geist Mono",
                &format!("{weight} {name}"),
            ));
            page.layers
                .push(text("sample", y + 24.0, 46.0, weight, family, sample));
            y += 100.0;
        }
        y += 30.0;
    }

    // The punctuation a design actually uses, which is where a narrow font hurts.
    page.layers
        .push(text("label", y, 22.0, 400, "Geist Mono", "Punctuation"));
    y += 44.0;
    for family in ["Geist", "DejaVu Sans"] {
        page.layers.push(text(
            "punct",
            y,
            30.0,
            400,
            family,
            "“Quotes” — dash… • €£¥ ° × ™ → ¿Español? ñá",
        ));
        y += 56.0;
    }

    document.pages.push(page);

    let out = root.join("data/previews");
    std::fs::create_dir_all(&out)?;
    let png = renderer::render_page_png(
        &document,
        &document.pages[0],
        &palette,
        &fonts,
        &NoAssets,
        1.0,
    )?;
    std::fs::write(out.join("font-specimen.png"), &png)?;
    println!(
        "wrote data/previews/font-specimen.png ({} bytes)",
        png.len()
    );
    Ok(())
}
