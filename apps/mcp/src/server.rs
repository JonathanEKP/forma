//! The MCP surface: the thirteen tools of plan section 48, plus `list_templates`.
//!
//! Deliberately few and composable. There is no `add_text`, `move_layer` or
//! `set_color` tool because `apply_operations` covers them in one atomic batch,
//! and a tool per property would make an agent spend its turns on round trips
//! instead of on the design.
//!
//! Every tool is a thin translation of one REST call. Design logic lives in
//! `forma-server`, so the agent and the editor cannot drift apart.
//!
//! What each tool is called, says, accepts and promises is not written here.
//! It comes from `agent_catalog`, the one definition both of Forma's agent doors
//! render (plan §10(b) D5): this file names each handler and nothing more, and
//! `from_registry` gives the routes everything else. A description or schema
//! written here instead would be a second definition, and the tests at the
//! bottom refuse one.

use std::borrow::Cow;
use std::sync::Arc;

use agent_catalog::params::{
    ApplyOperationsParams, AssetRequestParams, CreateDesignParams, DesignParams,
    ExportDesignParams, ProjectParams, RenderPreviewParams, RequestAssetParams,
};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, ContentBlock, Extensions, Implementation, ProtocolVersion, ServerCapabilities,
    ServerInfo, ToolAnnotations,
};
use rmcp::{tool, tool_handler, tool_router, ErrorData, Peer, RoleServer, ServerHandler};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};

use crate::client::{BearerCredential, ServerClient, ServerError};

// --- server ---------------------------------------------------------------

pub struct FormaMcp {
    client: ServerClient,
    credential_source: CredentialSource,
    /// The public name this deployment answers on, when it has one.
    ///
    /// `forma_status` is the "where am I" tool, and without this it could only
    /// report the URL it dials — which behind a proxy is loopback on the server's
    /// own machine. A local Forma and a remote one then answer identically, and
    /// the one question the tool exists to settle goes unanswered.
    deployment: Option<String>,
    tool_router: ToolRouter<Self>,
}

/// Where this transport is allowed to obtain an outbound identity.
///
/// Deliberately has no `Debug`: the stdio variant can contain a credential.
enum CredentialSource {
    CurrentRequest,
    StdioProcess(Option<String>),
}

impl FormaMcp {
    fn new(client: ServerClient, credential_source: CredentialSource) -> Self {
        Self {
            client,
            credential_source,
            deployment: None,
            tool_router: from_registry(Self::tool_router()),
        }
    }

    pub fn for_stdio(client: ServerClient, credential: Option<String>) -> Self {
        Self::new(client, CredentialSource::StdioProcess(credential))
    }

    pub fn for_http(client: ServerClient) -> Self {
        Self::new(client, CredentialSource::CurrentRequest)
    }

    pub fn on_deployment(mut self, deployment: Option<String>) -> Self {
        self.deployment = deployment;
        self
    }

    /// The bearer belongs to this tool call, not to the process or its session.
    ///
    /// The transport mode decides which source exists. An HTTP handler contains
    /// no process credential to fall back to when the request extension is absent.
    fn credential(&self, extensions: &Extensions) -> Option<String> {
        match &self.credential_source {
            CredentialSource::CurrentRequest => extensions
                .get::<axum::http::request::Parts>()
                .and_then(|parts| parts.extensions.get::<BearerCredential>())
                .map(|credential| credential.as_str().to_string()),
            CredentialSource::StdioProcess(credential) => credential.clone(),
        }
    }

    #[cfg(test)]
    pub(crate) fn has_process_credential_source(&self) -> bool {
        matches!(&self.credential_source, CredentialSource::StdioProcess(_))
    }

    async fn get<T: DeserializeOwned>(
        &self,
        extensions: &Extensions,
        tool: &str,
        path: &str,
    ) -> crate::client::Result<T> {
        let credential = self.credential(extensions);
        self.client.get(credential.as_deref(), tool, path).await
    }

