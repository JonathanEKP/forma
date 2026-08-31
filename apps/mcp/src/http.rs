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

use crate::client::{BearerCredential, ServerClient};
use crate::server::FormaMcp;

fn handler(base_url: String, public_host: Option<String>) -> FormaMcp {
    FormaMcp::for_http(ServerClient::new(base_url)).on_deployment(public_host)
}

/// The bearer every remote caller must present (plan section 57).
///
/// Kept as a hash so a comparison cannot be walked one byte at a time, the same
/// way `forma-server` holds it. `blake3::Hash` compares in constant time.
#[derive(Clone)]
struct Guard(Vec<blake3::Hash>);

impl Guard {
    fn new(token: &str) -> Self {
        Self(vec![blake3::hash(token.as_bytes())])
    }

    #[cfg(test)]
    fn for_test(tokens: &[&str]) -> Self {
        Self(
            tokens
                .iter()
                .map(|token| blake3::hash(token.as_bytes()))
                .collect(),
        )
    }

    fn credential(&self, headers: &HeaderMap) -> Option<BearerCredential> {
        let raw = headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())?;
        let (scheme, presented) = raw.trim().split_once(' ')?;
        if !scheme.eq_ignore_ascii_case("bearer") {
            return None;
        }

        let presented = presented.trim();
        self.0
            .iter()
            .any(|expected| *expected == blake3::hash(presented.as_bytes()))
            .then(|| BearerCredential::new(presented))
    }
}

