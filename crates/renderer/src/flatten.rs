//! usvg normalization and inspection.
//!
//! This module answers the blocking question of plan section 36: whether the
//! `<g id="layer-...">` wrappers survive usvg's simplification (Option A) or
//! have to be re-applied by Forma afterwards (Option B).

use design_core::LayerId;

use crate::error::RenderError;
use crate::fonts::FontRegistry;
use crate::svg::LAYER_ID_PREFIX;

/// Bounds of one layer as actually laid out by usvg, in canvas coordinates.
/// For text these are real shaped-glyph bounds, which is what makes
/// TEXT_OVERFLOW_* checks possible without a browser (plan section 35).
#[derive(Debug, Clone, PartialEq)]
pub struct LayerBounds {
    pub layer_id: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl LayerBounds {
    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }
}

#[derive(Debug, Clone)]
pub struct RenderInspection {
    pub canvas_width: f32,
    pub canvas_height: f32,
    pub layers: Vec<LayerBounds>,
    /// True when every authored layer group was still addressable by id after
    /// usvg normalization.
    pub ids_survived_normalization: bool,
}

impl RenderInspection {
    pub fn layer(&self, id: LayerId) -> Option<&LayerBounds> {
        let needle = format!("{LAYER_ID_PREFIX}{id}");
        self.layers.iter().find(|b| b.layer_id == needle)
    }
}

pub fn parse(svg: &str, fonts: &FontRegistry) -> Result<usvg::Tree, RenderError> {
    let mut options = usvg::Options {
        fontdb: fonts.db(),
        ..Default::default()
    };
    // Nothing may be pulled off disk or the network while parsing a design.
    options.resources_dir = None;

    Ok(usvg::Tree::from_str(svg, &options)?)
}

/// Serializes the normalized tree. Text becomes paths, which is what removes the
/// dependency on browser font layout for the editor preview.
pub fn to_normalized_svg(tree: &usvg::Tree) -> String {
    tree.to_string(&usvg::WriteOptions {
        preserve_text: false,
        ..Default::default()
    })
}

fn collect(node: &usvg::Node, out: &mut Vec<LayerBounds>) {
    if let usvg::Node::Group(group) = node {
        let id = group.id();
        if id.starts_with(LAYER_ID_PREFIX) {
            let bbox = group.abs_bounding_box();
            out.push(LayerBounds {
                layer_id: id.to_string(),
                x: bbox.x(),
                y: bbox.y(),
                width: bbox.width(),
                height: bbox.height(),
            });
        }
        for child in group.children() {
            collect(child, out);
        }
    }
}

pub fn inspect(tree: &usvg::Tree, expected_layer_ids: &[String]) -> RenderInspection {
    let mut layers = Vec::new();
    for child in tree.root().children() {
        collect(child, &mut layers);
    }

    let ids_survived_normalization = expected_layer_ids
        .iter()
        .all(|id| layers.iter().any(|b| &b.layer_id == id));

    RenderInspection {
        canvas_width: tree.size().width(),
        canvas_height: tree.size().height(),
        layers,
        ids_survived_normalization,
    }
}
