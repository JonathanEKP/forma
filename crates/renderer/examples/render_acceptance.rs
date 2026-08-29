//! Renders the Phase 2 acceptance fixture to disk.
//!
//!     cargo run -p renderer --example render_acceptance

use std::collections::HashMap;
use std::path::PathBuf;

use design_core::{AssetId, Color, DesignDocument};
use renderer::{svg::BrandPalette, FontRegistry, InMemoryAssets, RasterAsset};
use uuid::Uuid;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fonts = FontRegistry::from_dir(root.join("data/fonts"))?;

    let palette: BrandPalette = BrandPalette::from_colors(HashMap::from([
        ("brand.background".to_string(), Color::rgb(0x10, 0x10, 0x10)),
        ("brand.primary".to_string(), Color::rgb(0xFF, 0x54, 0x36)),
        ("brand.text".to_string(), Color::rgb(0xFF, 0xFF, 0xFF)),
        (
            "brand.textInverse".to_string(),
            Color::rgb(0x10, 0x10, 0x10),
        ),
        ("brand.muted".to_string(), Color::rgb(0xA3, 0xA3, 0xA3)),
    ]));

    let mut assets = InMemoryAssets::new();
    assets.insert(
        AssetId(Uuid::parse_str("eeeeeeee-0000-4000-8000-00000000beef")?),
        RasterAsset::from_bytes(std::fs::read(root.join("fixtures/assets/hero.png"))?)
            .ok_or("hero.png is not a readable raster")?,
    );

    let raw = std::fs::read_to_string(root.join("fixtures/designs/phase2-acceptance.json"))?;
    let document = DesignDocument::migrate(serde_json::from_str(&raw)?)?;
    let page = document.pages[0].clone();

    let out = root.join("data/previews");
    std::fs::create_dir_all(&out)?;

    let rendered = renderer::render_page(&document, &page, &palette, &fonts, &assets)?;
    for b in &rendered.inspection.layers {
        println!(
            "  {:<46} layout {:7.1},{:7.1} {:7.1}x{:<7.1}  painted {:7.1},{:7.1} {:7.1}x{:.1}",
            b.layer_id,
            b.layout.x,
            b.layout.y,
            b.layout.width,
            b.layout.height,
            b.painted.x,
            b.painted.y,
            b.painted.width,
            b.painted.height
        );
    }

    let png = renderer::render_page_png(&document, &page, &palette, &fonts, &assets, 1.0)?;
    std::fs::write(out.join("phase2.png"), &png)?;
    println!("png {} bytes", png.len());
    Ok(())
}
