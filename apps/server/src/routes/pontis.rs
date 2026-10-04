//! Boundary F: the fixed operations the Pontis sidecar calls (Pontis plan §4,
//! step 6 of §11).
//!
//! Each route is one capability, calling the same `application` use case the
//! REST handler beside it calls. There is no generic proxy here and no domain
//! logic: what this module adds is the transport Boundary F owes and the REST
//! surface does not have.
//!
//! - **Who.** By the time a handler runs, the service arm of
//!   `auth::require_bearer` has verified the `forma_svc_` credential, required a
//!   represented user and an application context, and revalidated the user's
//!   membership in it. Nothing here re-derives that; [`represented`] only
//!   refuses a request that did not come through that arm at all.
//! - **Bounds.** Request bodies are capped at [`MAX_REQUEST_BYTES`]. Results,
//!   lists and the validate deadline are capped at what the agent catalog
//!   declares for the capability ([`bounds`]) — the same numbers the sidecar
//!   hands Pontis, so the route and the runtime cannot disagree about what is
//!   too large. A result over its bound is refused as `result_too_large`,
//!   never truncated (pontis docs/06).
//! - **Errors.** Pontis owns the capability error registry; Forma keeps its own
//!   §32 envelope as the wire shape and adds one field, `capability_error`,
//!   which says where the failure sits in that registry. The Forma `code` stays,
//!   because `VERSION_CONFLICT` (refetch and rebase) and `DESIGN_NOT_FOUND`
//!   (give up) mean different things to a caller that both registries'
//!   categories would blur. A refusal from the authentication arm is written
//!   before any route runs and carries no `capability_error`; every 401 or 403
//!   reaching the sidecar that way is `authorization_denied`.
//! - **Idempotency.** A command carries the runtime's invocation id in
//!   [`INVOCATION_HEADER`] and goes through the ledger of 307a74a: claimed before
//!   it executes, answered from the record on a duplicate, refused when the same
//!   id arrives with a different request. Resources are not ledgered.
//! - **Cancellation.** Only `design.validate` has it today, and it gets it the
//!   way the REST validate route does, plus the caller's deadline.
//!
//! **What a stale claim means, decided here.** The ledger writes the claim and
//! the result separately, so an invocation interrupted between them answers
//! `InFlight` for good — at most once, not at least once. These routes never
//! execute an `InFlight` invocation again and never release one: whether its
//! batch landed is unknown, and guessing wrong in either direction is the
//! duplicate or the loss the ledger exists to prevent. What they do decide is
//! how to *say* it. A claim younger than [`IN_FLIGHT_STALE_AFTER`] is
//! `INVOCATION_IN_FLIGHT` with a `Retry-After`; an older one is
//! `INVOCATION_OUTCOME_UNKNOWN`, which no retry will change. Resolving those —
//! reconciling against the design's history, or a human releasing them — is a
//! policy this module does not set. The window is narrowed rather than left to
//! chance: claim, execution and result run on a task of their own, so a
//! sidecar that disconnects mid-call cannot strand a claim by dropping the
//! handler between them; only a crash can.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use actix_web::{get, http::StatusCode, post, web, HttpRequest, HttpResponse, ResponseError};
use agent_catalog::{CapabilityBounds, CapabilityRisk};
use application::{ApplicationError, Cancellation};
use design_core::{
    AccountId, AssetRequest, AuthKind, AuthUserId, BrandKit, DesignId, DesignOperation,
    InvocationId, ProjectId, Version,
};
use persistence::{
    Claim, InvocationLedger, InvocationRequest, PersistenceError, SqliteInvocationLedger,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::ApiError;
use crate::routes::{actor_from, design_id, parse_id};
use crate::state::AppState;
use crate::telemetry;

/// The largest request body a capability accepts. A full batch of twenty
/// operations with long copy is a few kilobytes; this leaves room without
/// letting a request become a way to make the server buffer anything.
pub const MAX_REQUEST_BYTES: usize = 256 * 1024;

/// How many validates one account may run at once. It is the cheapest
/// capability to call and the most expensive to serve (plan §4), so it is the
/// one that needs a throttle.
pub const MAX_CONCURRENT_VALIDATIONS: usize = 2;

/// How old a claim must be before its outcome is called unknown rather than
/// pending. A batch is a read and one transaction; nothing legitimate holds a
/// claim for a minute.
pub const IN_FLIGHT_STALE_AFTER: Duration = Duration::from_secs(60);

/// The runtime's invocation id, which a command carries as its idempotency
/// key. A header rather than a body field: it is not part of what the model
/// asked for, and the model never supplies it.
pub const INVOCATION_HEADER: &str = "x-forma-invocation-id";

/// The caller's remaining deadline, in milliseconds, for an operation that can
/// be abandoned.
pub const TIMEOUT_HEADER: &str = "x-forma-timeout-ms";

/// On a command's success: `executed` for the first delivery, `replayed` for an
/// answer read back from the ledger.
pub const INVOCATION_OUTCOME_HEADER: &str = "x-forma-invocation";

pub const PROJECT_LIST: &str = "project.list";
pub const DESIGN_LIST: &str = "design.list";
pub const DESIGN_GET: &str = "design.get";
pub const DESIGN_VALIDATE: &str = "design.validate";
pub const APPLY_OPERATIONS: &str = "design.applyOperations";
pub const FORMA_GUIDE: &str = "forma.guide";
pub const BRAND_GET: &str = "brand.get";
pub const ASSET_REQUEST_CREATE: &str = "assetRequest.create";

/// Every capability Boundary F serves, one per route.
pub const SERVED: [&str; 8] = [
    PROJECT_LIST,
    DESIGN_LIST,
    DESIGN_GET,
    DESIGN_VALIDATE,
    APPLY_OPERATIONS,
    FORMA_GUIDE,
    BRAND_GET,
    ASSET_REQUEST_CREATE,
];

/// What the agent catalog declares for a capability: the one source of its
/// result size, list length and timeout. The measurements behind the numbers
/// are beside them, in `agent_catalog::operations`.
///
/// # Panics
///
/// When the catalog has no such capability. `every_route_is_a_declared_capability`
/// fails on that first.
pub fn bounds(capability: &str) -> CapabilityBounds {
    agent_catalog::operation(capability)
        .unwrap_or_else(|| panic!("the agent catalog has no {capability}"))
        .bounds
}

/// The longest a validate may run, and what it gets when the caller names no
/// deadline.
pub fn max_validate_timeout() -> Duration {
    Duration::from_millis(bounds(DESIGN_VALIDATE).timeout_ms.into())
}

/// Where a failure sits in the Pontis capability error registry
/// (pontis/docs/06-capability-system.md § Typed error registry).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum CapabilityError {
    CapabilityNotFound,
    ValidationError,
    ResultTooLarge,
    Timeout,
    AuthorizationDenied,
    ApprovalRequired,
    ApprovalExpired,
    ApprovalScopeMismatch,
    NotFound,
    Conflict,
    ResourceStateChanged,
    InvocationAlreadyCompleted,
    RateLimited,
    Unavailable,
    InternalError,
}

