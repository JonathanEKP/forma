//! Transport-level parsing shared by the handlers.

use actix_web::{HttpMessage, HttpRequest};
use design_core::{Actor, ActorType, AuthContext, AuthKind, DesignId, ProjectId};
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;
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

/// Who to record as the author of a mutation (plan sections 61 and 117.2).
///
/// Derived from the resolved identity wherever there is one, and read from the
/// `X-Forma-Actor` header only in local mode.
///
/// That distinction is the point. The header is written by the caller, so
/// trusting it means anyone can claim to be anyone in the history. In local mode
/// that costs nothing: the caller is the process that started Forma, and its
/// word about itself is exactly as good as everything else it says. Once a
/// request is authenticated *as someone*, the history has to say who that was,
/// not who they typed.
///
/// One header rather than a field on each request body, for the same reason as
/// before: the actor is a property of who is calling, not of what they are
/// asking for, and duplicating it into every DTO would let the two disagree.
/// Accepted values are `human`, `system` and `agent:<id>`; anything absent is a
/// human, because the editor is the client that will not bother to send it.
pub fn actor_from(request: &HttpRequest, auth: &AuthContext) -> Actor {
    let actor = if auth.trusts_caller_supplied_actor() {
        actor_header(request)
    } else {
        derived(auth)
    };
    telemetry::record_actor(&actor);
    actor
}

/// The mapping of plan section 117.2.
fn derived(auth: &AuthContext) -> Actor {
    let subject = auth
        .user_id
        .map(|id| id.to_string())
        .unwrap_or_else(|| "unknown".to_string());

    match auth.kind {
        // Unreachable: `trusts_caller_supplied_actor` took this branch already.
        AuthKind::Local => Actor::human(),
        AuthKind::BrowserUser => Actor {
            actor_type: ActorType::Human,
            actor_id: Some(format!("user:{subject}")),
        },
        AuthKind::McpOAuth => Actor {
            actor_type: ActorType::Agent,
            actor_id: Some(format!(
                "oauth:{}:{subject}",
                auth.client_id.as_deref().unwrap_or("unknown")
            )),
        },
        AuthKind::McpPat => Actor {
            actor_type: ActorType::Agent,
            actor_id: Some(format!(
                "pat:{}",
                auth.api_key_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "unknown".into())
            )),
        },
        // Reviewed for this class rather than borrowed from a neighbour
        // (Pontis plan §10(b) D1).
        //
        // An agent, not a human. Everything that arrives with a service
        // credential is a capability a model invoked through Pontis. Some of
        // those were approved by a person first, but approving is not authoring:
        // the operation is the agent's, and recording it as `Human` would make
        // it indistinguishable from an edit made in the editor — which is the
        // distinction §61's history exists to keep.
        //
        // Both the credential and the person, not either alone. The PAT answer,
        // `pat:{key}`, would name the sidecar and hide whom it acted for, and one
        // sidecar acts for many people; that is the reason this class exists
        // instead of a PAT. The person alone would hide which delegate did it,
        // and the credential id is the unit that gets revoked when one leaks.
        // The arm refuses any request that does not name a user, so `unknown`
        // is not a value this can actually record.
        AuthKind::Service => Actor {
            actor_type: ActorType::Agent,
            actor_id: Some(format!(
                "service:{}:{subject}",
                auth.service_credential_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "unknown".into())
            )),
        },
    }
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

