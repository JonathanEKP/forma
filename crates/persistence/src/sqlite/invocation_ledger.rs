use async_trait::async_trait;
use design_core::{AccountId, AuthUserId, DesignId, InvocationId};
use sqlx::{Row, SqlitePool};

use crate::error::{PersistenceError, Result};
use crate::traits::{Claim, InvocationLedger, InvocationRecord, InvocationRequest};

use super::{now, owned_design, parse_uuid};

pub struct SqliteInvocationLedger {
    pool: SqlitePool,
}

impl SqliteInvocationLedger {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

const COLUMNS: &str = "account_id, invocation_id, capability, represented_user_id, fingerprint, \
     design_id, result_json, claimed_at, completed_at, target_deleted_at";

/// Versioned, so changing what goes in can never make an old row compare equal
/// to a request it did not see.
const FINGERPRINT_DOMAIN: &[u8] = b"forma.invocation-ledger.v1";

/// blake3 over every part of what the invocation asked for.
///
/// Each part is length-prefixed, so no two different requests can concatenate
/// to the same bytes, and the input is serialized canonically — object keys
/// sorted at every depth — so the same request fingerprints the same however
/// its JSON happened to be ordered. Without that, a retry re-serialized by the
/// sidecar could be refused as a conflict with itself.
fn fingerprint(account: AccountId, request: &InvocationRequest) -> String {
    let mut hasher = blake3::Hasher::new();
    let mut part = |bytes: &[u8]| {
        hasher.update(&(bytes.len() as u64).to_le_bytes());
        hasher.update(bytes);
    };

    part(FINGERPRINT_DOMAIN);
    part(account.to_string().as_bytes());
    part(request.represented_user.to_string().as_bytes());
    part(request.capability.as_bytes());
    part(
        request
            .design
            .map(|design| design.to_string())
            .unwrap_or_default()
            .as_bytes(),
    );
    let mut input = String::new();
    canonical_json(&request.input, &mut input);
    part(input.as_bytes());

    hasher.finalize().to_hex().to_string()
}

fn canonical_json(value: &serde_json::Value, out: &mut String) {
    use serde_json::Value;

    match value {
        Value::Object(map) => {
            let mut entries: Vec<_> = map.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            out.push('{');
            for (index, (key, value)) in entries.into_iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&Value::String(key.clone()).to_string());
                out.push(':');
                canonical_json(value, out);
            }
            out.push('}');
        }
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                canonical_json(item, out);
            }
            out.push(']');
        }
        scalar => out.push_str(&scalar.to_string()),
    }
}

fn record_from(row: &sqlx::sqlite::SqliteRow) -> Result<InvocationRecord> {
    let invocation: String = row.get("invocation_id");
    let design: Option<String> = row.get("design_id");
    let result: Option<String> = row.get("result_json");

    Ok(InvocationRecord {
        account: AccountId(parse_uuid(row.get::<String, _>("account_id"))?),
        invocation: InvocationId::parse(&invocation)
            .map_err(|e| PersistenceError::Storage(format!("stored invocation id: {e}")))?,
        capability: row.get("capability"),
        represented_user: AuthUserId(parse_uuid(row.get::<String, _>("represented_user_id"))?),
        fingerprint: row.get("fingerprint"),
        design: design.map(parse_uuid).transpose()?.map(DesignId),
        result: result
            .map(|json| serde_json::from_str(&json))
            .transpose()
            .map_err(|e| PersistenceError::Storage(format!("stored result is not JSON: {e}")))?,
        claimed_at: row.get("claimed_at"),
        completed_at: row.get("completed_at"),
        target_deleted_at: row.get("target_deleted_at"),
    })
}

/// How an existing row answers a new delivery.
///
/// The fingerprint is compared first and in every state: a different request
/// is refused whether the first one finished, is still running, or lost its
/// design.
fn answer(record: InvocationRecord, fingerprint: &str) -> Result<Claim> {
    if record.fingerprint != fingerprint {
        return Ok(Claim::Conflict);
    }
    if let Some(deleted_at) = record.target_deleted_at {
        return Ok(Claim::TargetDeleted { deleted_at });
    }
    match (record.completed_at, record.result) {
        (Some(_), Some(result)) => Ok(Claim::Replay(result)),
        (None, _) => Ok(Claim::InFlight {
            claimed_at: record.claimed_at,
        }),
        (Some(_), None) => Err(PersistenceError::Storage(format!(
            "invocation {} completed with no result",
            record.invocation
        ))),
    }
}

