use async_trait::async_trait;

use crate::error::Result;

/// Bytes storage, behind a key the database records.
///
/// The local filesystem in V1; Supabase Storage or S3 later, without the domain
/// or the application layer changing (plan section 6.2).
#[async_trait]
pub trait ObjectStore: Send + Sync {
    async fn put(&self, key: &str, bytes: &[u8]) -> Result<()>;
    async fn get(&self, key: &str) -> Result<Vec<u8>>;
    async fn exists(&self, key: &str) -> Result<bool>;
    async fn delete(&self, key: &str) -> Result<()>;
}
