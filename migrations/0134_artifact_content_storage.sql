-- Migration: 0134_artifact_content_storage.sql
--
-- Content storage for artifacts.
--
-- Migration 0042 is named `..._blobs_...` but created only `artifacts` and
-- `artifact_revisions`; no table in the schema could hold bytes. The registry
-- could therefore record that content existed, with its hash and size, while
-- the system had nowhere to put the content itself. This adds that place.
--
-- Why here and not in the event log: a run event is append-only forever, so
-- model output written into an event could never be redacted, purged or held
-- to a retention window. An artifact already carries the `purged` status that
-- makes deletion expressible.
--
-- Content-addressed: identical bytes are stored once per workspace. Dedup is
-- per workspace, never global, because a shared blob table would let one
-- workspace learn that another holds a particular document by observing a
-- hash collision.

CREATE TABLE IF NOT EXISTS artifact_blobs (
    workspace_id  UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    content_hash  TEXT NOT NULL,
    media_type    TEXT NOT NULL,
    bytes         BYTEA NOT NULL,
    byte_size     BIGINT NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    PRIMARY KEY (workspace_id, content_hash),

    -- The recorded size must match the stored bytes, so a reader can trust
    -- `byte_size` without loading the content to check.
    CONSTRAINT chk_artifact_blobs_size_matches
        CHECK (byte_size = octet_length(bytes)),
    -- SHA-256 rendered as lowercase hex. Pinning the shape here stops two
    -- writers disagreeing about the digest format and silently missing dedup.
    CONSTRAINT chk_artifact_blobs_hash_is_sha256_hex
        CHECK (content_hash ~ '^[0-9a-f]{64}$')
);

ALTER TABLE artifact_blobs ENABLE ROW LEVEL SECURITY;
ALTER TABLE artifact_blobs FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS artifact_blobs_workspace_isolation ON artifact_blobs;
CREATE POLICY artifact_blobs_workspace_isolation ON artifact_blobs
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());

-- Deliberately NOT a foreign key from artifact_revisions.content_hash.
--
-- A revision may legitimately pin content held elsewhere — that is the
-- registry behaviour migration 0042 already supports and which the artifacts
-- API exposes. A hard reference would turn every externally-stored revision
-- into an insert failure. Absence of a blob therefore means "content is not
-- held here", not "the registry is inconsistent".
