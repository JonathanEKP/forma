//! Phase 3 acceptance (plan section 77): a unit test for every code of
//! section 42.
//!
//! Each test provokes exactly one code and asserts its severity, because the
//! error/warning split is what decides whether an export is blocked.

use design_core::{
    BrandColors, BrandId, BrandKit, BrandRules, BrandTypography, Color, DesignColor,
    DesignDocument, DesignFormat, FormatPreset, ImageFit, ImageLayer, Layer, LayerCommon, LayerId,
    Page, ProjectId, Rect, ShapeKind, ShapeLayer, TextAlign, TextLayer, Transform, Version,
};
use validator::{
    validate, AssetInfo, MeasuredLayer, Severity, TextContrast, ValidationContext, ValidationReport,
};

fn brand() -> BrandKit {
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
        voice: String::new(),
        rules: BrandRules {
            safe_margin: 80.0,
            minimum_font_size: 24.0,
        },
        version: Version::INITIAL,
    }
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

fn text_layer(name: &str, transform: Transform) -> TextLayer {
    TextLayer {
        common: common(name, transform),
        text: "Hello".to_string(),
        font_family: "DejaVu Sans".to_string(),
        font_size: 48.0,
        font_weight: 400,
        line_height: 1.2,
        letter_spacing: 0.0,
        align: TextAlign::Left,
        color: DesignColor::token("brand.text"),
    }
}

fn shape_layer(name: &str, transform: Transform) -> ShapeLayer {
    ShapeLayer {
        common: common(name, transform),
        kind: ShapeKind::Rectangle,
        fill: Some(DesignColor::token("brand.primary")),
        stroke: None,
        stroke_width: 0.0,
        corner_radius: 0.0,
    }
}

fn document_with(layers: Vec<Layer>) -> DesignDocument {
    let mut document = DesignDocument::new(DesignFormat::from_preset(FormatPreset::Square));
    let mut page = Page::new("Page 1");
    page.background = Some(DesignColor::token("brand.background"));
    page.layers = layers;
    document.pages.push(page);
    document
}

/// A context that is clean by construction, so each test introduces exactly one
/// problem and any other code appearing is a bug in the validator.
fn clean_context<'a>(document: &'a DesignDocument, brand: &'a BrandKit) -> ValidationContext<'a> {
    ValidationContext::new(document)
        .with_brand(brand)
        .with_fonts(["DejaVu Sans"])
}

fn assert_only(report: &ValidationReport, code: &str, severity: Severity) {
    let matching: Vec<_> = report.issues.iter().filter(|i| i.code == code).collect();
    assert_eq!(
        matching.len(),
        1,
        "expected exactly one {code}, got: {:?}",
        report.issues
    );
    assert_eq!(matching[0].severity, severity, "wrong severity for {code}");
    assert_eq!(
        report.valid,
        severity == Severity::Warning,
        "a {severity:?} must {} block export",
        if severity == Severity::Error {
            ""
        } else {
            "not"
        }
    );
}

#[test]
fn a_sound_design_produces_no_issues() {
    let brand = brand();
    let document = document_with(vec![Layer::Text(text_layer(
        "Headline",
        Transform::new(100.0, 100.0, 800.0, 200.0),
    ))]);

    let report = validate(&clean_context(&document, &brand));
    assert!(report.valid, "unexpected issues: {:?}", report.issues);
    assert!(report.issues.is_empty(), "{:?}", report.issues);
}

// --- Structural -----------------------------------------------------------

#[test]
fn invalid_page_size() {
    let brand = brand();
    let mut document = document_with(vec![Layer::Text(text_layer(
        "Headline",
        Transform::new(10.0, 10.0, 100.0, 50.0),
    ))]);
    document.format.width = 0.0;

    let report = validate(&clean_context(&document, &brand));
    assert_only(&report, "INVALID_PAGE_SIZE", Severity::Error);
}

