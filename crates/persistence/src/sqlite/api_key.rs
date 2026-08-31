use async_trait::async_trait;
use chrono::{Duration, SecondsFormat, Utc};
use design_core::{AccountId, ApiKey, ApiKeyId, AuthUserId, CreatedApiKey};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::error::{PersistenceError, Result};
use crate::traits::ApiKeyRepository;

use super::parse_uuid;

pub const PAT_PREFIX: &str = "forma_pat_";
const SECRET_HEX_LEN: usize = 64;
const DISPLAY_PREFIX_LEN: usize = PAT_PREFIX.len() + 8;
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

fn candidate_prefix(presented: &str) -> Option<&str> {
    let secret = presented.strip_prefix(PAT_PREFIX)?;
    if secret.len() != SECRET_HEX_LEN || !secret.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }

    presented.get(..DISPLAY_PREFIX_LEN)
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
        // UUID v4 contributes 122 random bits after its fixed version/variant
        // bits. Two draw 244 bits through getrandom without another dependency.
        let token = format!(
            "{PAT_PREFIX}{}{}",
            Uuid::new_v4().simple(),
            Uuid::new_v4().simple()
        );
        let token_prefix = token
            .get(..DISPLAY_PREFIX_LEN)
            .ok_or_else(|| {
                PersistenceError::Storage("generated PAT was unexpectedly short".into())
            })?
            .to_string();
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
        let token_hash = blake3::hash(token.as_bytes()).to_hex().to_string();

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
        let Some(prefix) = candidate_prefix(presented) else {
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
            let raw_hash: String = row.get("token_hash");
            let stored_hash = blake3::Hash::from_hex(&raw_hash).map_err(|error| {
                PersistenceError::Storage(format!("stored API key hash is invalid: {error}"))
            })?;

            // A prefix is only an index hint. The full BLAKE3 Hash comparison is
            // constant-time, so a wrong secret cannot be found byte by byte.
            if stored_hash == presented_hash {
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
