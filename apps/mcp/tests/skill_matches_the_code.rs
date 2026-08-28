//! The skill is a contract, and a contract that drifts from the code is worse
//! than none: an agent follows it confidently and is wrong.
//!
//! These checks are mechanical — every tool, operation, brand token and error
//! code the skill names has to exist. They cannot judge whether the advice is
//! good, which is what the prompts in docs/agent-evaluation.md are for.

use std::collections::HashSet;
use std::process::Stdio;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

const SKILL: &str = include_str!("../../../skills/forma/SKILL.md");
const OPERATIONS: &str = include_str!("../../../bindings/generated/DesignOperation.ts");

/// Everything in `backticks`, which is how the skill names anything real.
fn quoted(text: &str) -> HashSet<String> {
    let mut found = HashSet::new();
    let mut rest = text;
    while let Some(start) = rest.find('`') {
        let after = &rest[start + 1..];
        match after.find('`') {
            Some(end) => {
                found.insert(after[..end].to_string());
                rest = &after[end + 1..];
            }
            None => break,
        }
    }
    found
}

/// The tool names the running binary actually exposes.
async fn live_tool_names() -> HashSet<String> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_forma-mcp"))
        .env("FORMA_SERVER_URL", "http://127.0.0.1:1")
        .env("FORMA_LOG", "error")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("forma-mcp must start");

    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let stdin = child.stdin.as_mut().unwrap();

    for line in [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion":"2025-06-18","capabilities":{},
            "clientInfo":{"name":"skill-check","version":"0"}}})
        .to_string(),
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#.to_string(),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}).to_string(),
    ] {
        stdin.write_all(line.as_bytes()).await.unwrap();
        stdin.write_all(b"\n").await.unwrap();
    }
    stdin.flush().await.unwrap();

    let names = loop {
        let mut line = String::new();
        tokio::time::timeout(
            std::time::Duration::from_secs(20),
            stdout.read_line(&mut line),
        )
        .await
        .expect("timed out")
        .expect("stdout");

        let message: Value = serde_json::from_str(&line).expect("JSON-RPC");
        if message.get("id").and_then(Value::as_u64) == Some(2) {
            break message["result"]["tools"]
                .as_array()
                .expect("tools")
                .iter()
                .map(|t| t["name"].as_str().unwrap().to_string())
                .collect();
        }
    };

    let _ = child.start_kill();
    names
}

#[tokio::test]
async fn the_skill_names_every_tool_and_invents_none() {
    let live = live_tool_names().await;
    let mentioned = quoted(SKILL);

    for tool in &live {
        assert!(
            mentioned.contains(tool),
            "the skill never mentions the {tool} tool"
        );
    }

    // Anything that looks like a tool name but is not one would send an agent
    // looking for something that does not exist.
    for candidate in mentioned.iter().filter(|c| {
        c.ends_with("_design")
            || c.ends_with("_asset")
            || c.starts_with("list_")
            || c.starts_with("get_")
            || c.starts_with("forma_")
    }) {
        assert!(
            live.contains(candidate),
            "the skill names `{candidate}`, which is not a tool"
        );
    }
}

#[test]
fn the_skill_documents_every_operation() {
    // The bindings are generated from the enum and CI fails when they drift, so
    // they are a fair stand-in for the Rust definition.
    let real: HashSet<String> = OPERATIONS
        .split("\"op\": \"")
        .skip(1)
        .filter_map(|chunk| chunk.split('"').next())
        .map(str::to_string)
        .collect();

    assert_eq!(
        real.len(),
        13,
        "expected thirteen operations, found {real:?}"
    );

    let mentioned = quoted(SKILL);
    for op in &real {
        assert!(
            mentioned.contains(op),
            "the skill never documents the {op} operation"
        );
    }
}

#[test]
fn the_skill_uses_real_brand_tokens() {
    let mentioned = quoted(SKILL);
    let real: HashSet<String> = design_core::COLOR_TOKENS
        .iter()
        .chain(design_core::FONT_TOKENS.iter())
        .map(|token| format!("${token}"))
        .collect();

    for token in &real {
        assert!(
            SKILL.contains(token.as_str()),
            "the skill never mentions {token}"
        );
    }

    for candidate in mentioned
        .iter()
        // `$brand.*` is prose for "any of them", not a token.
        .filter(|c| c.starts_with("$brand.") && *c != "$brand.*")
    {
        assert!(
            real.contains(candidate),
            "the skill names {candidate}, which is not a brand token"
        );
    }
}

#[test]
fn the_skill_only_promises_error_codes_the_code_emits() {
    // Codes appear in the recovery table as bare SCREAMING_CASE in backticks.
    let mentioned: HashSet<String> = quoted(SKILL)
        .into_iter()
        .filter(|c| {
            c.len() > 4
                && c.chars().all(|ch| ch.is_ascii_uppercase() || ch == '_')
                && c.contains('_')
        })
        .collect();
    assert!(
        mentioned.len() > 10,
        "the recovery table looks empty: {mentioned:?}"
    );

    let sources = collect_rust_sources();
    for code in &mentioned {
        let quoted_code = format!("\"{code}\"");
        assert!(
            sources.contains(&quoted_code),
            "the skill promises {code}, which nothing in the workspace emits"
        );
    }
}

/// Every Rust source in the workspace, concatenated. Crude, and enough: a code
/// no crate contains is one the agent will never see.
fn collect_rust_sources() -> String {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut all = String::new();
    for dir in ["crates", "apps"] {
        walk(&root.join(dir), &mut all);
    }
    all
}

fn walk(dir: &std::path::Path, out: &mut String) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            if let Ok(text) = std::fs::read_to_string(&path) {
                out.push_str(&text);
            }
        }
    }
}

/// Structure is part of the contract: plan section 51.2 fixes the thirteen
/// sections so a host can find the rule it needs without reading it all.
#[test]
fn the_skill_keeps_its_mandated_structure() {
    for heading in [
        "## 1. Mental model",
        "## 2. DesignRecord vs portable DesignDocument",
        "## 3. Format, pages and z-order",
        "## 4. Brand tokens, voice and the font catalog",
        "## 5. Layers and their valid fields",
        "## 6. Assets and AssetRequest",
        "## 7. MCP tools",
        "## 8. apply_operations and base_version",
        "## 9. Validation",
        "## 10. Preview and visual inspection",
        "## 11. Error codes and recovery",
        "## 12. Design heuristics",
        "## 13. A full session",
    ] {
        assert!(SKILL.contains(heading), "missing section: {heading}");
    }

    // The two rules an agent is most likely to break, and the plan calls out by
    // name: never resend a stale base_version, never claim to have seen a
    // preview the host withheld.
    assert!(SKILL.contains("Never resend the same `base_version`"));
    // Written across a line break in the source, so the newline is normalised.
    let flat = SKILL.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(flat.contains("Never claim to have looked at a preview you were not given."));
}

/// The layer fields have to match what the renderer actually reads, since the
/// skill is where an agent learns them.
#[test]
fn the_skill_lists_the_real_layer_fields() {
    for field in [
        "font_family",
        "font_size",
        "font_weight",
        "line_height",
        "letter_spacing",
        "asset_id",
        "focal_point",
        "corner_radius",
        "stroke_width",
        "rotation_deg",
    ] {
        assert!(SKILL.contains(field), "the skill never mentions {field}");
    }

    // A field the schema does not have would be invented confidently.
    for invented in [
        "text_wrap",
        "font_style",
        "shadow",
        "border_radius",
        "z_index",
    ] {
        assert!(
            !SKILL.contains(invented),
            "the skill mentions {invented}, which is not in the schema"
        );
    }
}