impl CapabilityError {
    /// `internal_error` lets no detail cross the boundary (docs/06), and
    /// `unavailable` is the same kind of fact about this server: which
    /// dependency failed, and how, is for its log.
    fn hides_detail(self) -> bool {
        matches!(self, Self::InternalError | Self::Unavailable)
    }
}

/// Where a Forma failure sits in the registry.
///
/// By variant where the code alone is ambiguous: `INVALID_DOCUMENT` is the
/// caller's mistake when it comes from a batch and a corrupt row when it comes
/// from storage, and only the variant says which.
pub fn classify(error: &ApiError) -> CapabilityError {
    match error {
        ApiError::Application(error) => match error {
            ApplicationError::Persistence(error) => match error {
                // The batch was built on a version that is no longer current.
                // An approved batch is reclassified where it is known to be
                // approved, in `apply_operations`.
                PersistenceError::VersionConflict { .. } => CapabilityError::Conflict,
                PersistenceError::InvalidTransition(_) => CapabilityError::Conflict,
                PersistenceError::DesignNotFound(_)
                | PersistenceError::ProjectNotFound(_)
                | PersistenceError::NotFound(_) => CapabilityError::NotFound,
                PersistenceError::CorruptDocument(_) => CapabilityError::InternalError,
                PersistenceError::Storage(_) => CapabilityError::Unavailable,
            },
            ApplicationError::Domain(_) | ApplicationError::Rejected { .. } => {
                CapabilityError::ValidationError
            }
            ApplicationError::Render(_) => CapabilityError::InternalError,
            ApplicationError::Cancelled => CapabilityError::Timeout,
        },
        ApiError::Request { status, .. } => match *status {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                CapabilityError::AuthorizationDenied
            }
            StatusCode::NOT_FOUND => CapabilityError::NotFound,
            status if status.is_client_error() => CapabilityError::ValidationError,
            _ => CapabilityError::InternalError,
        },
    }
}

/// The §32 envelope, with the registry code beside the Forma one. Boxed, so
/// every `Result` carrying it stays small.
#[derive(Debug)]
pub struct PontisError(Box<Refusal>);

#[derive(Debug)]
pub struct Refusal {
    status: StatusCode,
    code: String,
    message: String,
    hint: String,
    details: Option<serde_json::Value>,
    capability_error: CapabilityError,
    retry_after: Option<u64>,
}

impl std::ops::Deref for PontisError {
    type Target = Refusal;

    fn deref(&self) -> &Refusal {
        &self.0
    }
}

impl std::ops::DerefMut for PontisError {
    fn deref_mut(&mut self) -> &mut Refusal {
        &mut self.0
    }
}

impl PontisError {
    fn new(
        status: StatusCode,
        code: &str,
        capability_error: CapabilityError,
        message: impl Into<String>,
        hint: &str,
    ) -> Self {
        Self(Box::new(Refusal {
            status,
            code: code.to_string(),
            message: message.into(),
            hint: hint.to_string(),
            details: None,
            capability_error,
            retry_after: None,
        }))
    }

    fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }

    fn retry_after(mut self, seconds: u64) -> Self {
        self.retry_after = Some(seconds);
        self
    }

    pub fn code(&self) -> &str {
        &self.code
    }

    pub fn capability_error(&self) -> CapabilityError {
        self.capability_error
    }
}

impl std::fmt::Display for PontisError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl From<ApiError> for PontisError {
    fn from(error: ApiError) -> Self {
        let capability_error = classify(&error);
        Self(Box::new(Refusal {
            // Two statuses change. A dependency that is down is a 503, which a
            // caller may retry, where §32 has only the generic 500; and a fault
            // of ours is a 500 even when its code is shared with a caller's
            // mistake — `INVALID_DOCUMENT` from a corrupt row is not a 400.
            status: match capability_error {
                CapabilityError::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
                CapabilityError::InternalError => StatusCode::INTERNAL_SERVER_ERROR,
                _ => error.status_code(),
            },
            code: error.code().to_string(),
            message: error.to_string(),
            hint: error.hint().to_string(),
            details: error.details(),
            capability_error,
            retry_after: None,
        }))
    }
}

impl From<ApplicationError> for PontisError {
    fn from(error: ApplicationError) -> Self {
        ApiError::from(error).into()
    }
}

impl From<PersistenceError> for PontisError {
    fn from(error: PersistenceError) -> Self {
        ApplicationError::from(error).into()
    }
}

#[derive(Serialize)]
struct PontisErrorBody<'a> {
    code: &'a str,
    message: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<&'a serde_json::Value>,
    hint: &'a str,
    capability_error: CapabilityError,
}

#[derive(Serialize)]
struct PontisErrorEnvelope<'a> {
    error: PontisErrorBody<'a>,
}

impl ResponseError for PontisError {
    fn status_code(&self) -> StatusCode {
        self.status
    }

    fn error_response(&self) -> HttpResponse {
        let hidden = self.capability_error.hides_detail();
        if hidden {
            // The message can name a table or a file; it goes to the log, not
            // to a model. It never carries a credential or asset bytes.
            tracing::error!(code = %self.code, error = %self.message, "boundary F operation failed");
        }
        let mut response = HttpResponse::build(self.status);
        if let Some(seconds) = self.retry_after {
            response.insert_header(("retry-after", seconds.to_string()));
        }
        response.json(PontisErrorEnvelope {
            error: PontisErrorBody {
                code: &self.code,
                message: if hidden {
                    "the operation failed on the server"
                } else {
                    &self.message
                },
                details: if hidden { None } else { self.details.as_ref() },
                hint: &self.hint,
                capability_error: self.capability_error,
            },
        })
    }
}

