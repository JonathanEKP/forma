//! Plan section 65: a smoke test that launches the binary over stdio.
//!
//! It runs without `forma-server`, which is deliberate: an agent must get a
//! clear, actionable error when the server is down, not a hang or a panic.

use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdout, Command};

const INIT: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"smoke","version":"0"}}}"#;
const INITIALIZED: &str = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;

struct Harness {
    child: Child,
    stdout: BufReader<ChildStdout>,
}

impl Harness {
    async fn start() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_forma-mcp"))
            .arg("--transport")
            .arg("stdio")
            // Point at a port nothing listens on, so "server down" is deterministic.
            .env("FORMA_SERVER_URL", "http://127.0.0.1:1")
            .env("FORMA_LOG", "error")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("forma-mcp binary must start");

        let stdout = BufReader::new(child.stdout.take().unwrap());
        let mut harness = Self { child, stdout };

        harness.send(INIT).await;
        harness.recv().await;
        harness.send(INITIALIZED).await;
        harness
    }

    async fn send(&mut self, line: &str) {
        let stdin = self.child.stdin.as_mut().unwrap();
        stdin.write_all(line.as_bytes()).await.unwrap();
        stdin.write_all(b"\n").await.unwrap();
        stdin.flush().await.unwrap();
    }

    async fn recv(&mut self) -> serde_json::Value {
        let mut line = String::new();
        let read = tokio::time::timeout(
            std::time::Duration::from_secs(20),
            self.stdout.read_line(&mut line),
        )
        .await
        .expect("forma-mcp must answer before the timeout")
        .expect("stdout must be readable");
        assert!(read > 0, "forma-mcp closed stdout unexpectedly");
        serde_json::from_str(&line).expect("each stdout line must be one JSON-RPC message")
    }

    async fn call(
        &mut self,
        id: u32,
        method: &str,
        params: serde_json::Value,
    ) -> serde_json::Value {
        let request = serde_json::json!({
            "jsonrpc": "2.0", "id": id, "method": method, "params": params,
        });
        self.send(&request.to_string()).await;
        loop {
            let message = self.recv().await;
            if message.get("id").and_then(|v| v.as_u64()) == Some(id as u64) {
                return message;
            }
        }
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

#[tokio::test]
async fn exposes_the_v1_tool_surface_over_stdio() {
    let mut mcp = Harness::start().await;
    let response = mcp.call(2, "tools/list", serde_json::json!({})).await;

    let names: Vec<&str> = response["result"]["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();

    // Plan section 48 fixed the surface at thirteen: few and composable, with
    // no per-property tools, because apply_operations covers those in a batch.
    // `list_templates` is the fourteenth, added deliberately -- §51.7's own V2
    // flow reads `get_brand -> list_templates -> suitable template?`, so the
    // plan anticipated it. Reading templates is not a per-property tool and
    // nothing else can express it: a layout that already worked here cannot be
    // fetched through apply_operations.
    assert_eq!(names.len(), 14, "got {names:?}");
    for expected in [
        "forma_status",
        "list_projects",
        "get_project",
        "get_brand",
        "list_assets",
        "list_templates",
        "get_design",
        "get_asset_request",
        "create_design",
        "apply_operations",
        "request_asset",
        "validate_design",
        "render_preview",
        "export_design",
    ] {
        assert!(
            names.contains(&expected),
            "{expected} missing from {names:?}"
        );
    }

    // Every tool must describe itself: the agent has no other documentation.
    for tool in response["result"]["tools"].as_array().unwrap() {
        assert!(
            tool["description"].as_str().is_some_and(|d| d.len() > 40),
            "tool {} needs a usable description",
            tool["name"]
        );
    }
}

#[tokio::test]
async fn reports_an_unreachable_server_instead_of_hanging() {
    let mut mcp = Harness::start().await;
    let response = mcp
        .call(
            2,
            "tools/call",
            serde_json::json!({ "name": "forma_status", "arguments": {} }),
        )
        .await;

    let result = &response["result"];
    assert_eq!(result["isError"], true, "expected a tool error: {result}");

    let text = result["content"][0]["text"].as_str().unwrap_or_default();
    // Plan section 51.9: the agent needs a code it can act on, not a stack trace.
    assert!(text.contains("SERVER_UNREACHABLE"), "got: {text}");
}

/// The process must survive a failed call and keep serving, otherwise an agent
/// would have to restart the whole MCP server after any hiccup.
#[tokio::test]
async fn stays_alive_after_a_failed_call() {
    let mut mcp = Harness::start().await;
    mcp.call(
        2,
        "tools/call",
        serde_json::json!({ "name": "forma_status", "arguments": {} }),
    )
    .await;

    let response = mcp.call(3, "tools/list", serde_json::json!({})).await;
    assert!(response["result"]["tools"].is_array());
}

/// Closing stdin is how a host says it is done, and the process must go with
/// it. A stdio server that outlived its host would be an orphan nobody knows
/// to kill.
#[tokio::test]
async fn closing_stdin_ends_the_process() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_forma-mcp"))
        .env("FORMA_SERVER_URL", "http://127.0.0.1:1")
        .env("FORMA_LOG", "error")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("forma-mcp must start");

    {
        let stdin = child.stdin.as_mut().unwrap();
        stdin.write_all(INIT.as_bytes()).await.unwrap();
        stdin.write_all(b"\n").await.unwrap();
        stdin.flush().await.unwrap();
    }

    // Dropping the handle closes the pipe, which is what a host exiting does.
    drop(child.stdin.take());

    let status = tokio::time::timeout(std::time::Duration::from_secs(20), child.wait())
        .await
        .expect("forma-mcp should exit when its host goes away")
        .expect("wait");

    assert!(status.success(), "exited with {status}");
}