#[test]
fn duplicate_layer_id() {
    let brand = brand();
    let first = text_layer("A", Transform::new(100.0, 100.0, 400.0, 100.0));
    let mut second = text_layer("B", Transform::new(100.0, 300.0, 400.0, 100.0));
    second.common.id = first.common.id;

    let document = document_with(vec![Layer::Text(first), Layer::Text(second)]);
    let report = validate(&clean_context(&document, &brand));
    assert_only(&report, "DUPLICATE_LAYER_ID", Severity::Error);
}

#[test]
fn missing_layer() {
    let brand = brand();
    let document = document_with(vec![]);

    let report = validate(&clean_context(&document, &brand));
    // An empty page renders blank, which is suspicious but not broken.
    assert_only(&report, "MISSING_LAYER", Severity::Warning);
}

#[test]
fn invalid_opacity() {
    let brand = brand();
    let mut layer = text_layer("Ghost", Transform::new(100.0, 100.0, 400.0, 100.0));
    layer.common.opacity = 1.5;

    let document = document_with(vec![Layer::Text(layer)]);
    let report = validate(&clean_context(&document, &brand));
    assert_only(&report, "INVALID_OPACITY", Severity::Error);
}

#[test]
fn invalid_dimension() {
    let brand = brand();
    let layer = text_layer("Flat", Transform::new(100.0, 100.0, 0.0, 100.0));

    let document = document_with(vec![Layer::Text(layer)]);
    let report = validate(&clean_context(&document, &brand));
    assert!(report.has("INVALID_DIMENSION"));
    assert!(!report.valid);
}

/// INVALID_COLOR cannot be produced from a parsed document: a malformed literal
/// is rejected at deserialization, so it can never reach the validator. The code
/// stays reserved and is emitted by the operations layer instead.
#[test]
fn invalid_color_is_rejected_before_validation() {
    let raw = serde_json::json!({
        "schema_version": 1,
        "format": { "preset": "square", "width": 1080.0, "height": 1080.0 },
        "pages": [{
            "id": "11111111-1111-4111-8111-111111111111",
            "name": "Page",
            "background": "#GGGGGG",
            "layers": []
        }]
    });
    let error = DesignDocument::migrate(raw).unwrap_err();
    assert_eq!(error.code(), "INVALID_DOCUMENT");
    assert!(error.to_string().contains("hex"), "{error}");
}

// --- Bounds ---------------------------------------------------------------

#[test]
fn outside_canvas_is_an_error_for_text() {
    let brand = brand();
    // Sits well past the right edge of a 1080px canvas.
    let layer = text_layer("Lost headline", Transform::new(900.0, 400.0, 600.0, 100.0));

    let document = document_with(vec![Layer::Text(layer)]);
    let report = validate(&clean_context(&document, &brand));

    let issue = report
        .issues
        .iter()
        .find(|i| i.code == "OUTSIDE_CANVAS")
        .expect("expected OUTSIDE_CANVAS");
    assert_eq!(issue.severity, Severity::Error);
    assert!(!report.valid);
}

/// Plan section 15: a bleeding photo is deliberate, not a failure.
#[test]
fn outside_canvas_is_only_a_warning_for_decoration() {
    let brand = brand();
    let layer = shape_layer("Bleed", Transform::new(-100.0, -100.0, 1280.0, 400.0));

    let document = document_with(vec![Layer::Shape(layer)]);
    let report = validate(&clean_context(&document, &brand));

    assert_only(&report, "OUTSIDE_CANVAS", Severity::Warning);
    assert!(report.valid, "a bleed must not block export");
}

#[test]
fn a_layer_entirely_off_canvas_is_always_an_error() {
    let brand = brand();
    let layer = shape_layer("Stray", Transform::new(3000.0, 3000.0, 100.0, 100.0));

    let document = document_with(vec![Layer::Shape(layer)]);
    let report = validate(&clean_context(&document, &brand));

    assert_only(&report, "OUTSIDE_CANVAS", Severity::Error);
    let issue = &report.issues[0];
    assert_eq!(issue.details.as_ref().unwrap()["clipped"], "fully");
}