/// The represented account and person, as the service arm resolved them.
struct Represented {
    account: AccountId,
    user: AuthUserId,
}

/// Refuses anything that did not arrive through the service arm.
///
/// Not a second authentication: the arm already decided. This is what stops a
/// PAT, a browser session or local mode — which all reach `/api/v1` — from
/// using a surface whose ledger and actor both need a represented person.
fn represented(request: &HttpRequest, state: &AppState) -> Result<Represented, PontisError> {
    let auth = crate::routes::auth(request, state);
    match (auth.kind, auth.user_id) {
        (AuthKind::Service, Some(user)) => Ok(Represented {
            account: auth.account_id,
            user,
        }),
        _ => Err(PontisError::new(
            StatusCode::FORBIDDEN,
            "SERVICE_CREDENTIAL_REQUIRED",
            CapabilityError::AuthorizationDenied,
            "Boundary F operations take the sidecar's service credential",
            "Only the Pontis sidecar calls these routes; everything else uses /api/v1 directly.",
        )),
    }
}

/// Serializes a result, refusing one over the capability's bound rather than
/// cutting it.
fn bounded(capability: &str, value: &impl Serialize) -> Result<Vec<u8>, PontisError> {
    let max_bytes = bounds(capability).max_result_bytes as usize;
    let bytes = serde_json::to_vec(value).map_err(|error| {
        PontisError::from(ApiError::internal(
            "SERIALIZATION_FAILED",
            error.to_string(),
        ))
    })?;
    if bytes.len() > max_bytes {
        return Err(PontisError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "RESULT_TOO_LARGE",
            CapabilityError::ResultTooLarge,
            format!(
                "the result is {} bytes, over the {max_bytes}-byte bound",
                bytes.len()
            ),
            "Ask for less: a single design rather than a listing of everything.",
        )
        .with_details(serde_json::json!({ "bytes": bytes.len(), "max_bytes": max_bytes })));
    }
    Ok(bytes)
}

fn respond(capability: &str, value: &impl Serialize) -> Result<HttpResponse, PontisError> {
    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .body(bounded(capability, value)?))
}

/// The project list: the bounded read of PV-1.
#[get("/projects")]
async fn list_projects(
    state: web::Data<AppState>,
    request: HttpRequest,
) -> Result<HttpResponse, PontisError> {
    let who = represented(&request, &state)?;
    let projects = application::projects::list(&*state.projects, who.account).await?;
    let max_items = bounds(PROJECT_LIST).max_list_items;
    if let Some(max_items) = max_items
        .map(|max| max as usize)
        .filter(|max| projects.len() > *max)
    {
        return Err(PontisError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "RESULT_TOO_LARGE",
            CapabilityError::ResultTooLarge,
            format!(
                "this account has {} projects, over the {max_items}-item bound",
                projects.len()
            ),
            "The list cannot be paged yet; open a project by id instead.",
        )
        .with_details(serde_json::json!({ "items": projects.len(), "max_items": max_items })));
    }
    respond(PROJECT_LIST, &projects)
}

/// A project's designs, most recently changed first, without their documents.
///
/// What lets an agent turn "open my cover" into an id: nothing else on this door
/// enumerates designs, so a design the person had not pasted an id for could not
/// be reached at all (F-40-6). A project of another account is answered exactly
/// as a project that does not exist, like `brand.get`.
#[get("/projects/{id}/designs")]
async fn list_designs(
    state: web::Data<AppState>,
    request: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse, PontisError> {
    let who = represented(&request, &state)?;
    let project = ProjectId(parse_id(&path, "project id")?);
    application::projects::get(&*state.projects, who.account, project).await?;
    let designs = application::designs::list(&*state.designs, who.account, project).await?;
    let max_items = bounds(DESIGN_LIST).max_list_items;
    if let Some(max_items) = max_items
        .map(|max| max as usize)
        .filter(|max| designs.len() > *max)
    {
        return Err(PontisError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "RESULT_TOO_LARGE",
            CapabilityError::ResultTooLarge,
            format!(
                "this project has {} designs, over the {max_items}-item bound",
                designs.len()
            ),
            "The list cannot be paged yet; read a design by id instead.",
        )
        .with_details(serde_json::json!({ "items": designs.len(), "max_items": max_items })));
    }
    respond(DESIGN_LIST, &designs)
}

/// One design, carrying the version a batch must echo as `base_version`.
#[get("/designs/{id}")]
async fn get_design(
    state: web::Data<AppState>,
    request: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse, PontisError> {
    let who = represented(&request, &state)?;
    let record = application::designs::get(&*state.designs, who.account, design_id(&path)?).await?;
    respond(DESIGN_GET, &record)
}

/// The manual (plan §6.1): the template compiled into this binary, rendered for
/// the Pontis door, with its revision.
///
/// It holds nothing of the account's, and is still served only to a
/// represented person in an account they belong to, like every other
/// capability: there is no exception to carve out and then remember. Reading
/// it is not a precondition for anything — no write is refused because the
/// manual went unread, since a read proves delivery, not comprehension.
#[get("/guide")]
async fn guide(
    state: web::Data<AppState>,
    request: HttpRequest,
) -> Result<HttpResponse, PontisError> {
    represented(&request, &state)?;
    respond(FORMA_GUIDE, agent_catalog::manual::pontis())
}

/// What `brand.get` answers: the kit **and** the font families the renderer
/// can draw, as forma-mcp's `get_brand` answers them.
///
/// The catalog travels with the kit because a family the renderer lacks is a
/// `FONT_NOT_FOUND` the agent could have avoided, and the guidance tells the
/// agent to use only families from it. A bare kit would make that instruction
/// unfollowable on this door. `brand` is `null` when the project has no kit,
/// and then `$brand.*` tokens do not resolve.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct PontisBrand {
    pub brand: Option<BrandKit>,
    pub fonts: Vec<String>,
}

