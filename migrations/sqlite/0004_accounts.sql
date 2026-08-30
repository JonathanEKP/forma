-- no-transaction
--
-- Plan section 107 and 113.2: the account boundary.
--
-- This file runs OUTSIDE a transaction, which sqlx allows through the directive
-- on the first line, and it is the only migration here that does. The reason is
-- measured rather than assumed.
--
-- `projects` is populated on the deployed machine and SQLite has no ALTER
-- COLUMN, so making `account_id` NOT NULL is the documented create-copy-drop-
-- rename rebuild. With foreign keys enabled -- and `connect` enables them --
-- `DROP TABLE projects` performs an implicit DELETE FROM, which fires every
-- ON DELETE CASCADE pointing at it. Run naively inside a transaction that
-- deletes **every design, asset, template and asset request in the database**,
-- and `PRAGMA foreign_key_check` afterwards reports no violations, so nothing
-- looks wrong. `PRAGMA defer_foreign_keys` does not prevent it: the cascade is
-- an action, not a constraint check. Both were measured against a seeded
-- database before this file was written; `accounts.rs` keeps that measurement
-- as a test.
--
-- `PRAGMA foreign_keys` is a no-op inside a transaction, so switching it off is
-- what forces this file out of one. The cost is that a failure part-way leaves
-- the schema half-migrated with nothing to roll it back, which is why the
-- runbook takes a backup first.

PRAGMA foreign_keys = OFF;

-- The account that existed before anyone signed in.
--
-- One row, created here, adopting whatever projects already exist. On a fresh
-- database it adopts none and is simply the account everything belongs to in
-- local mode (plan 117.8). On the deployed database it is the legacy account of
-- plan 113.1, claimable exactly once.
--
-- Its id is fixed rather than random so the migration is deterministic, local
-- mode can resolve it without a query by flag, and a person reading the file
-- can recognise it. It is not a secret and there is exactly one.
CREATE TABLE IF NOT EXISTS accounts (
    id          TEXT PRIMARY KEY NOT NULL,
    name        TEXT NOT NULL,
    -- 1 for the pre-identity account, 0 for every account created afterwards.
    bootstrap   INTEGER NOT NULL DEFAULT 0,
    -- NULL until a signed-in user claims it with the bootstrap credential.
    claimed_at  TEXT,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

-- At most one bootstrap account, enforced by the schema rather than by a caller
-- remembering. A partial unique index is how SQLite says "at most one row where
-- this is true".
CREATE UNIQUE INDEX IF NOT EXISTS idx_accounts_bootstrap
    ON accounts (bootstrap) WHERE bootstrap = 1;

CREATE TABLE IF NOT EXISTS account_memberships (
    account_id  TEXT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    -- Authoritative: the identity provider's subject.
    user_id     TEXT NOT NULL,
    -- Descriptive only, never used to authorize. It exists so a person reading
    -- this table after the identity provider is gone can tell whose row is
    -- whose, and so `forma-server grant-owner` can rebind an account to a fresh
    -- subject from a new project (plan 113.3).
    email       TEXT NOT NULL,
    role        TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    PRIMARY KEY (account_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_memberships_user ON account_memberships (user_id);

-- Signing in is not the gate; owning an account is (plan 107.3).
CREATE TABLE IF NOT EXISTS account_invites (
    email               TEXT PRIMARY KEY NOT NULL,
    created_at          TEXT NOT NULL,
    claimed_at          TEXT,
    claimed_by_user_id  TEXT
);

-- Personal access tokens (plan 112.1). Present from this migration so the
-- hosted slice adds no schema change; nothing writes to it in slice one.
CREATE TABLE IF NOT EXISTS api_keys (
    id                  TEXT PRIMARY KEY NOT NULL,
    account_id          TEXT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    created_by_user_id  TEXT NOT NULL,
    name                TEXT NOT NULL,
    -- Enough of the token to recognise it in a list, never enough to present.
    token_prefix        TEXT NOT NULL,
    token_hash          TEXT NOT NULL,
    created_at          TEXT NOT NULL,
    last_used_at        TEXT,
    revoked_at          TEXT
);

CREATE INDEX IF NOT EXISTS idx_api_keys_account ON api_keys (account_id);

INSERT OR IGNORE INTO accounts (id, name, bootstrap, claimed_at, created_at, updated_at)
VALUES (
    '00000000-0000-4000-8000-000000000001',
    'Workspace',
    1,
    NULL,
    strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
    strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
);

-- The rebuild, in the order that never lets NOT NULL meet an unbackfilled row.
ALTER TABLE projects ADD COLUMN account_id TEXT;

UPDATE projects SET account_id = '00000000-0000-4000-8000-000000000001';

CREATE TABLE projects_rebuilt (
    id          TEXT PRIMARY KEY NOT NULL,
    -- NOT NULL with no default, deliberately. A default would let an INSERT
    -- that forgot the column land silently in the bootstrap account, which is
    -- the opposite of what the account boundary is for: a caller that forgets
    -- the scope must fail, not be guessed for.
    account_id  TEXT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    brand_id    TEXT,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

INSERT INTO projects_rebuilt (id, account_id, name, brand_id, created_at, updated_at)
SELECT id, account_id, name, brand_id, created_at, updated_at FROM projects;

DROP TABLE projects;

-- Keeping the name is what makes the existing ON DELETE CASCADE references from
-- designs, assets, templates and asset_requests resolve again.
ALTER TABLE projects_rebuilt RENAME TO projects;

CREATE INDEX IF NOT EXISTS idx_projects_updated_at ON projects (updated_at);
CREATE INDEX IF NOT EXISTS idx_projects_account ON projects (account_id, updated_at);

PRAGMA foreign_keys = ON;