    async fn post<T: DeserializeOwned, B: Serialize>(
        &self,
        extensions: &Extensions,
        tool: &str,
        path: &str,
        actor: &str,
        body: &B,
    ) -> crate::client::Result<T> {
        let credential = self.credential(extensions);
        self.client
            .post(credential.as_deref(), tool, path, actor, body)
            .await
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

/// A successful call: a one-line summary, the payload as text, and the payload
/// again as structured content.
///
/// The duplication is deliberate and the spec asks for it — "a tool that returns
/// structured content SHOULD also return the serialized JSON in a TextContent
/// block". Without it a host that does not read `structuredContent` gets the
/// summary and nothing else: `list_projects` answered "1 project(s)" with no
/// name and no id, which is useless, and every id-taking tool after it is
/// unusable because ids are the only handles there are.
///
/// It costs tokens on hosts that read both. That is the smaller problem: one
/// wastes context, the other cannot work at all.
fn ok(summary: impl Into<String>, value: Value) -> CallToolResult {
    let mut result = CallToolResult::success(vec![
        ContentBlock::text(summary.into()),
        ContentBlock::text(value.to_string()),
    ]);
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

/// Gives each route what the registry says about its tool: description, input
/// schema and annotations. The `#[tool]` attributes below name handlers and
/// carry nothing else, so this is the only place a host's tool list comes from.
///
/// # Panics
///
/// When the registry lists a tool no handler serves. `the_routes_are_the_registrys`
/// fails on that first, so a build that panics here never ships.
fn from_registry(mut router: ToolRouter<FormaMcp>) -> ToolRouter<FormaMcp> {
    for tool in agent_catalog::mcp_tools() {
        let Some(route) = router.map.get_mut(tool.name) else {
            panic!(
                "the agent catalog lists {}, which no handler serves",
                tool.name
            );
        };
        route.attr.description = Some(Cow::Owned(tool.description));
        route.attr.input_schema = Arc::new(tool.input_schema);
        route.attr.annotations = Some(ToolAnnotations::from_raw(
            None,
            Some(tool.hints.read_only),
            tool.hints.destructive,
            None,
            tool.hints.open_world,
        ));
    }
    router
}

#[tool_router]
impl FormaMcp {
    // --- reading ----------------------------------------------------------

    #[tool(name = "forma_status")]
    async fn forma_status(&self, extensions: Extensions) -> Result<CallToolResult, ErrorData> {
        let where_am_i = match &self.deployment {
            Some(deployment) => format!(
                "forma-server reachable, serving {deployment} (from {} on that host)",
                self.client.base_url()
            ),
            None => format!("forma-server reachable at {}", self.client.base_url()),
        };
        Ok(answer(
            where_am_i,
            self.get(&extensions, "forma_status", "/status").await,
        ))
    }

    #[tool(name = "list_projects")]
    async fn list_projects(&self, extensions: Extensions) -> Result<CallToolResult, ErrorData> {
        match self
            .get::<Value>(&extensions, "list_projects", "/projects")
            .await
        {
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

    #[tool(name = "get_project")]
    async fn get_project(
        &self,
        extensions: Extensions,
        Parameters(params): Parameters<ProjectParams>,
    ) -> Result<CallToolResult, ErrorData> {
        Ok(answer(
            "project",
            self.get(
                &extensions,
                "get_project",
                &format!("/projects/{}", params.project_id),
            )
            .await,
        ))
    }

    #[tool(name = "get_brand")]
    async fn get_brand(
        &self,
        extensions: Extensions,
        Parameters(params): Parameters<ProjectParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let brand = self
            .get::<Value>(
                &extensions,
                "get_brand",
                &format!("/projects/{}/brand", params.project_id),
            )
            .await;
        let status = self.get::<Value>(&extensions, "get_brand", "/status").await;

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

    #[tool(name = "list_assets")]
    async fn list_assets(
        &self,
        extensions: Extensions,
        Parameters(params): Parameters<ProjectParams>,
    ) -> Result<CallToolResult, ErrorData> {
        match self
            .get::<Value>(
                &extensions,
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

    #[tool(name = "list_templates")]
    async fn list_templates(
        &self,
        extensions: Extensions,
        Parameters(params): Parameters<ProjectParams>,
    ) -> Result<CallToolResult, ErrorData> {
        match self
            .get::<Value>(
                &extensions,
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

    #[tool(name = "get_design")]
    async fn get_design(
        &self,
        extensions: Extensions,
        Parameters(params): Parameters<DesignParams>,
    ) -> Result<CallToolResult, ErrorData> {
        match self
            .get::<Value>(
                &extensions,
                "get_design",
                &format!("/designs/{}", params.design_id),
            )
            .await
        {
            Ok(record) => Ok(ok(outline(&record), record)),
            Err(error) => Ok(failed(error)),
        }
    }

    #[tool(name = "get_asset_request")]
    async fn get_asset_request(
        &self,
        extensions: Extensions,
        Parameters(params): Parameters<AssetRequestParams>,
    ) -> Result<CallToolResult, ErrorData> {
        match self
            .get::<Value>(
                &extensions,
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

    #[tool(name = "create_design")]
    async fn create_design(
        &self,
        extensions: Extensions,
        peer: Peer<RoleServer>,
        Parameters(params): Parameters<CreateDesignParams>,
    ) -> Result<CallToolResult, ErrorData> {
        if let Some(template_id) = &params.template_id {
            return match self
                .post::<Value, _>(
                    &extensions,
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
            .post::<Value, _>(
                &extensions,
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

    #[tool(name = "apply_operations")]
    async fn apply_operations(
        &self,
        extensions: Extensions,
        peer: Peer<RoleServer>,
        Parameters(params): Parameters<ApplyOperationsParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let body = json!({
            "base_version": params.base_version,
            "operations": params.operations,
        });

        match self
            .post::<Value, _>(
                &extensions,
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

    #[tool(name = "request_asset")]
    async fn request_asset(
        &self,
        extensions: Extensions,
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
            .post::<Value, _>(
                &extensions,
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

    #[tool(name = "validate_design")]
    async fn validate_design(
        &self,
        extensions: Extensions,
        Parameters(params): Parameters<DesignParams>,
    ) -> Result<CallToolResult, ErrorData> {
        match self
            .post::<Value, _>(
                &extensions,
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

    #[tool(name = "render_preview")]
    async fn render_preview(
        &self,
        extensions: Extensions,
        Parameters(params): Parameters<RenderPreviewParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let body = json!({
            "page_id": params.page_id,
            "scale": params.scale.unwrap_or(1.0),
        });

        match self
            .post::<Value, _>(
                &extensions,
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
                let mut structured = render.clone();
                // The base64 is already in the image block; repeating it in the
                // structured result would double the tokens for nothing.
                if let Some(object) = structured.as_object_mut() {
                    object.remove("png_base64");
                }

                let mut result = CallToolResult::success(vec![
                    ContentBlock::text(format!(
                        "Rendered version {} at scale {} ({} bytes).",
                        render["version"], render["scale"], render["byte_size"]
                    )),
                    // The same JSON as text, for a host that reads no structured
                    // content. Stripped of the base64 first, so this cannot be
                    // the copy that doubles the image.
                    ContentBlock::text(structured.to_string()),
                    ContentBlock::image(encoded.to_string(), "image/png"),
                ]);

                result.structured_content = Some(structured);
                Ok(result)
            }
            Err(error) => Ok(failed(error)),
        }
    }

    #[tool(name = "export_design")]
    async fn export_design(
        &self,
        extensions: Extensions,
        peer: Peer<RoleServer>,
        Parameters(params): Parameters<ExportDesignParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let body = json!({
            "scope": params.scope.unwrap_or_else(|| "all_pages".to_string()),
            "page_id": params.page_id,
            "scale": params.scale.unwrap_or(1.0),
        });

        match self
            .post::<Value, _>(
                &extensions,
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    /// The routes as the `#[tool]` macros alone produce them, before the
    /// registry is applied: what this file would serve if it were its own
    /// definition.
    fn raw_routes() -> ToolRouter<FormaMcp> {
        FormaMcp::tool_router()
    }

    #[test]
    fn the_routes_are_the_registrys() {
        // Both directions: a handler the registry does not list would be served
        // with no description, and a listed tool no handler serves would be
        // promised to every host and then fail when called.
        let handlers: BTreeSet<String> = raw_routes()
            .map
            .keys()
            .map(|name| name.to_string())
            .collect();
        let listed: BTreeSet<String> = agent_catalog::mcp_tools()
            .iter()
            .map(|tool| tool.name.to_string())
            .collect();
        assert_eq!(handlers, listed);
    }

    #[test]
    fn no_handler_writes_its_own_description() {
        // rmcp falls back to a handler's doc comment for a description, so a
        // doc comment on a handler is a second definition the registry would
        // then silently overwrite. Keep comments on handlers as `//`.
        for (name, route) in raw_routes().map {
            assert_eq!(route.attr.description, None, "{name}");
            assert_eq!(route.attr.annotations, None, "{name}");
        }
    }

    #[test]
    fn each_handler_reads_the_type_the_registry_describes() {
        // The schema a host is shown comes from the registry; the type a
        // handler deserializes comes from its `Parameters<T>`. They must be the
        // same type, or a host is told one shape and the handler reads another.
        // rmcp derives this schema itself, so this also proves the registry
        // generates schemas exactly as rmcp does.
        let raw = raw_routes();
        for operation in agent_catalog::REGISTRY {
            let Some(name) = operation.doors.mcp_tool() else {
                continue;
            };
            assert_eq!(
                *raw.map[name].attr.input_schema,
                (operation.input)(),
                "{name} reads a different type than the registry describes"
            );
        }
    }

    #[test]
    fn the_served_tool_list_is_the_registry_rendered_for_mcp() {
        let served = from_registry(raw_routes());
        for tool in agent_catalog::mcp_tools() {
            let attr = &served.map[tool.name].attr;
            assert_eq!(attr.description.as_deref(), Some(tool.description.as_str()));
            assert_eq!(*attr.input_schema, tool.input_schema);
        }
    }
}
