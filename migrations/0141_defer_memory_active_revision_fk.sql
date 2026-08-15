-- Migration: 0141_defer_memory_active_revision_fk.sql
--
-- Let a memory and its first revision be written in one transaction.
--
-- # The cycle
--
-- `memories.active_revision_id` references `memory_revisions.id`, and
-- `memory_revisions.memory_id` references `memories.id`. Neither key was
-- deferrable, so for a new memory that already has an active revision there is
-- no order that works:
--
--   * memory first — refused, the revision it points at does not exist yet;
--   * revision first — refused, the memory it belongs to does not exist yet.
--
-- The only way through was to insert the memory with a NULL `active_revision_id`
-- and update it afterwards, which is a workaround for a cycle rather than a
-- design. Nothing in the codebase did that, and creating a memory through the
-- API had consequently never succeeded — `memories` held zero rows in a
-- long-lived development database.
--
-- # Why deferring is the right answer here
--
-- The schema already says these rows are one fact: `tr_active_memory_has_source`
-- is `DEFERRABLE INITIALLY DEFERRED`, which is the same statement about
-- `memory_sources`. A circular reference between two tables that are always
-- written together is the case deferred constraints exist for. Deferring this
-- key makes the order within the transaction irrelevant while keeping the
-- guarantee exactly as strong at commit.
--
-- It is deferred **initially**, not merely deferrable, so a caller does not have
-- to remember `SET CONSTRAINTS`. Forgetting that would reintroduce the failure
-- in a form that looks like a caller error rather than a schema one.
--
-- The reverse key, `memory_revisions.memory_id`, is deliberately left immediate:
-- only one side of a cycle needs to yield, and leaving the other strict keeps
-- an orphaned revision impossible at the moment it is written rather than at
-- commit.

ALTER TABLE memories
    DROP CONSTRAINT IF EXISTS fk_memories_active_revision;

ALTER TABLE memories
    ADD CONSTRAINT fk_memories_active_revision
        FOREIGN KEY (active_revision_id)
        REFERENCES memory_revisions (id)
        DEFERRABLE INITIALLY DEFERRED;
