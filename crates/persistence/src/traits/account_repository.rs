use async_trait::async_trait;
use design_core::{Account, AccountId, AccountInvite, AccountMembership, AccountRole, AuthUserId};

use crate::error::Result;

/// The tenant boundary (plan section 107).
///
/// Nothing here depends on an identity provider. `AuthUserId` is whatever
/// subject the provider gives, and in local mode nobody ever supplies one — the
/// bootstrap account is resolved directly. That is what lets the whole account
/// boundary be built and tested before any provider exists (plan 117.1).
#[async_trait]
pub trait AccountRepository: Send + Sync {
    /// The account that existed before anyone signed in.
    ///
    /// Always present: the migration creates it, and the schema keeps it single.
    async fn bootstrap(&self) -> Result<Account>;

    async fn get(&self, id: AccountId) -> Result<Account>;
    async fn create(&self, name: &str) -> Result<Account>;

    /// The account a user belongs to, if any.
    ///
    /// `None` is not an error: a valid identity with no membership is the
    /// `ACCOUNT_REQUIRED` case of plan 115, and the caller decides what to say.
    async fn membership_of(&self, user: AuthUserId) -> Result<Option<AccountMembership>>;

    /// Attaches a user to an account, or updates the email of an existing
    /// membership so an account can be rebound to a fresh subject (plan 113.3).
    async fn grant(
        &self,
        account: AccountId,
        user: AuthUserId,
        email: &str,
        role: AccountRole,
    ) -> Result<AccountMembership>;

    /// Claims the bootstrap account exactly once.
    ///
    /// Returns `None` when it has already been claimed. That is the whole
    /// mechanism of plan 113.1: a second attempt must not succeed, and the
    /// decision cannot live in a caller that might forget to check first.
    async fn claim_bootstrap(
        &self,
        user: AuthUserId,
        email: &str,
    ) -> Result<Option<AccountMembership>>;

    async fn invite(&self, email: &str) -> Result<AccountInvite>;
    async fn list_invites(&self) -> Result<Vec<AccountInvite>>;

    /// An unclaimed invite for this address, if one exists.
    async fn open_invite(&self, email: &str) -> Result<Option<AccountInvite>>;

    /// Marks an invite used. Returns false when it was already claimed.
    async fn claim_invite(&self, email: &str, user: AuthUserId) -> Result<bool>;
}
