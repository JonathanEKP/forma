//! What the validator needs to know beyond the document itself.

use std::collections::{HashMap, HashSet};

use design_core::{AssetId, BrandKit, DesignDocument, LayerId, Rect};

/// Measurements the render authority produced for one layer.
///
/// The validator never measures text itself: shaping belongs to the renderer
/// (plan section 35), and a second implementation would drift from it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeasuredLayer {
    /// Where the geometry claims to be, before clipping. For text this spans
    /// the line boxes, which is what an overflow check must compare.
    pub layout: Rect,
    /// What survives clipping and actually reaches the raster.
    pub painted: Rect,
}

/// What is known about an asset the document references.
#[derive(Debug, Clone, PartialEq)]
pub struct AssetInfo {
    pub readable: bool,
    /// Whether this asset can be drawn by an `ImageLayer` at all.
    pub raster: bool,
}

impl AssetInfo {
    pub fn usable() -> Self {
        Self {
            readable: true,
            raster: true,
        }
    }

    pub fn unreadable() -> Self {
        Self {
            readable: false,
            raster: true,
        }
    }

    pub fn wrong_type() -> Self {
        Self {
            readable: true,
            raster: false,
        }
    }
}

pub struct ValidationContext<'a> {
    pub document: &'a DesignDocument,
    /// Absent when a project has no Brand Kit yet: brand checks are then skipped
    /// rather than reported as failures.
    pub brand: Option<&'a BrandKit>,
    /// Families the renderer can actually draw, from the FontRegistry.
    pub fonts: HashSet<String>,
    pub assets: HashMap<AssetId, AssetInfo>,
    /// Absent when the caller has not rendered the design. Text overflow is then
    /// not reported, rather than guessed at.
    pub measurements: HashMap<LayerId, MeasuredLayer>,
}

impl<'a> ValidationContext<'a> {
    pub fn new(document: &'a DesignDocument) -> Self {
        Self {
            document,
            brand: None,
            fonts: HashSet::new(),
            assets: HashMap::new(),
            measurements: HashMap::new(),
        }
    }

    pub fn with_brand(mut self, brand: &'a BrandKit) -> Self {
        self.brand = Some(brand);
        self
    }

    pub fn with_fonts<I, S>(mut self, families: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.fonts = families.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_asset(mut self, id: AssetId, info: AssetInfo) -> Self {
        self.assets.insert(id, info);
        self
    }

    pub fn with_measurement(mut self, layer: LayerId, measured: MeasuredLayer) -> Self {
        self.measurements.insert(layer, measured);
        self
    }

    pub fn canvas(&self) -> Rect {
        Rect::new(
            0.0,
            0.0,
            self.document.format.width,
            self.document.format.height,
        )
    }

    /// The area critical content should stay within (plan section 13).
    pub fn safe_area(&self) -> Option<Rect> {
        let margin = self.brand?.rules.safe_margin;
        let canvas = self.canvas();
        if margin <= 0.0 || margin * 2.0 >= canvas.width || margin * 2.0 >= canvas.height {
            return None;
        }
        Some(Rect::new(
            margin,
            margin,
            canvas.width - margin * 2.0,
            canvas.height - margin * 2.0,
        ))
    }
}
