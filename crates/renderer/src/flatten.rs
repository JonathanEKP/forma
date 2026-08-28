//! usvg normalization and inspection.
//!
//! This module answers the blocking question of plan section 36: whether the
//! `<g id="layer-...">` wrappers survive usvg's simplification (Option A) or
//! have to be re-applied by Forma afterwards (Option B).

use design_core::{LayerId, Rect};

use crate::error::RenderError;
use crate::fonts::FontRegistry;
use crate::svg::LAYER_ID_PREFIX;

fn rect_of(bbox: usvg::Rect) -> Rect {
    Rect {
        x: bbox.x(),
        y: bbox.y(),
        width: bbox.width(),
        height: bbox.height(),
    }
}

/// Bounds of one layer as laid out by usvg, in canvas coordinates.
///
/// Two rectangles, because they answer different questions and routinely
/// disagree. Measured on the Phase 0 fixture, a two-line 140px headline reports
/// a 628x310 layout box and a 611x249 painted box; a `cover` image reports a
/// layout box hundreds of pixels wider than its painted box.
///
/// - `layout` is usvg's object bounding box: for text it spans the line boxes,
///   including leading and the full ascender/descender band. This is what a
///   TEXT_OVERFLOW_* check of section 42 should compare against a layer frame,
///   because that is the space the text actually claims.
/// - `painted` is what survives clipping and is what actually reaches the
///   raster. An OUTSIDE_CANVAS check must use this one: a `cover` image is
///   *meant* to overflow its frame, and judging it by its layout box would flag
///   every full-bleed photo as an error.
///
/// Note that usvg's own `layer_bounding_box` accounts for filters but not for
/// clip paths, so `painted` is computed here by intersecting with the clip.
#[derive(Debug, Clone, PartialEq)]
pub struct LayerBounds {
    pub layer_id: String,
    pub layout: Rect,
    pub painted: Rect,
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
        font_resolver: usvg::FontResolver {
            select_fallback: crate::fonts::fallback_selector(),
            ..Default::default()
        },
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
            let layout = rect_of(group.abs_bounding_box());

            // usvg reports the layer box with filters applied but without the
            // clip, so the clip is intersected in explicitly.
            let mut painted = rect_of(group.abs_layer_bounding_box().to_rect());
            if let Some(clip) = group.clip_path() {
                let clip_box = rect_of(clip.root().abs_bounding_box());
                painted = painted.intersect(&clip_box).unwrap_or(Rect {
                    x: clip_box.x,
                    y: clip_box.y,
                    width: 0.0,
                    height: 0.0,
                });
            }

            out.push(LayerBounds {
                layer_id: id.to_string(),
                layout,
                painted,
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
