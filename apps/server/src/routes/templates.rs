//! Templates over HTTP (plan section 85).
//!
//! A template is scoped to its project on purpose. An `ImageLayer` holds an
//! `asset_id` and assets belong to a project, so a workspace-wide template would
//! carry references to bytes another project cannot read. Widening it later only
//! adds rows.

use actix_web::{delete, get, post, web, HttpRequest, HttpResponse};
use design_core::{TemplateDraft, TemplateId};
use serde::Deserialize;

use crate::error::ApiError;
use crate::routes::helpers::{actor_from, design_id, parse_id, project_id};
use crate::state::AppState;
use crate::telemetry;

fn template_id(raw: &str) -> Result<TemplateId, ApiError> {
    Ok(TemplateId(parse_id(raw, "template id")?))
}

#[derive(Debug, Deserialize)]
struct CreateFromTemplateRequest {
    name: String,
}

/// Keeps the shape of a design that came out well.
///
/// The roles are derived from the document server-side rather than accepted from
/// the caller: a brief that disagrees with the layout it describes is worse than
/// no brief, and nobody should have to author metadata by hand.
#[post("/designs/{id}/template")]
async fn save(
    state: web::Data<AppState>,
    path: web::Path<String>,
    body: web::Json<TemplateDraft>,
) -> Result<HttpResponse, ApiError> {
    let template = application::templates::save_from_design(
        &*state.templates,
        &*state.designs,
        crate::routes::auth(&state).account_id,
        design_id(&path)?,
        &body.into_inner(),
    )
    .await?;
    Ok(HttpResponse::Created().json(template))
}

#[get("/projects/{id}/templates")]
async fn list(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let account = crate::routes::auth(&state).account_id;
    let project = project_id(&path)?;
    application::projects::get(&*state.projects, account, project).await?;
    let templates = application::templates::list(&*state.templates, account, project).await?;
    Ok(HttpResponse::Ok().json(templates))
}

#[get("/templates/{id}")]
async fn get(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let template = application::templates::get(
        &*state.templates,
        crate::routes::auth(&state).account_id,
        template_id(&path)?,
    )
    .await?;
    Ok(HttpResponse::Ok().json(template))
}

/// A design starts from the template's shape, in the template's own project.
///
/// Its own route rather than a `template_id` on `POST /projects/{id}/designs`:
/// that body already chooses between a document and a format, and a third
/// mutually exclusive way to say what the design should be would make the
/// request harder to read than the two routes are.
#[post("/templates/{id}/designs")]
async fn create_design(
    state: web::Data<AppState>,
    request: HttpRequest,
    path: web::Path<String>,
    body: web::Json<CreateFromTemplateRequest>,
) -> Result<HttpResponse, ApiError> {
    let record = application::templates::create_design(
        &*state.designs,
        &*state.templates,
        crate::routes::auth(&state).account_id,
        template_id(&path)?,
        &body.name,
        &actor_from(&request, &crate::routes::auth(&state)),
    )
    .await?;

    telemetry::record_project(record.project_id);
    telemetry::record_design(record.id, Some(record.version));
    Ok(HttpResponse::Created().json(record))
}

#[delete("/templates/{id}")]
async fn remove(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    application::templates::delete(
        &*state.templates,
        crate::routes::auth(&state).account_id,
        template_id(&path)?,
    )
    .await?;
    Ok(HttpResponse::NoContent().finish())
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(save)
        .service(list)
        .service(get)
        .service(create_design)
        .service(remove);
}
