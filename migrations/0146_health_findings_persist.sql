-- Migration: 0146_health_findings_persist.sql
--
-- Give health findings somewhere to live.
--
-- # Why a finding without storage is barely a finding
--
-- The previous slice wired the invariant registry, so findings are produced by
-- a registry rather than invented by an adapter. They were then rendered and
-- discarded. Everything the domain says about a finding needs it to survive the
-- run that produced it:
--
--   * `occurrence_count` was always zero, because every run built a new finding;
--   * recurrence and flapping detection had no history to assess, so HLT-008 and
--     HLT-009 were verified against the domain and inert in the deployment;
--   * a disposition had nothing to be recorded against, so a finding could not
--     be suppressed or accepted at all — the operator's only options were fix it
--     or keep reading it.
--
-- # The identity of a finding
--
-- `(workspace_id, invariant_id, fingerprint)` is unique. The fingerprint is what
-- the observer computes for "this violation of this invariant", so the same
-- problem seen on two runs is one finding with two occurrences rather than two
-- findings — which is the whole basis of recurrence detection.
--
-- `invariant_version` is deliberately **not** part of that key. A check whose
-- definition changes keeps its finding and records the new version on it; making
-- the version part of the identity would silently split the history of a problem
-- at the moment somebody edited the check that watches it.
--
-- # Payload and indexed columns
--
-- The JSONB payload is authoritative and the columns beside it are indexed
-- projections, following `qualification_bundles` and the recovery tables. The
-- adapter checks them against the payload on read, so a row whose columns have
-- drifted is refused rather than believed.

CREATE TABLE IF NOT EXISTS health_findings (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id),
    invariant_id TEXT NOT NULL,
    invariant_version TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    lifecycle_status TEXT NOT NULL,
    severity TEXT NOT NULL,
    observed_state TEXT NOT NULL,
    occurrence_count BIGINT NOT NULL DEFAULT 0,
    first_seen_at TIMESTAMPTZ NOT NULL,
    last_seen_at TIMESTAMPTZ NOT NULL,
    payload JSONB NOT NULL,
    CONSTRAINT health_findings_payload_object CHECK (jsonb_typeof(payload) = 'object'),
    CONSTRAINT health_findings_invariant_not_blank CHECK (btrim(invariant_id) <> ''),
    CONSTRAINT health_findings_fingerprint_not_blank CHECK (btrim(fingerprint) <> ''),
    CONSTRAINT health_findings_occurrences_not_negative CHECK (occurrence_count >= 0),
    CONSTRAINT health_findings_identity UNIQUE (workspace_id, invariant_id, fingerprint),
    CONSTRAINT health_findings_id_workspace UNIQUE (id, workspace_id)
);

CREATE INDEX IF NOT EXISTS idx_health_findings_open
    ON health_findings (workspace_id, severity, last_seen_at DESC)
    WHERE lifecycle_status IN ('open', 'reopened');

CREATE TABLE IF NOT EXISTS health_occurrences (
    id UUID PRIMARY KEY,
    finding_id UUID NOT NULL,
    workspace_id UUID NOT NULL,
    sequence BIGINT NOT NULL,
    observed_state TEXT NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL,
    payload JSONB NOT NULL,
    CONSTRAINT health_occurrences_payload_object CHECK (jsonb_typeof(payload) = 'object'),
    CONSTRAINT health_occurrences_sequence_positive CHECK (sequence > 0),
    -- The composite reference keeps an occurrence in the same workspace as its
    -- finding; the plain one would let the two drift apart under a policy that
    -- only sees one of them.
    CONSTRAINT health_occurrences_finding_workspace_fk
        FOREIGN KEY (finding_id, workspace_id)
        REFERENCES health_findings (id, workspace_id)
        ON DELETE RESTRICT,
    CONSTRAINT health_occurrences_sequence_unique UNIQUE (finding_id, sequence)
);

CREATE INDEX IF NOT EXISTS idx_health_occurrences_finding
    ON health_occurrences (workspace_id, finding_id, sequence);

-- Forced from the start. Every batch of the row-level-security work has been
-- about tables that were created without this and had to be retrofitted once
-- their adapters were scoped; a new tenant table should never join that list.
ALTER TABLE health_findings ENABLE ROW LEVEL SECURITY;
ALTER TABLE health_findings FORCE ROW LEVEL SECURITY;
CREATE POLICY health_findings_workspace_isolation ON health_findings
USING (workspace_id = vestrace_current_workspace_id())
WITH CHECK (workspace_id = vestrace_current_workspace_id());

ALTER TABLE health_occurrences ENABLE ROW LEVEL SECURITY;
ALTER TABLE health_occurrences FORCE ROW LEVEL SECURITY;
CREATE POLICY health_occurrences_workspace_isolation ON health_occurrences
USING (workspace_id = vestrace_current_workspace_id())
WITH CHECK (workspace_id = vestrace_current_workspace_id());
