use async_trait::async_trait;
use design_core::{AccountId, AuthUserId, DesignId, InvocationId};

use crate::error::Result;

/// What a delivery asked for. Everything in it goes into the fingerprint, so a
/// caller cannot record a request and forget to have one of its parts compared.
#[derive(Debug, Clone, PartialEq)]
pub struct InvocationRequest {
    /// The person the sidecar acts for, not the sidecar itself.
    pub represented_user: AuthUserId,
    /// The capability id, e.g. `design.apply_operations`.
    pub capability: String,
    /// The design the invocation targets, when it names one. One it produces
    /// instead (`create_design`) is attached on `complete`.
    pub design: Option<DesignId>,
    pub input: serde_json::Value,
}

/// The answer to "may I execute this delivery?".
///
/// `#[must_use]` and exhaustive on purpose: every arm except `Execute` means
/// the domain binding must not run.
#[must_use]
#[derive(Debug, Clone, PartialEq)]
pub enum Claim {
    /// First delivery. The caller now holds the claim and must end it with
    /// `complete` or `release`.
    Execute,
    /// A duplicate of an invocation that completed: answer with this, the
    /// stored result, and do nothing else.
    Replay(serde_json::Value),
    /// A duplicate of an invocation that has been claimed but not completed —
    /// still running, or interrupted by a crash with its outcome unknown. Not
    /// executed again: the ledger promises at most once, and a claim whose
    /// outcome is unknown is exactly the case where running twice would hurt.
    InFlight { claimed_at: String },
    /// A duplicate whose fingerprint differs from the first delivery's: same
    /// key, different request. Refused, never answered with the first result.
    Conflict,
    /// A duplicate of an invocation whose design has since been deleted. The
    /// result was erased with the design, and it is not executed again.
    TargetDeleted { deleted_at: String },
}

/// One row, as stored. For reading back what was written; callers deciding
/// whether to execute use `claim`.
#[derive(Debug, Clone, PartialEq)]
pub struct InvocationRecord {
    pub account: AccountId,
    pub invocation: InvocationId,
    pub capability: String,
    pub represented_user: AuthUserId,
    pub fingerprint: String,
    pub design: Option<DesignId>,
    pub result: Option<serde_json::Value>,
    pub claimed_at: String,
    pub completed_at: Option<String>,
    pub target_deleted_at: Option<String>,
}

/// The host-side invocation ledger of Boundary F (Pontis plan §10(b) D6).
///
/// Durable, and scoped to the account like every other repository: a row is
/// keyed on `(account, invocation)`, so one account can neither read another's
/// result nor learn that another used an id.
///
/// The protocol is claim → execute → complete, or claim → fail → release:
///
/// - only `command` invocations belong here; a resource has no side effect to
///   duplicate (pontis docs/06 § Idempotency);
/// - an answer that settled nothing (`approval_required`, a rejected batch) is
///   released rather than recorded, so the runtime can return under the same
///   id once it has consent, as the reference host does;
/// - the claim is written before execution and the result after, so a crash in
///   between leaves a claim answered `InFlight` forever. That is the price of
///   at most once, chosen over at least once deliberately. Closing that window
///   needs the result written in the domain mutation's own transaction, which
///   is a matter for whoever wires Boundary F, not for this table.
#[async_trait]
pub trait InvocationLedger: Send + Sync {
    /// Claims `invocation` for this request, or says why it must not run.
    ///
    /// A first delivery naming a design the account does not own is refused
    /// with `DESIGN_NOT_FOUND`, and nothing is written.
    async fn claim(
        &self,
        account: AccountId,
        invocation: &InvocationId,
        request: &InvocationRequest,
    ) -> Result<Claim>;

    /// Records the result of a claimed invocation, and the design it produced
    /// when it named none up front.
    ///
    /// Refuses with `INVALID_STATE` unless the row is a live claim: a result is
    /// written once, and never over another one.
    async fn complete(
        &self,
        account: AccountId,
        invocation: &InvocationId,
        design: Option<DesignId>,
        result: &serde_json::Value,
    ) -> Result<()>;

    /// Gives up a claim whose execution settled nothing, so the same id may be
    /// claimed again. Refuses with `INVALID_STATE` for a completed invocation:
    /// forgetting a result that was already acted on is how a retry would run
    /// twice.
    async fn release(&self, account: AccountId, invocation: &InvocationId) -> Result<()>;

    async fn get(
        &self,
        account: AccountId,
        invocation: &InvocationId,
    ) -> Result<Option<InvocationRecord>>;
}
