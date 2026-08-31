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
use axum::extract::{Request, State};
use axum::http::{header, uri::Authority, HeaderMap, HeaderValue, StatusCode, Uri};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};

use crate::client::{BearerCredential, ServerClient};
use crate::server::FormaMcp;

const PROTECTED_RESOURCE_PATH: &str = "/.well-known/oauth-protected-resource/mcp";

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

/// Everything an OAuth-capable client may learn before it has a credential.
///
/// The challenge is built at startup and stored as a `HeaderValue`, so an
/// operator-controlled domain cannot become an invalid response header on the
/// authentication hot path.
#[derive(Clone)]
struct ProtectedResource {
    document: String,
    challenge: HeaderValue,
}

impl ProtectedResource {
    fn from_config(
        public_host: Option<&str>,
        issuer: Option<&str>,
    ) -> anyhow::Result<Option<Self>> {
        let Some(issuer) = issuer.map(str::trim).filter(|value| !value.is_empty()) else {
            return Ok(None);
        };
        let public_host = public_host
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "FORMA_SUPABASE_ISSUER is set but FORMA_DOMAIN is not; OAuth discovery needs \
                     the public MCP resource name"
                )
            })?;
        let authority = public_host
            .parse::<Authority>()
            .map_err(|error| anyhow::anyhow!("FORMA_DOMAIN is not a valid hostname: {error}"))?;
        let issuer_uri = issuer.parse::<Uri>().map_err(|error| {
            anyhow::anyhow!("FORMA_SUPABASE_ISSUER is not a valid URL: {error}")
        })?;
        if !matches!(issuer_uri.scheme_str(), Some("http" | "https"))
            || issuer_uri.authority().is_none()
        {
            anyhow::bail!("FORMA_SUPABASE_ISSUER must be an absolute http or https URL");
        }

        let public_base = format!("https://{authority}");
        let metadata_url = format!("{public_base}{PROTECTED_RESOURCE_PATH}");
        let document = serde_json::to_string(&serde_json::json!({
            "resource": format!("{public_base}/mcp"),
            "authorization_servers": [issuer],
            "bearer_methods_supported": ["header"],
        }))?;
        let challenge =
            HeaderValue::from_str(&format!("Bearer resource_metadata=\"{metadata_url}\""))
                .map_err(|error| {
                    anyhow::anyhow!("OAuth discovery challenge is invalid: {error}")
                })?;

        Ok(Some(Self {
            document,
            challenge,
        }))
    }
}

#[derive(Clone)]
struct HttpAuth {
    guard: Guard,
    protected_resource: Option<ProtectedResource>,
}

impl HttpAuth {
    fn from_config(
        guard: Guard,
        public_host: Option<&str>,
        issuer: Option<&str>,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            guard,
            protected_resource: ProtectedResource::from_config(public_host, issuer)?,
        })
    }
}

async fn require_bearer(
    State(auth): State<HttpAuth>,
    mut request: Request,
    next: Next,
) -> Response {
    // Discovery has to be reachable before a client knows where to obtain a
    // credential. No other path bypasses the guard.
    if request.uri().path() == PROTECTED_RESOURCE_PATH {
        return next.run(request).await;
    }

    if let Some(credential) = auth.guard.credential(request.headers()) {
        request.extensions_mut().insert(credential);
        return next.run(request).await;
    }

    // Nothing about why. A caller learning whether the header was missing,
    // malformed or merely wrong is a caller being helped to guess.
    let mut response = (
        StatusCode::UNAUTHORIZED,
        [(header::CONTENT_TYPE, "application/json")],
        Body::from(
            r#"{"error":{"code":"UNAUTHORIZED","message":"this endpoint requires a bearer token","hint":"Send Authorization: Bearer <token>, with the value of FORMA_REMOTE_TOKEN."}}"#,
        ),
    )
        .into_response();
    let challenge = auth
        .protected_resource
        .as_ref()
        .map(|resource| resource.challenge.clone())
        .unwrap_or_else(|| HeaderValue::from_static("Bearer"));
    response
        .headers_mut()
        .insert(header::WWW_AUTHENTICATE, challenge);
    response
}

async fn protected_resource_metadata(State(auth): State<HttpAuth>) -> Response {
    match auth.protected_resource {
        Some(resource) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json")],
            resource.document,
        )
            .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "application/json")],
            Body::from(
                r#"{"error":{"code":"NOT_FOUND","message":"OAuth discovery is not configured","hint":"Set FORMA_SUPABASE_ISSUER and FORMA_DOMAIN before using OAuth discovery."}}"#,
            ),
        )
            .into_response(),
    }
}

fn router(
    base_url: String,
    guard: Guard,
    public_host: Option<String>,
    issuer: Option<String>,
) -> anyhow::Result<axum::Router> {
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

    let auth = HttpAuth::from_config(guard, public_host.as_deref(), issuer.as_deref())?;
    let named = public_host;
    let mcp = StreamableHttpService::new(
        move || Ok(handler(base_url.clone(), named.clone())),
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default().with_allowed_hosts(allowed),
    );

    Ok(axum::Router::new()
        .route(PROTECTED_RESOURCE_PATH, get(protected_resource_metadata))
        .fallback_service(mcp)
        .layer(axum::middleware::from_fn_with_state(
            auth.clone(),
            require_bearer,
        ))
        .with_state(auth))
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
    issuer: Option<String>,
) -> anyhow::Result<()> {
    let Some(token) = token else {
        anyhow::bail!(
            "--transport http needs FORMA_REMOTE_TOKEN. An HTTP port is reachable by anything \
             that can route to it, unlike stdio, so there is no unauthenticated mode here."
        );
    };

    let app = router(base_url, Guard::new(&token), public_host, issuer)?;

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

    #[test]
    fn an_issuer_without_a_public_domain_is_rejected_at_startup() {
        let error =
            ProtectedResource::from_config(None, Some("https://project.supabase.co/auth/v1"))
                .err()
                .expect("the configuration must fail");
        assert!(error.to_string().contains("FORMA_DOMAIN"));
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
        let (mcp_url, mcp_task) =
            start(router(upstream_url, guard, None, None).expect("HTTP router")).await;
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
