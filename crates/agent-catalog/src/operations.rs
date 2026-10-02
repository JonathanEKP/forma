//! Every operation an agent can reach, on either door, defined once.
//!
//! The descriptions of operations that forma-mcp already served were moved
//! here verbatim from its `#[tool]` attributes; rendered for the MCP door they
//! are byte-identical to what hosts were shown before. Risk, presentation and
//! bounds are new, because nothing needed them until Pontis.
//!
//! Deliberately absent, as on forma-mcp: creating a project, editing a brand,
//! saving a template. An agent works inside a workspace the user owns, and those
//! decisions are the user's. Also absent: deleting a design, which plan §10(b)
//! D3 leaves open.

use schemars::generate::SchemaSettings;
use schemars::JsonSchema;
use serde_json::{json, Map, Value};

use crate::descriptor::{CapabilityBounds, CapabilityKind, CapabilityRisk};
use crate::params::*;

/// Which doors expose an operation, said outright rather than inferred from a
/// missing name, so a reader and a test both see it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Doors {
    Both {
        mcp_tool: &'static str,
    },
    /// The two that stay off the Pontis door: `forma_status` answers "where
    /// am I", which a paired session settles before it starts, and
    /// `render_preview`'s image is the media plan §10(c) leaves undecided.
    McpOnly {
        mcp_tool: &'static str,
    },
    /// New surface: editor actions, and reads and commands no MCP tool had.
    PontisOnly,
}

impl Doors {
    pub fn mcp_tool(self) -> Option<&'static str> {
        match self {
            Doors::Both { mcp_tool } | Doors::McpOnly { mcp_tool } => Some(mcp_tool),
            Doors::PontisOnly => None,
        }
    }

    pub fn on_pontis(self) -> bool {
        matches!(self, Doors::Both { .. } | Doors::PontisOnly)
    }
}

/// What a person reads on an approval card. Plain text for people, so unlike a
/// description it never names another operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Presentation {
    pub summary: &'static str,
    pub important_arguments: &'static [&'static str],
    pub affected_resource_count_from: Option<&'static str>,
    pub resource_ids_from: Option<&'static str>,
}

#[derive(Debug, Clone, Copy)]
pub struct Operation {
    /// The capability id, which is also the operation's name on the Pontis
    /// door and how other descriptions refer to it.
    pub id: &'static str,
    pub doors: Doors,
    pub kind: CapabilityKind,
    pub risk: CapabilityRisk,
    /// May name other operations as `{id}`; see `surface::render_text`.
    pub description: &'static str,
    /// The raw schema, before any door renders it.
    pub input: fn() -> Map<String, Value>,
    pub presentation: Option<Presentation>,
    pub bounds: CapabilityBounds,
}

/// The input schema of `T`, generated exactly as rmcp generates a tool's: the
/// 2020-12 settings, with the root's `title` and `description` dropped because
/// they are the Rust type's name and doc, noise to a model. forma-mcp's tests
/// compare this against rmcp's own output, so the two cannot quietly diverge.
pub fn input_of<T: JsonSchema>() -> Map<String, Value> {
    let schema = SchemaSettings::draft2020_12()
        .into_generator()
        .into_root_schema_for::<T>();
    let Value::Object(mut object) = schema.to_value() else {
        panic!("a parameters type must produce an object schema");
    };
    object.remove("title");
    object.remove("description");
    object
}

/// An operation that takes nothing, as rmcp writes it.
pub fn no_input() -> Map<String, Value> {
    let Value::Object(object) = json!({ "type": "object", "properties": {} }) else {
        unreachable!()
    };
    object
}

