//! Format identification from magic bytes (plan section 44).
//!
//! The filename is never trusted. A `.png` that is really an SVG would be
//! stored with the wrong extension, served with the wrong MIME and handed to
//! the wrong decoder, so what the bytes say is the only input here.
//!
//! This is the one place in the workspace that answers "what are these bytes".
//! The renderer asks the same question when it embeds an image, and two
//! answers that disagreed would let ingest accept a format drawing could not
//! identify — which is exactly how SVG and WebP became unusable before.

use design_core::AssetKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Png,
    Jpeg,
    WebP,
    Svg,
}

impl Format {
    pub fn mime(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::WebP => "image/webp",
            Self::Svg => "image/svg+xml",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::WebP => "webp",
            Self::Svg => "svg",
        }
    }

    pub fn kind(self) -> AssetKind {
        match self {
            Self::Svg => AssetKind::Vector,
            _ => AssetKind::Raster,
        }
    }

    /// SVG is active content and is normalized before storage (plan section 45).
    pub fn needs_sanitizing(self) -> bool {
        matches!(self, Self::Svg)
    }
}

/// What the bytes are, and how big the image is.
///
/// For SVG the dimensions are left at zero here: they come from the usvg tree
/// during sanitization, which has to run anyway, rather than from a second
/// parser reading the width and height attributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Probed {
    pub format: Format,
    pub width: u32,
    pub height: u32,
}

pub fn probe(bytes: &[u8]) -> Option<Probed> {
    png(bytes)
        .or_else(|| jpeg(bytes))
        .or_else(|| webp(bytes))
        .or_else(|| svg(bytes))
}

/// An 8-byte signature, then an IHDR chunk carrying the dimensions.
fn png(bytes: &[u8]) -> Option<Probed> {
    if bytes.len() <= 24 || !bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return None;
    }
    Some(Probed {
        format: Format::Png,
        width: u32::from_be_bytes(bytes[16..20].try_into().ok()?),
        height: u32::from_be_bytes(bytes[20..24].try_into().ok()?),
    })
}

/// Walks the segment chain to the SOF marker that holds the size.
fn jpeg(bytes: &[u8]) -> Option<Probed> {
    if bytes.len() <= 4 || !bytes.starts_with(&[0xFF, 0xD8]) {
        return None;
    }

    let mut i = 2;
    while i + 9 < bytes.len() {
        if bytes[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = bytes[i + 1];
        let length = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;

        // SOF0..SOF15, excluding the DHT/JPG/DAC markers interleaved in that range.
        if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
            return Some(Probed {
                format: Format::Jpeg,
                height: u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]) as u32,
                width: u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]) as u32,
            });
        }
        // A zero-length segment would loop here forever on a malformed file.
        if length < 2 {
            return None;
        }
        i += 2 + length;
    }
    None
}

/// RIFF container. The dimensions live in a different place in each of the
/// three WebP variants, so all three are read rather than only the common one.
fn webp(bytes: &[u8]) -> Option<Probed> {
    if bytes.len() < 30 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WEBP" {
        return None;
    }

    let (width, height) = match &bytes[12..16] {
        // Lossy: a 3-byte frame tag, the 9D 01 2A sync code, then 14-bit sizes.
        b"VP8 " => {
            if bytes[23..26] != [0x9D, 0x01, 0x2A] {
                return None;
            }
            let w = u16::from_le_bytes([bytes[26], bytes[27]]) & 0x3FFF;
            let h = u16::from_le_bytes([bytes[28], bytes[29]]) & 0x3FFF;
            (w as u32, h as u32)
        }
        // Lossless: a 0x2F signature, then 14 bits each of width-1 and height-1
        // packed across four little-endian bytes.
        b"VP8L" => {
            if bytes[20] != 0x2F {
                return None;
            }
            let packed = u32::from_le_bytes([bytes[21], bytes[22], bytes[23], bytes[24]]);
            ((packed & 0x3FFF) + 1, ((packed >> 14) & 0x3FFF) + 1)
        }
        // Extended: canvas size as two 24-bit little-endian values, minus one.
        b"VP8X" => {
            let w = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], 0]) + 1;
            let h = u32::from_le_bytes([bytes[27], bytes[28], bytes[29], 0]) + 1;
            (w, h)
        }
        _ => return None,
    };

    Some(Probed {
        format: Format::WebP,
        width,
        height,
    })
}

