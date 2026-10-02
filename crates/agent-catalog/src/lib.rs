//! The one definition of every operation an agent can reach (plan §10(b) D5).
//!
//! Forma has two agent-facing doors: forma-mcp, which serves MCP hosts directly,
//! and the capability catalog the Pontis sidecar declares. Both describe the
//! same product, so both are rendered from [`REGISTRY`] rather than authored:
//! forma-mcp takes its tool list from [`mcp_tools`], and the sidecar imports
//! [`pontis_catalog`] as generated TypeScript. Each side has a test that fails
//! when it stops matching — forma-mcp's against its live tool list, the
//! sidecar's through `scripts/check-bindings.sh`.
//!
//! A drifting agent contract is worse than none, because the agent follows it
//! confidently and is wrong. That is why the skill has
//! `skill_matches_the_code.rs`, and it is why this crate exists.
//!
//! It lives in its own crate because nothing else may hold it. `design-core`
//! must not know about agents, and risk classes and approval cards are agent
//! concepts; `application` owns use cases, not how they are described to a
//! model; and putting it in forma-mcp would make the Pontis door a by-product of
//! the MCP one, when the registry is upstream of both.

pub mod descriptor;
pub mod operations;
pub mod params;
pub mod surface;

use serde_json::{Map, Value};

pub use descriptor::{
    CapabilityBounds, CapabilityKind, CapabilityPresentation, CapabilityRisk, CapabilitySpec,
};
pub use operations::{Doors, Operation, Presentation, REGISTRY};
pub use surface::Door;

/// The operation with this capability id, if the registry has one.
pub fn operation(id: &str) -> Option<&'static Operation> {
    REGISTRY.iter().find(|operation| operation.id == id)
}

/// One forma-mcp tool, as the registry defines it.
#[derive(Debug, Clone, PartialEq)]
pub struct McpTool {
    pub name: &'static str,
    pub description: String,
    pub input_schema: Map<String, Value>,
    pub hints: McpHints,
}

/// MCP's tool annotations, derived from the risk class so the two cannot
/// disagree. Hints only, by the MCP spec's own account — nothing enforces them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct McpHints {
    pub read_only: bool,
    pub destructive: Option<bool>,
    pub open_world: Option<bool>,
}

impl McpHints {
    pub fn of(risk: CapabilityRisk) -> Self {
        let (read_only, destructive, open_world) = match risk {
            CapabilityRisk::Read => (true, None, None),
            CapabilityRisk::Write | CapabilityRisk::Sensitive => (false, Some(false), None),
            CapabilityRisk::Destructive => (false, Some(true), None),
            CapabilityRisk::ExternalSideEffect => (false, Some(false), Some(true)),
        };
        Self {
            read_only,
            destructive,
            open_world,
        }
    }
}

/// Every operation the MCP door exposes, rendered for it.
///
/// # Panics
///
/// When a description names an operation the MCP door cannot reach. That is a
/// broken registry, and the registry's own tests catch it before any binary
/// is built from it.
pub fn mcp_tools() -> Vec<McpTool> {
    REGISTRY
        .iter()
        .filter_map(|operation| {
            let name = surface::name_on(operation, Door::Mcp)?;
            Some(McpTool {
                name,
                description: rendered(surface::render_text(operation.description, Door::Mcp)),
                input_schema: rendered(surface::render_schema(operation, Door::Mcp)),
                hints: McpHints::of(operation.risk),
            })
        })
        .collect()
}

/// Every operation the Pontis door exposes, rendered for it.
///
/// # Panics
///
/// As [`mcp_tools`].
pub fn pontis_catalog() -> Vec<CapabilitySpec> {
    REGISTRY
        .iter()
        .filter(|operation| operation.doors.on_pontis())
        .map(|operation| CapabilitySpec {
            id: operation.id.to_string(),
            kind: operation.kind,
            description: rendered(surface::render_text(operation.description, Door::Pontis)),
            input_schema: Value::Object(rendered(surface::render_schema(operation, Door::Pontis))),
            risk: operation.risk,
            bounds: operation.bounds,
            presentation: operation.presentation.map(|p| CapabilityPresentation {
                summary: p.summary.to_string(),
                important_arguments: p
                    .important_arguments
                    .iter()
                    .map(|a| a.to_string())
                    .collect(),
                affected_resource_count_from: p.affected_resource_count_from.map(str::to_string),
                resource_ids_from: p.resource_ids_from.map(str::to_string),
            }),
        })
        .collect()
}

fn rendered<T>(result: Result<T, String>) -> T {
    result.unwrap_or_else(|error| panic!("the agent catalog does not render: {error}"))
}

/// The generated module the sidecar imports: the Pontis catalog as a typed
/// constant, so `tsc` checks the data against [`CapabilitySpec`] as well.
pub fn pontis_catalog_module() -> String {
    let catalog = serde_json::to_string_pretty(&pontis_catalog())
        .expect("a catalog of strings and numbers serializes");
    format!(
        "// This file was generated from crates/agent-catalog by `cargo test`. Do not edit this file manually.\n\
         import type {{ CapabilitySpec }} from \"./CapabilitySpec\";\n\
         \n\
         export const capabilityCatalog: ReadonlyArray<CapabilitySpec> = {catalog};\n"
    )
}
