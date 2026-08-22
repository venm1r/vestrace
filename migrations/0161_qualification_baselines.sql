-- QualificationBaseline carried a durable publication identity, target binding,
-- lifecycle state and publication time in the domain, but had no store. Deriving
-- one from the bundle currently under release approval would make its comparison
-- vacuous: the bundle would always match a baseline made from itself. A baseline
-- is evidence only because an operator published it earlier for later comparison.
--
-- This table intentionally has no workspace_id, RLS enablement, or policy. It
-- describes a release target, not tenant data, so no workspace policy is the
-- stated storage choice rather than an omitted control.
--
-- The JSONB payload is authoritative. The columns beside it are projections for
-- lookup, uniqueness, lifecycle movement, and integrity checks on read.

CREATE TABLE qualification_baselines (
    id UUID PRIMARY KEY,
    profile TEXT NOT NULL,
    target_digest TEXT NOT NULL,
    state TEXT NOT NULL,
    published_at TIMESTAMPTZ NOT NULL,
    invalidation_reason TEXT,
    payload JSONB NOT NULL,
    CONSTRAINT qualification_baselines_profile_known CHECK (
        profile IN ('core', 'memory', 'cognition', 'autonomy', 'federation', 'trusted')
    ),
    CONSTRAINT qualification_baselines_state_known CHECK (
        state IN ('qualified', 'stale', 'invalidated', 'failed')
    ),
    CONSTRAINT qualification_baselines_target_digest_not_blank CHECK (
        btrim(target_digest) <> ''
    ),
    CONSTRAINT qualification_baselines_payload_object CHECK (
        jsonb_typeof(payload) = 'object'
    ),
    -- One baseline per target **and profile**, not per target.
    --
    -- The plan this table was built from asked for uniqueness on the target
    -- alone, and that was wrong. A baseline carries a profile, and the profiles
    -- are a ladder of increasingly demanding gates; a build qualified as
    -- `correct` and later as `trusted` has two true and different facts about
    -- itself. Uniqueness on the target alone would make the profile column
    -- carry no information the target does not already determine, and would
    -- force invalidating the earlier baseline to publish the later one, losing
    -- history for no reason.
    --
    -- The guarantee is unchanged at the granularity that matters: a target and
    -- profile with two baselines has no baseline.
    CONSTRAINT uq_qualification_baselines_target_digest_profile
        UNIQUE (target_digest, profile)
);

CREATE OR REPLACE FUNCTION vestrace_prevent_qualification_baseline_fact_rewrite()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.profile IS DISTINCT FROM OLD.profile THEN
        RAISE EXCEPTION 'qualification baseline profile is immutable' USING ERRCODE = '55000';
    END IF;
    IF NEW.target_digest IS DISTINCT FROM OLD.target_digest THEN
        RAISE EXCEPTION 'qualification baseline target_digest is immutable' USING ERRCODE = '55000';
    END IF;
    IF NEW.published_at IS DISTINCT FROM OLD.published_at THEN
        RAISE EXCEPTION 'qualification baseline published_at is immutable' USING ERRCODE = '55000';
    END IF;
    IF NEW.payload -> 'profile' IS DISTINCT FROM OLD.payload -> 'profile' THEN
        RAISE EXCEPTION 'qualification baseline payload.profile is immutable' USING ERRCODE = '55000';
    END IF;
    IF NEW.payload -> 'target_digest' IS DISTINCT FROM OLD.payload -> 'target_digest' THEN
        RAISE EXCEPTION 'qualification baseline payload.target_digest is immutable' USING ERRCODE = '55000';
    END IF;
    IF NEW.payload -> 'published_at' IS DISTINCT FROM OLD.payload -> 'published_at' THEN
        RAISE EXCEPTION 'qualification baseline payload.published_at is immutable' USING ERRCODE = '55000';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER tr_qualification_baselines_immutable_published_facts
    BEFORE UPDATE ON qualification_baselines
    FOR EACH ROW
    EXECUTE FUNCTION vestrace_prevent_qualification_baseline_fact_rewrite();
