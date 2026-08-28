-- Phase 0 migration: the smallest real slice of the schema in plan section 26.
-- The remaining tables (brand_kits, designs, design_versions, assets,
-- asset_requests, exports) land in Phase 4 together with the repositories.

CREATE TABLE IF NOT EXISTS projects (
    id          TEXT PRIMARY KEY NOT NULL,
    name        TEXT NOT NULL,
    brand_id    TEXT,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_projects_updated_at ON projects (updated_at);
