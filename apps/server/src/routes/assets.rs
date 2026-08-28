//! Assets: listing, metadata and soft delete (plan section 31).
//!
//! Upload is deliberately absent here for now. It needs magic-byte checks,
//! BLAKE3 hashing, dimension probing and SVG normalization (plan sections 44
//! and 45), which belong in the `assets` crate rather than in a handler.

use actix_web::{delete, get, web, HttpResponse};
use design_core::{AssetId, ProjectId};
use persistence::AssetRepository;

use crate::error::ApiError;
use crate::routes::parse_id;
use crate::state::AppState;

#[get("/projects/{id}/assets")]
async fn list(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let project = ProjectId(parse_id(&path, "project id")?);
    let assets = state
        .assets
        .list_for_project(project)
        .await
        .map_err(|e| ApiError::Application(e.into()))?;
    Ok(HttpResponse::Ok().json(assets))
}

#[get("/assets/{id}")]
async fn get(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let id = AssetId(parse_id(&path, "asset id")?);
    let asset = state
        .assets
        .get(id)
        .await
        .map_err(|e| ApiError::Application(e.into()))?;
    Ok(HttpResponse::Ok().json(asset))
}

/// A soft delete: historical design versions may still reference the asset, so
/// the bytes stay put and real garbage collection is deferred (plan section 74).
#[delete("/assets/{id}")]
async fn remove(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let id = AssetId(parse_id(&path, "asset id")?);
    state
        .assets
        .soft_delete(id)
        .await
        .map_err(|e| ApiError::Application(e.into()))?;
    Ok(HttpResponse::NoContent().finish())
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(list).service(get).service(remove);
}