#[test]
fn outside_safe_area() {
    let brand = brand();
    // Inside the canvas, but crossing the 80px margin.
    let layer = text_layer("Tight", Transform::new(20.0, 400.0, 400.0, 100.0));

    let document = document_with(vec![Layer::Text(layer)]);
    let report = validate(&clean_context(&document, &brand));

    assert_only(&report, "OUTSIDE_SAFE_AREA", Severity::Warning);
    assert!(report.valid);
}

// --- Text -----------------------------------------------------------------

#[test]
fn text_overflow_width() {
    let brand = brand();
    let layer = text_layer("Headline", Transform::new(100.0, 100.0, 400.0, 200.0));
    let id = layer.common.id;

    let document = document_with(vec![Layer::Text(layer)]);
    let context = clean_context(&document, &brand).with_measurement(
        id,
        MeasuredLayer::new(
            Rect::new(100.0, 100.0, 620.0, 120.0),
            Rect::new(100.0, 100.0, 620.0, 120.0),
        ),
    );

    let report = validate(&context);
    assert_only(&report, "TEXT_OVERFLOW_WIDTH", Severity::Error);
}

#[test]
fn text_overflow_height() {
    let brand = brand();
    let layer = text_layer("Body", Transform::new(100.0, 100.0, 800.0, 100.0));
    let id = layer.common.id;

    let document = document_with(vec![Layer::Text(layer)]);
    let context = clean_context(&document, &brand).with_measurement(
        id,
        MeasuredLayer::new(
            Rect::new(100.0, 100.0, 700.0, 260.0),
            Rect::new(100.0, 100.0, 700.0, 260.0),
        ),
    );

    let report = validate(&context);
    assert_only(&report, "TEXT_OVERFLOW_HEIGHT", Severity::Error);
}

/// Without measurements the check is skipped rather than guessed at.
#[test]
fn overflow_is_not_reported_without_measurements() {
    let brand = brand();
    let layer = text_layer("Headline", Transform::new(100.0, 100.0, 120.0, 40.0));

    let document = document_with(vec![Layer::Text(layer)]);
    let report = validate(&clean_context(&document, &brand));

    assert!(!report.has("TEXT_OVERFLOW_WIDTH"));
    assert!(!report.has("TEXT_OVERFLOW_HEIGHT"));
}

#[test]
fn font_not_found() {
    let brand = brand();
    let mut layer = text_layer("Headline", Transform::new(100.0, 100.0, 800.0, 200.0));
    layer.font_family = "Helvetica Neue".to_string();

    let document = document_with(vec![Layer::Text(layer)]);
    let report = validate(&clean_context(&document, &brand));
    assert_only(&report, "FONT_NOT_FOUND", Severity::Error);
}

#[test]
fn font_too_small() {
    let brand = brand();
    let mut layer = text_layer("Legal", Transform::new(100.0, 100.0, 800.0, 200.0));
    layer.font_size = 12.0;

    let document = document_with(vec![Layer::Text(layer)]);
    let report = validate(&clean_context(&document, &brand));
    assert_only(&report, "FONT_TOO_SMALL", Severity::Warning);
    assert!(report.valid);
}

#[test]
fn a_brand_font_token_resolves_instead_of_failing() {
    let brand = brand();
    let mut layer = text_layer("Headline", Transform::new(100.0, 100.0, 800.0, 200.0));
    layer.font_family = "$brand.display".to_string();

    let document = document_with(vec![Layer::Text(layer)]);
    let report = validate(&clean_context(&document, &brand));
    assert!(report.issues.is_empty(), "{:?}", report.issues);
}

// --- Assets ---------------------------------------------------------------

fn image_layer(name: &str, asset_id: design_core::AssetId) -> ImageLayer {
    ImageLayer {
        common: common(name, Transform::new(100.0, 100.0, 500.0, 500.0)),
        asset_id,
        fit: ImageFit::Cover,
        focal_point: Default::default(),
        overlay: None,
    }
}

#[test]
fn asset_not_found() {
    let brand = brand();
    let layer = image_layer("Hero", design_core::AssetId::new());

    let document = document_with(vec![Layer::Image(layer)]);
    let report = validate(&clean_context(&document, &brand));
    assert_only(&report, "ASSET_NOT_FOUND", Severity::Error);
}

