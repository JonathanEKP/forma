//! MCP over Streamable HTTP, with the bearer in front of it (plan §§56, 57).
//!
//! The remote deploy is the first time anything Forma runs is reachable by
//! something other than the person who started it, so the credential is the part
//! worth a test that runs on every commit rather than once by hand on a VM.

use std::sync::Arc;

use actix_web::{web, App, HttpServer};
use forma_server::{auth::RemoteToken, routes, state::AppState};
use renderer::FontRegistry;
use serde_json::{json, Value};
use tokio::process::{Child, Command};

const TOKEN: &str = "a-token-with-enough-entropy-to-be-worth-something";

/// forma-server, guarded by the same token, on an ephemeral port.
/// Starts forma-server and **waits until it answers**.
///
/// The port arrives over a channel as soon as it is bound, which is not the same
/// as the server being ready to serve. That gap was harmless while forma-mcp
/// answered every credential question itself; now it asks forma-server, so a
/// request that lands in the gap is refused for the wrong reason and the test
/// fails once in a while — the flaky layer everybody learns to ignore.
///
/// Waits on liveness answering, never on a clock.
async fn start_server(dir: &std::path::Path) -> String {
    let dir = dir.to_path_buf();
    let (tx, rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        actix_web::rt::System::new().block_on(async move {
            let db = dir.join("forma.db");
            let pool = persistence::connect(&format!("sqlite://{}", db.display()), &dir)
                .await
                .expect("migrations");
            let font_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/fonts");
            let fonts = Arc::new(FontRegistry::from_dir(font_dir).expect("fonts"));
            let cache = Arc::new(renderer::RenderCache::new(dir.join("previews")));
            let store = Arc::new(persistence::LocalObjectStore::new(dir.clone()));
            let state = AppState::new(pool, fonts, cache, store);

            let server = HttpServer::new(move || {
                App::new()
                    .wrap(actix_web::middleware::from_fn(
                        forma_server::auth::require_bearer,
                    ))
                    .app_data(web::Data::new(state.clone()))
                    .app_data(web::Data::new(RemoteToken::new(TOKEN)))
                    .configure(routes::configure)
            })
            .workers(1)
            .bind("127.0.0.1:0")
            .expect("bind");

            tx.send(server.addrs()[0].port()).expect("send port");
            server.run().await.expect("run");
        });
    });

    let base = format!("http://127.0.0.1:{}", rx.recv().expect("server port"));

    let health = format!("{base}/api/v1/health");
    let http = reqwest::Client::new();
    for _ in 0..200 {
        if http
            .get(&health)
            .send()
            .await
            .is_ok_and(|response| response.status().is_success())
        {
            return base;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    panic!("forma-server never answered {health}");
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("a free port")
        .local_addr()
        .expect("addr")
        .port()
}

struct Mcp {
    child: Child,
    url: String,
}

impl Drop for Mcp {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

async fn start_mcp(base_url: &str, token: Option<&str>) -> Mcp {
    start_mcp_for(base_url, token, None).await
}

async fn start_mcp_for(base_url: &str, token: Option<&str>, domain: Option<&str>) -> Mcp {
    start_mcp_with_discovery(base_url, token, domain, None).await
}

async fn start_mcp_with_discovery(
    base_url: &str,
    token: Option<&str>,
    domain: Option<&str>,
    issuer: Option<&str>,
) -> Mcp {
    let port = free_port();
    let mut command = Command::new(env!("CARGO_BIN_EXE_forma-mcp"));
    command
        .arg("--transport")
        .arg("http")
        .arg("--listen")
        .arg(format!("127.0.0.1:{port}"))
        .env("FORMA_SERVER_URL", base_url)
        .env("FORMA_LOG", "warn")
        .kill_on_drop(true);

    match token {
        Some(token) => command.env("FORMA_REMOTE_TOKEN", token),
        None => command.env_remove("FORMA_REMOTE_TOKEN"),
    };
    match domain {
        Some(domain) => command.env("FORMA_DOMAIN", domain),
        None => command.env_remove("FORMA_DOMAIN"),
    };
    match issuer {
        Some(issuer) => command.env("FORMA_SUPABASE_ISSUER", issuer),
        None => command.env_remove("FORMA_SUPABASE_ISSUER"),
    };

    let child = command.spawn().expect("forma-mcp starts");
    let url = format!("http://127.0.0.1:{port}/");

    // Wait on the port answering rather than on a clock.
    let http = reqwest::Client::new();
    for _ in 0..100 {
        if http.post(&url).send().await.is_ok() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    Mcp { child, url }
}

async fn initialize(url: &str, token: Option<&str>) -> reqwest::Response {
    let mut request = reqwest::Client::new()
        .post(url)
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .json(&json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "test", "version": "0" }
            }
        }));
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    request.send().await.expect("the endpoint answers")
}

