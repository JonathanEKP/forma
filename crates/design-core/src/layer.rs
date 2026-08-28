//! Layers (plan sections 16 to 20). No GroupLayer in V1.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::color::DesignColor;
use crate::geometry::Transform;
use crate::ids::{AssetId, LayerId};

/// Optional semantic metadata (plan section 17). It does not affect the renderer
/// by itself, but lets templates and tooling address layers by meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum LayerRole {
    Headline,
    Kicker,
    Body,
    Cta,
    Stat,
    Photo,
    Logo,
    Decoration,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LayerCommon {
    pub id: LayerId,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<LayerRole>,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    pub transform: Transform,
}

fn default_true() -> bool {
    true
}

fn default_opacity() -> f32 {
    1.0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

/// Plan section 18. V1 does no implicit word-wrap: line breaks are explicit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TextLayer {
    #[serde(flatten)]
    pub common: LayerCommon,
    pub text: String,
    pub font_family: String,
    pub font_size: f32,
    #[serde(default = "default_font_weight")]
    pub font_weight: u16,
    #[serde(default = "default_line_height")]
    pub line_height: f32,
    #[serde(default)]
    pub letter_spacing: f32,
    #[serde(default = "default_align")]
    pub align: TextAlign,
    pub color: DesignColor,
}

fn default_font_weight() -> u16 {
    400
}

fn default_line_height() -> f32 {
    1.2
}

fn default_align() -> TextAlign {
    TextAlign::Left
}

impl TextLayer {
    /// V1 wrapping is explicit, so the line count is known without shaping.
    pub fn lines(&self) -> impl Iterator<Item = &str> {
        self.text.split('\n')
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ImageFit {
    Contain,
    Cover,
    Fill,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FocalPoint {
    pub x: f32,
    pub y: f32,
}

impl Default for FocalPoint {
    fn default() -> Self {
        Self { x: 0.5, y: 0.5 }
    }
}

/// Plan section 19 and the portability rule in section 21: only an `asset_id`,
/// never a URL or a local path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ImageLayer {
    #[serde(flatten)]
    pub common: LayerCommon,
    pub asset_id: AssetId,
    pub fit: ImageFit,
    #[serde(default)]
    pub focal_point: FocalPoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ShapeKind {
    Rectangle,
    Ellipse,
    Line,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ShapeLayer {
    #[serde(flatten)]
    pub common: LayerCommon,
    pub kind: ShapeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<DesignColor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke: Option<DesignColor>,
    #[serde(default)]
    pub stroke_width: f32,
    /// Only applies to `ShapeKind::Rectangle`.
    #[serde(default)]
    pub corner_radius: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum Layer {
    Text(TextLayer),
    Image(ImageLayer),
    Shape(ShapeLayer),
}

impl Layer {
    pub fn common(&self) -> &LayerCommon {
        match self {
            Self::Text(l) => &l.common,
            Self::Image(l) => &l.common,
            Self::Shape(l) => &l.common,
        }
    }

    pub fn common_mut(&mut self) -> &mut LayerCommon {
        match self {
            Self::Text(l) => &mut l.common,
            Self::Image(l) => &mut l.common,
            Self::Shape(l) => &mut l.common,
        }
    }

    pub fn id(&self) -> LayerId {
        self.common().id
    }

    /// The `type` discriminant, used in error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Text(_) => "text",
            Self::Image(_) => "image",
            Self::Shape(_) => "shape",
        }
    }
}