#[test]
fn asset_unreadable() {
    let brand = brand();
    let asset = design_core::AssetId::new();
    let document = document_with(vec![Layer::Image(image_layer("Hero", asset))]);

    let context = clean_context(&document, &brand).with_asset(asset, AssetInfo::unreadable());
    let report = validate(&context);
    assert_only(&report, "ASSET_UNREADABLE", Severity::Error);
}

#[test]
fn invalid_asset_type() {
    let brand = brand();
    let asset = design_core::AssetId::new();
    let document = document_with(vec![Layer::Image(image_layer("Hero", asset))]);

    let context = clean_context(&document, &brand).with_asset(asset, AssetInfo::wrong_type());
    let report = validate(&context);
    assert_only(&report, "INVALID_ASSET_TYPE", Severity::Error);
}

#[test]
fn a_usable_asset_passes() {
    let brand = brand();
    let asset = design_core::AssetId::new();
    let document = document_with(vec![Layer::Image(image_layer("Hero", asset))]);

    let context = clean_context(&document, &brand).with_asset(asset, AssetInfo::usable());
    let report = validate(&context);
    assert!(report.issues.is_empty(), "{:?}", report.issues);
}

// --- Brand ----------------------------------------------------------------

#[test]
fn unknown_brand_token() {
    let brand = brand();
    let mut layer = text_layer("Headline", Transform::new(100.0, 100.0, 800.0, 200.0));
    layer.color = DesignColor::token("brand.headline");

    let document = document_with(vec![Layer::Text(layer)]);
    let report = validate(&clean_context(&document, &brand));
    assert_only(&report, "UNKNOWN_BRAND_TOKEN", Severity::Error);
}

/// Plan section 43: a literal is allowed, so this warns rather than rejects.
#[test]
fn color_outside_brand() {
    let brand = brand();
    let mut layer = text_layer("Headline", Transform::new(100.0, 100.0, 800.0, 200.0));
    layer.color = DesignColor::Literal(Color::rgb(0x12, 0x34, 0x56));

    let document = document_with(vec![Layer::Text(layer)]);
    let report = validate(&clean_context(&document, &brand));
    assert_only(&report, "COLOR_OUTSIDE_BRAND", Severity::Warning);
    assert!(report.valid, "an off-brand literal must not block export");
}

#[test]
fn a_literal_that_matches_the_palette_is_not_flagged() {
    let brand = brand();
    let mut layer = text_layer("Headline", Transform::new(100.0, 100.0, 800.0, 200.0));
    layer.color = DesignColor::Literal(brand.colors.primary);

    let document = document_with(vec![Layer::Text(layer)]);
    let report = validate(&clean_context(&document, &brand));
    assert!(report.issues.is_empty(), "{:?}", report.issues);
}

#[test]
fn missing_brand_font() {
    let mut brand = brand();
    brand.typography.mono = "Fira Code".to_string();

    let document = document_with(vec![Layer::Text(text_layer(
        "Headline",
        Transform::new(100.0, 100.0, 800.0, 200.0),
    ))]);
    let report = validate(&clean_context(&document, &brand));
    assert_only(&report, "MISSING_BRAND_FONT", Severity::Warning);
}

// --- Report behaviour -----------------------------------------------------

/// Without a Brand Kit the checks that need a palette or brand rules cannot run.
/// The ones that only need the document still do.
#[test]
fn checks_needing_a_palette_are_skipped_without_a_brand_kit() {
    let mut layer = text_layer("Headline", Transform::new(100.0, 100.0, 800.0, 200.0));
    layer.color = DesignColor::Literal(Color::rgb(0x12, 0x34, 0x56));
    layer.font_size = 8.0;

    let mut document = document_with(vec![Layer::Text(layer)]);
    // A literal background too, so nothing here depends on a token.
    document.pages[0].background = Some(DesignColor::Literal(Color::rgb(0x10, 0x10, 0x10)));

    let report = validate(&ValidationContext::new(&document).with_fonts(["DejaVu Sans"]));

    // No palette to be off, no safe margin, no minimum font size.
    assert!(!report.has("COLOR_OUTSIDE_BRAND"), "{:?}", report.issues);
    assert!(!report.has("OUTSIDE_SAFE_AREA"), "{:?}", report.issues);
    assert!(!report.has("FONT_TOO_SMALL"), "{:?}", report.issues);
    assert!(!report.has("MISSING_BRAND_FONT"), "{:?}", report.issues);
    assert!(report.valid, "{:?}", report.issues);
}