#[async_trait]
impl InvocationLedger for SqliteInvocationLedger {
    async fn claim(
        &self,
        account: AccountId,
        invocation: &InvocationId,
        request: &InvocationRequest,
    ) -> Result<Claim> {
        let fingerprint = fingerprint(account, request);

        // An existing row answers before anything else is checked: a replay
        // whose design was deleted must hear `TargetDeleted`, not
        // `DESIGN_NOT_FOUND` as though it were a first delivery.
        if let Some(record) = self.get(account, invocation).await? {
            return answer(record, &fingerprint);
        }

        if let Some(design) = request.design {
            owned_design(&self.pool, account, design).await?;
        }

        // Two deliveries racing past the read above meet at the primary key:
        // exactly one insert lands, and the other reads the winner's row.
        let inserted = sqlx::query(
            "INSERT INTO invocation_ledger
             (account_id, invocation_id, capability, represented_user_id, fingerprint,
              design_id, result_json, claimed_at, completed_at, target_deleted_at)
             VALUES (?, ?, ?, ?, ?, ?, NULL, ?, NULL, NULL)
             ON CONFLICT (account_id, invocation_id) DO NOTHING",
        )
        .bind(account.to_string())
        .bind(invocation.as_str())
        .bind(&request.capability)
        .bind(request.represented_user.to_string())
        .bind(&fingerprint)
        .bind(request.design.map(|design| design.to_string()))
        .bind(now())
        .execute(&self.pool)
        .await?;

        if inserted.rows_affected() == 1 {
            return Ok(Claim::Execute);
        }

        let record = self.get(account, invocation).await?.ok_or_else(|| {
            PersistenceError::Storage(format!(
                "invocation {invocation} conflicted on insert and then did not exist"
            ))
        })?;
        answer(record, &fingerprint)
    }

    async fn complete(
        &self,
        account: AccountId,
        invocation: &InvocationId,
        design: Option<DesignId>,
        result: &serde_json::Value,
    ) -> Result<()> {
        let Some(record) = self.get(account, invocation).await? else {
            return Err(PersistenceError::NotFound(format!(
                "invocation {invocation}"
            )));
        };
        if record.completed_at.is_some() || record.target_deleted_at.is_some() {
            return Err(PersistenceError::InvalidTransition(format!(
                "invocation {invocation} is not a live claim"
            )));
        }
        if let (Some(claimed), Some(produced)) = (record.design, design) {
            if claimed != produced {
                return Err(PersistenceError::InvalidTransition(format!(
                    "invocation {invocation} was claimed for design {claimed}, not {produced}"
                )));
            }
        }
        if let Some(design) = design {
            owned_design(&self.pool, account, design).await?;
        }

        let json = serde_json::to_string(result)
            .map_err(|e| PersistenceError::Storage(format!("result is not serializable: {e}")))?;

        // Guarded on the state just read, so two completions cannot both land.
        let updated = sqlx::query(
            "UPDATE invocation_ledger
             SET result_json = ?, completed_at = ?, design_id = COALESCE(design_id, ?)
             WHERE account_id = ? AND invocation_id = ?
               AND completed_at IS NULL AND target_deleted_at IS NULL",
        )
        .bind(json)
        .bind(now())
        .bind(design.map(|design| design.to_string()))
        .bind(account.to_string())
        .bind(invocation.as_str())
        .execute(&self.pool)
        .await?;

        if updated.rows_affected() == 0 {
            return Err(PersistenceError::InvalidTransition(format!(
                "invocation {invocation} stopped being a live claim while it completed"
            )));
        }
        Ok(())
    }

    async fn release(&self, account: AccountId, invocation: &InvocationId) -> Result<()> {
        let released = sqlx::query(
            "DELETE FROM invocation_ledger
             WHERE account_id = ? AND invocation_id = ? AND completed_at IS NULL",
        )
        .bind(account.to_string())
        .bind(invocation.as_str())
        .execute(&self.pool)
        .await?;

        if released.rows_affected() == 1 {
            return Ok(());
        }
        match self.get(account, invocation).await? {
            None => Err(PersistenceError::NotFound(format!(
                "invocation {invocation}"
            ))),
            Some(_) => Err(PersistenceError::InvalidTransition(format!(
                "invocation {invocation} completed; its result is never released"
            ))),
        }
    }

    async fn get(
        &self,
        account: AccountId,
        invocation: &InvocationId,
    ) -> Result<Option<InvocationRecord>> {
        let row = sqlx::query(&format!(
            "SELECT {COLUMNS} FROM invocation_ledger WHERE account_id = ? AND invocation_id = ?"
        ))
        .bind(account.to_string())
        .bind(invocation.as_str())
        .fetch_optional(&self.pool)
        .await?;

        row.as_ref().map(record_from).transpose()
    }
}
