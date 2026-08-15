-- Migration: 0140_force_rls_on_memory_graph.sql
--
-- Second batch: the memory graph.
--
-- These tables hold the tenant's actual content, which makes them the batch
-- that mattered most. Their four adapters took no `RequestContext` at all —
-- `save_memory(&memory)`, `save(&event)`, `save_source(&source)`,
-- `save_relation(&relation)` — so there was no authenticated workspace anywhere
-- in the write path. Every entity carries its own `workspace_id`, so an adapter
-- could have scoped itself from the value it was handed; that is worth naming
-- as the wrong answer. The policy enforces the scope it is given, faithfully,
-- and cannot know the scope was wrong. Scoping from the entity would make every
-- policy pass by construction and check nothing.
--
-- The ports now take the context and the adapters refuse an entity claiming a
-- different workspace, which is the only version of the check that means
-- anything.
--
-- # Two unbounded reads found while converting
--
-- `find_memory_by_id` and `EventRepository::find_by_id` selected on `id` alone,
-- with no workspace predicate. A memory or event id from any tenant returned
-- that tenant's row — and the policy did not stop it either, because both
-- tables enable row level security without forcing it and the runtime role owns
-- them. Both now filter on the workspace as well as the id, so the query and
-- the policy each carry the boundary independently.

ALTER TABLE memories FORCE ROW LEVEL SECURITY;
ALTER TABLE memory_revisions FORCE ROW LEVEL SECURITY;
ALTER TABLE memory_sources FORCE ROW LEVEL SECURITY;
ALTER TABLE knowledge_relations FORCE ROW LEVEL SECURITY;
ALTER TABLE events FORCE ROW LEVEL SECURITY;
