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
fn start_server(dir: &std::path::Path) -> String {
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

    format!("http://127.0.0.1:{}", rx.recv().expect("server port"))
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
    let base_url = start_server(dir.path());
    let mcp = start_mcp(&base_url, Some(TOKEN)).await;

    let response = initialize(&mcp.url, None).await;
    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);

    let body: Value = response.json().await.expect("the §32 envelope");
    assert_eq!(body["error"]["code"], "UNAUTHORIZED");

    let wrong = initialize(&mcp.url, Some("not-the-token")).await;
    assert_eq!(wrong.status(), reqwest::StatusCode::UNAUTHORIZED);
}

/// The whole chain: an authorized host reaches forma-mcp over HTTP, and
/// forma-mcp reaches a forma-server that is itself guarded. One credential for
/// the deployment, presented in both directions.
#[tokio::test]
async fn an_authorized_host_can_speak_mcp_and_the_token_travels_onward() {
    let dir = tempfile::tempdir().unwrap();
    let base_url = start_server(dir.path());
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
