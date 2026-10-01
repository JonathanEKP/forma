-- The Pontis integration sidecar's credential: Pontis plan section 10(b) D1.
--
-- A table of its own rather than a flag on `api_keys`. A PAT belongs to one
-- account and carries the subject who minted it; this belongs to the
-- deployment and carries nobody, because who a request acts for arrives with
-- each request and is checked against a membership every time. Sharing a table
-- would mean sharing a row shape that is wrong for one of them, and a flag on
-- `api_keys` is the "reuse a PAT" option D1 rejected, under a worse name.
--
-- Only new objects, so nothing cascades and this runs in the ordinary
-- migration transaction.
CREATE TABLE IF NOT EXISTS service_credentials (
    id            TEXT PRIMARY KEY NOT NULL,
    -- Which sidecar this is, for a person reading a list. Never authorizes.
    name          TEXT NOT NULL,
    -- Enough of the token to recognise it and to index a lookup, never enough
    -- to present.
    token_prefix  TEXT NOT NULL,
    token_hash    TEXT NOT NULL,
    created_at    TEXT NOT NULL,
    last_used_at  TEXT,
    revoked_at    TEXT
);

CREATE INDEX IF NOT EXISTS idx_service_credentials_prefix
    ON service_credentials (token_prefix);