/// The identity behind this request (plan section 108).
///
/// One function, so the account is resolved in a single place and a handler
/// cannot invent one. Middleware inserts credential-derived identity; test
/// harnesses and trusted-local calls that omit the middleware retain the
/// process account as their explicit fallback.
pub fn auth(request: &HttpRequest, state: &AppState) -> AuthContext {
    if let Some(resolved) = request.extensions().get::<AuthContext>().cloned() {
        return resolved;
    }

    // Nothing was resolved, which means one of two very different things.
    //
    // In local mode it is the ordinary case: there is no credential to read, the
    // process boundary is the security boundary, and every request belongs to
    // the one account this process serves.
    //
    // In hosted mode it should be unreachable — `require_bearer` refuses
    // anything under `/api/` that it could not resolve. But "should be
    // unreachable" is a property of another function, and `is_open` has already
    // grown once; a path added there later would silently arrive here. So this
    // fails closed rather than trusting that: an account id nothing references
    // owns no project, so every scoped query answers exactly as it would for an
    // id that was never issued. The tenancy boundary contains the bug instead of
    // the bug escaping through it.
    match state.mode {
        crate::config::AuthMode::Local => AuthContext::local(state.account),
        _ => AuthContext::local(design_core::AccountId::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test::TestRequest;
    use design_core::{AccountId, ApiKeyId, AuthUserId, ServiceCredentialId};

    /// Local mode: the caller is the process that started Forma.
    fn local() -> AuthContext {
        AuthContext::local(AccountId::new())
    }

    #[test]
    fn a_missing_actor_header_is_a_human() {
        let request = TestRequest::default().to_http_request();
        assert_eq!(actor_from(&request, &local()), Actor::human());
    }

    #[test]
    fn an_agent_keeps_its_id_so_history_can_name_it() {
        let request = TestRequest::default()
            .insert_header(("x-forma-actor", "agent:claude"))
            .to_http_request();
        let actor = actor_from(&request, &local());
        assert_eq!(actor.actor_type, ActorType::Agent);
        assert_eq!(actor.actor_id.as_deref(), Some("agent:claude"));
    }

    #[test]
    fn a_blank_header_does_not_become_an_agent_with_an_empty_id() {
        let request = TestRequest::default()
            .insert_header(("x-forma-actor", "   "))
            .to_http_request();
        assert_eq!(actor_from(&request, &local()), Actor::human());
    }

    /// Plan 117.2: once a request is authenticated as someone, the history says
    /// who that was, not who the caller typed.
    #[test]
    fn an_authenticated_caller_cannot_choose_its_own_actor() {
        let request = TestRequest::default()
            .insert_header(("x-forma-actor", "human"))
            .to_http_request();

        let user = AuthUserId::new();
        let browser = AuthContext {
            account_id: AccountId::new(),
            user_id: Some(user),
            kind: AuthKind::BrowserUser,
            client_id: None,
            api_key_id: None,
            service_credential_id: None,
        };
        let actor = actor_from(&request, &browser);
        assert_eq!(actor.actor_type, ActorType::Human);
        assert_eq!(actor.actor_id.as_deref(), Some(&*format!("user:{user}")));

        // The same header, and an agent's identity: it claims to be a human and
        // is recorded as the agent it is.
        let agent = AuthContext {
            kind: AuthKind::McpOAuth,
            client_id: Some("claude".into()),
            ..browser.clone()
        };
        let actor = actor_from(&request, &agent);
        assert_eq!(actor.actor_type, ActorType::Agent);
        assert_eq!(
            actor.actor_id.as_deref(),
            Some(&*format!("oauth:claude:{user}"))
        );

        let key = ApiKeyId::new();
        let pat = AuthContext {
            kind: AuthKind::McpPat,
            api_key_id: Some(key),
            ..browser
        };
        let actor = actor_from(&request, &pat);
        assert_eq!(actor.actor_type, ActorType::Agent);
        assert_eq!(actor.actor_id.as_deref(), Some(&*format!("pat:{key}")));
    }

    /// The sidecar acts for many people, so its history names both the
    /// credential and the person it represented — and records an agent even
    /// when the caller claims to be a human.
    #[test]
    fn a_service_request_records_the_delegate_and_the_person() {
        let request = TestRequest::default()
            .insert_header(("x-forma-actor", "human"))
            .to_http_request();

        let user = AuthUserId::new();
        let credential = ServiceCredentialId::new();
        let service = AuthContext {
            account_id: AccountId::new(),
            user_id: Some(user),
            kind: AuthKind::Service,
            client_id: None,
            api_key_id: None,
            service_credential_id: Some(credential),
        };

        let actor = actor_from(&request, &service);
        assert_eq!(actor.actor_type, ActorType::Agent);
        assert_eq!(
            actor.actor_id.as_deref(),
            Some(&*format!("service:{credential}:{user}"))
        );
    }

    #[test]
    fn a_malformed_id_is_rejected_in_the_stable_error_shape() {
        let error = parse_id("not-a-uuid", "design id").unwrap_err();
        assert_eq!(error.code(), "MALFORMED_ID");
    }
}
