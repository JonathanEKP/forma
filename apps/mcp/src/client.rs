//! Internal REST client for `forma-server`.
//!
//! Plan section 5: the MCP process never opens SQLite. It goes through the same
//! HTTP API the React editor uses, so there is one mutation authority.

use serde::Deserialize;

pub struct ServerClient {
    base_url: String,
    http: reqwest::Client,
}

#[derive(Debug, Deserialize)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
}

#[derive(Debug, Deserialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
}

pub struct RenderedCard {
    pub png: Vec<u8>,
    pub canvas: Option<String>,
}

impl ServerClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            http: reqwest::Client::new(),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    fn url(&self, path: &str) -> String {
        format!("{}/api/v1{path}", self.base_url)
    }

    /// Turns a non-2xx response into the plan's stable error shape, so the agent
    /// sees `code` and `message` instead of an HTML error page.
    async fn decode_error(response: reqwest::Response) -> anyhow::Error {
        let status = response.status();
        match response.json::<ErrorEnvelope>().await {
            Ok(envelope) => {
                anyhow::anyhow!("{}: {}", envelope.error.code, envelope.error.message)
            }
            Err(_) => anyhow::anyhow!("SERVER_ERROR: forma-server returned HTTP {status}"),
        }
    }

    pub async fn status(&self) -> anyhow::Result<serde_json::Value> {
        let response = self
            .http
            .get(self.url("/status"))
            .send()
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "SERVER_UNREACHABLE: cannot reach forma-server at {} ({e})",
                    self.base_url
                )
            })?;

        if !response.status().is_success() {
            return Err(Self::decode_error(response).await);
        }
        Ok(response.json().await?)
    }

    pub async fn render_test_card(&self, label: Option<String>) -> anyhow::Result<RenderedCard> {
        let response = self
            .http
            .post(self.url("/render/test-card"))
            .json(&serde_json::json!({ "label": label }))
            .send()
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "SERVER_UNREACHABLE: cannot reach forma-server at {} ({e})",
                    self.base_url
                )
            })?;

        if !response.status().is_success() {
            return Err(Self::decode_error(response).await);
        }

        let canvas = response
            .headers()
            .get("x-forma-canvas")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);

        Ok(RenderedCard {
            png: response.bytes().await?.to_vec(),
            canvas,
        })
    }
}
