//! What must hold of the manual's template (Pontis plan §6.1, Guards).
//!
//! The generated MCP skill has its own ten checks in
//! `apps/mcp/tests/skill_matches_the_code.rs`, which read real tool names and
//! so stay pointed at the generated file. These read the template, which holds
//! ids instead, and catch what rendering alone would let through.

use std::path::Path;

use agent_catalog::manual::{self, AgentManual, MCP_ONLY_CLOSE, MCP_ONLY_OPEN, TEMPLATE};
use agent_catalog::surface::names;
use agent_catalog::{operation, Door, REGISTRY};

/// As ts-rs writes the bindings during `cargo test`, this writes the skill,
/// and `scripts/check-bindings.sh` fails when that changed the committed file.
#[test]
fn export_the_mcp_skill() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(manual::MCP_SKILL_PATH);
    let skill = manual::mcp_skill();
    // Untouched when already current, so no other reader ever meets a file
    // halfway through being rewritten.
    if std::fs::read_to_string(&path).ok().as_deref() != Some(skill.as_str()) {
        std::fs::write(&path, skill).expect("skills/forma is writable");
    }
}

#[test]
fn every_name_in_the_template_resolves_in_the_catalog() {
    // Including the names in MCP-only blocks, which the Pontis rendering drops
    // before it could notice one that names nothing.
    let named = names(TEMPLATE).expect("the template's braces are balanced");
    assert!(
        named.len() > 30,
        "the template names almost nothing: {named:?}"
    );
    for id in named {
        assert!(operation(id).is_some(), "{{{id}}} names no operation");
    }
}

#[test]
fn the_template_renders_on_both_doors() {
    for door in [Door::Mcp, Door::Pontis] {
        let text = manual::render(TEMPLATE, door)
            .unwrap_or_else(|error| panic!("the manual does not render for {door:?}: {error}"));
        assert!(
            !text.contains("mcp-only"),
            "a marker survived on {door:?}, invisibly"
        );
        // A dropped block must not leave a gap where it was.
        assert!(!text.contains("\n\n\n"), "{door:?} has a double blank line");
    }
}

/// `render_text` validates braces and nothing else, so prose saying "call
/// get_design" renders cleanly on both doors and then lies on the Pontis one.
/// An operation is named with `{id}` or it is not named.
#[test]
fn no_mcp_tool_name_appears_in_the_template_literally() {
    for operation in REGISTRY {
        if let Some(tool) = operation.doors.mcp_tool() {
            assert!(
                !TEMPLATE.contains(tool),
                "the template names {tool} literally; write {{{}}}",
                operation.id
            );
        }
    }
}

/// The one door difference that is not a name runs one way, and only for the
/// operations that have no Pontis name at all. Anything the Pontis door can
/// reach is written outside a block, for both doors; otherwise the block grows
/// into a second manual.
#[test]
fn the_mcp_only_blocks_name_only_what_pontis_lacks() {
    let blocks = manual::mcp_only_blocks(TEMPLATE).expect("the markers pair up");
    assert!(!blocks.is_empty());
    for block in blocks {
        let named = names(&block).expect("a block's braces are balanced");
        assert!(
            !named.is_empty(),
            "an MCP-only block names no MCP-only operation, so it has no reason to exist:\n{block}"
        );
        for id in named {
            let reachable = operation(id).is_some_and(|op| op.doors.on_pontis());
            assert!(
                !reachable,
                "an MCP-only block names {id}, which Pontis has:\n{block}"
            );
        }
    }
}

#[test]
fn the_markers_are_lines_of_their_own_and_pair_up() {
    let render = |template: &str| manual::render(template, Door::Pontis);
    assert!(render(&format!("a\n{MCP_ONLY_OPEN}\nb\n")).is_err());
    assert!(render(&format!("a\n{MCP_ONLY_CLOSE}\n")).is_err());
    assert!(render(&format!(
        "{MCP_ONLY_OPEN}\n{MCP_ONLY_OPEN}\nb\n{MCP_ONLY_CLOSE}\n"
    ))
    .is_err());
    assert!(render(&format!("text {MCP_ONLY_OPEN}\n")).is_err());

    let template = format!("a\n{MCP_ONLY_OPEN}\n\n`{{forma.status}}`\n{MCP_ONLY_CLOSE}\n\nb\n");
    assert_eq!(render(&template).unwrap(), "a\n\nb\n");
    assert_eq!(
        manual::render(&template, Door::Mcp).unwrap(),
        "a\n\n`forma_status`\n\nb\n"
    );
}

#[test]
fn the_generated_skill_says_it_is_generated_under_its_front_matter() {
    let skill = manual::mcp_skill();
    assert!(skill.starts_with("---\nname: forma\n"));
    let after_front_matter = &skill[skill[4..].find("\n---\n").unwrap() + 9..];
    assert!(
        after_front_matter.starts_with("<!-- This file was generated from"),
        "{after_front_matter:.80}"
    );
}

/// The Pontis rendering is what `forma.guide` answers, and the revision is a
/// hash of exactly that text.
#[test]
fn the_revision_is_the_hash_of_the_text() {
    let answer = manual::pontis();
    assert_eq!(answer.revision, manual::revision(&answer.text));
    assert_eq!(
        answer.text,
        manual::render(TEMPLATE, Door::Pontis).unwrap(),
        "the answer is the template rendered for Pontis, whole"
    );
    assert!(answer.revision.starts_with("blake3:"));
    assert_ne!(
        answer.revision,
        manual::revision(&format!("{} ", answer.text))
    );
}

/// The whole manual, never sections, under a bound with room left. This fails
/// at 75% of the bound on purpose: it is what announces that the manual is
/// growing, early enough to decide what to do about it rather than finding out
/// from a refused read.
#[test]
fn the_manual_has_room_to_grow() {
    let bound = operation("forma.guide")
        .expect("the catalog declares forma.guide")
        .bounds
        .max_result_bytes as usize;
    let bytes = serde_json::to_vec::<AgentManual>(manual::pontis())
        .expect("the manual serializes")
        .len();
    assert!(
        bytes * 4 < bound * 3,
        "forma.guide answers {bytes} bytes, past 75% of its {bound}-byte bound: \
         raise the bound or shorten the manual, deliberately"
    );
}
