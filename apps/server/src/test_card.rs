//! The Phase 0 test card (plan section 77, Phase 0 acceptance).
//!
//! It exists so an MCP host can prove the whole pipeline end to end before any
//! real design exists. Phase 1 replaces it with actual documents.

use design_core::{
    BrandColors, BrandId, BrandKit, BrandRules, BrandTypography, Color, DesignColor,
    DesignDocument, DesignFormat, FormatPreset, Layer, LayerCommon, LayerId, LayerRole, Page,
    ProjectId, ShapeKind, ShapeLayer, TextAlign, TextLayer, Transform, Version,
};
use renderer::svg::BrandPalette;

/// Scaffolding until Phase 4 persists Brand Kits. It is a real `BrandKit`, so
/// token resolution here is the same code path the product will use.
pub fn default_brand_kit() -> BrandKit {
    BrandKit {
        id: BrandId::new(),
        project_id: ProjectId::new(),
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
        voice: "Direct, technical and brief. Do not use hype.".to_string(),
        rules: BrandRules::default(),
        version: Version::INITIAL,
    }
}

pub fn palette() -> BrandPalette {
    default_brand_kit().color_palette()
}

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

pub fn document(label: &str) -> (DesignDocument, Page) {
    let document = DesignDocument::new(DesignFormat::from_preset(FormatPreset::Square));

    let mut page = Page::new("Test card");
    page.background = Some(DesignColor::token("brand.background"));

    page.layers.push(Layer::Shape(ShapeLayer {
        common: common(
            "Frame",
            LayerRole::Decoration,
            Transform::new(60.0, 60.0, 960.0, 960.0),
        ),
        kind: ShapeKind::Rectangle,
        fill: Some(DesignColor::token("brand.surface")),
        stroke: Some(DesignColor::token("brand.primary")),
        stroke_width: 4.0,
        corner_radius: 24.0,
    }));

    page.layers.push(Layer::Text(TextLayer {
        common: common(
            "Title",
            LayerRole::Headline,
            Transform::new(120.0, 300.0, 840.0, 240.0),
        ),
        text: "FORMA\nTEST CARD".to_string(),
        font_family: "DejaVu Sans".to_string(),
        font_size: 96.0,
        font_weight: 700,
        line_height: 1.1,
        letter_spacing: 0.0,
        align: TextAlign::Left,
        color: DesignColor::token("brand.text"),
    }));

    page.layers.push(Layer::Shape(ShapeLayer {
        common: common(
            "Rule",
            LayerRole::Decoration,
            Transform::new(120.0, 560.0, 220.0, 10.0),
        ),
        kind: ShapeKind::Rectangle,
        fill: Some(DesignColor::token("brand.primary")),
        stroke: None,
        stroke_width: 0.0,
        corner_radius: 5.0,
    }));

    page.layers.push(Layer::Text(TextLayer {
        common: common(
            "Label",
            LayerRole::Body,
            Transform::new(120.0, 620.0, 840.0, 200.0),
        ),
        text: label.to_string(),
        font_family: "DejaVu Sans".to_string(),
        font_size: 40.0,
        font_weight: 400,
        line_height: 1.4,
        letter_spacing: 0.0,
        align: TextAlign::Left,
        color: DesignColor::token("brand.muted"),
    }));

    (document, page)
}
