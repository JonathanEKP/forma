//! Design operations (plan section 28).
//!
//! Mutations express intent and are checked against the real type of the target.
//! There is deliberately no `UpdateLayer { arbitrary_json }`: a patch aimed at
//! the wrong layer type returns `INVALID_OP` instead of becoming a silent no-op.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::color::DesignColor;
use crate::document::{DesignDocument, DesignFormat, FormatPreset};
use crate::error::DomainError;
use crate::ids::{AssetId, LayerId, PageId};
use crate::layer::{FocalPoint, ImageFit, Layer, TextAlign};
use crate::page::Page;

/// Plan section 28. Not a renderer limitation: an operational limit that keeps
/// payloads, errors and retries manageable.
pub const MAX_OPS_PER_BATCH: usize = 20;

/// A partial frame update. Absent fields are left untouched.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FramePatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation_deg: Option<f32>,
}

impl FramePatch {
    fn is_empty(&self) -> bool {
        self.x.is_none()
            && self.y.is_none()
            && self.width.is_none()
            && self.height.is_none()
            && self.rotation_deg.is_none()
    }
}

/// A partial text style update.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TextStylePatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_weight: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_height: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub letter_spacing: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<TextAlign>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<DesignColor>,
}

impl TextStylePatch {
    fn is_empty(&self) -> bool {
        self.font_family.is_none()
            && self.font_size.is_none()
            && self.font_weight.is_none()
            && self.line_height.is_none()
            && self.letter_spacing.is_none()
            && self.align.is_none()
            && self.color.is_none()
    }
}

/// A partial shape style update.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ShapeStylePatch {
    /// `Some(None)` clears the fill; absent leaves it untouched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<Option<DesignColor>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke: Option<Option<DesignColor>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke_width: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corner_radius: Option<f32>,
}

impl ShapeStylePatch {
    fn is_empty(&self) -> bool {
        self.fill.is_none()
            && self.stroke.is_none()
            && self.stroke_width.is_none()
            && self.corner_radius.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "op", rename_all = "snake_case")]
#[ts(export)]
pub enum DesignOperation {
    AddPage {
        page: Page,
        /// Appended when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        index: Option<usize>,
    },
    RemovePage {
        page_id: PageId,
    },
    /// The full new order. A partial list is rejected, so the result is never
    /// ambiguous.
    ReorderPages {
        page_ids: Vec<PageId>,
    },
    SetBackground {
        page_id: PageId,
        /// `None` clears the background.
        #[serde(default)]
        color: Option<DesignColor>,
    },
    AddLayer {
        page_id: PageId,
        layer: Layer,
        /// Appended on top when absent; layer order is the z-order.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        index: Option<usize>,
    },
    RemoveLayer {
        layer_id: LayerId,
    },
    MoveLayer {
        layer_id: LayerId,
        frame: FramePatch,
    },
    ReorderLayer {
        layer_id: LayerId,
        index: usize,
    },
    SetText {
        layer_id: LayerId,
        content: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        font_size: Option<f32>,
    },
    PatchTextStyle {
        layer_id: LayerId,
        #[serde(flatten)]
        patch: TextStylePatch,
    },
    SetImage {
        layer_id: LayerId,
        asset_id: AssetId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fit: Option<ImageFit>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        focal_point: Option<FocalPoint>,
    },
    PatchShapeStyle {
        layer_id: LayerId,
        #[serde(flatten)]
        patch: ShapeStylePatch,
    },
    SetFormat {
        format: DesignFormat,
    },
}

impl DesignOperation {
    /// The `op` discriminant, for logs and error messages.
    pub fn name(&self) -> &'static str {
        match self {
            Self::AddPage { .. } => "add_page",
            Self::RemovePage { .. } => "remove_page",
            Self::ReorderPages { .. } => "reorder_pages",
            Self::SetBackground { .. } => "set_background",
            Self::AddLayer { .. } => "add_layer",
            Self::RemoveLayer { .. } => "remove_layer",
            Self::MoveLayer { .. } => "move_layer",
            Self::ReorderLayer { .. } => "reorder_layer",
            Self::SetText { .. } => "set_text",
            Self::PatchTextStyle { .. } => "patch_text_style",
            Self::SetImage { .. } => "set_image",
            Self::PatchShapeStyle { .. } => "patch_shape_style",
            Self::SetFormat { .. } => "set_format",
        }
    }
}