/// An HTTP port is reachable by anything that can route to it, unlike stdio, so
/// there is no unauthenticated mode. The process refuses rather than starting
/// open, because a warning in a log nobody reads is not a control.
#[tokio::test]
async fn http_mode_refuses_to_start_without_a_token() {
    let output = Command::new(env!("CARGO_BIN_EXE_forma-mcp"))
        .arg("--transport")
        .arg("http")
        .env_remove("FORMA_REMOTE_TOKEN")
        .output()
        .await
        .expect("it runs");

    assert!(!output.status.success(), "it must not start");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("FORMA_REMOTE_TOKEN"), "{stderr}");
}

#[tokio::test]
async fn an_unauthenticated_caller_gets_the_envelope_and_nothing_else() {
    let dir = tempfile::tempdir().unwrap();
    let base_url = start_server(dir.path()).await;
    let mcp = start_mcp(&base_url, Some(TOKEN)).await;

    let response = initialize(&mcp.url, None).await;
    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
    assert_eq!(
        response
            .headers()
            .get("www-authenticate")
            .expect("bearer challenge"),
        "Bearer"
    );

    let body: Value = response.json().await.expect("the §32 envelope");
    assert_eq!(body["error"]["code"], "UNAUTHORIZED");

    let metadata = reqwest::Client::new()
        .get(format!(
            "{}.well-known/oauth-protected-resource/mcp",
            mcp.url
        ))
        .send()
        .await
        .expect("the metadata route answers");
    assert_eq!(metadata.status(), reqwest::StatusCode::NOT_FOUND);
    let body: Value = metadata.json().await.expect("the §32 envelope");
    assert_eq!(body["error"]["code"], "NOT_FOUND");

    let wrong = initialize(&mcp.url, Some("not-the-token")).await;
    assert_eq!(wrong.status(), reqwest::StatusCode::UNAUTHORIZED);
}