/// Regression: a design using $brand.* in a project with no Brand Kit used to
/// validate clean and then fail to render with UNKNOWN_BRAND_TOKEN, which broke
/// the validate-fix-render-export loop the agent depends on.
#[test]
fn a_color_token_without_a_brand_kit_is_an_error() {
    let mut layer = text_layer("Headline", Transform::new(100.0, 100.0, 800.0, 200.0));
    layer.color = DesignColor::token("brand.text");

    let mut document = document_with(vec![Layer::Text(layer)]);
    document.pages[0].background = None;

    let report = validate(&ValidationContext::new(&document).with_fonts(["DejaVu Sans"]));

    assert_only(&report, "UNKNOWN_BRAND_TOKEN", Severity::Error);
    assert!(
        !report.valid,
        "an unrenderable design must not report as valid"
    );
    assert!(
        report.issues[0].message.contains("no Brand Kit"),
        "the message should say why the token cannot resolve: {}",
        report.issues[0].message
    );
}

/// The two token paths must agree: it made no sense for a font token to be an
/// error without a Brand Kit while a color token was waved through.
#[test]
fn font_and_color_tokens_agree_when_there_is_no_brand_kit() {
    let mut with_font_token = text_layer("A", Transform::new(100.0, 100.0, 800.0, 200.0));
    with_font_token.font_family = "$brand.display".to_string();
    with_font_token.color = DesignColor::Literal(Color::rgb(0xFF, 0xFF, 0xFF));

    let mut with_color_token = text_layer("B", Transform::new(100.0, 400.0, 800.0, 200.0));
    with_color_token.color = DesignColor::token("brand.text");

    let mut document = document_with(vec![
        Layer::Text(with_font_token),
        Layer::Text(with_color_token),
    ]);
    document.pages[0].background = None;

    let report = validate(&ValidationContext::new(&document).with_fonts(["DejaVu Sans"]));

    let unresolved: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.code == "UNKNOWN_BRAND_TOKEN")
        .collect();
    assert_eq!(
        unresolved.len(),
        2,
        "both tokens must be reported: {:?}",
        report.issues
    );
    assert!(unresolved.iter().all(|i| i.severity == Severity::Error));
}

/// A page background is not a layer, so it needs its own coverage.
#[test]
fn a_background_token_without_a_brand_kit_is_an_error() {
    let mut layer = text_layer("Headline", Transform::new(100.0, 100.0, 800.0, 200.0));
    layer.color = DesignColor::Literal(Color::rgb(0xFF, 0xFF, 0xFF));

    let document = document_with(vec![Layer::Text(layer)]);
    // document_with sets a $brand.background page background.

    let report = validate(&ValidationContext::new(&document).with_fonts(["DejaVu Sans"]));
    assert_only(&report, "UNKNOWN_BRAND_TOKEN", Severity::Error);
    assert_eq!(report.issues[0].page_id, Some(document.pages[0].id));
    assert_eq!(report.issues[0].layer_id, None);
}

#[test]
fn every_problem_is_reported_not_just_the_first() {
    let brand = brand();
    let mut a = text_layer("A", Transform::new(100.0, 100.0, 800.0, 200.0));
    a.font_family = "Helvetica Neue".to_string();
    let mut b = text_layer("B", Transform::new(100.0, 400.0, 800.0, 200.0));
    b.font_size = 8.0;

    let document = document_with(vec![Layer::Text(a), Layer::Text(b)]);
    let report = validate(&clean_context(&document, &brand));

    assert!(report.has("FONT_NOT_FOUND"));
    assert!(report.has("FONT_TOO_SMALL"));
    assert_eq!(report.errors().count(), 1);
    assert_eq!(report.warnings().count(), 1);
}

