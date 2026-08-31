//! Liveness, capability reporting and the Phase 0 test card.

use actix_web::{get, post, web, HttpRequest, HttpResponse, Responder};
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
    /// The families the renderer can draw. An agent must not name one outside
    /// this list: the renderer cannot substitute a font it does not have.
    pub fonts: Vec<String>,
    /// i64 comes from SQLite's COUNT, but it travels as a plain JSON number,
    /// so it must not be typed as bigint on the TypeScript side.
    #[ts(type = "number")]
    pub projects: i64,
    /// The ceiling on one `apply_operations` batch (plan section 28), reported
    /// so a client can split its work without discovering the limit by failing.
    #[ts(type = "number")]
    pub max_ops_per_batch: usize,
}

/// The canonical agent skill, served to anyone (plan section 117.3).
///
/// Compiled in rather than read from disk. On disk it would be a deploy
/// artifact: it could go missing, drift from the binary that serves it, or 404
/// after a partial deploy — and a skill that disagrees with the code is worse
/// than none, because an agent follows it confidently and is wrong. This is the
/// same constant `apps/mcp/tests/skill_matches_the_code.rs` asserts against, so
/// the document the route serves is the document that test checks.
///
/// It carries no account data and no credentials, which is why it sits outside
/// `/api/v1` and needs no Caddy route of its own.
pub const SKILL: &str = include_str!("../../../../skills/forma/SKILL.md");

/// GET **and** HEAD. Actix's `#[get]` answers only GET, so a HEAD fell through
/// to the SPA fallback and reported `text/html` — a monitor probing this route
/// the cheap way would have concluded the skill is a web page.
pub async fn skill() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/markdown; charset=utf-8")
        .body(SKILL)
}

#[get("/status")]
async fn status(
    state: web::Data<AppState>,
    request: HttpRequest,
) -> Result<HttpResponse, ApiError> {
    let projects = state
        .projects
        .list(crate::routes::auth(&request, &state).account_id)
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
        max_ops_per_batch: design_core::MAX_OPS_PER_BATCH,
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
    .map_err(|e| ApiError::Application(e.into()))?;

    Ok(HttpResponse::Ok()
        .content_type("image/png")
        .insert_header((
            "x-forma-canvas",
            format!("{}x{}", document.format.width, document.format.height),
        ))
        .body(png))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(health)
        .service(status)
        .service(render_test_card);
}
