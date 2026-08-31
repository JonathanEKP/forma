//! The REST surface of plan section 31, under `/api/v1`.
//!
//! Handlers stay thin on purpose: they parse the transport, call one use case,
//! and serialize the result. Every decision worth testing lives in
//! `application`, so the MCP process reaches the same behaviour through the
//! same code rather than through a second implementation of it.

use actix_web::{web, HttpRequest, HttpResponse};

use crate::error::ApiError;
pub mod api_keys;

pub mod asset_requests;
pub mod assets;
pub mod designs;
pub mod exports;
pub mod meta;
pub mod projects;
pub mod renders;
pub mod templates;

mod helpers;

pub use helpers::{actor_from, auth, design_id, parse_id, project_id};
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
    // Outside the /api/v1 scope, and registered before the static files service
    // so the SPA fallback cannot shadow it: everything that is not /api/ now
    // resolves to index.html, which would have turned this into an HTML page
    // that looks like it works.
    cfg.service(
        web::resource("/skill")
            .guard(actix_web::guard::Any(actix_web::guard::Get()).or(actix_web::guard::Head()))
            .to(meta::skill),
    );

    cfg.app_data(json_config()).service(
        web::scope("/api/v1")
            .configure(api_keys::configure)
            .configure(meta::configure)
            .configure(projects::configure)
            .configure(designs::configure)
            .configure(renders::configure)
            .configure(assets::configure)
            .configure(asset_requests::configure)
            .configure(exports::configure)
            .configure(templates::configure)
            // The static app deliberately falls back to index.html. Ending the
            // API scope here keeps a miss machine-readable instead of letting
            // an API typo escape into that browser-only fallback.
            .default_service(web::route().to(api_not_found)),
    );
}

async fn api_not_found(request: HttpRequest) -> Result<HttpResponse, ApiError> {
    Err(ApiError::not_found(
        "NOT_FOUND",
        format!("no API route matches {}", request.path()),
        "Check the request method and the /api/v1 path.",
    ))
}
