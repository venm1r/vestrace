-- The outbox now has a drain, which is what makes its failure modes real.
--
-- Until a message could be delivered, "delivery fails forever" was not a state
-- the system could reach. It is now: a handler whose provider is down returns
-- Err on every pass, the message stays pending by design, and `claim_pending`
-- takes the oldest messages first — so a full batch of failing messages blocks
-- every message written after them, indefinitely, while the drain reports
-- itself as running.
--
-- These columns make an attempt a recorded fact rather than an event that
-- happened and left nothing behind.

ALTER TABLE outbox
    -- How many times a handler has been asked and refused. Not a count of
    -- deliveries: a successful delivery sets processed_at and stops.
    ADD COLUMN attempts integer NOT NULL DEFAULT 0,
    -- When the message may next be claimed. Backoff is expressed here rather
    -- than in the drain's memory, because the drain restarts and the backlog
    -- does not.
    ADD COLUMN next_attempt_at timestamptz NOT NULL DEFAULT now(),
    ADD COLUMN last_attempt_at timestamptz,
    -- The last handler error, kept verbatim. A dead-lettered message whose
    -- cause was only ever a log line is a message nobody can act on: the log
    -- has rotated by the time anyone looks at the table.
    ADD COLUMN last_error text,
    -- Set when the message has exhausted its attempts. It is never claimed
    -- again and it is never deleted — the row is the evidence that something
    -- the system promised to do was not done.
    ADD COLUMN dead_lettered_at timestamptz;

ALTER TABLE outbox
    -- A processed message cannot also be dead-lettered: those are contradictory
    -- claims about the same delivery, and a reader counting either one would be
    -- counting the other too.
    ADD CONSTRAINT outbox_terminal_state_is_singular
        CHECK (processed_at IS NULL OR dead_lettered_at IS NULL),
    ADD CONSTRAINT outbox_attempts_are_not_negative
        CHECK (attempts >= 0),
    -- A dead letter is reached by exhausting attempts, so one with no attempt
    -- behind it is a row written by something that skipped the path.
    ADD CONSTRAINT outbox_dead_letters_have_been_attempted
        CHECK (dead_lettered_at IS NULL OR attempts > 0);

-- The claim query's exact predicate: pending, not dead-lettered, and due.
CREATE INDEX IF NOT EXISTS idx_outbox_claimable
    ON outbox (workspace_id, next_attempt_at, created_at)
    WHERE processed_at IS NULL AND dead_lettered_at IS NULL;

-- Row-level security was ENABLEd on this table and never FORCEd. The runtime
-- role owns it, and ownership bypasses an unforced policy — so
-- `outbox_workspace_isolation` has been decorative since the day it was
-- written, and a query that forgot its WHERE clause would have read every
-- workspace's messages while the schema said otherwise.
--
-- Forcing it and scoping the adapter are two halves of one change: `save` ran
-- on a bare pool with no `vestrace.workspace_id` set, so forcing alone would
-- have made writing a memory fail. The port now carries a request context into
-- `save` for exactly this reason.
ALTER TABLE outbox FORCE ROW LEVEL SECURITY;