/// The whole chain: an authorized host reaches forma-mcp over HTTP, and
/// forma-mcp reaches a forma-server that is itself guarded. One credential for
/// the deployment, presented in both directions.
#[tokio::test]
async fn an_authorized_host_can_speak_mcp_and_the_token_travels_onward() {
    let dir = tempfile::tempdir().unwrap();
    let base_url = start_server(dir.path()).await;
    let mcp = start_mcp(&base_url, Some(TOKEN)).await;

    let response = initialize(&mcp.url, Some(TOKEN)).await;
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let session = response
        .headers()
        .get("mcp-session-id")
        .expect("a session id")
        .to_str()
        .unwrap()
        .to_string();
    let body = response.text().await.unwrap();
    assert!(body.contains("forma-mcp"), "{body}");

    let http = reqwest::Client::new();
    http.post(&mcp.url)
        .bearer_auth(TOKEN)
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-session-id", &session)
        .json(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
        .send()
        .await
        .expect("initialized");

    // forma_status reaches forma-server's REST API, which is guarded too. If the
    // token were not forwarded this would come back UNAUTHORIZED rather than
    // with the font catalogue.
    let called = http
        .post(&mcp.url)
        .bearer_auth(TOKEN)
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-session-id", &session)
        .json(&json!({
            "jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": { "name": "forma_status", "arguments": {} }
        }))
        .send()
        .await
        .expect("tools/call")
        .text()
        .await
        .unwrap();

    assert!(called.contains("forma-server"), "{called}");
    assert!(
        !called.contains("UNAUTHORIZED"),
        "the token did not travel to forma-server: {called}"
    );
}

#[tokio::test]
async fn configured_oauth_discovery_is_public_and_named_in_the_challenge() {
    const DOMAIN: &str = "forma.example.org";
    const ISSUER: &str = "https://project.supabase.co/auth/v1";

    let dir = tempfile::tempdir().unwrap();
    let base_url = start_server(dir.path()).await;
    let mcp = start_mcp_with_discovery(&base_url, Some(TOKEN), Some(DOMAIN), Some(ISSUER)).await;

    let unauthorized = initialize(&mcp.url, None).await;
    assert_eq!(unauthorized.status(), reqwest::StatusCode::UNAUTHORIZED);
    assert_eq!(
        unauthorized
            .headers()
            .get("www-authenticate")
            .expect("discovery challenge"),
        "Bearer resource_metadata=\"https://forma.example.org/.well-known/oauth-protected-resource/mcp\""
    );

    let metadata = reqwest::Client::new()
        .get(format!(
            "{}.well-known/oauth-protected-resource/mcp",
            mcp.url
        ))
        .send()
        .await
        .expect("discovery document answers without a bearer");
    assert_eq!(metadata.status(), reqwest::StatusCode::OK);
    let document: Value = metadata.json().await.expect("discovery JSON");
    assert_eq!(document["resource"], "https://forma.example.org/mcp");
    assert_eq!(document["authorization_servers"], json!([ISSUER]));
    assert_eq!(document["bearer_methods_supported"], json!(["header"]));
}

#[test]
fn caddy_forwards_the_canonical_discovery_path_without_rewriting_it() {
    let caddyfile = include_str!("../../../deploy/caddy/Caddyfile");
    assert!(caddyfile.contains(
        "handle /.well-known/oauth-protected-resource/mcp {\n\t\treverse_proxy 127.0.0.1:4766\n\t}"
    ));
}

/// The first real deploy failed here, and every smoke check after it cascaded
/// from the same 403.
///
/// rmcp validates the inbound `Host` header against an allowlist — a real
/// protection: it stops a page in someone's browser from being pointed at an MCP
/// server bound on their own machine. Its default is loopback only, and behind a
/// reverse proxy the Host is the public name, so a deployment has to name itself
/// or nothing is read at all.
#[tokio::test]
async fn a_public_host_is_refused_until_the_deployment_names_itself() {
    let dir = tempfile::tempdir().unwrap();
    let base_url = start_server(dir.path()).await;

    let anonymous = start_mcp_for(&base_url, Some(TOKEN), None).await;
    let refused = reqwest::Client::new()
        .post(&anonymous.url)
        .bearer_auth(TOKEN)
        .header("host", "forma.example.org")
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
        .expect("it answers");
    assert_eq!(
        refused.status(),
        reqwest::StatusCode::FORBIDDEN,
        "an unnamed host must not be served"
    );
    drop(anonymous);

    let named = start_mcp_for(&base_url, Some(TOKEN), Some("forma.example.org")).await;
    let served = reqwest::Client::new()
        .post(&named.url)
        .bearer_auth(TOKEN)
        .header("host", "forma.example.org")
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
        .expect("it answers");
    assert_eq!(served.status(), reqwest::StatusCode::OK);
    assert!(served.text().await.unwrap().contains("forma-mcp"));
}

/// Naming the deployment must not stop the loopback defaults working, or every
/// local check and the stdio path lose their transport for no reason.
#[tokio::test]
async fn naming_a_public_host_keeps_loopback_working() {
    let dir = tempfile::tempdir().unwrap();
    let base_url = start_server(dir.path()).await;
    let mcp = start_mcp_for(&base_url, Some(TOKEN), Some("forma.example.org")).await;

    let response = initialize(&mcp.url, Some(TOKEN)).await;
    assert_eq!(response.status(), reqwest::StatusCode::OK);
}

/// `forma_status` is the "where am I" tool, and behind a proxy the URL it dials
/// is loopback on the server's own machine — so a local Forma and a remote one
/// answered identically, and the one question it exists to settle went
/// unanswered. Named deployments say which they are.
#[tokio::test]
async fn status_says_which_deployment_you_reached() {
    let dir = tempfile::tempdir().unwrap();
    let base_url = start_server(dir.path()).await;
    let mcp = start_mcp_for(&base_url, Some(TOKEN), Some("forma.example.org")).await;

    let response = initialize(&mcp.url, Some(TOKEN)).await;
    let session = response
        .headers()
        .get("mcp-session-id")
        .expect("a session id")
        .to_str()
        .unwrap()
        .to_string();
    let _ = response.text().await;

    let http = reqwest::Client::new();
    let call = |body: serde_json::Value| {
        http.post(&mcp.url)
            .bearer_auth(TOKEN)
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream")
            .header("mcp-session-id", &session)
            .json(&body)
            .send()
    };
    call(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
        .await
        .expect("initialized");

    let said = call(json!({
        "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": { "name": "forma_status", "arguments": {} }
    }))
    .await
    .expect("tools/call")
    .text()
    .await
    .unwrap();

    assert!(
        said.contains("forma.example.org"),
        "the summary should name the deployment: {said}"
    );
}
