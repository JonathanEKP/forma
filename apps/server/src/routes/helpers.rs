//! Transport-level parsing shared by the handlers.

use actix_web::HttpRequest;
use design_core::{Actor, ActorType, DesignId, ProjectId};
use uuid::Uuid;

use crate::error::ApiError;
use crate::telemetry;

/// Parses a UUID from a path segment into the caller's own error shape.
///
/// Actix would reject a malformed `Path<Uuid>` before the handler runs, but
/// with its own body rather than the envelope of plan section 32. A client
/// parsing `error.code` must not meet a different shape depending on which
/// field it got wrong, so the ids arrive as strings and are parsed here.
pub fn parse_id(raw: &str, what: &'static str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| {
        ApiError::bad_request(
            "MALFORMED_ID",
            format!("{what} \"{raw}\" is not a UUID"),
            "Ids are UUIDs returned by a previous call; never construct one by hand.",
        )
    })
}

/// Parses a project id and records it on the request span.
///
/// Recording here rather than in each handler means a handler cannot forget:
/// every route that works on a project goes through this to get the id at all.
pub fn project_id(raw: &str) -> Result<ProjectId, ApiError> {
    let id = ProjectId(parse_id(raw, "project id")?);
    telemetry::record_project(id);
    Ok(id)
}

/// Parses a design id and records it on the request span.
pub fn design_id(raw: &str) -> Result<DesignId, ApiError> {
    let id = DesignId(parse_id(raw, "design id")?);
    telemetry::record_design(id, None);
    Ok(id)
}

/// The `X-Forma-Actor` header, which every mutation records (plan section 61).
///
/// One header rather than a field on each request body: the actor is a property
/// of who is calling, not of what they are asking for, and duplicating it into
/// every DTO would let the two disagree. Accepted values are `human`, `system`
/// and `agent:<id>`; anything absent is a human, because the editor is the
/// client that will not bother to send it.
pub fn actor_from(request: &HttpRequest) -> Actor {
    let actor = actor_header(request);
    telemetry::record_actor(&actor);
    actor
}

fn actor_header(request: &HttpRequest) -> Actor {
    let Some(raw) = request
        .headers()
        .get("x-forma-actor")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty())
    else {
        return Actor::human();
    };

    match raw {
        "human" => Actor::human(),
        "system" => Actor::system(),
        other => Actor {
            actor_type: ActorType::Agent,
            actor_id: Some(other.to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test::TestRequest;

    #[test]
    fn a_missing_actor_header_is_a_human() {
        let request = TestRequest::default().to_http_request();
        assert_eq!(actor_from(&request), Actor::human());
    }

    #[test]
    fn an_agent_keeps_its_id_so_history_can_name_it() {
        let request = TestRequest::default()
            .insert_header(("x-forma-actor", "agent:claude"))
            .to_http_request();
        let actor = actor_from(&request);
        assert_eq!(actor.actor_type, ActorType::Agent);
        assert_eq!(actor.actor_id.as_deref(), Some("agent:claude"));
    }

    #[test]
    fn a_blank_header_does_not_become_an_agent_with_an_empty_id() {
        let request = TestRequest::default()
            .insert_header(("x-forma-actor", "   "))
            .to_http_request();
        assert_eq!(actor_from(&request), Actor::human());
    }

    #[test]
    fn a_malformed_id_is_rejected_in_the_stable_error_shape() {
        let error = parse_id("not-a-uuid", "design id").unwrap_err();
        assert_eq!(error.code(), "MALFORMED_ID");
    }
}
