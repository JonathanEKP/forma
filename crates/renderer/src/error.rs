//! Render errors.
//!
//! Like `DomainError` and `PersistenceError`, each variant owns its stable code
//! and a hint, so the transport layers can build the envelope of plan section 32
//! without matching on this enum. The code is not repeated in the message: the
//! envelope carries the two separately.

use thiserror::Error;

use design_core::AssetId;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("asset {0} is referenced by a layer but was not supplied to the renderer")]
    AssetNotFound(AssetId),

    #[error("{0}")]
    FontDirUnreadable(String),

    #[error("{0}")]
    FontNotFound(String),

    #[error("{0}")]
    InvalidDimension(String),

    #[error("{0}")]
    SvgParse(#[from] usvg::Error),

    #[error("{0}")]
    RenderFailed(String),

    #[error("{0}")]
    PngEncode(String),

    #[error("cannot write the render cache at {0}")]
    CacheUnwritable(String),
}

impl RenderError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::AssetNotFound(_) => "ASSET_NOT_FOUND",
            Self::FontDirUnreadable(_) => "FONT_DIR_UNREADABLE",
            Self::FontNotFound(_) => "FONT_NOT_FOUND",
            Self::InvalidDimension(_) => "INVALID_DIMENSION",
            Self::SvgParse(_) => "SVG_PARSE_FAILED",
            Self::RenderFailed(_) => "RENDER_FAILED",
            Self::PngEncode(_) => "PNG_ENCODE_FAILED",
            Self::CacheUnwritable(_) => "STORAGE_ERROR",
        }
    }

    pub fn hint(&self) -> &'static str {
        match self {
            Self::AssetNotFound(_) => {
                "Upload the asset, or point the layer at an asset_id that list_assets returned."
            }
            Self::FontDirUnreadable(_) => {
                "Check FORMA_FONT_DIR: the renderer loads fonts only from that directory."
            }
            Self::FontNotFound(_) => {
                "Use a family that forma_status lists; the renderer cannot substitute one."
            }
            Self::InvalidDimension(_) => {
                "Widths, heights and font sizes must be finite and greater than zero."
            }
            Self::SvgParse(_) => "The generated SVG was not parseable. This is a Forma bug.",
            Self::RenderFailed(_) => "The page could not be rasterized. Try a smaller scale.",
            Self::PngEncode(_) => "The rendered pixels could not be encoded as PNG.",
            Self::CacheUnwritable(_) => {
                "Check that FORMA_DATA_DIR exists and is writable; the render itself succeeded."
            }
        }
    }
}
