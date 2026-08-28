//! Phase 1 acceptance (plan section 77) and the semantics of section 28.
//!
//! The acceptance is: a JSON fixture must deserialize, accept operations and
//! serialize again with no semantic loss.

use design_core::{
    apply_operations, DesignColor, DesignDocument, DesignFormat, DesignOperation, FormatPreset,
    FramePatch, Layer, LayerId, Page, PageId, TextAlign, TextStylePatch, MAX_OPS_PER_BATCH,
};
use uuid::Uuid;

const FIXTURE: &str = include_str!("../../../fixtures/designs/carousel.json");

fn id(raw: &str) -> Uuid {
    Uuid::parse_str(raw).unwrap()
}

fn cover() -> PageId {
    PageId(id("11111111-1111-4111-8111-111111111111"))
}

fn headline() -> LayerId {
    LayerId(id("aaaaaaaa-0000-4000-8000-000000000002"))
}

fn accent_bar() -> LayerId {
    LayerId(id("aaaaaaaa-0000-4000-8000-000000000001"))
}

fn hero_image() -> LayerId {
    LayerId(id("bbbbbbbb-0000-4000-8000-000000000002"))
}

fn fixture() -> DesignDocument {
    DesignDocument::migrate(serde_json::from_str(FIXTURE).expect("fixture must be valid JSON"))
        .expect("fixture must match schema 1")
}

fn text_layer(document: &DesignDocument, layer_id: LayerId) -> &design_core::TextLayer {
    for page in &document.pages {
        for layer in &page.layers {
            if layer.id() == layer_id {
                match layer {
                    Layer::Text(t) => return t,
                    _ => panic!("layer {layer_id} is not text"),
                }
            }
        }
    }
    panic!("layer {layer_id} not found");
}

/// The acceptance itself: parse, mutate, serialize, and prove nothing was lost
/// or invented along the way.
#[test]
fn fixture_survives_a_deserialize_apply_serialize_round_trip() {
    let original = fixture();

    // A pure round trip must be lossless before operations enter the picture.
    let reparsed: DesignDocument =
        serde_json::from_str(&serde_json::to_string(&original).unwrap()).unwrap();
    assert_eq!(original, reparsed, "serialization lost or altered data");

    let operations = vec![
        DesignOperation::SetText {
            layer_id: headline(),
            content: "SHIP\nWITH AI".to_string(),
            font_size: Some(104.0),
        },
        DesignOperation::MoveLayer {
            layer_id: headline(),
            frame: FramePatch {
                x: Some(80.0),
                y: Some(200.0),
                ..Default::default()
            },
        },
        DesignOperation::PatchShapeStyle {
            layer_id: accent_bar(),
            patch: design_core::ShapeStylePatch {
                fill: Some(Some("$brand.accent".parse().unwrap())),
                ..Default::default()
            },
        },
    ];

    let updated = apply_operations(&original, &operations).expect("batch must apply");

    // The mutations landed.
    let text = text_layer(&updated, headline());
    assert_eq!(text.text, "SHIP\nWITH AI");
    assert_eq!(text.font_size, 104.0);
    assert_eq!(text.common.transform.y, 200.0);

    // Everything not addressed is untouched, including the other page.
    assert_eq!(updated.pages.len(), original.pages.len());
    assert_eq!(updated.pages[1], original.pages[1]);
    assert_eq!(text.font_family, "DejaVu Sans");
    assert_eq!(text.align, TextAlign::Left);

    // And the result still round-trips.
    let serialized = serde_json::to_string(&updated).unwrap();
    let reparsed: DesignDocument = serde_json::from_str(&serialized).unwrap();
    assert_eq!(updated, reparsed);
}

#[test]
fn tokens_and_literals_both_survive_the_round_trip() {
    let document = fixture();
    assert_eq!(
        document.pages[0].background,
        Some(DesignColor::token("brand.background"))
    );

    let updated = apply_operations(
        &document,
        &[DesignOperation::SetBackground {
            page_id: cover(),
            color: Some("#0A0A0A".parse().unwrap()),
        }],
    )
    .unwrap();

    let json = serde_json::to_value(&updated).unwrap();
    // A literal requested explicitly stays a literal (plan section 13).
    assert_eq!(json["pages"][0]["background"], "#0A0A0A");
    // And a token stays a token, unexpanded, so the document stays portable.
    assert_eq!(json["pages"][1]["background"], "$brand.surface");
}

