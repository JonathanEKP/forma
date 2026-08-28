//! Image layers (plan sections 19 and 21).
//!
//! An `ImageLayer` carries only an `asset_id`, so the renderer is handed the
//! bytes by an `AssetSource` rather than resolving a path or a URL itself. That
//! keeps the renderer free of arbitrary network and filesystem access, and lets
//! the same document render identically locally and remotely.

use std::collections::HashMap;

use design_core::{AssetId, FocalPoint, ImageFit, Transform};

/// Decoded metadata plus the original bytes, which are embedded in the SVG.
#[derive(Debug, Clone)]
pub struct RasterAsset {
    pub bytes: Vec<u8>,
    pub mime_type: String,
    pub width: u32,
    pub height: u32,
}

impl RasterAsset {
    /// Detects the format from magic bytes rather than trusting a filename
    /// (plan section 44), and reads the intrinsic size from the header.
    ///
    /// Identification comes from the `assets` crate, which is also what ingest
    /// uses. A second implementation here is how SVG and WebP came to be
    /// accepted on upload and then unrecognised at draw time: usvg can render
    /// all four formats, but this side only knew two of them.
    pub fn from_bytes(bytes: Vec<u8>) -> Option<Self> {
        let probed = assets::probe(&bytes)?;

        // An SVG keeps its size in the document, not in a fixed-offset header,
        // so `probe` reports zero and the tree has to be read for it. Leaving
        // the zero would make `contain` and `cover` divide by it.
        let (width, height) = if probed.format == assets::Format::Svg {
            assets::svg::size(&bytes)?
        } else {
            (probed.width, probed.height)
        };

        Some(Self {
            mime_type: probed.format.mime().to_string(),
            width,
            height,
            bytes,
        })
    }
}

/// Where the renderer gets image bytes from. The server implements this over
/// its asset store; tests implement it over a map.
pub trait AssetSource {
    fn load(&self, id: AssetId) -> Option<&RasterAsset>;
}

/// A page with no image layers.
pub struct NoAssets;

impl AssetSource for NoAssets {
    fn load(&self, _id: AssetId) -> Option<&RasterAsset> {
        None
    }
}

#[derive(Debug, Default)]
pub struct InMemoryAssets {
    assets: HashMap<AssetId, RasterAsset>,
}

impl InMemoryAssets {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, id: AssetId, asset: RasterAsset) {
        self.assets.insert(id, asset);
    }

    pub fn len(&self) -> usize {
        self.assets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.assets.is_empty()
    }
}

impl AssetSource for InMemoryAssets {
    fn load(&self, id: AssetId) -> Option<&RasterAsset> {
        self.assets.get(&id)
    }
}

/// Where the image is drawn, and whether it needs clipping to the frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// True when the image overflows its frame and must be clipped.
    pub clip: bool,
}

