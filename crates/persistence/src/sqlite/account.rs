use async_trait::async_trait;
use design_core::{
    Account, AccountId, AccountInvite, AccountMembership, AccountRole, AuthUserId,
    BOOTSTRAP_ACCOUNT,
};
use sqlx::{Row, SqlitePool};

use crate::error::{PersistenceError, Result};
use crate::traits::AccountRepository;

use super::{now, parse_uuid};

pub struct SqliteAccountRepository {
    pool: SqlitePool,
}

impl SqliteAccountRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn account_from(row: &sqlx::sqlite::SqliteRow) -> Result<Account> {
    Ok(Account {
        id: AccountId(parse_uuid(row.get::<String, _>("id"))?),
        name: row.get("name"),
        bootstrap: row.get::<i64, _>("bootstrap") == 1,
        claimed_at: row.get("claimed_at"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn membership_from(row: &sqlx::sqlite::SqliteRow) -> Result<AccountMembership> {
    let raw: String = row.get("role");
    Ok(AccountMembership {
        account_id: AccountId(parse_uuid(row.get::<String, _>("account_id"))?),
        user_id: AuthUserId(parse_uuid(row.get::<String, _>("user_id"))?),
        email: row.get("email"),
        role: AccountRole::parse(&raw)
            .ok_or_else(|| PersistenceError::CorruptDocument(format!("unknown role {raw:?}")))?,
        created_at: row.get("created_at"),
    })
}

fn invite_from(row: &sqlx::sqlite::SqliteRow) -> Result<AccountInvite> {
    Ok(AccountInvite {
        email: row.get("email"),
        created_at: row.get("created_at"),
        claimed_at: row.get("claimed_at"),
        claimed_by_user_id: row
            .get::<Option<String>, _>("claimed_by_user_id")
            .map(parse_uuid)
            .transpose()?
            .map(AuthUserId),
    })
}

const COLUMNS: &str = "id, name, bootstrap, claimed_at, created_at, updated_at";

/// Addresses are compared case-insensitively and without surrounding space.
///
/// An invite typed with a capital letter is the same invite, and a person whose
/// provider reports a differently-cased address is the same person. Storing the
/// normalized form keeps the primary key doing that work rather than every
/// caller remembering to.
fn normalize(email: &str) -> String {
    email.trim().to_lowercase()
}

#[async_trait]
impl AccountRepository for SqliteAccountRepository {
    async fn bootstrap(&self) -> Result<Account> {
        let row = sqlx::query(&format!(
            "SELECT {COLUMNS} FROM accounts WHERE bootstrap = 1"
        ))
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| {
            // The migration creates it, so its absence means the database was
            // built by something other than the migrations.
            PersistenceError::Storage("no bootstrap account; migrations did not run".into())
        })?;

        account_from(&row)
    }

    async fn get(&self, id: AccountId) -> Result<Account> {
        let row = sqlx::query(&format!("SELECT {COLUMNS} FROM accounts WHERE id = ?"))
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| PersistenceError::Storage(format!("no account {id}")))?;

        account_from(&row)
    }

    async fn create(&self, name: &str) -> Result<Account> {
        let account = Account {
            id: AccountId::new(),
            name: name.to_string(),
            bootstrap: false,
            claimed_at: None,
            created_at: now(),
            updated_at: now(),
        };

        sqlx::query(
            "INSERT INTO accounts (id, name, bootstrap, claimed_at, created_at, updated_at)
             VALUES (?, ?, 0, NULL, ?, ?)",
        )
        .bind(account.id.to_string())
        .bind(&account.name)
        .bind(&account.created_at)
        .bind(&account.updated_at)
        .execute(&self.pool)
        .await?;

        Ok(account)
    }

    async fn membership_of(&self, user: AuthUserId) -> Result<Option<AccountMembership>> {
        let row = sqlx::query(
            "SELECT account_id, user_id, email, role, created_at
             FROM account_memberships WHERE user_id = ?",
        )
        .bind(user.to_string())
        .fetch_optional(&self.pool)
        .await?;

        row.as_ref().map(membership_from).transpose()
    }

    async fn grant(
        &self,
        account: AccountId,
        user: AuthUserId,
        email: &str,
        role: AccountRole,
    ) -> Result<AccountMembership> {
        let membership = AccountMembership {
            account_id: account,
            user_id: user,
            email: normalize(email),
            role,
            created_at: now(),
        };

        sqlx::query(
            "INSERT INTO account_memberships (account_id, user_id, email, role, created_at)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT (account_id, user_id) DO UPDATE SET
                 email = excluded.email,
                 role  = excluded.role",
        )
        .bind(membership.account_id.to_string())
        .bind(membership.user_id.to_string())
        .bind(&membership.email)
        .bind(membership.role.as_str())
        .bind(&membership.created_at)
        .execute(&self.pool)
        .await?;

        Ok(membership)
    }

    async fn claim_bootstrap(
        &self,
        user: AuthUserId,
        email: &str,
    ) -> Result<Option<AccountMembership>> {
        // Guarded like the optimistic UPDATE of section 27: the condition is in
        // the statement, so two simultaneous claims cannot both see it unclaimed
        // and both proceed.
        let claimed = sqlx::query(
            "UPDATE accounts SET claimed_at = ?, updated_at = ?
             WHERE bootstrap = 1 AND claimed_at IS NULL",
        )
        .bind(now())
        .bind(now())
        .execute(&self.pool)
        .await?;

        if claimed.rows_affected() == 0 {
            return Ok(None);
        }

        let account = AccountId(parse_uuid(BOOTSTRAP_ACCOUNT)?);
        self.grant(account, user, email, AccountRole::Owner)
            .await
            .map(Some)
    }

    async fn invite(&self, email: &str) -> Result<AccountInvite> {
        let invite = AccountInvite {
            email: normalize(email),
            created_at: now(),
            claimed_at: None,
            claimed_by_user_id: None,
        };

        sqlx::query(
            "INSERT INTO account_invites (email, created_at, claimed_at, claimed_by_user_id)
             VALUES (?, ?, NULL, NULL)
             ON CONFLICT (email) DO NOTHING",
        )
        .bind(&invite.email)
        .bind(&invite.created_at)
        .execute(&self.pool)
        .await?;

        // Re-read: inviting an address that already has an open invite must not
        // silently reset the one that exists, and must not report a fresh one.
        self.open_invite(&invite.email).await?.ok_or_else(|| {
            PersistenceError::Storage(format!("{} is already claimed", invite.email))
        })
    }

    async fn list_invites(&self) -> Result<Vec<AccountInvite>> {
        let rows = sqlx::query(
            "SELECT email, created_at, claimed_at, claimed_by_user_id
             FROM account_invites ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(invite_from).collect()
    }

    async fn open_invite(&self, email: &str) -> Result<Option<AccountInvite>> {
        let row = sqlx::query(
            "SELECT email, created_at, claimed_at, claimed_by_user_id
             FROM account_invites WHERE email = ? AND claimed_at IS NULL",
        )
        .bind(normalize(email))
        .fetch_optional(&self.pool)
        .await?;

        row.as_ref().map(invite_from).transpose()
    }

    async fn claim_invite(&self, email: &str, user: AuthUserId) -> Result<bool> {
        let claimed = sqlx::query(
            "UPDATE account_invites SET claimed_at = ?, claimed_by_user_id = ?
             WHERE email = ? AND claimed_at IS NULL",
        )
        .bind(now())
        .bind(user.to_string())
        .bind(normalize(email))
        .execute(&self.pool)
        .await?;

        Ok(claimed.rows_affected() == 1)
    }
}