/// Plan section 28: a patch aimed at the wrong type is an error, never a
/// silent no-op.
#[test]
fn rejects_a_patch_aimed_at_the_wrong_layer_type() {
    let document = fixture();
    let error = apply_operations(
        &document,
        &[DesignOperation::SetText {
            layer_id: accent_bar(),
            content: "nope".to_string(),
            font_size: None,
        }],
    )
    .unwrap_err();

    assert_eq!(error.code(), "INVALID_OP");
    assert!(error.to_string().contains("shape layer"), "{error}");
    assert!(!error.hint().is_empty());
}

#[test]
fn reports_which_operation_in_the_batch_failed() {
    let document = fixture();
    let error = apply_operations(
        &document,
        &[
            DesignOperation::SetText {
                layer_id: headline(),
                content: "fine".to_string(),
                font_size: None,
            },
            DesignOperation::SetImage {
                layer_id: headline(),
                asset_id: design_core::AssetId::new(),
                fit: None,
                focal_point: None,
            },
        ],
    )
    .unwrap_err();

    // Index and op name, so a rejected batch of twenty is actionable.
    assert!(error.to_string().contains("operation 1"), "{error}");
    assert!(error.to_string().contains("set_image"), "{error}");
}

/// Atomicity: a batch that fails halfway leaves nothing behind.
#[test]
fn a_failing_batch_changes_nothing() {
    let document = fixture();
    let before = serde_json::to_string(&document).unwrap();

    let error = apply_operations(
        &document,
        &[
            DesignOperation::SetText {
                layer_id: headline(),
                content: "half applied?".to_string(),
                font_size: None,
            },
            DesignOperation::RemoveLayer {
                layer_id: LayerId::new(),
            },
        ],
    )
    .unwrap_err();

    assert_eq!(error.code(), "LAYER_NOT_FOUND");
    assert_eq!(serde_json::to_string(&document).unwrap(), before);
}

#[test]
fn enforces_the_batch_size_limit() {
    let document = fixture();
    let op = DesignOperation::SetText {
        layer_id: headline(),
        content: "x".to_string(),
        font_size: None,
    };

    let at_limit = vec![op.clone(); MAX_OPS_PER_BATCH];
    assert!(apply_operations(&document, &at_limit).is_ok());

    let over = vec![op; MAX_OPS_PER_BATCH + 1];
    let error = apply_operations(&document, &over).unwrap_err();
    assert_eq!(error.code(), "LIMIT_EXCEEDED");
}

#[test]
fn rejects_an_empty_batch_rather_than_bumping_the_version_for_nothing() {
    let error = apply_operations(&fixture(), &[]).unwrap_err();
    assert_eq!(error.code(), "INVALID_OP");
}

#[test]
fn rejects_out_of_range_values_instead_of_clamping() {
    let document = fixture();

    for (label, operation) in [
        (
            "negative width",
            DesignOperation::MoveLayer {
                layer_id: headline(),
                frame: FramePatch {
                    width: Some(-10.0),
                    ..Default::default()
                },
            },
        ),
        (
            "zero font size",
            DesignOperation::SetText {
                layer_id: headline(),
                content: "x".to_string(),
                font_size: Some(0.0),
            },
        ),
        (
            "absurd font weight",
            DesignOperation::PatchTextStyle {
                layer_id: headline(),
                patch: TextStylePatch {
                    font_weight: Some(5000),
                    ..Default::default()
                },
            },
        ),
    ] {
        let error = match apply_operations(&document, &[operation]) {
            Err(error) => error,
            Ok(_) => panic!("{label} should have been rejected"),
        };
        assert!(
            matches!(error.code(), "INVALID_DIMENSION" | "INVALID_OP"),
            "{label} produced {}",
            error.code()
        );
    }
}

/// An empty patch expresses no intent, so it is rejected rather than quietly
/// producing a version identical to the previous one.
#[test]
fn rejects_patches_that_say_nothing() {
    let document = fixture();
    assert!(apply_operations(
        &document,
        &[DesignOperation::MoveLayer {
            layer_id: headline(),
            frame: FramePatch::default(),
        }]
    )
    .is_err());

    assert!(apply_operations(
        &document,
        &[DesignOperation::PatchTextStyle {
            layer_id: headline(),
            patch: TextStylePatch::default(),
        }]
    )
    .is_err());
}