#[test]
fn an_invisible_layer_is_not_judged_for_bounds() {
    let brand = brand();
    let mut layer = shape_layer("Hidden", Transform::new(5000.0, 5000.0, 100.0, 100.0));
    layer.common.visible = false;

    let document = document_with(vec![Layer::Shape(layer)]);
    let report = validate(&clean_context(&document, &brand));
    assert!(!report.has("OUTSIDE_CANVAS"), "{:?}", report.issues);
}

#[test]
fn painted_bounds_decide_the_canvas_check_not_the_frame() {
    let brand = brand();
    let layer = image_layer("Hero", design_core::AssetId::new());
    let id = layer.common.id;
    let asset = layer.asset_id;

    let document = document_with(vec![Layer::Image(layer)]);
    let context = clean_context(&document, &brand)
        .with_asset(asset, AssetInfo::usable())
        // A cover image whose layout overflows but whose painted area is clipped
        // back inside the frame must not be reported as leaving the canvas.
        .with_measurement(
            id,
            MeasuredLayer::new(
                Rect::new(-300.0, 100.0, 1400.0, 500.0),
                Rect::new(100.0, 100.0, 500.0, 500.0),
            ),
        );

    let report = validate(&context);
    assert!(!report.has("OUTSIDE_CANVAS"), "{:?}", report.issues);
}

/// Contrast arrives measured, exactly like bounds do. These pin the severities,
/// because severity is what decides whether an export is allowed (plan §42).
fn measured_with(ratio: f32, illegible_fraction: f32) -> MeasuredLayer {
    // Colours that are alike in hue as well as lightness. The cases where they
    // are not have their own test.
    measured_alike(ratio, illegible_fraction, 8.0)
}

fn measured_alike(ratio: f32, illegible_fraction: f32, delta_e: f32) -> MeasuredLayer {
    MeasuredLayer {
        layout: Rect::new(100.0, 100.0, 400.0, 60.0),
        painted: Rect::new(100.0, 100.0, 400.0, 60.0),
        contrast: Some(TextContrast {
            ratio,
            illegible_fraction,
            delta_e,
        }),
    }
}

#[test]
fn text_the_same_tone_as_its_backdrop_is_an_error() {
    let brand = brand();
    let layer = text_layer("Footer", Transform::new(100.0, 100.0, 800.0, 200.0));
    let id = layer.common.id;
    let document = document_with(vec![Layer::Text(layer)]);
    let context = clean_context(&document, &brand).with_measurement(id, measured_with(1.2, 1.0));

    assert_only(&validate(&context), "TEXT_LOW_CONTRAST", Severity::Error);
}

#[test]
fn text_with_little_contrast_is_a_warning() {
    let brand = brand();
    let layer = text_layer("Footer", Transform::new(100.0, 100.0, 800.0, 200.0));
    let id = layer.common.id;
    let document = document_with(vec![Layer::Text(layer)]);
    let context = clean_context(&document, &brand).with_measurement(id, measured_with(2.1, 0.9));

    assert_only(&validate(&context), "TEXT_LOW_CONTRAST", Severity::Warning);
}

/// The case a single ratio cannot see: comfortable on average, lost across most
/// of its own area.
#[test]
fn text_that_reads_on_average_but_not_over_most_of_itself_is_a_warning() {
    let brand = brand();
    let layer = text_layer("Headline", Transform::new(100.0, 100.0, 800.0, 200.0));
    let id = layer.common.id;
    let document = document_with(vec![Layer::Text(layer)]);
    let context = clean_context(&document, &brand).with_measurement(id, measured_with(6.0, 0.8));

    assert_only(&validate(&context), "TEXT_LOW_CONTRAST", Severity::Warning);
}

