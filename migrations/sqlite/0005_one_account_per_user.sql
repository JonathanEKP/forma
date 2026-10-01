-- Decision D7 of docs/pontis-integration-plan.md section 10(b): the account a
-- user resolves to must be deterministic.
--
-- `membership_of` answers "which account is this user in" with exactly one
-- membership, and 0004's primary key is (account_id, user_id), which lets one
-- subject hold rows in several accounts. With two rows the lookup returned
-- whichever SQLite found first: an arbitrary winner. Pontis ADR-024 binds an
-- ApplicationContextId for the life of a session, so an arbitrary winner there
-- is a permanent wrong answer rather than a flaky one.
--
-- This constrains the data instead of ordering the lookup. A tie-break such as
-- "oldest membership wins" would be deterministic but still Forma choosing the
-- context, where ADR-024 wants it selected by the host; and a real multi-account
-- design needs the caller to name the account, which is a different signature,
-- not a better ORDER BY. Until that question (D4) is answered, one user, one
-- account is what every caller already assumes, so the schema says so.
--
-- THIS FORECLOSES MULTI-ACCOUNT MEMBERSHIP while the index exists. Undoing it is
-- a DROP INDEX plus giving `membership_of` an explicit account to resolve.
--
-- No table is rebuilt, so the cascade hazard 0004 documents does not arise:
-- nothing is dropped, and this runs in the ordinary migration transaction.
--
-- A database that already holds a user with two memberships FAILS this
-- migration with "UNIQUE constraint failed: account_memberships.user_id". The
-- transaction rolls back, no row is lost or rewritten, and the server refuses
-- to start until a person decides which membership is the real one. Picking
-- for them here would be the arbitrary winner again, just moved. To find them:
--
--   SELECT user_id, account_id, email, created_at FROM account_memberships
--   WHERE user_id IN (SELECT user_id FROM account_memberships
--                     GROUP BY user_id HAVING count(*) > 1)
--   ORDER BY user_id, created_at;
--
-- Take a backup (docs/backup.md), delete the membership that should not exist,
-- and start the server again.

CREATE UNIQUE INDEX idx_memberships_one_account_per_user
    ON account_memberships (user_id);

-- Superseded: the unique index serves every lookup the plain one did.
DROP INDEX IF EXISTS idx_memberships_user;
