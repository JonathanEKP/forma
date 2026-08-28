use thiserror::Error;

#[derive(Debug, Error)]
pub enum RenderError {
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
}
