-- Caller-supplied timestamps describe when evidence says something happened;
-- they do not describe when that evidence joined this record. A receipt stamps
-- `recorded_at` before the adapter is called, so a receipt committed after a
-- crash-derived UNKNOWN can carry an earlier time and disappear behind the
-- guess if lifecycle order is asked of that timestamp.
--
-- `ordinal` gives every new append a database-assigned position. Existing rows
-- receive a deterministic best-effort order from `(created_at, id)`. PostgreSQL
-- `NOW()` is a transaction timestamp, so even distinct creation times do not
-- prove insertion order across overlapping transactions; UUID order has no
-- temporal meaning and serves only as the stable fallback. No exact historical
-- insertion sequence can be recovered from the columns 0153/0154 stored.
--
-- # Why ordinal is not added as BIGSERIAL
--
-- Migration 0154 deliberately left its dispatch-ownership CHECK NOT VALID so
-- 0153-era Dispatching rows could truthfully keep NULL owner/deadline evidence.
-- NOT VALID skips the constraint's initial scan; it is not a durable exemption
-- from later table rewrites. Adding BIGSERIAL here would attach the volatile
-- `nextval` default while adding the column, forcing PostgreSQL to rewrite every
-- row and re-check that ownership constraint. Any legacy Dispatching row would
-- then make this migration impossible to apply.
--
-- Add a nullable BIGINT with no default first, backfill it explicitly, and only
-- then create and attach the sequence/default for future inserts. SET NOT NULL
-- scans the completed column but does not rewrite the table. Future migrations
-- must preserve the same distinction while the 0154 legacy exemption exists.
--
-- The explicit backfill is still an UPDATE, and NOT VALID constraints are
-- enforced against every updated tuple. The migration therefore drops and
-- reinstalls the identical ownership CHECK around that UPDATE. The preceding
-- ALTER TABLE has already taken ACCESS EXCLUSIVE; migrations are transactional,
-- so no writer can use the temporary gap and any failure restores the original
-- constraint.
--
-- # Why the unique index is the adoption lock
--
-- Scoped transactions use PostgreSQL's default READ COMMITTED isolation. Two
-- sweepers can therefore both read the same Dispatching row as latest before
-- either inserts. A conditional INSERT cannot serialize them. The partial
-- unique index below is the point at which one adoption wins and the other is
-- classified as a harmless loser before it asks the provider.

ALTER TABLE external_effect_lifecycle_transitions NO FORCE ROW LEVEL SECURITY;

ALTER TABLE external_effect_lifecycle_transitions
    DROP CONSTRAINT external_effect_lifecycle_dispatch_ownership_qualified;

ALTER TABLE external_effect_lifecycle_transitions
    ADD COLUMN ordinal BIGINT;

WITH stored_order AS MATERIALIZED (
    SELECT id, row_number() OVER (ORDER BY created_at ASC, id ASC) AS ordinal
    FROM external_effect_lifecycle_transitions
)
UPDATE external_effect_lifecycle_transitions AS transition
SET ordinal = stored_order.ordinal
FROM stored_order
WHERE transition.id = stored_order.id;

ALTER TABLE external_effect_lifecycle_transitions
    ADD CONSTRAINT external_effect_lifecycle_dispatch_ownership_qualified
    CHECK (
           (status = 'dispatching'
            AND dispatch_owner IS NOT NULL
            AND dispatch_expires_at IS NOT NULL)
        OR (status <> 'dispatching'
            AND dispatch_owner IS NULL
            AND dispatch_expires_at IS NULL)
    ) NOT VALID;

CREATE SEQUENCE external_effect_lifecycle_transitions_ordinal_seq AS BIGINT;

SELECT setval(
    'external_effect_lifecycle_transitions_ordinal_seq',
    COALESCE(MAX(ordinal), 1),
    COUNT(*) > 0
)
FROM external_effect_lifecycle_transitions;

ALTER SEQUENCE external_effect_lifecycle_transitions_ordinal_seq
    OWNED BY external_effect_lifecycle_transitions.ordinal;

ALTER TABLE external_effect_lifecycle_transitions
    ALTER COLUMN ordinal
        SET DEFAULT nextval('external_effect_lifecycle_transitions_ordinal_seq'),
    ALTER COLUMN ordinal SET NOT NULL;

ALTER TABLE external_effect_lifecycle_transitions FORCE ROW LEVEL SECURITY;

ALTER TABLE external_effect_lifecycle_transitions
    DROP CONSTRAINT external_effect_lifecycle_cause_qualified;

ALTER TABLE external_effect_lifecycle_transitions
    ADD CONSTRAINT external_effect_lifecycle_cause_qualified CHECK (
           (status = 'prepared' AND cause = 'intent_recorded')
        OR (status = 'dispatching' AND cause = 'dispatch_started')
        OR (status IN ('acknowledged', 'failed', 'unknown') AND cause = 'receipt_recorded')
        OR (status = 'unknown' AND cause = 'dispatch_lost')
        OR (status = 'reconciling' AND cause = 'outcome_settled')
        OR (status IN ('confirmed', 'failed') AND cause = 'outcome_delivered')
    );

DROP INDEX idx_external_effect_lifecycle_latest;
CREATE INDEX idx_external_effect_lifecycle_latest
    ON external_effect_lifecycle_transitions
        (workspace_id, effect_id, ordinal DESC);

CREATE UNIQUE INDEX uq_external_effect_lifecycle_dispatch_lost
    ON external_effect_lifecycle_transitions (workspace_id, effect_id, cause_ref)
    WHERE cause = 'dispatch_lost';