async fn require_bearer(
    axum::extract::State(guard): axum::extract::State<Guard>,
    mut request: Request,
    next: Next,
) -> Response {
    if let Some(credential) = guard.credential(request.headers()) {
        request.extensions_mut().insert(credential);
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

fn router(base_url: String, guard: Guard, public_host: Option<String>) -> axum::Router {
    // rmcp validates the inbound Host header against an allowlist, and its
    // default is loopback only. That is a real protection rather than a nuisance:
    // it stops a page in someone's browser from being pointed at an MCP server
    // bound on their own machine (DNS rebinding). Behind a reverse proxy the Host
    // is the public name, so a deployment has to name itself or every request is
    // 403 before any JSON-RPC is read.
    //
    // Added to the loopback entries rather than replacing them, and never
    // `disable_allowed_hosts()`: an empty list means "allow everything", which
    // would trade the protection for a shorter config.
    let mut allowed = vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
    ];
    if let Some(host) = public_host.clone() {
        allowed.push(host);
    }
    tracing::info!(?allowed, "accepting these Host headers");

    let named = public_host;
    let mcp = StreamableHttpService::new(
        move || Ok(handler(base_url.clone(), named.clone())),
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default().with_allowed_hosts(allowed),
    );

    axum::Router::new()
        .fallback_service(mcp)
        .layer(axum::middleware::from_fn_with_state(
            guard.clone(),
            require_bearer,
        ))
        .with_state(guard)
}

/// Serves MCP over HTTP until the process is stopped.
///
/// A token is **required**, not optional. stdio is reachable only by the process
/// that spawned it; an HTTP port is reachable by whatever can route to it, so
/// there is no local case where running this unauthenticated is what someone
/// meant.
pub async fn serve(
    listen: &str,
    base_url: String,
    token: Option<String>,
    public_host: Option<String>,
) -> anyhow::Result<()> {
    let Some(token) = token else {
        anyhow::bail!(
            "--transport http needs FORMA_REMOTE_TOKEN. An HTTP port is reachable by anything \
             that can route to it, unlike stdio, so there is no unauthenticated mode here."
        );
    };

    let app = router(base_url, Guard::new(&token), public_host);

    let listener = tokio::net::TcpListener::bind(listen)
        .await
        .map_err(|e| anyhow::anyhow!("cannot bind {listen}: {e}"))?;

    tracing::info!(listen, "forma-mcp serving MCP over HTTP");
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use axum::extract::State;
    use axum::routing::get;
    use axum::Router;
    use serde_json::json;
    use tokio::sync::mpsc::{self, UnboundedSender};

    use super::*;

    #[test]
    fn the_http_factory_has_no_process_credential_source() {
        let service = handler("http://127.0.0.1:4765".to_string(), None);
        assert!(
            !service.has_process_credential_source(),
            "an HTTP handler must not contain a process credential to fall back to"
        );
    }

    async fn record_credential(
        State(seen): State<UnboundedSender<u8>>,
        headers: HeaderMap,
    ) -> impl IntoResponse {
        let marker = match headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
        {
            Some("Bearer credential-a") => 1,
            Some("Bearer credential-b") => 2,
            _ => 0,
        };
        seen.send(marker).expect("the test is still listening");
        (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json")],
            r#"{"version":"test"}"#,
        )
    }

    async fn start(app: Router) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test server");
        let address = listener.local_addr().expect("test server address");
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve test app");
        });
        (format!("http://{address}"), task)
    }

    async fn initialize(http: &reqwest::Client, url: &str, credential: &str) -> String {
        let response = http
            .post(url)
            .bearer_auth(credential)
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream")
            .json(&json!({
                "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": {
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": { "name": "test", "version": "0" }
                }
            }))
            .send()
            .await
            .expect("initialize answers");
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        response
            .headers()
            .get("mcp-session-id")
            .expect("session id")
            .to_str()
            .expect("session id is text")
            .to_string()
    }

    async fn initialized(http: &reqwest::Client, url: &str, credential: &str, session: &str) {
        let response = http
            .post(url)
            .bearer_auth(credential)
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream")
            .header("mcp-session-id", session)
            .json(&json!({
                "jsonrpc": "2.0", "method": "notifications/initialized"
            }))
            .send()
            .await
            .expect("initialized answers");
        assert!(response.status().is_success());
    }

    async fn call_status(http: &reqwest::Client, url: &str, credential: &str, session: &str) {
        let response = http
            .post(url)
            .bearer_auth(credential)
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream")
            .header("mcp-session-id", session)
            .json(&json!({
                "jsonrpc": "2.0", "id": 2, "method": "tools/call",
                "params": { "name": "forma_status", "arguments": {} }
            }))
            .send()
            .await
            .expect("tools/call answers");
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let body = response.text().await.expect("tools/call body");
        assert!(!body.contains("UNAUTHORIZED"), "call was refused: {body}");
    }

    /// The request extension, not the service factory or session, owns the
    /// outbound bearer. Two calls in flight therefore cannot overwrite or
    /// inherit one another's identity.
    #[tokio::test]
    async fn concurrent_sessions_forward_their_own_presented_credentials() {
        let (seen_tx, mut seen_rx) = mpsc::unbounded_channel();
        let upstream = Router::new()
            .route("/api/v1/status", get(record_credential))
            .with_state(seen_tx);
        let (upstream_url, upstream_task) = start(upstream).await;

        let guard = Guard::for_test(&["credential-a", "credential-b"]);
        let (mcp_url, mcp_task) = start(router(upstream_url, guard, None)).await;
        let http = reqwest::Client::new();

        let (session_a, session_b) = tokio::join!(
            initialize(&http, &mcp_url, "credential-a"),
            initialize(&http, &mcp_url, "credential-b")
        );
        tokio::join!(
            initialized(&http, &mcp_url, "credential-a", &session_a),
            initialized(&http, &mcp_url, "credential-b", &session_b)
        );
        tokio::join!(
            call_status(&http, &mcp_url, "credential-a", &session_a),
            call_status(&http, &mcp_url, "credential-b", &session_b)
        );

        let mut seen = vec![
            tokio::time::timeout(std::time::Duration::from_secs(1), seen_rx.recv())
                .await
                .expect("first forwarded credential arrives")
                .expect("upstream channel stays open"),
            tokio::time::timeout(std::time::Duration::from_secs(1), seen_rx.recv())
                .await
                .expect("second forwarded credential arrives")
                .expect("upstream channel stays open"),
        ];
        seen.sort();
        assert_eq!(
            seen,
            vec![1, 2],
            "each session must forward the bearer on its own request"
        );

        mcp_task.abort();
        upstream_task.abort();
    }
}