/// Computes the placement for a fit mode.
///
/// `preserveAspectRatio` is deliberately not used: it only offers nine discrete
/// alignments, which cannot express a continuous focal point. The geometry is
/// computed here so `cover` can honour `focal_point` exactly.
pub fn place(
    frame: &Transform,
    intrinsic: (u32, u32),
    fit: ImageFit,
    focal: FocalPoint,
) -> Placement {
    let (iw, ih) = (intrinsic.0 as f32, intrinsic.1 as f32);
    if iw <= 0.0 || ih <= 0.0 {
        return Placement {
            x: frame.x,
            y: frame.y,
            width: frame.width,
            height: frame.height,
            clip: false,
        };
    }

    match fit {
        // Stretches to the frame; aspect ratio is deliberately not preserved.
        ImageFit::Fill => Placement {
            x: frame.x,
            y: frame.y,
            width: frame.width,
            height: frame.height,
            clip: false,
        },

        // Whole image visible inside the frame, centered. A focal point cannot
        // matter here: nothing is cropped away.
        ImageFit::Contain => {
            let scale = (frame.width / iw).min(frame.height / ih);
            let (w, h) = (iw * scale, ih * scale);
            Placement {
                x: frame.x + (frame.width - w) / 2.0,
                y: frame.y + (frame.height - h) / 2.0,
                width: w,
                height: h,
                clip: false,
            }
        }

        // Frame fully covered, overflow cropped. The focal point decides which
        // part survives: 0.5 crops evenly, 0 keeps the leading edge.
        ImageFit::Cover => {
            let scale = (frame.width / iw).max(frame.height / ih);
            let (w, h) = (iw * scale, ih * scale);
            Placement {
                x: frame.x - (w - frame.width) * focal.x.clamp(0.0, 1.0),
                y: frame.y - (h - frame.height) * focal.y.clamp(0.0, 1.0),
                width: w,
                height: h,
                clip: true,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> Transform {
        Transform::new(100.0, 200.0, 400.0, 400.0)
    }

    #[test]
    fn fill_stretches_to_the_frame() {
        let p = place(&frame(), (1600, 900), ImageFit::Fill, FocalPoint::default());
        assert_eq!((p.x, p.y, p.width, p.height), (100.0, 200.0, 400.0, 400.0));
        assert!(!p.clip);
    }

    #[test]
    fn contain_fits_entirely_and_centers() {
        let p = place(
            &frame(),
            (1600, 900),
            ImageFit::Contain,
            FocalPoint::default(),
        );
        assert_eq!(p.width, 400.0);
        assert_eq!(p.height, 225.0);
        assert!(p.y > frame().y, "should be centered vertically");
        assert!(p.width <= frame().width && p.height <= frame().height);
        assert!(!p.clip);
    }

    #[test]
    fn cover_fills_the_frame_and_clips() {
        let p = place(
            &frame(),
            (1600, 900),
            ImageFit::Cover,
            FocalPoint::default(),
        );
        assert!(p.width >= frame().width && p.height >= frame().height);
        assert!(p.clip);
        // Centered by default: equal overflow on both sides.
        let overflow = p.width - frame().width;
        assert!((p.x - (frame().x - overflow / 2.0)).abs() < 0.01);
    }

    #[test]
    fn cover_honours_the_focal_point() {
        let left = place(
            &frame(),
            (1600, 900),
            ImageFit::Cover,
            FocalPoint { x: 0.0, y: 0.5 },
        );
        let right = place(
            &frame(),
            (1600, 900),
            ImageFit::Cover,
            FocalPoint { x: 1.0, y: 0.5 },
        );

        // focal 0 keeps the left edge flush with the frame.
        assert!((left.x - frame().x).abs() < 0.01);
        // focal 1 pushes it so the right edges line up.
        assert!((right.x + right.width - (frame().x + frame().width)).abs() < 0.01);
        assert!(right.x < left.x);
    }

    #[test]
    fn reads_png_dimensions_from_the_header() {
        let bytes = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/assets/hero.png"
        ))
        .unwrap();
        let asset = RasterAsset::from_bytes(bytes).unwrap();
        assert_eq!(asset.mime_type, "image/png");
        assert_eq!((asset.width, asset.height), (1600, 900));
    }

    #[test]
    fn refuses_bytes_that_are_not_an_image() {
        assert!(RasterAsset::from_bytes(vec![0; 64]).is_none());
        assert!(RasterAsset::from_bytes(b"%PDF-1.7\n%\xE2\xE3\xCF\xD3".to_vec()).is_none());
        assert!(RasterAsset::from_bytes(Vec::new()).is_none());
    }

    /// usvg draws all four formats plan section 44 lists, so identification is
    /// what decides whether one is usable — not the raster/vector distinction.
    #[test]
    fn an_svg_is_an_image_and_reports_the_size_from_its_tree() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="320" height="240"><rect width="320" height="240" fill="#FF5A36"/></svg>"##;

        let asset = RasterAsset::from_bytes(svg.to_vec()).expect("an SVG is an image");
        assert_eq!(asset.mime_type, "image/svg+xml");
        // Zero here would make `contain` and `cover` fall back to `fill`.
        assert_eq!((asset.width, asset.height), (320, 240));
    }
}
