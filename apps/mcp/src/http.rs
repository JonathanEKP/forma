//! MCP over Streamable HTTP (plan section 56).
//!
//! `forma-mcp` serves this on its own runtime rather than inside Actix. §56 says
//! not to force an adapter onto the main backend for it, and rmcp already
//! exposes a Tower service, so the shortest honest path is a small axum server
//! in front of it.
//!
//! Behind Caddy this is `/mcp`; on the machine it never leaves loopback.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::Request;
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};

use crate::client::ServerClient;
use crate::server::FormaMcp;

/// The bearer every remote caller must present (plan section 57).
///
/// Kept as a hash so a comparison cannot be walked one byte at a time, the same
/// way `forma-server` holds it. `blake3::Hash` compares in constant time.
#[derive(Clone)]
struct Guard(blake3::Hash);

impl Guard {
    fn accepts(&self, headers: &HeaderMap) -> bool {
        let Some(raw) = headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
        else {
            return false;
        };
        let Some((scheme, presented)) = raw.trim().split_once(' ') else {
            return false;
        };
        scheme.eq_ignore_ascii_case("bearer") && self.0 == blake3::hash(presented.trim().as_bytes())
    }
}

async fn require_bearer(
    axum::extract::State(guard): axum::extract::State<Guard>,
    request: Request,
    next: Next,
) -> Response {
    if guard.accepts(request.headers()) {
        return next.run(request).await;
    }

    // Nothing about why. A caller learning whether the header was missing,
    // malformed or merely wrong is a caller being helped to guess.
    (
        StatusCode::UNAUTHORIZED,
        [(header::CONTENT_TYPE, "application/json")],
        Body::from(
            r#"{"error":{"code":"UNAUTHORIZED","message":"this endpoint requires a bearer token","hint":"Send Authorization: Bearer <token>, with the value of FORMA_REMOTE_TOKEN."}}"#,
        ),
    )
        .into_response()
}

/// Serves MCP over HTTP until the process is stopped.
///
/// A token is **required**, not optional. stdio is reachable only by the process
/// that spawned it; an HTTP port is reachable by whatever can route to it, so
/// there is no local case where running this unauthenticated is what someone
/// meant.
pub async fn serve(listen: &str, base_url: String, token: Option<String>) -> anyhow::Result<()> {
    let Some(token) = token else {
        anyhow::bail!(
            "--transport http needs FORMA_REMOTE_TOKEN. An HTTP port is reachable by anything \
             that can route to it, unlike stdio, so there is no unauthenticated mode here."
        );
    };

    let guard = Guard(blake3::hash(token.as_bytes()));
    let for_client = token.clone();

    let mcp = StreamableHttpService::new(
        move || {
            Ok(FormaMcp::new(
                ServerClient::new(base_url.clone()).with_token(Some(for_client.clone())),
            ))
        },
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default(),
    );

    let app = axum::Router::new()
        .fallback_service(mcp)
        .layer(axum::middleware::from_fn_with_state(
            guard.clone(),
            require_bearer,
        ))
        .with_state(guard);

    let listener = tokio::net::TcpListener::bind(listen)
        .await
        .map_err(|e| anyhow::anyhow!("cannot bind {listen}: {e}"))?;

    tracing::info!(listen, "forma-mcp serving MCP over HTTP");
    axum::serve(listener, app).await?;
    Ok(())
}
