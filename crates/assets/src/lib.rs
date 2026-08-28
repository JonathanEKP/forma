//! Asset ingest (plan section 8.5).
//!
//! Uploads, hashing, MIME, dimensions, logical storage and sanitization. It
//! knows nothing about agents, HTTP or the database: it takes bytes and answers
//! what they are, what to call them and what to store.

pub mod detect;
pub mod error;
pub mod svg;

pub use detect::{probe, Format, Probed};
pub use error::AssetError;

/// The default ceiling on one upload, overridable by the caller.
///
/// Large enough for a print-resolution photograph, small enough that a single
/// request cannot exhaust a machine that is also serving the editor.
pub const DEFAULT_MAX_UPLOAD_BYTES: usize = 25 * 1024 * 1024;

/// The largest edge Forma will accept. resvg allocates width times height times
/// four bytes to rasterize, so a 100000-pixel edge is a way to run the server
/// out of memory with one upload rather than a legitimate image.
pub const MAX_EDGE_PIXELS: u32 = 20_000;

/// Everything the caller needs to record and store an accepted upload.
#[derive(Debug, Clone)]
pub struct Ingested {
    /// The canonical bytes to store. For an SVG these are the normalized ones,
    /// never what was uploaded (plan section 45).
    pub bytes: Vec<u8>,
    pub format: Format,
    pub width: u32,
    pub height: u32,
    /// BLAKE3 of the canonical bytes, which is what deduplication compares
    /// (plan section 73). Hashing the upload instead would store two copies of
    /// one picture whenever an editor rewrote a comment.
    pub hash: String,
    /// `assets/ab/cd/<hash>.<ext>`, sharded so one directory never holds
    /// everything (plan section 44).
    pub storage_key: String,
}

/// Validates, sanitizes and hashes an upload.
///
/// The order matters: the size limit is checked before anything parses the
/// bytes, so a hostile file cannot cost more than one length comparison.
pub fn ingest(bytes: Vec<u8>, max_bytes: usize) -> Result<Ingested, AssetError> {
    if bytes.is_empty() {
        return Err(AssetError::Empty);
    }
    if bytes.len() > max_bytes {
        return Err(AssetError::TooLarge(bytes.len(), max_bytes));
    }

    let probed = probe(&bytes).ok_or(AssetError::UnrecognisedFormat)?;

    let (bytes, width, height) = if probed.format.needs_sanitizing() {
        let clean = svg::sanitize(&bytes)?;
        (clean.bytes, clean.width, clean.height)
    } else {
        (bytes, probed.width, probed.height)
    };

    if width == 0 || height == 0 {
        return Err(AssetError::EmptyImage);
    }
    if width > MAX_EDGE_PIXELS || height > MAX_EDGE_PIXELS {
        return Err(AssetError::TooLargeDimensions(width, height));
    }

    let hash = blake3::hash(&bytes).to_hex().to_string();
    let storage_key = storage_key(&hash, probed.format.extension());

    Ok(Ingested {
        bytes,
        format: probed.format,
        width,
        height,
        hash,
        storage_key,
    })
}

/// `assets/ab/cd/<hash>.<ext>`.
///
/// Duplicated from `persistence::asset_storage_key` deliberately: this crate
/// must not depend on persistence, and the layout is part of what ingest
/// decides. The two are covered by tests that assert the same shape.
pub fn storage_key(hash: &str, extension: &str) -> String {
    let a = hash.get(0..2).unwrap_or("00");
    let b = hash.get(2..4).unwrap_or("00");
    format!("assets/{a}/{b}/{hash}.{extension}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        bytes.extend_from_slice(&13u32.to_be_bytes());
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&[8, 6, 0, 0, 0]);
        bytes
    }

    #[test]
    fn accepts_a_png_and_shards_it_by_hash() {
        let out = ingest(png(800, 600), DEFAULT_MAX_UPLOAD_BYTES).expect("a PNG");
        assert_eq!(out.format, Format::Png);
        assert_eq!((out.width, out.height), (800, 600));
        assert_eq!(out.hash.len(), 64, "BLAKE3 is 32 bytes of hex");
        assert_eq!(
            out.storage_key,
            format!(
                "assets/{}/{}/{}.png",
                &out.hash[0..2],
                &out.hash[2..4],
                out.hash
            )
        );
    }

    #[test]
    fn identical_bytes_hash_identically_so_they_deduplicate() {
        let a = ingest(png(10, 10), DEFAULT_MAX_UPLOAD_BYTES).unwrap();
        let b = ingest(png(10, 10), DEFAULT_MAX_UPLOAD_BYTES).unwrap();
        assert_eq!(a.hash, b.hash);
        assert_eq!(a.storage_key, b.storage_key);
    }

    #[test]
    fn an_svg_is_stored_normalized_and_hashed_on_the_normalized_bytes() {
        let uploaded =
            br#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><script>alert(1)</script><rect width="40" height="20"/></svg>"#;

        let out = ingest(uploaded.to_vec(), DEFAULT_MAX_UPLOAD_BYTES).expect("a valid SVG");
        assert_eq!(out.format, Format::Svg);
        assert_eq!((out.width, out.height), (40, 20));
        assert_ne!(out.bytes, uploaded.to_vec());

        // Hashing the upload rather than the canonical bytes would defeat
        // deduplication for any two files that normalize to the same picture.
        assert_eq!(out.hash, blake3::hash(&out.bytes).to_hex().to_string());
    }

    #[test]
    fn the_size_limit_is_checked_before_anything_parses_the_bytes() {
        let error = ingest(vec![0u8; 11], 10).unwrap_err();
        assert_eq!(error.code(), "PAYLOAD_TOO_LARGE");
    }

    #[test]
    fn an_extension_cannot_smuggle_in_an_unsupported_format() {
        // What a caller names the file is irrelevant; these bytes are a PDF.
        let error = ingest(
            b"%PDF-1.7\n%\xE2\xE3\xCF\xD3".to_vec(),
            DEFAULT_MAX_UPLOAD_BYTES,
        )
        .unwrap_err();
        assert_eq!(error.code(), "INVALID_ASSET");
    }

    #[test]
    fn an_empty_upload_is_refused_before_probing() {
        assert_eq!(
            ingest(Vec::new(), DEFAULT_MAX_UPLOAD_BYTES)
                .unwrap_err()
                .code(),
            "INVALID_ASSET"
        );
    }

    #[test]
    fn an_image_too_large_to_rasterize_is_refused_at_the_door() {
        let error = ingest(png(50_000, 10), DEFAULT_MAX_UPLOAD_BYTES).unwrap_err();
        assert_eq!(error.code(), "INVALID_ASSET");
        assert!(error.to_string().contains("50000"), "{error}");
    }
}