/// A project's Brand Kit and the font catalog.
#[get("/projects/{id}/brand")]
async fn get_brand(
    state: web::Data<AppState>,
    request: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse, PontisError> {
    let who = represented(&request, &state)?;
    let project = ProjectId(parse_id(&path, "project id")?);
    let brand = application::projects::get_brand(&*state.projects, who.account, project).await?;
    respond(
        BRAND_GET,
        &PontisBrand {
            brand,
            fonts: state.fonts.families().to_vec(),
        },
    )
}

/// The caller's deadline: absent means the ceiling, and one over the ceiling is
/// refused rather than lowered, so a caller never believes it has more time
/// than it does.
fn deadline(request: &HttpRequest) -> Result<Duration, PontisError> {
    let Some(raw) = request.headers().get(TIMEOUT_HEADER) else {
        return Ok(max_validate_timeout());
    };
    raw.to_str()
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .map(Duration::from_millis)
        .filter(|timeout| !timeout.is_zero() && *timeout <= max_validate_timeout())
        .ok_or_else(|| {
            PontisError::new(
                StatusCode::BAD_REQUEST,
                "INVALID_TIMEOUT",
                CapabilityError::ValidationError,
                format!(
                    "{TIMEOUT_HEADER} must be a whole number of milliseconds from 1 to {}",
                    max_validate_timeout().as_millis()
                ),
                "Send the caller's remaining deadline, or omit the header.",
            )
        })
}

/// Validation: the cancellable operation.
///
/// It writes nothing, so it may stop at any of its checkpoints and loses only
/// work. Its deadline starts when the request arrives, so loading the design
/// counts against it too.
#[post("/designs/{id}/validate")]
async fn validate(
    state: web::Data<AppState>,
    request: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse, PontisError> {
    let who = represented(&request, &state)?;
    let id = design_id(&path)?;
    let cancel = Cancellation::with_deadline(Instant::now() + deadline(&request)?);
    // As in the REST validate route: a dropped handler fires the same signal.
    let _abort_on_drop = cancel.cancel_on_drop();

    let Some(_running) = state.pontis_validations.enter(who.account) else {
        return Err(PontisError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "RATE_LIMITED",
            CapabilityError::RateLimited,
            format!("this account already has {MAX_CONCURRENT_VALIDATIONS} validations running"),
            "Wait for one to finish; validate renders every page and is not cheap.",
        )
        .retry_after(1));
    };

    let record = application::designs::get(&*state.designs, who.account, id).await?;
    let brand =
        application::projects::get_brand(&*state.projects, who.account, record.project_id).await?;

    match application::renders::validate(
        who.account,
        &record,
        brand.as_ref(),
        &state.fonts,
        &*state.assets,
        &*state.store,
        &cancel,
    )
    .await
    {
        Ok(report) => respond(DESIGN_VALIDATE, &report),
        Err(ApplicationError::Cancelled) if cancel.deadline_passed() => Err(PontisError::new(
            StatusCode::GATEWAY_TIMEOUT,
            "DEADLINE_EXCEEDED",
            CapabilityError::Timeout,
            "validation stopped at the caller's deadline",
            "Nothing was kept. Validate again with a longer deadline if the result is still wanted.",
        )),
        Err(error) => Err(error.into()),
    }
}

/// The partial order of docs/06: `read ≤ write ≤ {sensitive, destructive,
/// external_side_effect}`, the last three mutually incomparable.
///
/// Boundary F's own, not the catalog's: it decides which refusal to send and
/// whether elevation raised a class. It never decides consent — that is exact
/// equality, because equal rank is not coverage.
trait Rank {
    fn rank(self) -> u8;
}

impl Rank for CapabilityRisk {
    fn rank(self) -> u8 {
        match self {
            Self::Read => 0,
            Self::Write => 1,
            Self::Sensitive | Self::Destructive | Self::ExternalSideEffect => 2,
        }
    }
}

/// What an approval covers. PV-1 needs only `once` (plan §3.1); a session grant
/// is understood, so it can be refused by name, and never accepted here.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum ApprovalScope {
    Once {
        invocation_id: String,
    },
    Session {
        #[serde(default)]
        capability_ids: Vec<String>,
    },
}

/// The consent a sidecar forwards with an elevated invocation, as docs/06
/// § Approval evidence defines it.
///
/// Trusted as far as the credential that carried it: Forma cannot verify an
/// approval independently, so what it checks is that the evidence is *for this
/// invocation* — the double policy boundary of SEC-INV-012 — not that a person
/// really clicked.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ApprovalEvidence {
    pub approval_id: String,
    pub session_id: String,
    pub capability_id: String,
    pub scope: ApprovalScope,
    pub risk: CapabilityRisk,
    pub granted_at: String,
    pub expires_at: String,
}

/// `design.applyOperations` at Boundary F.
///
/// Unknown fields are refused, so a caller cannot send a `risk` of its own and
/// have it ignored quietly: the class is never the caller's to state.
#[derive(Debug, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct PontisApplyRequest {
    pub base_version: Version,
    pub operations: Vec<DesignOperation>,
    /// Required when the batch removes anything, ignored otherwise.
    #[serde(default)]
    pub approval: Option<ApprovalEvidence>,
}

/// What a batch answers, and what the ledger keeps.
///
/// Not the whole record. The document is what a caller fetches with
/// `design.get`; keeping it out bounds the answer before the batch runs, so a
/// batch can never commit and then be refused as too large to report, and the
/// ledger holds no copy of the person's work.
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PontisApplyResult {
    pub design_id: DesignId,
    pub project_id: ProjectId,
    pub version: Version,
    pub updated_at: String,
}

/// The class `design.applyOperations` is declared at, read from the agent
/// catalog — the same entry the generated Pontis descriptor carries — rather
/// than decided again here.
///
/// # Panics
///
/// When the catalog has no such capability. `the_baseline_is_the_catalogs`
/// fails on that first.
fn baseline_risk() -> CapabilityRisk {
    agent_catalog::operation(APPLY_OPERATIONS)
        .unwrap_or_else(|| panic!("the agent catalog has no {APPLY_OPERATIONS}"))
        .risk
}

