//! Projects and Brand Kits (plan section 31).

use actix_web::{delete, get, patch, post, put, web, HttpResponse};
use serde::Deserialize;
use ts_rs::TS;

use application::projects::BrandKitDraft;

use crate::error::ApiError;
use crate::routes::{auth, project_id};
use crate::state::AppState;

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct ProjectNameRequest {
    pub name: String,
}

#[get("/projects")]
async fn list(state: web::Data<AppState>) -> Result<HttpResponse, ApiError> {
    let projects = application::projects::list(&*state.projects, auth(&state).account_id).await?;
    Ok(HttpResponse::Ok().json(projects))
}

#[post("/projects")]
async fn create(
    state: web::Data<AppState>,
    body: web::Json<ProjectNameRequest>,
) -> Result<HttpResponse, ApiError> {
    let project =
        application::projects::create(&*state.projects, auth(&state).account_id, &body.name).await?;
    Ok(HttpResponse::Created().json(project))
}

#[get("/projects/{id}")]
async fn get(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let project =
        application::projects::get(&*state.projects, auth(&state).account_id, project_id(&path)?)
            .await?;
    Ok(HttpResponse::Ok().json(project))
}

#[patch("/projects/{id}")]
async fn rename(
    state: web::Data<AppState>,
    path: web::Path<String>,
    body: web::Json<ProjectNameRequest>,
) -> Result<HttpResponse, ApiError> {
    let project = application::projects::rename(
        &*state.projects,
        auth(&state).account_id,
        project_id(&path)?,
        &body.name,
    )
    .await?;
    Ok(HttpResponse::Ok().json(project))
}

#[delete("/projects/{id}")]
async fn remove(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    application::projects::delete(&*state.projects, auth(&state).account_id, project_id(&path)?)
        .await?;
    Ok(HttpResponse::NoContent().finish())
}

/// A project with no Brand Kit yet answers 200 with `null` rather than 404.
///
/// The project exists; it simply has no kit. A 404 here would make a client
/// that is checking whether to show the brand panel indistinguishable from one
/// that used a bad project id.
#[get("/projects/{id}/brand")]
async fn get_brand(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let brand =
        application::projects::get_brand(&*state.projects, auth(&state).account_id, project_id(&path)?)
            .await?;
    Ok(HttpResponse::Ok().json(brand))
}

#[put("/projects/{id}/brand")]
/// The path names the project and the server owns the kit's identity, so the
/// body carries neither. That also removes the mismatch a body-supplied
/// `project_id` used to make possible.
async fn put_brand(
    state: web::Data<AppState>,
    path: web::Path<String>,
    body: web::Json<BrandKitDraft>,
) -> Result<HttpResponse, ApiError> {
    let id = project_id(&path)?;
    let stored = application::projects::put_brand(
        &*state.projects,
        auth(&state).account_id,
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