// --- bounds -------------------------------------------------------------------
//
// These are the only bounds there are. Boundary F's routes in forma-server read
// them from here and enforce them before answering, and the sidecar declares
// them to Pontis, which enforces them again — so a result the route returns is
// never one Pontis then refuses. The route counts `serde_json`'s bytes and
// Pontis counts `JSON.stringify`'s; serde writes `1080.0` where JavaScript
// writes `1080` and nothing shorter, so the route's count is never the smaller.
//
// The three read bounds Boundary F serves were measured, by serializing what
// each route actually answers (`apps/server/tests/pontis.rs`, which re-measures
// the cases below on every run). The yardstick is the largest real design on a
// working Forma: a 1080x1350 cover from a real brief, one page of eight layers
// (photo, two scrims, four lines of type, an accent rule), 3,199 bytes as
// `design.get` answers it. Every version of every design in that workspace was
// under 3.4 KB. A design costs 360-500 bytes a layer and a validation issue
// about 323 bytes; neither has a cap in the domain, so the bound is the cap.
//
// - `design.get`: 256 KiB. A 20-page carousel at the real cover's density —
//   Instagram's ceiling of twenty slides, 160 layers — is 57,649 bytes, 4.5x
//   under. A stress case nobody has made, twenty pages of twenty layers with a
//   300-character paragraph on every other one, is 200,869 bytes and still
//   fits (1.3x). Twice that, 800 layers, is 398 KB and is refused: by then the
//   design alone is a large fraction of a model's context, and there is no
//   narrower read to send it to instead.
// - `design.validate`: 512 KiB, twice the design's, because a report outgrows
//   the design it describes. A text layer wrong in every way at once draws five
//   issues — off-brand colour, too small, outside the safe area, overflowing in
//   both directions — at 323 bytes each, four times the layer itself. That
//   20-page carousel, every line of it broken, reports 800 issues in 258,386
//   bytes: 242 bytes under the 256 KiB this used to share with `design.get`,
//   so a first draft one layer worse would have had its report refused at
//   exactly the moment the agent needed it. 512 KiB holds it at 2.0x, and
//   holds 1.4x even at seven issues a layer. Twenty pages of twenty broken
//   layers, 2,000 issues and 646 KB, is refused; a list that long is not
//   one an agent works through.
// - `project.list`: 64 KiB and 200 items. A project has no array in it, so
//   `maxListItems` — which Pontis applies to every array anywhere in a result —
//   caps exactly the list and nothing else, and is declared. Two hundred
//   projects with a 52-character name each answer 38,408 bytes, 192 a project:
//   1.7x under. The item cap is a ceiling rather than a measurement — a real
//   workspace holds one — and is kept because the list cannot be paged, and
//   because at 38 KB it is not what fills a context.
//
// `maxListItems` stays unset everywhere else, and `maxDepth` everywhere. A
// design nests pages in layers, and no other Forma read is paginated: a cap
// would turn a large project into a hard `result_too_large` with no way to ask
// for less. Pagination is the honest fix and is not this change's to make.
//
// The remaining bounds are authored, not measured: nothing serves them on the
// Pontis path yet. `apply_operations` answers four fields, 156 bytes, under
// `MUTATION`. A Story page at scale 4 takes 8.6s to render, which sets the floor
// for anything that renders; validating the 800-layer stress case took 9s.

const KIB: u32 = 1024;

const fn bounds(max_result_bytes: u32, timeout_ms: u32) -> CapabilityBounds {
    CapabilityBounds {
        max_result_bytes,
        timeout_ms,
        max_list_items: None,
        max_depth: None,
    }
}

/// A read of rows or one record. Unmeasured; see above.
const READ: CapabilityBounds = bounds(256 * KIB, 10_000);
/// The project list, measured; see above.
const PROJECT_LIST: CapabilityBounds = CapabilityBounds {
    max_list_items: Some(200),
    ..bounds(64 * KIB, 10_000)
};
/// One whole design, measured; see above.
const DESIGN: CapabilityBounds = bounds(256 * KIB, 10_000);
/// A mutation answering with the design it wrote.
const MUTATION: CapabilityBounds = bounds(256 * KIB, 15_000);
/// Renders every page to measure its text, and answers with every issue at
/// once; measured, see above.
const VALIDATE: CapabilityBounds = bounds(512 * KIB, 60_000);
/// Renders every page at up to scale 4, at 8.6s a page in the worst case.
const EXPORT: CapabilityBounds = bounds(64 * KIB, 120_000);
/// One page as a PNG, base64-encoded. MCP door only, where Pontis enforces
/// nothing, but the operation has bounds wherever it is served.
const PREVIEW: CapabilityBounds = bounds(16 * 1024 * KIB, 30_000);
/// An editor action, which answers with a fixed acknowledgement.
const UI: CapabilityBounds = bounds(4 * KIB, 5_000);

use CapabilityKind::{Command, Resource, Ui};
use CapabilityRisk::{Read, Write};

