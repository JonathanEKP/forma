//! Projects and Brand Kits (plan section 31).

use actix_web::{delete, get, patch, post, put, web, HttpResponse};
use design_core::{BrandKit, ProjectId};
use serde::Deserialize;
use ts_rs::TS;

use crate::error::ApiError;
use crate::routes::parse_id;
use crate::state::AppState;

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct ProjectNameRequest {
    pub name: String,
}

fn project_id(raw: &str) -> Result<ProjectId, ApiError> {
    Ok(ProjectId(parse_id(raw, "project id")?))
}

#[get("/projects")]
async fn list(state: web::Data<AppState>) -> Result<HttpResponse, ApiError> {
    let projects = application::projects::list(&*state.projects).await?;
    Ok(HttpResponse::Ok().json(projects))
}

#[post("/projects")]
async fn create(
    state: web::Data<AppState>,
    body: web::Json<ProjectNameRequest>,
) -> Result<HttpResponse, ApiError> {
    let project = application::projects::create(&*state.projects, &body.name).await?;
    Ok(HttpResponse::Created().json(project))
}

#[get("/projects/{id}")]
async fn get(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let project = application::projects::get(&*state.projects, project_id(&path)?).await?;
    Ok(HttpResponse::Ok().json(project))
}

#[patch("/projects/{id}")]
async fn rename(
    state: web::Data<AppState>,
    path: web::Path<String>,
    body: web::Json<ProjectNameRequest>,
) -> Result<HttpResponse, ApiError> {
    let project =
        application::projects::rename(&*state.projects, project_id(&path)?, &body.name).await?;
    Ok(HttpResponse::Ok().json(project))
}

#[delete("/projects/{id}")]
async fn remove(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    application::projects::delete(&*state.projects, project_id(&path)?).await?;
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
    let brand = application::projects::get_brand(&*state.projects, project_id(&path)?).await?;
    Ok(HttpResponse::Ok().json(brand))
}

#[put("/projects/{id}/brand")]
async fn put_brand(
    state: web::Data<AppState>,
    path: web::Path<String>,
    body: web::Json<BrandKit>,
) -> Result<HttpResponse, ApiError> {
    let id = project_id(&path)?;
    let mut brand = body.into_inner();

    // The path names the project; a body claiming a different one would write
    // the kit somewhere the caller did not ask for.
    if brand.project_id != id {
        return Err(ApiError::bad_request(
            "INVALID_OP",
            format!(
                "the Brand Kit names project {} but the path names {id}",
                brand.project_id
            ),
            "Send the kit to its own project, or correct project_id in the body.",
        ));
    }
    brand.project_id = id;

    let stored = application::projects::put_brand(&*state.projects, &brand).await?;
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
