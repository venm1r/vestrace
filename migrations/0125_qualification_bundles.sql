-- Durable global control-plane storage for target-bound qualification evidence.
-- The indexed columns support lookup and integrity checks; payload preserves
-- the complete serialized QualificationBundle without field loss.

CREATE TABLE IF NOT EXISTS qualification_bundles (
    id UUID PRIMARY KEY,
    lifecycle TEXT NOT NULL,
    profile TEXT NOT NULL,
    status TEXT NOT NULL,
    target_digest TEXT NOT NULL,
    payload JSONB NOT NULL,
    started_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT qualification_bundles_lifecycle_known CHECK (
        lifecycle IN ('pre_merge', 'release', 'deployment', 'periodic', 'post_incident')
    ),
    CONSTRAINT qualification_bundles_profile_known CHECK (
        profile IN ('core', 'memory', 'cognition', 'autonomy', 'federation', 'trusted')
    ),
    CONSTRAINT qualification_bundles_status_known CHECK (
        status IN ('passed', 'failed', 'incomplete')
    ),
    CONSTRAINT qualification_bundles_target_digest_not_blank CHECK (btrim(target_digest) <> ''),
    CONSTRAINT qualification_bundles_payload_object CHECK (jsonb_typeof(payload) = 'object'),
    CONSTRAINT qualification_bundles_completion_after_start CHECK (
        completed_at IS NULL OR completed_at >= started_at
    )
);

CREATE INDEX idx_qualification_bundles_target
    ON qualification_bundles(profile, lifecycle, target_digest, started_at DESC);

CREATE INDEX idx_qualification_bundles_created
    ON qualification_bundles(created_at DESC);
