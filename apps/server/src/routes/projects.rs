//! Projects and Brand Kits (plan section 31).

use actix_web::{delete, get, patch, post, put, web, HttpRequest, HttpResponse};
use serde::Deserialize;
use ts_rs::TS;

use application::projects::BrandKitDraft;

use crate::error::ApiError;
use crate::routes::project_id;
use crate::state::AppState;

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct ProjectNameRequest {
    pub name: String,
}

#[get("/projects")]
async fn list(state: web::Data<AppState>, request: HttpRequest) -> Result<HttpResponse, ApiError> {
    let projects = application::projects::list(
        &*state.projects,
        crate::routes::auth(&request, &state).account_id,
    )
    .await?;
    Ok(HttpResponse::Ok().json(projects))
}

#[post("/projects")]
async fn create(
    state: web::Data<AppState>,
    request: HttpRequest,
    body: web::Json<ProjectNameRequest>,
) -> Result<HttpResponse, ApiError> {
    let project = application::projects::create(
        &*state.projects,
        crate::routes::auth(&request, &state).account_id,
        &body.name,
    )
    .await?;
    Ok(HttpResponse::Created().json(project))
}

#[get("/projects/{id}")]
async fn get(
    state: web::Data<AppState>,
    request: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let project = application::projects::get(
        &*state.projects,
        crate::routes::auth(&request, &state).account_id,
        project_id(&path)?,
    )
    .await?;
    Ok(HttpResponse::Ok().json(project))
}

#[patch("/projects/{id}")]
async fn rename(
    state: web::Data<AppState>,
    request: HttpRequest,
    path: web::Path<String>,
    body: web::Json<ProjectNameRequest>,
) -> Result<HttpResponse, ApiError> {
    let project = application::projects::rename(
        &*state.projects,
        crate::routes::auth(&request, &state).account_id,
        project_id(&path)?,
        &body.name,
    )
    .await?;
    Ok(HttpResponse::Ok().json(project))
}

#[delete("/projects/{id}")]
async fn remove(
    state: web::Data<AppState>,
    request: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    application::projects::delete(
        &*state.projects,
        crate::routes::auth(&request, &state).account_id,
        project_id(&path)?,
    )
    .await?;
    Ok(HttpResponse::NoContent().finish())
}

/// A project this account owns with no Brand Kit answers `200 null`; an unknown
/// id or one owned by another account answers 404, like every other scoped
/// route. That distinction lets the editor show an empty brand panel without
/// turning a missing parent into a valid empty state.
#[get("/projects/{id}/brand")]
async fn get_brand(
    state: web::Data<AppState>,
    request: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let brand = application::projects::get_brand(
        &*state.projects,
        crate::routes::auth(&request, &state).account_id,
        project_id(&path)?,
    )
    .await?;
    Ok(HttpResponse::Ok().json(brand))
}

#[put("/projects/{id}/brand")]
/// The path names the project and the server owns the kit's identity, so the
/// body carries neither. That also removes the mismatch a body-supplied
/// `project_id` used to make possible.
async fn put_brand(
    state: web::Data<AppState>,
    request: HttpRequest,
    path: web::Path<String>,
    body: web::Json<BrandKitDraft>,
) -> Result<HttpResponse, ApiError> {
    let id = project_id(&path)?;
    let stored = application::projects::put_brand(
        &*state.projects,
        crate::routes::auth(&request, &state).account_id,
        id,
        body.into_inner(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(stored))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(list)
        .service(create)
        .service(get)
        .service(rename)
        .service(remove)
        .service(get_brand)
        .service(put_brand);
}
