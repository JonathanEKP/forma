//! Asset requests (plan sections 22, 23 and 31).

use actix_web::{get, post, web, HttpResponse};
use design_core::{AssetId, AssetRequestId, DesignId, ProjectId};
use serde::Deserialize;
use ts_rs::TS;

use crate::error::ApiError;
use crate::routes::parse_id;
use crate::state::AppState;

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct CreateAssetRequestBody {
    /// What the image is for, in the design's own terms.
    #[serde(default)]
    pub purpose: String,
    pub description: String,
    /// Text for a human to take to whatever tool they prefer. Forma never runs
    /// it: image generation is out of scope for V1.
    #[serde(default)]
    pub suggested_prompt: Option<String>,
    #[serde(default)]
    pub design_id: Option<DesignId>,
    #[serde(default)]
    pub requested_width: Option<u32>,
    #[serde(default)]
    pub requested_height: Option<u32>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct ResolveAssetRequestBody {
    pub asset_id: AssetId,
}

fn request_id(raw: &str) -> Result<AssetRequestId, ApiError> {
    Ok(AssetRequestId(parse_id(raw, "asset request id")?))
}

#[get("/projects/{id}/asset-requests")]
async fn list(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let project = ProjectId(parse_id(&path, "project id")?);
    let requests = application::asset_requests::list(&*state.asset_requests, project).await?;
    Ok(HttpResponse::Ok().json(requests))
}

#[post("/projects/{id}/asset-requests")]
async fn create(
    state: web::Data<AppState>,
    path: web::Path<String>,
    body: web::Json<CreateAssetRequestBody>,
) -> Result<HttpResponse, ApiError> {
    let project = ProjectId(parse_id(&path, "project id")?);
    let body = body.into_inner();

    let request = application::asset_requests::create(
        &*state.projects,
        &*state.designs,
        &*state.asset_requests,
        crate::routes::auth(&state).account_id,
        application::asset_requests::NewAssetRequest {
            project_id: project,
            design_id: body.design_id,
            purpose: body.purpose,
            description: body.description,
            suggested_prompt: body.suggested_prompt,
            requested_width: body.requested_width,
            requested_height: body.requested_height,
        },
    )
    .await?;

    Ok(HttpResponse::Created().json(request))
}

#[get("/asset-requests/{id}")]
async fn get(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let request =
        application::asset_requests::get(&*state.asset_requests, request_id(&path)?).await?;
    Ok(HttpResponse::Ok().json(request))
}

/// The user uploaded the missing image (plan section 23).
#[post("/asset-requests/{id}/resolve")]
async fn resolve(
    state: web::Data<AppState>,
    path: web::Path<String>,
    body: web::Json<ResolveAssetRequestBody>,
) -> Result<HttpResponse, ApiError> {
    let request = application::asset_requests::resolve(
        &*state.asset_requests,
        &*state.assets,
        request_id(&path)?,
        body.asset_id,
    )
    .await?;
    Ok(HttpResponse::Ok().json(request))
}

#[post("/asset-requests/{id}/cancel")]
async fn cancel(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let request =
        application::asset_requests::cancel(&*state.asset_requests, request_id(&path)?).await?;
    Ok(HttpResponse::Ok().json(request))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(list)
        .service(create)
        .service(get)
        .service(resolve)
        .service(cancel);
}
