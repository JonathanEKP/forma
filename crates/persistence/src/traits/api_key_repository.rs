use async_trait::async_trait;
use design_core::{AccountId, ApiKey, ApiKeyId, AuthUserId, CreatedApiKey};

use crate::error::Result;

/// Personal credentials owned by one account (plan §112.1).
///
/// Kept apart from `AccountRepository`: admission decides who owns a
/// workspace, while this lifecycle decides which independent credentials can
/// act inside one. Revoking a PAT must never touch membership or another key.
#[async_trait]
pub trait ApiKeyRepository: Send + Sync {
    /// Mints a high-entropy token and persists only its safe prefix and hash.
    async fn create(
        &self,
        account: AccountId,
        created_by: Option<AuthUserId>,
        name: &str,
    ) -> Result<CreatedApiKey>;

    async fn list(&self, account: AccountId) -> Result<Vec<ApiKey>>;

    /// Resolves the account from the credential itself.
    ///
    /// Revoked keys are returned too so the HTTP boundary can use the stable
    /// `API_KEY_REVOKED` code. They never become an `AuthContext`.
    async fn authenticate(&self, presented: &str) -> Result<Option<ApiKey>>;

    /// Coalesces last-use writes; see the SQLite adapter for the time window.
    async fn touch_last_used(&self, account: AccountId, id: ApiKeyId) -> Result<()>;

    /// Revocation is account-scoped and idempotent for a key this account owns.
    async fn revoke(&self, account: AccountId, id: ApiKeyId) -> Result<ApiKey>;
}