pub static REGISTRY: &[Operation] = &[
    // --- reading --------------------------------------------------------------
    Operation {
        id: "forma.status",
        doors: Doors::McpOnly {
            mcp_tool: "forma_status",
        },
        kind: Resource,
        risk: Read,
        description: "Report whether forma-server is reachable and what it currently holds: \
                      version, document schema version, the font families the renderer can \
                      actually draw, and the project count. Call this first in a Forma session.",
        input: no_input,
        presentation: None,
        bounds: READ,
    },
    Operation {
        id: "project.list",
        doors: Doors::Both {
            mcp_tool: "list_projects",
        },
        kind: Resource,
        risk: Read,
        description: "List the projects on this Forma. Each is a workspace with its own Brand \
                      Kit, assets and designs.",
        input: no_input,
        presentation: None,
        bounds: PROJECT_LIST,
    },
    Operation {
        id: "project.get",
        doors: Doors::Both {
            mcp_tool: "get_project",
        },
        kind: Resource,
        risk: Read,
        description: "Read one project: its name and which Brand Kit it uses. Use it to \
                      confirm you are working in the project the user meant before creating \
                      anything in it.",
        input: input_of::<ProjectParams>,
        presentation: None,
        bounds: READ,
    },
    Operation {
        id: "brand.get",
        doors: Doors::Both {
            mcp_tool: "get_brand",
        },
        kind: Resource,
        risk: Read,
        description: "Read a project's Brand Kit: its colours, its type, its voice and its \
                      rules, plus the font families the renderer can draw. Read this before \
                      writing copy or choosing values. Use $brand.* tokens for anything you \
                      choose yourself, and never name a font outside the returned catalog.",
        input: input_of::<ProjectParams>,
        presentation: None,
        bounds: READ,
    },
    Operation {
        id: "asset.list",
        doors: Doors::Both {
            mcp_tool: "list_assets",
        },
        kind: Resource,
        risk: Read,
        description: "List a project's images. Check this before requesting a new asset: \
                      reusing what exists is faster than waiting for a person to upload.",
        input: input_of::<ProjectParams>,
        presentation: None,
        bounds: READ,
    },
    Operation {
        id: "template.list",
        doors: Doors::Both {
            mcp_tool: "list_templates",
        },
        kind: Resource,
        risk: Read,
        description: "List the layouts this project has saved, each with the roles it expects: \
                      the page a slot sits on, what it is for, how many characters the layout \
                      actually held, and the text that was in it. That is a brief. Read it \
                      before designing from scratch: a template whose shape matches what the \
                      piece has to say is a better starting point than an empty canvas, and \
                      its budgets tell you how much copy the layout can take. A project with \
                      none returns an empty list, which is not a problem -- design from \
                      scratch. You cannot save a template; a person does that from a design \
                      they liked.",
        input: input_of::<ProjectParams>,
        presentation: None,
        bounds: READ,
    },
    Operation {
        id: "design.list",
        doors: Doors::PontisOnly,
        kind: Resource,
        risk: Read,
        description: "List a project's designs, most recently changed first, each with its name, \
                      canvas, page count and current version. Read it before creating a design, so you continue the \
                      piece the person meant rather than starting a second one beside it. \
                      Read a design in full with {design.get}.",
        input: input_of::<ProjectParams>,
        presentation: None,
        bounds: READ,
    },
    Operation {
        id: "design.get",
        doors: Doors::Both {
            mcp_tool: "get_design",
        },
        kind: Resource,
        risk: Read,
        description: "Read a design in full: its current version and every page and layer. The \
                      ids it returns are the only valid handles for later operations, and the \
                      version it returns is what you pass as base_version.",
        input: input_of::<DesignParams>,
        presentation: None,
        bounds: DESIGN,
    },
    Operation {
        id: "design.history",
        doors: Doors::PontisOnly,
        kind: Resource,
        risk: Read,
        description: "List every version of a design, newest first, each with who wrote it -- a \
                      person, an agent or the system -- and when. Nothing is ever removed from \
                      it: an undo adds a version rather than taking one away. Use it to see \
                      what changed since you last read the design before deciding whether \
                      to undo.",
        input: input_of::<DesignParams>,
        presentation: None,
        bounds: READ,
    },
    Operation {
        id: "assetRequest.get",
        doors: Doors::Both {
            mcp_tool: "get_asset_request",
        },
        kind: Resource,
        risk: Read,
        description: "Check whether an image you asked for has been uploaded. A request is \
                      pending until a person resolves it; poll this rather than assuming.",
        input: input_of::<AssetRequestParams>,
        presentation: None,
        bounds: READ,
    },
    // --- writing --------------------------------------------------------------
    Operation {
        id: "design.create",
        doors: Doors::Both {
            mcp_tool: "create_design",
        },
        kind: Command,
        risk: Write,
        description: "Create a design in a project, empty or from a saved template. An empty \
                      one starts with a single page; add more with an add_page operation for a \
                      carousel, and all pages share one canvas. Passing template_id starts from \
                      a layout that already worked in this project, carrying its words for you \
                      to replace.",
        input: input_of::<CreateDesignParams>,
        presentation: Some(Presentation {
            summary: "Start a new design in this project",
            important_arguments: &["name", "project_id", "template_id", "preset"],
            affected_resource_count_from: None,
            resource_ids_from: Some("project_id"),
        }),
        bounds: MUTATION,
    },
    Operation {
        id: "template.createDesign",
        doors: Doors::PontisOnly,
        kind: Command,
        risk: Write,
        description: "Create a design from a saved template, by its id from {template.list}. \
                      The new design takes the template's canvas, pages and layout, and \
                      carries its words for you to replace. The template itself is not \
                      changed.",
        input: input_of::<TemplateDesignParams>,
        presentation: Some(Presentation {
            summary: "Start a new design from a saved template",
            important_arguments: &["name", "template_id"],
            affected_resource_count_from: None,
            resource_ids_from: Some("template_id"),
        }),
        bounds: MUTATION,
    },
    Operation {
        id: "design.applyOperations",
        doors: Doors::Both {
            mcp_tool: "apply_operations",
        },
        kind: Command,
        // The baseline. A batch that removes a layer or a page destroys work,
        // and the sidecar's approval hook raises such a batch to the class the
        // removal deserves (plan §3.1). Elevation only ever goes up.
        risk: Write,
        description: "Apply a batch of typed operations to a design. This is how every edit is \
                      made: add_page, remove_page, reorder_pages, set_background, add_layer, \
                      remove_layer, move_layer, reorder_layer, set_text, patch_text_style, \
                      set_image, patch_shape_style, set_format. The batch is atomic — if one \
                      operation is rejected, none are saved — so group a coherent change into \
                      one call rather than sending several. On VERSION_CONFLICT, call \
                      {design.get}, rebase onto the version it returns, and send again; never \
                      resend the same base_version.",
        input: input_of::<ApplyOperationsParams>,
        presentation: Some(Presentation {
            summary: "Edit this design: apply the listed changes together as one new version",
            important_arguments: &["design_id", "base_version", "operations"],
            // Each operation acts on one page or one layer, so the batch's
            // length is the number of changes the person is agreeing to.
            affected_resource_count_from: Some("operations"),
            resource_ids_from: Some("design_id"),
        }),
        bounds: MUTATION,
    },
    Operation {
        id: "design.undo",
        doors: Doors::PontisOnly,
        kind: Command,
        // Nothing is lost: the version undone stays in the history, and
        // undoing again restores it. That is a write, not a destructive act.
        risk: Write,
        description: "Undo the most recent change to a design by restoring the version before \
                      base_version. This writes a new version rather than removing one: \
                      undoing version 10 saves version 11 with version 9's content, and \
                      version 10 stays in {design.history}. Undoing twice in a row therefore \
                      restores the change you undid -- to go further back, read the history \
                      and rebuild with {design.applyOperations}. Pass the version you \
                      actually read; on VERSION_CONFLICT, someone else changed the design \
                      after you read it, so read it again before deciding to undo.",
        input: input_of::<UndoParams>,
        presentation: Some(Presentation {
            summary: "Undo the latest change to this design, saved as a new version",
            important_arguments: &["design_id", "base_version"],
            affected_resource_count_from: None,
            resource_ids_from: Some("design_id"),
        }),
        bounds: MUTATION,
    },
    Operation {
        id: "assetRequest.create",
        doors: Doors::Both {
            mcp_tool: "request_asset",
        },
        kind: Command,
        risk: Write,
        description: "Record that the design needs an image that does not exist yet, so a \
                      person can upload it. Forma generates no images. Do this instead of \
                      inventing an asset id or stalling, then carry on with the rest of the \
                      design and check back with {assetRequest.get}.",
        input: input_of::<RequestAssetParams>,
        presentation: Some(Presentation {
            summary: "Ask for an image this design needs, for you to upload",
            important_arguments: &["purpose", "description", "design_id", "suggested_prompt"],
            affected_resource_count_from: None,
            resource_ids_from: Some("project_id"),
        }),
        bounds: READ,
    },
    // --- quality and output ---------------------------------------------------
    Operation {
        id: "design.validate",
        doors: Doors::Both {
            mcp_tool: "validate_design",
        },
        // No side effect, so a resource despite being an "output" tool in
        // forma-mcp's vocabulary (plan §3).
        kind: Resource,
        risk: Read,
        description: "Check a design against deterministic rules: bounds, safe area, real text \
                      overflow measured from rendered glyphs, missing assets and brand \
                      conformance. Errors block export; warnings do not. Fix the errors before \
                      exporting.",
        input: input_of::<DesignParams>,
        presentation: None,
        bounds: VALIDATE,
    },
    Operation {
        id: "design.renderPreview",
        doors: Doors::McpOnly {
            mcp_tool: "render_preview",
        },
        kind: Resource,
        risk: Read,
        description: "Render one page and return it as a PNG image alongside the structured \
                      result. If your host shows you the image, look at it and say what it \
                      actually shows. If it does not, say the mechanical checks passed and \
                      leave the visual judgement to the user — never claim to have seen a \
                      preview you were not given.",
        input: input_of::<RenderPreviewParams>,
        presentation: None,
        bounds: PREVIEW,
    },
    Operation {
        id: "design.export",
        doors: Doors::Both {
            mcp_tool: "export_design",
        },
        kind: Command,
        // Writes PNG files under the data directory. Nothing leaves the
        // deployment, so it is not an external side effect (plan §3).
        risk: Write,
        description: "Export a design to PNG files: the whole design for a carousel, or one \
                      page. Validate first — exporting a design with errors ships the errors.",
        input: input_of::<ExportDesignParams>,
        presentation: Some(Presentation {
            summary: "Export this design as PNG files you can download",
            important_arguments: &["design_id", "scope", "page_id", "scale"],
            affected_resource_count_from: None,
            resource_ids_from: Some("design_id"),
        }),
        bounds: EXPORT,
    },
    // --- the editor -------------------------------------------------------------
    //
    // `docs/06` § UI: routed to the person's open editor and never executed by
    // the server, so always `read`. Each still names what it acts on, because
    // authorization still runs: opening a design the person may not read would
    // disclose that it exists.
    Operation {
        id: "ui.openDesign",
        doors: Doors::PontisOnly,
        kind: Ui,
        risk: Read,
        description: "Show a design in the person's editor, at its first page or the page you \
                      name. Use it to point at the piece you are talking about; it changes \
                      what they are looking at and nothing in the design.",
        input: input_of::<OpenDesignParams>,
        presentation: None,
        bounds: UI,
    },
    Operation {
        id: "ui.selectLayer",
        doors: Doors::PontisOnly,
        kind: Ui,
        risk: Read,
        description: "Select a layer in the design the person has open, so the properties \
                      panel shows it. Use it to point at the layer you mean; it changes \
                      nothing in the design.",
        input: input_of::<SelectLayerParams>,
        presentation: None,
        bounds: UI,
    },
    Operation {
        id: "ui.setPanel",
        doors: Doors::PontisOnly,
        kind: Ui,
        risk: Read,
        description: "Switch the side panel beside the person's canvas: properties, \
                      validation, assets or brand. Use validation to show them the issues \
                      {design.validate} reported.",
        input: input_of::<SetPanelParams>,
        presentation: None,
        bounds: UI,
    },
    Operation {
        id: "ui.setZoom",
        doors: Doors::PontisOnly,
        kind: Ui,
        risk: Read,
        description: "Set the zoom of the person's canvas, from 0.05 to 4, where 1 is the \
                      canvas at native size. It changes their view, not the design.",
        input: input_of::<SetZoomParams>,
        presentation: None,
        bounds: UI,
    },
];
