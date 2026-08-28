use thiserror::Error;

use design_core::AssetId;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error(
        "ASSET_NOT_FOUND: asset {0} is referenced by a layer but was not supplied to the renderer"
    )]
    AssetNotFound(AssetId),

    #[error("FONT_DIR_UNREADABLE: {0}")]
    FontDirUnreadable(String),

    #[error("FONT_NOT_FOUND: {0}")]
    FontNotFound(String),

    #[error("INVALID_DIMENSION: {0}")]
    InvalidDimension(String),

    #[error("SVG_PARSE_FAILED: {0}")]
    SvgParse(#[from] usvg::Error),

    #[error("RENDER_FAILED: {0}")]
    RenderFailed(String),

    #[error("PNG_ENCODE_FAILED: {0}")]
    PngEncode(String),

    #[error("STORAGE_ERROR: cannot write the render cache at {0}")]
    CacheUnwritable(String),
}
