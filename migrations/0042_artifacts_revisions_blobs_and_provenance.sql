-- Migration: 0042_artifacts_revisions_blobs_and_provenance.sql

CREATE TABLE IF NOT EXISTS artifacts (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'quarantined',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE artifacts ENABLE ROW LEVEL SECURITY;

CREATE POLICY artifacts_workspace_isolation ON artifacts
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());

CREATE TABLE IF NOT EXISTS artifact_revisions (
    id UUID PRIMARY KEY,
    artifact_id UUID NOT NULL REFERENCES artifacts(id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    revision_number INT NOT NULL,
    media_type TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    byte_size BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_artifact_revision UNIQUE (artifact_id, revision_number)
);

ALTER TABLE artifact_revisions ENABLE ROW LEVEL SECURITY;

CREATE POLICY artifact_revisions_workspace_isolation ON artifact_revisions
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());
