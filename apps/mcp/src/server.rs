//! The MCP surface (plan sections 47 and 48).
//!
//! Phase 0 ships only the two tools the phase acceptance requires. The rest of
//! the thirteen tools arrive in Phase 8, once designs actually exist.

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, ContentBlock, Implementation, ProtocolVersion, ServerCapabilities, ServerInfo,
};
use rmcp::{tool, tool_handler, tool_router, ErrorData, ServerHandler};

use base64::Engine as _;

use crate::client::ServerClient;

#[derive(Debug, Default, serde::Deserialize, schemars::JsonSchema)]
pub struct RenderTestCardParams {
    /// Optional text painted onto the card, useful to prove the render is live.
    #[serde(default)]
    pub label: Option<String>,
}

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

/// Errors reach the agent as text with the plan's code prefix, so the recovery
/// table of section 51.9 applies to MCP exactly as it does to REST.
fn tool_error(error: anyhow::Error) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(error.to_string())])
}

#[tool_router]
impl FormaMcp {
    #[tool(
        name = "forma_status",
        description = "Report whether forma-server is reachable and what it currently holds: \
                       version, document schema version, available font families and project count. \
                       Call this first when a Forma session starts."
    )]
    async fn forma_status(&self) -> Result<CallToolResult, ErrorData> {
        match self.client.status().await {
            Ok(status) => {
                let mut result = CallToolResult::success(vec![ContentBlock::text(format!(
                    "forma-server reachable at {}",
                    self.client.base_url()
                ))]);
                result.structured_content = Some(status);
                Ok(result)
            }
            Err(e) => Ok(tool_error(e)),
        }
    }

    #[tool(
        name = "render_test_card",
        description = "Render a Forma test card and return it as a PNG image. This proves the \
                       full pipeline (document -> SVG -> PNG) works end to end and that this \
                       host can actually receive images. It creates no project and no design."
    )]
    async fn render_test_card(
        &self,
        Parameters(params): Parameters<RenderTestCardParams>,
    ) -> Result<CallToolResult, ErrorData> {
        match self.client.render_test_card(params.label).await {
            Ok(card) => {
                let encoded = base64::engine::general_purpose::STANDARD.encode(&card.png);
                let canvas = card.canvas.unwrap_or_else(|| "unknown".to_string());

                // Plan section 49: a structured result AND the image itself, so
                // multimodal hosts can inspect the piece instead of trusting a path.
                let mut result = CallToolResult::success(vec![
                    ContentBlock::text(format!(
                        "Rendered a {canvas} test card ({} bytes of PNG).",
                        card.png.len()
                    )),
                    ContentBlock::image(encoded, "image/png"),
                ]);
                result.structured_content = Some(serde_json::json!({
                    "canvas": canvas,
                    "byte_size": card.png.len(),
                    "mime_type": "image/png",
                }));
                Ok(result)
            }
            Err(e) => Ok(tool_error(e)),
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
             deterministically and renders it. Start with forma_status. If it reports that \
             forma-server is unreachable, tell the user to start it rather than retrying. \
             render_test_card returns a real PNG: if you can see it, say what it shows; if \
             your host did not deliver the image, say so instead of claiming it looks fine."
                .to_string(),
        );
        info
    }
}
