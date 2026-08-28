//! Structured logging (plan section 60).
//!
//! One span per request carrying the fields the plan names, so a line in the
//! log answers "who changed what, when, and how long did it take" without
//! joining anything by hand. That matters here more than in most servers:
//! Forma has two concurrent writers, a person in the editor and an agent over
//! MCP, and "which of them did this" is usually the first question.
//!
//! The fields start empty and are filled in by the handlers that know them, so
//! a request that never touches a design simply has no `design_id`.
//!
//! Never recorded: auth headers, tokens, and asset bytes. Uploads are logged by
//! size and type only.

use std::time::Instant;

use actix_web::body::MessageBody;
use actix_web::dev::{ServiceRequest, ServiceResponse};
use actix_web::middleware::Next;
use actix_web::{Error, HttpMessage};
use design_core::{Actor, ActorType, DesignId, ProjectId, Version};
use tracing::{field::Empty, Instrument, Span};
use uuid::Uuid;

/// Correlates a client's retry with the request it is retrying.
const REQUEST_ID_HEADER: &str = "x-request-id";

/// Set by `forma-mcp` so a server-side line says which tool caused it.
const TOOL_HEADER: &str = "x-forma-tool";

pub async fn trace_request(
    request: ServiceRequest,
    next: Next<impl MessageBody + 'static>,
) -> Result<ServiceResponse<impl MessageBody>, Error> {
    // An id the client supplied wins, so a trace survives the process boundary
    // between forma-mcp and here.
    let request_id = request
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty() && value.len() <= 128)
        .map(str::to_string)
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let tool_name = request
        .headers()
        .get(TOOL_HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty() && value.len() <= 64)
        .map(str::to_string);

    let span = tracing::info_span!(
        "request",
        request_id = %request_id,
        method = %request.method(),
        path = %request.path(),
        tool_name = tool_name.as_deref().unwrap_or(""),
        // Filled in by whichever handler learns them.
        project_id = Empty,
        design_id = Empty,
        version = Empty,
        actor_type = Empty,
        actor_id = Empty,
        render_ms = Empty,
        status = Empty,
        duration_ms = Empty,
    );

    request
        .extensions_mut()
        .insert(RequestId(request_id.clone()));

    let started = Instant::now();
    let result = next.call(request).instrument(span.clone()).await;
    let elapsed = started.elapsed().as_millis();

    match result {
        Ok(mut response) => {
            let status = response.status();
            span.record("status", status.as_u16());
            span.record("duration_ms", elapsed as u64);

            // Echo it back so a client can quote the id when reporting a bug.
            if let Ok(value) = request_id.parse() {
                response.headers_mut().insert(
                    actix_web::http::header::HeaderName::from_static(REQUEST_ID_HEADER),
                    value,
                );
            }

            let _guard = span.enter();
            if status.is_server_error() {
                tracing::error!("request failed");
            } else if status.is_client_error() {
                tracing::warn!("request rejected");
            } else {
                tracing::info!("request served");
            }

            Ok(response)
        }
        Err(error) => {
            span.record("duration_ms", elapsed as u64);
            let _guard = span.enter();
            tracing::error!(error = %error, "request errored before a response");
            Err(error)
        }
    }
}

/// The id assigned to the request being handled.
#[derive(Debug, Clone)]
pub struct RequestId(pub String);

/// Records the project a request turned out to be about.
pub fn record_project(id: ProjectId) {
    Span::current().record("project_id", tracing::field::display(id));
}

/// Records the design and the version the request left it at.
pub fn record_design(id: DesignId, version: Option<Version>) {
    let span = Span::current();
    span.record("design_id", tracing::field::display(id));
    if let Some(version) = version {
        span.record("version", version.0);
    }
}

/// Records who asked (plan section 61), so the log distinguishes an agent's
/// edit from a person's.
pub fn record_actor(actor: &Actor) {
    let span = Span::current();
    span.record(
        "actor_type",
        match actor.actor_type {
            ActorType::Human => "human",
            ActorType::Agent => "agent",
            ActorType::System => "system",
        },
    );
    if let Some(id) = &actor.actor_id {
        span.record("actor_id", id.as_str());
    }
}

/// Records how long a render took. Rendering is the slow part, and a preview
/// that got slower is the kind of regression nobody notices without a number.
pub fn record_render_ms(elapsed: u128) {
    Span::current().record("render_ms", elapsed as u64);
}
