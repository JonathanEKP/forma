//! Phase 8 acceptance (plan section 77): from a host, get_brand, create_design,
//! apply_operations, validate_design and render_preview produce a real design.
//!
//! The real `forma-mcp` binary is driven over stdio against a real
//! `forma-server` on a real SQLite, because the seam between the two processes
//! is exactly what this phase adds. Mocking either side would test the mock.

use std::process::Stdio;
use std::sync::Arc;

use actix_web::{web, App, HttpServer};
use forma_server::{routes, state::AppState};
use renderer::FontRegistry;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdout, Command};

/// Starts forma-server on an ephemeral port and returns its base URL.
///
/// It runs in its own thread with its own actix System, so the test itself
/// stays on a plain tokio runtime and can drive a child process.
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
                    .app_data(web::Data::new(state.clone()))
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

struct Host {
    child: Child,
    stdout: BufReader<ChildStdout>,
    next_id: u32,
}

impl Host {
    /// Connects as a host called "Claude Code", so the actor recorded on every
    /// mutation is derived from a real client name.
    async fn connect(base_url: &str) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_forma-mcp"))
            .env("FORMA_SERVER_URL", base_url)
            .env("FORMA_LOG", "error")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("forma-mcp must start");

        let stdout = BufReader::new(child.stdout.take().unwrap());
        let mut host = Self {
            child,
            stdout,
            next_id: 1,
        };

