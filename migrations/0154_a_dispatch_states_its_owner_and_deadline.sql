-- A dispatch with no receipt was called lost when its transition was old
-- enough. That was a guess about the clock, not evidence about the call: two
-- replicas could disagree about a live call, and a replica starting five
-- minutes later could treat another process's in-flight dispatch as abandoned.
-- Run leases have always stated who owns the work and until when. A dispatch
-- now states the same two facts, using the same `WorkerId` text vocabulary,
-- before the adapter is called.
--
-- This is a deadline rather than a renewable lease. Dispatch is one adapter
-- call, not long-running work whose owner sends heartbeats. A call that outlives
-- the deadline it stated is operational evidence worth preserving, not a claim
-- to extend silently.
--
-- # Why the CHECK is NOT VALID
--
-- Migration 0153 wrote `dispatching` transitions before an owner or deadline
-- existed. Backfilling them would mean inventing the identity of a process and
-- the promise that process supposedly made about a call which may already have
-- touched the world. Leaving the columns NULL is the truthful compatibility
-- state. `NOT VALID` exempts those existing rows while still refusing every new
-- malformed transition; they remain visible through the explicit legacy count
-- and can never satisfy a deadline comparison.

ALTER TABLE external_effect_lifecycle_transitions
    ADD COLUMN dispatch_owner TEXT,
    ADD COLUMN dispatch_expires_at TIMESTAMPTZ;

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

-- Both recovery's `deadline < cutoff` and the operator-visible NULL count use
-- this partial index. Excluding NULL deadlines would make the compatibility
-- exemption measurable only by scanning the whole lifecycle history.
DROP INDEX idx_external_effect_lifecycle_lost_dispatch;
CREATE INDEX idx_external_effect_lifecycle_lost_dispatch
    ON external_effect_lifecycle_transitions (workspace_id, dispatch_expires_at)
    WHERE status = 'dispatching';
