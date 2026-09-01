//! Internal REST client for `forma-server`.
//!
//! Plan section 5: the MCP process never opens SQLite. It goes through the same
//! HTTP API the React editor uses, so there is one mutation authority and the
//! agent and the human get identical behaviour.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// A failure the agent can act on.
///
/// The code and the hint both survive the trip: plan section 51.9 is a recovery
/// table keyed by code, and an agent that only saw prose would have to guess
/// whether to refetch, fix the design, or stop.
#[derive(Debug, Clone)]
pub struct ServerError {
    pub code: String,
    pub message: String,
    pub hint: Option<String>,
    pub details: Option<Value>,
}

impl ServerError {
    fn local(code: &str, message: impl Into<String>, hint: &str) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
            hint: Some(hint.to_string()),
            details: None,
        }
    }

    pub fn as_json(&self) -> Value {
        serde_json::json!({
            "error": {
                "code": self.code,
                "message": self.message,
                "hint": self.hint,
                "details": self.details,
            }
        })
    }
}

impl std::fmt::Display for ServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)?;
        if let Some(hint) = &self.hint {
            write!(f, "\n{hint}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ServerError {}

pub type Result<T> = std::result::Result<T, ServerError>;

#[derive(Debug, Deserialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    code: String,
    message: String,
    #[serde(default)]
    hint: Option<String>,
    #[serde(default)]
    details: Option<Value>,
}

/// A bearer already accepted at Forma's inbound boundary.
///
/// Deliberately has no Debug implementation: a derived formatter is an easy way
/// for an otherwise harmless trace to print a credential.
#[derive(Clone)]
pub(crate) struct BearerCredential(String);

/// What the verifier said (plan sections 109.2 and 115).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    Yes,
    /// The credential is not good.
    No,
    /// The credential is good and belongs to nobody here yet.
    NoAccount,
}

impl BearerCredential {
    pub(crate) fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

pub struct ServerClient {
    base_url: String,
    http: reqwest::Client,
}

impl ServerClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            http: reqwest::Client::new(),
        }
    }

    /// Adds this call's bearer when there is one.
    ///
    /// The credential is an argument rather than client state because one MCP
    /// process serves concurrent callers. Holding it here would silently make
    /// every session use whichever person's credential constructed the client.
    fn authorized(
        &self,
        request: reqwest::RequestBuilder,
        credential: Option<&str>,
    ) -> reqwest::RequestBuilder {
        match credential {
            Some(credential) => request.bearer_auth(credential),
            None => request,
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    fn url(&self, path: &str) -> String {
        format!("{}/api/v1{path}", self.base_url)
    }

    fn unreachable(&self, error: &reqwest::Error) -> ServerError {
        ServerError::local(
            "SERVER_UNREACHABLE",
            format!("cannot reach forma-server at {} ({error})", self.base_url),
            "Start forma-server and try again. Retrying without starting it will keep failing.",
        )
    }

    /// Turns a non-2xx response into the stable error shape of plan section 32.
    async fn decode_error(response: reqwest::Response) -> ServerError {
        let status = response.status();
        match response.json::<ErrorEnvelope>().await {
            Ok(envelope) => ServerError {
                code: envelope.error.code,
                message: envelope.error.message,
                hint: envelope.error.hint,
                details: envelope.error.details,
            },
            // A body that is not the documented envelope means something
            // upstream of the handlers failed; say so rather than inventing a
            // domain code the agent would try to recover from.
            Err(_) => ServerError::local(
                "SERVER_ERROR",
                format!("forma-server returned HTTP {status}"),
                "This is not a design problem. Check the server logs.",
            ),
        }
    }

    async fn read<T: DeserializeOwned>(response: reqwest::Response) -> Result<T> {
        if !response.status().is_success() {
            return Err(Self::decode_error(response).await);
        }
        response.json::<T>().await.map_err(|e| {
            ServerError::local(
                "SERVER_ERROR",
                format!("forma-server sent a body this client could not read: {e}"),
                "This is a version mismatch between forma-mcp and forma-server.",
            )
        })
    }

    /// Reads, telling the server which tool asked.
    ///
    /// The tool name and a request id travel as headers so the server's own log
    /// line names both (plan section 60). Logging it only here would put the
    /// tool name in a process whose stderr the host usually discards.
    pub async fn get<T: DeserializeOwned>(
        &self,
        credential: Option<&str>,
        tool: &str,
        path: &str,
    ) -> Result<T> {
        let request_id = Uuid::new_v4().to_string();
        tracing::debug!(tool_name = tool, request_id = %request_id, path, "GET");

        let response = self
            .authorized(self.http.get(self.url(path)), credential)
            .header("x-forma-tool", tool)
            // Says which door this came through, so `forma-server` knows to
            // require an agent credential rather than accepting a session that
            // merely exists in somebody's browser (plan section 112). It only
            // narrows what is accepted, which is what makes it safe to send.
            .header("x-forma-entry", "mcp")
            .header("x-request-id", &request_id)
            .send()
            .await
            .map_err(|e| self.unreachable(&e))?;
        Self::read(response).await
    }

    /// Every mutation carries the actor, so the history of plan section 61
    /// records which agent made each version rather than attributing it to a
    /// human who was not there.
    pub async fn post<T: DeserializeOwned, B: Serialize>(
        &self,
        credential: Option<&str>,
        tool: &str,
        path: &str,
        actor: &str,
        body: &B,
    ) -> Result<T> {
        let request_id = Uuid::new_v4().to_string();
        tracing::debug!(tool_name = tool, request_id = %request_id, path, actor, "POST");

        let response = self
            .authorized(self.http.post(self.url(path)), credential)
            .header("x-forma-actor", actor)
            .header("x-forma-tool", tool)
            // Says which door this came through, so `forma-server` knows to
            // require an agent credential rather than accepting a session that
            // merely exists in somebody's browser (plan section 112). It only
            // narrows what is accepted, which is what makes it safe to send.
            .header("x-forma-entry", "mcp")
            .header("x-request-id", &request_id)
            .json(body)
            .send()
            .await
            .map_err(|e| self.unreachable(&e))?;
        Self::read(response).await
    }
}

impl ServerClient {
    /// What `forma-server` says about this credential.
    ///
    /// The single verifier answers and this process relays the question (plan
    /// section 109.2). `/api/v1/status` is used because it is behind the
    /// credential and cheap; what is read is the status, not the body.
    pub async fn admits(&self, credential: &str) -> Admission {
        let answer = self
            .http
            .get(format!("{}/api/v1/status", self.base_url))
            .bearer_auth(credential)
            .header("x-forma-entry", "mcp")
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await;

        match answer {
            Ok(response) if response.status().is_success() => Admission::Yes,
            // A verified identity that owns no Forma account. Kept distinct
            // because the two need opposite things from the caller, and saying
            // 401 here would send a client to authenticate again — which cannot
            // help, because signing in a second time does not create an account.
            Ok(response) if response.status() == reqwest::StatusCode::FORBIDDEN => {
                Admission::NoAccount
            }
            Ok(_) => Admission::No,
            // A server that cannot be reached answers no. Failing closed is the
            // only safe direction: the alternative is serving the tool surface
            // to whoever asks whenever the backend is briefly down.
            Err(_) => Admission::No,
        }
    }
}
