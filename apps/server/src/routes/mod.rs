//! The REST surface of plan section 31, under `/api/v1`.
//!
//! Handlers stay thin on purpose: they parse the transport, call one use case,
//! and serialize the result. Every decision worth testing lives in
//! `application`, so the MCP process reaches the same behaviour through the
//! same code rather than through a second implementation of it.

use actix_web::web;

pub mod asset_requests;
pub mod assets;
pub mod designs;
pub mod exports;
pub mod meta;
pub mod projects;
pub mod renders;

mod helpers;

pub use helpers::{actor_from, parse_id};
pub use meta::Status;

/// Malformed request bodies must answer in the shape of plan section 32 too.
///
/// Actix's own extractor error is plain text, which would reach an agent as an
/// unparseable blob and give it nothing to act on. This turns it into the same
/// envelope every handler produces.
fn json_config() -> web::JsonConfig {
    web::JsonConfig::default().error_handler(|error, _| {
        crate::error::ApiError::bad_request(
            "INVALID_BODY",
            format!("the request body could not be read: {error}"),
            "Check the tool's schema: a field is missing, misspelled, or the wrong type.",
        )
        .into()
    })
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.app_data(json_config()).service(
        web::scope("/api/v1")
            .configure(meta::configure)
            .configure(projects::configure)
            .configure(designs::configure)
            .configure(renders::configure)
            .configure(assets::configure)
            .configure(asset_requests::configure)
            .configure(exports::configure),
    );
}