/// The class this batch deserves: the catalog's baseline, raised when the
/// batch removes work.
///
/// Exhaustive on purpose: a new operation must be classified here before it
/// compiles, rather than inheriting the baseline by default.
pub fn required_risk(operations: &[DesignOperation]) -> CapabilityRisk {
    let destroys = |operation: &DesignOperation| match operation {
        DesignOperation::RemovePage { .. } | DesignOperation::RemoveLayer { .. } => true,
        DesignOperation::AddPage { .. }
        | DesignOperation::ReorderPages { .. }
        | DesignOperation::SetBackground { .. }
        | DesignOperation::AddLayer { .. }
        | DesignOperation::MoveLayer { .. }
        | DesignOperation::ReorderLayer { .. }
        | DesignOperation::SetText { .. }
        | DesignOperation::PatchTextStyle { .. }
        | DesignOperation::SetImage { .. }
        | DesignOperation::PatchShapeStyle { .. }
        | DesignOperation::SetFormat { .. } => false,
    };
    let baseline = baseline_risk();
    if operations.iter().any(destroys) {
        elevate(baseline, CapabilityRisk::Destructive)
    } else {
        baseline
    }
}

/// Elevation on top of a baseline, one-directional (docs/06 § Dynamic risk
/// elevation): the higher class wins and nothing is ever lowered.
///
/// Two classes of equal rank cannot be combined into one, so a baseline that
/// is already sensitive, destructive or external would leave this unable to
/// say what a removal needs. `the_baseline_is_the_catalogs` keeps the baseline
/// below them.
fn elevate(baseline: CapabilityRisk, raised: CapabilityRisk) -> CapabilityRisk {
    if raised.rank() > baseline.rank() {
        raised
    } else {
        baseline
    }
}

fn approval_refused(
    status: StatusCode,
    code: &str,
    kind: CapabilityError,
    message: &str,
) -> PontisError {
    PontisError::new(
        status,
        code,
        kind,
        message,
        "Obtain a once-scoped approval for this invocation at the batch's class, then resend under the same invocation id.",
    )
}

/// Whether `approval` is consent for this invocation at `required`.
///
/// Absent or below the class is `approval_required` — ask for the right thing.
/// For another invocation, capability or class, or out of its window, it is a
/// refusal that re-opening a prompt would not fix (docs/06 § Approval evidence).
fn check_consent(
    required: CapabilityRisk,
    approval: Option<&ApprovalEvidence>,
    invocation: &InvocationId,
) -> Result<bool, PontisError> {
    if required.rank() <= CapabilityRisk::Write.rank() {
        return Ok(false);
    }
    let Some(approval) = approval else {
        return Err(approval_refused(
            StatusCode::PRECONDITION_REQUIRED,
            "APPROVAL_REQUIRED",
            CapabilityError::ApprovalRequired,
            "this batch removes work and carries no approval",
        )
        .with_details(serde_json::json!({ "required_risk": required })));
    };

    for (field, value) in [
        ("approval_id", &approval.approval_id),
        ("session_id", &approval.session_id),
        ("capability_id", &approval.capability_id),
    ] {
        if value.is_empty() || value.len() > InvocationId::MAX_LEN {
            return Err(PontisError::new(
                StatusCode::BAD_REQUEST,
                "INVALID_APPROVAL",
                CapabilityError::ValidationError,
                format!(
                    "approval.{field} must be 1 to {} bytes",
                    InvocationId::MAX_LEN
                ),
                "Forward the approval evidence exactly as the gateway issued it.",
            ));
        }
    }
    let instant = |field: &str, raw: &str| {
        chrono::DateTime::parse_from_rfc3339(raw)
            .map(|at| at.with_timezone(&chrono::Utc))
            .map_err(|_| {
                PontisError::new(
                    StatusCode::BAD_REQUEST,
                    "INVALID_APPROVAL",
                    CapabilityError::ValidationError,
                    format!("approval.{field} is not an RFC 3339 timestamp"),
                    "Forward the approval evidence exactly as the gateway issued it.",
                )
            })
    };
    let granted_at = instant("granted_at", &approval.granted_at)?;
    let expires_at = instant("expires_at", &approval.expires_at)?;

    if approval.capability_id != APPLY_OPERATIONS {
        return Err(approval_refused(
            StatusCode::FORBIDDEN,
            "APPROVAL_SCOPE_MISMATCH",
            CapabilityError::ApprovalScopeMismatch,
            "the approval is for another capability",
        ));
    }
    match &approval.scope {
        ApprovalScope::Once { invocation_id } if invocation_id == invocation.as_str() => {}
        ApprovalScope::Once { .. } => {
            return Err(approval_refused(
                StatusCode::FORBIDDEN,
                "APPROVAL_SCOPE_MISMATCH",
                CapabilityError::ApprovalScopeMismatch,
                "the approval is for another invocation",
            ))
        }
        ApprovalScope::Session { .. } => {
            return Err(approval_refused(
                StatusCode::FORBIDDEN,
                "APPROVAL_SCOPE_MISMATCH",
                CapabilityError::ApprovalScopeMismatch,
                "a removal takes a once-scoped approval; session grants are not accepted here",
            ))
        }
    }
    if approval.risk != required {
        return Err(if approval.risk.rank() < required.rank() {
            approval_refused(
                StatusCode::PRECONDITION_REQUIRED,
                "APPROVAL_REQUIRED",
                CapabilityError::ApprovalRequired,
                "the approval does not reach the class this batch needs",
            )
        } else {
            approval_refused(
                StatusCode::FORBIDDEN,
                "APPROVAL_SCOPE_MISMATCH",
                CapabilityError::ApprovalScopeMismatch,
                "the approval is for another risk class, and the classes are incomparable",
            )
        }
        .with_details(
            serde_json::json!({ "required_risk": required, "approved_risk": approval.risk }),
        ));
    }
    if expires_at <= chrono::Utc::now() || expires_at <= granted_at {
        return Err(approval_refused(
            StatusCode::FORBIDDEN,
            "APPROVAL_EXPIRED",
            CapabilityError::ApprovalExpired,
            "the approval's window has closed",
        ));
    }
    Ok(true)
}

