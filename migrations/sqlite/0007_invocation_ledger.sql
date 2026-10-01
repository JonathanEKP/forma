-- The host-side invocation ledger: decision D6 of
-- docs/pontis-integration-plan.md section 10(b).
--
-- Pontis ADR-023 Decision 2 makes Boundary F carry an idempotency key, and the
-- reference host (pontis/examples/polyglot-host-fixture) keeps the ledger in
-- the host: a duplicate delivery is answered from the record of the first, and
-- a different request under the same key is refused. Without this table a
-- sidecar that retries an `apply_operations` which already committed gets
-- VERSION_CONFLICT -- an error, where idempotency owes it the first answer.
--
-- It is a table rather than a map in memory because the failure it exists for
-- is a retry of a request that already committed, and a restart of this
-- process is exactly when that retry arrives.
--
-- Only new objects, so nothing cascades from a DROP and this runs in the
-- ordinary migration transaction.
CREATE TABLE IF NOT EXISTS invocation_ledger (
    -- The owner. Scoped like every other repository: a key is only ever looked
    -- up inside one account, so the same invocation id under another account
    -- is a different row, and nothing here can tell one account that another
    -- used an id. Deleting the account deletes its ledger: nobody can present
    -- that account again, so there is no retry left to answer.
    account_id          TEXT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    -- Assigned by the Pontis runtime and kept across an approval round trip.
    -- Opaque here; its shape is checked by `InvocationId::parse`.
    invocation_id       TEXT NOT NULL,
    -- Recorded in the clear for a person reading the table. Neither decides
    -- anything: the fingerprint does.
    capability          TEXT NOT NULL,
    represented_user_id TEXT NOT NULL,
    -- blake3 over account, represented user, capability, target design and
    -- canonical input. A second delivery whose fingerprint differs is refused
    -- rather than handed the first one's answer (pontis docs/06 § Idempotency).
    fingerprint         TEXT NOT NULL,
    -- The design the invocation targeted or produced, when there is one. Its
    -- deletion is handled by the trigger below, before this action can run;
    -- SET NULL is the backstop that keeps the reference from ever dangling.
    design_id           TEXT REFERENCES designs (id) ON DELETE SET NULL,
    -- NULL while the invocation is executing, and again after its design was
    -- deleted. Never NULL on a row that completed and still has its design.
    result_json         TEXT,
    claimed_at          TEXT NOT NULL,
    completed_at        TEXT,
    target_deleted_at   TEXT,
    PRIMARY KEY (account_id, invocation_id),
    CHECK (result_json IS NULL OR completed_at IS NOT NULL),
    CHECK (target_deleted_at IS NULL OR (result_json IS NULL AND design_id IS NULL))
);

CREATE INDEX IF NOT EXISTS idx_invocation_ledger_design
    ON invocation_ledger (design_id) WHERE design_id IS NOT NULL;

-- What deleting a design does to the invocations that touched it.
--
-- Not CASCADE. Dropping the row would let a late retry execute again: a
-- retried `create_design` whose design the person has since deleted would
-- quietly make it again, which is the duplicate side effect this table exists
-- to prevent. And not plain SET NULL either, because the stored result can
-- carry the deleted design's content, and deleting a piece of work must not
-- leave a copy of it here.
--
-- So the row becomes a tombstone: the fingerprint stays, so a replay is still
-- recognised and still never executes, while the result is erased and the
-- replay is answered with "its target was deleted" instead. A project or
-- account deletion reaches this through the designs' own ON DELETE CASCADE,
-- which fires this trigger like any other delete.
CREATE TRIGGER IF NOT EXISTS invocation_ledger_design_deleted
BEFORE DELETE ON designs
BEGIN
    UPDATE invocation_ledger
    SET result_json = NULL,
        design_id = NULL,
        target_deleted_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    WHERE design_id = OLD.id;
END;
