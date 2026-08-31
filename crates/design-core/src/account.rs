//! The tenant boundary (plan section 107).
//!
//! An Account owns Projects, and everything else is owned through one: a design,
//! asset, template or brand kit derives its account from its project rather than
//! carrying a copy. A second copy is a second thing that can disagree, and
//! disagreement about who owns a row is the failure this boundary exists to
//! prevent.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::{AccountId, ApiKeyId, AuthUserId};

/// The fixed id of the account that existed before anyone signed in.
///
/// Created by the accounts migration, which backfills every project that existed
/// before accounts did. On a fresh local database it adopts nothing and is
/// simply the account everything belongs to (plan 117.8); on the deployed one it
/// is the legacy account, claimable exactly once (plan 113.1).
///
/// Fixed rather than random so the migration is deterministic and local mode can
/// resolve it without a lookup by flag. It is not a secret, and the schema keeps
/// it single with a partial unique index.
pub const BOOTSTRAP_ACCOUNT: &str = "00000000-0000-4000-8000-000000000001";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Account {
    pub id: AccountId,
    pub name: String,
    /// Whether this is the pre-identity account. Exactly one row may set it.
    pub bootstrap: bool,
    /// When a signed-in user claimed it. `None` means unclaimed, which is what
    /// stops the legacy workspace falling to whoever signs up first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claimed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Account {
    /// Whether this account can still be claimed with the bootstrap credential.
    pub fn is_claimable(&self) -> bool {
        self.bootstrap && self.claimed_at.is_none()
    }
}

/// One variant, on purpose.
///
/// Plan 117.7 rules out RBAC beyond the owner model for this phase. The column
/// exists so team roles can arrive later without changing how a Project is
/// owned; it does not pretend to express anything yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum AccountRole {
    Owner,
}

impl AccountRole {
    pub fn as_str(self) -> &'static str {
        match self {
            AccountRole::Owner => "owner",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "owner" => Some(AccountRole::Owner),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AccountMembership {
    pub account_id: AccountId,
    /// Authoritative. Everything about who someone is resolves through this.
    pub user_id: AuthUserId,
    /// Descriptive only, and never used to authorize.
    ///
    /// It exists so a person reading the table after the identity provider is
    /// gone can tell whose row is whose, and so an account can be rebound to a
    /// fresh subject from a new provider project (plan 113.3). Authorizing on it
    /// would make a change of email address a change of identity.
    pub email: String,
    pub role: AccountRole,
    pub created_at: String,
}

/// Signing in is not the gate; owning an account is (plan 107.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AccountInvite {
    pub email: String,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claimed_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claimed_by_user_id: Option<AuthUserId>,
}

impl AccountInvite {
    pub fn is_open(&self) -> bool {
        self.claimed_at.is_none()
    }
}

/// Safe metadata for one personal access token (plan §112.1).
///
/// The hash is deliberately absent. The prefix is enough for a person to
/// recognise a key in the Agents page and is never used as authentication on
/// its own.
///
/// `created_by_user_id` is absent for the trusted-local process, which has no
/// external subject.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ApiKey {
    pub id: ApiKeyId,
    pub account_id: AccountId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by_user_id: Option<AuthUserId>,
    pub name: String,
    pub token_prefix: String,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<String>,
}

impl ApiKey {
    pub fn is_revoked(&self) -> bool {
        self.revoked_at.is_some()
    }
}

/// The only response that contains a complete PAT.
///
/// Deliberately not `Debug` or `Clone`: the server must serialize this once
/// to the caller, and diagnostics must not acquire an easy path to the secret.
#[derive(Serialize, TS)]
#[ts(export)]
pub struct CreatedApiKey {
    pub key: ApiKey,
    pub token: String,
}

/// How a caller proved who they are (plan section 108).
///
/// `Local` is not a weaker version of the others: it is the trusted-local model
/// V1 was built on, where the security boundary is the process that launched
/// Forma. It carries no user because there is none to carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum AuthKind {
    Local,
    BrowserUser,
    McpOAuth,
    McpPat,
}

/// The resolved identity behind one request.
///
/// `account_id` is a security boundary, not an input field: it is resolved
/// here, from a credential, and never read from a request body, a query
/// parameter, an MCP tool schema or an agent prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthContext {
    pub account_id: AccountId,
    pub user_id: Option<AuthUserId>,
    pub kind: AuthKind,
    /// The OAuth client that presented the token, when one did.
    pub client_id: Option<String>,
    pub api_key_id: Option<crate::ids::ApiKeyId>,
}

impl AuthContext {
    /// The local case: no identity, one account, and the process boundary.
    pub fn local(account_id: AccountId) -> Self {
        Self {
            account_id,
            user_id: None,
            kind: AuthKind::Local,
            client_id: None,
            api_key_id: None,
        }
    }

    /// Whether the caller's claim about who they are can be taken at face value.
    ///
    /// True only for `Local`, where the caller is the process that started
    /// Forma and the `x-forma-actor` header is as trustworthy as anything else
    /// it says. Once a request is authenticated *as someone*, the actor is
    /// derived from that identity instead, so history cannot be forged
    /// (plan 117.2).
    pub fn trusts_caller_supplied_actor(&self) -> bool {
        matches!(self.kind, AuthKind::Local)
    }
}