/// SVG has no magic number, so the start of the document is inspected instead.
///
/// Leading whitespace, a byte-order mark, an XML declaration, a doctype and
/// comments may all precede the root element, and a real export usually has
/// several of them.
fn svg(bytes: &[u8]) -> Option<Probed> {
    let start = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);

    // Only the head matters, and decoding a whole megabyte to answer this would
    // be wasteful. Lossy is safe here: we are looking for ASCII markers.
    let head_len = start.len().min(4096);
    let head = String::from_utf8_lossy(&start[..head_len]);
    let mut rest = head.trim_start();

    loop {
        if rest.starts_with("<svg") {
            return Some(Probed {
                format: Format::Svg,
                // Filled in by sanitization, which parses the tree anyway.
                width: 0,
                height: 0,
            });
        }

        let skipped = ["<?xml", "<!DOCTYPE", "<!doctype", "<!--"]
            .iter()
            .find(|prefix| rest.starts_with(**prefix))?;

        let close = if *skipped == "<!--" { "-->" } else { ">" };
        let end = rest.find(close)? + close.len();
        rest = rest[end..].trim_start();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_bytes(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        bytes.extend_from_slice(&13u32.to_be_bytes());
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&[8, 6, 0, 0, 0]);
        bytes
    }

    #[test]
    fn reads_png_dimensions_from_the_header() {
        let probed = probe(&png_bytes(1080, 1350)).expect("a PNG");
        assert_eq!(probed.format, Format::Png);
        assert_eq!((probed.width, probed.height), (1080, 1350));
    }

    #[test]
    fn reads_lossy_webp_dimensions() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(b"WEBP");
        bytes.extend_from_slice(b"VP8 ");
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&[0, 0, 0]); // frame tag
        bytes.extend_from_slice(&[0x9D, 0x01, 0x2A]); // sync code
        bytes.extend_from_slice(&640u16.to_le_bytes());
        bytes.extend_from_slice(&480u16.to_le_bytes());

        let probed = probe(&bytes).expect("a lossy WebP");
        assert_eq!(probed.format, Format::WebP);
        assert_eq!((probed.width, probed.height), (640, 480));
    }

    #[test]
    fn reads_extended_webp_dimensions() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(b"WEBP");
        bytes.extend_from_slice(b"VP8X");
        bytes.extend_from_slice(&10u32.to_le_bytes());
        bytes.extend_from_slice(&[0, 0, 0, 0]); // flags and reserved
        bytes.extend_from_slice(&[0x7F, 0x00, 0x00]); // width - 1 = 127
        bytes.extend_from_slice(&[0x3F, 0x00, 0x00]); // height - 1 = 63

        let probed = probe(&bytes).expect("an extended WebP");
        assert_eq!((probed.width, probed.height), (128, 64));
    }

    #[test]
    fn recognises_an_svg_behind_the_usual_preamble() {
        let cases = [
            r#"<svg xmlns="http://www.w3.org/2000/svg"></svg>"#.to_string(),
            format!("\n\n  {}", r#"<svg xmlns="x"></svg>"#),
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>{}"#,
                r#"<svg xmlns="x"></svg>"#
            ),
            format!(
                "<!-- Generator: some editor -->\n{}",
                r#"<svg xmlns="x"></svg>"#
            ),
            format!(
                r#"<?xml version="1.0"?><!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "x.dtd">{}"#,
                r#"<svg xmlns="x"></svg>"#
            ),
        ];

        for case in cases {
            let probed = probe(case.as_bytes())
                .unwrap_or_else(|| panic!("should have been an SVG: {case:.60}"));
            assert_eq!(probed.format, Format::Svg);
        }
    }

    #[test]
    fn an_svg_with_a_byte_order_mark_is_still_an_svg() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(br#"<svg xmlns="x"></svg>"#);
        assert_eq!(probe(&bytes).unwrap().format, Format::Svg);
    }

    #[test]
    fn refuses_bytes_that_are_not_an_image() {
        for bytes in [
            &b""[..],
            &b"not an image at all"[..],
            &b"<html><body>hello</body></html>"[..],
            // A PDF, which is a plausible thing for a user to drag in.
            &b"%PDF-1.7\n%\xE2\xE3\xCF\xD3"[..],
        ] {
            assert!(
                probe(bytes).is_none(),
                "{:?} should be refused",
                &bytes[..4.min(bytes.len())]
            );
        }
    }

    #[test]
    fn the_extension_follows_the_bytes_not_a_filename() {
        assert_eq!(Format::Svg.extension(), "svg");
        assert_eq!(Format::Svg.kind(), AssetKind::Vector);
        assert_eq!(Format::Png.kind(), AssetKind::Raster);
    }

    #[test]
    fn a_zero_length_segment_does_not_loop_forever() {
        // 0xDB is DQT, not an SOF, so the walk advances by the declared segment
        // length — zero here. Without the guard the cursor never moves and this
        // test hangs instead of failing.
        let bytes = vec![0xFF, 0xD8, 0xFF, 0xDB, 0x00, 0x00, 0xFF, 0xD9, 0, 0, 0, 0];
        assert!(jpeg(&bytes).is_none());
    }
}