fn finite(value: f32, what: &str) -> Result<f32, DomainError> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(DomainError::InvalidDimension(format!(
            "{what} must be a finite number, got {value}"
        )))
    }
}

fn positive(value: f32, what: &str) -> Result<f32, DomainError> {
    let value = finite(value, what)?;
    if value > 0.0 {
        Ok(value)
    } else {
        Err(DomainError::InvalidDimension(format!(
            "{what} must be greater than zero, got {value}"
        )))
    }
}

fn non_negative(value: f32, what: &str) -> Result<f32, DomainError> {
    let value = finite(value, what)?;
    if value >= 0.0 {
        Ok(value)
    } else {
        Err(DomainError::InvalidDimension(format!(
            "{what} must not be negative, got {value}"
        )))
    }
}

/// Applies a batch atomically (plan section 28).
///
/// The batch is size-checked, applied in order to a copy, then validated
/// structurally. The caller receives a new document only if every step
/// succeeded, so a failing operation can never leave a half-applied design
/// behind. The version increment belongs to the caller: one batch, one version.
pub fn apply_operations(
    document: &DesignDocument,
    operations: &[DesignOperation],
) -> Result<DesignDocument, DomainError> {
    if operations.is_empty() {
        return Err(DomainError::invalid_op(
            "an empty batch would create a new version with no changes",
            "Send at least one operation, or do not call apply_operations at all.",
        ));
    }
    if operations.len() > MAX_OPS_PER_BATCH {
        return Err(DomainError::LimitExceeded(format!(
            "{} operations exceeds the maximum of {MAX_OPS_PER_BATCH} per batch",
            operations.len()
        )));
    }

    let mut draft = document.clone();
    for (index, operation) in operations.iter().enumerate() {
        apply_one(&mut draft, operation).map_err(|e| annotate(e, index, operation))?;
    }
    draft.validate_structure()?;
    Ok(draft)
}

/// Names the failing operation, so a rejected batch of twenty is actionable.
///
/// Every failure is wrapped, not just the malformed ones: a batch rejected for
/// naming a page that does not exist is exactly as hard to debug as one
/// rejected for a bad patch, and the caller cannot tell which of twenty
/// operations carried the bad id. The wrapper keeps the original code and hint,
/// so what a client branches on is unchanged.
fn annotate(error: DomainError, index: usize, operation: &DesignOperation) -> DomainError {
    DomainError::InOperation {
        index,
        op: operation.name(),
        source: Box::new(error),
    }
}

fn page_index(document: &DesignDocument, page_id: PageId) -> Result<usize, DomainError> {
    document
        .pages
        .iter()
        .position(|p| p.id == page_id)
        .ok_or(DomainError::PageNotFound(page_id))
}

/// Locates a layer anywhere in the document. Layer ids are unique across pages,
/// which `validate_structure` enforces.
fn locate_layer(
    document: &mut DesignDocument,
    layer_id: LayerId,
) -> Result<(usize, usize), DomainError> {
    for (p, page) in document.pages.iter().enumerate() {
        if let Some(l) = page.layers.iter().position(|layer| layer.id() == layer_id) {
            return Ok((p, l));
        }
    }
    Err(DomainError::LayerNotFound(layer_id))
}