#[test]
fn reordering_pages_requires_the_complete_order() {
    let document = fixture();

    let partial = apply_operations(
        &document,
        &[DesignOperation::ReorderPages {
            page_ids: vec![cover()],
        }],
    );
    assert_eq!(partial.unwrap_err().code(), "INVALID_OP");

    let full = apply_operations(
        &document,
        &[DesignOperation::ReorderPages {
            page_ids: vec![document.pages[1].id, document.pages[0].id],
        }],
    )
    .unwrap();
    assert_eq!(full.pages[0].id, document.pages[1].id);
    assert_eq!(full.pages[1].id, document.pages[0].id);
}

#[test]
fn layer_order_is_the_z_order() {
    let document = fixture();
    let updated = apply_operations(
        &document,
        &[DesignOperation::ReorderLayer {
            layer_id: headline(),
            index: 0,
        }],
    )
    .unwrap();

    assert_eq!(updated.pages[0].layers[0].id(), headline());
    assert_eq!(updated.pages[0].layers[1].id(), accent_bar());
}

#[test]
fn refuses_a_duplicate_layer_id() {
    let document = fixture();
    let clone = document.pages[0].layers[0].clone();

    let error = apply_operations(
        &document,
        &[DesignOperation::AddLayer {
            page_id: cover(),
            layer: clone,
            index: None,
        }],
    )
    .unwrap_err();
    assert_eq!(error.code(), "DUPLICATE_LAYER_ID");
}

#[test]
fn refuses_to_leave_a_design_with_no_pages() {
    let document = fixture();
    let error = apply_operations(
        &document,
        &[
            DesignOperation::RemovePage {
                page_id: document.pages[0].id,
            },
            DesignOperation::RemovePage {
                page_id: document.pages[1].id,
            },
        ],
    )
    .unwrap_err();
    assert_eq!(error.code(), "INVALID_DOCUMENT");
}

#[test]
fn set_image_keeps_the_asset_reference_portable() {
    let document = fixture();
    let asset = design_core::AssetId::new();
    let updated = apply_operations(
        &document,
        &[DesignOperation::SetImage {
            layer_id: hero_image(),
            asset_id: asset,
            fit: Some(design_core::ImageFit::Contain),
            focal_point: None,
        }],
    )
    .unwrap();

    let json = serde_json::to_value(&updated).unwrap();
    let layer = &json["pages"][1]["layers"][1];
    assert_eq!(layer["asset_id"], asset.to_string());
    // The portability rule of section 21: no URL, no path, ever.
    assert!(layer.get("url").is_none());
    assert!(!serde_json::to_string(&updated).unwrap().contains("http"));
}

#[test]
fn adding_a_page_and_a_layer_works_end_to_end() {
    let document = fixture();
    let mut page = Page::new("Outro");
    page.background = Some(DesignColor::token("brand.background"));
    let page_id = page.id;

    let layer = Layer::Text(design_core::TextLayer {
        common: design_core::LayerCommon {
            id: LayerId::new(),
            name: "CTA".to_string(),
            role: Some(design_core::LayerRole::Cta),
            visible: true,
            locked: false,
            opacity: 1.0,
            transform: design_core::Transform::new(80.0, 600.0, 920.0, 200.0),
        },
        text: "Follow for more".to_string(),
        font_family: "DejaVu Sans".to_string(),
        font_size: 64.0,
        font_weight: 700,
        line_height: 1.2,
        letter_spacing: 0.0,
        align: TextAlign::Left,
        color: DesignColor::token("brand.text"),
    });

    let updated = apply_operations(
        &document,
        &[
            DesignOperation::AddPage { page, index: None },
            DesignOperation::AddLayer {
                page_id,
                layer,
                index: None,
            },
        ],
    )
    .unwrap();

    assert_eq!(updated.pages.len(), 3);
    assert_eq!(updated.pages[2].layers.len(), 1);
    updated.validate_structure().unwrap();
}

#[test]
fn set_format_applies_to_the_whole_design() {
    let document = fixture();
    let updated = apply_operations(
        &document,
        &[DesignOperation::SetFormat {
            format: DesignFormat::from_preset(FormatPreset::Square),
        }],
    )
    .unwrap();

    assert_eq!(updated.format.width, 1080.0);
    assert_eq!(updated.format.height, 1080.0);
    // One canvas for every page of a carousel (plan section 14).
    assert_eq!(updated.pages.len(), 2);
}
