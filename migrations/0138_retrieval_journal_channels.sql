-- Migration: 0138_retrieval_journal_channels.sql
--
-- Make the retrieval journal record what it is for.
--
-- The requirement is that the journal records "the intent, parameters, channels
-- and resulting pack". It recorded the intent, the query text, a candidate
-- count and the pack. The rest of the parameters — which statuses and kinds
-- were admissible, which temporal perspective was asked for, what limit and
-- budget applied — were absent, and the channels were absent entirely.
--
-- Those omissions are what turn a journal from evidence into a log line. Two
-- retrievals with the same query and intent can legitimately return different
-- results because their admissible statuses differed, or because a channel was
-- down; without those fields the journal cannot distinguish a ranking that was
-- correct from one that was degraded, and cannot be replayed at all.
--
-- `channels` records every configured channel and its outcome, not just the
-- ones that worked. A journal that lists only successes cannot answer "was the
-- vector channel consulted?" — the two possible reasons for its absence, not
-- configured and failed, are the two an investigation needs to tell apart.

ALTER TABLE retrieval_runs
    ADD COLUMN IF NOT EXISTS parameters JSONB NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN IF NOT EXISTS channels   JSONB NOT NULL DEFAULT '[]'::jsonb;

-- An object and an array respectively: a journal reader should not have to
-- guess the shape, and a scalar smuggled into either column would make the
-- entry unparseable without announcing itself.
ALTER TABLE retrieval_runs
    DROP CONSTRAINT IF EXISTS chk_retrieval_runs_parameters_is_object;
ALTER TABLE retrieval_runs
    ADD CONSTRAINT chk_retrieval_runs_parameters_is_object
        CHECK (jsonb_typeof(parameters) = 'object');

ALTER TABLE retrieval_runs
    DROP CONSTRAINT IF EXISTS chk_retrieval_runs_channels_is_array;
ALTER TABLE retrieval_runs
    ADD CONSTRAINT chk_retrieval_runs_channels_is_array
        CHECK (jsonb_typeof(channels) = 'array');

-- Both tables enabled row level security and neither forced it, so the owning
-- role read every workspace's retrieval history — including the query text,
-- which is often the most sensitive thing a workspace holds. Every other table
-- in this schema forces it.
ALTER TABLE retrieval_runs FORCE ROW LEVEL SECURITY;
ALTER TABLE context_packs FORCE ROW LEVEL SECURITY;
