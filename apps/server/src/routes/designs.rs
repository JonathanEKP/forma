//! Designs, the mutation path, undo and history (plan section 31).

use actix_web::{delete, get, post, web, HttpRequest, HttpResponse};
use design_core::{DesignDocument, DesignFormat, DesignOperation, FormatPreset, Page, Version};
use serde::Deserialize;
use ts_rs::TS;

use crate::error::ApiError;
use crate::routes::{actor_from, design_id, project_id};
use crate::state::AppState;
use crate::telemetry;

/// A canvas named by preset, with the dimensions optional.
///
/// `DesignFormat` requires all three, which is right for a stored document but
/// wrong for a request: a client that says "square" should not have to know
/// what square measures, and if it guessed it could disagree with the domain.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct FormatRequest {
    pub preset: FormatPreset,
    /// Only needed for the custom preset; otherwise it overrides the default.
    #[serde(default)]
    pub width: Option<f32>,
    #[serde(default)]
    pub height: Option<f32>,
}

impl FormatRequest {
    fn resolve(&self) -> DesignFormat {
        let mut format = DesignFormat::from_preset(self.preset);
        if let Some(width) = self.width {
            format.width = width;
        }
        if let Some(height) = self.height {
            format.height = height;
        }
        format
    }
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct CreateDesignRequest {
    pub name: String,
    /// The canvas for a design that starts empty.
    #[serde(default)]
    pub format: Option<FormatRequest>,
    /// A whole document, when importing rather than starting blank. It is
    /// migrated on the way in, so an older schema_version is upgraded instead
    /// of being silently accepted as-is.
    #[serde(default)]
    pub document: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct ApplyOperationsRequest {
    /// The version the caller read. The UPDATE is guarded by it, so a design
    /// that moved in between yields VERSION_CONFLICT (plan section 27).
    pub base_version: Version,
    pub operations: Vec<DesignOperation>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct UndoRequest {
    pub base_version: Version,
}

/// A design that starts blank still starts with one page.
///
/// `validate_structure` requires at least one, so an empty document could never
/// be persisted anyway — and a design with nowhere to put a layer is not a
/// starting point a caller would want.
fn blank(format: DesignFormat) -> DesignDocument {
    let mut document = DesignDocument::new(format);
    document.pages.push(Page::new("Page 1"));
    document
}

#[get("/projects/{id}/designs")]
async fn list(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let designs = application::designs::list(&*state.designs, crate::routes::auth(&state).account_id, project_id(&path)?).await?;
    Ok(HttpResponse::Ok().json(designs))
}

#[post("/projects/{id}/designs")]
async fn create(
    state: web::Data<AppState>,
    request: HttpRequest,
    path: web::Path<String>,
    body: web::Json<CreateDesignRequest>,
) -> Result<HttpResponse, ApiError> {
    let project = project_id(&path)?;
    let body = body.into_inner();

    let document = match (body.document, body.format) {
        (Some(_), Some(_)) => {
            return Err(ApiError::bad_request(
                "INVALID_OP",
                "a design cannot be created from both a document and a format",
                "Send `document` to import an existing design, or `format` to start blank.",
            ))
        }
        // Migrating rather than deserializing directly means an import from an
        // older schema is upgraded, and a newer one is refused by code rather
        // than half-parsed.
        (Some(document), None) => {
            DesignDocument::migrate(document).map_err(|e| ApiError::Application(e.into()))?
        }
        (None, Some(requested)) => {
            let format = requested.resolve();
            format
                .validate()
                .map_err(|e| ApiError::Application(e.into()))?;
            blank(format)
        }
        (None, None) => blank(DesignFormat::from_preset(FormatPreset::Square)),
    };

    let record = application::designs::create(&*state.designs, crate::routes::auth(&state).account_id, project, &body.name, &document, &actor_from(&request))
    .await?;

    // The version the design ended at, so one log line says what this call did.
    telemetry::record_design(record.id, Some(record.version));
    Ok(HttpResponse::Created().json(record))
}

#[get("/designs/{id}")]
async fn get(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let record = application::designs::get(&*state.designs, crate::routes::auth(&state).account_id, design_id(&path)?).await?;
    Ok(HttpResponse::Ok().json(record))
}

/// The single mutation entry point (plan section 28).
///
/// There is deliberately no route that accepts arbitrary JSON for a design:
/// every change arrives as typed operations, so a patch aimed at the wrong
/// layer type is rejected rather than silently ignored.
#[post("/designs/{id}/operations")]
async fn operations(
    state: web::Data<AppState>,
    request: HttpRequest,
    path: web::Path<String>,
    body: web::Json<ApplyOperationsRequest>,
) -> Result<HttpResponse, ApiError> {
    let record = application::designs::apply(&*state.designs, crate::routes::auth(&state).account_id, design_id(&path)?, body.base_version, &body.operations, &actor_from(&request))
    .await?;

    telemetry::record_design(record.id, Some(record.version));

    Ok(HttpResponse::Ok().json(record))
}

/// Restores the previous snapshot as a new version (plan section 29).
#[post("/designs/{id}/undo")]
async fn undo(
    state: web::Data<AppState>,
    request: HttpRequest,
    path: web::Path<String>,
    body: web::Json<UndoRequest>,
) -> Result<HttpResponse, ApiError> {
    let record = application::designs::undo(&*state.designs, crate::routes::auth(&state).account_id, design_id(&path)?, body.base_version, &actor_from(&request))
    .await?;

    telemetry::record_design(record.id, Some(record.version));

    Ok(HttpResponse::Ok().json(record))
}

#[get("/designs/{id}/history")]
async fn history(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let history = application::designs::history(&*state.designs, crate::routes::auth(&state).account_id, design_id(&path)?).await?;
    Ok(HttpResponse::Ok().json(history))
}

/// Plan section 31 defined a delete for projects and for assets and not for one
/// design, so nothing a person or an agent made could ever be taken back — a
/// design that fails to render is as permanent as a good one.
///
/// It is a hard delete, unlike an asset's. An asset is soft-deleted because
/// historical versions of *other* designs still point at its bytes; a design is
/// pointed at by nothing, and its snapshots exist only to undo it.
///
/// There is deliberately no MCP tool for this. Section 48 fixes the surface at
/// thirteen, and an agent works inside a workspace the person owns: deciding
/// that a piece of their work should stop existing is not the agent's call.
#[delete("/designs/{id}")]
async fn remove(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    application::designs::delete(&*state.designs, &*state.exports, &*state.store, &state.cache, crate::routes::auth(&state).account_id, design_id(&path)?)
    .await?;
    Ok(HttpResponse::NoContent().finish())
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(list)
        .service(create)
        .service(get)
        .service(operations)
        .service(undo)
        .service(history)
        .service(remove);
}