        host.request(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "Claude Code", "version": "1" },
            }),
        )
        .await;

        host.send(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
            .await;
        host
    }

    async fn send(&mut self, line: &str) {
        let stdin = self.child.stdin.as_mut().unwrap();
        stdin.write_all(line.as_bytes()).await.unwrap();
        stdin.write_all(b"\n").await.unwrap();
        stdin.flush().await.unwrap();
    }

    async fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;

        self.send(
            &json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string(),
        )
        .await;

        loop {
            let mut line = String::new();
            let read = tokio::time::timeout(
                std::time::Duration::from_secs(30),
                self.stdout.read_line(&mut line),
            )
            .await
            .expect("forma-mcp must answer before the timeout")
            .expect("stdout readable");
            assert!(read > 0, "forma-mcp closed stdout");

            let message: Value = serde_json::from_str(&line).expect("one JSON-RPC message");
            if message.get("id").and_then(Value::as_u64) == Some(id as u64) {
                return message;
            }
        }
    }

    /// Calls a tool and returns the result, failing loudly if it errored.
    async fn call(&mut self, name: &str, arguments: Value) -> Value {
        let response = self
            .request(
                "tools/call",
                json!({ "name": name, "arguments": arguments }),
            )
            .await;
        let result = &response["result"];
        assert_ne!(
            result["isError"],
            json!(true),
            "{name} failed: {}",
            result["content"][0]["text"]
        );
        result.clone()
    }

    async fn call_expecting_error(&mut self, name: &str, arguments: Value) -> Value {
        let response = self
            .request(
                "tools/call",
                json!({ "name": name, "arguments": arguments }),
            )
            .await;
        let result = &response["result"];
        assert_eq!(
            result["isError"],
            json!(true),
            "{name} unexpectedly succeeded"
        );
        result.clone()
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

/// Sets up what a person owns: the project and its Brand Kit. There is no tool
/// for either, deliberately — an agent works inside a workspace the user made.
async fn seed(base_url: &str, font: &str) -> String {
    let http = reqwest::Client::new();

    let project: Value = http
        .post(format!("{base_url}/api/v1/projects"))
        .json(&json!({ "name": "Launch" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let project_id = project["id"].as_str().unwrap().to_string();

    let response = http
        .put(format!("{base_url}/api/v1/projects/{project_id}/brand"))
        .json(&json!({
            "name": "Default",
            "colors": {
                "primary": "#FF5436", "secondary": "#FDBA2D", "accent": "#7C3AED",
                "background": "#101010", "surface": "#191919", "text": "#FFFFFF",
                "text_inverse": "#101010", "muted": "#A3A3A3"
            },
            "typography": { "display": font, "body": font, "mono": font },
            "voice": "Direct, technical and brief."
        }))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success(), "brand kit setup failed");

    project_id
}

#[tokio::test]
async fn an_agent_designs_validates_and_renders_a_real_piece() {
    let dir = tempfile::tempdir().unwrap();
    let base_url = start_server(dir.path());

    // Whatever the shipped font directory provides, so this does not encode the
    // default-font decision of plan section 93.2.
    let fonts = FontRegistry::from_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/fonts"))
        .expect("fonts");
    let font = fonts.families().first().cloned().expect("a font family");

    let project_id = seed(&base_url, &font).await;
    let mut host = Host::connect(&base_url).await;

    // --- the agent orients itself ------------------------------------------
    let status = host.call("forma_status", json!({})).await;
    assert_eq!(status["structuredContent"]["name"], "forma-server");

    let projects = host.call("list_projects", json!({})).await;
    assert_eq!(
        projects["structuredContent"]["projects"][0]["id"],
        project_id
    );

    // The font catalog travels with the brand, so the agent cannot name a
    // family the renderer lacks.
    let brand = host
        .call("get_brand", json!({ "project_id": project_id }))
        .await;
    assert_eq!(
        brand["structuredContent"]["brand"]["colors"]["primary"],
        "#FF5436"
    );
    assert!(brand["structuredContent"]["fonts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f == &json!(font)));

    // --- it creates and edits ----------------------------------------------
    let created = host
        .call(
            "create_design",
            json!({ "project_id": project_id, "name": "Teaser", "preset": "square" }),
        )
        .await;
    let design_id = created["structuredContent"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let page_id = created["structuredContent"]["document"]["pages"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(created["structuredContent"]["version"], 1);

    let edited = host
        .call(
            "apply_operations",
            json!({
                "design_id": design_id,
                "base_version": 1,
                "operations": [
                    { "op": "set_background", "page_id": page_id, "color": "$brand.background" },
                    {
                        "op": "add_layer",
                        "page_id": page_id,
                        "layer": {
                            "type": "text",
                            "id": "aaaaaaaa-0000-4000-8000-00000000000a",
                            "name": "Headline",
                            "role": "headline",
                            "visible": true, "locked": false, "opacity": 1.0,
                            "transform": { "x": 90.0, "y": 300.0, "width": 900.0, "height": 400.0, "rotation_deg": 0.0 },
                            "text": "SHIP\nWITH AI",
                            "font_family": font,
                            "font_size": 120.0,
                            "font_weight": 700,
                            "line_height": 1.1,
                            "letter_spacing": 0.0,
                            "align": "left",
                            "color": "$brand.text"
                        }
                    }
                ]
            }),
        )
        .await;

    // One batch, one version (plan section 28).
    assert_eq!(edited["structuredContent"]["version"], 2);
    assert!(edited["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("1 layer"));

    // --- it checks its work ------------------------------------------------
    let report = host
        .call("validate_design", json!({ "design_id": design_id }))
        .await;
    assert_eq!(
        report["structuredContent"]["valid"],
        json!(true),
        "issues: {}",
        report["structuredContent"]["issues"]
    );

    // --- and looks at it ---------------------------------------------------
    let preview = host
        .call("render_preview", json!({ "design_id": design_id }))
        .await;

    // Plan section 49: the structured result AND the image itself.
    assert_eq!(preview["structuredContent"]["version"], 2);
    let image = preview["content"]
        .as_array()
        .unwrap()
        .iter()
        .find(|block| block["type"] == "image")
        .expect("render_preview must return an image block");
    assert_eq!(image["mimeType"], "image/png");

    let png = base64_decode(image["data"].as_str().unwrap());
    assert_eq!(&png[1..4], b"PNG");
    let pixmap = resvg::tiny_skia::Pixmap::decode_png(&png).expect("a real PNG");
    assert_eq!((pixmap.width(), pixmap.height()), (1080, 1080));

    // The background token resolved, so the agent's design really rendered.
    let corner = pixmap.pixel(5, 5).unwrap();
    assert_eq!(
        (corner.red(), corner.green(), corner.blue()),
        (0x10, 0x10, 0x10)
    );

    // The base64 is not repeated in the structured result: it is already above.
    assert!(preview["structuredContent"].get("png_base64").is_none());

    // --- and ships it ------------------------------------------------------
    let export = host
        .call("export_design", json!({ "design_id": design_id }))
        .await;
    assert_eq!(export["structuredContent"]["export"]["design_version"], 2);
    // One page, so one file; the names come from the version it pinned.
    assert_eq!(
        export["structuredContent"]["files"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    // --- the history knows who did all that --------------------------------
    let history: Value = reqwest::get(format!("{base_url}/api/v1/designs/{design_id}/history"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(history[0]["actor_type"], "agent");
    // Derived from the host's own name, per plan section 61.
    assert_eq!(history[0]["actor_id"], "agent:claude-code");
}

/// The recovery path of plan section 51.9, which is the difference between an
/// agent that recovers and one that loops.
#[tokio::test]
async fn a_stale_base_version_is_told_what_to_do() {
    let dir = tempfile::tempdir().unwrap();
    let base_url = start_server(dir.path());
    let fonts = FontRegistry::from_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/fonts"))
        .expect("fonts");
    let font = fonts.families().first().cloned().unwrap();
    let project_id = seed(&base_url, &font).await;

    let mut host = Host::connect(&base_url).await;
    let created = host
        .call(
            "create_design",
            json!({ "project_id": project_id, "name": "Teaser" }),
        )
        .await;
    let design_id = created["structuredContent"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let page_id = created["structuredContent"]["document"]["pages"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let rename = |name: &str| {
        json!({
            "design_id": design_id,
            "base_version": 1,
            "operations": [{ "op": "set_background", "page_id": page_id, "color": "$brand.surface" }],
            "_note": name,
        })
    };

    host.call("apply_operations", rename("first")).await;

    let rejected = host
        .call_expecting_error("apply_operations", rename("second"))
        .await;

    let text = rejected["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("VERSION_CONFLICT"), "got: {text}");
    // The hint is what stops a blind retry.
    assert!(text.contains("rebase"), "no recovery hint in: {text}");

    let error = &rejected["structuredContent"]["error"];
    assert_eq!(error["code"], "VERSION_CONFLICT");
    assert_eq!(error["details"]["expected"], 1);
    assert_eq!(error["details"]["actual"], 2);
}

/// Plan section 23: a missing image is recorded, not invented.
#[tokio::test]
async fn a_missing_image_becomes_a_request_the_agent_can_poll() {
    let dir = tempfile::tempdir().unwrap();
    let base_url = start_server(dir.path());
    let fonts = FontRegistry::from_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/fonts"))
        .expect("fonts");
    let font = fonts.families().first().cloned().unwrap();
    let project_id = seed(&base_url, &font).await;

    let mut host = Host::connect(&base_url).await;

    // Nothing to reuse, which is what the agent checks first.
    let assets = host
        .call("list_assets", json!({ "project_id": project_id }))
        .await;
    assert_eq!(assets["structuredContent"]["assets"], json!([]));

    let requested = host
        .call(
            "request_asset",
            json!({
                "project_id": project_id,
                "purpose": "hero photo",
                "description": "A futuristic coffee shop at night",
                "suggested_prompt": "wide shot, neon, rain",
                "requested_width": 1600,
                "requested_height": 900,
            }),
        )
        .await;

    let request_id = requested["structuredContent"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(requested["structuredContent"]["status"], "pending");

    let polled = host
        .call(
            "get_asset_request",
            json!({ "asset_request_id": request_id }),
        )
        .await;
    assert_eq!(polled["structuredContent"]["status"], "pending");
    assert!(polled["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("pending"));
}

fn base64_decode(encoded: &str) -> Vec<u8> {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .expect("valid base64")
}

/// Plan section 85, over the wire the agent actually uses.
///
/// A person keeps a layout that worked; the agent finds it, reads the brief out
/// of it, and starts from it instead of an empty canvas. `list_templates` is the
/// fourteenth tool, and this is the reason it earns the place: nothing else in
/// the surface can express "a layout that already worked here", and the flow of
/// section 51.7 reads `get_brand -> list_templates -> suitable template?`.
#[tokio::test]
async fn an_agent_starts_from_a_layout_a_person_kept() {
    let dir = tempfile::tempdir().unwrap();
    let base_url = start_server(dir.path());
    let fonts = FontRegistry::from_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/fonts"))
        .expect("fonts");
    let font = fonts.families().first().expect("a font").clone();
    let project_id = seed(&base_url, &font).await;
    let http = reqwest::Client::new();

    // The person builds a cover and keeps it. There is deliberately no tool for
    // this: deciding which design is worth keeping is theirs.
    let design: Value = http
        .post(format!("{base_url}/api/v1/projects/{project_id}/designs"))
        .json(&json!({ "name": "Issue 07" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let design_id = design["id"].as_str().unwrap();
    let page_id = design["document"]["pages"][0]["id"].as_str().unwrap();

    let updated: Value = http
        .post(format!("{base_url}/api/v1/designs/{design_id}/operations"))
        .json(&json!({
            "base_version": design["version"],
            "operations": [{
                "op": "add_layer",
                "page_id": page_id,
                "layer": {
                    "type": "text",
                    "id": uuid::Uuid::new_v4().to_string(),
                    "name": "Headline",
                    "role": "headline",
                    "transform": { "x": 96.0, "y": 900.0, "width": 888.0, "height": 200.0 },
                    "text": "llueve y la ciudad",
                    "font_family": font,
                    "font_size": 72.0,
                    "font_weight": 700,
                    "color": "$brand.text"
                }
            }]
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(updated["version"], 2);

    let template: Value = http
        .post(format!("{base_url}/api/v1/designs/{design_id}/template"))
        .json(&json!({ "name": "Portada de número" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let mut host = Host::connect(&base_url).await;

    let listed = host
        .call("list_templates", json!({ "project_id": project_id }))
        .await;
    let templates = listed["structuredContent"]["templates"].as_array().unwrap();
    assert_eq!(templates.len(), 1);

    // The brief, not just the document: the agent is told what the slot is for
    // and how much text the layout really held.
    let role = &templates[0]["roles"][0];
    assert_eq!(role["role"], "headline");
    assert_eq!(role["page_index"], 0);
    assert_eq!(role["max_chars"], 18);
    assert_eq!(role["sample_text"], "llueve y la ciudad");

    let made = host
        .call(
            "create_design",
            json!({
                "project_id": project_id,
                "name": "Issue 08",
                "template_id": template["id"].as_str().unwrap()
            }),
        )
        .await;
    let record = &made["structuredContent"];

    assert_eq!(record["version"], 1);
    assert_eq!(record["project_id"], project_id.as_str());
    assert_eq!(
        record["document"]["pages"][0]["layers"][0]["text"], "llueve y la ciudad",
        "the copy should carry the template's words for the agent to replace"
    );
    assert_ne!(
        record["document"]["pages"][0]["layers"][0]["id"],
        updated["document"]["pages"][0]["layers"][0]["id"],
        "a design from a template must not share the template's layer ids"
    );
}
