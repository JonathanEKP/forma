//! What must hold of the registry itself, whichever door renders it.
//!
//! The doors' own conformance lives beside them: forma-mcp compares its live
//! tool list against `mcp_tools()`, and `scripts/check-bindings.sh` fails when
//! the Pontis catalog this file writes no longer matches what is committed.

use std::collections::HashSet;

use agent_catalog::surface::{name_on, render_schema, render_text};
use agent_catalog::{
    mcp_tools, pontis_catalog, pontis_catalog_module, CapabilityKind, CapabilityRisk, Door, Doors,
    REGISTRY,
};
use serde_json::Value;

/// ts-rs writes the bindings during `cargo test`, and so does this: the
/// catalog is data rather than a type, but it reaches the sidecar the same way
/// and `check-bindings.sh` guards it the same way.
#[test]
fn export_the_pontis_catalog() {
    let dir =
        std::env::var("TS_RS_EXPORT_DIR").expect("TS_RS_EXPORT_DIR comes from .cargo/config.toml");
    std::fs::write(
        std::path::Path::new(&dir).join("capabilityCatalog.ts"),
        pontis_catalog_module(),
    )
    .expect("the bindings directory is writable");
}

#[test]
fn every_id_and_every_tool_name_is_unique() {
    let mut ids = HashSet::new();
    let mut tools = HashSet::new();
    for operation in REGISTRY {
        assert!(ids.insert(operation.id), "{} twice", operation.id);
        if let Some(tool) = operation.doors.mcp_tool() {
            assert!(tools.insert(tool), "tool {tool} twice");
        }
    }
}

/// Pontis's capability-id grammar, which is **its contract, not our style**:
/// `CAPABILITY_ID_PATTERN` in `pontis/packages/capability-core/src/capability.ts`,
///
/// ```text
/// /^[a-z][a-zA-Z0-9]*(\.[a-z][a-zA-Z0-9]*)+$/
/// ```
///
/// enforced by `defineCapability` at registration — so an id outside it is a
/// catalog the sidecar cannot load at all. Four ids shipped that way
/// (`design.apply_operations` among them) because nothing here asked; written
/// out by hand rather than with a regex crate, and kept to the letter of that
/// pattern: dot-separated segments, at least two, each a lowercase ASCII letter
/// followed by ASCII letters and digits. No underscore, in any segment.
fn is_pontis_capability_id(id: &str) -> bool {
    let segments: Vec<&str> = id.split('.').collect();
    segments.len() >= 2
        && segments.iter().all(|segment| {
            let mut bytes = segment.bytes();
            bytes.next().is_some_and(|first| first.is_ascii_lowercase())
                && bytes.all(|byte| byte.is_ascii_alphanumeric())
        })
}

#[test]
fn every_id_is_one_pontis_will_register() {
    for operation in REGISTRY {
        assert!(
            is_pontis_capability_id(operation.id),
            "{} is outside Pontis's capability-id grammar; defineCapability refuses it",
            operation.id
        );
    }
    // The checker itself, against the cases that slipped through before.
    for refused in [
        "design.apply_operations",
        "asset_request.get",
        "design",
        "Design.get",
        "design.",
        "design..get",
        "design.1get",
    ] {
        assert!(!is_pontis_capability_id(refused), "{refused}");
    }
    for accepted in ["design.applyOperations", "ui.openDesign", "a.b.c2"] {
        assert!(is_pontis_capability_id(accepted), "{accepted}");
    }
}

#[test]
fn every_operation_renders_on_every_door_that_exposes_it() {
    for operation in REGISTRY {
        for door in [Door::Mcp, Door::Pontis] {
            if name_on(operation, door).is_none() {
                continue;
            }
            render_text(operation.description, door)
                .unwrap_or_else(|e| panic!("{} on {door:?}: {e}", operation.id));
            render_schema(operation, door)
                .unwrap_or_else(|e| panic!("{} on {door:?}: {e}", operation.id));
        }
    }
    // And the entry points that panic on a broken registry do not.
    assert_eq!(mcp_tools().len(), 14);
    assert!(!pontis_catalog().is_empty());
}