fn apply_one(
    document: &mut DesignDocument,
    operation: &DesignOperation,
) -> Result<(), DomainError> {
    match operation {
        DesignOperation::AddPage { page, index } => {
            if document.pages.iter().any(|p| p.id == page.id) {
                return Err(DomainError::invalid_op(
                    format!("page {} already exists", page.id),
                    "Generate a fresh page id rather than reusing an existing one.",
                ));
            }
            let at =
                match index {
                    Some(i) if *i > document.pages.len() => return Err(DomainError::invalid_op(
                        format!(
                            "index {i} is past the end of {} pages",
                            document.pages.len()
                        ),
                        "Use an index between 0 and the current page count, or omit it to append.",
                    )),
                    Some(i) => *i,
                    None => document.pages.len(),
                };
            document.pages.insert(at, page.clone());
        }

        DesignOperation::RemovePage { page_id } => {
            let at = page_index(document, *page_id)?;
            document.pages.remove(at);
        }

        DesignOperation::ReorderPages { page_ids } => {
            let mut current: Vec<PageId> = document.pages.iter().map(|p| p.id).collect();
            let mut requested = page_ids.clone();
            current.sort_by_key(|id| id.0);
            requested.sort_by_key(|id| id.0);
            if current != requested {
                return Err(DomainError::invalid_op(
                    "reorder_pages must list every existing page exactly once",
                    "Send the complete new order, not a subset.",
                ));
            }
            let mut reordered = Vec::with_capacity(page_ids.len());
            for id in page_ids {
                let at = page_index(document, *id)?;
                reordered.push(document.pages[at].clone());
            }
            document.pages = reordered;
        }

        DesignOperation::SetBackground { page_id, color } => {
            let at = page_index(document, *page_id)?;
            document.pages[at].background = color.clone();
        }

        DesignOperation::AddLayer {
            page_id,
            layer,
            index,
        } => {
            let at = page_index(document, *page_id)?;
            let id = layer.id();
            if document
                .pages
                .iter()
                .any(|p| p.layers.iter().any(|l| l.id() == id))
            {
                return Err(DomainError::DuplicateLayerId(id));
            }
            validate_layer(layer)?;

            let layers = &mut document.pages[at].layers;
            let position = match index {
                Some(i) if *i > layers.len() => return Err(DomainError::invalid_op(
                    format!("index {i} is past the end of {} layers", layers.len()),
                    "Use an index between 0 and the current layer count, or omit it to add on top.",
                )),
                Some(i) => *i,
                None => layers.len(),
            };
            layers.insert(position, layer.clone());
        }

        DesignOperation::RemoveLayer { layer_id } => {
            let (p, l) = locate_layer(document, *layer_id)?;
            document.pages[p].layers.remove(l);
        }

        DesignOperation::MoveLayer { layer_id, frame } => {
            if frame.is_empty() {
                return Err(DomainError::invalid_op(
                    "move_layer received an empty frame",
                    "Set at least one of x, y, width, height or rotation_deg.",
                ));
            }
            let (p, l) = locate_layer(document, *layer_id)?;
            let transform = &mut document.pages[p].layers[l].common_mut().transform;
            if let Some(x) = frame.x {
                transform.x = finite(x, "x")?;
            }
            if let Some(y) = frame.y {
                transform.y = finite(y, "y")?;
            }
            if let Some(width) = frame.width {
                transform.width = positive(width, "width")?;
            }
            if let Some(height) = frame.height {
                transform.height = positive(height, "height")?;
            }
            if let Some(rotation) = frame.rotation_deg {
                transform.rotation_deg = finite(rotation, "rotation_deg")?;
            }
        }

        DesignOperation::ReorderLayer { layer_id, index } => {
            let (p, from) = locate_layer(document, *layer_id)?;
            let layers = &mut document.pages[p].layers;
            if *index >= layers.len() {
                return Err(DomainError::invalid_op(
                    format!("index {index} is out of range for {} layers", layers.len()),
                    "Valid indices run from 0 (bottom) to layer count minus one (top).",
                ));
            }
            let layer = layers.remove(from);
            layers.insert(*index, layer);
        }

        DesignOperation::SetText {
            layer_id,
            content,
            font_size,
        } => {
            let (p, l) = locate_layer(document, *layer_id)?;
            let layer = &mut document.pages[p].layers[l];
            let Layer::Text(text) = layer else {
                return Err(wrong_type("set_text", "text", layer));
            };
            text.text = content.clone();
            if let Some(size) = font_size {
                text.font_size = positive(*size, "font_size")?;
            }
        }

        DesignOperation::PatchTextStyle { layer_id, patch } => {
            if patch.is_empty() {
                return Err(DomainError::invalid_op(
                    "patch_text_style received no properties",
                    "Set at least one style property, or drop the operation.",
                ));
            }
            let (p, l) = locate_layer(document, *layer_id)?;
            let layer = &mut document.pages[p].layers[l];
            let Layer::Text(text) = layer else {
                return Err(wrong_type("patch_text_style", "text", layer));
            };
            if let Some(family) = &patch.font_family {
                if family.trim().is_empty() {
                    return Err(DomainError::invalid_op(
                        "font_family must not be empty",
                        "Use a family from the font catalog returned by get_brand.",
                    ));
                }
                text.font_family = family.clone();
            }
            if let Some(size) = patch.font_size {
                text.font_size = positive(size, "font_size")?;
            }
            if let Some(weight) = patch.font_weight {
                if !(1..=1000).contains(&weight) {
                    return Err(DomainError::invalid_op(
                        format!("font_weight {weight} is outside the CSS range 1-1000"),
                        "Use a weight between 1 and 1000, typically 400 or 700.",
                    ));
                }
                text.font_weight = weight;
            }
            if let Some(line_height) = patch.line_height {
                text.line_height = positive(line_height, "line_height")?;
            }
            if let Some(spacing) = patch.letter_spacing {
                text.letter_spacing = finite(spacing, "letter_spacing")?;
            }
            if let Some(align) = patch.align {
                text.align = align;
            }
            if let Some(color) = &patch.color {
                text.color = color.clone();
            }
        }

        DesignOperation::SetImage {
            layer_id,
            asset_id,
            fit,
            focal_point,
        } => {
            let (p, l) = locate_layer(document, *layer_id)?;
            let layer = &mut document.pages[p].layers[l];
            let Layer::Image(image) = layer else {
                return Err(wrong_type("set_image", "image", layer));
            };
            image.asset_id = *asset_id;
            if let Some(fit) = fit {
                image.fit = *fit;
            }
            if let Some(point) = focal_point {
                if !(0.0..=1.0).contains(&point.x) || !(0.0..=1.0).contains(&point.y) {
                    return Err(DomainError::invalid_op(
                        format!(
                            "focal_point ({}, {}) is outside the image",
                            point.x, point.y
                        ),
                        "Focal point coordinates are fractions between 0 and 1.",
                    ));
                }
                image.focal_point = *point;
            }
        }

        DesignOperation::PatchShapeStyle { layer_id, patch } => {
            if patch.is_empty() {
                return Err(DomainError::invalid_op(
                    "patch_shape_style received no properties",
                    "Set at least one style property, or drop the operation.",
                ));
            }
            let (p, l) = locate_layer(document, *layer_id)?;
            let layer = &mut document.pages[p].layers[l];
            let Layer::Shape(shape) = layer else {
                return Err(wrong_type("patch_shape_style", "shape", layer));
            };
            if let Some(fill) = &patch.fill {
                shape.fill = fill.clone();
            }
            if let Some(stroke) = &patch.stroke {
                shape.stroke = stroke.clone();
            }
            if let Some(width) = patch.stroke_width {
                shape.stroke_width = non_negative(width, "stroke_width")?;
            }
            if let Some(radius) = patch.corner_radius {
                shape.corner_radius = non_negative(radius, "corner_radius")?;
            }
        }

        DesignOperation::SetFormat { format } => {
            format.validate()?;
            document.format = *format;
        }
    }
    Ok(())
}

