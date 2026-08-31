//! Validation, rendering and page previews (plan section 31).
//!
//! Every route here goes through `application::renders`, so the SVG the editor
//! draws, the PNG a preview returns and the bytes an export writes all come off
//! the same pipeline. A second render path is how preview and export drift
//! apart, which the plan calls risk R3.

use actix_web::{get, post, web, HttpResponse};
use design_core::{DesignId, PageId};
use serde::Deserialize;
use ts_rs::TS;

use crate::error::ApiError;
use crate::routes::{design_id, parse_id};
use crate::state::AppState;

/// A scale that is not given is 1.0: a preview at native canvas size.
fn default_scale() -> f32 {
    1.0
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct RenderRequest {
    /// Omitted renders the first page, which is what a thumbnail wants.
    #[serde(default)]
    pub page_id: Option<PageId>,
    #[serde(default = "default_scale")]
    pub scale: f32,
}

#[derive(Debug, Deserialize)]
pub struct ScaleQuery {
    #[serde(default = "default_scale")]
    pub scale: f32,
}

/// Resolves the page to render, defaulting to the first.
fn resolve_page(
    document: &design_core::DesignDocument,
    requested: Option<PageId>,
) -> Result<PageId, ApiError> {
    match requested {
        Some(id) => Ok(id),
        None => document.pages.first().map(|p| p.id).ok_or_else(|| {
            ApiError::bad_request(
                "INVALID_OP",
                "this design has no pages to render",
                "Add a page with an add_page operation before rendering.",
            )
        }),
    }
}

/// Loads the design, its project's Brand Kit, and the assets one page needs.
async fn context(
    state: &AppState,
    id: DesignId,
) -> Result<(design_core::DesignRecord, Option<design_core::BrandKit>), ApiError> {
    let record =
        application::designs::get(&*state.designs, crate::routes::auth(state).account_id, id)
            .await?;
    let brand = application::projects::get_brand(
        &*state.projects,
        crate::routes::auth(state).account_id,
        record.project_id,
    )
    .await?;
    Ok((record, brand))
}

#[post("/designs/{id}/validate")]
async fn validate(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let (record, brand) = context(&state, design_id(&path)?).await?;

    let report = application::renders::validate(
        crate::routes::auth(&state).account_id,
        &record,
        brand.as_ref(),
        &state.fonts,
        &*state.assets,
        &*state.store,
    )
    .await?;

    Ok(HttpResponse::Ok().json(report))
}

/// Renders a page and returns the PNG as base64 alongside its metadata.
///
/// The binary routes below are what a browser `<img>` points at; this one exists
/// for callers that want the bytes and the version they came from in a single
/// JSON response, which is what the MCP `render_preview` tool needs.
#[post("/designs/{id}/render")]
async fn render(
    state: web::Data<AppState>,
    path: web::Path<String>,
    body: Option<web::Json<RenderRequest>>,
) -> Result<HttpResponse, ApiError> {
    let id = design_id(&path)?;
    let (record, brand) = context(&state, id).await?;

    let request = body.map(web::Json::into_inner).unwrap_or(RenderRequest {
        page_id: None,
        scale: default_scale(),
    });
    let page_id = resolve_page(&record.document, request.page_id)?;

    let assets = application::renders::load_page_assets(
        &*state.assets,
        &*state.store,
        crate::routes::auth(&state).account_id,
        &record.document,
        page_id,
    )
    .await?;

    let png = application::renders::render_png(
        &state.cache,
        id,
        record.version,
        &record.document,
        page_id,
        brand.as_ref(),
        &state.fonts,
        &assets,
        request.scale,
    )?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "design_id": id,
        "page_id": page_id,
        "version": record.version,
        "scale": request.scale,
        "mime_type": "image/png",
        "byte_size": png.len(),
        "png_base64": base64_of(&png),
    })))
}

fn base64_of(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

#[get("/designs/{design_id}/pages/{page_id}/preview.svg")]
async fn preview_svg(
    state: web::Data<AppState>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, ApiError> {
    let (raw_design, raw_page) = path.into_inner();
    let id = design_id(&raw_design)?;
    let page_id = PageId(parse_id(&raw_page, "page id")?);

    let (record, brand) = context(&state, id).await?;
    let assets = application::renders::load_page_assets(
        &*state.assets,
        &*state.store,
        crate::routes::auth(&state).account_id,
        &record.document,
        page_id,
    )
    .await?;

    let rendered = application::renders::render(
        &record.document,
        page_id,
        brand.as_ref(),
        &state.fonts,
        &assets,
    )?;

    // The normalized tree, not the authored SVG: this is what the editor draws
    // under its interaction layer, with text already flattened to paths
    // (plan section 37).
    Ok(HttpResponse::Ok()
        .content_type("image/svg+xml")
        .insert_header(("x-forma-version", record.version.to_string()))
        .body(rendered.normalized_svg))
}

#[get("/designs/{design_id}/pages/{page_id}/preview.png")]
async fn preview_png(
    state: web::Data<AppState>,
    path: web::Path<(String, String)>,
    query: web::Query<ScaleQuery>,
) -> Result<HttpResponse, ApiError> {
    let (raw_design, raw_page) = path.into_inner();
    let id = design_id(&raw_design)?;
    let page_id = PageId(parse_id(&raw_page, "page id")?);

    let (record, brand) = context(&state, id).await?;
    let assets = application::renders::load_page_assets(
        &*state.assets,
        &*state.store,
        crate::routes::auth(&state).account_id,
        &record.document,
        page_id,
    )
    .await?;

    let png = application::renders::render_png(
        &state.cache,
        id,
        record.version,
        &record.document,
        page_id,
        brand.as_ref(),
        &state.fonts,
        &assets,
        query.scale,
    )?;

    Ok(HttpResponse::Ok()
        .content_type("image/png")
        .insert_header(("x-forma-version", record.version.to_string()))
        // A version's bytes never change, so this is safe to cache hard.
        .insert_header(("cache-control", "private, max-age=31536000, immutable"))
        .body(png))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(validate)
        .service(render)
        .service(preview_svg)
        .service(preview_png);
}
