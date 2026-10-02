//! The input of every operation, as Rust types.
//!
//! These are the only definition of what an operation accepts. forma-mcp
//! deserializes its tool arguments into them, so the schema a host is shown and
//! the type the handler reads cannot be two things.
//!
//! Doc comments become the schema descriptions an agent reads, so they are
//! written for an agent deciding what to send, not for us. A name in braces,
//! such as `{project.list}`, is another operation, written as whatever that
//! operation is called on the door being rendered — see `surface::render_text`.

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use ts_rs::TS;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProjectParams {
    /// The project's id, as returned by {project.list}.
    pub project_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DesignParams {
    /// The design's id, as returned by {design.create} or {project.list}.
    pub design_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AssetRequestParams {
    /// The asset request's id, as returned by {asset_request.create}.
    pub asset_request_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateDesignParams {
    pub project_id: String,
    /// A short human name, shown in the editor's design list.
    pub name: String,
    /// Start from a saved template instead of an empty canvas, by its id from
    /// {template.list}. The new design carries the template's layout and its
    /// words, for you to replace; the canvas comes from the template, so preset
    /// and dimensions are ignored with one.
    #[serde(default)]
    pub template_id: Option<String>,
    /// The canvas preset. One of instagram_portrait, square, story, landscape
    /// or custom. Defaults to square.
    #[serde(default)]
    pub preset: Option<String>,
    /// Only needed with the custom preset.
    #[serde(default)]
    pub width: Option<f32>,
    #[serde(default)]
    pub height: Option<f32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ApplyOperationsParams {
    pub design_id: String,
    /// The version you actually read from {design.get} or a previous mutation.
    /// Sending a stale one returns VERSION_CONFLICT rather than overwriting
    /// someone else's edit.
    pub base_version: u64,
    /// The batch, applied in order and atomically: if one operation fails,
    /// none of them are saved. At most 20 per call.
    ///
    /// Each entry is an object with an `op` field; section 8 of the skill lists
    /// the thirteen and their fields. The schema says "object" rather than
    /// spelling them out: `serde_json::Value` alone emits `items: true`, which
    /// is legal JSON Schema for "anything" and which some hosts refuse to
    /// convert — one unconvertible schema breaks the whole tool list, not just
    /// this tool.
    #[schemars(schema_with = "operations_schema")]
    pub operations: Vec<Value>,
}

/// An array of plain objects, described without a single boolean subschema.
///
/// JSON Schema lets `true` stand for "any value", and that is what
/// `Vec<serde_json::Value>` generates for its items. It is legal and some hosts
/// refuse to convert it — and because a host converts the whole tool list at
/// once, one unconvertible schema takes down every tool, not just this one.
fn operations_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "array",
        "items": { "type": "object" }
    })
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RequestAssetParams {
    pub project_id: String,
    /// The design the image is for, when there is one.
    #[serde(default)]
    pub design_id: Option<String>,
    /// What the image is for, in the design's own terms, such as "hero photo".
    pub purpose: String,
    /// What the image should show, specific enough for someone else to find or
    /// make it.
    pub description: String,
    /// A prompt the user can paste into whatever tool they prefer. Forma runs
    /// nothing: it is text for a person.
    #[serde(default)]
    pub suggested_prompt: Option<String>,
    #[serde(default)]
    pub requested_width: Option<u32>,
    #[serde(default)]
    pub requested_height: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RenderPreviewParams {
    pub design_id: String,
    /// Omitted renders the first page.
    #[serde(default)]
    pub page_id: Option<String>,
    /// 1.0 is the canvas at native size. Above 2.0 is rarely worth the bytes
    /// for a preview.
    #[serde(default)]
    pub scale: Option<f32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExportDesignParams {
    pub design_id: String,
    /// "all_pages" for a carousel, "single_page" for one. Defaults to all.
    #[serde(default)]
    pub scope: Option<String>,
    /// Required for a single-page export.
    #[serde(default)]
    pub page_id: Option<String>,
    #[serde(default)]
    pub scale: Option<f32>,
}

// --- reachable only through the Pontis door ----------------------------------
//
// No MCP tool takes these today. They are new agent-facing surface rather than
// relocated text, so nothing proves them in use yet.

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UndoParams {
    /// The design's id, as returned by {design.create} or {design.list}.
    pub design_id: String,
    /// The version you actually read from {design.get} or a previous mutation.
    /// Undo restores the document as it was one version before this one, and
    /// saves that as a new version after it: undoing version 10 writes version
    /// 11, whose content is version 9's. A stale version returns
    /// VERSION_CONFLICT; version 1 has nothing before it and returns
    /// NOTHING_TO_UNDO.
    pub base_version: u64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TemplateDesignParams {
    /// The template's id, as returned by {template.list}.
    pub template_id: String,
    /// A short human name, shown in the editor's design list.
    pub name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct OpenDesignParams {
    /// The design to show the person, as returned by {design.list}.
    pub design_id: String,
    /// The page to show. Omitted shows the first.
    #[serde(default)]
    pub page_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SelectLayerParams {
    /// The design the layer is in, which must be the one the person has open.
    pub design_id: String,
    /// The layer to select, as returned by {design.get}.
    pub layer_id: String,
}

/// The editor's side panels.
///
/// Exported so the editor can stop declaring its own `Panel` union by hand;
/// until it does, the drift between the two is guarded by nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[schemars(inline)]
#[ts(export)]
pub enum EditorPanel {
    Properties,
    Validation,
    Assets,
    Brand,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SetPanelParams {
    /// Which panel to show beside the canvas.
    pub panel: EditorPanel,
}

/// The editor's zoom range. `MIN_ZOOM` and `MAX_ZOOM` in
/// `apps/web/src/store.ts` must agree; the schema refuses outside it rather
/// than leaving the editor to clamp a value the agent believes it set.
pub const MIN_ZOOM: f64 = 0.05;
pub const MAX_ZOOM: f64 = 4.0;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SetZoomParams {
    /// 1.0 shows the canvas at its native size.
    #[schemars(range(min = MIN_ZOOM, max = MAX_ZOOM))]
    pub zoom: f64,
}
