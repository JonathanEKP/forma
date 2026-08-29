-- Plan section 85: a template is a DesignDocument plus role metadata, which is
-- what makes it a structural brief for the agent rather than a copied document.
--
-- Scoped to a project on purpose. An ImageLayer holds an asset_id and assets
-- belong to a project, so a workspace-wide template would carry references to
-- bytes another project cannot read. Widening this later only adds rows.
CREATE TABLE IF NOT EXISTS templates (
    id            TEXT PRIMARY KEY NOT NULL,
    project_id    TEXT NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    name          TEXT NOT NULL,
    description   TEXT NOT NULL,
    -- The document as it was when saved. Versions of the design it came from
    -- move on; a template does not follow them.
    document_json TEXT NOT NULL,
    roles_json    TEXT NOT NULL,
    created_at    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_templates_project ON templates (project_id, created_at);