fn invocation_id(request: &HttpRequest) -> Result<InvocationId, PontisError> {
    let raw = request
        .headers()
        .get(INVOCATION_HEADER)
        .and_then(|raw| raw.to_str().ok())
        .ok_or_else(|| {
            PontisError::new(
                StatusCode::BAD_REQUEST,
                "INVOCATION_ID_REQUIRED",
                CapabilityError::ValidationError,
                format!("a command carries its invocation id in {INVOCATION_HEADER}"),
                "Forward the runtime's invocation id; it is the idempotency key.",
            )
        })?;
    InvocationId::parse(raw).map_err(|error| {
        PontisError::new(
            StatusCode::BAD_REQUEST,
            "INVOCATION_ID_REQUIRED",
            CapabilityError::ValidationError,
            error.to_string(),
            "Forward the runtime's invocation id unchanged.",
        )
    })
}

/// How a duplicate that may not execute is answered.
fn refuse_duplicate(claim: Claim) -> PontisError {
    match claim {
        Claim::InFlight { claimed_at } => {
            let age = chrono::DateTime::parse_from_rfc3339(&claimed_at)
                .ok()
                .and_then(|at| {
                    (chrono::Utc::now() - at.with_timezone(&chrono::Utc))
                        .to_std()
                        .ok()
                });
            // An unreadable timestamp is treated as old: "unknown" is the
            // honest answer about a claim this process cannot date.
            let stale = age.is_none_or(|age| age >= IN_FLIGHT_STALE_AFTER);
            let details = serde_json::json!({ "claimed_at": claimed_at });
            if stale {
                PontisError::new(
                    StatusCode::CONFLICT,
                    "INVOCATION_OUTCOME_UNKNOWN",
                    CapabilityError::Conflict,
                    "this invocation was claimed and never finished; whether it applied is unknown",
                    "Do not retry it. Read the design's history for a version recorded after claimed_at, then decide under a new invocation id.",
                )
                .with_details(details)
            } else {
                PontisError::new(
                    StatusCode::CONFLICT,
                    "INVOCATION_IN_FLIGHT",
                    CapabilityError::Conflict,
                    "this invocation is still executing",
                    "Retry the same delivery shortly; it will be answered from the first one's result.",
                )
                .with_details(details)
                .retry_after(1)
            }
        }
        Claim::Conflict => PontisError::new(
            StatusCode::CONFLICT,
            "INVOCATION_REUSED",
            CapabilityError::InvocationAlreadyCompleted,
            "this invocation id was already used for a different request",
            "An invocation id names one request; a different batch needs a new invocation.",
        ),
        Claim::TargetDeleted { deleted_at } => PontisError::new(
            StatusCode::NOT_FOUND,
            "INVOCATION_TARGET_DELETED",
            CapabilityError::NotFound,
            "this invocation ran, and its design has since been deleted",
            "Nothing was executed again. The design is gone; list the project's designs.",
        )
        .with_details(serde_json::json!({ "deleted_at": deleted_at })),
        // Unreachable: the caller only hands over the claims it will not run.
        Claim::Execute | Claim::Replay(_) => PontisError::from(ApiError::internal(
            "INVALID_STATE",
            "a runnable claim was refused as a duplicate",
        )),
    }
}

enum Delivered {
    Executed(serde_json::Value),
    Replayed(serde_json::Value),
}

/// Claim, execute, record: one task, so a dropped handler cannot leave the
/// claim behind with nothing having run.
///
/// The batch is one transaction and the last thing that can fail, so an error
/// from it means nothing landed and the claim is released — a rejected batch
/// settled nothing, and the runtime may return under the same id once it has
/// rebased. A failure to record the result after the batch committed is only
/// logged: the effect happened and the caller is told so, and the claim then
/// answers `InFlight` until someone resolves it.
async fn claim_and_apply(
    state: web::Data<AppState>,
    account: AccountId,
    invocation: InvocationId,
    claim: InvocationRequest,
    id: DesignId,
    batch: PontisApplyRequest,
    actor: design_core::Actor,
) -> Result<Delivered, PontisError> {
    let ledger = SqliteInvocationLedger::new(state.pool.clone());
    match ledger.claim(account, &invocation, &claim).await? {
        Claim::Execute => {}
        Claim::Replay(result) => return Ok(Delivered::Replayed(result)),
        duplicate => return Err(refuse_duplicate(duplicate)),
    }

    let applied = application::designs::apply(
        &*state.designs,
        account,
        id,
        batch.base_version,
        &batch.operations,
        &actor,
    )
    .await;

    let record = match applied {
        Ok(record) => record,
        Err(error) => {
            if let Err(release) = ledger.release(account, &invocation).await {
                tracing::error!(error = %release, %invocation, "a rejected batch's claim could not be released");
            }
            return Err(error.into());
        }
    };

    telemetry::record_design(record.id, Some(record.version));
    let result = serde_json::to_value(PontisApplyResult {
        design_id: record.id,
        project_id: record.project_id,
        version: record.version,
        updated_at: record.updated_at,
    })
    .map_err(|error| {
        PontisError::from(ApiError::internal(
            "SERIALIZATION_FAILED",
            error.to_string(),
        ))
    })?;

    if let Err(error) = ledger.complete(account, &invocation, None, &result).await {
        tracing::error!(%error, %invocation, "a batch committed and its result was not recorded; the invocation will answer as in flight");
    }
    Ok(Delivered::Executed(result))
}

