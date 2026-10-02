//! What the catalog says about one operation, in the vocabulary of
//! `pontis/docs/06-capability-system.md`.
//!
//! The exported types serialize in camelCase, unlike every other binding in
//! this repository, because their only consumer outside Rust is the Pontis
//! sidecar and their field names are Pontis's: `inputSchema`, `maxResultBytes`,
//! `importantArguments`. Matching them means the sidecar hands a descriptor to
//! `defineCapability` as it is, with no hand-written mapping to drift.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// `docs/06` § Capability kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum CapabilityKind {
    /// Reads, and changes nothing a person made.
    Resource,
    /// Changes the domain, and so carries presentation metadata.
    Command,
    /// Changes what the person is looking at in the editor. Routed to the
    /// editor, never executed by the server, and always `read`.
    Ui,
}

/// A risk class of pontis `docs/06` § Risk model. The declared class is a
/// baseline: an approval hook may raise it for a particular input, never lower
/// it.
///
/// The one definition. Boundary F reads it too — as the baseline a capability
/// is declared at, and inside the approval evidence a sidecar forwards — so it
/// deserializes as well as serializes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum CapabilityRisk {
    Read,
    Write,
    Sensitive,
    Destructive,
    ExternalSideEffect,
}

/// What a person is shown before consenting to a command (`docs/06`
/// § Presentation metadata). Every field naming an input names a top-level
/// property of the operation's input schema; a test holds that.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CapabilityPresentation {
    /// A short sentence in Forma's terms, for an approval card.
    pub summary: String,
    /// Input fields worth showing, in reading order.
    pub important_arguments: Vec<String>,
    /// An input field whose array length is the affected count.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub affected_resource_count_from: Option<String>,
    /// An input field holding the identifier, or identifiers, acted on.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub resource_ids_from: Option<String>,
}

/// Execution and result bounds (`docs/06` § Large results). Pontis enforces
/// them at both ends and never truncates: a result over `maxResultBytes` is an
/// error, not a shorter result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CapabilityBounds {
    pub max_result_bytes: u32,
    pub timeout_ms: u32,
    /// Applies to every array anywhere in the result, so it is left unset for
    /// results that nest layers and pages — see `operations::bounds`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub max_list_items: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub max_depth: Option<u32>,
}

/// One operation as the Pontis door declares it: the generated half of a
/// capability. The sidecar adds the output schema and the hand-written
/// `authorize` and `execute` bindings, which are application code rather than
/// contract (plan §10(b) D5).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CapabilitySpec {
    pub id: String,
    pub kind: CapabilityKind,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub risk: CapabilityRisk,
    pub bounds: CapabilityBounds,
    /// Required for a command, which Pontis refuses to register without one.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub presentation: Option<CapabilityPresentation>,
}
