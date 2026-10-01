use async_trait::async_trait;
use chrono::{Duration, SecondsFormat, Utc};
use design_core::{AccountId, ApiKey, ApiKeyId, AuthUserId, CreatedApiKey};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::error::{PersistenceError, Result};
use crate::traits::ApiKeyRepository;

use super::parse_uuid;
use super::secret;

pub const PAT_PREFIX: &str = "forma_pat_";
const LAST_USED_WRITE_INTERVAL_MINUTES: i64 = 5;

pub struct SqliteApiKeyRepository {
    pool: SqlitePool,
}

impl SqliteApiKeyRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

const COLUMNS: &str =
    "id, account_id, created_by_user_id, name, token_prefix, created_at, last_used_at, revoked_at";

fn api_key_from(row: &sqlx::sqlite::SqliteRow) -> Result<ApiKey> {
    let created_by = parse_uuid(row.get::<String, _>("created_by_user_id"))?;
    Ok(ApiKey {
        id: ApiKeyId(parse_uuid(row.get::<String, _>("id"))?),
        account_id: AccountId(parse_uuid(row.get::<String, _>("account_id"))?),
        // Local mode has no external subject. The schema predates that use and
        // keeps this column NOT NULL, so nil is its on-disk sentinel only.
        created_by_user_id: (!created_by.is_nil()).then_some(AuthUserId(created_by)),
        name: row.get("name"),
        token_prefix: row.get("token_prefix"),
        created_at: row.get("created_at"),
        last_used_at: row.get("last_used_at"),
        revoked_at: row.get("revoked_at"),
    })
}

fn timestamp_now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

#[async_trait]
impl ApiKeyRepository for SqliteApiKeyRepository {
    async fn create(
        &self,
        account: AccountId,
        created_by: Option<AuthUserId>,
        name: &str,
    ) -> Result<CreatedApiKey> {
        let (token, token_prefix) = secret::mint(PAT_PREFIX)?;
        let key = ApiKey {
            id: ApiKeyId::new(),
            account_id: account,
            created_by_user_id: created_by,
            name: name.trim().to_string(),
            token_prefix,
            created_at: timestamp_now(),
            last_used_at: None,
            revoked_at: None,
        };
        let token_hash = secret::hash_hex(&token);

        sqlx::query(
            "INSERT INTO api_keys
             (id, account_id, created_by_user_id, name, token_prefix, token_hash, created_at,
              last_used_at, revoked_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, NULL, NULL)",
        )
        .bind(key.id.to_string())
        .bind(key.account_id.to_string())
        .bind(
            key.created_by_user_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| Uuid::nil().to_string()),
        )
        .bind(&key.name)
        .bind(&key.token_prefix)
        .bind(token_hash)
        .bind(&key.created_at)
        .execute(&self.pool)
        .await?;

        Ok(CreatedApiKey { key, token })
    }

    async fn list(&self, account: AccountId) -> Result<Vec<ApiKey>> {
        let rows = sqlx::query(&format!(
            "SELECT {COLUMNS} FROM api_keys WHERE account_id = ? ORDER BY created_at DESC"
        ))
        .bind(account.to_string())
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(api_key_from).collect()
    }

    async fn authenticate(&self, presented: &str) -> Result<Option<ApiKey>> {
        let Some(prefix) = secret::candidate_prefix(presented, PAT_PREFIX) else {
            return Ok(None);
        };
        let presented_hash = blake3::hash(presented.as_bytes());
        let rows = sqlx::query(&format!(
            "SELECT {COLUMNS}, token_hash FROM api_keys WHERE token_prefix = ?"
        ))
        .bind(prefix)
        .fetch_all(&self.pool)
        .await?;

        for row in rows {
            if secret::stored_matches(row.get("token_hash"), &presented_hash)? {
                return api_key_from(&row).map(Some);
            }
        }

        Ok(None)
    }

    async fn touch_last_used(&self, account: AccountId, id: ApiKeyId) -> Result<()> {
        let instant = Utc::now();
        let used_at = instant.to_rfc3339_opts(SecondsFormat::Millis, true);
        let stale_before = (instant - Duration::minutes(LAST_USED_WRITE_INTERVAL_MINUTES))
            .to_rfc3339_opts(SecondsFormat::Millis, true);

        // The condition is inside the UPDATE so simultaneous requests may
        // contend once but cannot turn every authenticated read into a write.
        sqlx::query(
            "UPDATE api_keys SET last_used_at = ?
             WHERE id = ? AND account_id = ? AND revoked_at IS NULL
               AND (last_used_at IS NULL OR last_used_at < ?)",
        )
        .bind(used_at)
        .bind(id.to_string())
        .bind(account.to_string())
        .bind(stale_before)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn revoke(&self, account: AccountId, id: ApiKeyId) -> Result<ApiKey> {
        let revoked = sqlx::query(
            "UPDATE api_keys SET revoked_at = COALESCE(revoked_at, ?)
             WHERE id = ? AND account_id = ?",
        )
        .bind(timestamp_now())
        .bind(id.to_string())
        .bind(account.to_string())
        .execute(&self.pool)
        .await?;

        if revoked.rows_affected() == 0 {
            return Err(PersistenceError::NotFound(format!("API key {id}")));
        }

        let row = sqlx::query(&format!(
            "SELECT {COLUMNS} FROM api_keys WHERE id = ? AND account_id = ?"
        ))
        .bind(id.to_string())
        .bind(account.to_string())
        .fetch_one(&self.pool)
        .await?;

        api_key_from(&row)
    }
}