/// `design.applyOperations`: the write, and the same write elevated.
///
/// Order is the point. Everything that can refuse without touching anything —
/// the caller, the input, the class and its consent — runs first. Then the
/// claim, then the batch, then the result. There is no cancellation checkpoint
/// after the claim: the batch is one transaction, so the only places a cancel
/// could land are before it (where it would have to release the claim, and the
/// request has nothing yet to cancel) or between it and its record, which is
/// exactly "between its halves". This operation has no signal to receive yet,
/// and if it gets one it belongs before `claim_and_apply`.
#[post("/designs/{id}/operations")]
async fn apply_operations(
    state: web::Data<AppState>,
    request: HttpRequest,
    path: web::Path<String>,
    body: web::Json<PontisApplyRequest>,
) -> Result<HttpResponse, PontisError> {
    let who = represented(&request, &state)?;
    let id = design_id(&path)?;
    let invocation = invocation_id(&request)?;
    let body = body.into_inner();

    let required = required_risk(&body.operations);
    let approved = check_consent(required, body.approval.as_ref(), &invocation)?;

    // The fingerprint covers what was asked — the version and the batch — and
    // not the evidence that it was allowed, which a retry may carry afresh.
    let input = serde_json::json!({
        "base_version": body.base_version,
        "operations": body.operations,
    });
    let claim = InvocationRequest {
        represented_user: who.user,
        capability: APPLY_OPERATIONS.to_string(),
        design: Some(id),
        input,
    };
    let actor = actor_from(&request, &crate::routes::auth(&request, &state));

    let delivered = actix_web::rt::spawn(claim_and_apply(
        state.clone(),
        who.account,
        invocation,
        claim,
        id,
        body,
        actor,
    ))
    .await
    .map_err(|error| {
        PontisError::from(ApiError::internal("EXECUTION_FAILED", error.to_string()))
    })?;

    let (outcome, result) = match delivered {
        Ok(Delivered::Executed(result)) => ("executed", result),
        Ok(Delivered::Replayed(result)) => ("replayed", result),
        // Consent was given against the version the person saw. A design that
        // moved since is not what they agreed to, so it is not a rebase the
        // agent may do alone: docs/06 calls that `resource_state_changed`.
        Err(mut error) if approved && error.code == "VERSION_CONFLICT" => {
            error.capability_error = CapabilityError::ResourceStateChanged;
            error.hint = "The design changed after the removal was approved. Refetch, and ask for approval again for the batch you now mean.".to_string();
            return Err(error);
        }
        Err(error) => return Err(error),
    };

    Ok(HttpResponse::Ok()
        .insert_header((INVOCATION_OUTCOME_HEADER, outcome))
        .content_type("application/json")
        .body(bounded(APPLY_OPERATIONS, &result)?))
}

/// `assetRequest.create` at Boundary F: the request body, with the project in
/// the path. Unknown fields are refused, as for a batch.
#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct PontisAssetRequestBody {
    #[serde(default)]
    pub design_id: Option<DesignId>,
    pub purpose: String,
    pub description: String,
    #[serde(default)]
    pub suggested_prompt: Option<String>,
    #[serde(default)]
    pub requested_width: Option<u32>,
    #[serde(default)]
    pub requested_height: Option<u32>,
}

/// Claim, create, record, for an asset request — `claim_and_apply`'s shape,
/// for the same reason: a retry of a request that committed must answer the
/// first request rather than open a second one a person then has to resolve
/// twice.
async fn claim_and_request(
    state: web::Data<AppState>,
    account: AccountId,
    invocation: InvocationId,
    claim: InvocationRequest,
    new: application::asset_requests::NewAssetRequest,
) -> Result<Delivered, PontisError> {
    let ledger = SqliteInvocationLedger::new(state.pool.clone());
    match ledger.claim(account, &invocation, &claim).await? {
        Claim::Execute => {}
        Claim::Replay(result) => return Ok(Delivered::Replayed(result)),
        duplicate => return Err(refuse_duplicate(duplicate)),
    }

    let created: AssetRequest = match application::asset_requests::create(
        &*state.projects,
        &*state.designs,
        &*state.asset_requests,
        account,
        new,
    )
    .await
    {
        Ok(created) => created,
        // One insert, and the last thing that can fail: nothing landed, so the
        // claim is released and the same id may come back corrected.
        Err(error) => {
            if let Err(release) = ledger.release(account, &invocation).await {
                tracing::error!(error = %release, %invocation, "a refused asset request's claim could not be released");
            }
            return Err(error.into());
        }
    };

    let result = serde_json::to_value(&created).map_err(|error| {
        PontisError::from(ApiError::internal(
            "SERIALIZATION_FAILED",
            error.to_string(),
        ))
    })?;
    if let Err(error) = ledger.complete(account, &invocation, None, &result).await {
        tracing::error!(%error, %invocation, "an asset request was created and its result was not recorded; the invocation will answer as in flight");
    }
    Ok(Delivered::Executed(result))
}

/// `assetRequest.create`: the write that records an image the design needs.
///
/// Ledgered like `design.applyOperations`. The claim names no design even
/// when the request is pinned to one: its target is the project, and a
/// replay after that design was deleted still describes a request that was
/// made, rather than one the ledger should call deleted.
#[post("/projects/{id}/asset-requests")]
async fn create_asset_request(
    state: web::Data<AppState>,
    request: HttpRequest,
    path: web::Path<String>,
    body: web::Json<PontisAssetRequestBody>,
) -> Result<HttpResponse, PontisError> {
    let who = represented(&request, &state)?;
    let project = ProjectId(parse_id(&path, "project id")?);
    let invocation = invocation_id(&request)?;
    let body = body.into_inner();

    let mut input = serde_json::to_value(&body).map_err(|error| {
        PontisError::from(ApiError::internal(
            "SERIALIZATION_FAILED",
            error.to_string(),
        ))
    })?;
    input["project_id"] = serde_json::json!(project);
    let claim = InvocationRequest {
        represented_user: who.user,
        capability: ASSET_REQUEST_CREATE.to_string(),
        design: None,
        input,
    };
    let new = application::asset_requests::NewAssetRequest {
        project_id: project,
        design_id: body.design_id,
        purpose: body.purpose,
        description: body.description,
        suggested_prompt: body.suggested_prompt,
        requested_width: body.requested_width,
        requested_height: body.requested_height,
    };

    let delivered = actix_web::rt::spawn(claim_and_request(
        state.clone(),
        who.account,
        invocation,
        claim,
        new,
    ))
    .await
    .map_err(|error| {
        PontisError::from(ApiError::internal("EXECUTION_FAILED", error.to_string()))
    })??;

    let (outcome, result) = match delivered {
        Delivered::Executed(result) => ("executed", result),
        Delivered::Replayed(result) => ("replayed", result),
    };
    Ok(HttpResponse::Ok()
        .insert_header((INVOCATION_OUTCOME_HEADER, outcome))
        .content_type("application/json")
        .body(bounded(ASSET_REQUEST_CREATE, &result)?))
}

