use async_trait::async_trait;
use design_core::{CreatedServiceCredential, ServiceCredential, ServiceCredentialId};

use crate::error::Result;

/// Credentials for the Pontis integration sidecar (Pontis plan §10(b) D1).
///
/// Kept apart from `ApiKeyRepository` on purpose. A PAT is scoped to one
/// account and names the person who minted it; a service credential is scoped
/// to the deployment and names nobody, so none of these methods take an
/// account. Who a request acts for is not stored here at all.
#[async_trait]
pub trait ServiceCredentialRepository: Send + Sync {
    /// Mints a high-entropy token and persists only its safe prefix and hash.
    async fn create(&self, name: &str) -> Result<CreatedServiceCredential>;

    async fn list(&self) -> Result<Vec<ServiceCredential>>;

    /// Resolves a presented token to its credential.
    ///
    /// Revoked credentials are returned too, so the HTTP boundary can answer
    /// with a stable code. They never become an `AuthContext`.
    async fn authenticate(&self, presented: &str) -> Result<Option<ServiceCredential>>;

    /// Coalesces last-use writes, like `ApiKeyRepository::touch_last_used`.
    async fn touch_last_used(&self, id: ServiceCredentialId) -> Result<()>;

    /// Idempotent: revoking twice keeps the first date.
    async fn revoke(&self, id: ServiceCredentialId) -> Result<ServiceCredential>;
}
