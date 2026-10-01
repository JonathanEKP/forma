use async_trait::async_trait;
use chrono::{Duration, SecondsFormat, Utc};
use design_core::{CreatedServiceCredential, ServiceCredential, ServiceCredentialId};
use sqlx::{Row, SqlitePool};

use crate::error::{PersistenceError, Result};
use crate::traits::ServiceCredentialRepository;

use super::parse_uuid;
use super::secret;

/// Its own prefix, never `forma_pat_`.
///
/// `require_bearer` tells credential classes apart by prefix alone, so this
/// string *is* the class. Reusing the PAT prefix would not have made the
/// sidecar a PAT; it would have made every PAT already minted a credential that
/// reaches any account (Pontis plan §10(b) D1).
pub const SERVICE_PREFIX: &str = "forma_svc_";
const LAST_USED_WRITE_INTERVAL_MINUTES: i64 = 5;

pub struct SqliteServiceCredentialRepository {
    pool: SqlitePool,
}

impl SqliteServiceCredentialRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

const COLUMNS: &str = "id, name, token_prefix, created_at, last_used_at, revoked_at";

fn credential_from(row: &sqlx::sqlite::SqliteRow) -> Result<ServiceCredential> {
    Ok(ServiceCredential {
        id: ServiceCredentialId(parse_uuid(row.get::<String, _>("id"))?),
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
impl ServiceCredentialRepository for SqliteServiceCredentialRepository {
    async fn create(&self, name: &str) -> Result<CreatedServiceCredential> {
        let (token, token_prefix) = secret::mint(SERVICE_PREFIX)?;
        let credential = ServiceCredential {
            id: ServiceCredentialId::new(),
            name: name.trim().to_string(),
            token_prefix,
            created_at: timestamp_now(),
            last_used_at: None,
            revoked_at: None,
        };

        sqlx::query(
            "INSERT INTO service_credentials
             (id, name, token_prefix, token_hash, created_at, last_used_at, revoked_at)
             VALUES (?, ?, ?, ?, ?, NULL, NULL)",
        )
        .bind(credential.id.to_string())
        .bind(&credential.name)
        .bind(&credential.token_prefix)
        .bind(secret::hash_hex(&token))
        .bind(&credential.created_at)
        .execute(&self.pool)
        .await?;

        Ok(CreatedServiceCredential { credential, token })
    }

    async fn list(&self) -> Result<Vec<ServiceCredential>> {
        let rows = sqlx::query(&format!(
            "SELECT {COLUMNS} FROM service_credentials ORDER BY created_at DESC"
        ))
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(credential_from).collect()
    }

    async fn authenticate(&self, presented: &str) -> Result<Option<ServiceCredential>> {
        let Some(prefix) = secret::candidate_prefix(presented, SERVICE_PREFIX) else {
            return Ok(None);
        };
        let presented_hash = blake3::hash(presented.as_bytes());
        let rows = sqlx::query(&format!(
            "SELECT {COLUMNS}, token_hash FROM service_credentials WHERE token_prefix = ?"
        ))
        .bind(prefix)
        .fetch_all(&self.pool)
        .await?;

        for row in rows {
            if secret::stored_matches(row.get("token_hash"), &presented_hash)? {
                return credential_from(&row).map(Some);
            }
        }

        Ok(None)
    }

    async fn touch_last_used(&self, id: ServiceCredentialId) -> Result<()> {
        let instant = Utc::now();
        let used_at = instant.to_rfc3339_opts(SecondsFormat::Millis, true);
        let stale_before = (instant - Duration::minutes(LAST_USED_WRITE_INTERVAL_MINUTES))
            .to_rfc3339_opts(SecondsFormat::Millis, true);

        sqlx::query(
            "UPDATE service_credentials SET last_used_at = ?
             WHERE id = ? AND revoked_at IS NULL
               AND (last_used_at IS NULL OR last_used_at < ?)",
        )
        .bind(used_at)
        .bind(id.to_string())
        .bind(stale_before)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn revoke(&self, id: ServiceCredentialId) -> Result<ServiceCredential> {
        let revoked = sqlx::query(
            "UPDATE service_credentials SET revoked_at = COALESCE(revoked_at, ?) WHERE id = ?",
        )
        .bind(timestamp_now())
        .bind(id.to_string())
        .execute(&self.pool)
        .await?;

        if revoked.rows_affected() == 0 {
            return Err(PersistenceError::NotFound(format!(
                "service credential {id}"
            )));
        }

        let row = sqlx::query(&format!(
            "SELECT {COLUMNS} FROM service_credentials WHERE id = ?"
        ))
        .bind(id.to_string())
        .fetch_one(&self.pool)
        .await?;

        credential_from(&row)
    }
}
