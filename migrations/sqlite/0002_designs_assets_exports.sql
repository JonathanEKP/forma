-- The remaining tables of plan section 26.
--
-- design_versions stores a full JSON snapshot per version rather than an event
-- log (section 26): documents are small, undo becomes trivial, and the audit
-- trail is readable. Event sourcing is explicitly out of scope for V1.

CREATE TABLE IF NOT EXISTS brand_kits (
    id          TEXT PRIMARY KEY NOT NULL,
    project_id  TEXT NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    brand_json  TEXT NOT NULL,
    version     INTEGER NOT NULL,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_brand_kits_project ON brand_kits (project_id);

CREATE TABLE IF NOT EXISTS designs (
    id             TEXT PRIMARY KEY NOT NULL,
    project_id     TEXT NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    name           TEXT NOT NULL,
    schema_version INTEGER NOT NULL,
    -- Guarded by the optimistic UPDATE of section 27.
    version        INTEGER NOT NULL,
    document_json  TEXT NOT NULL,
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_designs_project ON designs (project_id, updated_at);

CREATE TABLE IF NOT EXISTS design_versions (
    id            TEXT PRIMARY KEY NOT NULL,
    design_id     TEXT NOT NULL REFERENCES designs (id) ON DELETE CASCADE,
    version       INTEGER NOT NULL,
    document_json TEXT NOT NULL,
    actor_type    TEXT NOT NULL,
    actor_id      TEXT,
    created_at    TEXT NOT NULL,
    UNIQUE (design_id, version)
);

CREATE TABLE IF NOT EXISTS assets (
    id                TEXT PRIMARY KEY NOT NULL,
    project_id        TEXT NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    hash              TEXT NOT NULL,
    kind              TEXT NOT NULL,
    mime_type         TEXT NOT NULL,
    width             INTEGER NOT NULL,
    height            INTEGER NOT NULL,
    byte_size         INTEGER NOT NULL,
    storage_key       TEXT NOT NULL,
    original_filename TEXT,
    origin            TEXT NOT NULL,
    attribution       TEXT,
    created_at        TEXT NOT NULL,
    -- Deleting an asset is a soft delete: historical design versions may still
    -- reference it, and real GC is deferred (plan section 74).
    deleted_at        TEXT
);

-- Deduplication is per project in V1 (plan section 73).
CREATE UNIQUE INDEX IF NOT EXISTS idx_assets_project_hash ON assets (project_id, hash);

CREATE TABLE IF NOT EXISTS asset_requests (
    id                TEXT PRIMARY KEY NOT NULL,
    project_id        TEXT NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    design_id         TEXT REFERENCES designs (id) ON DELETE SET NULL,
    purpose           TEXT NOT NULL,
    description       TEXT NOT NULL,
    suggested_prompt  TEXT,
    requested_width   INTEGER,
    requested_height  INTEGER,
    status            TEXT NOT NULL,
    resolved_asset_id TEXT REFERENCES assets (id) ON DELETE SET NULL,
    created_at        TEXT NOT NULL,
    resolved_at       TEXT
);

CREATE INDEX IF NOT EXISTS idx_asset_requests_project
    ON asset_requests (project_id, status);

CREATE TABLE IF NOT EXISTS exports (
    id             TEXT PRIMARY KEY NOT NULL,
    design_id      TEXT NOT NULL REFERENCES designs (id) ON DELETE CASCADE,
    design_version INTEGER NOT NULL,
    scope          TEXT NOT NULL,
    page_id        TEXT,
    scale          REAL NOT NULL,
    storage_key    TEXT NOT NULL,
    byte_size      INTEGER NOT NULL,
    created_at     TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_exports_design ON exports (design_id, created_at);
