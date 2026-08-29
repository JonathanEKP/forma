//! Renders the Phase 0 fixture to disk so a human can look at it.
//!
//!     cargo run -p renderer --example render_fixture

use std::collections::HashMap;
use std::path::PathBuf;

use design_core::{
    Color, DesignColor, DesignDocument, DesignFormat, FormatPreset, Layer, LayerCommon, LayerId,
    LayerRole, Page, ShapeKind, ShapeLayer, TextAlign, TextLayer, Transform,
};
use renderer::{fonts::FontRegistry, svg::BrandPalette, NoAssets};

fn common(name: &str, role: LayerRole, transform: Transform) -> LayerCommon {
    LayerCommon {
        id: LayerId::new(),
        name: name.to_string(),
        role: Some(role),
        visible: true,
        locked: false,
        opacity: 1.0,
        transform,
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fonts = FontRegistry::from_dir(root.join("data/fonts"))?;

    let palette: BrandPalette = BrandPalette::from_colors(HashMap::from([
        ("brand.background".to_string(), Color::rgb(0x10, 0x10, 0x10)),
        ("brand.surface".to_string(), Color::rgb(0x19, 0x19, 0x19)),
        ("brand.primary".to_string(), Color::rgb(0xFF, 0x54, 0x36)),
        ("brand.muted".to_string(), Color::rgb(0xA3, 0xA3, 0xA3)),
        ("brand.text".to_string(), Color::rgb(0xFF, 0xFF, 0xFF)),
    ]));

    let document = DesignDocument::new(DesignFormat::from_preset(FormatPreset::InstagramPortrait));
    let mut page = Page::new("Slide 1");
    page.background = Some(DesignColor::token("brand.background"));

    page.layers.push(Layer::Shape(ShapeLayer {
        common: common(
            "Panel",
            LayerRole::Decoration,
            Transform::new(0.0, 1100.0, 1080.0, 250.0),
        ),
        kind: ShapeKind::Rectangle,
        fill: Some(DesignColor::token("brand.surface")),
        stroke: None,
        stroke_width: 0.0,
        corner_radius: 0.0,
    }));

    page.layers.push(Layer::Text(TextLayer {
        common: common(
            "Headline",
            LayerRole::Headline,
            Transform::new(80.0, 200.0, 920.0, 460.0),
        ),
        text: "FORMA\nRENDERS\nTHIS".to_string(),
        font_family: "DejaVu Sans".to_string(),
        font_size: 150.0,
        font_weight: 700,
        line_height: 1.05,
        letter_spacing: 0.0,
        align: TextAlign::Left,
        color: DesignColor::token("brand.text"),
    }));

    page.layers.push(Layer::Shape(ShapeLayer {
        common: common(
            "Accent",
            LayerRole::Decoration,
            Transform::new(80.0, 720.0, 260.0, 14.0),
        ),
        kind: ShapeKind::Rectangle,
        fill: Some(DesignColor::token("brand.primary")),
        stroke: None,
        stroke_width: 0.0,
        corner_radius: 7.0,
    }));

    page.layers.push(Layer::Text(TextLayer {
        common: common(
            "Body",
            LayerRole::Body,
            Transform::new(80.0, 790.0, 920.0, 220.0),
        ),
        text: "A local design runtime for agents.\nRust authors the SVG.\nresvg rasterizes it."
            .to_string(),
        font_family: "DejaVu Sans".to_string(),
        font_size: 46.0,
        font_weight: 400,
        line_height: 1.4,
        letter_spacing: 0.0,
        align: TextAlign::Left,
        color: DesignColor::token("brand.muted"),
    }));

    page.layers.push(Layer::Shape(ShapeLayer {
        common: common(
            "Dot",
            LayerRole::Decoration,
            Transform::new(880.0, 1180.0, 90.0, 90.0),
        ),
        kind: ShapeKind::Ellipse,
        fill: Some(DesignColor::token("brand.primary")),
        stroke: None,
        stroke_width: 0.0,
        corner_radius: 0.0,
    }));

    page.layers.push(Layer::Text(TextLayer {
        common: common(
            "Footer",
            LayerRole::Cta,
            Transform::new(80.0, 1190.0, 700.0, 80.0),
        ),
        text: "forma-plan.md - phase 0".to_string(),
        font_family: "DejaVu Sans".to_string(),
        font_size: 34.0,
        font_weight: 400,
        line_height: 1.2,
        letter_spacing: 2.0,
        align: TextAlign::Left,
        color: DesignColor::token("brand.text"),
    }));

    let out = root.join("data/previews");
    std::fs::create_dir_all(&out)?;

    let rendered = renderer::render_page(&document, &page, &palette, &fonts, &NoAssets)?;
    std::fs::write(out.join("phase0.svg"), &rendered.normalized_svg)?;

    let png = renderer::render_page_png(&document, &page, &palette, &fonts, &NoAssets, 1.0)?;
    std::fs::write(out.join("phase0.png"), &png)?;

    println!(
        "canvas      {}x{}",
        rendered.inspection.canvas_width, rendered.inspection.canvas_height
    );
    println!(
        "ids survived usvg: {}",
        rendered.inspection.ids_survived_normalization
    );
    for b in &rendered.inspection.layers {
        println!(
            "  {:<46} x={:7.1} y={:7.1} w={:7.1} h={:7.1}",
            b.layer_id, b.layout.x, b.layout.y, b.layout.width, b.layout.height
        );
    }
    println!("svg  {} bytes", rendered.normalized_svg.len());
    println!("png  {} bytes", png.len());
    Ok(())
}
