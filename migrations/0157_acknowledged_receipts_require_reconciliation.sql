-- A provider acknowledgement only proves that it accepted a request. It does
-- not prove that the business effect the request asked for happened, so an
-- `acknowledged` receipt must enter the same reconciliation sweep as `unknown`.
--
-- Migration 0128's `idx_external_effect_receipts_unknown` deliberately covered
-- only `outcome_status = 'unknown'`, matching the first version of recovery.
-- Leaving that index in place after widening the candidate predicate would make
-- acknowledged history pay for a workspace-wide scan in the largest evidence
-- table whenever a worker drains its bounded batch. The replacement predicate
-- is exactly the two unsettled transport states, and its leading workspace key
-- followed by ascending creation/id order matches the workspace-scoped,
-- oldest-first candidate access path.
--
-- Read-back capability is intentionally absent from this index and every other
-- persisted receipt column. `supports_read_back` belongs to the configured
-- adapter descriptor, not the historical evidence: the recovery service routes
-- by the persisted adapter name, refuses a route whose live descriptor cannot
-- be asked, and records the existing failed-attempt backoff. Storing or
-- inferring capability here would create a second source of truth and could
-- silently starve otherwise eligible candidates before that backoff can work.

DROP INDEX IF EXISTS idx_external_effect_receipts_unknown;

CREATE INDEX idx_external_effect_receipts_reconciliation_candidates
    ON external_effect_receipts (workspace_id, created_at ASC, id ASC)
    WHERE outcome_status IN ('unknown', 'acknowledged');