fn wrong_type(op: &'static str, expected: &'static str, layer: &Layer) -> DomainError {
    let actual = match layer {
        Layer::Text(_) => "text",
        Layer::Image(_) => "image",
        Layer::Shape(_) => "shape",
    };
    DomainError::invalid_op(
        format!(
            "{op} targets a {expected} layer, but {} is a {actual} layer",
            layer.id()
        ),
        "Check the layer type in get_design before patching it; Forma does not silently ignore mismatched patches.",
    )
}

fn validate_layer(layer: &Layer) -> Result<(), DomainError> {
    let common = layer.common();
    if !(0.0..=1.0).contains(&common.opacity) {
        return Err(DomainError::InvalidOpacity(format!(
            "opacity {} is outside 0..=1",
            common.opacity
        )));
    }
    finite(common.transform.x, "x")?;
    finite(common.transform.y, "y")?;
    positive(common.transform.width, "width")?;
    positive(common.transform.height, "height")?;
    finite(common.transform.rotation_deg, "rotation_deg")?;

    match layer {
        Layer::Text(text) => {
            positive(text.font_size, "font_size")?;
            positive(text.line_height, "line_height")?;
            finite(text.letter_spacing, "letter_spacing")?;
        }
        Layer::Shape(shape) => {
            non_negative(shape.stroke_width, "stroke_width")?;
            non_negative(shape.corner_radius, "corner_radius")?;
        }
        Layer::Image(_) => {}
    }
    Ok(())
}

/// Used by `FormatPreset` consumers that build a format by hand.
impl DesignFormat {
    pub fn custom(width: f32, height: f32) -> Result<Self, DomainError> {
        let format = Self {
            preset: FormatPreset::Custom,
            width,
            height,
        };
        format.validate()?;
        Ok(format)
    }
}