#[test]
fn a_name_resolves_only_on_a_door_that_has_it() {
    assert_eq!(
        render_text("call {design.get} then {project.list}", Door::Mcp).unwrap(),
        "call get_design then list_projects"
    );
    assert_eq!(
        render_text("call {design.get}", Door::Pontis).unwrap(),
        "call design.get"
    );
    // MCP-only and Pontis-only operations cannot be named on the other door,
    // so no description can send an agent to a tool it does not have.
    assert!(render_text("{design.renderPreview}", Door::Pontis).is_err());
    assert!(render_text("{design.undo}", Door::Mcp).is_err());
    assert!(render_text("{design.nonexistent}", Door::Mcp).is_err());
    assert!(render_text("a stray { brace", Door::Mcp).is_err());
    assert!(render_text("a stray } brace", Door::Mcp).is_err());
}

#[test]
fn a_doubled_brace_is_a_literal_one() {
    assert_eq!(
        render_text(r#"{design.get} {{ "design_id": "d1" }}"#, Door::Mcp).unwrap(),
        r#"get_design { "design_id": "d1" }"#
    );
    // Escaped, an id is text: it names nothing and so cannot fail to resolve.
    assert_eq!(
        render_text("{{design.nonexistent}}", Door::Pontis).unwrap(),
        "{design.nonexistent}"
    );
    assert_eq!(render_text("}}{{", Door::Mcp).unwrap(), "}{");
    // One brace of a pair is still a stray.
    assert!(render_text("{{ half }", Door::Mcp).is_err());
}

#[test]
fn the_doors_are_the_ones_the_plan_settled() {
    let mcp_only: Vec<&str> = REGISTRY
        .iter()
        .filter(|o| matches!(o.doors, Doors::McpOnly { .. }))
        .map(|o| o.id)
        .collect();
    assert_eq!(mcp_only, ["forma.status", "design.renderPreview"]);

    let pontis: HashSet<&str> = pontis_catalog().into_iter().map(|c| leak(c.id)).collect();
    for id in [
        "project.list",
        "project.get",
        "brand.get",
        "design.list",
        "design.get",
        "design.history",
        "asset.list",
        "template.list",
        "assetRequest.get",
        "design.validate",
        "design.create",
        "design.applyOperations",
        "design.undo",
        "design.export",
        "assetRequest.create",
        "template.createDesign",
        "ui.openDesign",
        "ui.selectLayer",
        "ui.setPanel",
        "ui.setZoom",
        // Plan §6.1: the manual, on this door only.
        "forma.guide",
    ] {
        assert!(pontis.contains(id), "plan §3 lists {id}");
    }
}

#[test]
fn the_deliberate_absences_stay_absent() {
    // Deciding a project exists, what a brand is, which design is worth keeping
    // as a template, and that a design should stop existing are the user's.
    // The last is plan §10(b) D3, still open; the rest are CLAUDE.md § MCP surface.
    for operation in REGISTRY {
        let id = operation.id;
        assert!(
            !id.starts_with("project.") || id == "project.list" || id == "project.get",
            "{id}"
        );
        assert!(!id.starts_with("brand.") || id == "brand.get", "{id}");
        assert!(
            !id.starts_with("template.") || id == "template.list" || id == "template.createDesign",
            "{id}"
        );
        assert!(!id.contains("delete"), "{id}");
    }
}

#[test]
fn kinds_and_risks_follow_the_contract() {
    for operation in REGISTRY {
        match operation.kind {
            // `docs/06` § UI: risk is always read, and it is a Pontis concept.
            CapabilityKind::Ui => {
                assert_eq!(operation.risk, CapabilityRisk::Read, "{}", operation.id);
                assert_eq!(operation.doors, Doors::PontisOnly, "{}", operation.id);
            }
            CapabilityKind::Resource => {
                assert_eq!(operation.risk, CapabilityRisk::Read, "{}", operation.id)
            }
            CapabilityKind::Command => {
                assert_ne!(operation.risk, CapabilityRisk::Read, "{}", operation.id)
            }
        }
    }
}

#[test]
fn every_command_explains_itself_in_terms_of_its_own_input() {
    for operation in REGISTRY {
        let Some(presentation) = operation.presentation else {
            assert_ne!(
                operation.kind,
                CapabilityKind::Command,
                "{}: Pontis refuses to register a command with no presentation",
                operation.id
            );
            continue;
        };
        let schema = (operation.input)();
        let properties = schema["properties"].as_object().expect("properties");

        let summary = presentation.summary;
        assert!(summary.len() > 20, "{}: {summary:?}", operation.id);
        // A sentence for a person, not the tool's name again.
        assert!(
            !summary.contains('_') && !summary.contains('.'),
            "{}: {summary:?} reads like an identifier",
            operation.id
        );

        for argument in presentation.important_arguments {
            assert!(
                properties.contains_key(*argument),
                "{}: {argument}",
                operation.id
            );
        }
        if let Some(field) = presentation.resource_ids_from {
            assert!(properties.contains_key(field), "{}: {field}", operation.id);
        }
        if let Some(field) = presentation.affected_resource_count_from {
            assert_eq!(
                properties[field]["type"], "array",
                "{}: the affected count is an array's length",
                operation.id
            );
        }
    }
}

#[test]
fn every_schema_is_an_object_with_no_boolean_subschema() {
    // A host converts the whole tool list at once, and one emitted `items: true`
    // took every tool down with it, not just the one that carried it.
    for operation in REGISTRY {
        for door in [Door::Mcp, Door::Pontis] {
            if name_on(operation, door).is_none() {
                continue;
            }
            let schema = Value::Object(render_schema(operation, door).unwrap());
            assert_eq!(schema["type"], "object", "{} on {door:?}", operation.id);
            assert_no_boolean_subschema(&schema, operation.id);
        }
    }
}

#[test]
fn the_pontis_door_speaks_the_dialect_ajv_compiles() {
    // Pontis compiles input schemas with Ajv in strict draft-07 mode, which
    // throws on a 2020-12 `$schema` and on a format it does not know — and it
    // knows none of schemars' numeric ones.
    for capability in pontis_catalog() {
        let text = capability.input_schema.to_string();
        assert!(!text.contains("\"$schema\""), "{}: {text}", capability.id);
        assert!(!text.contains("\"format\""), "{}: {text}", capability.id);
        assert!(!text.contains("\"$ref\""), "{}: {text}", capability.id);
    }
}

#[test]
fn every_operation_declares_usable_bounds() {
    for operation in REGISTRY {
        assert!(operation.bounds.max_result_bytes > 0, "{}", operation.id);
        assert!(operation.bounds.timeout_ms > 0, "{}", operation.id);
    }
}

fn assert_no_boolean_subschema(value: &Value, id: &str) {
    // The keywords whose value is itself a schema, or a map or list of them.
    const SCHEMA: &[&str] = &[
        "items",
        "additionalProperties",
        "not",
        "contains",
        "propertyNames",
        "if",
        "then",
        "else",
        "unevaluatedItems",
        "unevaluatedProperties",
    ];
    const SCHEMA_MAPS: &[&str] = &["properties", "patternProperties", "$defs", "definitions"];
    const SCHEMA_LISTS: &[&str] = &["anyOf", "allOf", "oneOf", "prefixItems"];

    let Value::Object(object) = value else {
        panic!("{id}: a schema must be an object, got {value}");
    };
    for (key, child) in object {
        if SCHEMA.contains(&key.as_str()) {
            assert_no_boolean_subschema(child, id);
        } else if SCHEMA_MAPS.contains(&key.as_str()) {
            for schema in child.as_object().expect("a map of schemas").values() {
                assert_no_boolean_subschema(schema, id);
            }
        } else if SCHEMA_LISTS.contains(&key.as_str()) {
            for schema in child.as_array().expect("a list of schemas") {
                assert_no_boolean_subschema(schema, id);
            }
        }
    }
}

fn leak(id: String) -> &'static str {
    Box::leak(id.into_boxed_str())
}