#[test]
fn text_that_reads_reports_nothing() {
    let brand = brand();
    let layer = text_layer("Headline", Transform::new(100.0, 100.0, 800.0, 200.0));
    let id = layer.common.id;
    let document = document_with(vec![Layer::Text(layer)]);
    let context = clean_context(&document, &brand).with_measurement(id, measured_with(9.4, 0.02));

    assert!(
        !validate(&context)
            .issues
            .iter()
            .any(|issue| issue.code == "TEXT_LOW_CONTRAST"),
        "a readable line must not be reported"
    );
}

/// Without a measurement the check is skipped, never approximated -- the same
/// rule the overflow checks follow.
#[test]
fn contrast_is_skipped_when_it_was_not_measured() {
    let brand = brand();
    let layer = text_layer("Headline", Transform::new(100.0, 100.0, 800.0, 200.0));
    let id = layer.common.id;
    let document = document_with(vec![Layer::Text(layer)]);
    let context = clean_context(&document, &brand).with_measurement(
        id,
        MeasuredLayer::new(
            Rect::new(100.0, 100.0, 400.0, 60.0),
            Rect::new(100.0, 100.0, 400.0, 60.0),
        ),
    );

    assert!(!validate(&context)
        .issues
        .iter()
        .any(|issue| issue.code == "TEXT_LOW_CONTRAST"));
}

/// The accent red on wet asphalt: nearly the same lightness as its backdrop, and
/// perfectly readable, because it breaks by hue. Reporting it would send an agent
/// to fix the one element on the cover that worked.
#[test]
fn a_colour_that_breaks_by_hue_is_not_reported() {
    let brand = brand();
    let layer = text_layer("Number", Transform::new(100.0, 100.0, 800.0, 200.0));
    let id = layer.common.id;
    let document = document_with(vec![Layer::Text(layer)]);
    // Measured from the real cover: ratio 1.09, ΔE 75.2. The illegible share is
    // low because the renderer's per-pixel test weighs colour as well, which
    // `renderer/tests/contrast.rs` pins separately.
    let context =
        clean_context(&document, &brand).with_measurement(id, measured_alike(1.09, 0.01, 75.2));

    assert!(
        !validate(&context)
            .issues
            .iter()
            .any(|issue| issue.code == "TEXT_LOW_CONTRAST"),
        "a hue break is contrast too"
    );
}

/// An image layer carried no colour at all until it gained an overlay. A token
/// the kit cannot resolve is fatal in the renderer, so an empty arm here would
/// let a design validate clean and then refuse to draw — the exact shape of the
/// font-token bug this project already paid for once.
#[test]
fn an_overlay_naming_an_unknown_token_is_reported() {
    let brand = brand();
    let mut layer = image_layer("Photo", design_core::AssetId::new());
    layer.overlay = Some(design_core::ImageOverlay {
        color: DesignColor::token("brand.nonesuch"),
        opacity: 0.6,
        from: Some(design_core::OverlayEdge::Bottom),
    });
    let id = layer.common.id;
    let document = document_with(vec![Layer::Image(layer)]);

    let report = validate(&clean_context(&document, &brand).with_asset(
        match &document.pages[0].layers[0] {
            Layer::Image(image) => image.asset_id,
            _ => unreachable!(),
        },
        AssetInfo::usable(),
    ));
    let found: Vec<_> = report
        .issues
        .iter()
        .filter(|issue| issue.code == "UNKNOWN_BRAND_TOKEN")
        .collect();
    assert_eq!(found.len(), 1, "{:?}", report.issues);
    assert_eq!(found[0].layer_id, Some(id));
}

/// The overlay carries its own opacity, and a non-finite one reaches the SVG as
/// the literal text "NaN".
#[test]
fn an_overlay_with_an_impossible_opacity_is_reported() {
    let brand = brand();
    let mut layer = image_layer("Photo", design_core::AssetId::new());
    layer.overlay = Some(design_core::ImageOverlay {
        color: DesignColor::token("brand.background"),
        opacity: f32::NAN,
        from: None,
    });
    let document = document_with(vec![Layer::Image(layer)]);

    assert!(validate(&clean_context(&document, &brand))
        .issues
        .iter()
        .any(|issue| issue.code == "INVALID_OPACITY"));
}
