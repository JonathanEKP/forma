//! The MCP surface: the thirteen tools of plan section 48.
//!
//! Deliberately few and composable. There is no `add_text`, `move_layer` or
//! `set_color` tool because `apply_operations` covers them in one atomic batch,
//! and a tool per property would make an agent spend its turns on round trips
//! instead of on the design.
//!
//! Every tool is a thin translation of one REST call. Design logic lives in
//! `forma-server`, so the agent and the editor cannot drift apart.

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, ContentBlock, Implementation, ProtocolVersion, ServerCapabilities, ServerInfo,
};
use rmcp::{tool, tool_handler, tool_router, ErrorData, Peer, RoleServer, ServerHandler};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::client::{ServerClient, ServerError};

// --- parameters -----------------------------------------------------------
//
// Doc comments become the schema descriptions the host shows the model, so they
// are written for an agent deciding whether to call the tool, not for us.

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ProjectParams {
    /// The project's id, as returned by list_projects.
    pub project_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DesignParams {
    /// The design's id, as returned by create_design or list_projects.
    pub design_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AssetRequestParams {
    /// The asset request's id, as returned by request_asset.
    pub asset_request_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateDesignParams {
    pub project_id: String,
    /// A short human name, shown in the editor's design list.
    pub name: String,
    /// Start from a saved template instead of an empty canvas, by its id from
    /// list_templates. The new design carries the template's layout and its
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

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ApplyOperationsParams {
    pub design_id: String,
    /// The version you actually read from get_design or a previous mutation.
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

#[derive(Debug, Deserialize, schemars::JsonSchema)]
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

#[derive(Debug, Deserialize, schemars::JsonSchema)]
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

#[derive(Debug, Deserialize, schemars::JsonSchema)]
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

// --- server ---------------------------------------------------------------

pub struct FormaMcp {
    client: ServerClient,
    tool_router: ToolRouter<Self>,
}

impl FormaMcp {
    pub fn new(client: ServerClient) -> Self {
        Self {
            client,
            tool_router: Self::tool_router(),
        }
    }
}

/// Who to record as the author of a mutation.
///
/// Taken from the host's own name at initialize, so the history of plan
/// section 61 reads `agent:claude-code` rather than a generic label — which is
/// the point of recording an actor at all.
fn actor_of(peer: &Peer<RoleServer>) -> String {
    let name = peer
        .peer_info()
        .map(|info| info.client_info.name.clone())
        .unwrap_or_default();

    let slug: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let slug = slug.trim_matches('-').to_string();

    if slug.is_empty() {
        "agent:mcp".to_string()
    } else {
        format!("agent:{slug}")
    }
}

/// A failed call, carrying the code and hint the recovery table needs.
fn failed(error: ServerError) -> CallToolResult {
    let mut result = CallToolResult::error(vec![ContentBlock::text(error.to_string())]);
    result.structured_content = Some(error.as_json());
    result
}

/// A successful call: a one-line summary for a host that shows text, and the
/// payload as structured content for one that can use it.
fn ok(summary: impl Into<String>, value: Value) -> CallToolResult {
    let mut result = CallToolResult::success(vec![ContentBlock::text(summary.into())]);
    result.structured_content = Some(value);
    result
}

fn answer(summary: impl Into<String>, result: Result<Value, ServerError>) -> CallToolResult {
    match result {
        Ok(value) => ok(summary, value),
        Err(error) => failed(error),
    }
}

/// A compact description of a design, so a chat transcript is not flooded by a
/// document the agent can fetch in full when it needs to.
fn outline(record: &Value) -> String {
    let pages = record["document"]["pages"]
        .as_array()
        .map(Vec::len)
        .unwrap_or(0);
    let layers: usize = record["document"]["pages"]
        .as_array()
        .map(|pages| {
            pages
                .iter()
                .map(|p| p["layers"].as_array().map(Vec::len).unwrap_or(0))
                .sum()
        })
        .unwrap_or(0);

    format!(
        "{} — v{}, {pages} page{}, {layers} layer{}",
        record["name"].as_str().unwrap_or("design"),
        record["version"].as_u64().unwrap_or(0),
        if pages == 1 { "" } else { "s" },
        if layers == 1 { "" } else { "s" },
    )
}

#[tool_router]
impl FormaMcp {
    // --- reading ----------------------------------------------------------

    #[tool(
        name = "forma_status",
        description = "Report whether forma-server is reachable and what it currently holds: \
                       version, document schema version, the font families the renderer can \
                       actually draw, and the project count. Call this first in a Forma session.",
        annotations(read_only_hint = true)
    )]
    async fn forma_status(&self) -> Result<CallToolResult, ErrorData> {
        Ok(answer(
            format!("forma-server reachable at {}", self.client.base_url()),
            self.client.get("forma_status", "/status").await,
        ))
    }

    #[tool(
        name = "list_projects",
        description = "List the projects on this Forma. Each is a workspace with its own Brand \
                       Kit, assets and designs.",
        annotations(read_only_hint = true)
    )]
    async fn list_projects(&self) -> Result<CallToolResult, ErrorData> {
        match self.client.get::<Value>("list_projects", "/projects").await {
            Ok(projects) => {
                let count = projects.as_array().map(Vec::len).unwrap_or(0);
                Ok(ok(
                    format!("{count} project(s)"),
                    json!({ "projects": projects }),
                ))
            }
            Err(error) => Ok(failed(error)),
        }
    }

    #[tool(
        name = "get_project",
        description = "Read one project: its name and which Brand Kit it uses. Use it to \
                       confirm you are working in the project the user meant before creating \
                       anything in it.",
        annotations(read_only_hint = true)
    )]
    async fn get_project(
        &self,
        Parameters(params): Parameters<ProjectParams>,
    ) -> Result<CallToolResult, ErrorData> {
        Ok(answer(
            "project",
            self.client
                .get("get_project", &format!("/projects/{}", params.project_id))
                .await,
        ))
    }

    #[tool(
        name = "get_brand",
        description = "Read a project's Brand Kit: its colours, its type, its voice and its \
                       rules, plus the font families the renderer can draw. Read this before \
                       writing copy or choosing values. Use $brand.* tokens for anything you \
                       choose yourself, and never name a font outside the returned catalog.",
        annotations(read_only_hint = true)
    )]
    async fn get_brand(
        &self,
        Parameters(params): Parameters<ProjectParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let brand = self
            .client
            .get::<Value>(
                "get_brand",
                &format!("/projects/{}/brand", params.project_id),
            )
            .await;
        let status = self.client.get::<Value>("get_brand", "/status").await;

        match (brand, status) {
            (Ok(brand), Ok(status)) => {
                // The font catalog travels with the kit because a family the
                // renderer lacks is a FONT_NOT_FOUND the agent could have
                // avoided (plan section 13).
                let summary = if brand.is_null() {
                    "This project has no Brand Kit yet: use colour literals, not $brand.* tokens."
                } else {
                    "brand kit"
                };
                Ok(ok(
                    summary,
                    json!({ "brand": brand, "fonts": status["fonts"] }),
                ))
            }
            (Err(error), _) | (_, Err(error)) => Ok(failed(error)),
        }
    }

    #[tool(
        name = "list_assets",
        description = "List a project's images. Check this before requesting a new asset: \
                       reusing what exists is faster than waiting for a person to upload.",
        annotations(read_only_hint = true)
    )]
    async fn list_assets(
        &self,
        Parameters(params): Parameters<ProjectParams>,
    ) -> Result<CallToolResult, ErrorData> {
        match self
            .client
            .get::<Value>(
                "list_assets",
                &format!("/projects/{}/assets", params.project_id),
            )
            .await
        {
            Ok(assets) => {
                let count = assets.as_array().map(Vec::len).unwrap_or(0);
                Ok(ok(format!("{count} asset(s)"), json!({ "assets": assets })))
            }
            Err(error) => Ok(failed(error)),
        }
    }

    #[tool(
        name = "list_templates",
        description = "List the layouts this project has saved, each with the roles it expects: \
                       the page a slot sits on, what it is for, how many characters the layout \
                       actually held, and the text that was in it. That is a brief. Read it \
                       before designing from scratch: a template whose shape matches what the \
                       piece has to say is a better starting point than an empty canvas, and \
                       its budgets tell you how much copy the layout can take. A project with \
                       none returns an empty list, which is not a problem -- design from \
                       scratch. You cannot save a template; a person does that from a design \
                       they liked.",
        annotations(read_only_hint = true)
    )]
    async fn list_templates(
        &self,
        Parameters(params): Parameters<ProjectParams>,
    ) -> Result<CallToolResult, ErrorData> {
        match self
            .client
            .get::<Value>(
                "list_templates",
                &format!("/projects/{}/templates", params.project_id),
            )
            .await
        {
            Ok(templates) => {
                let count = templates.as_array().map(Vec::len).unwrap_or(0);
                Ok(ok(
                    format!("{count} template(s)"),
                    json!({ "templates": templates }),
                ))
            }
            Err(error) => Ok(failed(error)),
        }
    }

    #[tool(
        name = "get_design",
        description = "Read a design in full: its current version and every page and layer. The \
                       ids it returns are the only valid handles for later operations, and the \
                       version it returns is what you pass as base_version.",
        annotations(read_only_hint = true)
    )]
    async fn get_design(
        &self,
        Parameters(params): Parameters<DesignParams>,
    ) -> Result<CallToolResult, ErrorData> {
        match self
            .client
            .get::<Value>("get_design", &format!("/designs/{}", params.design_id))
            .await
        {
            Ok(record) => Ok(ok(outline(&record), record)),
            Err(error) => Ok(failed(error)),
        }
    }

    #[tool(
        name = "get_asset_request",
        description = "Check whether an image you asked for has been uploaded. A request is \
                       pending until a person resolves it; poll this rather than assuming.",
        annotations(read_only_hint = true)
    )]
    async fn get_asset_request(
        &self,
        Parameters(params): Parameters<AssetRequestParams>,
    ) -> Result<CallToolResult, ErrorData> {
        match self
            .client
            .get::<Value>(
                "get_asset_request",
                &format!("/asset-requests/{}", params.asset_request_id),
            )
            .await
        {
            Ok(request) => {
                let status = request["status"].as_str().unwrap_or("unknown").to_string();
                Ok(ok(format!("request is {status}"), request))
            }
            Err(error) => Ok(failed(error)),
        }
    }

    // --- writing ----------------------------------------------------------

    #[tool(
        name = "create_design",
        description = "Create a design in a project, empty or from a saved template. An empty \
                       one starts with a single page; add more with an add_page operation for a \
                       carousel, and all pages share one canvas. Passing template_id starts from \
                       a layout that already worked in this project, carrying its words for you \
                       to replace.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn create_design(
        &self,
        peer: Peer<RoleServer>,
        Parameters(params): Parameters<CreateDesignParams>,
    ) -> Result<CallToolResult, ErrorData> {
        if let Some(template_id) = &params.template_id {
            return match self
                .client
                .post::<Value, _>(
                    "create_design",
                    &format!("/templates/{template_id}/designs"),
                    &actor_of(&peer),
                    &json!({ "name": params.name }),
                )
                .await
            {
                Ok(record) => Ok(ok(outline(&record), record)),
                Err(error) => Ok(failed(error)),
            };
        }

        let preset = params.preset.unwrap_or_else(|| "square".to_string());
        let mut body = json!({ "name": params.name, "format": { "preset": preset } });

        // Only a custom canvas needs explicit dimensions; for a preset the
        // server owns them, so sending guesses would fight it.
        if let (Some(width), Some(height)) = (params.width, params.height) {
            body["format"]["width"] = json!(width);
            body["format"]["height"] = json!(height);
        }

        match self
            .client
            .post::<Value, _>(
                "create_design",
                &format!("/projects/{}/designs", params.project_id),
                &actor_of(&peer),
                &body,
            )
            .await
        {
            Ok(record) => Ok(ok(outline(&record), record)),
            Err(error) => Ok(failed(error)),
        }
    }

    #[tool(
        name = "apply_operations",
        description = "Apply a batch of typed operations to a design. This is how every edit is \
                       made: add_page, remove_page, reorder_pages, set_background, add_layer, \
                       remove_layer, move_layer, reorder_layer, set_text, patch_text_style, \
                       set_image, patch_shape_style, set_format. The batch is atomic — if one \
                       operation is rejected, none are saved — so group a coherent change into \
                       one call rather than sending several. On VERSION_CONFLICT, call \
                       get_design, rebase onto the version it returns, and send again; never \
                       resend the same base_version.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn apply_operations(
        &self,
        peer: Peer<RoleServer>,
        Parameters(params): Parameters<ApplyOperationsParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let body = json!({
            "base_version": params.base_version,
            "operations": params.operations,
        });

        match self
            .client
            .post::<Value, _>(
                "apply_operations",
                &format!("/designs/{}/operations", params.design_id),
                &actor_of(&peer),
                &body,
            )
            .await
        {
            Ok(record) => Ok(ok(outline(&record), record)),
            Err(error) => Ok(failed(error)),
        }
    }

    #[tool(
        name = "request_asset",
        description = "Record that the design needs an image that does not exist yet, so a \
                       person can upload it. Forma generates no images. Do this instead of \
                       inventing an asset id or stalling, then carry on with the rest of the \
                       design and check back with get_asset_request.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn request_asset(
        &self,
        peer: Peer<RoleServer>,
        Parameters(params): Parameters<RequestAssetParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let body = json!({
            "design_id": params.design_id,
            "purpose": params.purpose,
            "description": params.description,
            "suggested_prompt": params.suggested_prompt,
            "requested_width": params.requested_width,
            "requested_height": params.requested_height,
        });

        match self
            .client
            .post::<Value, _>(
                "request_asset",
                &format!("/projects/{}/asset-requests", params.project_id),
                &actor_of(&peer),
                &body,
            )
            .await
        {
            Ok(request) => Ok(ok(
                "Asked for an image. Tell the user what you need; the editor shows the request.",
                request,
            )),
            Err(error) => Ok(failed(error)),
        }
    }

    // --- quality and output ------------------------------------------------

    #[tool(
        name = "validate_design",
        description = "Check a design against deterministic rules: bounds, safe area, real text \
                       overflow measured from rendered glyphs, missing assets and brand \
                       conformance. Errors block export; warnings do not. Fix the errors before \
                       exporting.",
        annotations(read_only_hint = true)
    )]
    async fn validate_design(
        &self,
        Parameters(params): Parameters<DesignParams>,
    ) -> Result<CallToolResult, ErrorData> {
        match self
            .client
            .post::<Value, _>(
                "validate_design",
                &format!("/designs/{}/validate", params.design_id),
                "agent:mcp",
                &json!({}),
            )
            .await
        {
            Ok(report) => {
                let errors = report["issues"]
                    .as_array()
                    .map(|issues| issues.iter().filter(|i| i["severity"] == "error").count())
                    .unwrap_or(0);
                let summary = if report["valid"] == json!(true) {
                    "Valid: this design can be exported.".to_string()
                } else {
                    format!("{errors} error(s) block export.")
                };
                Ok(ok(summary, report))
            }
            Err(error) => Ok(failed(error)),
        }
    }

    #[tool(
        name = "render_preview",
        description = "Render one page and return it as a PNG image alongside the structured \
                       result. If your host shows you the image, look at it and say what it \
                       actually shows. If it does not, say the mechanical checks passed and \
                       leave the visual judgement to the user — never claim to have seen a \
                       preview you were not given.",
        annotations(read_only_hint = true)
    )]
    async fn render_preview(
        &self,
        Parameters(params): Parameters<RenderPreviewParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let body = json!({
            "page_id": params.page_id,
            "scale": params.scale.unwrap_or(1.0),
        });

        match self
            .client
            .post::<Value, _>(
                "render_preview",
                &format!("/designs/{}/render", params.design_id),
                "agent:mcp",
                &body,
            )
            .await
        {
            Ok(render) => {
                let Some(encoded) = render["png_base64"].as_str() else {
                    return Ok(failed(ServerError {
                        code: "RENDER_FAILED".to_string(),
                        message: "the server returned no image".to_string(),
                        hint: Some("This is a server problem, not a design problem.".to_string()),
                        details: None,
                    }));
                };

                // Plan section 49: the structured result AND the image, so a
                // multimodal host can actually inspect the piece instead of
                // trusting a path it cannot open.
                let mut result = CallToolResult::success(vec![
                    ContentBlock::text(format!(
                        "Rendered version {} at scale {} ({} bytes).",
                        render["version"], render["scale"], render["byte_size"]
                    )),
                    ContentBlock::image(encoded.to_string(), "image/png"),
                ]);

                let mut structured = render.clone();
                // The base64 is already in the image block; repeating it in the
                // structured result would double the tokens for nothing.
                if let Some(object) = structured.as_object_mut() {
                    object.remove("png_base64");
                }
                result.structured_content = Some(structured);
                Ok(result)
            }
            Err(error) => Ok(failed(error)),
        }
    }

    #[tool(
        name = "export_design",
        description = "Export a design to PNG files: the whole design for a carousel, or one \
                       page. Validate first — exporting a design with errors ships the errors.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn export_design(
        &self,
        peer: Peer<RoleServer>,
        Parameters(params): Parameters<ExportDesignParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let body = json!({
            "scope": params.scope.unwrap_or_else(|| "all_pages".to_string()),
            "page_id": params.page_id,
            "scale": params.scale.unwrap_or(1.0),
        });

        match self
            .client
            .post::<Value, _>(
                "export_design",
                &format!("/designs/{}/exports", params.design_id),
                &actor_of(&peer),
                &body,
            )
            .await
        {
            Ok(export) => {
                // The response nests the record and the files it wrote.
                let files = export["files"].as_array().map(Vec::len).unwrap_or(0);
                let summary = format!(
                    "Exported version {} as {files} file{}.",
                    export["export"]["design_version"],
                    if files == 1 { "" } else { "s" }
                );
                Ok(ok(summary, export))
            }
            Err(error) => Ok(failed(error)),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for FormaMcp {
    fn get_info(&self) -> ServerInfo {
        // These types are #[non_exhaustive], so they are built by mutation.
        let mut info = ServerInfo::default();
        info.protocol_version = ProtocolVersion::LATEST;
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info.server_info = Implementation::new("forma-mcp", env!("CARGO_PKG_VERSION"));
        info.instructions = Some(
            "Forma is a local design runtime. You decide the design; Forma executes it \
             deterministically and renders it. It integrates no image generation and no model \
             API of its own.\n\n\
             Start with forma_status, then get_brand before writing any copy or choosing any \
             value. Use $brand.* tokens for values you choose, and only fonts the catalog \
             lists. Ids returned by the server are the only valid handles.\n\n\
             Edit through apply_operations, grouping a coherent change into one atomic batch \
             and passing the version you actually read as base_version. On VERSION_CONFLICT, \
             refetch with get_design and rebase; never resend the same base_version.\n\n\
             If an image you need does not exist, call request_asset rather than inventing an \
             asset id, and never put a URL in a layer. Validate before exporting, and fix the \
             errors it reports. If render_preview's image does not reach you, say so instead \
             of claiming the design looks right."
                .to_string(),
        );
        info
    }
}
