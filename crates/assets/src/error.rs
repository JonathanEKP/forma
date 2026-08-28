//! Ingest errors, each owning its stable code and hint like every other error
//! enum in the workspace.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AssetError {
    #[error("{0} bytes exceeds the {1} byte upload limit")]
    TooLarge(usize, usize),

    #[error("the upload is empty")]
    Empty,

    #[error("the bytes are not a PNG, JPEG, WebP or SVG")]
    UnrecognisedFormat,

    #[error("the SVG could not be parsed: {0}")]
    UnreadableSvg(String),

    #[error("the image has no area")]
    EmptyImage,

    #[error("{0}x{1} exceeds the maximum supported dimensions")]
    TooLargeDimensions(u32, u32),
}

impl AssetError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::TooLarge(..) => "PAYLOAD_TOO_LARGE",
            Self::Empty
            | Self::UnrecognisedFormat
            | Self::UnreadableSvg(_)
            | Self::EmptyImage
            | Self::TooLargeDimensions(..) => "INVALID_ASSET",
        }
    }

    pub fn hint(&self) -> &'static str {
        match self {
            Self::TooLarge(..) => "Resize or recompress the file and upload it again.",
            Self::Empty => "The request carried no bytes. Attach the file and retry.",
            Self::UnrecognisedFormat => {
                "Forma reads PNG, JPEG, WebP and SVG. The extension is ignored; \
                 what matters is the content."
            }
            Self::UnreadableSvg(_) => {
                "The SVG must parse as a static picture. Export it again from your editor."
            }
            Self::EmptyImage => "The file describes an image with zero width or height.",
            Self::TooLargeDimensions(..) => {
                "Scale the image down before uploading; a canvas this large cannot be rendered."
            }
        }
    }
}
