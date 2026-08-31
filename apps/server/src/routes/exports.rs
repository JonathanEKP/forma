//! Export (plan sections 31 and 71).

use actix_web::{get, post, web, HttpRequest, HttpResponse};
use design_core::{ExportId, ExportScope, PageId};
use serde::Deserialize;
use ts_rs::TS;

use crate::error::ApiError;
use crate::routes::{design_id, parse_id};
use crate::state::AppState;

fn default_scale() -> f32 {
    1.0
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct CreateExportRequest {
    /// Defaults to the whole design, which is what a carousel wants.
    #[serde(default)]
    pub scope: Option<ExportScope>,
    /// Only meaningful for a single-page export.
    #[serde(default)]
    pub page_id: Option<PageId>,
    #[serde(default = "default_scale")]
    pub scale: f32,
}

#[derive(Debug, Deserialize)]
pub struct DownloadQuery {
    /// Which file of a multi-page export to send.
    #[serde(default)]
    pub file: Option<String>,
}

fn export_id(raw: &str) -> Result<ExportId, ApiError> {
    Ok(ExportId(parse_id(raw, "export id")?))
}

#[post("/designs/{id}/exports")]
async fn create(
    state: web::Data<AppState>,
    request: HttpRequest,
    path: web::Path<String>,
    body: Option<web::Json<CreateExportRequest>>,
) -> Result<HttpResponse, ApiError> {
    let design = design_id(&path)?;
    let export_request = body
        .map(web::Json::into_inner)
        .unwrap_or(CreateExportRequest {
            scope: None,
            page_id: None,
            scale: default_scale(),
        });

    let record = application::designs::get(
        &*state.designs,
        crate::routes::auth(&request, &state).account_id,
        design,
    )
    .await?;
    let brand = application::projects::get_brand(
        &*state.projects,
        crate::routes::auth(&request, &state).account_id,
        record.project_id,
    )
    .await?;

    let (export, files) = application::exports::create(
        &*state.exports,
        &*state.assets,
        &*state.store,
        &state.cache,
        &state.fonts,
        crate::routes::auth(&request, &state).account_id,
        &record,
        brand.as_ref(),
        export_request.scope.unwrap_or(ExportScope::AllPages),
        export_request.page_id,
        export_request.scale,
    )
    .await?;

    Ok(HttpResponse::Created().json(serde_json::json!({
        "export": export,
        "files": files.iter().map(|f| serde_json::json!({
            "filename": f.filename,
            "byte_size": f.byte_size,
        })).collect::<Vec<_>>(),
    })))
}

/// The record, plus the names a caller can download.
#[get("/exports/{id}")]
async fn get(
    state: web::Data<AppState>,
    request: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let export = application::exports::get(
        &*state.exports,
        crate::routes::auth(&request, &state).account_id,
        export_id(&path)?,
    )
    .await?;

    // The names come from the snapshot the export pinned, not from the design
    // as it stands now: a page added since must not appear in this listing.
    let record = application::designs::get(
        &*state.designs,
        crate::routes::auth(&request, &state).account_id,
        export.design_id,
    )
    .await?;
    let document = application::designs::snapshot(
        &*state.designs,
        crate::routes::auth(&request, &state).account_id,
        export.design_id,
        export.design_version,
    )
    .await?;

    let filenames = application::exports::filenames(&export, &record.name, document.pages.len());

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "export": export,
        "files": filenames,
    })))
}

#[get("/exports/{id}/download")]
async fn download(
    state: web::Data<AppState>,
    request: HttpRequest,
    path: web::Path<String>,
    query: web::Query<DownloadQuery>,
) -> Result<HttpResponse, ApiError> {
    let export = application::exports::get(
        &*state.exports,
        crate::routes::auth(&request, &state).account_id,
        export_id(&path)?,
    )
    .await?;
    let bytes =
        application::exports::download(&*state.store, &export, query.file.as_deref()).await?;

    let filename = query
        .file
        .clone()
        .or_else(|| export.storage_key.rsplit('/').next().map(str::to_string))
        .unwrap_or_else(|| "export.png".to_string());

    Ok(HttpResponse::Ok()
        .content_type("image/png")
        .insert_header((
            "content-disposition",
            format!("attachment; filename=\"{filename}\""),
        ))
        .body(bytes))
}

#[get("/designs/{id}/exports")]
async fn list(
    state: web::Data<AppState>,
    request: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let design = design_id(&path)?;
    let account = crate::routes::auth(&request, &state).account_id;
    application::designs::get(&*state.designs, account, design).await?;
    let exports = application::exports::list(&*state.exports, account, design).await?;
    Ok(HttpResponse::Ok().json(exports))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(create)
        .service(list)
        .service(get)
        .service(download);
}