/// One count per account of validations running now.
#[derive(Debug, Default)]
pub struct ValidationGate {
    running: Mutex<HashMap<AccountId, usize>>,
}

impl ValidationGate {
    /// A pass while this account is under [`MAX_CONCURRENT_VALIDATIONS`];
    /// `None` once it is not. Dropping the pass frees the slot.
    pub fn enter(self: &Arc<Self>, account: AccountId) -> Option<ValidationPass> {
        let mut running = self
            .running
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let count = running.entry(account).or_insert(0);
        if *count >= MAX_CONCURRENT_VALIDATIONS {
            return None;
        }
        *count += 1;
        Some(ValidationPass {
            gate: Arc::clone(self),
            account,
        })
    }
}

#[derive(Debug)]
pub struct ValidationPass {
    gate: Arc<ValidationGate>,
    account: AccountId,
}

impl Drop for ValidationPass {
    fn drop(&mut self) {
        let mut running = self
            .gate
            .running
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(count) = running.get_mut(&self.account) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                running.remove(&self.account);
            }
        }
    }
}

/// Request bodies are bounded here, and an unreadable one answers in this
/// module's envelope like every other refusal.
fn json_config() -> web::JsonConfig {
    web::JsonConfig::default()
        .limit(MAX_REQUEST_BYTES)
        .error_handler(|error, _| {
            PontisError::new(
                StatusCode::BAD_REQUEST,
                "INVALID_BODY",
                CapabilityError::ValidationError,
                format!("the request body could not be read: {error}"),
                "Check the capability's input schema, and that the body is under its size bound.",
            )
            .into()
        })
}

async fn no_such_operation(request: HttpRequest) -> Result<HttpResponse, PontisError> {
    Err(PontisError::new(
        StatusCode::NOT_FOUND,
        "NOT_FOUND",
        CapabilityError::CapabilityNotFound,
        format!(
            "no Boundary F operation matches {} {}",
            request.method(),
            request.path()
        ),
        "Boundary F has fixed operations; there is no generic route behind it.",
    ))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/pontis")
            .app_data(json_config())
            .service(list_projects)
            .service(list_designs)
            .service(get_design)
            .service(validate)
            .service(apply_operations)
            .service(guide)
            .service(get_brand)
            .service(create_asset_request)
            .default_service(web::route().to(no_such_operation)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use design_core::LayerId;

    fn removal() -> DesignOperation {
        serde_json::from_value(serde_json::json!({
            "op": "remove_layer",
            "layer_id": LayerId::new(),
        }))
        .expect("a remove_layer operation")
    }

    #[test]
    fn a_batch_is_elevated_by_what_it_contains_not_by_its_length() {
        assert_eq!(required_risk(&[]), CapabilityRisk::Write);
        assert_eq!(required_risk(&[removal()]), CapabilityRisk::Destructive);
    }

    #[test]
    fn every_route_is_a_declared_capability() {
        for capability in SERVED {
            let operation = agent_catalog::operation(capability)
                .unwrap_or_else(|| panic!("the catalog does not declare {capability}"));
            assert!(
                operation.doors.on_pontis(),
                "{capability} is served on Boundary F but not declared on the Pontis door"
            );
        }
    }

    #[test]
    fn the_bounds_are_the_ones_the_sidecar_declares() {
        // What the sidecar hands Pontis is `pontis_catalog()`, so compare the
        // route's source against that rendering rather than the raw registry.
        let declared = agent_catalog::pontis_catalog();
        for capability in SERVED {
            let spec = declared
                .iter()
                .find(|spec| spec.id == capability)
                .unwrap_or_else(|| panic!("the sidecar declares no {capability}"));
            assert_eq!(bounds(capability), spec.bounds, "{capability}");
        }
        assert_eq!(
            max_validate_timeout().as_millis(),
            u128::from(bounds(DESIGN_VALIDATE).timeout_ms)
        );
    }

    #[test]
    fn the_baseline_is_the_catalogs() {
        let declared = agent_catalog::operation(APPLY_OPERATIONS)
            .expect("the catalog declares design.applyOperations");
        assert_eq!(required_risk(&[]), declared.risk);
        // Elevation is only well defined above a baseline it can raise.
        assert!(declared.risk.rank() < CapabilityRisk::Destructive.rank());
        assert_eq!(
            elevate(declared.risk, CapabilityRisk::Destructive),
            CapabilityRisk::Destructive
        );
        assert_eq!(
            elevate(CapabilityRisk::Destructive, CapabilityRisk::Write),
            CapabilityRisk::Destructive,
            "elevation never lowers"
        );
    }

    #[test]
    fn the_risk_order_is_partial() {
        assert!(CapabilityRisk::Write.rank() < CapabilityRisk::Destructive.rank());
        // Equal rank is not coverage: consent to one is not consent to another.
        assert_eq!(
            CapabilityRisk::Sensitive.rank(),
            CapabilityRisk::Destructive.rank()
        );
    }

    #[test]
    fn the_gate_frees_a_slot_when_a_pass_drops() {
        let gate = Arc::new(ValidationGate::default());
        let account = AccountId::new();
        let first = gate.enter(account).expect("first");
        let _second = gate.enter(account).expect("second");
        assert!(gate.enter(account).is_none());
        assert!(
            gate.enter(AccountId::new()).is_some(),
            "per account, not global"
        );
        drop(first);
        assert!(gate.enter(account).is_some());
    }

    #[test]
    fn hidden_failures_carry_no_detail() {
        let error = PontisError::from(PersistenceError::Storage(
            "database is locked at /srv".into(),
        ));
        assert_eq!(error.capability_error(), CapabilityError::Unavailable);
        let body = actix_web::body::to_bytes(error.error_response().into_body());
        let body = futures_util::FutureExt::now_or_never(body)
            .expect("an in-memory body")
            .expect("bytes");
        let text = String::from_utf8(body.to_vec()).expect("utf-8");
        assert!(!text.contains("/srv"), "{text}");
        assert!(
            text.contains("\"capability_error\":\"unavailable\""),
            "{text}"
        );
    }
}
