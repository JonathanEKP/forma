//! Phase 0 REST surface, under the `/api/v1` prefix of plan section 31.

use actix_web::{get, post, web, HttpResponse, Responder};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use persistence::ProjectRepository;

use crate::error::ApiError;
use crate::state::AppState;
use crate::test_card;

#[derive(Serialize)]
struct Health {
    status: &'static str,
}

#[get("/health")]
async fn health() -> impl Responder {
    HttpResponse::Ok().json(Health { status: "ok" })
}

/// Exported to TypeScript (plan section 30) so the editor never hand-maintains
/// a copy of this shape.
#[derive(Serialize, TS)]
#[ts(export)]
pub struct Status {
    #[ts(type = "string")]
    pub name: &'static str,
    #[ts(type = "string")]
    pub version: &'static str,
    pub schema_version: u32,
    #[ts(type = "string")]
    pub database: &'static str,
    pub fonts: Vec<String>,
    /// i64 comes from SQLite's COUNT, but it travels as a plain JSON number,
    /// so it must not be typed as bigint on the TypeScript side.
    #[ts(type = "number")]
    pub projects: i64,
}

#[get("/status")]
async fn status(state: web::Data<AppState>) -> Result<HttpResponse, ApiError> {
    let projects = state
        .projects
        .list()
        .await
        .map_err(|e| ApiError::internal("STORAGE_ERROR", e.to_string()))?
        .len() as i64;

    Ok(HttpResponse::Ok().json(Status {
        name: "forma-server",
        version: env!("CARGO_PKG_VERSION"),
        schema_version: design_core::SCHEMA_VERSION,
        database: "sqlite",
        fonts: state.fonts.families().to_vec(),
        projects,
    }))
}

#[derive(Debug, Default, Deserialize)]
pub struct TestCardRequest {
    /// Free text painted on the card, so a host can prove the render is live.
    #[serde(default)]
    pub label: Option<String>,
}

/// Phase 0 acceptance: renders a PNG without any design existing yet.
#[post("/render/test-card")]
async fn render_test_card(
    state: web::Data<AppState>,
    body: Option<web::Json<TestCardRequest>>,
) -> Result<HttpResponse, ApiError> {
    let label = body
        .and_then(|b| b.into_inner().label)
        .unwrap_or_else(|| "phase 0 - stdio MCP".to_string());

    let (document, page) = test_card::document(&label);

    let png = renderer::render_page_png(
        &document,
        &page,
        &test_card::palette(),
        &state.fonts,
        &renderer::NoAssets,
        1.0,
    )
    .map_err(|e| ApiError::internal("RENDER_FAILED", e.to_string()))?;

    Ok(HttpResponse::Ok()
        .content_type("image/png")
        .insert_header((
            "x-forma-canvas",
            format!("{}x{}", document.format.width, document.format.height),
        ))
        .body(png))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v1")
            .service(health)
            .service(status)
            .service(render_test_card),
    );
}
