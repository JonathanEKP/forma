//! The invariant the agent workflow of plan section 51.7 depends on: if
//! `validate` says a design is valid, rendering it must succeed.
//!
//! This existed to be broken. A design using `$brand.*` tokens in a project
//! with no Brand Kit validated clean and then failed to render with
//! UNKNOWN_BRAND_TOKEN, so an agent would fix nothing and then hit a 500.

use std::path::PathBuf;

use design_core::{
    BrandColors, BrandId, BrandKit, BrandRules, BrandTypography, Color, DesignColor,
    DesignDocument, DesignFormat, DesignId, DesignRecord, FormatPreset, Layer, LayerCommon,
    LayerId, Page, ProjectId, ShapeKind, ShapeLayer, TextAlign, TextLayer, Transform, Version,
};
use persistence::{LocalObjectStore, SqliteAssetRepository};
use renderer::{FontRegistry, NoAssets};
use sqlx::SqlitePool;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fonts() -> FontRegistry {
    FontRegistry::from_dir(root().join("data/fonts")).expect("data/fonts must load")
}

fn brand_kit() -> BrandKit {
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
            display: font_family(),
            body: font_family(),
            mono: font_family(),
        },
        logo_asset_id: None,
        voice: String::new(),
        rules: BrandRules {
            safe_margin: 40.0,
            minimum_font_size: 12.0,
        },
        version: Version::INITIAL,
    }
}

/// Whatever family the shipped font directory actually provides, so this test
/// does not encode the default-font decision of section 93.2.
fn font_family() -> String {
    fonts()
        .families()
        .first()
        .cloned()
        .expect("data/fonts must contain at least one family")
}

fn common(name: &str, transform: Transform) -> LayerCommon {
    LayerCommon {
        id: LayerId::new(),
        name: name.to_string(),
        role: None,
        visible: true,
        locked: false,
        opacity: 1.0,
        transform,
    }
}

/// A design that leans on brand tokens for every colour it uses.
fn document_using_tokens() -> DesignDocument {
    let mut document = DesignDocument::new(DesignFormat::from_preset(FormatPreset::Square));
    let mut page = Page::new("Page 1");
    page.background = Some(DesignColor::token("brand.background"));

    page.layers.push(Layer::Shape(ShapeLayer {
        common: common("Bar", Transform::new(80.0, 880.0, 300.0, 16.0)),
        kind: ShapeKind::Rectangle,
        fill: Some(DesignColor::token("brand.primary")),
        stroke: None,
        stroke_width: 0.0,
        corner_radius: 8.0,
    }));

    page.layers.push(Layer::Text(TextLayer {
        common: common("Headline", Transform::new(80.0, 120.0, 900.0, 300.0)),
        text: "TOKENS".to_string(),
        font_family: font_family(),
        font_size: 90.0,
        font_weight: 700,
        line_height: 1.1,
        letter_spacing: 0.0,
        align: TextAlign::Left,
        color: DesignColor::token("brand.text"),
    }));

    document.pages.push(page);
    document
}

fn record(document: DesignDocument) -> DesignRecord {
    DesignRecord {
        id: DesignId::new(),
        project_id: ProjectId::new(),
        name: "Under test".to_string(),
        version: Version::INITIAL,
        created_at: "2026-08-28T00:00:00.000Z".to_string(),
        updated_at: "2026-08-28T00:00:00.000Z".to_string(),
        document,
    }
}

/// An empty asset store: the documents here use no images.
async fn empty_store() -> (SqliteAssetRepository, LocalObjectStore, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("forma.db");
    let pool: SqlitePool = persistence::connect(&format!("sqlite://{}", db.display()), dir.path())
        .await
        .expect("migrations");
    (
        SqliteAssetRepository::new(pool),
        LocalObjectStore::new(dir.path()),
        dir,
    )
}

#[tokio::test]
async fn a_design_reported_valid_can_actually_be_rendered() {
    let (assets, store, _dir) = empty_store().await;
    let fonts = fonts();
    let brand = brand_kit();
    let record = record(document_using_tokens());
    let page_id = record.document.pages[0].id;

    let report =
        application::renders::validate(account(), &record, Some(&brand), &fonts, &assets, &store)
            .await
            .unwrap();
    assert!(report.valid, "unexpected issues: {:?}", report.issues);

    // The promise the report just made has to hold.
    application::renders::render(&record.document, page_id, Some(&brand), &fonts, &NoAssets)
        .expect("a design reported valid must render");
}

/// The regression itself, at the layer where it actually bit.
#[tokio::test]
async fn tokens_without_a_brand_kit_are_reported_rather_than_failing_at_render() {
    let (assets, store, _dir) = empty_store().await;
    let fonts = fonts();
    let record = record(document_using_tokens());
    let page_id = record.document.pages[0].id;

    // Rendering genuinely cannot work: there is nothing to resolve the tokens.
    // RenderedPage carries no Debug, so the error is matched explicitly.
    let render_error =
        match application::renders::render(&record.document, page_id, None, &fonts, &NoAssets) {
            Err(error) => error,
            Ok(_) => panic!("tokens must not resolve without a Brand Kit"),
        };
    assert!(
        render_error.to_string().contains("UNKNOWN_BRAND_TOKEN"),
        "got: {render_error}"
    );

    // So validation must say so, instead of reporting the design as exportable.
    let report = application::renders::validate(account(), &record, None, &fonts, &assets, &store)
        .await
        .unwrap();

    assert!(
        !report.valid,
        "validate called an unrenderable design valid: {:?}",
        report.issues
    );
    assert!(
        report.errors().any(|i| i.code == "UNKNOWN_BRAND_TOKEN"),
        "expected UNKNOWN_BRAND_TOKEN, got: {:?}",
        report.issues
    );
}

/// The account these fixtures belong to: the one the migration creates.
fn account() -> design_core::AccountId {
    design_core::AccountId(
        uuid::Uuid::parse_str(design_core::BOOTSTRAP_ACCOUNT).expect("a constant uuid"),
    )
}
