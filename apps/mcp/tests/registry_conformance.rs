//! The MCP door against the registry, through the real binary.
//!
//! `server.rs`'s unit tests check the router before it is served. This checks
//! what a host actually receives over stdio, so a change anywhere between the
//! registry and the wire — a handler, the router, rmcp's own serialization —
//! fails here rather than in front of a model.
//!
//! The Pontis door's half is `scripts/check-bindings.sh`, which fails when the
//! generated `capabilityCatalog.ts` no longer matches what is committed.

use std::collections::BTreeMap;
use std::process::Stdio;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

async fn served_tools() -> Vec<Value> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_forma-mcp"))
        .arg("--transport")
        .arg("stdio")
        // Listing tools reaches no server, so nothing needs to listen here.
        .env("FORMA_SERVER_URL", "http://127.0.0.1:1")
        .env("FORMA_LOG", "error")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("forma-mcp binary must start");

    let mut stdin = child.stdin.take().unwrap();
    for message in [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion":"2025-06-18","capabilities":{},
            "clientInfo":{"name":"conformance","version":"0"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
    ] {
        stdin
            .write_all(format!("{message}\n").as_bytes())
            .await
            .unwrap();
    }
    stdin.flush().await.unwrap();

    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    loop {
        let line = tokio::time::timeout(std::time::Duration::from_secs(20), lines.next_line())
            .await
            .expect("forma-mcp must answer before the timeout")
            .unwrap()
            .expect("forma-mcp closed stdout before listing its tools");
        let message: Value = serde_json::from_str(&line).unwrap();
        if message["id"] == 2 {
            return message["result"]["tools"].as_array().unwrap().clone();
        }
    }
}

#[tokio::test]
async fn the_mcp_door_serves_exactly_the_registry() {
    let served: BTreeMap<String, Value> = served_tools()
        .await
        .into_iter()
        .map(|tool| (tool["name"].as_str().unwrap().to_string(), tool))
        .collect();

    let expected: BTreeMap<String, Value> = agent_catalog::mcp_tools()
        .into_iter()
        .map(|tool| {
            let mut annotations = json!({ "readOnlyHint": tool.hints.read_only });
            if let Some(destructive) = tool.hints.destructive {
                annotations["destructiveHint"] = json!(destructive);
            }
            if let Some(open_world) = tool.hints.open_world {
                annotations["openWorldHint"] = json!(open_world);
            }
            let tool = json!({
                "name": tool.name,
                "description": tool.description,
                "inputSchema": tool.input_schema,
                "annotations": annotations,
            });
            (tool["name"].as_str().unwrap().to_string(), tool)
        })
        .collect();

    // Both directions, and whole tools rather than chosen fields: a key the
    // registry does not produce is as much a drift as a value that differs.
    assert_eq!(
        served.keys().collect::<Vec<_>>(),
        expected.keys().collect::<Vec<_>>(),
        "the served tool names and the registry's differ"
    );
    for (name, tool) in &expected {
        assert_eq!(
            &served[name], tool,
            "{name} as served differs from the registry"
        );
    }
}
