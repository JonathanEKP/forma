//! SVG sanitization (plan section 45).
//!
//! An SVG is a document, not an image: it can carry scripts, event handlers and
//! references that fetch from the network. The bytes a user uploaded are
//! therefore never stored or served as they arrived.
//!
//! The pipeline is the one the plan draws: parse with usvg, with external
//! resource loading disabled, and re-serialize the tree. What comes out is a
//! static picture built only from what usvg understood — scripts and handlers
//! are not part of its model, so they cannot survive the round trip. Anything
//! that will not parse is refused rather than stored unexamined.

use crate::error::AssetError;

#[derive(Debug, Clone)]
pub struct Sanitized {
    /// The canonical bytes to store, not the ones that were uploaded.
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

fn parse(bytes: &[u8]) -> Result<usvg::Tree, AssetError> {
    let options = usvg::Options {
        // Nothing may be pulled off disk while parsing an upload. A relative
        // href in a hostile file would otherwise read a local path.
        resources_dir: None,
        ..Default::default()
    };
    usvg::Tree::from_data(bytes, &options).map_err(|e| AssetError::UnreadableSvg(e.to_string()))
}

/// The canvas size of an SVG, without re-serializing it.
///
/// An SVG keeps its size in the document rather than in a fixed-offset header,
/// so `probe` cannot report it and leaves zero. The renderer needs the real
/// numbers: `contain` and `cover` divide by them, and a zero would put a NaN
/// into an SVG attribute and take the whole page down with it.
pub fn size(bytes: &[u8]) -> Option<(u32, u32)> {
    let tree = parse(bytes).ok()?;
    let size = tree.size();
    let (width, height) = (size.width().ceil() as u32, size.height().ceil() as u32);
    (width > 0 && height > 0).then_some((width, height))
}

pub fn sanitize(bytes: &[u8]) -> Result<Sanitized, AssetError> {
    let tree = parse(bytes)?;

    let size = tree.size();
    let (width, height) = (size.width().ceil() as u32, size.height().ceil() as u32);
    if width == 0 || height == 0 {
        return Err(AssetError::EmptyImage);
    }

    // usvg writes back only what it parsed into its own model.
    let canonical = tree.to_string(&usvg::WriteOptions::default());

    Ok(Sanitized {
        bytes: canonical.into_bytes(),
        width,
        height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_the_canvas_size_from_the_tree() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="240" height="120"><rect width="240" height="120" fill="#FF5A36"/></svg>"##;
        let out = sanitize(svg).expect("a valid SVG");
        assert_eq!((out.width, out.height), (240, 120));
    }

    #[test]
    fn a_script_does_not_survive_the_round_trip() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
            <script>alert(1)</script>
            <rect width="10" height="10" onclick="alert(2)" fill="#000"/>
        </svg>"##;

        let out = sanitize(svg).expect("still a parseable SVG");
        let text = String::from_utf8(out.bytes).unwrap();

        assert!(!text.contains("script"), "script survived: {text}");
        assert!(!text.contains("onclick"), "handler survived: {text}");
        assert!(!text.contains("alert"), "payload survived: {text}");
    }

    #[test]
    fn the_stored_bytes_are_the_canonical_ones_not_the_upload() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><!-- a comment --><rect width="10" height="10"/></svg>"#;
        let out = sanitize(svg).unwrap();
        assert_ne!(out.bytes, svg.to_vec(), "the upload was stored verbatim");
    }

    #[test]
    fn something_that_is_not_an_svg_is_refused() {
        let error = sanitize(b"<html><body>not an svg</body></html>").unwrap_err();
        assert_eq!(error.code(), "INVALID_ASSET");
    }
}
